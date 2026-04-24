# adp-sse-registry

SSE (Server-Sent Events) endpoint for actix-web with connection management and TTL monitoring.

## Features

- **SSE broadcast registry**: manage connections per service type
- **TTL monitoring**: auto-expire stale connections
- **Actix-web integration**: drop-in route configuration

## Usage

```rust
use adp_sse_registry::{create_sse_registry, start_ttl_monitor, configure_sse_routes};

let registry = create_sse_registry();
start_ttl_monitor(registry.clone());

// In your actix config:
configure_sse_routes(cfg);
```

Clients connect to `GET /api/agentic-loop/connect?service_type=<type>&service_id=<id>`.

## Tests

8 unit tests.
