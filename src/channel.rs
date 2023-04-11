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
use rand::{thread_rng, Rng};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{sleep_until, timeout_at, Instant};

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
    inner: Arc<Mutex<ConnectionManager>>,
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

        let mut inner = ConnectionManager::new(opts);
        let balancer = RandomBalancer::new();

        inner.init();

        Ok(Channel {
            inner: Arc::new(Mutex::new(inner)),
            load_balancer: Arc::new(Mutex::new(Box::new(balancer))),
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

struct ConnectionManager {
    opts: Opts,
    conns: BTreeMap<ChannelId, SubChannel>,
}

impl ConnectionManager {
    fn new(opts: Opts) -> Self {
        ConnectionManager {
            opts,
            conns: BTreeMap::new(),
        }
    }

    pub fn init(&mut self) {
        for endpoint in self.resolve_addrs() {
            let conn = SubChannel::new(endpoint);
            self.conns.insert(conn.id, conn);
        }
    }

    async fn make_connection(&mut self, id: ChannelId) -> Result<Connection, Status> {
        match self.conns.get_mut(&id) {
            Some(channel) => channel.make_connection().await,
            None => Err(Status::internal("can not request connection")),
        }
    }

    fn get_conns(&self) -> Vec<ChannelId> {
        self.conns.keys().cloned().collect()
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

    pub fn resolve_addrs(&self) -> Vec<Endpoint> {
        let mut endpoints = Vec::new();

        for uri in &self.get_opts().uris {
            match Resolver::resolve(uri) {
                Ok(addrs) => {
                    for addr in addrs {
                        let endpoint =
                            Endpoint::new(uri.clone(), addr, self.get_opts().credentail.clone());
                        endpoints.push(endpoint);
                    }
                }
                Err(err) => {
                    tracing::error!(?err, ?uri, "resolve failed");
                }
            }
        }

        endpoints
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
    state: Arc<Mutex<ConnectivityState>>,
}

impl Connection {
    pub fn new(
        id: ChannelId,
        uri: Uri,
        state: Arc<Mutex<ConnectivityState>>,
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
                let mut state = self.state.lock().await;
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
    state: Arc<Mutex<ConnectivityState>>,
    transport: Option<SendRequest<BoxBody>>,
}

impl SubChannel {
    pub fn new(endpoint: Endpoint) -> Self {
        SubChannel {
            id: ChannelId::next_id(),
            endpoint,
            state: Arc::new(Mutex::new(ConnectivityState::Idle)),
            transport: None,
        }
    }

    pub fn get_id(&self) -> ChannelId {
        self.id
    }

    pub async fn get_state(&self) -> ConnectivityState {
        *self.state.lock().await
    }

    pub async fn connect(&mut self) -> ConnectivityState {
        let mut state = self.state.lock().await;

        *state = ConnectivityState::Connecting;
        match self.inner_connect().await {
            Some(transport) => {
                self.transport = Some(transport);
                *state = ConnectivityState::Ready;
            }
            None => {
                *state = ConnectivityState::TransientFailure;
            }
        }

        *state
    }

    pub async fn make_connection(&mut self) -> Result<Connection, Status> {
        let mut state = self.state.lock().await;

        // do connect when not ready
        if *state == ConnectivityState::Idle || *state == ConnectivityState::TransientFailure {
            *state = ConnectivityState::Connecting;
            match self.inner_connect().await {
                Some(transport) => {
                    self.transport = Some(transport);
                    *state = ConnectivityState::Ready;
                }
                None => {
                    *state = ConnectivityState::TransientFailure;
                }
            }
        }

        match *state {
            ConnectivityState::Ready => {
                let transport = self
                    .transport
                    .clone()
                    .expect("transport must not None when ConnectivityState::Ready");

                let conn = Connection::new(
                    self.id,
                    self.endpoint.uri.clone(),
                    self.state.clone(),
                    transport,
                );
                Ok(conn)
            }
            ConnectivityState::TransientFailure => Err(Status::internal("make connection failed")),
            _ => {
                unreachable!("make_connection, state={:?}", *state);
            }
        }
    }

    async fn inner_connect(&self) -> Option<SendRequest<BoxBody>> {
        let mut retrier = Retrier::new();

        loop {
            match timeout_at(retrier.connect_deadline(), self.build_transport()).await {
                Ok(ret) => {
                    match ret {
                        Ok(transport) => {
                            return Some(transport);
                        }
                        Err(err) => {
                            tracing::error!(endpoint=?self.endpoint, ?err, "connect failed");
                            retrier.next();
                        }
                    };
                }
                Err(elapsed) => {
                    tracing::error!(endpoint=?self.endpoint, timeout=?elapsed, "connect timeout");
                }
            }

            sleep_until(retrier.deadline()).await;

            retrier.next();

            if retrier.retry >= retrier.max_retry() {
                return None;
            }
        }
    }

    async fn build_transport(&self) -> Result<SendRequest<BoxBody>, Status> {
        match self.endpoint.credential {
            Credential::InSecure => self.tcp_connect().await,
            Credential::SecureTls => self.tls_connect().await,
        }
    }

    async fn tcp_connect(&self) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect(self.endpoint.addr).await?;

        let (sender, conn) = handshake(TokioExec, stream).await?;

        let id = self.id;
        let state = self.state.clone();
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                tracing::error!(%err, "SubChannel[{:?}] transport failed", id);
                *state.lock().await = ConnectivityState::TransientFailure;
            } else {
                tracing::debug!("SubChannel[{:?}] transport done", id);
                *state.lock().await = ConnectivityState::Idle;
            }
        });

        Ok(sender)
    }

    async fn tls_connect(&self) -> Result<SendRequest<BoxBody>, Status> {
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
    async fn pick(&mut self, channel: &mut ConnectionManager) -> Result<ChannelId, Status>;
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
    async fn pick(&mut self, manager: &mut ConnectionManager) -> Result<ChannelId, Status> {
        match self.prefer {
            Some(id) => {
                return Ok(id);
            }
            None => {
                let conns = manager.get_conns();
                if conns.is_empty() {
                    return Err(Status::internal("no SubChannels"));
                }
                // try connect on one by one
                for id in conns {
                    match manager.make_connection(id).await {
                        Ok(_) => {
                            self.prefer = Some(id);
                            return Ok(id);
                        }
                        Err(err) => {
                            tracing::error!(?id, ?err, "make_connection failed");
                        }
                    }
                }

                tracing::error!("all subchannel were failed to connected!");

                return Err(Status::internal("can not pick connection"));
            }
        }
    }
}

struct RandomBalancer {}

impl RandomBalancer {
    pub fn new() -> Self {
        RandomBalancer {}
    }
}

#[async_trait::async_trait]
impl LoadBalancer for RandomBalancer {
    async fn pick(&mut self, manager: &mut ConnectionManager) -> Result<ChannelId, Status> {
        let conns = manager.get_conns();
        if conns.is_empty() {
            return Err(Status::internal("no SubChannels"));
        }

        let chosen = thread_rng().gen_range(0..conns.len());

        Ok(conns[chosen])
    }
}

struct Resolver;

impl Resolver {
    fn resolve(addr: &Uri) -> Result<impl Iterator<Item = SocketAddr>, Status> {
        match addr.host() {
            Some(host) => {
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

                let addrs = ToSocketAddrs::to_socket_addrs(&(host, port))?;

                Ok(addrs.into_iter())
            }
            None => Err(Status::internal("invalid host in address")),
        }
    }
}

const INITIAL_BACKOFF: f32 = 1.0;
const MAX_BACKOFF: f32 = 120.0;
const JITTER: f32 = 0.2;
const MIN_CONNECT_TIMEOUT: f32 = 20.0;

struct Retrier {
    backoff: f32,
    retry: usize,
}

impl Retrier {
    fn new() -> Self {
        Retrier {
            backoff: INITIAL_BACKOFF,
            retry: 0,
        }
    }

    fn connect_deadline(&self) -> Instant {
        let timeout = f32::max(self.backoff, MIN_CONNECT_TIMEOUT);
        Instant::now()
            .checked_add(Duration::from_secs_f32(timeout))
            .unwrap()
    }

    fn deadline(&self) -> Instant {
        let backoff = self.backoff + self.backoff * thread_rng().gen_range(-JITTER..=JITTER);
        Instant::now()
            .checked_add(Duration::from_secs_f32(backoff))
            .unwrap()
    }

    fn next(&mut self) {
        self.backoff = f32::min(1.6 * self.backoff, MAX_BACKOFF);
        self.retry += 1;
    }

    fn max_retry(&self) -> usize {
        4
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
