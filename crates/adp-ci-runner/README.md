# adp-ci-runner

CI runner execution engine — the library that runner binaries link against.

## Responsibilities

- **Workspace isolation**: each job gets a clean temp directory
- **Git checkout**: clone repo at commit SHA, init submodules
- **Secret mounting**: K8s pattern (tmpfs files + env vars via `SecretMount`)
- **Step execution**: run each `JobStep` with timeout enforcement, stop-on-failure
- **Log streaming**: capture stdout/stderr with offset tracking
- **Git ref write-back**: persist results/logs to `refs/ci/jobs/{job_id}/*`

## Modules

| Module | Purpose |
|--------|---------|
| `executor` | `JobExecutor` — runs a complete `JobRequest` through its steps |
| `workspace` | Isolated temp dirs (workspace/secrets/artifacts), git checkout, cleanup on drop |
| `secrets` | `SecretMounter` — K8s-pattern: base64 decode → tmpfs files + env vars |
| `log_stream` | `LogStreamer` — append-only stdout/stderr, offset tracking, historical read |
| `ref_writer` | `RefWriter` — persistent audit trail at `refs/ci/jobs/{job_id}/*` |

## Usage

```rust
use adp_ci_runner::{JobExecutor, ExecutorConfig};

let executor = JobExecutor::new(ExecutorConfig {
    enable_git_checkout: false,
    ..ExecutorConfig::default()
});

let result = executor.execute_job(&request).await;
assert_eq!(result.status, JobStatus::Completed);
```

## Secret Mounting (Kubernetes Pattern)

```json
{
  "name": "deploy-credentials",
  "data": { "db-password": "czNjcmV0" },
  "env_mapping": { "DB_PASSWORD": "db-password" },
  "mount_path": "/run/secrets/deploy-credentials",
  "tmpfs": true
}
```

Files are created in the mount directory, environment variables injected into each step.

## Tests

24 unit tests.
