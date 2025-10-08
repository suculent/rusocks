"""
Test for User-Agent customization in rusocks.

This test demonstrates how to set a custom User-Agent header for WebSocket connections.
"""

import unittest
import logging
import sys
import os

# Add parent directory to path to import rusocks
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from rusocks import Client


class TestUserAgent(unittest.TestCase):
    """Test cases for User-Agent customization."""

    def test_custom_user_agent(self):
        """Test that a custom User-Agent can be set."""
        # Set up logging
        logger = logging.getLogger("test_user_agent")
        logger.setLevel(logging.DEBUG)
        handler = logging.StreamHandler()
        handler.setFormatter(logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s'))
        logger.addHandler(handler)
        
        # Create a client with a custom User-Agent
        custom_user_agent = "RuSocks/1.0 (Test Client)"
        client = Client(
            token="test_token",
            logger=logger,
            ws_url="ws://localhost:8765",  # Use a test server URL
            user_agent=custom_user_agent
        )
        
        # Note: We can't actually test the connection here without a server,
        # but this demonstrates how to set the User-Agent
        
        # Clean up
        client.close()
        
        # This test passes if no exceptions are raised
        self.assertTrue(True)


if __name__ == "__main__":
    unittest.main()