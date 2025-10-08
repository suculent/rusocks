//! Socket management for rusocks

use log::{debug, trace};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

/// SocketManager manages socket lifecycle and reuse
pub struct SocketManager {
    sockets: Arc<Mutex<HashMap<u16, ManagedSocket>>>,
    host: String,
}

/// ManagedSocket represents a socket with reference counting
struct ManagedSocket {
    listener: TcpListener,
    ref_count: usize,
    close_timer: Option<Instant>,
}

impl SocketManager {
    /// Create a new SocketManager
    pub fn new(host: &str) -> Self {
        SocketManager {
            sockets: Arc::new(Mutex::new(HashMap::new())),
            host: host.to_string(),
        }
    }

    /// Get a listener for the specified port
    pub fn get_listener(&self, port: u16) -> std::io::Result<TcpListener> {
        let mut sockets = self.sockets.lock().unwrap();
        
        // Check if we have an existing socket
        if let Some(sock) = sockets.get_mut(&port) {
            sock.close_timer = None;
            sock.ref_count += 1;
            debug!("Reusing socket for port {}", port);
            
            // Clone the listener
            let _addr = sock.listener.local_addr()?;
            return Ok(sock.listener.try_clone()?);
        }
        
        // Create new socket
        let addr = format!("{}:{}", self.host, port);
        let listener = TcpListener::bind(addr)?;
        debug!("Allocated new socket for port {}", port);
        
        sockets.insert(port, ManagedSocket {
            listener: listener.try_clone()?,
            ref_count: 1,
            close_timer: None,
        });
        
        Ok(listener)
    }

    /// Release a listener for the specified port
    pub fn release_listener(&self, port: u16) {
        let mut sockets = self.sockets.lock().unwrap();
        
        if let Some(sock) = sockets.get_mut(&port) {
            sock.ref_count -= 1;
            if sock.ref_count <= 0 {
                // Start delayed cleanup
                sock.close_timer = Some(Instant::now() + Duration::from_secs(30));
                debug!("Socket scheduled for delayed cleanup: port {}", port);
                
                // Clone for async cleanup
                let sockets_clone = self.sockets.clone();
                let port_clone = port;
                
                // Spawn a task to clean up after delay
                tokio::spawn(async move {
                    sleep(Duration::from_secs(30)).await;
                    
                    let mut sockets = sockets_clone.lock().unwrap();
                    if let Some(s) = sockets.get(&port_clone) {
                        if let Some(timer) = s.close_timer {
                            if timer <= Instant::now() {
                                sockets.remove(&port_clone);
                                debug!("Socket closed after delay: port {}", port_clone);
                            }
                        }
                    }
                });
            }
        }
    }

    /// Close all managed sockets immediately
    pub fn close(&self) {
        let mut sockets = self.sockets.lock().unwrap();
        sockets.clear();
    }
}

/// AsyncSocketManager is an async version of SocketManager for use with tokio
pub struct AsyncSocketManager {
    sockets: Arc<AsyncMutex<HashMap<u16, AsyncManagedSocket>>>,
    host: String,
}

/// AsyncManagedSocket represents an async socket with reference counting
struct AsyncManagedSocket {
    addr: SocketAddr,
    ref_count: usize,
    close_timer: Option<Instant>,
}

impl AsyncSocketManager {
    /// Create a new AsyncSocketManager
    pub fn new(host: &str) -> Self {
        AsyncSocketManager {
            sockets: Arc::new(AsyncMutex::new(HashMap::new())),
            host: host.to_string(),
        }
    }

    /// Get a socket address for the specified port
    pub async fn get_socket_addr(&self, port: u16) -> std::io::Result<SocketAddr> {
        let mut sockets = self.sockets.lock().await;
        
        // Check if we have an existing socket
        if let Some(sock) = sockets.get_mut(&port) {
            sock.close_timer = None;
            sock.ref_count += 1;
            trace!("Reusing socket address for port {}", port);
            return Ok(sock.addr);
        }
        
        // Create new socket address
        let addr_str = format!("{}:{}", self.host, port);
        let addr: SocketAddr = addr_str.parse().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?;
        
        // Test that we can bind to this address
        let listener = TcpListener::bind(&addr)?;
        drop(listener); // Release immediately
        
        debug!("Allocated new socket address for port {}", port);
        
        sockets.insert(port, AsyncManagedSocket {
            addr,
            ref_count: 1,
            close_timer: None,
        });
        
        Ok(addr)
    }

    /// Release a socket for the specified port
    pub async fn release_socket(&self, port: u16) {
        let mut sockets = self.sockets.lock().await;
        
        if let Some(sock) = sockets.get_mut(&port) {
            sock.ref_count -= 1;
            if sock.ref_count <= 0 {
                // Start delayed cleanup
                sock.close_timer = Some(Instant::now() + Duration::from_secs(30));
                debug!("Socket address scheduled for delayed cleanup: port {}", port);
                
                // Clone for async cleanup
                let sockets_clone = self.sockets.clone();
                let port_clone = port;
                
                // Spawn a task to clean up after delay
                tokio::spawn(async move {
                    sleep(Duration::from_secs(30)).await;
                    
                    let mut sockets = sockets_clone.lock().await;
                    if let Some(s) = sockets.get(&port_clone) {
                        if let Some(timer) = s.close_timer {
                            if timer <= Instant::now() {
                                sockets.remove(&port_clone);
                                debug!("Socket address released after delay: port {}", port_clone);
                            }
                        }
                    }
                });
            }
        }
    }

    /// Close all managed sockets immediately
    pub async fn close(&self) {
        let mut sockets = self.sockets.lock().await;
        sockets.clear();
    }
}