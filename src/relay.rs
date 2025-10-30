//! Relay implementation for rusocks

use crate::message::{
    ConnectMessage, ConnectResponseMessage, DataMessage, DisconnectMessage, Message,
};
use log::error;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

/// Default buffer size for data transfer
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default channel timeout
pub const DEFAULT_CHANNEL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connect timeout
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Relay options
#[derive(Clone)]
pub struct RelayOption {
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
}

impl Default for RelayOption {
    fn default() -> Self {
        RelayOption {
            buffer_size: DEFAULT_BUFFER_SIZE,
            channel_timeout: DEFAULT_CHANNEL_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            fast_open: false,
            upstream_proxy: None,
            upstream_username: None,
            upstream_password: None,
        }
    }
}

impl RelayOption {
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
}

/// Channel state
enum ChannelState {
    /// Waiting for connection
    Connecting,

    /// Connected
    Connected,

    /// Disconnected
    Disconnected,
}

/// Channel information
struct ChannelInfo {
    /// Channel ID
    _id: Uuid,

    /// Channel state
    state: ChannelState,

    /// TCP write half
    writer: Option<OwnedWriteHalf>,

    /// WebSocket sender
    ws_sender: mpsc::Sender<WsMessage>,

    /// Message queue (WS->TCP)
    message_queue: mpsc::Receiver<Vec<u8>>,

    /// Message sender (WS->TCP)
    message_tx: mpsc::Sender<Vec<u8>>,
}

/// Relay handles the relay of data between WebSocket and TCP connections
pub struct Relay {
    /// Relay options
    options: RelayOption,

    /// Channels
    channels: Arc<RwLock<HashMap<Uuid, Arc<Mutex<ChannelInfo>>>>>,

    /// Fast open success channels
    fast_open_success: Arc<RwLock<HashMap<Uuid, bool>>>,
}

impl Relay {
    /// Create a new Relay
    pub fn new(options: RelayOption) -> Self {
        Relay {
            options,
            channels: Arc::new(RwLock::new(HashMap::new())),
            fast_open_success: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new Relay with default options
    pub fn new_default() -> Self {
        Self::new(RelayOption::default())
    }

    /// Handle a network connection
    pub async fn handle_network_connection(
        &self,
        ws_sender: mpsc::Sender<WsMessage>,
        connect_msg: ConnectMessage,
    ) -> Result<(), String> {
        let channel_id = connect_msg.channel_id;
        let address = connect_msg.address;

        // Create message queue
        let (queue_tx, queue_rx) = mpsc::channel(1000);

        // Create channel info
        let channel_info = Arc::new(Mutex::new(ChannelInfo {
            _id: channel_id,
            state: ChannelState::Connecting,
            writer: None,
            ws_sender: ws_sender.clone(),
            message_queue: queue_rx,
            message_tx: queue_tx.clone(),
        }));

        // Store channel info
        self.channels
            .write()
            .await
            .insert(channel_id, channel_info.clone());

        // Connect to the target
        let addr_str = format!("{}:{}", address, connect_msg.port);
        let addr = match addr_str.parse::<SocketAddr>() {
            Ok(addr) => addr,
            Err(_) => {
                // Try to resolve the address
                match tokio::net::lookup_host(&addr_str).await {
                    Ok(mut addrs) => {
                        if let Some(addr) = addrs.next() {
                            addr
                        } else {
                            let response = ConnectResponseMessage::failure(
                                channel_id,
                                format!("Failed to resolve address: {}", addr_str),
                            );
                            if let Ok(binary) = response.pack() {
                                let _ = ws_sender.send(WsMessage::Binary(binary)).await;
                            }
                            return Err(format!("Failed to resolve address: {}", addr_str));
                        }
                    }
                    Err(e) => {
                        let response = ConnectResponseMessage::failure(
                            channel_id,
                            format!("Failed to resolve address: {}", e),
                        );
                        if let Ok(binary) = response.pack() {
                            let _ = ws_sender.send(WsMessage::Binary(binary)).await;
                        }
                        return Err(format!("Failed to resolve address: {}", e));
                    }
                }
            }
        };

        // Connect with timeout
        let connect_result = timeout(self.options.connect_timeout, TcpStream::connect(addr)).await;

        match connect_result {
            Ok(Ok(stream)) => {
                // Connection successful
                let mut channel = channel_info.lock().await;
                // Split into read and write halves
                let (reader, writer) = stream.into_split();
                channel.writer = Some(writer);
                channel.state = ChannelState::Connected;

                // Send success response
                let response = ConnectResponseMessage::success(channel_id);
                if let Ok(binary) = response.pack() {
                    let _ = ws_sender.send(WsMessage::Binary(binary)).await;
                }

                // Start data transfer with the reader half
                drop(channel); // release lock before spawn
                self.start_data_transfer(channel_id, reader, ws_sender.clone())
                    .await;

                Ok(())
            }
            Ok(Err(e)) => {
                // Connection failed
                let response = ConnectResponseMessage::failure(
                    channel_id,
                    format!("Connection failed: {}", e),
                );
                if let Ok(binary) = response.pack() {
                    let _ = ws_sender.send(WsMessage::Binary(binary)).await;
                }

                // Remove channel
                self.channels.write().await.remove(&channel_id);

                Err(format!("Connection failed: {}", e))
            }
            Err(_) => {
                // Connection timeout
                let response =
                    ConnectResponseMessage::failure(channel_id, "Connection timeout".to_string());
                if let Ok(binary) = response.pack() {
                    let _ = ws_sender.send(WsMessage::Binary(binary)).await;
                }

                // Remove channel
                self.channels.write().await.remove(&channel_id);

                Err("Connection timeout".to_string())
            }
        }
    }

    /// Start data transfer between WebSocket and TCP connection
    async fn start_data_transfer(
        &self,
        channel_id: Uuid,
        mut reader: OwnedReadHalf,
        ws_sender: mpsc::Sender<WsMessage>,
    ) {
        // Clone for async tasks
        let channel_id_clone = channel_id;
        let relay_clone1 = self.clone();
        let relay_clone2 = self.clone();

        // Read from TCP and send to WebSocket
        tokio::spawn(async move {
            let mut buffer = vec![0u8; relay_clone1.options.buffer_size];

            loop {
                match reader.read(&mut buffer).await {
                    Ok(0) => {
                        // EOF
                        break;
                    }
                    Ok(n) => {
                        // Send data to WebSocket as DataMessage
                        let data = buffer[..n].to_vec();
                        let msg = crate::message::DataMessage::new(channel_id_clone, data);
                        if let Ok(frame) = msg.pack() {
                            if let Err(e) = ws_sender.send(WsMessage::Binary(frame)).await {
                                error!("Failed to send data to WS: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from TCP: {}", e);
                        break;
                    }
                }
            }

            // Disconnect
            relay_clone1.disconnect_channel(channel_id_clone).await;
        });

        // Read from WebSocket and send to TCP
        // Clone channels to avoid borrowing self
        let channels = self.channels.clone();

        tokio::spawn(async move {
            if let Some(channel_info) = channels.read().await.get(&channel_id) {
                let mut channel = channel_info.lock().await;

                while let Some(data) = channel.message_queue.recv().await {
                    if let Some(writer) = &mut channel.writer {
                        if let Err(e) = writer.write_all(&data).await {
                            error!("Failed to write to TCP: {}", e);
                            break;
                        }
                    } else {
                        error!("TCP stream not available");
                        break;
                    }
                }
            }

            // Disconnect
            relay_clone2.disconnect_channel(channel_id).await;
        });
    }

    /// Disconnect a channel
    pub async fn disconnect_channel(&self, channel_id: Uuid) {
        if let Some(channel_info) = self.channels.read().await.get(&channel_id) {
            let mut channel = channel_info.lock().await;

            // Send disconnect message
            let disconnect_msg = DisconnectMessage::new(channel_id);
            if let Ok(binary) = disconnect_msg.pack() {
                let _ = channel.ws_sender.send(WsMessage::Binary(binary)).await;
            }

            // Close TCP writer
            if let Some(writer) = &mut channel.writer {
                let _ = writer.shutdown().await;
            }

            // Update state
            channel.state = ChannelState::Disconnected;
        }

        // Remove channel
        self.channels.write().await.remove(&channel_id);
        self.fast_open_success.write().await.remove(&channel_id);
    }

    /// Handle a data message
    pub async fn handle_data_message(&self, data_msg: DataMessage) -> Result<(), String> {
        let channel_id = data_msg.channel_id;

        // Check if channel exists
        if let Some(channel_info) = self.channels.read().await.get(&channel_id) {
            let channel = channel_info.lock().await;

            // Check if fast open is enabled and connection is not yet confirmed
            if self.options.fast_open {
                if let Some(success) = self.fast_open_success.read().await.get(&channel_id) {
                    if !*success {
                        // Connection not yet confirmed, buffer the data
                        return Ok(());
                    }
                }
            }

            // Check if channel is connected
            match channel.state {
                ChannelState::Connected => {
                    let data = data_msg.data.clone();
                    // Queue data for TCP writer task
                    if channel.message_tx.send(data).await.is_err() {
                        return Err("Failed to enqueue data to TCP writer".to_string());
                    }
                }
                _ => {
                    return Err("Channel not connected".to_string());
                }
            }
        } else {
            return Err("Channel not found".to_string());
        }

        Ok(())
    }

    /// Set connection success for fast open
    pub async fn set_connection_success(&self, channel_id: Uuid) {
        self.fast_open_success
            .write()
            .await
            .insert(channel_id, true);
    }

    /// Close the relay
    pub async fn close(&self) {
        // Disconnect all channels
        let channel_ids: Vec<Uuid> = self.channels.read().await.keys().cloned().collect();
        for channel_id in channel_ids {
            self.disconnect_channel(channel_id).await;
        }
    }
}

impl Clone for Relay {
    fn clone(&self) -> Self {
        Relay {
            options: self.options.clone(),
            channels: self.channels.clone(),
            fast_open_success: self.fast_open_success.clone(),
        }
    }
}
