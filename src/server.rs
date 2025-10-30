use crate::message::{
    parse_connect_response, parse_data_frame, parse_disconnect_frame, parse_message,
    ConnectMessage, Message,
};
use crate::message::{AuthMessage, AuthResponseMessage};
use crate::portpool::PortPool;
use crate::socket::AsyncSocketManager;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use log::{debug, info, warn};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex, Notify, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage, WebSocketStream};
use uuid::Uuid;

/// Default buffer size for data transfer
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default channel timeout
pub const DEFAULT_CHANNEL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connect timeout
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

struct SocksTask {
    stop: Arc<Notify>,
    is_running: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl SocksTask {
    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    async fn stop(self) {
        self.stop.notify_waiters();
        let _ = self.handle.await;
    }
}

struct ListenerTask {
    stop: Arc<Notify>,
    is_running: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ListenerTask {
    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    async fn stop(self) {
        self.stop.notify_waiters();
        let _ = self.handle.await;
    }
}

/// Snapshot of high-level server status metrics.
#[derive(Clone)]
pub struct StatusSnapshot {
    pub client_count: usize,
    pub forward_token_count: usize,
    pub reverse_token_count: usize,
    pub connector_token_count: usize,
}

/// Snapshot of a token entry used for API responses.
#[derive(Clone)]
pub struct TokenSnapshot {
    pub token: String,
    pub port: Option<u16>,
    pub client_count: usize,
}

/// Server options for LinkSocksServer
#[derive(Clone)]
pub struct ServerOption {
    /// WebSocket server listen address
    pub ws_host: String,

    /// WebSocket server listen port
    pub ws_port: u16,

    /// SOCKS server listen address
    pub socks_host: String,

    /// Port pool for SOCKS servers
    pub port_pool: PortPool,

    /// Whether to wait for client before starting SOCKS server
    pub socks_wait_client: bool,

    /// Buffer size for data transfer
    pub buffer_size: usize,

    /// API key for HTTP API
    pub api_key: Option<String>,

    /// Channel timeout
    pub channel_timeout: Duration,

    /// Connect timeout
    pub connect_timeout: Duration,

    /// Whether to use fast open
    pub fast_open: bool,

    /// Upstream SOCKS5 proxy
    pub upstream_proxy: Option<String>,

    /// Upstream SOCKS5 proxy username
    pub upstream_username: Option<String>,

    /// Upstream SOCKS5 proxy password
    pub upstream_password: Option<String>,
}

impl Default for ServerOption {
    fn default() -> Self {
        ServerOption {
            ws_host: "0.0.0.0".to_string(),
            ws_port: 8765,
            socks_host: "127.0.0.1".to_string(),
            port_pool: PortPool::new_default(),
            socks_wait_client: true,
            buffer_size: DEFAULT_BUFFER_SIZE,
            api_key: None,
            channel_timeout: DEFAULT_CHANNEL_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            fast_open: false,
            upstream_proxy: None,
            upstream_username: None,
            upstream_password: None,
        }
    }
}

impl ServerOption {
    /// Set the WebSocket host
    pub fn with_ws_host(mut self, host: String) -> Self {
        self.ws_host = host;
        self
    }

    /// Set the WebSocket port
    pub fn with_ws_port(mut self, port: u16) -> Self {
        self.ws_port = port;
        self
    }

    /// Set the SOCKS host
    pub fn with_socks_host(mut self, host: String) -> Self {
        self.socks_host = host;
        self
    }

    /// Set the port pool
    pub fn with_port_pool(mut self, pool: PortPool) -> Self {
        self.port_pool = pool;
        self
    }

    /// Set whether to wait for client before starting SOCKS server
    pub fn with_socks_wait_client(mut self, wait: bool) -> Self {
        self.socks_wait_client = wait;
        self
    }

    /// Set the buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set the API key
    pub fn with_api(mut self, key: String) -> Self {
        self.api_key = Some(key);
        self
    }

    /// Set the channel timeout
    pub fn with_channel_timeout(mut self, timeout: Duration) -> Self {
        self.channel_timeout = timeout;
        self
    }

    /// Set the connect timeout
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set whether to use fast open
    pub fn with_fast_open(mut self, fast_open: bool) -> Self {
        self.fast_open = fast_open;
        self
    }

    /// Set the upstream SOCKS5 proxy
    pub fn with_upstream_proxy(mut self, proxy: String) -> Self {
        self.upstream_proxy = Some(proxy);
        self
    }

    /// Set the upstream SOCKS5 proxy authentication
    pub fn with_upstream_auth(mut self, username: String, password: String) -> Self {
        self.upstream_username = Some(username);
        self.upstream_password = Some(password);
        self
    }
}

/// Options for reverse token
#[derive(Default)]
pub struct ReverseTokenOptions {
    /// Token to use (auto-generated if None)
    pub token: Option<String>,

    /// Port to use (allocated from pool if None)
    pub port: Option<u16>,

    /// SOCKS5 username for authentication
    pub username: Option<String>,

    /// SOCKS5 password for authentication
    pub password: Option<String>,

    /// Whether to allow managing connectors
    pub allow_manage_connector: bool,
}

/// Result of adding a reverse token
pub struct ReverseTokenResult {
    /// Token that was created or used
    pub token: String,

    /// Port assigned to the token
    pub port: Option<u16>,
}

/// Client information
#[allow(dead_code)]
#[derive(Clone)]
struct ClientInfo {
    /// Client ID
    _id: Uuid,

    /// Client WebSocket sender (outbound)
    sender: mpsc::Sender<WsMessage>,
}

/// WebSocket connection
#[allow(dead_code)]
struct WsConn {
    /// Client ID
    _id: Uuid,

    /// Client IP address
    _client_ip: String,

    /// WebSocket sender
    _sender: mpsc::Sender<WsMessage>,
}

/// Waiting socket
#[allow(dead_code)]
struct WaitingSocket {
    /// TCP listener
    _listener: TcpListener,

    /// Cancel timer
    _cancel_timer: Option<tokio::time::Instant>,
}

/// Connector cache
#[allow(dead_code)]
struct ConnectorCache {
    /// Maps channel_id to reverse client WebSocket connection
    _channel_id_to_client: HashMap<Uuid, mpsc::Sender<WsMessage>>,

    /// Maps channel_id to connector WebSocket connection
    _channel_id_to_connector: HashMap<Uuid, mpsc::Sender<WsMessage>>,

    /// Maps token to list of channel_ids
    _token_cache: HashMap<String, Vec<Uuid>>,
}

impl ConnectorCache {
    /// Create a new connector cache
    fn new() -> Self {
        ConnectorCache {
            _channel_id_to_client: HashMap::new(),
            _channel_id_to_connector: HashMap::new(),
            _token_cache: HashMap::new(),
        }
    }
}

/// LinkSocksServer represents a SOCKS5 over WebSocket protocol server
pub struct LinkSocksServer {
    /// Server options
    options: ServerOption,

    /// Ready notification
    ready: Arc<Notify>,

    /// WebSocket server address
    ws_addr: SocketAddr,

    /// SOCKS server address
    socks_host: String,

    /// Port pool
    port_pool: PortPool,

    /// Whether to wait for client before starting SOCKS server
    socks_wait_client: bool,

    /// Client connections
    clients: Arc<RwLock<HashMap<Uuid, WsConn>>>,

    /// Forward tokens
    forward_tokens: Arc<RwLock<HashSet<String>>>,

    /// Reverse tokens to ports
    tokens: Arc<RwLock<HashMap<String, u16>>>,

    /// Token clients
    token_clients: Arc<RwLock<HashMap<String, Vec<ClientInfo>>>>,

    /// Token indexes for load balancing
    token_indexes: Arc<RwLock<HashMap<String, usize>>>,

    /// Token options
    token_options: Arc<RwLock<HashMap<String, ReverseTokenOptions>>>,

    /// Connector tokens
    connector_tokens: Arc<RwLock<HashMap<String, String>>>,

    /// Internal tokens
    internal_tokens: Arc<RwLock<HashMap<String, Vec<String>>>>,

    /// SHA256 token map
    sha256_token_map: Arc<RwLock<HashMap<String, String>>>,

    /// Connector cache
    conn_cache: Arc<AsyncMutex<ConnectorCache>>,

    /// Active SOCKS servers
    socks_tasks: Arc<RwLock<HashMap<u16, SocksTask>>>,

    /// Pending connect responses per channel
    pending_connect: Arc<AsyncMutex<HashMap<Uuid, oneshot::Sender<Result<(), String>>>>>,

    /// Channel to TCP stream mapping for data relay
    channel_streams: Arc<AsyncMutex<HashMap<Uuid, Arc<tokio::sync::Mutex<OwnedWriteHalf>>>>>,

    /// Waiting sockets
    waiting_sockets: Arc<RwLock<HashMap<u16, WaitingSocket>>>,

    /// Socket manager
    socket_manager: Arc<AsyncSocketManager>,

    /// API key
    api_key: Option<String>,

    /// Shutdown notification
    shutdown: Arc<Notify>,

    /// WebSocket listener task
    ws_task: Arc<AsyncMutex<Option<ListenerTask>>>,
}

impl LinkSocksServer {
    /// Create a new LinkSocksServer
    pub fn new(options: ServerOption) -> Self {
        let ws_addr = format!("{}:{}", options.ws_host, options.ws_port)
            .parse()
            .expect("Invalid WebSocket address");

        LinkSocksServer {
            options: options.clone(),
            ready: Arc::new(Notify::new()),
            ws_addr,
            socks_host: options.socks_host.clone(),
            port_pool: options.port_pool.clone(),
            socks_wait_client: options.socks_wait_client,
            clients: Arc::new(RwLock::new(HashMap::new())),
            forward_tokens: Arc::new(RwLock::new(HashSet::new())),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            token_clients: Arc::new(RwLock::new(HashMap::new())),
            token_indexes: Arc::new(RwLock::new(HashMap::new())),
            token_options: Arc::new(RwLock::new(HashMap::new())),
            connector_tokens: Arc::new(RwLock::new(HashMap::new())),
            internal_tokens: Arc::new(RwLock::new(HashMap::new())),
            sha256_token_map: Arc::new(RwLock::new(HashMap::new())),
            conn_cache: Arc::new(AsyncMutex::new(ConnectorCache::new())),
            socks_tasks: Arc::new(RwLock::new(HashMap::new())),
            waiting_sockets: Arc::new(RwLock::new(HashMap::new())),
            socket_manager: Arc::new(AsyncSocketManager::new(&options.socks_host)),
            api_key: options.api_key.clone(),
            shutdown: Arc::new(Notify::new()),
            ws_task: Arc::new(AsyncMutex::new(None)),
            pending_connect: Arc::new(AsyncMutex::new(HashMap::new())),
            channel_streams: Arc::new(AsyncMutex::new(HashMap::new())),
        }
    }

    /// Generate a random token
    fn generate_random_token(length: usize) -> String {
        let mut rng = rand::thread_rng();
        let mut bytes = vec![0u8; length / 2];
        rng.fill(&mut bytes[..]);
        hex::encode(&bytes)
    }

    /// Check if a token exists
    async fn token_exists(&self, token: &str) -> bool {
        if self.forward_tokens.read().await.contains(token) {
            return true;
        }

        if self.tokens.read().await.contains_key(token) {
            return true;
        }

        if self.connector_tokens.read().await.contains_key(token) {
            return true;
        }

        false
    }

    /// Add a reverse token
    pub async fn add_reverse_token(
        &self,
        opts: ReverseTokenOptions,
    ) -> Result<ReverseTokenResult, String> {
        if let Some(ref token) = opts.token {
            if self.token_exists(token).await {
                return Err("Token already exists".to_string());
            }
        }

        let token = match opts.token {
            Some(ref t) => t.clone(),
            None => Self::generate_random_token(16),
        };

        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), token.clone());

        if opts.allow_manage_connector {
            self.tokens.write().await.insert(token.clone(), 0);
            self.token_options.write().await.insert(token.clone(), opts);
            info!("New autonomy reverse token added");
            return Ok(ReverseTokenResult { token, port: None });
        }

        if let Some(&port) = self.tokens.read().await.get(&token) {
            return Ok(ReverseTokenResult {
                token,
                port: Some(port),
            });
        }

        let assigned_port = self.port_pool.get(opts.port);
        if assigned_port == 0 {
            return Err(format!("Cannot allocate port: {:?}", opts.port));
        }

        self.tokens
            .write()
            .await
            .insert(token.clone(), assigned_port);
        self.token_options.write().await.insert(token.clone(), opts);

        if self.socks_wait_client {
            debug!(
                "Deferring SOCKS listener startup for token {} until reverse client signals readiness",
                token
            );
        }

        if !self.socks_wait_client {
            if let Err(err) = self.run_socks_server(token.clone(), assigned_port).await {
                self.tokens.write().await.remove(&token);
                self.token_options.write().await.remove(&token);
                self.port_pool.put(assigned_port);
                return Err(err);
            }
        }

        info!("New reverse proxy token added, port: {}", assigned_port);
        debug!("SHA256 for the token: {}", sha256_token.clone());

        Ok(ReverseTokenResult {
            token,
            port: Some(assigned_port),
        })
    }

    /// Add a forward token
    pub async fn add_forward_token(&self, token: Option<String>) -> Result<String, String> {
        if let Some(ref t) = token {
            if self.token_exists(t).await {
                return Err("Token already exists".to_string());
            }
        }

        let token = match token {
            Some(t) => t,
            None => Self::generate_random_token(16),
        };

        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), token.clone());

        self.forward_tokens.write().await.insert(token.clone());

        info!("New forward proxy token added");
        debug!("SHA256 for the token: {}", sha256_token.clone());

        self.ready.notify_waiters();
        Ok(token)
    }

    /// Add a connector token
    pub async fn add_connector_token(
        &self,
        connector_token: Option<String>,
        reverse_token: &str,
    ) -> Result<String, String> {
        if let Some(ref token) = connector_token {
            if self.token_exists(token).await {
                return Err("Connector token already exists".to_string());
            }
        }

        let connector_token = match connector_token {
            Some(t) => t,
            None => Self::generate_random_token(16),
        };

        if !self.tokens.read().await.contains_key(reverse_token) {
            return Err("Reverse token does not exist".to_string());
        }

        let mut hasher = Sha256::new();
        hasher.update(connector_token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), connector_token.clone());

        self.connector_tokens
            .write()
            .await
            .insert(connector_token.clone(), reverse_token.to_string());

        info!("New connector token added");

        self.ready.notify_waiters();
        Ok(connector_token)
    }

    /// Remove a token
    pub async fn remove_token(&self, token: &str) -> bool {
        let mut removed = false;
        let mut reverse_port: Option<u16> = None;

        {
            let mut tokens_guard = self.tokens.write().await;
            if let Some(port) = tokens_guard.remove(token) {
                reverse_port = Some(port);
                removed = true;
            }
        }

        if let Some(port) = reverse_port {
            self.token_options.write().await.remove(token);
            self.token_clients.write().await.remove(token);
            self.token_indexes.write().await.remove(token);
            self.port_pool.put(port);
            self.stop_socks_task(port).await;
        }

        if self.forward_tokens.write().await.remove(token) {
            removed = true;
        }

        {
            let mut internal_tokens = self.internal_tokens.write().await;
            internal_tokens.retain(|_, tokens| {
                tokens.retain(|t| t != token);
                !tokens.is_empty()
            });
        }

        let mut connector_removed = HashSet::new();
        {
            let mut connector_guard = self.connector_tokens.write().await;
            connector_guard.retain(|connector, reverse| {
                if reverse == token || connector == token {
                    connector_removed.insert(connector.clone());
                    false
                } else {
                    true
                }
            });
        }
        if !connector_removed.is_empty() {
            removed = true;
        }

        {
            let mut sha_guard = self.sha256_token_map.write().await;
            sha_guard.retain(|_, value| value != token && !connector_removed.contains(value));
        }

        removed
    }

    /// Remove a connector token
    pub async fn remove_connector_token(&self, token: &str) -> bool {
        let removed = self.connector_tokens.write().await.remove(token).is_some();
        if removed {
            self.sha256_token_map
                .write()
                .await
                .retain(|_, value| value != token);
        }
        removed
    }

    /// Start the server (idempotent)
    pub async fn serve(&self) -> Result<(), String> {
        {
            let task_guard = self.ws_task.lock().await;
            if let Some(task) = task_guard.as_ref() {
                if task.is_running() {
                    return Ok(());
                }
            }
        }

        let listener = TcpListener::bind(self.ws_addr).await.map_err(|e| {
            format!(
                "Failed to bind WebSocket listener on {}: {}",
                self.ws_addr, e
            )
        })?;

        let stop = Arc::new(Notify::new());
        let is_running = Arc::new(AtomicBool::new(true));
        let stop_clone = stop.clone();
        let running_clone = is_running.clone();
        let address = self.ws_addr;
        let server = self.clone();

        let handle = tokio::spawn(async move {
            let listener = listener;
            let server = server;
            loop {
                select! {
                    _ = stop_clone.notified() => {
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, addr)) => {
                                debug!("Accepted WebSocket connection from {}", addr);
                                let session_server = server.clone();
                                tokio::spawn(async move {
                                    if let Err(err) = session_server.handle_ws_connection(stream, addr).await {
                                        warn!("WebSocket session error from {}: {}", addr, err);
                                    }
                                });
                            }
                            Err(err) => {
                                warn!("WebSocket accept error on {}: {}", address, err);
                                break;
                            }
                        }
                    }
                }
            }
            running_clone.store(false, Ordering::SeqCst);
        });

        let mut task_guard = self.ws_task.lock().await;
        let previous = task_guard.take();
        *task_guard = Some(ListenerTask {
            stop,
            is_running,
            handle,
        });
        drop(task_guard);

        if let Some(task) = previous {
            task.stop().await;
        }

        info!("WebSocket server listening on {}", self.ws_addr);
        self.ready.notify_waiters();
        Ok(())
    }

    /// Wait for the server to be ready
    pub async fn wait_ready(&self) -> Result<(), String> {
        self.serve().await?;
        if self.is_ready().await {
            return Ok(());
        }
        self.ready.notified().await;
        Ok(())
    }

    async fn handle_ws_connection(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<(), String> {
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("Failed WebSocket handshake with {}: {}", addr, e))?;

        debug!("WebSocket handshake completed for {}", addr);

        // Relay for forward mode (server-side network dialer)
        let relay = crate::relay::Relay::new_default();

        let (ws_sender_init, mut ws_receiver) = ws_stream.split();
        let mut ws_sender_opt = Some(ws_sender_init);
        let mut authenticated = false;
        // Outbound writer channel after auth
        let mut outbound_tx_opt: Option<mpsc::Sender<WsMessage>> = None;

        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) => {
                    let frame_label = if msg.is_text() {
                        "text"
                    } else if msg.is_binary() {
                        "binary"
                    } else if msg.is_ping() {
                        "ping"
                    } else if msg.is_pong() {
                        "pong"
                    } else if msg.is_close() {
                        "close"
                    } else {
                        "other"
                    };
                    debug!(
                        "WebSocket frame from {} classified as {}",
                        addr, frame_label
                    );

                    match msg {
                        WsMessage::Ping(payload) => {
                            if let Some(tx) = outbound_tx_opt.as_ref() {
                                let _ = tx.send(WsMessage::Pong(payload)).await;
                            } else {
                                if let Some(s) = ws_sender_opt.as_mut() {
                                    s.send(WsMessage::Pong(payload)).await.map_err(|e| {
                                        format!("Failed to send pong to {}: {}", addr, e)
                                    })?;
                                }
                            }
                        }
                        WsMessage::Pong(_) => {
                            // Ignore pong frames
                        }
                        WsMessage::Binary(payload) => {
                            if !authenticated {
                                match Self::parse_binary_auth(&payload) {
                                    Ok(auth_msg) => match self
                                        .process_auth_message(
                                            ws_sender_opt.as_mut().unwrap(),
                                            addr,
                                            auth_msg.clone(),
                                        )
                                        .await
                                    {
                                    Ok(()) => {
                                            // Create outbound channel and writer task for this WS connection
                                            let (tx, mut rx) = mpsc::channel::<WsMessage>(200);
                                            let mut sink = ws_sender_opt.take().unwrap();
                                            tokio::spawn(async move {
                                                while let Some(msg) = rx.recv().await {
                                                    if let Err(e) = sink.send(msg).await {
                                                        warn!("WS writer error: {}", e);
                                                        break;
                                                    }
                                                }
                                            });
                                            outbound_tx_opt = Some(tx.clone());

                                            // If reverse client, register for load balancing
                                            if auth_msg.reverse {
                                                let token = auth_msg.token.clone();
                                                let info = ClientInfo { _id: Uuid::new_v4(), sender: tx };
                                                let mut guard = self.token_clients.write().await;
                                                guard.entry(token).or_default().push(info);
                                            }
                                            authenticated = true;
                                            continue;
                                        }
                                        Err(err) => {
                                            debug!(
                                                "Binary authentication flow terminated for {}: {}",
                                                addr, err
                                            );
                                            break;
                                        }
                                    },
                                    Err(err) => {
                                        let error_msg = err;
                                        warn!(
                                            "Binary authentication from {} rejected: {}",
                                            addr, error_msg
                                        );
                                        Self::send_auth_response(
                                            ws_sender_opt.as_mut().unwrap(),
                                            addr,
                                            AuthResponseMessage::failure(error_msg.clone()),
                                        )
                                        .await?;
                                        break;
                                    }
                                }
                            } else {
                                // Dispatch inbound messages from authenticated client
                                match parse_message(&payload) {
                                    Ok(msg) => {
                                        match msg.message_type() {
                                            "connect" => {
                                                // Forward mode: server dials out
                                                if let Ok(conn) = crate::message::parse_connect_frame(&payload) {
                                                    if let Some(tx) = outbound_tx_opt.as_ref() {
                                                        let _ = relay.handle_network_connection(tx.clone(), conn).await;
                                                    }
                                                }
                                            }
                                            ,"connect_response" => {
                                                // Extract channel_id and success
                                                // Reparse using helper in message module
                                                if let Ok(resp) = parse_connect_response(&payload) {
                                                    let mut pending =
                                                        self.pending_connect.lock().await;
                                                    if let Some(tx) =
                                                        pending.remove(&resp.channel_id)
                                                    {
                                                        let _ = tx.send(if resp.success {
                                                            Ok(())
                                                        } else {
                                                            Err(resp.error.unwrap_or_else(|| {
                                                                "connect failed".to_string()
                                                            }))
                                                        });
                                                    }
                                                }
                                            }
                                            "data" => {
                                                if let Ok(data) = parse_data_frame(&payload) {
                                                    let map = self.channel_streams.lock().await;
                                                    if let Some(writer) = map.get(&data.channel_id)
                                                    {
                                                        let mut s = writer.lock().await;
                                                        let _ = s.write_all(&data.data).await;
                                                    }
                                                }
                                            }
                                            "disconnect" => {
                                                if let Ok(ch) = parse_disconnect_frame(&payload) {
                                                    let mut map = self.channel_streams.lock().await;
                                                    map.remove(&ch);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                        WsMessage::Close(frame) => {
                            if let Some(tx) = outbound_tx_opt.as_ref() {
                                let _ = tx.send(WsMessage::Close(frame)).await;
                            } else if let Some(s) = ws_sender_opt.as_mut() {
                                let _ = s.send(WsMessage::Close(frame)).await;
                            }
                            break;
                        }
                        WsMessage::Text(text) => {
                            debug!("Received text message from {}: {}", addr, text);

                            if !authenticated {
                                match serde_json::from_str::<AuthMessage>(&text) {
                                    Ok(auth_msg) => {
                                        match self
                                            .process_auth_message(
                                                ws_sender_opt.as_mut().unwrap(),
                                                addr,
                                                auth_msg,
                                            )
                                            .await
                                        {
                                            Ok(()) => {
                                                authenticated = true;
                                                continue;
                                            }
                                            Err(err) => {
                                                debug!(
                                                    "Text authentication flow terminated for {}: {}",
                                                    addr, err
                                                );
                                                break;
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        debug!(
                                            "Failed to parse auth message from {}: {}",
                                            addr, err
                                        );
                                    }
                                }
                            }

                            debug!(
                                "Received unsupported message from {}: Text(\"{}\")",
                                addr, text
                            );
                        }
                        other => {
                            debug!("Received unsupported message from {}: {:?}", addr, other);
                        }
                    }
                }
                Err(e) => {
                    return Err(format!("WebSocket receive error from {}: {}", addr, e));
                }
            }
        }

        debug!("WebSocket connection closed for {}", addr);
        Ok(())
    }

    fn parse_binary_auth(payload: &[u8]) -> Result<AuthMessage, String> {
        use crate::message::parse_message;

        match parse_message(payload) {
            Ok(_msg) => {
                // We need to safely check if this is an AuthMessage
                // Since we can't downcast trait objects directly, we'll re-parse it
                if payload.len() >= 2 && payload[1] == crate::message::BINARY_TYPE_AUTH {
                    // Re-parse directly as AuthMessage
                    if payload.len() < 2 {
                        return Err("Message too short".to_string());
                    }

                    let payload = &payload[2..];
                    if payload.len() < 1 {
                        return Err("Invalid auth message".to_string());
                    }

                    let token_len = payload[0] as usize;
                    if payload.len() < 1 + token_len + 1 + 16 {
                        return Err("Invalid auth message length".to_string());
                    }

                    let token = String::from_utf8(payload[1..1 + token_len].to_vec())
                        .map_err(|e| format!("Invalid UTF-8 in token: {}", e))?;
                    let reverse = payload[1 + token_len] != 0;

                    let mut uuid_bytes = [0u8; 16];
                    uuid_bytes.copy_from_slice(&payload[1 + token_len + 1..1 + token_len + 1 + 16]);
                    let instance = Uuid::from_bytes(uuid_bytes);

                    Ok(AuthMessage {
                        token,
                        reverse,
                        instance,
                    })
                } else {
                    Err("Expected auth message".to_string())
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn process_auth_message(
        &self,
        ws_sender: &mut SplitSink<WebSocketStream<TcpStream>, WsMessage>,
        addr: SocketAddr,
        auth_msg: AuthMessage,
    ) -> Result<(), String> {
        let token = auth_msg.token.clone();

        if token.is_empty() {
            let error = "token is required".to_string();
            Self::send_auth_response(ws_sender, addr, AuthResponseMessage::failure(error.clone()))
                .await?;
            warn!("Authentication from {} rejected: empty token", addr);
            return Err(error);
        }

        if auth_msg.reverse {
            let port = {
                let guard = self.tokens.read().await;
                guard.get(&token).copied()
            };

            match port {
                Some(port) => {
                    if let Err(err) = self.ensure_reverse_socks_running(&token, port).await {
                        Self::send_auth_response(
                            ws_sender,
                            addr,
                            AuthResponseMessage::failure(err.clone()),
                        )
                        .await?;
                        warn!("Reverse authentication from {} failed: {}", addr, err);
                        return Err(err);
                    }

                    Self::send_auth_response(ws_sender, addr, AuthResponseMessage::success())
                        .await?;
                    info!(
                        "Reverse client {} authenticated for token {} on port {}",
                        addr, token, port
                    );
                    self.ready.notify_waiters();
                    Ok(())
                }
                None => {
                    let error = "invalid reverse token".to_string();
                    Self::send_auth_response(
                        ws_sender,
                        addr,
                        AuthResponseMessage::failure(error.clone()),
                    )
                    .await?;
                    warn!(
                        "Reverse authentication from {} failed: invalid token {}",
                        addr, token
                    );
                    Err(error)
                }
            }
        } else {
            let valid = {
                let guard = self.forward_tokens.read().await;
                guard.contains(&token)
            };

            if !valid {
                let error = "invalid forward token".to_string();
                Self::send_auth_response(
                    ws_sender,
                    addr,
                    AuthResponseMessage::failure(error.clone()),
                )
                .await?;
                warn!(
                    "Forward authentication from {} failed: invalid token {}",
                    addr, token
                );
                return Err(error);
            }

            Self::send_auth_response(ws_sender, addr, AuthResponseMessage::success()).await?;
            info!("Forward client {} authenticated for token {}", addr, token);
            self.ready.notify_waiters();
            Ok(())
        }
    }

    async fn send_auth_response(
        ws_sender: &mut SplitSink<WebSocketStream<TcpStream>, WsMessage>,
        addr: SocketAddr,
        response: AuthResponseMessage,
    ) -> Result<(), String> {
        use crate::message::Message;

        let frame = response
            .pack()
            .map_err(|e| format!("Failed to pack auth response: {}", e))?;

        ws_sender
            .send(WsMessage::Binary(frame))
            .await
            .map_err(|e| format!("Failed to send auth response to {}: {}", addr, e))
    }

    /// Handle a single SOCKS5 connection (minimal CONNECT support)
    async fn handle_socks_connection(
        &self,
        token: String,
        mut stream: TcpStream,
    ) -> Result<(), String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // Method negotiation
        let mut buf = [0u8; 2];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        if buf[0] != 0x05 {
            return Err("Invalid SOCKS version".to_string());
        }
        let n_methods = buf[1] as usize;
        let mut methods = vec![0u8; n_methods];
        stream
            .read_exact(&mut methods)
            .await
            .map_err(|e| e.to_string())?;
        // Reply: no auth
        stream
            .write_all(&[0x05, 0x00])
            .await
            .map_err(|e| e.to_string())?;

        // Request
        let mut hdr = [0u8; 4];
        stream
            .read_exact(&mut hdr)
            .await
            .map_err(|e| e.to_string())?;
        if hdr[0] != 0x05 || hdr[1] != 0x01 {
            return Err("Only CONNECT supported".to_string());
        }
        let atyp = hdr[3];
        // Parse address
        let address = match atyp {
            0x01 => {
                // IPv4
                let mut a = [0u8; 4];
                stream.read_exact(&mut a).await.map_err(|e| e.to_string())?;
                std::net::Ipv4Addr::from(a).to_string()
            }
            0x03 => {
                // Domain
                let mut len = [0u8; 1];
                stream
                    .read_exact(&mut len)
                    .await
                    .map_err(|e| e.to_string())?;
                let l = len[0] as usize;
                let mut name = vec![0u8; l];
                stream
                    .read_exact(&mut name)
                    .await
                    .map_err(|e| e.to_string())?;
                String::from_utf8(name).map_err(|e| e.to_string())?
            }
            0x04 => {
                // IPv6
                let mut a = [0u8; 16];
                stream.read_exact(&mut a).await.map_err(|e| e.to_string())?;
                std::net::Ipv6Addr::from(a).to_string()
            }
            _ => return Err("Invalid ATYP".to_string()),
        };
        let mut pbuf = [0u8; 2];
        stream
            .read_exact(&mut pbuf)
            .await
            .map_err(|e| e.to_string())?;
        let port = u16::from_be_bytes(pbuf);

        // Load-balance pick a reverse client sender
        let sender = {
            let mut idx_guard = self.token_indexes.write().await;
            let idx = idx_guard.entry(token.clone()).or_insert(0);
            let list = self.token_clients.read().await;
            let clients_opt = list.get(&token);
            let clients: Vec<ClientInfo> = clients_opt
                .map(|v| v.iter().cloned().collect())
                .unwrap_or_default();
            if clients.is_empty() {
                return Err("No reverse clients available".to_string());
            }
            let chosen = &clients[*idx % clients.len()];
            *idx = (*idx + 1) % clients.len();
            chosen.sender.clone()
        };

        // Create channel id and send ConnectMessage
        let channel_id = Uuid::new_v4();
        let connect = ConnectMessage {
            protocol: "tcp".to_string(),
            channel_id,
            address: address.clone(),
            port,
        };
        let frame = connect.pack().map_err(|e| e.to_string())?;
        sender
            .send(WsMessage::Binary(frame))
            .await
            .map_err(|e| e.to_string())?;

        // Await ConnectResponse via oneshot
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_connect.lock().await;
            pending.insert(channel_id, tx);
        }
        let ok = tokio::time::timeout(self.options.connect_timeout, rx)
            .await
            .map_err(|_| "Connect response timeout".to_string())?
            .map_err(|_| "Connect response channel closed".to_string())?;

        if let Err(err) = ok {
            // Reply failure
            let reply = vec![0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
            stream.write_all(&reply).await.map_err(|e| e.to_string())?;
            return Err(err);
        }
        // Reply success
        let reply = vec![0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        stream.write_all(&reply).await.map_err(|e| e.to_string())?;

        // Register stream and start WS<->TCP handling
        {
            let (mut ri, wi) = stream.into_split();
            let mut map = self.channel_streams.lock().await;
            map.insert(channel_id, Arc::new(tokio::sync::Mutex::new(wi)));

            // TCP->WS forwarder within scope of ri
            let sender_clone = sender.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buf = vec![0u8; 8192];
                loop {
                    match ri.read(&mut buf).await {
                        Ok(0) => {
                            break;
                        }
                        Ok(n) => {
                            let dm =
                                crate::message::DataMessage::new(channel_id, buf[..n].to_vec());
                            if let Ok(f) = dm.pack() {
                                if sender_clone.send(WsMessage::Binary(f)).await.is_err() {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _ = sender_clone
                    .send(WsMessage::Binary(
                        crate::message::DisconnectMessage::new(channel_id)
                            .pack()
                            .unwrap_or_default(),
                    ))
                    .await;
            });
        }

        Ok(())
    }

    /// Ensure the reverse SOCKS listener for this token is running.
    ///
    /// When `socks_wait_client` is enabled we lazily start the listener only after
    /// the first authenticated reverse client arrives. Subsequent authentications
    /// simply reuse the existing running listener.
    async fn ensure_reverse_socks_running(&self, token: &str, port: u16) -> Result<(), String> {
        let is_running = {
            let guard = self.socks_tasks.read().await;
            guard
                .get(&port)
                .map(|task| task.is_running())
                .unwrap_or(false)
        };
        if is_running {
            return Ok(());
        }

        if !self.socks_wait_client {
            // Listener should already be running in non-lazy mode.
            return Ok(());
        }

        self.run_socks_server(token.to_string(), port).await
    }

    /// Run a SOCKS server
    async fn run_socks_server(&self, token: String, port: u16) -> Result<(), String> {
        {
            let mut tasks_guard = self.socks_tasks.write().await;
            let previous = match tasks_guard.entry(port) {
                Entry::Occupied(entry) => {
                    if entry.get().is_running() {
                        return Ok(());
                    }
                    Some(entry.remove())
                }
                Entry::Vacant(_) => None,
            };
            drop(tasks_guard);

            if let Some(task) = previous {
                task.stop().await;
            }
        }

        let addr = self
            .socket_manager
            .get_socket_addr(port)
            .await
            .map_err(|e| format!("Failed to allocate socket address for port {}: {}", port, e))?;

        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(err) => {
                self.socket_manager.release_socket(port).await;
                return Err(format!(
                    "Failed to bind SOCKS listener on {}: {}",
                    addr, err
                ));
            }
        };

        let stop = Arc::new(Notify::new());
        let is_running = Arc::new(AtomicBool::new(true));
        let stop_clone = stop.clone();
        let running_clone = is_running.clone();
        let server_clone = self.clone();
        let token_label = token.clone();

        let handle = tokio::spawn(async move {
            let listener = listener;
            loop {
                select! {
                    _ = stop_clone.notified() => {
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, addr)) => {
                                debug!("Accepted reverse SOCKS connection for token {} from {}", token_label, addr);
                                let server_clone2 = server_clone.clone();
                                let token_use = token_label.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = server_clone2.handle_socks_connection(token_use, stream).await {
                                        warn!("SOCKS connection error: {}", e);
                                    }
                                });
                            }
                            Err(err) => {
                                warn!("SOCKS accept error on port {}: {}", port, err);
                                break;
                            }
                        }
                    }
                }
            }
            running_clone.store(false, Ordering::SeqCst);
            server_clone.socket_manager.release_socket(port).await;
        });

        self.socks_tasks.write().await.insert(
            port,
            SocksTask {
                stop,
                is_running,
                handle,
            },
        );

        info!(
            "Reverse SOCKS listener started on {} for token {}",
            addr, token
        );
        self.ready.notify_waiters();
        Ok(())
    }

    /// Close the server
    pub async fn close(&self) {
        self.shutdown.notify_waiters();

        let ws_task = { self.ws_task.lock().await.take() };
        if let Some(task) = ws_task {
            task.stop().await;
        }

        let tasks: Vec<SocksTask> = {
            let mut guard = self.socks_tasks.write().await;
            guard.drain().map(|(_, task)| task).collect()
        };
        for task in tasks {
            task.stop().await;
        }

        self.socket_manager.close().await;
    }

    /// Get the number of connected clients
    pub async fn get_client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Check if there are any connected clients
    pub async fn has_clients(&self) -> bool {
        !self.clients.read().await.is_empty()
    }

    /// Get the number of clients connected for a given token
    pub async fn get_token_client_count(&self, token: &str) -> usize {
        if let Some(clients) = self.token_clients.read().await.get(token) {
            return clients.len();
        }

        if self.forward_tokens.read().await.contains(token) {
            return self.clients.read().await.len();
        }

        0
    }

    /// Produce a snapshot of current status metrics.
    pub async fn status_snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            client_count: self.clients.read().await.len(),
            forward_token_count: self.forward_tokens.read().await.len(),
            reverse_token_count: self.tokens.read().await.len(),
            connector_token_count: self.connector_tokens.read().await.len(),
        }
    }

    /// Produce token snapshots suitable for API responses.
    pub async fn token_snapshot(&self) -> Vec<TokenSnapshot> {
        let reverse_entries: Vec<(String, u16)> = self
            .tokens
            .read()
            .await
            .iter()
            .map(|(token, port)| (token.clone(), *port))
            .collect();

        let forward_entries: Vec<String> =
            self.forward_tokens.read().await.iter().cloned().collect();

        let mut results = Vec::with_capacity(reverse_entries.len() + forward_entries.len());
        for (token, port) in reverse_entries {
            let client_count = self.get_token_client_count(&token).await;
            results.push(TokenSnapshot {
                token,
                port: Some(port),
                client_count,
            });
        }

        for token in forward_entries {
            let client_count = self.get_token_client_count(&token).await;
            results.push(TokenSnapshot {
                token,
                port: None,
                client_count,
            });
        }

        results
    }

    async fn is_ready(&self) -> bool {
        let res = {
            let guard = self.ws_task.lock().await;
            guard
                .as_ref()
                .map(|task| task.is_running())
                .unwrap_or(false)
        };
        if res {
            return true;
        }

        let guard = self.socks_tasks.read().await;
        guard.values().any(|task| task.is_running())
    }

    async fn stop_socks_task(&self, port: u16) {
        let task = { self.socks_tasks.write().await.remove(&port) };
        if let Some(task) = task {
            task.stop().await;
        }
    }
}

impl Clone for LinkSocksServer {
    fn clone(&self) -> Self {
        LinkSocksServer {
            options: self.options.clone(),
            ready: self.ready.clone(),
            ws_addr: self.ws_addr,
            socks_host: self.socks_host.clone(),
            port_pool: self.port_pool.clone(),
            socks_wait_client: self.socks_wait_client,
            clients: self.clients.clone(),
            forward_tokens: self.forward_tokens.clone(),
            tokens: self.tokens.clone(),
            token_clients: self.token_clients.clone(),
            token_indexes: self.token_indexes.clone(),
            token_options: self.token_options.clone(),
            connector_tokens: self.connector_tokens.clone(),
            internal_tokens: self.internal_tokens.clone(),
            sha256_token_map: self.sha256_token_map.clone(),
            conn_cache: self.conn_cache.clone(),
            socks_tasks: self.socks_tasks.clone(),
            waiting_sockets: self.waiting_sockets.clone(),
            socket_manager: self.socket_manager.clone(),
            api_key: self.api_key.clone(),
            shutdown: self.shutdown.clone(),
            ws_task: self.ws_task.clone(),
            pending_connect: self.pending_connect.clone(),
            channel_streams: self.channel_streams.clone(),
        }
    }
}
