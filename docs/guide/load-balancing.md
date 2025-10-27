# Load Balancing

Reverse proxy mode distributes connections across connected clients.

## How it works
- Per-connection round-robin assignment
- Long-lived connections stay on the same client
- Automatic failover when a client disconnects

## Setup
```bash
# Server
rusocks server -t token -r -p 9870

# Multiple clients
rusocks client -t token -r &
rusocks client -t token -r &
```
