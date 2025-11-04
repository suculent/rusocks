//! Client implementation for rusocks

use crate::message::{AuthMessage, ConnectorMessage, Message};
use log::error;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex, Notify, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

/// Type aliases to simplify complex types used in channels and pending maps
type PendingConnectMap = HashMap<Uuid, oneshot::Sender<Result<(), String>>>;
type PendingConnect = Arc<tokio::sync::Mutex<PendingConnectMap>>;
type WriterHalf = Arc<tokio::sync::Mutex<OwnedWriteHalf>>;
type ChannelWritersMap = HashMap<Uuid, WriterHalf>;
type ChannelWriters = Arc<tokio::sync::Mutex<ChannelWritersMap>>;

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

    /// Pending connect (forward mode)
    pending_connect: PendingConnect,
 
    /// Channel to TCP writer mapping (forward mode)
    channel_streams: ChannelWriters,
 
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
            pending_connect: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            channel_streams: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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
        let (mut handler, sender, mut inbound_rx) =
            crate::conn::connect_to_websocket(&self.options.ws_url, user_agent).await?;
        // Store the sender
        let mut ws_sender = self.ws_sender.lock().await;
        *ws_sender = Some(sender);
        let auth_sender = ws_sender.as_ref().cloned();
        drop(ws_sender);

        // Start the handler
        handler
            .start()
            .await
            .map_err(|e| format!("Failed to start WebSocket handler: {}", e))?;

        if let Some(sender) = auth_sender {
            let auth_message = AuthMessage::new(self.token.clone(), self.options.reverse);
            let payload = auth_message
                .pack()
                .map_err(|e| format!("Failed to pack auth message: {}", e))?;
            sender
                .send(WsMessage::Binary(payload))
                .await
                .map_err(|e| format!("Failed to send auth message: {}", e))?;

            // Start periodic WebSocket pings (keepalive)
            let ping_sender = sender.clone();
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                use tokio::time::{interval, Duration};
                let mut ticker = interval(Duration::from_secs(15));
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            if ping_sender.send(WsMessage::Ping(Vec::new())).await.is_err() {
                                break;
                            }
                        }
                        _ = shutdown.notified() => {
                            break;
                        }
                    }
                }
            });

            // Inbound dispatcher (both modes)
            let pending = self.pending_connect.clone();
            let writers = self.channel_streams.clone();
            tokio::spawn(async move {
                use crate::message::{
                    parse_connect_response, parse_data_frame, parse_disconnect_frame, parse_message,
                };
                use log::debug;
                while let Some(msg) = inbound_rx.recv().await {
                    if let WsMessage::Binary(payload) = msg {
                        match parse_message(&payload) {
                            Ok(m) => match m.message_type() {
                                "connect_response" => {
                                    if let Ok(resp) = parse_connect_response(&payload) {
                                        log::debug!(
                                            "Inbound connect_response: channel={} success={} err={:?}",
                                            resp.channel_id, resp.success, resp.error
                                        );
                                        let mut map = pending.lock().await;
                                        if let Some(tx) = map.remove(&resp.channel_id) {
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
                                    if let Ok(dm) = parse_data_frame(&payload) {
                                        log::debug!(
                                            "WS->TCP data: channel={} bytes={}",
                                            dm.channel_id,
                                            dm.data.len()
                                        );
                                        let map = writers.lock().await;
                                        if let Some(w) = map.get(&dm.channel_id) {
                                            let mut wh = w.lock().await;
                                            let _ = wh.write_all(&dm.data).await;
                                        }
                                    }
                                }
                                "disconnect" => {
                                    if let Ok(ch) = parse_disconnect_frame(&payload) {
                                        log::debug!("WS disconnect for channel {}", ch);
                                        let mut map = writers.lock().await;
                                        map.remove(&ch);
                                    }
                                }
                                other => debug!("Unsupported inbound type: {}", other),
                            },
                            Err(e) => debug!("Failed to parse inbound message: {}", e),
                        }
                    }
                }
            });

            // If forward mode, start local SOCKS5 server
            if !self.options.reverse {
                let ws_tx = sender.clone();
                let socks_host = self.options.socks_host.clone();
                let socks_port = self.options.socks_port;
                let pending = self.pending_connect.clone();
                let writers = self.channel_streams.clone();
                tokio::spawn(async move {
                    let addr = format!("{}:{}", socks_host, socks_port);
                    match TcpListener::bind(&addr).await {
                        Ok(listener) => {
                            log::info!("SOCKS5 server listening on {}", addr);
                            loop {
                                match listener.accept().await {
                                    Ok((stream, peer)) => {
                                        log::debug!("SOCKS accepted from {}", peer);
                                        let ws_tx = ws_tx.clone();
                                        let pending = pending.clone();
                                        let writers = writers.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) =
                                                handle_socks_conn(ws_tx, pending, writers, stream)
                                                    .await
                                            {
                                                log::warn!(
                                                    "SOCKS connection error from {}: {}",
                                                    peer,
                                                    e
                                                );
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        log::warn!("SOCKS accept error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => log::error!("Failed to bind SOCKS5 server on {}: {}", addr, e),
                    }
                });
            }
        } else {
            return Err("WebSocket sender not initialized".to_string());
        }

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
        let payload = message
            .pack()
            .map_err(|e| format!("Failed to pack connector message: {}", e))?;

        sender
            .send(WsMessage::Binary(payload))
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

async fn handle_socks_conn(
    ws_tx: mpsc::Sender<WsMessage>,
    pending: PendingConnect,
    writers: ChannelWriters,
    mut stream: TcpStream,
) -> Result<(), String> {
    // Method negotiation
    let mut hdr = [0u8; 2];
    stream
        .read_exact(&mut hdr)
        .await
        .map_err(|e| e.to_string())?;
    if hdr[0] != 0x05 {
        return Err("Invalid SOCKS version".to_string());
    }
    let n = hdr[1] as usize;
    let mut methods = vec![0u8; n];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|e| e.to_string())?;
    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| e.to_string())?;

    // Request
    let mut req = [0u8; 4];
    stream
        .read_exact(&mut req)
        .await
        .map_err(|e| e.to_string())?;
    if req[0] != 0x05 || req[1] != 0x01 {
        return Err("Only CONNECT supported".to_string());
    }
    let atyp = req[3];
    let address = match atyp {
        0x01 => {
            let mut a = [0u8; 4];
            stream.read_exact(&mut a).await.map_err(|e| e.to_string())?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        0x03 => {
            let mut l = [0u8; 1];
            stream.read_exact(&mut l).await.map_err(|e| e.to_string())?;
            let sz = l[0] as usize;
            let mut name = vec![0u8; sz];
            stream
                .read_exact(&mut name)
                .await
                .map_err(|e| e.to_string())?;
            String::from_utf8(name).map_err(|e| e.to_string())?
        }
        0x04 => {
            let mut a = [0u8; 16];
            stream.read_exact(&mut a).await.map_err(|e| e.to_string())?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => return Err("Invalid ATYP".to_string()),
    };
    let mut p = [0u8; 2];
    stream.read_exact(&mut p).await.map_err(|e| e.to_string())?;
    let port = u16::from_be_bytes(p);

    log::debug!("SOCKS connect request target {}:{}", address, port);

    // Create channel and send Connect
    let channel_id = Uuid::new_v4();
    log::debug!("Allocating channel {} for {}:{}", channel_id, address, port);
    let connect = crate::message::ConnectMessage {
        protocol: "tcp".to_string(),
        channel_id,
        address: address.clone(),
        port,
    };
    let frame = connect.pack().map_err(|e| e.to_string())?;
    ws_tx
        .send(WsMessage::Binary(frame))
        .await
        .map_err(|e| e.to_string())?;
    log::debug!("Sent connect frame for channel {}", channel_id);

    let (tx, rx) = oneshot::channel();
    {
        let mut map = pending.lock().await;
        map.insert(channel_id, tx);
    }
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| "Connect response timeout".to_string())?
        .map_err(|_| "Connect response channel closed".to_string())??;
    log::debug!(
        "Received connect_response success for channel {}",
        channel_id
    );

    // Reply success to SOCKS client
    let reply = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
    stream.write_all(&reply).await.map_err(|e| e.to_string())?;

    // Split and register writer
    let (mut ri, wi) = stream.into_split();
    {
        let mut map = writers.lock().await;
        map.insert(channel_id, Arc::new(tokio::sync::Mutex::new(wi)));
    }
    log::debug!("Registered TCP writer for channel {}", channel_id);

    // TCP->WS forward
    tokio::spawn(async move {
        log::debug!("TCP->WS forward loop started for channel {}", channel_id);
        let mut buf = vec![0u8; 8192];
        loop {
            match ri.read(&mut buf).await {
                Ok(0) => {
                    log::debug!("TCP EOF on channel {}", channel_id);
                    break;
                }
                Ok(n) => {
                    log::debug!("TCP->WS {} bytes on channel {}", n, channel_id);
                    let dm = crate::message::DataMessage::new(channel_id, buf[..n].to_vec());
                    if let Ok(f) = dm.pack() {
                        if ws_tx.send(WsMessage::Binary(f)).await.is_err() {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    log::debug!("TCP read error on channel {}: {}", channel_id, e);
                    break;
                }
            }
        }
        let _ = ws_tx
            .send(WsMessage::Binary(
                crate::message::DisconnectMessage::new(channel_id)
                    .pack()
                    .unwrap_or_default(),
            ))
            .await;
        log::debug!("Sent WS disconnect for channel {}", channel_id);
    });

    Ok(())
}

impl Clone for LinkSocksClient {
    fn clone(&self) -> Self {
        LinkSocksClient {
            token: self.token.clone(),
            options: self.options.clone(),
            ws_sender: self.ws_sender.clone(),
            channels: self.channels.clone(),
            pending_connect: self.pending_connect.clone(),
            channel_streams: self.channel_streams.clone(),
            ready: self.ready.clone(),
            shutdown: self.shutdown.clone(),
            socks_listener: self.socks_listener.clone(),
        }
    }
}
