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
use hyper::http::uri::Scheme;
use hyper::{Request, Response, Uri};
use rand::{thread_rng, Rng};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
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
    connect_manager: Arc<Mutex<ConnectionManager>>,
    load_balancer: Arc<Mutex<Box<dyn LoadBalancer + Send + Sync>>>,
    resolver: Arc<Box<dyn Resolver + Send + Sync>>,
}

impl Channel {
    pub fn new(uri: Uri) -> Result<Channel, Status> {
        if uri.scheme().is_none() || uri.host().is_none() {
            return Err(Status::invalid_argument("invalid address"));
        }

        let scheme = uri
            .scheme()
            .ok_or(Status::invalid_argument("invalid scheme"))?;

        let mut resolver = if scheme == &Scheme::HTTP || scheme == &Scheme::HTTPS {
            StaticResolver::new()
        } else {
            return Err(Status::invalid_argument("unsupport scheme to resolve"));
        };

        let opts = Opts {
            uri,
            credentail: Credential::InSecure,
        };

        let mut inner = ConnectionManager::new(opts);
        let balancer = RandomBalancer::new();

        Ok(Channel {
            connect_manager: Arc::new(Mutex::new(inner)),
            load_balancer: Arc::new(Mutex::new(Box::new(balancer))),
            resolver: Arc::new(Box::new(resolver)),
        })
    }

    pub async fn call(&mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        let mut inner = self.connect_manager.lock().await;

        let mut load_balancer = self.load_balancer.lock().await;

        let id = load_balancer.pick(&mut inner).await?;

        let conn = inner.make_connection(id).await?;

        conn.request(req).await
    }
}

struct ConnectionManager {
    opts: Opts,
    conns: Vec<SubChannel>,
    state_rx: mpsc::UnboundedReceiver<ConnectivityState>,
    state_tx: mpsc::UnboundedSender<ConnectivityState>,
}

impl ConnectionManager {
    fn new(opts: Opts) -> Self {
        let (state_tx, state_rx) = mpsc::unbounded_channel();

        ConnectionManager {
            opts,
            conns: Vec::new(),
            state_rx,
            state_tx,
        }
    }

    async fn make_connection(&mut self, id: ChannelId) -> Result<Connection, Status> {
        for conn in &mut self.conns {
            if conn.get_id().await == id {
                return conn.make_connection().await;
            }
        }

        Err(Status::internal("can not request connection"))
    }

    async fn get_ready_conns(&self) -> Vec<ChannelId> {
        let mut conns = Vec::new();

        for conn in &self.conns {
            let conn = conn.read().await;

            if conn.state() == ConnectivityState::Ready {
                conns.push(conn.id())
            }
        }

        conns
    }

    async fn get_conns(&self) -> Vec<ChannelId> {
        let mut conns = Vec::new();

        for conn in &self.conns {
            conns.push(conn.get_id().await);
        }

        conns
    }

    // fn get_opts(&self) -> &Opts {
    //     &self.opts
    // }

    // pub async fn connect_endpoint(&mut self, endpoint: ResolvedAddr) -> Result<ChannelId, Status> {
    //     let mut sub_channel = SubChannelInner::new(endpoint);
    //     let state = sub_channel.connect().await;
    //     if state == ConnectivityState::Ready {
    //         let id = sub_channel.get_id();
    //         self.conns.insert(id, sub_channel);
    //         Ok(id)
    //     } else {
    //         Err(Status::internal("connect endpoint failed"))
    //     }
    // }

    // pub fn resolve_addrs(&self) -> Vec<ResolvedAddr> {
    //     let mut endpoints = Vec::new();

    //     for uri in &self.get_opts().uris {
    //         match Resolver::resolve(uri) {
    //             Ok(addrs) => {
    //                 for addr in addrs {
    //                     let endpoint =
    //                         ResolvedAddr::new(uri.clone(), addr, self.get_opts().credentail.clone());
    //                     endpoints.push(endpoint);
    //                 }
    //             }
    //             Err(err) => {
    //                 tracing::error!(?err, ?uri, "resolve failed");
    //             }
    //         }
    //     }

    //     endpoints
    // }
}

#[derive(Clone)]
struct Opts {
    uri: Uri,
    credentail: Credential,
}

#[derive(Clone)]
pub(crate) struct Connection {
    inner: SubChannel,
}

impl Connection {
    fn new(inner: SubChannel) -> Self {
        Connection { inner }
    }

    pub async fn request(mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        // let mut req = req;
        // let mut parts = self.uri.into_parts();
        // parts.path_and_query = req.uri().path_and_query().cloned();

        // *req.uri_mut() = Uri::from_parts(parts).unwrap();
        // match self.transport.send_request(req).await {
        //     Ok(resp) => {
        //         tracing::debug!(id=?self.id, "http2 request done");
        //         Ok(resp)
        //     }
        //     Err(err) => {
        //         tracing::error!(?err, id=?self.id, "http2 request failed");
        //         let mut state = self.state.lock().await;
        //         *state = ConnectivityState::TransientFailure;
        //         Err(err.into())
        //     }
        // }
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedAddr {
    uri: Uri,
    host: String,
    port: u16,
}

impl ResolvedAddr {
    fn new(uri: Uri, host: impl ToString, port: u16) -> Self {
        ResolvedAddr {
            uri,
            host: host.to_string(),
            port,
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
    inner: Arc<RwLock<SubChannelInner>>,
}

impl SubChannel {
    pub fn new(
        addr: ResolvedAddr,
        credential: Credential,
        state_tx: mpsc::UnboundedSender<ConnectivityState>,
    ) -> Self {
        let inner = SubChannelInner::new(addr, credential, state_tx);

        SubChannel {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub async fn get_id(&self) -> ChannelId {
        let inner = self.inner.read().await;
        inner.id()
    }

    pub async fn get_state(&self) -> ConnectivityState {
        let inner = self.inner.read().await;
        inner.state()
    }

    pub(crate) async fn make_connection(&self) -> Result<Connection, Status> {
        let inner = self.inner.write().await;

        inner.make_connection(self.clone()).await
    }

    pub(crate) async fn update_state(&self, state: ConnectivityState) {
        let mut inner = self.inner.write().await;

        inner.update_state(state);
    }

    pub(crate) async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, SubChannelInner> {
        self.inner.read().await
    }

    pub(crate) async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, SubChannelInner> {
        self.inner.write().await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubChannelInner {
    id: ChannelId,
    addr: ResolvedAddr,
    credential: Credential,
    state: ConnectivityState,
    transport: Option<SendRequest<BoxBody>>,
    state_tx: mpsc::UnboundedSender<ConnectivityState>,
}

impl SubChannelInner {
    pub fn new(
        addr: ResolvedAddr,
        credential: Credential,
        state_tx: mpsc::UnboundedSender<ConnectivityState>,
    ) -> Self {
        SubChannelInner {
            id: ChannelId::next_id(),
            state_tx,
            addr,
            credential,
            state: ConnectivityState::Idle,
            transport: None,
        }
    }

    pub fn id(&self) -> ChannelId {
        self.id
    }

    pub fn state(&self) -> ConnectivityState {
        self.state
    }

    fn update_state(&mut self, state: ConnectivityState) {
        if state == ConnectivityState::Idle || state == ConnectivityState::TransientFailure {
            self.transport.take();
        }
        self.state = state;
        self.state_tx.send(state);
    }

    pub(crate) async fn make_connection(&self, channel: SubChannel) -> Result<Connection, Status> {
        // do connect when not ready
        if self.state == ConnectivityState::Idle
            || self.state == ConnectivityState::TransientFailure
        {
            let channel_cloned = channel.clone();
            self.build_transport(channel_cloned).await;
        }

        match self.state {
            ConnectivityState::Ready => Ok(Connection::new(channel)),
            ConnectivityState::TransientFailure => Err(Status::internal("make connection failed")),
            _ => {
                unreachable!("make_connection, state={:?}", self.state);
            }
        }
    }

    pub(crate) async fn connect(&mut self, channel: SubChannel) -> ConnectivityState {
        self.state = ConnectivityState::Connecting;
        match self.build_transport(channel).await {
            Ok(transport) => {
                self.transport = Some(transport);
                self.update_state(ConnectivityState::Ready);
            }
            Err(err) => self.update_state(ConnectivityState::TransientFailure),
        }

        self.state
    }

    async fn build_transport(&self, channel: SubChannel) -> Result<SendRequest<BoxBody>, Status> {
        match self.credential {
            Credential::InSecure => self.tcp_connect(channel).await,
            Credential::SecureTls => self.tls_connect(channel).await,
        }
    }

    async fn tcp_connect(&self, channel: SubChannel) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect((self.addr.host.clone(), self.addr.port)).await?;

        let (sender, conn) = handshake(TokioExec, stream).await?;

        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                tracing::error!(%err, "SubChannel[{:?}] transport failed", channel.get_id().await);
                channel.update_state(ConnectivityState::TransientFailure);
            } else {
                tracing::debug!("SubChannel[{:?}] transport done", channel.get_id().await);
                channel.update_state(ConnectivityState::Idle);
            }
        });

        Ok(sender)
    }

    async fn tls_connect(&self, channel: SubChannel) -> Result<SendRequest<BoxBody>, Status> {
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
                let conns = manager.get_conns().await;
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
        let conns = manager.get_ready_conns().await;
        if conns.is_empty() {
            return Err(Status::internal("no SubChannels"));
        }

        let chosen = thread_rng().gen_range(0..conns.len());

        Ok(conns[chosen])
    }
}

#[async_trait::async_trait]
trait Resolver {
    fn resolve(&self, target: &Uri) -> Result<Vec<ResolvedAddr>, Status>;
}

struct StaticResolver;

impl StaticResolver {
    fn new() -> Self {
        StaticResolver {}
    }
}

impl StaticResolver {
    fn resolve(target: &Uri) -> Result<Vec<ResolvedAddr>, Status> {
        match target.host() {
            Some(host) => {
                let port = match target.port_u16() {
                    Some(port) => port,
                    None => match target.scheme_str() {
                        Some("http") => 80,
                        Some("https") => 443,
                        sheme => {
                            return Err(Status::internal(format!(
                                "unsupported sheme {:?} for StaticResolver",
                                sheme
                            )));
                        }
                    },
                };

                let mut resolved_addrs = Vec::new();

                resolved_addrs.push(ResolvedAddr::new(target.clone(), host, port));

                Ok(resolved_addrs)
            }
            None => Err(Status::internal("invalid host in address")),
        }
    }
}

#[async_trait::async_trait]
impl Resolver for StaticResolver {
    fn resolve(&self, target: &Uri) -> Result<Vec<ResolvedAddr>, Status> {
        self.resolve(target)
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
