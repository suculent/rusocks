//! Forwarder implementation for rusocks

use log::{debug, error, info, trace, warn};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// Forwarder forwards TCP connections
pub struct Forwarder {
    /// Source address
    source: SocketAddr,
    
    /// Target address
    target: SocketAddr,
    
    /// Buffer size
    buffer_size: usize,
    
    /// Listener
    listener: Arc<Mutex<Option<TcpListener>>>,
}

impl Forwarder {
    /// Create a new Forwarder
    pub fn new(source: SocketAddr, target: SocketAddr, buffer_size: usize) -> Self {
        Forwarder {
            source,
            target,
            buffer_size,
            listener: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the forwarder
    pub async fn start(&self) -> io::Result<()> {
        // Create listener
        let listener = TcpListener::bind(self.source).await?;
        info!("Forwarder listening on {}", self.source);
        
        // Store listener in the struct
        *self.listener.lock().await = Some(listener);
        
        // Create a new listener for accepting connections
        let accept_listener = TcpListener::bind(self.source).await?;
        
        // Accept connections
        loop {
            match accept_listener.accept().await {
                Ok((inbound, addr)) => {
                    info!("Accepted connection from {}", addr);
                    
                    // Handle connection
                    let target = self.target;
                    let buffer_size = self.buffer_size;
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(inbound, target, buffer_size).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a connection
    async fn handle_connection(
        mut inbound: TcpStream,
        target: SocketAddr,
        buffer_size: usize,
    ) -> io::Result<()> {
        // Connect to target
        let mut outbound = match TcpStream::connect(target).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to connect to target: {}", e);
                return Err(e);
            }
        };
        
        // Copy data in both directions
        let (mut ri, mut wi) = inbound.split();
        let (mut ro, mut wo) = outbound.split();
        
        let client_to_server = async {
            let mut buffer = vec![0u8; buffer_size];
            loop {
                match ri.read(&mut buffer).await {
                    Ok(0) => {
                        // EOF
                        break;
                    }
                    Ok(n) => {
                        if let Err(e) = wo.write_all(&buffer[..n]).await {
                            error!("Failed to write to target: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from client: {}", e);
                        break;
                    }
                }
            }
            
            // Shutdown write to signal EOF
            let _ = wo.shutdown().await;
        };
        
        let server_to_client = async {
            let mut buffer = vec![0u8; buffer_size];
            loop {
                match ro.read(&mut buffer).await {
                    Ok(0) => {
                        // EOF
                        break;
                    }
                    Ok(n) => {
                        if let Err(e) = wi.write_all(&buffer[..n]).await {
                            error!("Failed to write to client: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from target: {}", e);
                        break;
                    }
                }
            }
            
            // Shutdown write to signal EOF
            let _ = wi.shutdown().await;
        };
        
        // Run both directions concurrently
        tokio::select! {
            _ = client_to_server => {}
            _ = server_to_client => {}
        }
        
        Ok(())
    }

    /// Stop the forwarder
    pub async fn stop(&self) {
        let mut listener = self.listener.lock().await;
        *listener = None;
    }
}