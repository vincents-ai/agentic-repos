---
name: adp-dispatch
description: Dispatch worker sessions to tmux with model acquisition, session tracking, and async monitoring. Used by the orchestrator to parallelise work across multiple models.
---

# ADP Dispatch Skill

This skill defines the full dispatch flow for sending work to autonomous worker agents.

## Prerequisites

Before dispatching, ensure:
- `$WORKSPACE/.internal/concurrency/` counter files exist
- `$WORKSPACE/.internal/sessions.json` exists
- `$WORKSPACE/.internal/acquire-model.sh` and `$WORKSPACE/.internal/release-model.sh` are executable
- `$WORKSPACE/.internal/check-sessions.sh` is executable
- The adp-worker agent is defined at `.opencode/agents/adp-worker.md`
- The adp-worker skill exists at `.opencode/skills/adp-worker/SKILL.md`

`$WORKSPACE` is `/home/shift/code/agentic-git`. All `.internal/` state lives there.

## Important: Workdir for Workers

Workers must be launched from the project directory they need to work in (e.g. `agentic-repos/`, `engram/`). The tmux `-c` flag sets the starting directory. Workers have bash access and can run engram commands in any workdir — they are not jailed.

## Dispatch Flow

### Step 1: Prepare the task in engram

Ensure the engram task exists and is marked `in_progress`:

```bash
engram task update <UUID> --status in_progress
```

Create a reasoning chain documenting the dispatch intent.

### Step 2: Acquire a model

```bash
MODEL=$(bash /home/shift/code/agentic-git/.internal/acquire-model.sh)
```

If it exits 1, no models are available. Wait and retry, or report to the user.

### Step 3: Derive session identifiers

```bash
SHORT_ID="${UUID:0:8}"
SESSION_NAME="adp-${SHORT_ID}"
```

### Step 4: Create the tmux session

```bash
tmux new-session -d -s "$SESSION_NAME" -c <project-dir>
```

Where `<project-dir>` is the project the worker should operate in (e.g. `/home/shift/code/agentic-git/agentic-repos`).

### Step 5: Launch the worker

```bash
tmux send-keys -t "$SESSION_NAME" "opencode run --model $MODEL \"Your task UUID is $UUID. Load the adp-worker skill and <task description>.\"" Enter
```

Do NOT use `--agent adp-worker` — it is a subagent and will be rejected by `opencode run`. The skill instructions govern behavior.

### Step 6: Register the session

```bash
python3 -c "
import json, datetime
sessions_file = '/home/shift/code/agentic-git/.internal/sessions.json'
with open(sessions_file) as f:
    sessions = json.load(f)
sessions.append({
    'session': '$SESSION_NAME',
    'task_uuid': '$UUID',
    'model': '$MODEL',
    'started': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'status': 'running',
    'workdir': '<project-dir>'
})
with open(sessions_file, 'w') as f:
    json.dump(sessions, f, indent=2)
"
```

### Step 7: Dispatch async monitor

```
Dispatch a general subagent with the instruction:
  sleep 300 && bash /home/shift/code/agentic-git/.internal/check-sessions.sh
```

### Step 8: Handle monitor results

**If session status is "completed" (::DONE:: detected):**
1. Read the completion report from engram (linked to the task UUID)
2. Release the model: `bash /home/shift/code/agentic-git/.internal/release-model.sh <model>`
3. Kill the tmux session: `tmux kill-session -t <SESSION_NAME>`
4. Mark the engram task as done

**If session status is "blocked" (::BLOCKED:: detected):**
1. Read the escalation entity from engram using the escalation-id
2. Resolve the issue
3. Resume: `bash /home/shift/code/agentic-git/.internal/resume-session.sh <SESSION_NAME> "<resolution>"`
4. Dispatch another monitor subagent

**If session status is "running":**
1. Dispatch another monitor subagent

### Step 9: Cleanup

```bash
tmux kill-session -t "$SESSION_NAME"
bash /home/shift/code/agentic-git/.internal/release-model.sh "$MODEL"
```

## Session Recovery

If the orchestrator crashes or is compacted, run `check-sessions.sh` to rediscover state. It scans all `adp-*` tmux sessions, detects signals, and updates `sessions.json`. Untracked `adp-*` sessions are added with `task_uuid: unknown`.

## Error Handling

- If `acquire-model.sh` fails: wait and retry, or report capacity exhaustion
- If tmux session creation fails: release the model, report the error
- If `opencode run` fails: release model, kill tmux, report error
- If monitor detects dead tmux: mark completed, release model
