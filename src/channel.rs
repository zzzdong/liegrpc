use std::net::SocketAddr;
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
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::time::{sleep_until, timeout_at, Instant};

use crate::status::Status;

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
    opts: Opts,
    pool: SharedPool,
    balancer: Arc<Mutex<Box<dyn LoadBalancer + Send + Sync>>>,
    resolver: Arc<Box<dyn Resolver + Send + Sync>>,
}

impl Channel {
    pub fn new(conn_str: impl AsRef<str>) -> Result<Channel, Status> {
        const DNS_SCHEME: &'static str = "dns:";
        const IP_SCHEME: &'static str = "ip:";

        let conn_str = conn_str.as_ref();

        let resolver: Box<dyn Resolver + Send + Sync> = if conn_str.starts_with(DNS_SCHEME) {
            let host = conn_str
                .strip_prefix(DNS_SCHEME)
                .ok_or(Status::invalid_argument("invalid address"))?;

            Box::new(DnsResolver::new(host))
        } else if conn_str.starts_with(IP_SCHEME) {
            let hosts = conn_str
                .strip_prefix(IP_SCHEME)
                .ok_or(Status::invalid_argument("invalid address"))?;

            let mut addrs = Vec::new();
            for part in hosts.split(',') {
                let addr = part
                    .parse::<SocketAddr>()
                    .map_err(|err| Status::invalid_argument("invalid address").with_cause(err))?;
                addrs.push(addr);
            }

            Box::new(StaticResolver::new(addrs))
        } else {
            return Err(Status::invalid_argument("invalid address"));
        };

        let opts = Opts {
            credentail: Credential::InSecure,
            auto_reconnect: true,
        };

        let balancer = RandomBalancer::new();

        let pool = SharedPool::new();

        Ok(Channel {
            opts,
            pool,
            balancer: Arc::new(Mutex::new(Box::new(balancer))),
            resolver: Arc::new(resolver),
        })
    }

    pub async fn call(&mut self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        // if self.pool.get_conns().await.is_empty() {
        //     let resolved = self.resolver.resolve().await?;
        //     let mut load_balancer = self.balancer.lock().await;
        //     load_balancer
        //         .update_channel_state(
        //             self.pool.clone(),
        //             ChannelState {
        //                 opts: self.opts.clone(),
        //                 addresses: resolved,
        //             },
        //         )
        //         .await;
        // }
        let state = self.pool.get_state().await;
        match state {
            ConnectivityState::Idle | ConnectivityState::TransientFailure => {
                let resolved = self.resolver.resolve().await?;
                let mut load_balancer = self.balancer.lock().await;
                load_balancer
                    .update_channel_state(
                        self.pool.clone(),
                        ChannelState {
                            opts: self.opts.clone(),
                            addresses: resolved,
                        },
                    )
                    .await;

                self.pool.wait_state(ConnectivityState::Ready).await;
            }
            _ => {}
        }

        let mut load_balancer = self.balancer.lock().await;

        let ch = load_balancer.pick(self.pool.clone()).await?;

        let conn = ch.make_connection().await?;

        conn.request(req).await
    }
}

pub struct ChannelState {
    opts: Opts,
    addresses: Vec<ResolvedAddr>,
}

#[derive(Clone)]
pub struct SharedPool {
    inner: Arc<Mutex<Pool>>,
    state_tx: broadcast::Sender<ConnectionState>,
}

impl SharedPool {
    fn new() -> Self {
        let (state_tx, mut state_rx) = broadcast::channel(16);

        let inner = Arc::new(Mutex::new(Pool::new(state_tx.clone())));

        let pool = inner.clone();

        tokio::task::spawn(async move {
            while let Ok(state) = state_rx.recv().await {
                let mut pool = pool.lock().await;
                pool.on_connection_state(state).await;
            }
        });

        SharedPool { inner, state_tx }
    }

    pub async fn get_state(&self) -> ConnectivityState {
        let inner = self.inner.lock().await;
        inner.get_state().await
    }

    pub async fn try_connect(&self, addr: &ResolvedAddr, opts: Opts) -> Result<SubChannel, Status> {
        let mut inner = self.inner.lock().await;

        inner.try_connect(addr, opts).await
    }

    pub async fn get_conns(&self) -> Vec<SubChannel> {
        let inner = self.inner.lock().await;

        inner.conns.clone()
    }

    pub async fn get_ready_conns(&self) -> Vec<SubChannel> {
        let inner = self.inner.lock().await;

        let mut ret = Vec::new();

        for conn in &inner.conns {
            if conn.get_state().await == ConnectivityState::Ready {
                ret.push(conn.clone())
            }
        }

        ret
    }

    pub async fn update_channel_state(&self, state: ChannelState) {
        let mut inner = self.inner.lock().await;

        inner.update_channel_state(state).await;
    }

    pub async fn wait_state(&self, expect: ConnectivityState) {
        let mut state_rx = self.state_tx.subscribe();

        while let Ok(state) = state_rx.recv().await {
            if state.state == expect {
                return;
            }
        }
    }
}

struct Pool {
    conns: Vec<SubChannel>,
    state_tx: broadcast::Sender<ConnectionState>,
}

impl Pool {
    fn new(state_tx: broadcast::Sender<ConnectionState>) -> Self {
        Pool {
            conns: Vec::new(),
            state_tx,
        }
    }

    async fn try_connect(&mut self, addr: &ResolvedAddr, opt: Opts) -> Result<SubChannel, Status> {
        let ch = SubChannel::new(addr.clone(), opt.credentail.clone(), self.state_tx.clone());

        match ch.connect().await {
            Ok(_) => {
                self.conns.push(ch.clone());
                Ok(ch)
            }
            Err(err) => Err(err),
        }
    }

    fn new_subchannel(&mut self, addr: &ResolvedAddr, opt: Opts) -> SubChannel {
        let ch = SubChannel::new(addr.clone(), opt.credentail.clone(), self.state_tx.clone());

        self.conns.push(ch.clone());

        ch
    }

    async fn get_conns_snapshot(&self) -> Vec<SubChannelInner> {
        let mut conns = Vec::new();

        for conn in &self.conns {
            let inner = conn.inner.read().await;
            conns.push(inner.clone());
        }

        conns
    }

    async fn get_state(&self) -> ConnectivityState {
        if self
            .get_conns_snapshot()
            .await
            .iter()
            .any(|conn| conn.state() == ConnectivityState::Ready)
        {
            return ConnectivityState::Ready;
        } else if self
            .get_conns_snapshot()
            .await
            .iter()
            .any(|conn| conn.state() == ConnectivityState::Connecting)
        {
            return ConnectivityState::Connecting;
        } else if self
            .get_conns_snapshot()
            .await
            .iter()
            .any(|conn| conn.state() == ConnectivityState::Idle)
        {
            return ConnectivityState::Idle;
        } else if self
            .get_conns_snapshot()
            .await
            .iter()
            .all(|conn| conn.state() == ConnectivityState::TransientFailure)
        {
            return ConnectivityState::TransientFailure;
        } else {
            return ConnectivityState::Shutdown;
        }
    }

    async fn update_channel_state(&mut self, state: ChannelState) {
        // remove expired conns
        self.get_conns_snapshot().await.retain_mut(|conn| {
            match state.addresses.iter().find(|addr| conn.addr() == **addr) {
                Some(conn) => true,
                None => {
                    conn.update_state(ConnectivityState::Shutdown);
                    false
                }
            }
        });

        // add new conn by addr
        for addr in &state.addresses {
            if !self
                .get_conns_snapshot()
                .await
                .iter()
                .any(|conn| conn.addr() == *addr)
            {
                let channel = self.new_subchannel(addr, state.opts.clone());
                channel.set_auto_reconnect(state.opts.auto_reconnect).await;
                let channel_cloned = channel.clone();
                tokio::task::spawn(async move {
                    if let Err(err) = channel_cloned.connect().await {
                        tracing::error!(%err, "SubChannel connect failed");
                    }
                });
                self.conns.push(channel);
            }
        }
    }

    async fn on_connection_state(&mut self, conn_state: ConnectionState) {
        match conn_state.state {
            ConnectivityState::Shutdown => {
                let mut found = None;
                for (i, conn) in self.conns.iter().enumerate() {
                    if conn.get_id().await == conn_state.id {
                        found = Some(i);
                        break;
                    }
                }
                if let Some(i) = found {
                    self.conns.remove(i);
                }
            }
            ConnectivityState::TransientFailure => {
                let conn = self.find_conn(conn_state.id).await;
                if let Some(conn) = conn {
                    tokio::task::spawn(async move { conn.try_reconnect().await });
                }
            }
            _ => {}
        }
    }

    async fn find_conn(&self, id: ChannelId) -> Option<SubChannel> {
        let mut found = None;
        for (i, conn) in self.conns.iter().enumerate() {
            if conn.get_id().await == id {
                found = Some(conn.clone());
                break;
            }
        }

        found
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionState {
    id: ChannelId,
    state: ConnectivityState,
}

impl ConnectionState {
    fn new(id: ChannelId, state: ConnectivityState) -> ConnectionState {
        ConnectionState { id, state }
    }
}

#[derive(Clone)]
pub struct Opts {
    credentail: Credential,
    auto_reconnect: bool,
}

#[derive(Clone)]
pub(crate) struct Connection {
    channel: SubChannel,
}

impl Connection {
    fn new(inner: SubChannel) -> Self {
        Connection { channel: inner }
    }

    pub async fn request(&self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        self.channel.request(req).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAddr {
    uri: Uri,
    addr: SocketAddr,
}

impl ResolvedAddr {
    fn new(uri: Uri, addr: SocketAddr) -> Self {
        ResolvedAddr { uri, addr }
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
pub struct SubChannel {
    inner: Arc<RwLock<SubChannelInner>>,
}

impl SubChannel {
    pub fn new(
        addr: ResolvedAddr,
        credential: Credential,
        state_tx: broadcast::Sender<ConnectionState>,
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

    pub async fn get_addr(&self) -> ResolvedAddr {
        let inner = self.inner.read().await;
        inner.addr()
    }

    pub async fn get_auto_reconnect(&self) -> bool {
        let inner = self.inner.read().await;
        inner.auto_reconnect
    }

    pub async fn set_auto_reconnect(&self, auto_reconnect: bool) {
        let mut inner = self.inner.write().await;
        inner.set_auto_reconnect(auto_reconnect);
    }

    pub async fn request(&self, req: Request<BoxBody>) -> Result<Response<Incoming>, Status> {
        let this = self.clone();
        let mut inner = this.inner.write().await;

        inner.request(req).await
    }

    pub(crate) async fn connect(&self) -> Result<(), Status> {
        let mut inner = self.inner.write().await;

        inner.connect(self.clone()).await
    }

    pub(crate) async fn reconnect(&self) -> Result<(), Status> {
        let mut inner = self.inner.write().await;

        inner.reconnect(self.clone()).await
    }

    pub(crate) async fn try_reconnect(&self) -> Result<(), Status> {
        let mut inner = self.inner.write().await;
        if inner.auto_reconnect {
            inner.reconnect(self.clone()).await?;
        }

        Ok(())
    }

    pub(crate) async fn make_connection(&self) -> Result<Connection, Status> {
        let inner = self.inner.write().await;

        inner.make_connection(self.clone()).await
    }

    pub(crate) async fn update_state(&self, state: ConnectivityState) {
        let mut inner = self.inner.write().await;

        inner.update_state(state);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SubChannelInner {
    id: ChannelId,
    addr: ResolvedAddr,
    credential: Credential,
    state: ConnectivityState,
    transport: Option<SendRequest<BoxBody>>,
    state_tx: broadcast::Sender<ConnectionState>,
    auto_reconnect: bool,
}

impl SubChannelInner {
    pub fn new(
        addr: ResolvedAddr,
        credential: Credential,
        state_tx: broadcast::Sender<ConnectionState>,
    ) -> Self {
        SubChannelInner {
            id: ChannelId::next_id(),
            state_tx,
            addr,
            credential,
            state: ConnectivityState::Idle,
            transport: None,
            auto_reconnect: false,
        }
    }

    pub fn id(&self) -> ChannelId {
        self.id
    }

    pub fn state(&self) -> ConnectivityState {
        self.state
    }

    pub fn addr(&self) -> ResolvedAddr {
        self.addr.clone()
    }

    pub fn set_auto_reconnect(&mut self, auto_reconnect: bool) {
        self.auto_reconnect = auto_reconnect;
    }

    pub async fn request(
        &mut self,
        mut req: Request<BoxBody>,
    ) -> Result<Response<Incoming>, Status> {
        match self.transport {
            Some(ref transport) => {
                let mut parts = self.addr.uri.clone().into_parts();
                parts.path_and_query = req.uri().path_and_query().cloned();
                parts.scheme = Some(Scheme::HTTP);
                *req.uri_mut() = Uri::from_parts(parts).expect("uri error");
                // *req.uri_mut() = self.addr.uri.clone();

                match transport.clone().send_request(req).await {
                    Ok(resp) => {
                        tracing::debug!(id=?self.id, "http2 request done");
                        Ok(resp)
                    }
                    Err(err) => {
                        tracing::error!(?err, id=?self.id, "http2 request failed");
                        self.update_state(ConnectivityState::TransientFailure);
                        Err(err.into())
                    }
                }
            }
            None => Err(Status::internal("SubChannel is broken")),
        }
    }

    fn update_state(&mut self, state: ConnectivityState) {
        if state == ConnectivityState::Idle
            || state == ConnectivityState::TransientFailure
            || state == ConnectivityState::Shutdown
        {
            let _ = self.transport.take();
        }
        self.state = state;
        self.state_tx
            .send(ConnectionState::new(self.id, state))
            .expect("SubChannel send state failed");
    }

    pub(crate) async fn make_connection(&self, channel: SubChannel) -> Result<Connection, Status> {
        // do connect when not ready
        if self.state == ConnectivityState::Idle
            || self.state == ConnectivityState::TransientFailure
        {
            let channel_cloned = channel.clone();
            match self.build_transport(channel_cloned).await {
                Ok(_conn) => {
                    tracing::debug!(id=?self.id, addr=?self.addr, "build http2 transport succeded")
                }
                Err(err) => {
                    tracing::error!(err=?err, id=?self.id, addr=?self.addr, "build http2 transport failed")
                }
            }
        }

        match self.state {
            ConnectivityState::Ready => Ok(Connection::new(channel)),
            ConnectivityState::TransientFailure => Err(Status::internal("make connection failed")),
            _ => {
                unreachable!("make_connection, state={:?}", self.state);
            }
        }
    }

    pub(crate) async fn connect(&mut self, channel: SubChannel) -> Result<(), Status> {
        self.update_state(ConnectivityState::Connecting);
        match self.build_transport(channel).await {
            Ok(transport) => {
                self.transport = Some(transport);
                self.update_state(ConnectivityState::Ready);
                Ok(())
            }
            Err(err) => {
                self.update_state(ConnectivityState::TransientFailure);
                Err(err)
            }
        }
    }

    pub(crate) async fn reconnect(&mut self, channel: SubChannel) -> Result<(), Status> {
        match self.build_transport(channel).await {
            Ok(transport) => {
                self.transport = Some(transport);
                self.update_state(ConnectivityState::Ready);
                Ok(())
            }
            Err(err) => {
                self.update_state(ConnectivityState::TransientFailure);
                Err(err)
            }
        }
    }

    async fn build_transport(&self, channel: SubChannel) -> Result<SendRequest<BoxBody>, Status> {
        match self.credential {
            Credential::InSecure => self.tcp_connect(channel).await,
            Credential::SecureTls => self.tls_connect(channel).await,
        }
    }

    async fn tcp_connect(&self, channel: SubChannel) -> Result<SendRequest<BoxBody>, Status> {
        let stream = TcpStream::connect(self.addr.addr).await?;

        let (sender, conn) = handshake(TokioExec, stream).await?;

        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                tracing::error!(%err, "SubChannel[{:?}] transport failed", channel.get_id().await);
                channel
                    .update_state(ConnectivityState::TransientFailure)
                    .await;
            } else {
                tracing::debug!("SubChannel[{:?}] transport done", channel.get_id().await);
                channel.update_state(ConnectivityState::Idle).await;
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
    async fn pick(&mut self, pool: SharedPool) -> Result<SubChannel, Status>;
    async fn update_channel_state(&mut self, pool: SharedPool, state: ChannelState);
}

struct PickFirstBalancer {
    prefer: Option<SubChannel>,
}

impl PickFirstBalancer {
    pub fn new() -> Self {
        PickFirstBalancer { prefer: None }
    }

    async fn try_connect<'a>(&mut self, pool: SharedPool, state: ChannelState) {
        // try one by one
        for addr in &state.addresses {
            match pool.try_connect(addr, state.opts.clone()).await {
                Ok(ch) => {
                    tracing::debug!("connect new subchannel done, {:?}", ch.get_id().await);
                    self.prefer = Some(ch);
                    return;
                }
                Err(err) => {
                    tracing::error!(%err, "connect failed");
                }
            }
        }

        tracing::error!("all subchannel were failed to connected!");
    }
}

#[async_trait::async_trait]
impl LoadBalancer for PickFirstBalancer {
    async fn pick(&mut self, pool: SharedPool) -> Result<SubChannel, Status> {
        match self.prefer {
            Some(ref conn) => {
                return Ok(conn.clone());
            }
            None => {
                return Err(Status::internal("can not pick connection"));
            }
        }
    }

    async fn update_channel_state(&mut self, pool: SharedPool, state: ChannelState) {
        match &self.prefer {
            Some(ch) => {
                let cur_addr = ch.get_addr().await;

                if state
                    .addresses
                    .iter()
                    .find(|addr| **addr == cur_addr)
                    .is_none()
                {
                    let _ = self.prefer.take();

                    self.try_connect(pool, state).await;
                }
            }
            None => {
                self.try_connect(pool, state).await;
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
    async fn pick(&mut self, pool: SharedPool) -> Result<SubChannel, Status> {
        let conns = pool.get_ready_conns().await;
        if conns.is_empty() {
            return Err(Status::internal("no SubChannels"));
        }

        let chosen = thread_rng().gen_range(0..conns.len());

        Ok(conns[chosen].clone())
    }

    async fn update_channel_state(&mut self, pool: SharedPool, state: ChannelState) {
        pool.update_channel_state(state).await
    }
}

#[async_trait::async_trait]
trait Resolver {
    async fn resolve(&self) -> Result<Vec<ResolvedAddr>, Status>;
}

struct StaticResolver {
    addrs: Vec<SocketAddr>,
}

impl StaticResolver {
    fn new(addrs: Vec<SocketAddr>) -> Self {
        StaticResolver { addrs }
    }
}

impl StaticResolver {
    fn resolve_addr(&self) -> Result<Vec<ResolvedAddr>, Status> {
        let mut resolved_addrs = Vec::new();

        for addr in &self.addrs {
            let uri = Uri::builder()
                .authority(addr.to_string())
                .build()
                .map_err(|err| Status::invalid_argument("invalid addr").with_cause(err))?;
            resolved_addrs.push(ResolvedAddr::new(uri, *addr));
        }

        Ok(resolved_addrs)
    }
}

#[async_trait::async_trait]
impl Resolver for StaticResolver {
    async fn resolve(&self) -> Result<Vec<ResolvedAddr>, Status> {
        self.resolve_addr()
    }
}

struct DnsResolver {
    target: String,
}

impl DnsResolver {
    fn new(target: impl ToString) -> Self {
        DnsResolver {
            target: target.to_string(),
        }
    }
}

impl DnsResolver {
    async fn resolve_addr(&self) -> Result<Vec<ResolvedAddr>, Status> {
        let mut resolved_addrs = Vec::new();

        let uri = self
            .target
            .parse::<Uri>()
            .map_err(|err| Status::invalid_argument("parse uri failed").with_cause(err))?;

        let addrs = tokio::net::lookup_host(uri.authority().unwrap().as_str()).await?;

        for addr in addrs {
            let uri = Uri::builder()
                .authority(addr.to_string())
                .build()
                .map_err(|err| Status::invalid_argument("invalid addr").with_cause(err))?;
            resolved_addrs.push(ResolvedAddr::new(uri, addr));
        }

        Ok(resolved_addrs)
    }
}

#[async_trait::async_trait]
impl Resolver for DnsResolver {
    async fn resolve(&self) -> Result<Vec<ResolvedAddr>, Status> {
        self.resolve_addr().await
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
