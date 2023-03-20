use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::client::conn::http2::{self, handshake, Builder, SendRequest};
use hyper::{body::Body, Request, Response, Uri};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};

use crate::status::{Code, Status};

static AUTO_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelID(usize);

impl ChannelID {
    pub fn next_id() -> ChannelID {
        let id = AUTO_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ChannelID(id)
    }
}

// #[derive(Debug, Clone)]
// pub struct Channel<B> {
//     conns: Vec<SendRequest<B>>,
//     uris: Vec<Uri>,
// }

// impl<B> Channel<B>
// where
//     B: Body + 'static,
// {
//     pub fn new(target: impl Iterator<Item = impl AsRef<str>>) -> Result<Self, Status> {
//         let mut uris = Vec::new();

//         for item in target {
//             let uri = Uri::try_from(item.as_ref())
//                 .map_err(|err| Status::new(Code::InvalidArgument, "invalid uri").with_cause(err))?;

//             if uri.authority().is_none() || uri.scheme().is_none() {
//                 return Err(Status::new(Code::InvalidArgument, "invalid uri"));
//             }
//             uris.push(uri);
//         }

//         if uris.is_empty() {
//             return Err(Status::new(Code::InvalidArgument, "invalid target"));
//         }

//         let https = HttpsConnectorBuilder::new()
//             .with_native_roots()
//             .https_or_http()
//             .enable_http2()
//             .build();

//         let client = hyper::Client::builder().http2_only(true).build(https);

//         Ok(Channel { client, uris })
//     }

//     pub async fn call(
//         &mut self,
//         method: &str,
//         mut request: Request<B>,
//     ) -> Result<Response<Incoming>, Status> {
//         // load balance select
//         let i = rand::random::<usize>() % self.uris.len();

//         let uri = self
//             .uris
//             .get(i)
//             .ok_or_else(|| Status::internal("no avaliable target"))?;

//         let uri = Uri::builder()
//             .scheme(uri.scheme().expect("scheme must not empty").clone())
//             .authority(uri.authority().expect("authority must not empty").clone())
//             .path_and_query(method)
//             .build()
//             .map_err(|err| Status::new(Code::InvalidArgument, "uri error").with_cause(err))?;

//         *request.uri_mut() = uri;

//         let resp = self.client.request(request).await?;

//         Ok(resp)
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityState {
    Connecting,
    Ready,
    TransientFailure,
    Idle,
    Shutdown,
}

#[derive(Debug, Clone)]
pub(crate) struct SubChannel<B> {
    id: ChannelID,
    addr: SocketAddr,
    credentail: Credential,
    state: Arc<RwLock<ConnectivityState>>,
    state_tx: mpsc::Sender<ConnectivityState>,
    sender: Option<SendRequest<B>>,
}

impl<B> SubChannel<B> {
    pub fn new(
        addr: SocketAddr,
        credentail: Credential,
        state_tx: mpsc::Sender<ConnectivityState>,
    ) -> Self {
        SubChannel {
            id: ChannelID::next_id(),
            addr,
            credentail,
            state_tx,
            state: Arc::new(RwLock::new(ConnectivityState::Idle)),
            sender: None,
        }
    }

    pub fn get_id(&self) -> ChannelID {
        self.id
    }

    pub async fn get_state(&self) -> ConnectivityState {
        *self.state.read().await
    }

    async fn set_state(&mut self, state: ConnectivityState) {
        *self.state.write().await = state;
        self.state_tx.send(state).await;
    }
}


impl<B> SubChannel<B>
where
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{


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

    async fn tcp_connect(&mut self) -> Result<SendRequest<B>, Status> {
        let stream = TcpStream::connect(self.addr).await?;

        let (sender, conn) = http2::handshake(TokioExec, stream).await?;

        tokio::task::spawn(async move { conn.await });

        Ok(sender)
    }

    async fn tls_connect(&mut self) -> Result<SendRequest<B>, Status> {
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub enum Credential {
    InSecure,
    SecureTls, // TODO: support tls
}

struct ConnectionManager<B> {
    state_rx: mpsc::Receiver<ConnectivityState>,
    state_tx: mpsc::Sender<ConnectivityState>,
    conns: BTreeMap<ChannelID, SubChannel<B>>,
}

impl<B> ConnectionManager<B>
    where

 {
    pub fn new() -> Self {
        let (state_tx, state_rx) = mpsc::channel(1);

        ConnectionManager {
            state_rx,
            state_tx,
            conns: BTreeMap::new(),
        }
    }

    pub async fn connect_one(&mut self, addr: SocketAddr) -> Option<ChannelID> {
        unimplemented!()
    }

    pub(crate) async fn wait_for_state(&mut self, state: ConnectivityState) -> Option<ConnectivityState> {
        loop {
            match self.state_rx.recv().await {
                Some(s) if s == state => {
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

    pub(crate) async fn get_subchannel_state(&self, id: ChannelID) -> Option<ConnectivityState> {
        match self.conns.get(&id) {
            Some(ch) => {
                Some(ch.get_state().await)
            }
            None => None,
        }
    }

}


#[async_trait::async_trait]
pub trait LoadBalancer {
    async fn pick<B>(&mut self, manager: &mut ConnectionManager<B>) -> Result<ChannelID, Status>;
}

pub struct PickFirstBalancer {
    prefer: Option<ChannelID>,
}

impl PickFirstBalancer {
    pub async fn new() -> Self {
        PickFirstBalancer { prefer: None }
    }
}

#[async_trait::async_trait]
impl LoadBalancer for PickFirstBalancer {
    async fn pick<B>(&mut self, manager: &mut ConnectionManager<B>) -> Result<ChannelID, Status> {
        match self.prefer {
            Some(id) => {
                return Ok(id);
            }
            None => {
                // try connect on one by one
                unimplemented!()
            }
        }
    }
}

struct Resolver;

impl Resolver {
    fn resolve(addrs: impl Iterator<Item = impl AsRef<str>>) -> Result<Vec<SocketAddr>, Status> {
        let mut endpoints = Vec::new();

        for addr in addrs.into_iter() {
            for ep in ToSocketAddrs::to_socket_addrs(addr.as_ref())? {
                endpoints.push(ep);
            }
        }

        Ok(endpoints)
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
