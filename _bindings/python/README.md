# Rusocks Python Bindings

Python bindings for the Rusocks SOCKS5 over WebSocket proxy tool.

## Overview

Rusocks is a Rust implementation of a SOCKS5 proxy over WebSocket connections, allowing for secure tunneling through firewalls and proxies. These Python bindings provide a convenient way to use Rusocks from Python applications.

## Features

- SOCKS5 proxy over WebSocket
- Forward and reverse proxy modes
- Authentication support
- Multiple client connections
- Load balancing
- HTTP API for management
- Fast open mode for improved performance
- Command-line interface
- Customizable User-Agent for WebSocket connections

## Installation

```bash
pip install rusocks
```

## Usage

### Server Mode

```python
from rusocks import Server

# Create a forward proxy server
server = Server(ws_host="0.0.0.0", ws_port=8765)

# Add a token for authentication
token = server.add_forward_token("my-secret-token")
print(f"Added token: {token}")

# Wait for the server to be ready
server.wait_ready()

# Keep the server running
try:
    # Your application logic here
    input("Press Enter to stop the server...")
finally:
    server.close()
```

### Reverse Server Mode

```python
from rusocks import Server, ReverseTokenResult

# Create a server
server = Server(ws_host="0.0.0.0", ws_port=8765)

# Add a reverse token
result: ReverseTokenResult = server.add_reverse_token(
    token="my-reverse-token",
    port=9870,  # Optional, auto-assigned if not provided
    username="user",  # Optional
    password="pass",  # Optional
    allow_manage_connector=True  # Optional
)
print(f"Added reverse token: {result.token} on port {result.port}")

# Wait for the server to be ready
server.wait_ready()

# Keep the server running
try:
    # Your application logic here
    input("Press Enter to stop the server...")
finally:
    server.close()
```

### Client Mode

```python
from rusocks import Client

# Create a forward proxy client
client = Client(
    token="my-secret-token",
    ws_url="ws://server-address:8765",
    socks_host="127.0.0.1",
    socks_port=1080,
    user_agent="MyApp/1.0 (Custom Client)"  # Optional custom User-Agent
)

# Wait for the client to be ready
client.wait_ready()

# Keep the client running
try:
    # Your application logic here
    input("Press Enter to stop the client...")
finally:
    client.close()
```

### Reverse Client Mode (Provider)

```python
from rusocks import Client

# Create a reverse proxy client
client = Client(
    token="my-reverse-token",
    ws_url="ws://server-address:8765",
    reverse=True
)

# Wait for the client to be ready
client.wait_ready()

# Keep the client running
try:
    # Your application logic here
    input("Press Enter to stop the client...")
finally:
    client.close()
```

### Async Support

All methods have async versions prefixed with `async_`:

```python
import asyncio
from rusocks import Server

async def main():
    server = Server(ws_host="0.0.0.0", ws_port=8765)
    token = await server.async_add_forward_token("my-secret-token")
    print(f"Added token: {token}")
    
    await server.async_wait_ready()
    
    try:
        # Your async application logic here
        await asyncio.sleep(60)  # Run for 60 seconds
    finally:
        await server.async_close()

asyncio.run(main())
```

### Context Manager Support

Both Server and Client classes support context managers:

```python
from rusocks import Server

# Using with statement
with Server(ws_host="0.0.0.0", ws_port=8765) as server:
    server.add_forward_token("my-secret-token")
    # Server is automatically closed when exiting the with block
```

### Async Context Manager Support

```python
import asyncio
from rusocks import Client

async def main():
    async with Client(
        token="my-secret-token",
        ws_url="ws://server-address:8765"
    ) as client:
        # Client is automatically closed when exiting the async with block
        await asyncio.sleep(60)  # Run for 60 seconds

asyncio.run(main())
```

## Command-Line Interface

The package also provides a command-line interface:

### Server Mode

```bash
rusocks server -t my-secret-token -p 8765
```

### Reverse Server Mode

```bash
rusocks server -t my-reverse-token -r -p 8765
```

### Client Mode

```bash
rusocks client -t my-secret-token -u ws://server-address:8765
```

### Reverse Client Mode (Provider)

```bash
rusocks provider -t my-reverse-token -u ws://server-address:8765
```

## API Reference

### Server Class

```python
Server(
    *,
    logger=None,
    ws_host=None,
    ws_port=None,
    socks_host=None,
    port_pool=None,
    socks_wait_client=None,
    buffer_size=None,
    api_key=None,
    channel_timeout=None,
    connect_timeout=None,
    fast_open=None,
    upstream_proxy=None,
    upstream_username=None,
    upstream_password=None,
)
```

#### Methods

- `add_forward_token(token=None) -> str`: Add a forward proxy token
- `async_add_forward_token(token=None) -> str`: Async version of add_forward_token
- `add_reverse_token(*, token=None, port=None, username=None, password=None, allow_manage_connector=None) -> ReverseTokenResult`: Add a reverse proxy token
- `async_add_reverse_token(*, token=None, port=None, username=None, password=None, allow_manage_connector=None) -> ReverseTokenResult`: Async version of add_reverse_token
- `add_connector_token(connector_token, reverse_token) -> str`: Add a connector token for reverse proxy
- `async_add_connector_token(connector_token, reverse_token) -> str`: Async version of add_connector_token
- `remove_token(token) -> bool`: Remove a token from the server
- `async_remove_token(token) -> bool`: Async version of remove_token
- `wait_ready(timeout=None) -> None`: Wait for the server to be ready
- `async_wait_ready(timeout=None) -> None`: Async version of wait_ready
- `close() -> None`: Close the server and clean up resources
- `async_close() -> None`: Async version of close

### Client Class

```python
Client(
    token,
    *,
    logger=None,
    ws_url=None,
    reverse=None,
    socks_host=None,
    socks_port=None,
    socks_username=None,
    socks_password=None,
    socks_wait_server=None,
    reconnect=None,
    reconnect_delay=None,
    buffer_size=None,
    channel_timeout=None,
    connect_timeout=None,
    threads=None,
    fast_open=None,
    upstream_proxy=None,
    upstream_username=None,
    upstream_password=None,
    no_env_proxy=None,
    user_agent=None,  # Custom User-Agent header for WebSocket connections
)
```

#### Methods

- `wait_ready(timeout=None) -> None`: Wait for the client to be ready
- `async_wait_ready(timeout=None) -> None`: Async version of wait_ready
- `add_connector(connector_token=None) -> str`: Add a connector token for reverse proxy
- `async_add_connector(connector_token=None) -> str`: Async version of add_connector
- `close() -> None`: Close the client and clean up resources
- `async_close() -> None`: Async version of close

#### Properties

- `is_connected -> bool`: Check if the client is connected to the server
- `socks_port -> Optional[int]`: Get the SOCKS5 server port (for forward mode)

### ReverseTokenResult Class

```python
@dataclass
class ReverseTokenResult:
    token: str
    port: int
```

### Utility Functions

- `set_log_level(level)`: Set the global log level for rusocks

## License

Same as the Rusocks project.