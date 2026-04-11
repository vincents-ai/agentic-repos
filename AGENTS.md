# AGENTS.md — agentic-repos (repos-mcp)

**Read the workspace AGENTS.md at `../AGENTS.md` first.** This file extends it with repo-specific detail for `agentic-repos`. Platform-wide rules (engram protocol, licensing, build conventions, commit format) are defined there and apply here without exception.

---

## Repo Role in ADP

`agentic-repos` (package name: `repos-mcp`) is the Git server and API backend of the ADP platform. It has two deployment modes:

1. **Standalone binary** — full-featured multi-tenant Git server with HTTP API, SSH server, web UI host, monitoring, and LFS.
2. **Skylet v2 plugin** — compiles as `librepos_mcp.so`, loaded by the Skylet runtime to provide Git operations via FFI.

The `adp-web-ui` frontend is baked into this binary at compile time via `rust-embed`. Any new HTTP endpoint added here is part of the frontend contract.

---

## Engram Protocol

Follow the workspace engram protocol from `../AGENTS.md`. Repo-specific reminders:

```bash
# Start every session
engram sync pull --remote origin
# Load engram-session-start skill
engram ask query "agentic-repos <your task>"

# End every session
# Load engram-session-end skill
engram sync push --remote origin
```

**Commit format** (pre-commit hook enforces this):
```
<type>: <title> [<ENGRAM_TASK_UUID>]
```

`--no-verify` is forbidden. If the hook rejects your commit, fix the task linkage.

---

## Project Structure

```
agentic-repos/
├── src/
│   ├── api/          # HTTP REST API (actix-web) — the frontend contract
│   ├── auth/         # Authentication (SSH keys, OAuth, session)
│   ├── backup/       # Backup & restore
│   ├── cache/        # Redis caching layer
│   ├── compliance/   # Audit, privacy, retention
│   ├── config/       # Configuration parsing
│   ├── git/          # Git operations + SSH server (port 2222)
│   │   └── protocol/ # Git wire protocol (upload-pack, receive-pack)
│   ├── lfs/          # Git LFS — quota, locks, audit, throttle, verify
│   ├── rbac/         # Role-Based Access Control
│   ├── scaling/      # Horizontal scaling, cluster, leader election
│   ├── security/     # Security scanning
│   ├── skynet/       # Skylet v2 plugin implementation (FFI)
│   │   ├── ffi.rs        # ABI bridge: plugin_init_v2, plugin_shutdown_v2, plugin_handle_request_v2
│   │   ├── interface.rs  # Plugin API handlers
│   │   ├── multi_tenant/ # Namespace, permissions, quota
│   │   ├── rate_limit/   # Token bucket rate limiter
│   │   └── storage/      # Local + S3 storage backends
│   ├── storage/      # User storage backends (SQLite)
│   ├── web/          # Web UI host — serves rust-embed assets
│   └── webhooks/     # Webhook delivery
├── features/         # BDD test scenarios (Skylet plugin validation)
├── tests/            # Rust integration tests
├── docs/             # API documentation — keep in sync with code
├── .opencode/         # OpenCode config: agents, skills
├── .agents/skills/    # Agent-compatible skill definitions (synced to .opencode/skills/)
├── .internal/         # Dispatcher runtime state (gitignored)
├── migrations/       # SQL migrations (Postgres + SQLite)
├── k8s/              # Kubernetes manifests
├── monitoring/       # Prometheus + Grafana configs
├── nix/              # NixOS module + VM integration tests
└── config-example.yaml  # Fully documented configuration reference
```

---

## Building

### Nix (preferred — reproducible)

```bash
nix build
# Output: result/bin/repos-mcp  or  result/lib/librepos_mcp.so
```

### Cargo (fallback)

```bash
cargo build --release
# Binary:  target/release/repos-mcp
# Plugin:  target/release/librepos_mcp.so  (Linux)
#          target/release/librepos_mcp.dylib (macOS)
```

### Pre-commit checks (must pass before every commit)

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

---

## Testing

### Unit tests

```bash
cargo test
```

### Integration tests

```bash
cargo test --test git_protocol_integration
cargo test --test lfs_integration_test
cargo test --test lfs_locks_integration
```

### Skylet plugin validation

```bash
# BDD tests
~/code/skylet/target/release/plugin-test-harness bdd \
  --feature-path ./features \
  --plugin-path ./target/release/librepos_mcp.so

# ABI compliance check
~/code/skylet/target/release/plugin-test-harness validate \
  --plugin-path ./target/release/librepos_mcp.so
```

Full validation checklist before marking work done:

- [ ] `nix build` succeeds
- [ ] `cargo test` passes
- [ ] BDD feature files pass against test harness
- [ ] `plugin-test-harness validate` passes
- [ ] Health check responds: `health.check` action
- [ ] Repository CRUD operations work
- [ ] Error paths tested with invalid inputs

---

## Key Concepts

### Skylet Plugin Architecture

The crate is a Skylet v2 plugin providing Git operations:

| File | Purpose |
|---|---|
| `src/skynet/ffi.rs` | C FFI bridge — bridges async Rust to sync C ABI |
| `src/skynet/interface.rs` | Request routing + handler dispatch |
| `src/skynet/api.rs` | Method definitions |

**Exported symbols**: `plugin_init_v2`, `plugin_shutdown_v2`, `plugin_handle_request_v2`

**Plugin API methods**: `repo.create`, `repo.delete`, `repo.list`, `repo.info`, `lfs.operation`, `storage.*`, `health.check`, `metrics.get`

### Frontend Embedding

`adp-web-ui` assets are embedded at compile time via `rust-embed`. The embedding target in `src/web/` must point to `embedded-ui/` (symlinked or copied from `adp-web-ui/dist/` during the build pipeline). Do not serve assets from the filesystem at runtime — the binary must be self-contained.

### HTTP API (port 8090)

The REST API is the contract between this backend and the `adp-web-ui` frontend. Any change to an existing endpoint is a breaking change that requires:

1. An engram ADR documenting the change
2. A corresponding update to `docs/`
3. A coordinated update in `adp-web-ui`

---

## Configuration

`config-example.yaml` is the canonical reference. Key sections:

| Section | Purpose |
|---|---|
| `server.ssh.port` | SSH server port (default: 2222) |
| `server.http.port` | HTTP API port (default: 8090) |
| `auth.authorized_keys_path` | SSH public key store |
| `storage.type` | `local` or `s3` |
| `rbac.enabled` | Enable role-based access control |
| `rate_limit` | Per-tier request rate limits |
| `lfs` | LFS JWT secret, base URL, quotas |
| `features` | Feature flags: `git_lfs`, `web_ui`, `rbac`, `redis_cache`, etc. |

When loaded as a Skylet plugin:
```yaml
plugins:
  - name: git-operations
    path: ./target/release/librepos_mcp.so
    config:
      storage.default_backend: "local"
      log.level: "info"
```

---

## Common Tasks

### Adding a new HTTP endpoint

1. Define the handler in `src/api/handlers.rs`
2. Add routing in `src/api/routes/`
3. Update `docs/` with the new endpoint spec
4. Add an integration test
5. Note the new endpoint in engram knowledge if it affects the frontend contract

### Adding a new Skylet plugin method

1. Define the method in `src/skynet/api.rs`
2. Implement the handler in `src/skynet/interface.rs`
3. Add routing in `handle_request()`
4. Create a BDD test scenario in `features/`
5. Update `docs/`

### Modifying storage backends

1. Implement the `StorageBackend` trait in `src/skynet/storage/`
2. Register the backend in the storage factory
3. Add configuration support in `config-example.yaml`
4. Test with storage BDD scenarios

---

## Dependencies

### Key runtime dependencies

| Crate | Purpose |
|---|---|
| `skylet-abi` | Skylet v2 FFI ABI bindings |
| `actix-web` | HTTP server |
| `russh` / `russh-keys` | SSH server |
| `git2` | Git operations (vendored libgit2) |
| `gix-protocol` / `gix-transport` | Pure Rust Git wire protocol |
| `sqlx` | Database (Postgres + SQLite) |
| `aws-sdk-s3` | S3-compatible storage |
| `deadpool-redis` | Redis connection pool |
| `prometheus` | Metrics |
| `tera` | HTML templating (legacy; being replaced by rust-embed) |
| `jsonwebtoken` | LFS JWT auth |

### Dev dependencies

| Crate | Purpose |
|---|---|
| `mockall` | Mock generation for unit tests |
| `tokio-test` | Async test utilities |
| `serial_test` | Enforce serial test execution |

---

## Deployment

### Docker

```bash
docker compose up        # uses compose.yml
docker compose -f compose-nix.yml up  # Nix-based image
```

### Kubernetes

```bash
kubectl apply -f k8s/
```

### Skylet plugin (manual)

```bash
cp target/release/librepos_mcp.so ~/code/skylet/plugins/repos-mcp/
cp config/skylet-config.yaml ~/code/skylet/config.yaml
source config/repos-mcp.env
```

### With garage-s3 storage plugin

Load order matters — garage-s3 must load before repos-mcp:

1. `garage-s3` (priority 10) — starts embedded Garage, registers S3 service
2. `repos-mcp` (priority 5) — discovers garage-s3 via `global_registry()`, uses it for LFS/storage

See `DEPLOYMENT.md` for the full deployment guide.

---

## Troubleshooting

| Symptom | Check |
|---|---|
| Build fails: missing deps | `nix build` to get all deps in scope |
| Plugin not loading | `ldd target/release/librepos_mcp.so` for missing symbols |
| ABI version mismatch | Ensure `skylet-abi` version matches the Skylet runtime |
| Test harness not found | Build from `~/code/skylet/plugin-test-harness` |
| SSH auth failure | Verify `authorized_keys` format: `username:ssh-key-type key comment` |
| LFS 401 errors | Check `lfs.jwt_secret` in config matches client configuration |

---

## Autonomous Session Dispatcher

The workspace has a tmux-based session dispatcher for parallelising work across multiple model providers. Two skills and one custom agent definition enable the orchestrator to dispatch, monitor, and resume worker sessions.

### When to Use

| Situation | Action |
|---|---|
| Multi-step implementation task | Dispatch via `adp-dispatch` skill |
| Multiple independent tasks ready | Dispatch one worker per task, each gets its own tmux session and model slot |
| Worker needs clarification | Worker escalates via engram, orchestrator resolves and sends resume via `resume-session.sh` |
| Worker completes | Monitor detects `::DONE::`, orchestrator reads report from engram, cleans up |

### Architecture

```
Orchestrator (this session)
  │
  ├── loads adp-dispatch skill
  ├── acquire-model.sh → picks model with capacity
  ├── tmux new-session -s adp-<SHORT_ID>
  ├── opencode run --agent adp-worker --model <model> "Task UUID: <UUID>..."
  ├── registers in .internal/sessions.json
  └── dispatches async monitor (sleep 300 + check-sessions.sh)
```

### Skills

**`adp-dispatch`** — used by the orchestrator to launch workers. Full dispatch flow: prepare engram task, acquire model, create tmux session, send `opencode run`, register session, dispatch async monitor, handle results (DONE/BLOCKED/running), cleanup.

**`adp-worker`** — loaded by the dispatched worker session. Rules:
- Never use the question tool (denied in agent definition)
- Never make assumptions — escalate if unsure
- Never batch findings — write to engram immediately
- Never run broad engram queries (`ask query`, `task list`, `next`) — only targeted commands with specific entity IDs
- On blockage: write engram escalation, output `::BLOCKED::<id>::`
- On completion: write completion report to engram, output `::DONE::<summary>::`

### Agent

**`adp-worker`** (`.opencode/agents/adp-worker.md`) — custom agent with `question: deny`, `plan_enter: deny`, `plan_exit: deny`. All other tools enabled. Launched via `opencode run --agent adp-worker`.

### Model Concurrency

Tracked via atomic counter files in `.internal/concurrency/`:

| Model | Max Concurrent | Priority |
|---|---|---|
| `zai-coding-plan/glm-5-turbo` | 5 | 1 (primary) |
| `zai-coding-plan/glm-5.1` | 1 | 2 (fallback) |
| `opencode/big-pickle` | 2 | 3 |
| `opencode/minimax-m2.5-free` | 2 | 4 |
| `featherless/MiniMaxAI/MiniMax-M2.5` | 1 | 5 |
| `github-copilot/claude-sonnet-4` | 1 | 6 |

`acquire-model.sh` picks the highest-priority model below its limit. `release-model.sh` decrements on completion.

### Scripts (`.internal/`)

| Script | Purpose |
|---|---|
| `acquire-model.sh` | Pick model with capacity, atomically increment counter |
| `release-model.sh` | Decrement counter on session completion |
| `check-sessions.sh` | Scan tmux sessions for `::DONE::` / `::BLOCKED::` signals, update `sessions.json`, release models for dead sessions |
| `resume-session.sh` | Send resume message to blocked tmux session via `send-keys` |

### Dispatch Quick Reference

```bash
# 1. Create and mark engram task in_progress
engram task update <UUID> --status in_progress

# 2. Acquire model
MODEL=$(bash .internal/acquire-model.sh)

# 3. Launch worker
SHORT="${UUID:0:8}"
tmux new-session -d -s "adp-${SHORT}" -c /path/to/workdir
tmux send-keys -t "adp-${SHORT}" "opencode run --agent adp-worker --model $MODEL \"Task UUID: $UUID. Load the adp-worker skill and <task description>.\"" Enter

# 4. Register session (via python3/jq append to .internal/sessions.json)

# 5. Monitor after 5 min
# Dispatch async subagent: sleep 300 && bash .internal/check-sessions.sh

# 6. On DONE: release model, kill session
bash .internal/release-model.sh "$MODEL"
tmux kill-session -t "adp-${SHORT}"

# 7. On BLOCKED: resolve, resume
bash .internal/resume-session.sh "adp-${SHORT}" "Resolved: <answer to escalation>"
```

### .internal/ is gitignored

The `.internal/` directory contains runtime state (concurrency counters, session tracking) and is excluded from version control. It must not contain secrets.

---

## Documentation

- `docs/SKYLET_PLUGIN_VALIDATION.md` — plugin validation walkthrough
- `docs/ARCHITECTURE.md` — system architecture
- `docs/GIT_LFS_VALIDATION.md` — LFS testing guide
- `DEPLOYMENT.md` — full deployment reference
- `monitoring/README.md` — Prometheus + Grafana setup
