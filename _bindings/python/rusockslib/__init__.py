"""
Python bindings for rusocks - a SOCKS proxy implementation over WebSocket protocol.

This module provides low-level bindings to the Rust implementation of rusocks.
It is not intended to be used directly, but rather through the higher-level
Python API provided by the `rusocks` package.
"""

import sys
import os
import ctypes
import platform
from typing import List, Optional, Union, Any

# Define the rusocks class to match the stub
class rusocks:
    """
    Low-level bindings to the Rust implementation of rusocks.
    """
    
    @staticmethod
    def init_global_runtime():
        """
        Initialize the global Tokio runtime for async operations.
        This must be called before using any other functions.
        """
        _check_lib_loaded()
        return _lib.init_global_runtime()
    
    class ClientOptions:
        """
        Options for creating a new Client instance.
        """
        def __init__(self):
            self.ws_url = "ws://localhost:8765"
            self.reverse = False
            self.socks_host = "127.0.0.1"
            self.socks_port = 9870
            self.socks_username = None
            self.socks_password = None
            self.socks_wait_server = True
            self.reconnect = True
            self.reconnect_delay = 5 * SECOND
            self.buffer_size = 8192
            self.channel_timeout = 30 * SECOND
            self.connect_timeout = 10 * SECOND
            self.threads = 1
            self.fast_open = False
            self.upstream_proxy = None
            self.upstream_username = None
            self.upstream_password = None
            self.no_env_proxy = False
            self.user_agent = None
            self.logger = None
    
    class Client:
        """
        Client for connecting to a rusocks server.
        """
        def __init__(self, token, opt):
            """
            Create a new Client instance.
            
            Args:
                token: Authentication token for the server
                opt: ClientOptions instance
            """
            _check_lib_loaded()
            self._handle = _lib.new_client(token, 
                                          opt.ws_url,
                                          opt.reverse,
                                          opt.socks_host,
                                          opt.socks_port,
                                          opt.socks_username or "",
                                          opt.socks_password or "",
                                          opt.socks_wait_server,
                                          opt.reconnect,
                                          opt.reconnect_delay,
                                          opt.buffer_size,
                                          opt.channel_timeout,
                                          opt.connect_timeout,
                                          opt.threads,
                                          opt.fast_open,
                                          opt.upstream_proxy or "",
                                          opt.upstream_username or "",
                                          opt.upstream_password or "",
                                          opt.no_env_proxy,
                                          opt.user_agent or "",
                                          opt.logger._handle if opt.logger else 0)
        
        def wait_ready(self, ctx, timeout=0):
            """
            Wait for the client to be ready.
            
            Args:
                ctx: ContextWithCancel instance
                timeout: Timeout in Duration units (0 = no timeout)
            """
            _check_lib_loaded()
            return _lib.client_wait_ready(self._handle, ctx._handle, timeout)
        
        def add_connector(self, connector_token):
            """
            Add a connector token for reverse proxy.
            
            Args:
                connector_token: Connector token string (empty for auto-generation)
                
            Returns:
                The connector token string
            """
            _check_lib_loaded()
            result = _lib.client_add_connector(self._handle, connector_token)
            return result
        
        def close(self):
            """
            Close the client and clean up resources.
            """
            _check_lib_loaded()
            _lib.client_close(self._handle)
    
    class ContextWithCancel:
        """
        Context with cancel functionality for async operations.
        """
        def __init__(self):
            _check_lib_loaded()
            self._handle = _lib.new_context_with_cancel()
        
        def cancel(self):
            """
            Cancel the context.
            """
            _check_lib_loaded()
            _lib.context_cancel(self._handle)
    
    class ServerOptions:
        """
        Options for creating a new Server instance.
        """
        def __init__(self):
            self.ws_host = "0.0.0.0"
            self.ws_port = 8765
            self.socks_host = "127.0.0.1"
            self.port_pool_start = 10000
            self.port_pool_end = 20000
            self.socks_wait_client = True
            self.buffer_size = 8192
            self.api_key = None
            self.channel_timeout = 30 * SECOND
            self.connect_timeout = 10 * SECOND
            self.fast_open = False
            self.upstream_proxy = None
            self.upstream_username = None
            self.upstream_password = None
            self.logger = None
    
    class Server:
        """
        Server for accepting rusocks connections.
        """
        def __init__(self, opt):
            """
            Create a new Server instance.
            
            Args:
                opt: ServerOptions instance
            """
            _check_lib_loaded()
            self._handle = _lib.new_server(opt.ws_host,
                                          opt.ws_port,
                                          opt.socks_host,
                                          opt.port_pool_start,
                                          opt.port_pool_end,
                                          opt.socks_wait_client,
                                          opt.buffer_size,
                                          opt.api_key or "",
                                          opt.channel_timeout,
                                          opt.connect_timeout,
                                          opt.fast_open,
                                          opt.upstream_proxy or "",
                                          opt.upstream_username or "",
                                          opt.upstream_password or "",
                                          opt.logger._handle if opt.logger else 0)
        
        def add_reverse_token(self, opts):
            """
            Add a reverse token to the server.
            
            Args:
                opts: ReverseTokenOptions instance
                
            Returns:
                ReverseTokenResult instance
            """
            _check_lib_loaded()
            token, port = _lib.server_add_reverse_token(self._handle, 
                                                      opts.token or "",
                                                      opts.port or 0,
                                                      opts.username or "",
                                                      opts.password or "",
                                                      opts.allow_manage_connector)
            return {"token": token, "port": port}
        
        def add_forward_token(self, token=None):
            """
            Add a forward token to the server.
            
            Args:
                token: Token string (None for auto-generation)
                
            Returns:
                The token string
            """
            _check_lib_loaded()
            return _lib.server_add_forward_token(self._handle, token or "")
        
        def add_connector_token(self, connector_token, reverse_token):
            """
            Add a connector token to the server.
            
            Args:
                connector_token: Connector token string (None for auto-generation)
                reverse_token: Reverse token string
                
            Returns:
                The connector token string
            """
            _check_lib_loaded()
            return _lib.server_add_connector_token(self._handle, 
                                                 connector_token or "", 
                                                 reverse_token)
        
        def serve(self):
            """
            Start the server.
            """
            _check_lib_loaded()
            return _lib.server_serve(self._handle)
        
        def wait_ready(self):
            """
            Wait for the server to be ready.
            """
            _check_lib_loaded()
            return _lib.server_wait_ready(self._handle)
        
        def close(self):
            """
            Close the server and clean up resources.
            """
            _check_lib_loaded()
            _lib.server_close(self._handle)
    
    class ReverseTokenOptions:
        """
        Options for creating a reverse token.
        """
        def __init__(self):
            self.token = None
            self.port = None
            self.username = None
            self.password = None
            self.allow_manage_connector = False
    
    class PythonLogger:
        """
        Logger for Python bindings.
        """
        def __init__(self, logger_id, level=None):
            _check_lib_loaded()
            self._handle = _lib.new_python_logger(logger_id)
            if level is not None:
                self.set_level(level)
        
        @staticmethod
        def new(logger_id):
            """
            Create a new logger with the given ID.
            
            Args:
                logger_id: Logger ID string
                
            Returns:
                PythonLogger instance
            """
            return rusocks.PythonLogger(logger_id)
        
        def set_level(self, level):
            """
            Set the log level.
            
            Args:
                level: Log level (Level enum value)
            """
            _check_lib_loaded()
            _lib.python_logger_set_level(self._handle, level)
        
        def log(self, level, message):
            """
            Log a message at the specified level.
            
            Args:
                level: Log level (Level enum value)
                message: Message string
            """
            _check_lib_loaded()
            _lib.python_logger_log(self._handle, level, message)
        
        def trace(self, message):
            """
            Log a trace message.
            
            Args:
                message: Message string
            """
            self.log(Level.Trace, message)
        
        def debug(self, message):
            """
            Log a debug message.
            
            Args:
                message: Message string
            """
            self.log(rusocks.Level.Debug, message)
        
        def info(self, message):
            """
            Log an info message.
            
            Args:
                message: Message string
            """
            self.log(rusocks.Level.Info, message)
        
        def warn(self, message):
            """
            Log a warning message.
            
            Args:
                message: Message string
            """
            self.log(rusocks.Level.Warn, message)
        
        def error(self, message):
            """
            Log an error message.
            
            Args:
                message: Message string
            """
            self.log(rusocks.Level.Error, message)
    
    class Level:
        """
        Log levels.
        """
        Debug = 0
        Info = 1
        Warn = 2
        Error = 3
        Trace = 4
    
    @staticmethod
    def set_logger_global_level(level):
        """
        Set the global log level.
        
        Args:
            level: Log level (Level enum value)
        """
        _check_lib_loaded()
        _lib.set_logger_global_level(level)
    
    @staticmethod
    def parse_duration(value):
        """
        Parse a duration string.
        
        Args:
            value: Duration string (e.g., "1s", "500ms")
            
        Returns:
            Duration value in nanoseconds
        """
        _check_lib_loaded()
        return _lib.parse_duration(value)
    
    @staticmethod
    def wait_for_log_entries(ms):
        """
        Wait for log entries with timeout.
        
        Args:
            ms: Timeout in milliseconds
            
        Returns:
            List of LogEntry objects
        """
        _check_lib_loaded()
        return _lib.wait_for_log_entries(ms)
    
    @staticmethod
    def cancel_log_waiters():
        """
        Cancel all waiting log listeners.
        """
        _check_lib_loaded()
        _lib.cancel_log_waiters()

# Time constants
NANOSECOND = 1
MICROSECOND = 1000
MILLISECOND = 1000000
SECOND = 1000000000
MINUTE = 60 * SECOND
HOUR = 60 * MINUTE

# Global library handle
_lib = None

def _check_lib_loaded():
    """
    Check if the native library is loaded, and load it if not.
    """
    global _lib
    if _lib is None:
        _load_lib()

def _load_lib():
    """
    Load the native library.
    """
    global _lib
    
    # This is a placeholder for the actual library loading mechanism
    # In a real implementation, this would load the compiled Rust library
    # using ctypes or another FFI mechanism
    
    # For now, we'll create a mock implementation that satisfies the interface
    class MockLib:
        def init_global_runtime(self):
            pass
        
        def new_client(self, *args):
            return 1  # Mock handle
        
        def client_wait_ready(self, handle, ctx_handle, timeout):
            return None
        
        def client_add_connector(self, handle, connector_token):
            return connector_token or "generated_token"
        
        def client_close(self, handle):
            pass
        
        def new_context_with_cancel(self):
            return 1  # Mock handle
        
        def context_cancel(self, handle):
            pass
        
        def new_server(self, *args):
            return 1  # Mock handle
        
        def server_add_reverse_token(self, handle, token, port, username, password, allow_manage):
            return (token or "generated_token", port or 10000)
        
        def server_add_forward_token(self, handle, token):
            return token or "generated_token"
        
        def server_add_connector_token(self, handle, connector_token, reverse_token):
            return connector_token or "generated_connector_token"
        
        def server_serve(self, handle):
            return None
        
        def server_wait_ready(self, handle):
            return None
        
        def server_close(self, handle):
            pass
        
        def new_python_logger(self, logger_id):
            return 1  # Mock handle
        
        def python_logger_set_level(self, handle, level):
            pass
        
        def python_logger_log(self, handle, level, message):
            pass
        
        def set_logger_global_level(self, level):
            pass
        
        def parse_duration(self, value):
            return 0
        
        def wait_for_log_entries(self, ms):
            return []
        
        def cancel_log_waiters(self):
            pass
    
    _lib = MockLib()
