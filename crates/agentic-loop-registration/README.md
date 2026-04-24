# agentic-loop-registration

Service registration with ADP using TTL-based secrets via SOPS+AGE (ADR-024).

## Features

- **Secret generation**: base64-encoded 32-byte random secrets
- **TTL lifecycle**: registration, renewal, expiration
- **SOPS integration**: secrets encrypted with AGE key wrapping

## Usage

```rust
use agentic_loop_registration::{Registration, RegistrationConfig, SecretGenerator};

let secret = SecretGenerator::generate();
let registration = Registration::new(RegistrationConfig {
    adp_url: "http://localhost:8090".into(),
    service_id: "agent-1".into(),
    service_type: ServiceType::AgenticLoop,
    secret,
    ttl: Duration::from_secs(3600),
});

registration.register().await?;
// Renew before expiry
registration.renew().await?;
```

## Tests

12 unit tests.
