# adp-ci-types

Shared types for the ADP CI runner system.

## Types

| Type | Purpose |
|------|---------|
| `CiEvent` | SSE events from adp → runners (5 variants) |
| `RunnerEvent` | REST callbacks from runners → adp (5 variants) |
| `JobRequest` | Full job specification (trigger, steps, secrets, compliance) |
| `JobResult` | Job outcome (status, metrics, artifacts, step results) |
| `JobStatus` | Lifecycle: `Pending → Assigned → Running → Completed/Failed/Cancelled` |
| `RunnerCapabilities` | Runner environment, platforms, compliance, labels |
| `SecretMount` | K8s-pattern secret injection (tmpfs files + env vars) |
| `ArtifactRef` / `ArtifactStorage` | Build outputs (Git LFS default, S3/GCS configurable) |
| `CiWorkflow` | Workflow definition stored as JSON at `refs/ci/workflows/{name}` |

## Usage

```rust
use adp_ci_types::{CiEvent, JobRequest, JobStatus, SecretMount};

let event = CiEvent::JobAssigned {
    job_id: "j-1".into(),
    deadline: "2026-04-24T01:00:00Z".into(),
    secret_refs: vec![],
};
```

## Tests

14 unit tests + 8 integration tests.
