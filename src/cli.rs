//! Command-line interface for rusocks

use crate::client::{ClientOption, LinkSocksClient};
use crate::server::{LinkSocksServer, ReverseTokenOptions, ServerOption};
use crate::version::{PLATFORM, VERSION};
use clap::{Parser, Subcommand};
use log::{error, info, LevelFilter};
use std::error::Error;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;
use url::Url;

/// CLI represents the command-line interface for rusocks
pub struct CLI {
    app: App,
}

/// SOCKS5 over WebSocket proxy tool
#[derive(Parser)]
#[clap(version = VERSION, about = "SOCKS5 over WebSocket proxy tool")]
struct App {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the version number
    Version,

    /// Start SOCKS5 over WebSocket proxy client
    Client {
        /// Authentication token
        #[clap(short = 't', long)]
        token: Option<String>,

        /// WebSocket server address
        #[clap(short = 'u', long, default_value = "ws://localhost:8765")]
        url: String,

        /// Use reverse socks5 proxy
        #[clap(short = 'r', long)]
        reverse: bool,

        /// Specify connector token for reverse proxy
        #[clap(short = 'c', long)]
        connector_token: Option<String>,

        /// SOCKS5 server listen address for forward proxy
        #[clap(short = 's', long, default_value = "127.0.0.1")]
        socks_host: String,

        /// SOCKS5 server listen port for forward proxy
        #[clap(short = 'p', long, default_value = "9870")]
        socks_port: u16,

        /// SOCKS5 authentication username
        #[clap(short = 'n', long)]
        socks_username: Option<String>,

        /// SOCKS5 authentication password
        #[clap(short = 'w', long)]
        socks_password: Option<String>,

        /// Start the SOCKS server immediately
        #[clap(short = 'i', long)]
        socks_no_wait: bool,

        /// Stop when the server disconnects
        #[clap(short = 'R', long)]
        no_reconnect: bool,

        /// Show debug logs (use -dd for trace logs)
        #[clap(short = 'd', long, action = clap::ArgAction::Count)]
        debug: u8,

        /// Number of threads for data transfer
        #[clap(short = 'T', long, default_value = "1")]
        threads: u32,

        /// Upstream SOCKS5 proxy (e.g., socks5://user:pass@127.0.0.1:9870)
        #[clap(short = 'x', long)]
        upstream_proxy: Option<String>,

        /// Assume connection success and allow data transfer immediately
        #[clap(short = 'f', long)]
        fast_open: bool,

        /// Ignore proxy settings from environment variables when connecting to the websocket server
        #[clap(short = 'E', long)]
        no_env_proxy: bool,
    },

    /// Alias for client command
    Connector {
        #[clap(flatten)]
        client_args: ClientArgs,
    },

    /// Alias for client -r command
    Provider {
        #[clap(flatten)]
        client_args: ClientArgs,
    },

    /// Start SOCKS5 over WebSocket proxy server
    Server {
        /// WebSocket server listen address
        #[clap(short = 'H', long, default_value = "0.0.0.0")]
        ws_host: String,

        /// WebSocket server listen port
        #[clap(short = 'P', long, default_value = "8765")]
        ws_port: u16,

        /// Specify auth token, auto-generate if not provided
        #[clap(short = 't', long)]
        token: Option<String>,

        /// Specify connector token for reverse proxy, auto-generate if not provided
        #[clap(short = 'c', long)]
        connector_token: Option<String>,

        /// Allow clients to manage their connector tokens
        #[clap(short = 'a', long)]
        connector_autonomy: bool,

        /// Set buffer size for data transfer
        #[clap(short = 'b', long, default_value = "8192")]
        buffer_size: usize,

        /// Use reverse socks5 proxy
        #[clap(short = 'r', long)]
        reverse: bool,

        /// SOCKS5 server listen address for reverse proxy
        #[clap(short = 's', long, default_value = "127.0.0.1")]
        socks_host: String,

        /// SOCKS5 server listen port for reverse proxy
        #[clap(short = 'p', long, default_value = "9870")]
        socks_port: u16,

        /// SOCKS5 username for authentication
        #[clap(short = 'n', long)]
        socks_username: Option<String>,

        /// SOCKS5 password for authentication
        #[clap(short = 'w', long)]
        socks_password: Option<String>,

        /// Start the SOCKS server immediately
        #[clap(short = 'i', long)]
        socks_nowait: bool,

        /// Show debug logs (use -dd for trace logs)
        #[clap(short = 'd', long, action = clap::ArgAction::Count)]
        debug: u8,

        /// Enable HTTP API with specified key
        #[clap(short = 'k', long)]
        api_key: Option<String>,

        /// Upstream SOCKS5 proxy (e.g., socks5://user:pass@127.0.0.1:9870)
        #[clap(short = 'x', long)]
        upstream_proxy: Option<String>,

        /// Assume connection success and allow data transfer immediately
        #[clap(short = 'f', long)]
        fast_open: bool,
    },
}

/// Shared client arguments for reuse in connector and provider commands
#[derive(Parser)]
struct ClientArgs {
    /// Authentication token
    #[clap(short = 't', long)]
    token: Option<String>,

    /// WebSocket server address
    #[clap(short = 'u', long, default_value = "ws://localhost:8765")]
    url: String,

    /// Use reverse socks5 proxy
    #[clap(short = 'r', long)]
    reverse: bool,

    /// Specify connector token for reverse proxy
    #[clap(short = 'c', long)]
    connector_token: Option<String>,

    /// SOCKS5 server listen address for forward proxy
    #[clap(short = 's', long, default_value = "127.0.0.1")]
    socks_host: String,

    /// SOCKS5 server listen port for forward proxy
    #[clap(short = 'p', long, default_value = "9870")]
    socks_port: u16,

    /// SOCKS5 authentication username
    #[clap(short = 'n', long)]
    socks_username: Option<String>,

    /// SOCKS5 authentication password
    #[clap(short = 'w', long)]
    socks_password: Option<String>,

    /// Start the SOCKS server immediately
    #[clap(short = 'i', long)]
    socks_no_wait: bool,

    /// Stop when the server disconnects
    #[clap(short = 'R', long)]
    no_reconnect: bool,

    /// Show debug logs (use -dd for trace logs)
    #[clap(short = 'd', long, action = clap::ArgAction::Count)]
    debug: u8,

    /// Number of threads for data transfer
    #[clap(short = 'T', long, default_value = "1")]
    threads: u32,

    /// Upstream SOCKS5 proxy (e.g., socks5://user:pass@127.0.0.1:9870)
    #[clap(short = 'x', long)]
    upstream_proxy: Option<String>,

    /// Assume connection success and allow data transfer immediately
    #[clap(short = 'f', long)]
    fast_open: bool,

    /// Ignore proxy settings from environment variables when connecting to the websocket server
    #[clap(short = 'E', long)]
    no_env_proxy: bool,
}

impl CLI {
    /// Creates a new CLI instance
    pub fn new() -> Self {
        CLI { app: App::parse() }
    }

    /// Executes the CLI application
    pub fn execute(&self) -> Result<(), Box<dyn Error>> {
        match &self.app.command {
            Commands::Version => {
                println!("rusocks version {} {}", VERSION, PLATFORM);
                Ok(())
            }
            Commands::Client {
                token,
                url,
                reverse,
                connector_token,
                socks_host,
                socks_port,
                socks_username,
                socks_password,
                socks_no_wait,
                no_reconnect,
                debug,
                threads,
                upstream_proxy,
                fast_open,
                no_env_proxy,
            } => {
                self.init_logging(*debug);
                self.run_client(
                    token.clone(),
                    url,
                    *reverse,
                    connector_token.clone(),
                    socks_host,
                    *socks_port,
                    socks_username.clone(),
                    socks_password.clone(),
                    *socks_no_wait,
                    *no_reconnect,
                    *threads,
                    upstream_proxy.clone(),
                    *fast_open,
                    *no_env_proxy,
                )
            }
            Commands::Connector { client_args } => {
                self.init_logging(client_args.debug);
                self.run_client(
                    client_args.token.clone(),
                    &client_args.url,
                    client_args.reverse,
                    client_args.connector_token.clone(),
                    &client_args.socks_host,
                    client_args.socks_port,
                    client_args.socks_username.clone(),
                    client_args.socks_password.clone(),
                    client_args.socks_no_wait,
                    client_args.no_reconnect,
                    client_args.threads,
                    client_args.upstream_proxy.clone(),
                    client_args.fast_open,
                    client_args.no_env_proxy,
                )
            }
            Commands::Provider { client_args } => {
                self.init_logging(client_args.debug);
                // Force reverse to true for provider
                self.run_client(
                    client_args.token.clone(),
                    &client_args.url,
                    true, // Always use reverse mode
                    client_args.connector_token.clone(),
                    &client_args.socks_host,
                    client_args.socks_port,
                    client_args.socks_username.clone(),
                    client_args.socks_password.clone(),
                    client_args.socks_no_wait,
                    client_args.no_reconnect,
                    client_args.threads,
                    client_args.upstream_proxy.clone(),
                    client_args.fast_open,
                    client_args.no_env_proxy,
                )
            }
            Commands::Server {
                ws_host,
                ws_port,
                token,
                connector_token,
                connector_autonomy,
                buffer_size,
                reverse,
                socks_host,
                socks_port,
                socks_username,
                socks_password,
                socks_nowait,
                debug,
                api_key,
                upstream_proxy,
                fast_open,
            } => {
                self.init_logging(*debug);
                self.run_server(
                    ws_host,
                    *ws_port,
                    token.clone(),
                    connector_token.clone(),
                    *connector_autonomy,
                    *buffer_size,
                    *reverse,
                    socks_host,
                    *socks_port,
                    socks_username.clone(),
                    socks_password.clone(),
                    *socks_nowait,
                    api_key.clone(),
                    upstream_proxy.clone(),
                    *fast_open,
                )
            }
        }
    }

    /// Initialize logging with appropriate level
    fn init_logging(&self, debug_level: u8) {
        let level = match debug_level {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        };

        env_logger::Builder::new()
            .format_timestamp_millis()
            .filter_level(level)
            .init();
    }

    /// Parse SOCKS5 proxy URL and return address, username, and password
    fn parse_socks_proxy(
        &self,
        proxy_url: Option<String>,
    ) -> Result<(Option<String>, Option<String>, Option<String>), Box<dyn Error>> {
        if let Some(url_str) = proxy_url {
            let url = Url::parse(&url_str)?;

            if url.scheme() != "socks5" {
                return Err(format!("Unsupported proxy scheme: {}", url.scheme()).into());
            }

            let username = if !url.username().is_empty() {
                Some(url.username().to_string())
            } else {
                None
            };

            let password = url.password().map(|s| s.to_string());

            let host = url.host_str().ok_or("Missing host in proxy URL")?;
            let port = url.port().unwrap_or(9870);
            let address = format!("{}:{}", host, port);

            Ok((Some(address), username, password))
        } else {
            Ok((None, None, None))
        }
    }

    /// Run the client with the given options
    #[tokio::main]
    async fn run_client(
        &self,
        token: Option<String>,
        url: &str,
        reverse: bool,
        connector_token: Option<String>,
        socks_host: &str,
        socks_port: u16,
        socks_username: Option<String>,
        socks_password: Option<String>,
        socks_no_wait: bool,
        no_reconnect: bool,
        threads: u32,
        upstream_proxy: Option<String>,
        fast_open: bool,
        no_env_proxy: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Parse proxy URL
        let (proxy_addr, proxy_user, proxy_pass) = self.parse_socks_proxy(upstream_proxy)?;

        // Create client options
        let mut client_opt = ClientOption::default()
            .with_ws_url(url.to_string())
            .with_reverse(reverse)
            .with_socks_host(socks_host.to_string())
            .with_socks_port(socks_port)
            .with_socks_wait_server(!socks_no_wait)
            .with_reconnect(!no_reconnect)
            .with_threads(threads)
            .with_no_env_proxy(no_env_proxy);

        // Add new options
        if let Some(addr) = proxy_addr {
            client_opt = client_opt.with_upstream_proxy(addr);
            if let Some(user) = proxy_user {
                client_opt = client_opt.with_upstream_auth(user, proxy_pass.unwrap_or_default());
            }
        }

        if fast_open {
            client_opt = client_opt.with_fast_open(true);
        }

        // Add authentication options if provided
        if let Some(username) = socks_username {
            client_opt = client_opt.with_socks_username(username);
        }

        if let Some(password) = socks_password {
            client_opt = client_opt.with_socks_password(password);
        }

        // Create client instance
        let token = token.unwrap_or_default();
        let client = LinkSocksClient::new(token, client_opt);

        // Wait for client to be ready
        if let Err(err) = client.wait_ready().await {
            error!("Exit due to error: {}", err);
            return Err(err.into());
        }

        // Add connector token if provided
        if let Some(conn_token) = connector_token {
            if reverse {
                if let Err(err) = client.add_connector(&conn_token).await {
                    error!("Failed to add connector token: {}", err);
                    return Err(err.into());
                }
            }
        }

        // Wait for Ctrl+C signal
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutting down client...");
                client.close().await;
                // Allow time for log messages to be written before exit
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
            Err(err) => {
                error!("Error waiting for Ctrl+C: {}", err);
                Err(err.into())
            }
        }
    }

    /// Run the server with the given options
    #[tokio::main]
    async fn run_server(
        &self,
        ws_host: &str,
        ws_port: u16,
        token: Option<String>,
        connector_token: Option<String>,
        connector_autonomy: bool,
        buffer_size: usize,
        reverse: bool,
        socks_host: &str,
        socks_port: u16,
        socks_username: Option<String>,
        socks_password: Option<String>,
        _socks_nowait: bool,
        api_key: Option<String>,
        upstream_proxy: Option<String>,
        fast_open: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Parse proxy URL
        let (proxy_addr, proxy_user, proxy_pass) = self.parse_socks_proxy(upstream_proxy)?;

        // Create server options
        let mut server_opt = ServerOption::default()
            .with_ws_host(ws_host.to_string())
            .with_ws_port(ws_port)
            .with_socks_host(socks_host.to_string())
            .with_buffer_size(buffer_size);

        // Add new options
        if let Some(addr) = proxy_addr {
            server_opt = server_opt.with_upstream_proxy(addr);
            if let Some(user) = proxy_user {
                server_opt = server_opt.with_upstream_auth(user, proxy_pass.unwrap_or_default());
            }
        }

        if fast_open {
            server_opt = server_opt.with_fast_open(true);
        }

        // Add API key if provided
        if let Some(ref key) = api_key {
            server_opt = server_opt.with_api(key.clone());
        }

        // Create server instance
        let server = LinkSocksServer::new(server_opt);

        // Skip token operations if API key is provided
        if api_key.is_none() {
            // Add token based on mode
            if reverse {
                let reverse_opts = ReverseTokenOptions {
                    token: token.clone(),
                    port: Some(socks_port),
                    username: socks_username.clone(),
                    password: socks_password.clone(),
                    allow_manage_connector: connector_autonomy,
                };

                let result = server.add_reverse_token(reverse_opts).await?;
                let use_token = result.token;
                let port = result.port.ok_or("Failed to allocate port")?;

                if port == 0 {
                    return Err(format!(
                        "Cannot allocate SOCKS5 port: {}:{}",
                        socks_host, socks_port
                    )
                    .into());
                }

                let mut use_connector_token = String::new();
                if !connector_autonomy {
                    use_connector_token = server
                        .add_connector_token(connector_token, &use_token)
                        .await?;
                }

                info!("Configuration:");
                info!("  Mode: reverse proxy (SOCKS5 on server -> client -> network)");
                info!("  Token: {}", use_token);
                info!("  SOCKS5 port: {}", port);

                if !connector_autonomy {
                    info!("  Connector Token: {}", use_connector_token);
                }

                if socks_username.is_some() && socks_password.is_some() {
                    info!("  SOCKS5 username: {}", socks_username.unwrap());
                }

                if connector_autonomy {
                    info!("  Connector autonomy: enabled");
                }
            } else {
                let use_token = server.add_forward_token(token).await?;
                info!("Configuration:");
                info!("  Mode: forward proxy (SOCKS5 on client -> server -> network)");
                info!("  Token: {}", use_token);
            }
        }

        // Wait for server to be ready
        server.wait_ready().await?;

        // Wait for Ctrl+C signal
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutting down server...");
                server.close().await;
                // Allow time for log messages to be written before exit
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
            Err(err) => {
                error!("Error waiting for Ctrl+C: {}", err);
                Err(err.into())
            }
        }
    }
}
