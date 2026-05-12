# HOOK-1: rmap hook CLI Surface

Status: PLANNED
Depends: HOST-1
Track: Distribution / Install / Host Integration

## Objective

Implement `rmap hook` subcommands that provide the policy surface for agent host
integration. These commands are called by thin host shims (Claude Code hooks,
Codex hooks, etc.) and contain all orientation, refresh, and validation logic.

## Command Surface

```
rmap hook session-start [--from-env | --db <path> --repo <path>]
  Orient agent at session start. Outputs context summary to stdout.

rmap hook prompt-submit [--from-env | --db <path> --repo <path>]
  Inject task-relevant context before prompt processing.

rmap hook post-edit [--from-env | --db <path> --repo <path> --files <paths>]
  Refresh index and report impact after file edits.

rmap hook pre-compact [--from-env | --db <path> --repo <path>]
  Checkpoint session state before context compaction.

rmap hook stop [--from-env | --db <path> --repo <path>]
  Validate and summarize at task completion.

rmap hook status
  Show current hook state and configuration.
```

## Payload Transport: --from-env

The `--from-env` flag is the **recommended invocation mode** for host integrations.

**Problem:** Passing large or multiline payloads (prompt text, tool output, file lists)
as shell command arguments is fragile due to quoting, escaping, and length limits.

**Solution:** With `--from-env`, hook commands read all context from environment variables
set by the host (Claude Code, Codex, etc.).

| Variable (Claude Code) | Variable (Codex) | Consumed by |
|------------------------|------------------|-------------|
| `CLAUDE_PROJECT_PATH` | `CODEX_PROJECT_PATH` | All hooks |
| `CLAUDE_SESSION_ID` | `CODEX_SESSION_ID` | All hooks |
| `PROMPT_TEXT` | `PROMPT` | `prompt-submit` |
| `TOOL_OUTPUT` | `CHANGED_FILES` | `post-edit` |
| `TOOL_NAME` | `TOOL_NAME` | `post-edit` |

The hook commands auto-detect which host is invoking them based on which environment
variables are present, and read from the appropriate ones.

**Explicit arguments still supported:** For testing, scripting, or non-host invocation,
explicit `--db`, `--repo`, `--files` arguments are still accepted and take precedence
over environment variables.

## Command Specifications

### rmap hook session-start

**Purpose:** Orient agent before any action.

**Behavior:**
1. Resolve DB path (explicit, environment, or discovery)
2. If DB missing: output warning, suggest `rmap index`
3. Run lightweight trust check
4. Run orient bundle
5. Check for `CURRENT_SLICE.md`
6. Output structured summary

**Output (JSON):**
```json
{
  "status": "ok",
  "db_path": "/path/to/repo.db",
  "repo": "my-repo",
  "trust": {
    "level": "high",
    "caveats": []
  },
  "orientation": {
    "modules": 12,
    "boundaries": 8,
    "recent_changes": 3
  },
  "current_slice": {
    "path": "CURRENT_SLICE.md",
    "summary": "Working on GR-3A CLI wiring"
  },
  "suggestions": [
    "Run `rmap refresh` if files changed since last index"
  ]
}
```

**Output (Human-readable, default):**
```
repo-graph session start
  Database: /path/to/repo.db
  Repository: my-repo
  Trust: high (no caveats)
  
  Orientation:
    12 modules, 8 boundary surfaces
    3 files changed since last index
  
  Current slice: GR-3A CLI wiring
    See: CURRENT_SLICE.md
  
  Suggestion: Run `rmap refresh` if files changed
```

**Exit codes:**
- 0: Success
- 1: Warning (DB stale, trust degraded)
- 2: Error (DB not found, invalid)

### rmap hook prompt-submit

**Purpose:** Inject context before prompt processing.

**Behavior:**
1. Optionally classify prompt (feature/bug/refactor/validation)
2. If code-relevant prompt, gather targeted context
3. Output context for injection

**Arguments:**
- `--prompt <text>`: The user prompt (for classification)
- `--classify`: Enable prompt classification

**Output (JSON):**
```json
{
  "status": "ok",
  "classification": "feature",
  "context": {
    "trust_snapshot": "high, no caveats",
    "relevant_modules": ["auth", "api"],
    "relevant_boundaries": 3,
    "active_slice": "GR-3A"
  },
  "inject": "Trust: high. Relevant: auth, api modules. Active slice: GR-3A."
}
```

**Exit codes:**
- 0: Success
- 1: Warning (partial context)
- 2: Error

### rmap hook post-edit

**Purpose:** Keep index fresh after edits.

**Behavior:**
1. Parse file paths from `--files` argument
2. Check if files are in indexed repo
3. Run incremental refresh or mark dirty
4. Detect affected artifact families
5. Report impact

**Arguments:**
- `--files <paths>`: Comma-separated or JSON array of edited file paths
- `--dry-run`: Report what would be refreshed without doing it

**Output (JSON):**
```json
{
  "status": "ok",
  "files_edited": ["src/auth.rs", "src/api.rs"],
  "files_in_repo": 2,
  "refresh_triggered": true,
  "impact": {
    "symbols_affected": 5,
    "edges_affected": 12,
    "modules_affected": ["auth"]
  }
}
```

**Output (Human-readable):**
```
post-edit: 2 files refreshed
  src/auth.rs: 3 symbols, 8 edges
  src/api.rs: 2 symbols, 4 edges
  
  Impact: auth module affected
```

**Exit codes:**
- 0: Success, refresh completed
- 1: Warning (some files not in repo)
- 2: Error (refresh failed)

### rmap hook pre-compact

**Purpose:** Checkpoint state before compaction.

**Behavior:**
1. Capture session state
2. Write to session state file
3. Optionally update `CURRENT_SLICE.md`

**Output (JSON):**
```json
{
  "status": "ok",
  "checkpoint": {
    "timestamp": "2024-01-15T10:30:00Z",
    "db_path": "/path/to/repo.db",
    "changed_files": ["src/auth.rs"],
    "trust_summary": "high",
    "current_slice": "GR-3A"
  },
  "state_file": "{sessions_dir}/abc123.json"
}
```

**Exit codes:**
- 0: Success
- 1: Warning (partial checkpoint)
- 2: Error

### rmap hook stop

**Purpose:** Validate and summarize at completion.

**Behavior:**
1. Check what validation was run during session
2. Run any required validation not yet run
3. Produce validation transcript
4. Compare before/after if baseline exists

**Arguments:**
- `--require-validation`: Fail if required validation not run
- `--transcript <path>`: Write transcript to file

**Output (JSON):**
```json
{
  "status": "ok",
  "validation": {
    "tests_run": true,
    "refresh_run": true,
    "trust_check": "passed"
  },
  "summary": {
    "files_changed": 3,
    "symbols_added": 5,
    "symbols_removed": 2,
    "trust_delta": "unchanged"
  },
  "transcript_path": "/path/to/transcript.json"
}
```

**Output (Human-readable):**
```
session complete
  Validation: tests run, refresh run, trust passed
  
  Summary:
    3 files changed
    +5 symbols, -2 symbols
    Trust: unchanged
  
  Transcript: /path/to/transcript.json
```

**Exit codes:**
- 0: Success, all validation passed
- 1: Warning (validation incomplete but not blocking)
- 2: Error (required validation failed, in enforcement mode)

### rmap hook status

**Purpose:** Show hook configuration state.

**Output:**
```
rmap hook status
  Configuration: {config_dir}/hooks.toml
  
  Integrations:
    Claude Code: installed (global)
      Config: ~/.claude/settings.json
      Hooks: session-start, post-edit, stop
    
    Codex: not installed
    Cursor: not installed
  
  Session state:
    Active: yes
    DB: /path/to/repo.db
    Last refresh: 2024-01-15T10:25:00Z
```

## DB and Repo Resolution

Commands resolve DB and repo paths in this order:

1. Explicit `--db` and `--repo` arguments
2. Environment variables: `RMAP_DB_PATH`, `RMAP_REPO_PATH`
3. Session state file (if in active session)
4. Discovery: find `.rmap.db` or `repo.db` in current directory or parents
5. Daemon query: ask running daemon for known repos

## Platform Path Resolution

Hook commands use platform-native paths per DIST-1 D3.

| Path | macOS | Linux |
|------|-------|-------|
| Config | `~/Library/Application Support/repo-graph/` | `~/.config/rmap/` |
| Data | `~/Library/Application Support/repo-graph/` | `~/.local/share/rmap/` |
| Logs | `~/Library/Logs/repo-graph/` | `~/.local/share/rmap/logs/` |
| Sessions | `~/Library/Application Support/repo-graph/sessions/` | `~/.local/share/rmap/sessions/` |

Path resolution is handled by a shared `rmap_paths` module that detects platform
at runtime. Hook commands call this module, not hardcoded paths.

```rust
// Conceptual API
fn config_dir() -> PathBuf;    // Platform-resolved config directory
fn data_dir() -> PathBuf;      // Platform-resolved data directory  
fn logs_dir() -> PathBuf;      // Platform-resolved logs directory
fn sessions_dir() -> PathBuf;  // Platform-resolved sessions directory
```

## Configuration

### hooks.toml

```toml
[session]
# Auto-refresh on session-start if DB older than this
stale_threshold_minutes = 30

[post_edit]
# Batch edits within this window before refresh
batch_window_seconds = 5

# Skip refresh for these patterns
ignore_patterns = [
  "*.log",
  "*.tmp",
  "node_modules/**"
]

[stop]
# Require these validations before completion
required_validations = ["refresh", "trust"]

# Enforcement mode (future)
enforcement = false
```

## Session State

### State File Location

`{sessions_dir}/{session_id}.json`

Where `{sessions_dir}` is platform-resolved per the Platform Path Resolution table above.

### State Schema

```json
{
  "session_id": "abc123",
  "started_at": "2024-01-15T10:00:00Z",
  "db_path": "/path/to/repo.db",
  "repo_path": "/path/to/repo",
  "baseline_snapshot": "snap_xyz",
  "files_edited": ["src/auth.rs", "src/api.rs"],
  "refreshes": [
    {"at": "2024-01-15T10:15:00Z", "files": ["src/auth.rs"]}
  ],
  "validations": {
    "trust_check": {"at": "2024-01-15T10:20:00Z", "result": "passed"}
  }
}
```

## Error Handling

### DB Not Found

```
Error: Database not found
Searched:
  - ./repo.db
  - ./.rmap.db
  - ../repo.db

To create a database:
  rmap index /path/to/repo ./repo.db
```

### Repo Not Indexed

```
Warning: Repository not fully indexed
  Last index: 3 days ago
  Files changed: 47

Consider running:
  rmap refresh /path/to/repo ./repo.db
```

### Daemon Not Running

```
Warning: Daemon not running
Hook will use direct execution (slower)

To start daemon:
  rmapd
```

## Integration with Daemon

When daemon is running, hook commands route through daemon for:
- Faster execution (no cold start)
- Shared state across hooks
- Coordinated refresh

When daemon is not running:
- Direct execution
- State via session files
- Warning but not failure

## Testing

### Unit Tests

- DB resolution logic
- Session state management
- Output formatting (JSON and human)
- Configuration parsing

### Integration Tests

- Full hook sequence: session-start → post-edit → stop
- Daemon vs direct execution parity
- Error recovery

### End-to-End Tests

- Simulated Claude Code hook invocation
- Simulated Codex hook invocation

## Out of Scope (HOOK-1)

- Host-specific shim generation (CLAUDE-1, CODEX-1)
- Enforcement mode implementation (future slice)
- Prompt classification ML (use heuristics only)

## Deliverables

1. `rmap hook session-start` command
2. `rmap hook prompt-submit` command
3. `rmap hook post-edit` command
4. `rmap hook pre-compact` command
5. `rmap hook stop` command
6. `rmap hook status` command
7. Session state management
8. hooks.toml configuration support
9. Unit and integration tests

## Success Criteria

- All hook commands implemented and tested
- JSON and human-readable output modes work
- Session state persists across hook invocations
- Works with and without daemon
- Exit codes follow specification
