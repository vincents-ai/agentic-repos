# agentic-loop-sse-client

Auto-reconnecting SSE client with exponential backoff and heartbeat timeout detection.

## Features

- **Auto-reconnect**: exponential backoff on disconnect (1s → 30s cap)
- **Heartbeat timeout**: detect dead connections (30s default)
- **Event dispatch**: typed event parsing from SSE data

## Usage

```rust
use agentic_loop_sse_client::{SseClient, SseClientConfig};

let client = SseClient::new(SseClientConfig {
    url: "http://localhost:8090/api/agentic-loop/connect".into(),
    service_id: "agent-1".into(),
    service_type: ServiceType::AgenticLoop,
    heartbeat_timeout: Duration::from_secs(30),
});

let mut event_stream = client.connect().await?;
while let Some(event) = event_stream.next().await {
    handle_event(event?);
}
// Auto-reconnects on disconnect
```

## Tests

4 unit tests.
