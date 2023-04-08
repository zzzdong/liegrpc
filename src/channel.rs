use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::Stream;
use futures::stream::Concat;
use http_body::Frame;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{Full, StreamBody};
use hyper::body::Incoming;
use hyper::client::conn::http2::{self, handshake, Builder, SendRequest};
use hyper::{body::Body, Request, Response, Uri};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::status::{self, Code, Status};

pub type BoxBody = UnsyncBoxBody<Bytes, Status>;

#[derive(Clone)]
pub struct Channel {
    inner: Arc<Mutex<Inner>>,
}

impl Channel {
    pub fn new(target: impl Iterator<Item = impl AsRef<str>>) -> Result<Channel, Status> {
        let mut uris = Vec::new();

        for item in target {
            let uri = Uri::try_from(item.as_ref()).map_err(|err| {
                Status::new(Code::InvalidArgument, "invalid address").with_cause(err)
            })?;

            if uri.authority().is_none() || uri.scheme().is_none() {
                return Err(Status::new(Code::InvalidArgument, "invalid address"));
            }
            uris.push(uri);
        }

        if uris.is_empty() {
            return Err(Status::new(Code::InvalidArgument, "invalid addresses"));
        }

        let opts = Opts {
            addrs: uris,
            credentail: Credential::InSecure,
        };



        let inner = Inner {
            opts,
            conns: BTreeMap::new(),
            // load_balancer: Box::new(PickFirstBalancer::new()),
        };

        Ok(Channel {
            inner: Arc::new(Mutex::new(inner)),
        })
    }


    pub async fn call(&mut self, path: &str, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        let mut conn = {
            let mut inner = self.inner.lock().unwrap();
            inner.request_connection().await?.clone()
        };
        conn.request(req).await
    }

}


struct Inner {
    opts: Opts,
    conns: BTreeMap<ChannelId, SubChannel>,
    // load_balancer: Box<dyn LoadBalancer + Send>,
}

impl Inner {
    fn get_opts(&self) -> &Opts {
        &self.opts
    }



    pub async fn connect_endpoint(&mut self, endpoint: Endpoint) -> Result<ChannelId, Status> {
        let mut sub_channel = SubChannel::new(endpoint);
        let state = sub_channel.connect().await;
        if state == ConnectivityState::Ready {
            let id = sub_channel.get_id();
            self.conns.insert(id.clone(), sub_channel);
            Ok(id)
        } else {
            Err(Status::internal("connect endpoint failed"))
        }
    }

    async fn request_connection(&mut self) -> Result<Connection, Status> {
        if let Some(mut entry) = self.conns.first_entry() {
            let conn = entry.get_mut().request_connection().await?;
            return Ok(conn);
        } else {
            return Err(Status::internal("can not request connection"));
        }
    }

    async fn connect_one(&mut self) -> Result<ChannelId, Status> {
        for addr in &self.get_opts().addrs.clone() {
            for ep in Resolver::resolve(addr)? {
                let endpoint = Endpoint { addr: ep, credentail: self.get_opts().credentail.clone() };
                let mut  conn = SubChannel::new(endpoint);
                if ConnectivityState::Ready == conn.connect().await {
                    let id = conn.get_id();
                    self.conns.insert(id.clone(), conn);
                    return Ok(id);
                }
            }
        }

        Err(Status::internal("can not connect"))
    }


    async fn connect_and_wait_one(&mut self) -> Result<(), Status> {
        for addr in &self.get_opts().addrs.clone() {
            for ep in Resolver::resolve(addr)? {
                let endpoint = Endpoint { addr: ep, credentail: self.get_opts().credentail.clone() };
                let conn = SubChannel::new(endpoint);
                self.conns.insert(conn.get_id().clone(), conn);
            }
        }

        Ok(())
    }
}


#[derive(Clone)]
struct Opts {
    addrs: Vec<Uri>,
    credentail: Credential,
}

#[derive(Clone)]
struct Connection {
    id: ChannelId,
    transport: SendRequest<BoxBody>,
    state: Arc<RwLock<ConnectivityState>>,
}

impl Connection {
    pub async fn request(&mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        match self.transport.send_request(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                let mut state = self.state.write().await;
                *state = ConnectivityState::TransientFailure;
                Err(err.into())
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Endpoint {
    addr: SocketAddr,
    credentail: Credential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityState {
    Connecting,
    Ready,
    TransientFailure,
    Idle,
    Shutdown,
}

#[derive(Debug, Clone)]
pub(crate) struct SubChannel {
    id: ChannelId,
    endpoint: Endpoint,
    state: Arc<RwLock<ConnectivityState>>,
    transport: Option<SendRequest<BoxBody>>,
}

impl SubChannel {
    pub fn new(endpoint: Endpoint) -> Self {
        SubChannel {
            id: ChannelId::next_id(),
            endpoint,
            state: Arc::new(RwLock::new(ConnectivityState::Idle)),
            transport: None,
        }
    }

    pub fn get_id(&self) -> ChannelId {
        self.id
    }

    pub async fn get_state(&self) -> ConnectivityState {
        *self.state.read().await
    }

    async fn set_state(&mut self, state: ConnectivityState) {
        *self.state.write().await = state;
    }

    pub async fn connect(&mut self) -> ConnectivityState {
        if let Some(ref transport) =  self.transport {
            return self.get_state().await;
        }

        self.set_state(ConnectivityState::Connecting);

        match self.inner_connect().await {
            Ok(_) => {
                self.set_state(ConnectivityState::Ready);
            }
            Err(err) => {
                self.set_state(ConnectivityState::TransientFailure);
            }
        };

        self.get_state().await
    }

    pub async fn request_connection(&mut self) -> Result<Connection, Status> {
        let mut retry = 0;

        loop {
            match self.get_state().await {
                ConnectivityState::Connecting | ConnectivityState::Ready => {
                    // let it go
                }
                ConnectivityState::Shutdown => {
                    unreachable!();
                }
                _ => {
                    let state = self.connect().await;
                    if state == ConnectivityState::Ready {
                        let conn = self.transport.clone().expect("transport must ok when ready");
                        let conn = Connection {
                            id: self.id.clone(),
                            state: self.state.clone(),
                            transport: conn.clone(),
                        };
                        return Ok(conn);
                    }
                }
            };

            retry += 1;
            if retry >= 3 {
                return Err(Status::internal("request connection failed"));
            }
        }
    }

    async fn inner_connect(&mut self) -> Result<(), Status> {
        self.transport = match self.endpoint.credentail {
            Credential::InSecure => self.tcp_connect().await,
            Credential::SecureTls => self.tls_connect().await,
        }
        .map(|sender| Some(sender))?;

        Ok(())
    }

    async fn tcp_connect(&mut self) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect(self.endpoint.addr).await?;

        let (sender, conn) = http2::handshake(TokioExec, stream).await?;

        tokio::task::spawn(async move { conn.await });

        Ok(sender)
    }

    async fn tls_connect(&mut self) -> Result<SendRequest<BoxBody>, Status> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(usize);

impl ChannelId {
    pub fn next_id() -> ChannelId {
        static AUTO_ID: AtomicUsize = AtomicUsize::new(0);

        let id = AUTO_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ChannelId(id)
    }
}

#[derive(Debug, Clone)]
pub enum Credential {
    InSecure,
    SecureTls, // TODO: support tls
}

// #[async_trait::async_trait]
// trait LoadBalancer {
//     async fn pick(&mut self, channel: &mut Inner) -> Result<ChannelId, Status>;
// }

// struct PickFirstBalancer {
//     prefer: Option<ChannelId>,
// }

// impl PickFirstBalancer {
//     pub fn new() -> Self {
//         PickFirstBalancer { prefer: None }
//     }

//     async fn init(&mut self, channel: &mut Inner) -> Result<ChannelId, Status> {
//         Err(Status::internal("connect to address failed"))
//     }
// }

// #[async_trait::async_trait]
// impl LoadBalancer for PickFirstBalancer {
//     async fn pick(&mut self, channel: &mut Inner) -> Result<ChannelId, Status> {
//         match self.prefer {
//             Some(id) => {
//                 return Ok(id);
//             }
//             None => {
//                 // try connect on one by one
//                 for addr in &channel.get_opts().addrs {
//                     for ep in Resolver::resolve(addr)? {
//                         let endpoint = Endpoint { addr: ep, credentail: channel.get_opts().credentail.clone() };
//                         match channel.connect_endpoint(endpoint).await {
//                             Ok(id) => {
//                                 return Ok(id);
//                             }
//                             Err(err) => {
//                                 // let it go
//                             }
//                         }
//                     }
//                 }

//                 return Err(Status::internal("con not pick connection"));
//             }
//         }
//     }
// }

struct Resolver;

impl Resolver {
    fn resolve(addr: &Uri) -> Result<Vec<SocketAddr>, Status> {
        match addr.host() {
            Some(host) => {
                let mut addrs = ToSocketAddrs::to_socket_addrs(host)?;

                let port = match addr.port_u16() {
                    Some(port) => port,
                    None => match addr.scheme_str() {
                        Some("http") => 80,
                        Some("https") => 443,
                        None => {
                            return Err(Status::internal("unknown port in address"));
                        }
                    },
                };

                Ok(addrs
                    .map(|a| {
                        a.set_port(port);
                        a
                    })
                    .collect::<Vec<SocketAddr>>())
            }
            None => {
                return Err(Status::internal("invalid host in address"));
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TokioExec;

impl<F> hyper::rt::Executor<F> for TokioExec
where
    F: std::future::Future + Send + 'static,
    F::Output: Send,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn(fut);
    }
}

// struct ReqStream {
//     inner: Box<dyn Stream<Item = Result<Frame<Bytes>, Status>> + Send + 'static + Unpin>,
// }

// impl Stream for ReqStream {
//     type Item = Result<Frame<Bytes>, Status>;

//     fn poll_next(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Self::Item>> {
//         Pin::new(&mut self.get_mut().inner).poll_next(cx)
//     }

//     fn size_hint(&self) -> (usize, Option<usize>) {
//         self.inner.size_hint()
//     }
// }

// impl Body for ReqStream {
//     type Data = Bytes;
//     type Error = Status;

//     fn poll_frame(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
//         Pin::new(&mut self.inner).poll_next(cx)
//     }
// }

// enum ReqBody {
//     Unary(Full<Bytes>),
//     Stream(StreamBody<Box<dyn Stream<Item = Result<Frame<Bytes>, Status>>>>),
// }

// impl Body for ReqBody {
//     type Data = Bytes;
//     type Error = Status;

//     fn poll_frame(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
//         match self.get_mut() {
//             ReqBody::Unary(frame) => Pin::new(frame).poll_frame(cx).map_err(Into::into),

//             ReqBody::Stream(stream) => Pin::new(stream).poll_frame(cx),
//         }
//     }
// }

// enum ReqBody {
//     Unary(Full<Bytes>),
//     Stream(ReqStream),
// }

// impl Body for ReqBody {
//     type Data = Bytes;
//     type Error = Status;

//     fn poll_frame(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
//         match self.get_mut() {
//             ReqBody::Unary(frame) => Pin::new(frame).poll_frame(cx).map_err(Into::into),
//             ReqBody::Stream(stream) => Pin::new(stream).poll_frame(cx),
//         }
//     }
// }
