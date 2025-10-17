//! Client implementation for rusocks

use crate::message::ConnectorMessage;
use log::error;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex, Notify, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

/// Default buffer size for data transfer
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default channel timeout
pub const DEFAULT_CHANNEL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connect timeout
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Client options for LinkSocksClient
#[derive(Clone)]
pub struct ClientOption {
    /// WebSocket server URL
    pub ws_url: String,

    /// Whether to use reverse proxy
    pub reverse: bool,

    /// SOCKS server listen address
    pub socks_host: String,

    /// SOCKS server listen port
    pub socks_port: u16,

    /// SOCKS server username
    pub socks_username: Option<String>,

    /// SOCKS server password
    pub socks_password: Option<String>,

    /// Whether to wait for server before starting SOCKS server
    pub socks_wait_server: bool,

    /// Whether to reconnect on disconnect
    pub reconnect: bool,

    /// Number of threads for data transfer
    pub threads: u32,

    /// Buffer size for data transfer
    pub buffer_size: usize,

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

    /// Whether to ignore environment proxy settings
    pub no_env_proxy: bool,

    /// Custom User-Agent header for WebSocket connections
    pub user_agent: Option<String>,
}

impl Default for ClientOption {
    fn default() -> Self {
        ClientOption {
            ws_url: "ws://localhost:8765".to_string(),
            reverse: false,
            socks_host: "127.0.0.1".to_string(),
            socks_port: 9870,
            socks_username: None,
            socks_password: None,
            socks_wait_server: true,
            reconnect: true,
            threads: 1,
            buffer_size: DEFAULT_BUFFER_SIZE,
            channel_timeout: DEFAULT_CHANNEL_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            fast_open: false,
            upstream_proxy: None,
            upstream_username: None,
            upstream_password: None,
            no_env_proxy: false,
            user_agent: None,
        }
    }
}

impl ClientOption {
    /// Set the WebSocket URL
    pub fn with_ws_url(mut self, url: String) -> Self {
        self.ws_url = url;
        self
    }

    /// Set whether to use reverse proxy
    pub fn with_reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    /// Set the SOCKS host
    pub fn with_socks_host(mut self, host: String) -> Self {
        self.socks_host = host;
        self
    }

    /// Set the SOCKS port
    pub fn with_socks_port(mut self, port: u16) -> Self {
        self.socks_port = port;
        self
    }

    /// Set the SOCKS username
    pub fn with_socks_username(mut self, username: String) -> Self {
        self.socks_username = Some(username);
        self
    }

    /// Set the SOCKS password
    pub fn with_socks_password(mut self, password: String) -> Self {
        self.socks_password = Some(password);
        self
    }

    /// Set whether to wait for server before starting SOCKS server
    pub fn with_socks_wait_server(mut self, wait: bool) -> Self {
        self.socks_wait_server = wait;
        self
    }

    /// Set whether to reconnect on disconnect
    pub fn with_reconnect(mut self, reconnect: bool) -> Self {
        self.reconnect = reconnect;
        self
    }

    /// Set the number of threads for data transfer
    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = threads;
        self
    }

    /// Set the buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
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

    /// Set whether to ignore environment proxy settings
    pub fn with_no_env_proxy(mut self, no_env_proxy: bool) -> Self {
        self.no_env_proxy = no_env_proxy;
        self
    }

    /// Set the custom User-Agent header for WebSocket connections
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
}

/// Channel state
#[allow(dead_code)]
enum ChannelState {
    /// Waiting for connection
    Connecting,

    /// Connected
    Connected,

    /// Disconnected
    Disconnected,
}

/// Channel information
#[allow(dead_code)]
struct ChannelInfo {
    /// Channel state
    state: ChannelState,

    /// Channel sender
    sender: mpsc::Sender<Vec<u8>>,
}

/// LinkSocksClient represents a SOCKS5 over WebSocket protocol client
pub struct LinkSocksClient {
    /// Authentication token
    token: String,

    /// Client options
    options: ClientOption,

    /// WebSocket sender
    ws_sender: Arc<Mutex<Option<mpsc::Sender<WsMessage>>>>,

    /// Channels
    channels: Arc<RwLock<HashMap<Uuid, ChannelInfo>>>,

    /// Ready notification
    ready: Arc<Notify>,

    /// Shutdown notification
    shutdown: Arc<Notify>,

    /// SOCKS server listener
    socks_listener: Arc<Mutex<Option<TcpListener>>>,
}

impl LinkSocksClient {
    /// Create a new LinkSocksClient
    pub fn new(token: String, options: ClientOption) -> Self {
        let client = LinkSocksClient {
            token,
            options,
            ws_sender: Arc::new(Mutex::new(None)),
            channels: Arc::new(RwLock::new(HashMap::new())),
            ready: Arc::new(Notify::new()),
            shutdown: Arc::new(Notify::new()),
            socks_listener: Arc::new(Mutex::new(None)),
        };

        // Start the client
        let client_clone = client.clone();
        tokio::spawn(async move {
            if let Err(e) = client_clone.run().await {
                error!("Client error: {}", e);
            }
        });

        client
    }

    /// Run the client
    async fn run(&self) -> Result<(), String> {
        // Connect to WebSocket server
        let user_agent = self.options.user_agent.as_deref();
        let (mut handler, sender) =
            crate::conn::connect_to_websocket(&self.options.ws_url, user_agent).await?;

        // Store the sender
        let mut ws_sender = self.ws_sender.lock().await;
        *ws_sender = Some(sender);

        // Start the handler
        handler
            .start()
            .await
            .map_err(|e| format!("Failed to start WebSocket handler: {}", e))?;

        // Notify that the client is ready
        self.ready.notify_one();

        // Wait for shutdown
        self.shutdown.notified().await;

        Ok(())
    }

    /// Wait for the client to be ready
    pub async fn wait_ready(&self) -> Result<(), String> {
        // Wait for the ready notification
        self.ready.notified().await;
        Ok(())
    }

    /// Add a connector token
    pub async fn add_connector(&self, connector_token: &str) -> Result<(), String> {
        let ws_sender = self.ws_sender.lock().await;
        let sender = match ws_sender.as_ref() {
            Some(sender) => sender.clone(),
            None => return Err("Client not connected".to_string()),
        };
        drop(ws_sender);

        let message = ConnectorMessage::add(connector_token.to_string());
        let payload = serde_json::to_string(&message)
            .map_err(|e| format!("Failed to serialize connector message: {}", e))?;

        sender
            .send(WsMessage::Text(payload))
            .await
            .map_err(|e| format!("Failed to send connector message: {}", e))?;
        Ok(())
    }

    /// Close the client
    pub async fn close(&self) {
        // Notify shutdown
        self.shutdown.notify_one();

        // Close SOCKS server listener if it exists
        let mut listener = self.socks_listener.lock().await;
        if let Some(l) = listener.take() {
            drop(l);
        }
    }
}

impl Clone for LinkSocksClient {
    fn clone(&self) -> Self {
        LinkSocksClient {
            token: self.token.clone(),
            options: self.options.clone(),
            ws_sender: self.ws_sender.clone(),
            channels: self.channels.clone(),
            ready: self.ready.clone(),
            shutdown: self.shutdown.clone(),
            socks_listener: self.socks_listener.clone(),
        }
    }
}
