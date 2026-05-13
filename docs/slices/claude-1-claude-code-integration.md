# CLAUDE-1: Claude Code Integration

Status: IMPLEMENTED (2026-05-13)
Depends: HOST-1, HOOK-1, HOOK-1A
Track: Distribution / Install / Host Integration

**Execution order note:** Follows MAC-1 in rollout sequence (macOS-first). This slice
implements HOST-1 contract for Claude Code specifically. Platform installers (MAC-1,
LINUX-1) call `rmap integrate claude-code install`.

**Amendment (2026-05-13):** Command structure changed from mode flags to subcommands
for cleaner CLI grammar and better scalability to future hosts (Codex, Cursor).
Original: `rmap integrate [--remove|--status] claude-code`
New: `rmap integrate claude-code [install|remove|status]`

**Schema source:** This slice is aligned with the official Claude Code hooks reference
at https://code.claude.com/docs/en/hooks (verified 2026-05-13).

## Objective

Implement repo-graph integration with Claude Code via lifecycle hooks. This slice
produces:
1. Claude Code-specific hook configuration
2. Stdin-JSON transport adapter for HOOK-1 commands
3. Integration CLI tooling

## Transport Architecture

### The Problem

HOOK-1 was designed with `--from-env` assuming hosts provide context via environment
variables. Claude Code does not use this model — it passes a **JSON payload on stdin**.

### The Solution

Introduce explicit transport separation in hook commands:

| Flag | Transport | Host |
|------|-----------|------|
| `--from-env` | Environment variables | Codex, future hosts |
| `--from-stdin` | JSON on stdin | Claude Code |

Both transports normalize to the same internal `HookContext` structure. Policy handlers
remain unchanged.

```
┌─────────────────┐     ┌─────────────────┐
│  Claude Code    │     │     Codex       │
│  (stdin JSON)   │     │  (env vars)     │
└────────┬────────┘     └────────┬────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐     ┌─────────────────┐
│ --from-stdin    │     │ --from-env      │
│ transport       │     │ transport       │
└────────┬────────┘     └────────┬────────┘
         │                       │
         └───────────┬───────────┘
                     ▼
            ┌─────────────────┐
            │   HookContext   │
            │  (normalized)   │
            └────────┬────────┘
                     ▼
            ┌─────────────────┐
            │  Policy Handler │
            │  (unchanged)    │
            └─────────────────┘
```

## Claude Code Hook Model

Claude Code exposes lifecycle hooks via `.claude/settings.json`. Hooks are organized
in **matcher groups** with nested hook arrays.

### Hook Configuration Structure

```json
{
  "hooks": {
    "<EventName>": [
      {
        "matcher": "<pattern>",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/handler",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

- **EventName**: Hook event (e.g., `SessionStart`, `PostToolUse`)
- **matcher**: String pattern for filtering (tool events only)
- **hooks**: Array of hook handlers to execute
- **type**: Handler type (`"command"` for shell commands)
- **timeout**: Seconds before timeout (not milliseconds)

### Events Without Matchers

Some events do not support matchers and always fire:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook session-start --from-stdin",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

## Hook Entry Schema

### Command Hook Handler

```json
{
  "type": "command",
  "command": "rmap hook session-start --from-stdin",
  "timeout": 30
}
```

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `type` | Yes | string | Must be `"command"` for shell commands |
| `command` | Yes | string | Shell command to execute |
| `timeout` | No | number | Seconds before timeout (default: 600) |

### Matcher Patterns (Tool Events)

Matchers filter which tool invocations trigger the hook.

| Pattern | Meaning | Example |
|---------|---------|---------|
| Omitted or `""` | Match all | Fires on every tool |
| `"Edit"` | Exact match | Only Edit tool |
| `"Edit\|Write"` | Pipe-separated | Edit or Write |
| `"^mcp__.*"` | Regex | Any MCP tool |

## Stdin JSON Payload

Claude Code passes hook input as **JSON on stdin**, not environment variables.

### Common Fields (All Events)

```json
{
  "session_id": "abc123-def456",
  "cwd": "/path/to/project",
  "hook_event_name": "SessionStart"
}
```

### SessionStart Payload

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "SessionStart"
}
```

### UserPromptSubmit Payload

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "Implement the login feature"
}
```

### PostToolUse Payload

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/path/to/file.rs",
    "old_string": "...",
    "new_string": "..."
  },
  "tool_output": "File edited successfully"
}
```

### PreCompact Payload

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PreCompact",
  "compaction_type": "auto"
}
```

### Stop Payload

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "Stop"
}
```

## Environment Variables

Claude Code provides limited environment variables (not payload data):

| Variable | Description |
|----------|-------------|
| `CLAUDE_PROJECT_DIR` | Project root directory |
| `CLAUDE_ENV_FILE` | Path to file for persisting env vars (SessionStart only) |
| `CLAUDE_EFFORT` | Effort level: `low`, `medium`, `high`, `xhigh`, `max` |

**Note:** Session ID, prompt text, tool inputs are in the stdin JSON, not env vars.

## repo-graph Hook Configuration

### Minimal Configuration (Default)

Installed by `rmap integrate claude-code install` (no flags).

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook session-start --from-stdin",
            "timeout": 30
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook stop --from-stdin",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

### Full Configuration

Installed by `rmap integrate claude-code install --full`.

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook session-start --from-stdin",
            "timeout": 30
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook prompt-submit --from-stdin",
            "timeout": 10
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook post-edit --from-stdin",
            "timeout": 60
          }
        ]
      }
    ],
    "PreCompact": [
      {
        "matcher": "auto|manual",
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook pre-compact --from-stdin",
            "timeout": 10
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook stop --from-stdin",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

## Hook Output Protocol

### stdout

Hook stdout is processed by Claude Code. repo-graph hooks output:

**Plain text (default):** Human-readable summary, added to context.

**JSON with decision control:**

```json
{
  "decision": "continue",
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "repo-graph: 12 modules, 8 boundaries, trust: high"
  }
}
```

| Field | Description |
|-------|-------------|
| `decision` | `"continue"` or `"block"` |
| `additionalContext` | String added to Claude's context |
| `reason` | Shown to user if blocked |

### Exit Codes

| Code | Meaning | Claude Code Behavior |
|------|---------|---------------------|
| 0 | Success | Continue, process stdout |
| 1 | Warning | Continue, log warning |
| 2 | Block | Stop processing, show stderr to user |

## Configuration Locations

| Location | Scope | Shareable |
|----------|-------|-----------|
| `~/.claude/settings.json` | All projects | No (local) |
| `.claude/settings.json` | Single project | Yes (commit) |
| `.claude/settings.local.json` | Single project | No (gitignored) |

### Precedence

Project-level config overrides global for that project.

## Integration Commands

### rmap integrate claude-code install

```
$ rmap integrate claude-code install [--global|--project] [--full] [--dry-run] [--force]

Options:
  --global    Install to ~/.claude/settings.json (default)
  --project   Install to ./.claude/settings.json
  --full      Install all hooks (default: minimal - SessionStart + Stop only)
  --dry-run   Show changes without applying
  --force     Overwrite existing repo-graph hooks
```

**Default behavior (minimal profile):**

Installs only:
- `SessionStart` — orientation at session start
- `Stop` — validation summary at session end

**Full profile (`--full`):**

Installs all hooks:
- `SessionStart` — orientation at session start
- `UserPromptSubmit` — prompt classification
- `PostToolUse` — file dirty tracking after Edit/Write
- `PreCompact` — checkpoint before compaction
- `Stop` — validation summary at session end

**Rationale for minimal default:** CLAUDE-1 is the first external host config mutation.
Minimal reduces config blast radius, merge complexity, and rollback risk while still
providing startup orientation and end-of-session reporting.

**Install sequence:**

1. Check for existing config file
2. Parse existing hooks (validate JSON)
3. Detect existing repo-graph hooks
4. Plan merge strategy
5. Create backup (`{file}.rmap-backup`)
6. Merge hooks into config
7. Validate resulting JSON
8. Record in install manifest

### rmap integrate claude-code remove

```
$ rmap integrate claude-code remove [--global|--project]

Options:
  --global    Remove from ~/.claude/settings.json (default)
  --project   Remove from ./.claude/settings.json
```

**Behavior (surgical removal):**

1. Find config file
2. Remove repo-graph hooks only (preserve non-repo-graph hooks)
3. If event becomes empty, remove event key
4. If hooks section becomes empty, remove hooks key
5. Update install manifest

**Note:** Surgical removal preserves any custom hooks the user added after repo-graph
installation. This is safer than backup restore, which would destroy post-install user
changes. Backup remains available for manual emergency recovery.

### rmap integrate claude-code status

```
$ rmap integrate claude-code status [--global|--project] [--json]

Options:
  --global    Check ~/.claude/settings.json (default)
  --project   Check ./.claude/settings.json
  --json      Output JSON instead of human-readable text
```

**Example output:**

```
Claude Code Integration Status

Global (~/.claude/settings.json):
  Status: installed (minimal)
  Hooks:
    SessionStart: rmap hook session-start --from-stdin
    Stop: rmap hook stop --from-stdin
  Backup: ~/.claude/settings.json.rmap-backup
  Installed: 2024-01-15T10:30:00Z

Project (./.claude/settings.json):
  Status: not installed
```

## Merging with Existing Hooks

### Strategy

1. Parse existing hook configuration
2. For each event, check if repo-graph hooks exist
3. If no repo-graph hooks: prepend to existing hooks array
4. If repo-graph hooks exist: update in place or skip
5. Preserve all non-repo-graph hooks

### Duplicate Detection

repo-graph hooks are identified by command containing `rmap hook`.

### Example Merge (Minimal Install)

**Before:**
```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {"type": "command", "command": "my-setup.sh", "timeout": 5}
        ]
      }
    ]
  }
}
```

**After (`rmap integrate claude-code install`):**
```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {"type": "command", "command": "rmap hook session-start --from-stdin", "timeout": 30}
        ]
      },
      {
        "hooks": [
          {"type": "command", "command": "my-setup.sh", "timeout": 5}
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {"type": "command", "command": "rmap hook stop --from-stdin", "timeout": 30}
        ]
      }
    ]
  }
}
```

## Module Structure

```
commands/integrate/
├── mod.rs           # Dispatcher: parse host + action subcommands
├── claude_code.rs   # Claude Code policy: install/remove/status logic
├── config.rs        # JSON merge/patch planning (shared across hosts)
└── manifest.rs      # Install manifest recording (shared across hosts)
```

**Boundary rules:**
- `mod.rs` owns dispatch only, no host-specific logic
- `claude_code.rs` owns Claude-specific paths, schema, detection
- `config.rs` owns JSON parsing, merge planning, validation
- `manifest.rs` owns `host_integrations` array in install manifest

Backup/restore logic lives in `config.rs` unless it grows large enough to warrant
a dedicated `backup.rs`.

## HOOK-1A Transport (Implemented)

**Status:** Implemented in HOOK-1A slice.

All hook commands support `--from-stdin` for Claude Code JSON payloads:

```
rmap hook session-start --from-stdin
rmap hook prompt-submit --from-stdin
rmap hook post-edit --from-stdin
rmap hook pre-compact --from-stdin
rmap hook stop --from-stdin
```

Implementation details in `docs/slices/hook-1a-stdin-transport.md`.

### Resolution Chain

```
1. Explicit --db/--repo arguments (highest priority)
2. RMAP_DB_PATH, RMAP_REPO_PATH environment variables
3. --from-stdin: read JSON payload, use cwd as repo path
4. --from-env: read host environment variables
5. Discovery: find .rmap.db in current directory or parents
```

## Backup Contract

Per HOST-1 D3:

- **Backup naming:** `{original}.rmap-backup`
- **Multiple backups:** `{original}.rmap-backup.{timestamp}`
- **Recorded in manifest:** `host_integrations` array

### Manifest Recording

```json
{
  "host_integrations": [
    {
      "host": "claude-code",
      "scope": "global",
      "config_path": "~/.claude/settings.json",
      "backup_path": "~/.claude/settings.json.rmap-backup",
      "installed_at": "2024-01-15T10:30:00Z",
      "hooks_installed": ["SessionStart", "PostToolUse", "PreCompact", "Stop"]
    }
  ]
}
```

## Error Handling

### Config File Not Valid JSON

```
Error: ~/.claude/settings.json is not valid JSON
  Parse error at line 5: unexpected token

Cannot proceed. Fix the JSON syntax or use --force to overwrite.
```

### No Write Permission

```
Error: Cannot write to ~/.claude/settings.json
  Permission denied

Try: chmod u+w ~/.claude/settings.json
```

### Claude Code Not Detected

```
Note: ~/.claude directory does not exist
Creating ~/.claude/settings.json

Proceed? [y/N]
```

## Available Hook Events (Reference)

Claude Code supports these events (repo-graph uses subset):

**Session Lifecycle:**
- `SessionStart` - Session begins/resumes (used)
- `Setup` - During init
- `SessionEnd` - Session terminates

**Per-Turn:**
- `UserPromptSubmit` - User submits prompt (used)
- `Stop` - Claude finishes responding (used)
- `StopFailure` - Turn ends due to error

**Tool Events:**
- `PreToolUse` - Before tool executes (future: enforcement)
- `PostToolUse` - After tool succeeds (used)
- `PostToolUseFailure` - After tool fails

**Compaction:**
- `PreCompact` - Before compaction (used)
- `PostCompact` - After compaction

**Other (not used by repo-graph):**
- `SubagentStart`, `SubagentStop`
- `TaskCreated`, `TaskCompleted`
- `FileChanged`, `CwdChanged`
- `WorktreeCreate`, `WorktreeRemove`

## Out of Scope (CLAUDE-1)

- PreToolUse enforcement (future slice)
- SubagentStop hook (same as Stop for now)
- Custom hook timeout configuration
- PostCompact hook
- FileChanged incremental refresh

## Deliverables

1. `--from-stdin` transport mode in HOOK-1 commands — **DONE (HOOK-1A)**
2. `rmap integrate claude-code install` command (minimal default, --full opt-in)
3. `rmap integrate claude-code remove` command
4. `rmap integrate claude-code status` command
5. Correct hook configuration schema — **DONE (scripts/lib/macos.sh)**
6. Merge logic for existing configs
7. Backup/restore logic
8. Manifest recording for host integrations
9. Integration tests
10. Module structure: dispatcher, claude_code, config, manifest

## Success Criteria

- Minimal install (`install`) installs SessionStart + Stop only
- Full install (`install --full`) installs all 5 hooks
- Integration installs cleanly on fresh Claude Code setup
- Integration merges correctly with existing hooks
- `--from-stdin` correctly parses Claude Code JSON payloads — **DONE (HOOK-1A)**
- All hooks execute and produce expected output
- Removal cleanly restores previous state
- Project and global scopes work correctly
- Backup/restore is reliable
- Status correctly reports installed profile (minimal/full)

## Dependencies on HOOK-1/HOOK-1A

**Completed (HOOK-1A):**
- `--from-stdin` flag on all hook commands
- `StdinPayload` parsing
- Normalization to existing `HookContext`

Policy handlers remain unchanged. HOOK-1A is implemented and validated.
