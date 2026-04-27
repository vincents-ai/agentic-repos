# adp-ci-runner-bin

ADP CI runner binary — connects to an adp server, receives CI jobs via SSE, executes them, and reports results back via REST.

## Usage

```bash
adp-ci-runner \
  --adp-url http://localhost:8090 \
  --runner-id my-runner-1 \
  --runner-type build_worker \
  --environments nix,sh \
  --platforms linux/amd64 \
  --max-concurrent 4
```

## Configuration

All options support environment variables:

| Flag | Env Var | Default |
|------|---------|---------|
| `--adp-url` | `ADP_URL` | `http://localhost:8090` |
| `--runner-id` | `ADP_RUNNER_ID` | *(required)* |
| `--runner-type` | `ADP_RUNNER_TYPE` | `build_worker` |
| `--environments` | `ADP_ENVIRONMENTS` | `sh` |
| `--platforms` | `ADP_PLATFORMS` | `linux/amd64` |
| `--compliance` | `ADP_COMPLIANCE` | *(empty)* |
| `--labels` | `ADP_LABELS` | *(empty)* |
| `--max-concurrent` | `ADP_MAX_CONCURRENT` | `2` |
| `--enable-git-checkout` | `ADP_ENABLE_GIT_CHECKOUT` | `true` |

## Lifecycle

1. Register runner capabilities with adp
2. Connect to SSE endpoint (auto-reconnect with 5s backoff)
3. Receive `JobAssigned` events
4. Fetch job details, execute via `JobExecutor`
5. Report status back (`started` → `completed`/`failed`) via REST
