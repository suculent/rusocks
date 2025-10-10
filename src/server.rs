//! Server implementation for rusocks

use crate::portpool::PortPool;
use crate::socket::AsyncSocketManager;
use log::{debug, info, warn};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

/// Default buffer size for data transfer
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default channel timeout
pub const DEFAULT_CHANNEL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connect timeout
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
struct ClientInfo {
    /// Client ID
    id: Uuid,

    /// Client WebSocket sender
    sender: mpsc::Sender<WsMessage>,
}

/// WebSocket connection
struct WsConn {
    /// Client ID
    id: Uuid,

    /// Client IP address
    client_ip: String,

    /// WebSocket sender
    sender: mpsc::Sender<WsMessage>,
}

/// Waiting socket
struct WaitingSocket {
    /// TCP listener
    listener: TcpListener,

    /// Cancel timer
    cancel_timer: Option<tokio::time::Instant>,
}

/// Connector cache
struct ConnectorCache {
    /// Maps channel_id to reverse client WebSocket connection
    channel_id_to_client: HashMap<Uuid, mpsc::Sender<WsMessage>>,

    /// Maps channel_id to connector WebSocket connection
    channel_id_to_connector: HashMap<Uuid, mpsc::Sender<WsMessage>>,

    /// Maps token to list of channel_ids
    token_cache: HashMap<String, Vec<Uuid>>,
}

impl ConnectorCache {
    /// Create a new connector cache
    fn new() -> Self {
        ConnectorCache {
            channel_id_to_client: HashMap::new(),
            channel_id_to_connector: HashMap::new(),
            token_cache: HashMap::new(),
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
    socks_tasks: Arc<RwLock<HashMap<u16, Arc<Notify>>>>,

    /// Waiting sockets
    waiting_sockets: Arc<RwLock<HashMap<u16, WaitingSocket>>>,

    /// Socket manager
    socket_manager: Arc<AsyncSocketManager>,

    /// API key
    api_key: Option<String>,

    /// Shutdown notification
    shutdown: Arc<Notify>,
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
        // Check if token exists as a forward token
        if self.forward_tokens.read().await.contains(token) {
            return true;
        }

        // Check if token exists as a reverse token
        if self.tokens.read().await.contains_key(token) {
            return true;
        }

        // Check if token exists as a connector token
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
        // If token is provided, check if it already exists
        if let Some(ref token) = opts.token {
            if self.token_exists(token).await {
                return Err("Token already exists".to_string());
            }
        }

        // Generate random token if not provided
        let token = match opts.token {
            Some(ref t) => t.clone(),
            None => Self::generate_random_token(16),
        };

        // Generate SHA256 version of the token
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), token.clone());

        // For autonomy tokens, don't allocate a port
        if opts.allow_manage_connector {
            self.tokens.write().await.insert(token.clone(), 0);
            self.token_options.write().await.insert(token.clone(), opts);
            info!("New autonomy reverse token added");
            return Ok(ReverseTokenResult { token, port: None });
        }

        // Check if token already exists
        if let Some(&port) = self.tokens.read().await.get(&token) {
            return Ok(ReverseTokenResult {
                token,
                port: Some(port),
            });
        }

        // Get port from pool
        let assigned_port = self.port_pool.get(opts.port);
        if assigned_port == 0 {
            return Err(format!("Cannot allocate port: {:?}", opts.port));
        }

        // Store token information
        self.tokens
            .write()
            .await
            .insert(token.clone(), assigned_port);
        self.token_options.write().await.insert(token.clone(), opts);

        // Start SOCKS server immediately if we're not waiting for clients
        if !self.socks_wait_client {
            let notify = Arc::new(Notify::new());
            self.socks_tasks
                .write()
                .await
                .insert(assigned_port, notify.clone());

            let server = self.clone();
            let token_clone = token.clone();
            tokio::spawn(async move {
                if let Err(e) = server.run_socks_server(token_clone, assigned_port).await {
                    warn!("SOCKS server error: {}", e);
                }
            });
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
        // Check if token already exists
        if let Some(ref t) = token {
            if self.token_exists(t).await {
                return Err("Token already exists".to_string());
            }
        }

        // Generate random token if not provided
        let token = match token {
            Some(t) => t,
            None => Self::generate_random_token(16),
        };

        // Generate SHA256 version of the token
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), token.clone());

        // Store token
        self.forward_tokens.write().await.insert(token.clone());

        info!("New forward proxy token added");
        debug!("SHA256 for the token: {}", sha256_token.clone());

        Ok(token)
    }

    /// Add a connector token
    pub async fn add_connector_token(
        &self,
        connector_token: Option<String>,
        reverse_token: &str,
    ) -> Result<String, String> {
        // Check if connector token already exists
        if let Some(ref token) = connector_token {
            if self.token_exists(token).await {
                return Err("Connector token already exists".to_string());
            }
        }

        // Generate random token if not provided
        let connector_token = match connector_token {
            Some(t) => t,
            None => Self::generate_random_token(16),
        };

        // Verify reverse token exists
        if !self.tokens.read().await.contains_key(reverse_token) {
            return Err("Reverse token does not exist".to_string());
        }

        // Generate SHA256 version of the token
        let mut hasher = Sha256::new();
        hasher.update(connector_token.as_bytes());
        let sha256_token = hex::encode(hasher.finalize());
        self.sha256_token_map
            .write()
            .await
            .insert(sha256_token.clone(), connector_token.clone());

        // Store connector token mapping
        self.connector_tokens
            .write()
            .await
            .insert(connector_token.clone(), reverse_token.to_string());

        info!("New connector token added");

        Ok(connector_token)
    }

    /// Remove a token
    pub async fn remove_token(&self, _token: &str) -> bool {
        // TODO: Implement token removal
        true
    }

    /// Start the server
    pub async fn serve(&self) -> Result<(), String> {
        // TODO: Implement server
        Ok(())
    }

    /// Wait for the server to be ready
    pub async fn wait_ready(&self) -> Result<(), String> {
        // TODO: Implement wait_ready
        Ok(())
    }

    /// Run a SOCKS server
    async fn run_socks_server(&self, _token: String, _port: u16) -> Result<(), String> {
        // TODO: Implement SOCKS server
        Ok(())
    }

    /// Close the server
    pub async fn close(&self) {
        // TODO: Implement close
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
        // Check reverse proxy clients
        if let Some(clients) = self.token_clients.read().await.get(token) {
            return clients.len();
        }

        // Check forward proxy clients
        if self.forward_tokens.read().await.contains(token) {
            return self.clients.read().await.len();
        }

        0
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
        }
    }
}
