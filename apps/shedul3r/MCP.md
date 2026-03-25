# shedul3r MCP — Remote Task Execution for AI Agents

shedul3r is a task execution server that runs shell commands with resilience patterns. It's available as an MCP server — agents can call it directly to execute tasks on remote machines.

## What It Does

Runs any shell command as a managed subprocess with:
- **Rate limiting** — pace requests per key (e.g., max 10/s for Anthropic API)
- **Circuit breaking** — stop sending if failure rate exceeds threshold
- **Bulkhead** — limit concurrent executions (e.g., max 3 claude processes)
- **Retry** — automatic retry with exponential backoff
- **Timeout** — kill long-running tasks

The primary use case: **run `claude -p` on a remote server** so your local machine doesn't get overloaded. The remote server has the repo cloned, you send a prompt, it runs claude, commits the result, and pushes a branch.

## MCP Tools

### `execute_task`

Execute a task through the engine.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task` | string | Yes | YAML task definition (see format below) |
| `input` | string | No | Piped to subprocess stdin. For `claude -p`, this is the prompt. |
| `limiter_key` | string | No | Override the limiter key from the YAML |
| `environment` | object | No | Environment variables as key-value pairs |
| `working_directory` | string | No | Absolute path for the subprocess |
| `timeout_ms` | number | No | Override timeout in milliseconds |

**Returns:** JSON with `success`, `output`, `metadata.exit_code`, `metadata.elapsed`.

### `get_status`

Get scheduler status: active task count, pending tasks, when the scheduler started.

**Parameters:** None.

## YAML Task Definition Format

The `task` parameter is a YAML string defining what to run and how to manage it.

**Minimal (just a command):**
```yaml
name: my-task
command: echo hello
```

**With timeout:**
```yaml
name: my-task
command: echo hello
timeout: 30s
```

**Full resilience configuration:**
```yaml
name: claude-agent
command: claude -p --model claude-sonnet-4-6 --setting-sources "" --permission-mode bypassPermissions
timeout: 15m
provider-id: claude
rate-limit:
  max-rate: 10
  window: 1s
retry:
  max-retries: 3
  initial-delay: 5s
  backoff-multiplier: 2.0
  max-delay: 30s
circuit-breaker:
  failure-rate-threshold: 50
  sliding-window-size: 10
  wait-duration-in-open-state: 30s
max-concurrent: 3
max-wait: 5m
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Task name (for logging) |
| `command` (or `run`) | string | Shell command to execute (via `/bin/sh -c`) |
| `timeout` | duration | Max execution time (`30s`, `5m`, `1h`, `PT30S`) |
| `provider-id` | string | Limiter key — groups tasks sharing resilience state |
| `rate-limit.max-rate` | number | Max requests per window |
| `rate-limit.window` | duration | Rate limit window |
| `retry.max-retries` | number | Max retry attempts |
| `retry.initial-delay` | duration | Delay before first retry |
| `retry.backoff-multiplier` | number | Multiplier per retry |
| `retry.max-delay` | duration | Cap on retry delay |
| `circuit-breaker.failure-rate-threshold` | number | % failures to open circuit (default 50) |
| `circuit-breaker.sliding-window-size` | number | Calls in the window (default 10) |
| `circuit-breaker.wait-duration-in-open-state` | duration | Time before half-open (default 30s) |
| `max-concurrent` | number | Max simultaneous tasks for this key |
| `max-wait` | duration | Max time waiting for a bulkhead permit |

## Running `claude -p` Remotely

This is the primary use case. Each shedul3r worker has a repo cloned at `/data/repo`. You send a prompt, claude runs on the remote machine, and you get the output back.

### Basic: run claude and get output

```
execute_task({
  task: "name: ask-claude\ncommand: claude -p --model claude-sonnet-4-6 --setting-sources \"\"\ntimeout: 5m",
  input: "Explain what the main.rs file does",
  working_directory: "/data/repo",
  environment: {"CLAUDE_CODE_OAUTH_TOKEN": "<your-token>"}
})
```

### With file edits: worktree → claude → commit → push

Run claude in an isolated git worktree so it can edit files without affecting the main branch:

```
execute_task({
  task: "name: agent-task\ncommand: |\n  set -e;\n  WORKDIR=/tmp/worktree-$$;\n  git -C /data/repo fetch origin;\n  git -C /data/repo worktree add $WORKDIR -b agent/my-branch origin/main --no-track;\n  cd $WORKDIR;\n  claude -p --model claude-sonnet-4-6 --setting-sources \"\" --permission-mode bypassPermissions;\n  git add -A;\n  if git diff --cached --quiet; then echo NO_CHANGES; else\n  git commit -m \"agent: my-branch\";\n  git push origin agent/my-branch;\n  echo PUSHED;\n  echo COMMIT=$(git rev-parse HEAD); fi;\n  git -C /data/repo worktree remove --force $WORKDIR 2>/dev/null || true\ntimeout: 10m",
  input: "Fix the authentication bug in src/auth.rs",
  working_directory: "/data/repo",
  environment: {"CLAUDE_CODE_OAUTH_TOKEN": "<your-token>"}
})
```

This:
1. Creates a git worktree from origin/main
2. Runs claude with the prompt inside the worktree
3. Commits and pushes changes to a new branch
4. Cleans up the worktree
5. Returns PUSHED + commit SHA, or NO_CHANGES

### Authentication

The `CLAUDE_CODE_OAUTH_TOKEN` is required for `claude -p` to authenticate with Anthropic. This is the OAuth token from your Claude subscription (Max plan). Pass it in the `environment` field of every task that runs claude.

The token is stored in macOS keychain under `Claude Code-credentials-*`. It expires and refreshes automatically when Claude Code runs locally. For remote execution, you need to extract and pass it.

### Authentication gotchas

- If a remote task fails with a bare `Exit 1:` and no useful stderr, do not assume the worker is broken. An expired Claude OAuth token often only shows up in Claude's stdout, not stderr.
- Refresh the token locally first by running a real `claude -p` command on your machine, then re-extract `claudeAiOauth.accessToken` from the keychain item. Pulling a stale token from keychain and forwarding it to the worker will poison every remote Claude task.
- The useful keychain value is the nested `claudeAiOauth.accessToken`, not the whole JSON blob.

### File-writing task gotchas

- For file-writing Claude tasks, process exit is not a reliable success signal. Claude can write the expected file and then keep the session open instead of exiting promptly.
- The robust success condition is: declared output files exist, are non-empty, and stop changing. If you only trust Claude's exit code, file-writing steps can look like retries/timeouts even when the output is already on disk.
- Rerunning into the same work directory without clearing the step subtree contaminates later attempts with stale `iter-*` artifacts. Reset the step directory before each verified-step run.

### Permission mode

On the remote server, shedul3r runs as a non-root `worker` user, so `--permission-mode bypassPermissions` works (it's blocked for root). This lets claude edit files without interactive permission prompts.

## Bundles (REST API only)

Bundles are file transfer mechanism for shipping files to/from the remote server. Available via the REST API (`/api/bundles`), not yet exposed as MCP tools.

- `POST /api/bundles` — upload a multipart file bundle, returns bundle ID + path
- `GET /api/bundles/:id/files/*path` — download a file from a bundle
- `DELETE /api/bundles/:id` — delete a bundle

The pipelin3r SDK uses bundles when `remote` mode is enabled to ship input files to the worker and retrieve output files.

## Architecture

shedul3r runs two transports on the same server:

| Transport | Port | Framework | Purpose |
|-----------|------|-----------|---------|
| REST API | PORT (default 7943) | actix-web | Task execution, bundles, status |
| MCP | PORT+1 | Axum (rmcp) | Streamable HTTP for AI agent tool calls |

Both share the same `TaskEngine` — rate limits, circuit breakers, and bulkhead state are unified across transports.

### How tasks execute internally

```
execute_task call
  → Parse YAML task definition
  → Acquire resilience permits (rate limit → circuit breaker → bulkhead)
  → Validate working directory (absolute path, no traversal, must exist)
  → Build SubprocessCommand
  → Spawn /bin/sh -c "command"
    → Pipe stdin (the `input` parameter)
    → Collect stdout/stderr concurrently
    → Enforce timeout (kill on expiry)
    → Strip CLAUDECODE env var (prevents nested session blocking)
  → Record circuit breaker outcome
  → Release bulkhead permit
  → Return TaskResponse
```

### Deployment

Each worker service is deployed on Railway with:
- A persistent volume at `/data` for the repo clone
- Node.js + `@anthropic-ai/claude-code` npm package for the claude CLI
- shedul3r musl static binary (downloaded from GitHub releases)
- A non-root `worker` user for subprocess execution
- Custom domains: `claude-worker-{project}-rest.trtk.me` (REST) and `claude-worker-{project}-mcp.trtk.me` (MCP)

### Limiter key grouping

The `provider-id` field in the YAML groups tasks that share resilience state. All tasks with `provider-id: claude` share the same rate limiter, circuit breaker, and bulkhead. This means if you send 10 claude tasks with `max-concurrent: 3`, only 3 run at a time — the rest queue up to `max-wait`.

## CLI Mode

shedul3r also has a CLI mode for local execution without the HTTP server:

```bash
shedul3r execute \
  --task "name: test\ncommand: echo hello" \
  --input "stdin data" \
  --env KEY=value \
  --workdir /some/dir \
  --timeout 5000
```

Same engine, same resilience patterns, no network.
