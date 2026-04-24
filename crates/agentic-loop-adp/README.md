# agentic-loop-adp

Unified client for agentic-loop services connecting to an ADP server.

Combines SSE client, registration, and entity writing into a single interface.

## Usage

```rust
use agentic_loop_adp::{AdpClient, AdpConfig};

let client = AdpClient::new(AdpConfig {
    adp_url: "http://localhost:8090".into(),
    service_id: "agent-1".into(),
    service_type: ServiceType::AgenticLoop,
    secret: "base64-secret".into(),
});

// Register with ADP
client.register().await?;

// Connect SSE for real-time events
let mut rx = client.connect_sse().await?;

// Write an entity to git refs
client.write_entity("engram-task", &entity_data).await?;
```

## Tests

9 unit tests.
