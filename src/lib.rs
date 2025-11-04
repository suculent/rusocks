//! SOCKS5 over WebSocket proxy tool

pub mod api;
pub mod batchlog;
pub mod cli;
pub mod client;
pub mod conn;
pub mod forwarder;
pub mod message;
pub mod portpool;
pub mod python;
pub mod relay;
pub mod server;
pub mod socket;
pub mod version;

// Re-export commonly used items
pub use crate::cli::CLI;
pub use crate::client::LinkSocksClient;
pub use crate::server::LinkSocksServer;
pub use crate::version::{PLATFORM, VERSION};

#[cfg(test)]
mod tests {
    pub mod user_agent_test;
}
