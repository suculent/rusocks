//! Port pool management for rusocks

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// PortPool manages a pool of available ports
pub struct PortPool {
    min: u16,
    max: u16,
    used: Arc<Mutex<HashSet<u16>>>,
}

impl PortPool {
    /// Create a new PortPool with the specified range
    pub fn new_from_range(min: u16, max: u16) -> Self {
        PortPool {
            min,
            max,
            used: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Create a new PortPool with the default range (1024-10240)
    pub fn new_default() -> Self {
        Self::new_from_range(1024, 10240)
    }

    /// Get a port from the pool
    /// If preferred_port is Some, try to allocate that port
    /// Returns 0 if no ports are available
    pub fn get(&self, preferred_port: Option<u16>) -> u16 {
        let mut used = self.used.lock().unwrap();

        // Try to use preferred port if specified
        if let Some(port) = preferred_port {
            if port >= self.min && port <= self.max && !used.contains(&port) {
                used.insert(port);
                return port;
            }
        }

        // Find an available port
        for port in self.min..=self.max {
            if !used.contains(&port) {
                used.insert(port);
                return port;
            }
        }

        // No ports available
        0
    }

    /// Return a port to the pool
    pub fn put(&self, port: u16) {
        if port >= self.min && port <= self.max {
            let mut used = self.used.lock().unwrap();
            used.remove(&port);
        }
    }

    /// Check if a port is in use
    pub fn is_used(&self, port: u16) -> bool {
        let used = self.used.lock().unwrap();
        used.contains(&port)
    }

    /// Get the number of used ports
    pub fn used_count(&self) -> usize {
        let used = self.used.lock().unwrap();
        used.len()
    }

    /// Get the number of available ports
    pub fn available_count(&self) -> usize {
        let used = self.used.lock().unwrap();
        (self.max - self.min + 1) as usize - used.len()
    }
}

impl Default for PortPool {
    fn default() -> Self {
        Self::new_default()
    }
}

impl Clone for PortPool {
    fn clone(&self) -> Self {
        PortPool {
            min: self.min,
            max: self.max,
            used: self.used.clone(),
        }
    }
}
