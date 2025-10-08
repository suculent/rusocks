//! SOCKS5 over WebSocket proxy tool

pub mod cli;
pub mod server;
pub mod socket;
pub mod client;
pub mod message;
pub mod relay;
pub mod version;
pub mod forwarder;
pub mod portpool;
pub mod conn;
pub mod api;
pub mod batchlog;
pub mod python;

#[cfg(test)]
mod tests {
    pub mod user_agent_test;
}

// Re-export commonly used items
pub use crate::server::LinkSocksServer;
pub use crate::client::LinkSocksClient;
pub use crate::cli::CLI;
pub use crate::version::{VERSION, PLATFORM};