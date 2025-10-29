//! Connection handling for rusocks

use crate::message::Message;
use futures_util::{SinkExt, StreamExt};
use log::error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Error as WsError, Message as WsMessage},
    MaybeTlsStream, WebSocketStream,
};
use url::Url;
use uuid::Uuid;

/// WebSocket connection
pub struct WSConn {
    /// Connection ID
    _id: Uuid,

    /// Client IP address
    client_ip: Arc<RwLock<String>>,

    /// Connection label
    label: Arc<RwLock<String>>,

    /// WebSocket sender
    sender: mpsc::Sender<WsMessage>,

    /// Closed flag
    closed: Arc<Mutex<bool>>,
}

impl WSConn {
    /// Create a new WebSocket connection
    pub fn new(sender: mpsc::Sender<WsMessage>, label: &str) -> Self {
        WSConn {
            _id: Uuid::new_v4(),
            client_ip: Arc::new(RwLock::new(String::new())),
            label: Arc::new(RwLock::new(label.to_string())),
            sender,
            closed: Arc::new(Mutex::new(false)),
        }
    }

    /// Set the client IP from a request
    pub async fn set_client_ip_from_request(&self, addr: SocketAddr) {
        let mut client_ip = self.client_ip.write().await;
        *client_ip = addr.to_string();
    }

    /// Set the connection label
    pub async fn set_label(&self, label: &str) {
        let mut l = self.label.write().await;
        *l = label.to_string();
    }

    /// Get the connection label
    pub async fn label(&self) -> String {
        let l = self.label.read().await;
        l.clone()
    }

    /// Get the client IP
    pub async fn get_client_ip(&self) -> String {
        let client_ip = self.client_ip.read().await;
        client_ip.clone()
    }

    /// Write a message to the WebSocket
    pub async fn write_message<T: Message>(&self, message: T) -> Result<(), String> {
        let binary = match message.pack() {
            Ok(binary) => binary,
            Err(e) => return Err(format!("Failed to pack message: {}", e)),
        };

        self.write_raw_message(WsMessage::Binary(binary)).await
    }

    /// Write a raw WebSocket message
    pub async fn write_raw_message(&self, message: WsMessage) -> Result<(), String> {
        // Check if closed
        let closed = self.closed.lock().await;
        if *closed {
            return Err("Connection closed".to_string());
        }

        // Send message
        match self.sender.send(message).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to send message: {}", e)),
        }
    }

    /// Close the connection
    pub async fn close(&self) {
        let mut closed = self.closed.lock().await;
        if !*closed {
            *closed = true;

            // Send close message
            let _ = self.sender.send(WsMessage::Close(None)).await;
        }
    }
}

/// WebSocket message handler
pub struct WSHandler {
    /// WebSocket stream
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,

    /// Message sender
    sender: mpsc::Sender<WsMessage>,

    /// Message receiver
    receiver: mpsc::Receiver<WsMessage>,

    /// Closed flag
    closed: Arc<Mutex<bool>>,
}

impl WSHandler {
    /// Create a new WebSocket handler
    pub fn new(
        stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> (Self, mpsc::Sender<WsMessage>) {
        let (sender, receiver) = mpsc::channel(100);

        (
            WSHandler {
                stream: Some(stream),
                sender: sender.clone(),
                receiver,
                closed: Arc::new(Mutex::new(false)),
            },
            sender,
        )
    }

    /// Start the WebSocket handler
    pub async fn start(&mut self) -> Result<(), WsError> {
        // Start reader and writer tasks
        let stream = self.stream.take().ok_or(WsError::ConnectionClosed)?;
        let (mut ws_sender, mut ws_receiver) = stream.split();

        // Reader task: consume incoming messages but do not forward them to outbound channel
        let closed = self.closed.clone();

        tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(msg) => {
                        if msg.is_close() {
                            let mut c = closed.lock().await;
                            *c = true;
                            break;
                        }
                        // Ignore all incoming messages (auth responses, pings, etc.)
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }

            let mut c = closed.lock().await;
            *c = true;
        });

        // Writer task
        // Can't clone receiver, so we need to take ownership of it
        let mut receiver = std::mem::replace(&mut self.receiver, mpsc::channel(1).1);
        let closed = self.closed.clone();

        tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                // Check if closed
                let c = closed.lock().await;
                if *c {
                    break;
                }

                // Send message
                if let Err(e) = ws_sender.send(msg).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }

            // Connection closed
            let mut c = closed.lock().await;
            *c = true;
        });

        Ok(())
    }

    /// Check if the connection is closed
    pub async fn is_closed(&self) -> bool {
        let closed = self.closed.lock().await;
        *closed
    }

    /// Close the connection
    pub async fn close(&self) {
        let mut closed = self.closed.lock().await;
        *closed = true;
    }
}

/// Connect to a WebSocket server
pub async fn connect_to_websocket(
    url: &str,
    user_agent: Option<&str>,
) -> Result<(WSHandler, mpsc::Sender<WsMessage>), String> {
    // Parse URL
    let url = match Url::parse(url) {
        Ok(url) => url,
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    // Connect to WebSocket server with optional custom User-Agent
    let request = match url.clone().into_client_request() {
        Ok(mut request) => {
            // Set custom User-Agent if provided
            if let Some(agent) = user_agent {
                request.headers_mut().insert(
                    "User-Agent",
                    tokio_tungstenite::tungstenite::http::HeaderValue::from_str(agent)
                        .map_err(|e| format!("Invalid User-Agent header: {}", e))?,
                );
            }
            request
        }
        Err(e) => return Err(format!("Failed to create WebSocket request: {}", e)),
    };

    // Connect with the request
    let (ws_stream, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(conn) => conn,
        Err(e) => return Err(format!("Failed to connect to WebSocket server: {}", e)),
    };

    // Create handler using the established WebSocket stream
    let (handler, sender) = WSHandler::new(ws_stream);

    Ok((handler, sender))
}
