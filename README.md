# Rusocks

A Rust implementation of the LinkSocks SOCKS5 over WebSocket proxy tool.

## Overview

Rusocks is a port of the Go-based [LinkSocks](https://github.com/linksocks/linksocks) project to Rust. It provides a SOCKS5 proxy over WebSocket connections, allowing for secure tunneling through firewalls and proxies.

## Features

- SOCKS5 proxy over WebSocket
- Forward and reverse proxy modes
- Authentication support
- Multiple client connections
- Load balancing
- HTTP API for management
- Fast open mode for improved performance
- Python bindings

## Usage

### Server Mode

```bash
rusocks server -t <token> -r -p 9870
```

This starts a reverse proxy server that listens for WebSocket connections and forwards SOCKS5 traffic.

### Client Mode

```bash
rusocks client -t <token> -u ws://server-address:8765
```

This starts a forward proxy client that connects to a WebSocket server and provides a local SOCKS5 proxy.

### Reverse Client Mode

```bash
rusocks client -t <token> -u ws://server-address:8765 -r
```

This starts a reverse proxy client that connects to a WebSocket server and forwards traffic to the local network.

### Provider Mode (Alias for Reverse Client)

```bash
rusocks provider -t <token> -u ws://server-address:8765
```

This is an alias for the reverse client mode.

## Building

To build the project, you need Rust and Cargo installed. Then run:

```bash
cargo build --release
```

The binary will be available at `target/release/rusocks`.

## Supported Platforms

Rusocks supports the same platforms as the original LinkSocks project. The platform information is reported in the format `os/arch`, where:

- OS can be: `windows`, `darwin` (macOS), `linux`, or other supported OS
- Architecture can be:
  - `x86_64` - 64-bit x86 architecture (most common for desktop/server)
  - `i686` - 32-bit x86 architecture
  - `aarch64` - 64-bit ARM architecture (Apple M1/M2, newer ARM servers)
  - `arm` - 32-bit ARM architecture
  - And other architectures supported by Rust

## Migration Notes

This project is a port of the Go-based LinkSocks project to Rust. The migration preserves the same functionality and API, but takes advantage of Rust's safety features and async runtime.

Key differences:
- Uses Tokio for async I/O
- Uses Rust's type system for improved safety
- Maintains the same command-line interface
- Preserves the same WebSocket protocol

## License

Same as the original LinkSocks project.