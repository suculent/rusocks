# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

Rusocks is a Rust implementation of SOCKS5 over WebSocket proxy. It's a compatible port of the Go-based LinkSocks project, providing both forward and reverse proxy modes with authentication, load balancing, and HTTP API management. The project includes Python bindings via PyO3.

**Protocol Compatibility**: Rusocks implements the same binary protocol as LinkSocks Go, allowing full interoperability - Rusocks clients can connect to LinkSocks Go servers and vice versa.

## Key Architecture Concepts

### Core Components

**Message Protocol** (`src/message.rs`): Custom JSON-based protocol over WebSocket for proxy communication. Key message types:
- `AuthMessage` / `AuthResponseMessage`: WebSocket-level authentication
- `ConnectMessage` / `ConnectResponseMessage`: Establish SOCKS5 channels  
- `DataMessage` / `DisconnectMessage`: Binary data transfer (base64-encoded)
- `ConnectorMessage`: Agent mode coordination

**Client** (`src/client.rs`): Handles WebSocket connections and SOCKS5 server for forward mode or network access provision for reverse mode. Uses `ClientOption` builder pattern for configuration.

**Server** (`src/server.rs`): WebSocket server managing multiple authenticated clients. Maintains token-based authentication with separate forward/reverse/connector token types. Implements SOCKS5 server for reverse mode.

**Relay** (`src/relay.rs`): Core data transfer logic between WebSocket channels and TCP connections. Handles connection lifecycle, timeouts, and optional fast-open mode.

**API** (`src/api.rs`): HTTP REST API for runtime token management when enabled with `--api-key`. Supports dynamic add/remove of tokens without restart.

### Proxy Modes

1. **Forward Proxy**: Client connects to remote WebSocket server, provides local SOCKS5 interface. Traffic flows: `Local App -> SOCKS5 (Client) -> WebSocket -> Server -> Internet`

2. **Reverse Proxy**: Server exposes SOCKS5 interface, clients share their network access. Multiple clients enable load balancing. Traffic flows: `Local App -> SOCKS5 (Server) -> WebSocket -> Client -> Internet`

3. **Agent Mode** (Reverse with Connectors): Server acts as relay between providers (network source) and connectors (SOCKS5 interface). Supports autonomy mode where providers set their own connector tokens.

### Authentication Model

Three-tier authentication system:
- **WebSocket Tokens**: SHA256-hashed, used for client-server WebSocket authentication
- **Connector Tokens**: In agent mode, separate tokens for connector access
- **SOCKS5 Credentials**: Optional username/password on SOCKS5 interface

### Concurrency Architecture

Built on Tokio async runtime with heavy use of:
- `Arc<RwLock<>>` for shared state (tokens, channels)
- `mpsc::channel` for WebSocket message routing
- `Notify` for graceful shutdown signaling
- Separate tasks for listener, SOCKS servers, relay handlers

## Common Commands

### Build & Test
```bash
# Build release binary
cargo build --release

# Build for specific architecture (macOS)
./build-mac-aarch64.sh    # Apple Silicon
./build-mac-x86_64.sh     # Intel Mac

# Build for Windows (cross-compile)
cargo build --release --target x86_64-pc-windows-gnu

# Run tests
cargo test

# Run specific test
cargo test user_agent_test

# Check with clippy
cargo clippy -- -D warnings
```

### Python Bindings
```bash
# Build Python package (from _bindings/python/)
cd _bindings/python
pip install maturin
maturin develop  # Development build
maturin build --release  # Production wheel

# Install from wheel
pip install rusocks

# Run Python tests
pytest
```

### Running the Proxy

**Forward Mode:**
```bash
# Server
./target/release/rusocks server -t <token> -p 8765

# Client with SOCKS5 on port 1080
./target/release/rusocks client -t <token> -u ws://server:8765 -p 1080
```

**Reverse Mode:**
```bash
# Server with SOCKS5 on port 9870
./target/release/rusocks server -t <token> -r -p 9870

# Multiple clients for load balancing
./target/release/rusocks client -t <token> -u ws://server:8765 -r
./target/release/rusocks client -t <token> -u ws://server:8765 -r
```

**With HTTP API:**
```bash
# Server with API enabled
./target/release/rusocks server --api-key secret123 -p 8765

# Add token via API
curl -X POST -H "X-API-Key: secret123" -H "Content-Type: application/json" \
  -d '{"reverse":true}' http://localhost:8765/api/tokens
```

## Development Guidelines

### Code Organization

- **No `main.rs` logic**: CLI parsing in `src/cli.rs`, main is minimal
- **Builder patterns**: All options structs (`ClientOption`, `ServerOption`, `RelayOption`) use `with_*` builder methods
- **PyO3 integration**: Python bindings in `src/python.rs`, use `#[pyclass]` and `#[pymethods]` macros
- **Error handling**: Uses `thiserror` for typed errors, async functions return `Result<T, String>` at boundaries

### Tokio Patterns Used

- **Graceful shutdown**: All long-running tasks take `Arc<Notify>` for stop signaling
- **Channel cleanup**: Close channels before awaiting tasks to prevent deadlocks
- **Select macros**: Use `tokio::select!` with biased when shutdown precedence matters
- **Timeout handling**: Wrap operations with `tokio::time::timeout` for channel/connect timeouts

### WebSocket Message Flow

**Protocol Format**: Binary protocol over WebSocket binary frames (compatible with LinkSocks Go)

1. Client sends `AuthMessage` with token + instance UUID
2. Server validates, responds with `AuthResponseMessage`
3. For each SOCKS5 connection:
   - `ConnectMessage` with channel_id + target address/port
   - `ConnectResponseMessage` with success/failure
   - Bidirectional `DataMessage` stream (raw binary)
   - `DisconnectMessage` to close channel

**Protocol Details**: 
- Uses LinkSocks binary protocol with version/type headers and length-prefixed fields
- All messages start with: Version(1 byte) + Type(1 byte) + payload
- Binary data is transmitted directly without base64 encoding
- Fully compatible with LinkSocks Go implementation

### Platform-Specific Considerations

- **Version reporting**: `src/version.rs` detects OS and architecture at compile time
- **Cross-compilation**: Use `rustup target add` for Windows/Linux targets
- **Path separators**: Code assumes Unix paths, Windows builds need GNU toolchain

### Python API Surface

Key Python classes mirror Rust implementation:
- `Server(ws_host, ws_port, ...)`: Async server with context manager support
- `Client(token, ws_url, reverse=False, ...)`: Async client with auto-reconnect
- Both support `async_*` methods and `with` / `async with` patterns
- Use `set_log_level(level)` for debugging

### Testing

- Unit tests in `src/tests/` directory
- Integration tests should verify:
  - Token authentication (valid/invalid)
  - Forward/reverse proxy modes
  - Load balancing with multiple clients
  - Fast-open mode
  - Upstream proxy chaining
  - HTTP API endpoints
- Use `env_logger::init()` for debug output in tests

## Important Implementation Details

### Security

- Tokens are SHA256-hashed before storage
- WebSocket authentication happens once per connection
- SOCKS5 auth is separate and optional
- API requires `X-API-Key` header for all operations

### Performance Features

- **Fast Open**: Optional mode to send initial data with connect message (reduces RTT)
- **Buffer Size**: Configurable (default 8192), affects memory vs throughput tradeoff
- **Thread Count**: Client option for parallel connection handling (forward mode only)
- **Port Pool**: Server pre-allocates port range for reverse mode SOCKS5 servers

### State Management

- Server maintains `tokens: Arc<RwLock<HashMap>>` for all token types
- Client tracks `is_connected: Arc<AtomicBool>` for reconnection logic
- Relay uses per-channel state machines (Connecting -> Connected -> Disconnected)
- PortPool manages available/allocated ports thread-safely

### Known Limitations

- WebSocket protocol is custom (not standard SOCKS5 over WS)
- No built-in TLS (use wss:// URLs with separate TLS terminator)
- Upstream proxy only supports SOCKS5, not HTTP CONNECT
- Agent mode autonomy requires server started with `-a` flag
