use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use bytes::Bytes;
use futures::Stream;
use http_body::Frame;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{Full, StreamBody};
use hyper::body::Incoming;
use hyper::client::conn::http2::{self, handshake, Builder, SendRequest};
use hyper::{body::Body, Request, Response, Uri};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::status::{self, Code, Status};

static AUTO_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct Channel {
    tx: mpsc::UnboundedSender<(
        Request<BoxBody>,
        oneshot::Sender<Result<Response<Incoming>, Status>>,
    )>,
}

impl Channel {
    pub fn new(target: impl Iterator<Item = impl AsRef<str>>) -> Result<Channel, Status> {
        let mut uris = Vec::new();

        for item in target {
            let uri = Uri::try_from(item.as_ref())
                .map_err(|err| Status::new(Code::InvalidArgument, "invalid uri").with_cause(err))?;

            if uri.authority().is_none() || uri.scheme().is_none() {
                return Err(Status::new(Code::InvalidArgument, "invalid uri"));
            }
            uris.push(uri);
        }

        if uris.is_empty() {
            return Err(Status::new(Code::InvalidArgument, "invalid target"));
        }

        let manager = ConnectionManager::new(uris, Credential::InSecure);

        let (tx, rx) = mpsc::unbounded_channel();

        let manager = ChannelManager { rx, manager };

        tokio::task::spawn(async move { manager.dispatch().await });

        Ok(Channel { tx })
    }

    pub async fn call(
        &mut self,
        method: Uri,
        mut request: Request<BoxBody>,
    ) -> Result<Response<Incoming>, Status> {
        *request.uri_mut() = method;

        let (tx, rx) = oneshot::channel();

        let resp = self.tx.send((request, tx));

        rx.await.unwrap()
    }
}

struct ChannelManager {
    rx: mpsc::UnboundedReceiver<(
        Request<BoxBody>,
        oneshot::Sender<Result<Response<Incoming>, Status>>,
    )>,
    manager: ConnectionManager,
}

impl ChannelManager {
    pub async fn dispatch(&mut self) {
        while let Some((req, tx)) = self.rx.recv().await {
            let conn = self.manager.request_connection().await;

            match conn {
                Ok(conn) => {
                    let SubChannel {
                        state,
                        state_tx,
                        sender,
                        ..
                    } = conn.clone();
                    tokio::task::spawn(async move {
                        let ret = sender.unwrap().send_request(req).await.map_err(Into::into);
                        tx.send(ret);
                    });
                }
                Err(err) => {
                    tx.send(Err(err));
                }
            }
        }
    }
}

pub type BoxBody = UnsyncBoxBody<Bytes, Status>;

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
    addr: SocketAddr,
    credentail: Credential,
    state: Arc<RwLock<ConnectivityState>>,
    state_tx: mpsc::Sender<ConnectivityState>,
    sender: Option<SendRequest<BoxBody>>,
}

impl SubChannel {
    pub fn new(
        addr: SocketAddr,
        credentail: Credential,
        state_tx: mpsc::Sender<ConnectivityState>,
    ) -> Self {
        SubChannel {
            id: ChannelId::next_id(),
            addr,
            credentail,
            state_tx,
            state: Arc::new(RwLock::new(ConnectivityState::Idle)),
            sender: None,
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
        self.state_tx.send(state).await;
    }

    pub async fn connect(&mut self) -> ConnectivityState {
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

    pub async fn request_connection(&mut self) {
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
                    self.connect().await;
                }
            };

            retry += 1;
            if retry >= 3 {
                return;
            }
        }
    }

    async fn inner_connect(&mut self) -> Result<(), Status> {
        self.sender = match self.credentail {
            Credential::InSecure => self.tcp_connect().await,
            Credential::SecureTls => self.tls_connect().await,
        }
        .map(|sender| Some(sender))?;

        Ok(())
    }

    async fn tcp_connect(&mut self) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect(self.addr).await?;

        let (sender, conn) = http2::handshake(TokioExec, stream).await?;

        tokio::task::spawn(async move { conn.await });

        Ok(sender)
    }

    async fn tls_connect(&mut self) -> Result<SendRequest<BoxBody>, Status> {
        unimplemented!()
    }

    async fn request(&mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        match self.sender {
            Some(ref sender) => sender.clone().send_request(req).await.map_err(Into::into),
            None => {
                return Err(Status::internal("subchannel is not inited"));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelId(usize);

impl ChannelId {
    pub fn next_id() -> ChannelId {
        let id = AUTO_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ChannelId(id)
    }
}

#[derive(Debug, Clone)]
pub enum Credential {
    InSecure,
    SecureTls, // TODO: support tls
}

#[derive(Debug)]
struct ConnectionManager {
    credentail: Credential,
    state_rx: mpsc::Receiver<ConnectivityState>,
    state_tx: mpsc::Sender<ConnectivityState>,
    conns: BTreeMap<ChannelId, SubChannel>,
    addrs: Vec<Uri>,
}

impl ConnectionManager {
    pub fn new(addrs: Vec<Uri>, credentail: Credential) -> Self {
        let (state_tx, state_rx) = mpsc::channel(1);

        ConnectionManager {
            state_rx,
            state_tx,
            addrs,
            credentail,
            conns: BTreeMap::new(),
        }
    }

    pub async fn connect_one(&self, addr: SocketAddr) -> Result<ChannelId, Status> {
        let mut sub_channel = SubChannel::new(addr, self.credentail.clone(), self.state_tx.clone());
        sub_channel.connect().await;

        Ok(sub_channel.id)
    }

    pub fn get_addrs(&self) -> impl Iterator<Item = &Uri> {
        self.addrs.iter()
    }

    pub async fn request_connection(&self) -> Result<&SubChannel, Status> {
        unimplemented!()
    }

    pub(crate) async fn wait_for_state(
        &mut self,
        desire: ConnectivityState,
    ) -> Option<ConnectivityState> {
        loop {
            match self.state_rx.recv().await {
                Some(s) if s == desire => {
                    return Some(s);
                }
                Some(s) => {
                    // let it go
                    continue;
                }
                None => {
                    return None;
                }
            }
        }
    }

    pub(crate) async fn get_subchannel_state(&self, id: ChannelId) -> Option<ConnectivityState> {
        match self.conns.get(&id) {
            Some(ch) => Some(ch.get_state().await),
            None => None,
        }
    }
}

#[async_trait::async_trait]
pub trait LoadBalancer {
    async fn pick(&mut self, manager: &mut ConnectionManager) -> Result<ChannelId, Status>;
}

pub struct PickFirstBalancer {
    prefer: Option<ChannelId>,
}

impl PickFirstBalancer {
    pub async fn new() -> Self {
        PickFirstBalancer { prefer: None }
    }

    async fn init(&mut self, manager: &mut ConnectionManager) -> Result<ChannelId, Status> {
        for addr in manager.get_addrs() {
            for endpoint in Resolver::resolve(addr)? {
                match manager.connect_one(endpoint).await {
                    Ok(id) => return Ok(id),
                    Err(err) => {
                        // when err, try next
                        // TODO: add log
                    }
                }
            }
        }

        Err(Status::internal("connect to address failed"))
    }
}

#[async_trait::async_trait]
impl LoadBalancer for PickFirstBalancer {
    async fn pick(&mut self, manager: &mut ConnectionManager) -> Result<ChannelId, Status> {
        match self.prefer {
            Some(id) => {
                return Ok(id);
            }
            None => {
                // try connect on one by one
                self.init(manager).await
            }
        }
    }
}

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
