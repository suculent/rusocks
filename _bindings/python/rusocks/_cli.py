"""
Command-line interface for rusocks.

This module provides a CLI for the rusocks package, allowing users to start
servers and clients from the command line.
"""

import asyncio
import click
import logging
import os
import signal
import sys
from typing import Optional, List, Dict, Any

from rich.console import Console
from rich.logging import RichHandler
from loguru import logger as loguru_logger

from . import Server, Client, set_log_level
from ._base import _to_duration

# Set up logging
console = Console()
logging.basicConfig(
    level=logging.INFO,
    format="%(message)s",
    datefmt="[%X]",
    handlers=[RichHandler(console=console, rich_tracebacks=True)]
)
log = logging.getLogger("rusocks")

# Global state
_running_servers: List[Server] = []
_running_clients: List[Client] = []


def _setup_logging(verbose: bool) -> None:
    """Set up logging based on verbosity level."""
    level = logging.DEBUG if verbose else logging.INFO
    log.setLevel(level)
    set_log_level(level)


def _handle_sigint(signum, frame):
    """Handle SIGINT (Ctrl+C) gracefully."""
    console.print("\n[yellow]Received interrupt signal. Shutting down...[/yellow]")
    for server in _running_servers:
        try:
            server.close()
        except Exception as e:
            console.print(f"[red]Error closing server: {e}[/red]")
    
    for client in _running_clients:
        try:
            client.close()
        except Exception as e:
            console.print(f"[red]Error closing client: {e}[/red]")
    
    sys.exit(0)


# Register signal handler
signal.signal(signal.SIGINT, _handle_sigint)


@click.group()
@click.version_option()
def cli():
    """Rusocks: SOCKS5 over WebSocket proxy tool."""
    pass


@cli.command()
@click.option("-t", "--token", help="Authentication token")
@click.option("-r", "--reverse", is_flag=True, help="Enable reverse proxy mode")
@click.option("-p", "--port", type=int, default=8765, help="WebSocket server port")
@click.option("--host", default="0.0.0.0", help="WebSocket server host")
@click.option("--socks-host", default="127.0.0.1", help="SOCKS5 server host (for reverse mode)")
@click.option("--api", help="API key for HTTP management interface")
@click.option("--buffer-size", type=int, help="Buffer size for data transfer")
@click.option("--channel-timeout", help="Timeout for WebSocket channels (e.g., '30s')")
@click.option("--connect-timeout", help="Timeout for outbound connections (e.g., '5s')")
@click.option("--fast-open", is_flag=True, help="Assume connection success and allow data transfer immediately")
@click.option("--upstream-proxy", help="Upstream proxy address for chaining")
@click.option("--upstream-username", help="Username for upstream proxy authentication")
@click.option("--upstream-password", help="Password for upstream proxy authentication")
@click.option("-v", "--verbose", is_flag=True, help="Enable verbose logging")
def server(
    token: Optional[str],
    reverse: bool,
    port: int,
    host: str,
    socks_host: str,
    api: Optional[str],
    buffer_size: Optional[int],
    channel_timeout: Optional[str],
    connect_timeout: Optional[str],
    fast_open: bool,
    upstream_proxy: Optional[str],
    upstream_username: Optional[str],
    upstream_password: Optional[str],
    verbose: bool,
):
    """Start a SOCKS5 over WebSocket server."""
    _setup_logging(verbose)
    
    # Create server options
    server_opts: Dict[str, Any] = {
        "ws_host": host,
        "ws_port": port,
        "logger": log,
    }
    
    if reverse:
        server_opts["socks_host"] = socks_host
    
    if api:
        server_opts["api_key"] = api
    
    if buffer_size:
        server_opts["buffer_size"] = buffer_size
    
    if channel_timeout:
        server_opts["channel_timeout"] = channel_timeout
    
    if connect_timeout:
        server_opts["connect_timeout"] = connect_timeout
    
    if fast_open:
        server_opts["fast_open"] = True
    
    if upstream_proxy:
        server_opts["upstream_proxy"] = upstream_proxy
        
    if upstream_username:
        server_opts["upstream_username"] = upstream_username
        
    if upstream_password:
        server_opts["upstream_password"] = upstream_password
    
    # Create and start server
    server = Server(**server_opts)
    _running_servers.append(server)
    
    if token:
        if reverse:
            result = server.add_reverse_token(token=token)
            console.print(f"[green]Added reverse token: {result.token} on port {result.port}[/green]")
        else:
            server.add_forward_token(token)
            console.print(f"[green]Added forward token: {token}[/green]")
    
    console.print(f"[green]Server started on {host}:{port}[/green]")
    console.print("[yellow]Press Ctrl+C to stop[/yellow]")
    
    try:
        # Keep the server running
        server.wait_ready()
        # Block until interrupted
        while True:
            try:
                asyncio.get_event_loop().run_until_complete(asyncio.sleep(1))
            except KeyboardInterrupt:
                break
    finally:
        server.close()


@cli.command()
@click.option("-t", "--token", required=True, help="Authentication token")
@click.option("-u", "--url", required=True, help="WebSocket server URL")
@click.option("-r", "--reverse", is_flag=True, help="Enable reverse proxy mode")
@click.option("--socks-host", default="127.0.0.1", help="SOCKS5 server host (for forward mode)")
@click.option("--socks-port", type=int, default=1080, help="SOCKS5 server port (for forward mode)")
@click.option("--socks-username", help="SOCKS5 authentication username")
@click.option("--socks-password", help="SOCKS5 authentication password")
@click.option("--reconnect", is_flag=True, help="Automatically reconnect on disconnection")
@click.option("--reconnect-delay", help="Delay between reconnection attempts (e.g., '5s')")
@click.option("--buffer-size", type=int, help="Buffer size for data transfer")
@click.option("--channel-timeout", help="Timeout for WebSocket channels (e.g., '30s')")
@click.option("--connect-timeout", help="Timeout for outbound connections (e.g., '5s')")
@click.option("--threads", type=int, help="Number of threads for concurrent processing")
@click.option("--fast-open", is_flag=True, help="Assume connection success and allow data transfer immediately")
@click.option("--upstream-proxy", help="Upstream proxy address for chaining")
@click.option("--upstream-username", help="Username for upstream proxy authentication")
@click.option("--upstream-password", help="Password for upstream proxy authentication")
@click.option("--no-env-proxy", is_flag=True, help="Ignore proxy environment variables")
@click.option("-v", "--verbose", is_flag=True, help="Enable verbose logging")
def client(
    token: str,
    url: str,
    reverse: bool,
    socks_host: str,
    socks_port: int,
    socks_username: Optional[str],
    socks_password: Optional[str],
    reconnect: bool,
    reconnect_delay: Optional[str],
    buffer_size: Optional[int],
    channel_timeout: Optional[str],
    connect_timeout: Optional[str],
    threads: Optional[int],
    fast_open: bool,
    upstream_proxy: Optional[str],
    upstream_username: Optional[str],
    upstream_password: Optional[str],
    no_env_proxy: bool,
    verbose: bool,
):
    """Start a SOCKS5 over WebSocket client."""
    _setup_logging(verbose)
    
    # Create client options
    client_opts: Dict[str, Any] = {
        "ws_url": url,
        "reverse": reverse,
        "logger": log,
    }
    
    if not reverse:
        client_opts["socks_host"] = socks_host
        client_opts["socks_port"] = socks_port
    
    if socks_username:
        client_opts["socks_username"] = socks_username
        
    if socks_password:
        client_opts["socks_password"] = socks_password
    
    if reconnect:
        client_opts["reconnect"] = True
        
    if reconnect_delay:
        client_opts["reconnect_delay"] = reconnect_delay
    
    if buffer_size:
        client_opts["buffer_size"] = buffer_size
    
    if channel_timeout:
        client_opts["channel_timeout"] = channel_timeout
    
    if connect_timeout:
        client_opts["connect_timeout"] = connect_timeout
    
    if threads:
        client_opts["threads"] = threads
    
    if fast_open:
        client_opts["fast_open"] = True
    
    if upstream_proxy:
        client_opts["upstream_proxy"] = upstream_proxy
        
    if upstream_username:
        client_opts["upstream_username"] = upstream_username
        
    if upstream_password:
        client_opts["upstream_password"] = upstream_password
    
    if no_env_proxy:
        client_opts["no_env_proxy"] = True
    
    # Create and start client
    client = Client(token, **client_opts)
    _running_clients.append(client)
    
    console.print(f"[green]Client connected to {url}[/green]")
    if not reverse:
        console.print(f"[green]SOCKS5 server listening on {socks_host}:{socks_port}[/green]")
    console.print("[yellow]Press Ctrl+C to stop[/yellow]")
    
    try:
        # Keep the client running
        client.wait_ready()
        # Block until interrupted
        while True:
            try:
                asyncio.get_event_loop().run_until_complete(asyncio.sleep(1))
            except KeyboardInterrupt:
                break
    finally:
        client.close()


@cli.command()
@click.option("-t", "--token", required=True, help="Authentication token")
@click.option("-u", "--url", required=True, help="WebSocket server URL")
@click.option("--reconnect", is_flag=True, help="Automatically reconnect on disconnection")
@click.option("--reconnect-delay", help="Delay between reconnection attempts (e.g., '5s')")
@click.option("--buffer-size", type=int, help="Buffer size for data transfer")
@click.option("--channel-timeout", help="Timeout for WebSocket channels (e.g., '30s')")
@click.option("--connect-timeout", help="Timeout for outbound connections (e.g., '5s')")
@click.option("--threads", type=int, help="Number of threads for concurrent processing")
@click.option("--fast-open", is_flag=True, help="Assume connection success and allow data transfer immediately")
@click.option("--upstream-proxy", help="Upstream proxy address for chaining")
@click.option("--upstream-username", help="Username for upstream proxy authentication")
@click.option("--upstream-password", help="Password for upstream proxy authentication")
@click.option("--no-env-proxy", is_flag=True, help="Ignore proxy environment variables")
@click.option("-v", "--verbose", is_flag=True, help="Enable verbose logging")
def provider(
    token: str,
    url: str,
    reconnect: bool,
    reconnect_delay: Optional[str],
    buffer_size: Optional[int],
    channel_timeout: Optional[str],
    connect_timeout: Optional[str],
    threads: Optional[int],
    fast_open: bool,
    upstream_proxy: Optional[str],
    upstream_username: Optional[str],
    upstream_password: Optional[str],
    no_env_proxy: bool,
    verbose: bool,
):
    """Start a reverse proxy client (alias for 'client -r')."""
    # Call client command with reverse flag
    ctx = click.Context(client)
    client.callback(
        token=token,
        url=url,
        reverse=True,  # Force reverse mode
        socks_host="127.0.0.1",  # Default value, not used in reverse mode
        socks_port=1080,  # Default value, not used in reverse mode
        socks_username=None,
        socks_password=None,
        reconnect=reconnect,
        reconnect_delay=reconnect_delay,
        buffer_size=buffer_size,
        channel_timeout=channel_timeout,
        connect_timeout=connect_timeout,
        threads=threads,
        fast_open=fast_open,
        upstream_proxy=upstream_proxy,
        upstream_username=upstream_username,
        upstream_password=upstream_password,
        no_env_proxy=no_env_proxy,
        verbose=verbose,
    )


if __name__ == "__main__":
    cli()