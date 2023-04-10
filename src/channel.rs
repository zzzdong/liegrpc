use std::cmp::min;
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::client::conn::http2::{handshake, SendRequest};
use hyper::{Request, Response, Uri};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio::time::Instant;

use crate::status::{Code, Status};

pub type BoxBody = UnsyncBoxBody<Bytes, Status>;

pub(crate) fn boxed<B>(body: B) -> BoxBody
where
    B: http_body::Body<Data = bytes::Bytes> + Send + 'static,
    B::Error: Into<crate::Status>,
{
    body.map_err(|err| err.into()).boxed_unsync()
}

#[derive(Clone)]
pub struct Channel {
    inner: Arc<Mutex<Inner>>,
    load_balancer: Arc<Mutex<Box<dyn LoadBalancer + Send + Sync>>>,
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
            uris,
            credentail: Credential::InSecure,
        };

        let inner = Inner::new(opts);

        Ok(Channel {
            inner: Arc::new(Mutex::new(inner)),
            load_balancer: Arc::new(Mutex::new(Box::new(PickFirstBalancer::new()))),
        })
    }

    pub async fn call(&mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        let mut inner = self.inner.lock().await;

        let mut load_balancer = self.load_balancer.lock().await;

        let id = load_balancer.pick(&mut inner).await?;

        let conn = inner.make_connection(id).await?;

        conn.request(req).await
    }
}

struct Inner {
    opts: Opts,
    conns: BTreeMap<ChannelId, SubChannel>,
}

impl Inner {
    fn new(opts: Opts) -> Self {
        Inner {
            opts,
            conns: BTreeMap::new(),
        }
    }

    async fn make_connection(&mut self, id: ChannelId) -> Result<Connection, Status> {
        match self.conns.get_mut(&id) {
            Some(channel) => channel.make_connection().await,
            None => Err(Status::internal("can not request connection")),
        }
    }

    fn get_opts(&self) -> &Opts {
        &self.opts
    }

    pub async fn connect_endpoint(&mut self, endpoint: Endpoint) -> Result<ChannelId, Status> {
        let mut sub_channel = SubChannel::new(endpoint);
        let state = sub_channel.connect().await;
        if state == ConnectivityState::Ready {
            let id = sub_channel.get_id();
            self.conns.insert(id, sub_channel);
            Ok(id)
        } else {
            Err(Status::internal("connect endpoint failed"))
        }
    }

    pub fn resolve_addrs(&self) -> Result<Vec<Endpoint>, Status> {
        let mut endpoints = Vec::new();

        for uri in &self.get_opts().uris.clone() {
            for addr in Resolver::resolve(uri)? {
                let endpoint = Endpoint::new(uri.clone(), addr, self.get_opts().credentail.clone());
                endpoints.push(endpoint);
            }
        }

        Ok(endpoints)
    }
}

#[derive(Clone)]
struct Opts {
    uris: Vec<Uri>,
    credentail: Credential,
}

#[derive(Clone)]
pub struct Connection {
    id: ChannelId,
    uri: Uri,
    transport: SendRequest<BoxBody>,
    state: Arc<RwLock<ConnectivityState>>,
}

impl Connection {
    pub fn new(
        id: ChannelId,
        uri: Uri,
        state: Arc<RwLock<ConnectivityState>>,
        transport: SendRequest<BoxBody>,
    ) -> Self {
        Connection {
            id,
            uri,
            state,
            transport,
        }
    }

    pub async fn request(mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        let mut req = req;
        let mut parts = self.uri.into_parts();
        parts.path_and_query = req.uri().path_and_query().cloned();

        *req.uri_mut() = Uri::from_parts(parts).unwrap();
        match self.transport.send_request(req).await {
            Ok(resp) => {
                tracing::debug!(id=?self.id, "http2 request done");
                Ok(resp)
            }
            Err(err) => {
                tracing::error!(?err, id=?self.id, "http2 request failed");
                let mut state = self.state.write().await;
                *state = ConnectivityState::TransientFailure;
                Err(err.into())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    uri: Uri,
    addr: SocketAddr,
    credential: Credential,
}

impl Endpoint {
    fn new(uri: Uri, addr: SocketAddr, credential: Credential) -> Self {
        Endpoint {
            uri,
            addr,
            credential,
        }
    }
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
        if self.transport.is_some() {
            return self.get_state().await;
        }

        self.set_state(ConnectivityState::Connecting).await;

        match self.inner_connect().await {
            Ok(_) => {
                self.set_state(ConnectivityState::Ready).await;
            }
            Err(err) => {
                println!("connect {:?} failed, {:?}", self.endpoint, err);
                self.set_state(ConnectivityState::TransientFailure).await;
            }
        };

        self.get_state().await
    }

    pub async fn make_connection(&mut self) -> Result<Connection, Status> {
        let mut retry = 0;

        loop {
            match self.get_state().await {
                ConnectivityState::Ready => {
                    let transport = self
                        .transport
                        .clone()
                        .expect("transport must ok when ready");
                    let conn = Connection {
                        id: self.id.clone(),
                        uri: self.endpoint.uri.clone(),
                        state: self.state.clone(),
                        transport,
                    };
                    return Ok(conn);
                }
                ConnectivityState::Connecting => {
                    // let it go
                }
                ConnectivityState::Shutdown => {
                    unreachable!();
                }
                _ => {
                    let state = self.connect().await;
                    if state == ConnectivityState::Ready {
                        let transport = self
                            .transport
                            .clone()
                            .expect("transport must ok when ready");
                        let conn = Connection::new(
                            self.id.clone(),
                            self.endpoint.uri.clone(),
                            self.state.clone(),
                            transport,
                        );
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
        self.transport = match self.endpoint.credential {
            Credential::InSecure => self.tcp_connect().await,
            Credential::SecureTls => self.tls_connect().await,
        }
        .map(|sender| Some(sender))?;

        Ok(())
    }

    async fn tcp_connect(&mut self) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect(self.endpoint.addr).await?;

        let (sender, conn) = handshake(TokioExec, stream).await?;

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

#[async_trait::async_trait]
trait LoadBalancer {
    async fn pick(&mut self, channel: &mut Inner) -> Result<ChannelId, Status>;
}

struct PickFirstBalancer {
    prefer: Option<ChannelId>,
}

impl PickFirstBalancer {
    pub fn new() -> Self {
        PickFirstBalancer { prefer: None }
    }
}

#[async_trait::async_trait]
impl LoadBalancer for PickFirstBalancer {
    async fn pick(&mut self, channel: &mut Inner) -> Result<ChannelId, Status> {
        match self.prefer {
            Some(id) => {
                return Ok(id);
            }
            None => {
                // try connect on one by one
                let endpoints = channel.resolve_addrs()?;
                for endpoint in &endpoints {
                    match channel.connect_endpoint(endpoint.clone()).await {
                        Ok(id) => {
                            self.prefer = Some(id);
                            return Ok(id);
                        }
                        Err(err) => {
                            tracing::trace!(?endpoint, ?err, "connect endpoint failed");
                        }
                    }
                }

                tracing::error!(?endpoints, "all endpoints were failed to connected!");

                return Err(Status::internal("can not pick connection"));
            }
        }
    }
}

struct Resolver;

impl Resolver {
    fn resolve(addr: &Uri) -> Result<Vec<SocketAddr>, Status> {
        match addr.host() {
            Some(host) => {
                let addrs = ToSocketAddrs::to_socket_addrs(&(host, 0))?;

                let port = match addr.port_u16() {
                    Some(port) => port,
                    None => match addr.scheme_str() {
                        Some("http") => 80,
                        Some("https") => 443,
                        _ => {
                            return Err(Status::internal("unknown port in address"));
                        }
                    },
                };

                Ok(addrs
                    .map(|mut a| {
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

const INITIAL_BACKOFF: f32 = 1.0;
const MAX_BACKOFF: f32 = 120.0;
const JITTER: f32 = 0.2;

struct Backoff {
    backoff: f32,
}

impl Backoff {
    fn new() -> Self {
        Backoff { backoff: INITIAL_BACKOFF }
    }

    fn deadline(&self) -> Instant {
        let backoff = self.backoff;
        Instant::now().checked_add(Duration::from_secs_f32(backoff)).unwrap()
    }

    fn next(&mut self) {
       self.backoff = f32::min(1.6 * self.backoff, MAX_BACKOFF);
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
