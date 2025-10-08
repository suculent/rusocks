"""
Tests for the rusockslib Python bindings.
"""

import unittest
import sys
import os
from pathlib import Path

# Add the parent directory to the path so we can import rusockslib
sys.path.insert(0, str(Path(__file__).parent.parent))

from rusockslib import rusocks

class TestRusocksLib(unittest.TestCase):
    """Test the rusockslib Python bindings."""
    
    def setUp(self):
        """Set up the test environment."""
        # Initialize the global runtime
        rusocks.init_global_runtime()
    
    def test_client_creation(self):
        """Test creating a client."""
        # Create client options
        options = rusocks.ClientOptions()
        options.ws_url = "ws://localhost:8765"
        options.socks_port = 9870
        
        # Create a client
        client = rusocks.Client("test_token", options)
        
        # Close the client
        client.close()
    
    def test_server_creation(self):
        """Test creating a server."""
        # Create server options
        options = rusocks.ServerOptions()
        options.ws_port = 8765
        
        # Create a server
        server = rusocks.Server(options)
        
        # Close the server
        server.close()
    
    def test_context_with_cancel(self):
        """Test creating and canceling a context."""
        # Create a context
        ctx = rusocks.ContextWithCancel()
        
        # Cancel the context
        ctx.cancel()
    
    def test_python_logger(self):
        """Test creating and using a logger."""
        # Create a logger
        logger = rusocks.PythonLogger.new("test_logger")
        
        # Log some messages
        logger.debug("Debug message")
        logger.info("Info message")
        logger.warn("Warning message")
        logger.error("Error message")
    
    def test_parse_duration(self):
        """Test parsing duration strings."""
        # Parse some durations
        rusocks.parse_duration("1s")
        rusocks.parse_duration("500ms")
    
    def test_log_entries(self):
        """Test waiting for log entries."""
        # Wait for log entries with a short timeout
        entries = rusocks.wait_for_log_entries(100)
        
        # Cancel log waiters
        rusocks.cancel_log_waiters()

if __name__ == "__main__":
    unittest.main()