# adp-ci-router

CI job routing engine: SQLite-backed job queue, runner matching, job assignment, and timeout monitoring.

## Architecture

```
Git push → router.submit_job() → SQLite (pending)
                                    ↓
Router loop → match_pending_jobs() → assign to runner → SQLite (assigned)
                                                          ↓
Runner SSE ← JobAssigned event ← broadcast channel
                                                          ↓
Runner REST → update_job(Running) → SQLite (running)
             → append logs → git ref
                                                          ↓
Runner REST → update_job(Completed/Failed) → SQLite (done)
                                              → write result → git ref
```

## Components

| Module | Purpose |
|--------|---------|
| `queue` | SQLite job queue — create/assign/update/query jobs and runners |
| `matcher` | Score-based runner selection (available slots, label affinity) |
| `router` | `CiRouter` orchestrator — submit, match, assign, cancel, timeout, async loop |
| `metrics` | `CiMetricsSnapshot` for KEDA auto-scaling |

## Usage

```rust
use adp_ci_router::{CiRouter, JobQueue};

let queue = Arc::new(JobQueue::open_in_memory()?);
let (tx, _) = broadcast::channel::<CiEvent>(64);
let router = Arc::new(CiRouter::new(queue, tx));

router.submit_job("j-1", "repo-1", JobType::Build, JobPriority::High, "nix", "{}", "[]", "main", "abc")?;
router.match_pending_jobs()?; // assigns to best runner
router.handle_timeouts()?;    // re-queues timed-out assignments
```

## SQLite Schema

Two tables: `ci_jobs` (status, priority, runner assignment) and `ci_runners` (capabilities, heartbeat, job count). Indexed for fast priority-ordered queries and runner availability checks.

## Tests

20 unit tests.
