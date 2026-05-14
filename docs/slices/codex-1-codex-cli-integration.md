# CODEX-1: Codex CLI Integration

Status: IMPLEMENTED (2026-05-14)
Depends: HOST-1, HOOK-1, HOOK-1A
Track: Distribution / Install / Host Integration

**Schema verified:** May 2026 from official OpenAI documentation.
**Volatility note:** Codex hooks are marked as experimental by OpenAI and may change.

## Objective

Implement repo-graph integration with Codex CLI via lifecycle hooks. This slice
produces:
1. Codex-specific hook configuration
2. Integration CLI tooling (`rmap integrate codex`)

## Verified Codex Contract

**Source:** https://developers.openai.com/codex/hooks (verified May 2026)

### Transport

Codex passes hook context as **JSON on stdin**, same as Claude Code. Use `--from-stdin`.

**NOT environment variables.** The CODEX_* env var assumptions in HOST-1 v1 were incorrect.

### Hook Schema

Codex uses the same nested matcher-group structure as Claude Code:

```json
{
  "hooks": {
    "EventName": [
      {
        "matcher": "pattern",
        "hooks": [
          {
            "type": "command",
            "command": "script_path",
            "timeout": 600
          }
        ]
      }
    ]
  }
}
```

### Timeout

**Seconds** (default 600). Same as Claude Code.

### Matcher Syntax

Regex strings. Use `|` for alternatives. Omit or use `""` to match all.

### Config Locations (Codex native)

Codex supports two config formats:
- `hooks.json` — standalone JSON file
- `config.toml` — inline `[hooks]` table

| Scope | hooks.json | config.toml |
|-------|------------|-------------|
| Global | `~/.codex/hooks.json` | `~/.codex/config.toml` |
| Project | `.codex/hooks.json` | `.codex/config.toml` |

**repo-graph scope (CODEX-1):** This slice targets `hooks.json` only. `config.toml` support
is out of scope — users with inline hooks must manually add repo-graph entries or migrate
to `hooks.json`.

| Scope | repo-graph target |
|-------|-------------------|
| Global | `~/.codex/hooks.json` |
| Project | `.codex/hooks.json` |

### Trust Model

Project-level hooks only load for trusted projects. Untrusted projects skip
`.codex/` layers entirely.

### Supported Events

| Event | Description | repo-graph usage |
|-------|-------------|------------------|
| `SessionStart` | Session begins (source: startup/resume/clear) | Orientation |
| `UserPromptSubmit` | User submits prompt | Prompt classification |
| `PreToolUse` | Before tool executes | Future: enforcement |
| `PostToolUse` | After tool succeeds | File dirty tracking |
| `PermissionRequest` | Permission prompt | Not used |
| `Stop` | Turn ends | Validation summary |

### Input JSON (stdin)

Common fields:
- `session_id` — session identifier
- `cwd` — working directory
- `hook_event_name` — current event name
- `model` — active model slug
- `turn_id` — for turn-scoped hooks
- `transcript_path` — optional path to session transcript

Event-specific:
- `tool_name`, `tool_input`, `tool_response` — tool events
- `prompt` — UserPromptSubmit
- `source` — SessionStart (startup/resume/clear)

### Output JSON (stdout)

```json
{
  "continue": true,
  "stopReason": "optional reason",
  "systemMessage": "optional context injection",
  "suppressOutput": false
}
```

## repo-graph Hook Configuration

### Minimal Configuration (Default)

Installed by `rmap integrate codex install` (no flags).

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
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

Installed by `rmap integrate codex install --full`.

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
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
        "matcher": "Edit|Write|apply_patch",
        "hooks": [
          {
            "type": "command",
            "command": "rmap hook post-edit --from-stdin",
            "timeout": 60
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

**Note:** Codex uses `apply_patch` for file edits in addition to `Edit|Write`.

## Integration Commands

### rmap integrate codex install

```
$ rmap integrate codex install [--global|--project] [--full] [--dry-run] [--force]

Options:
  --global    Install to ~/.codex/hooks.json (default)
  --project   Install to ./.codex/hooks.json
  --full      Install all hooks (default: minimal - SessionStart + Stop only)
  --dry-run   Show changes without applying
  --force     Overwrite existing repo-graph hooks
```

**Default behavior (minimal profile):**
- `SessionStart` — orientation at session start
- `Stop` — validation summary at session end

**Full profile (`--full`):**
- `SessionStart` — orientation at session start
- `UserPromptSubmit` — prompt classification
- `PostToolUse` — file dirty tracking after edits
- `Stop` — validation summary at session end

### rmap integrate codex remove

```
$ rmap integrate codex remove [--global|--project]

Options:
  --global    Remove from ~/.codex/hooks.json (default)
  --project   Remove from ./.codex/hooks.json
```

Surgical removal: removes repo-graph hooks only, preserves other hooks.

### rmap integrate codex status

```
$ rmap integrate codex status [--global|--project] [--json]

Options:
  --global    Check ~/.codex/hooks.json (default)
  --project   Check ./.codex/hooks.json
  --json      Output JSON instead of human-readable text
```

## Module Structure

Because Codex and Claude Code share the same structural hook schema, the config
transformation logic can be shared:

```
commands/integrate/
├── mod.rs              # Dispatcher: parse host + action subcommands
├── claude_code.rs      # Claude Code policy: paths, schema constants
├── codex.rs            # Codex policy: paths, schema constants (NEW)
├── config.rs           # Generic JSON merge/patch (SHARED)
└── manifest.rs         # Install manifest recording (SHARED)
```

**Boundary rules:**
- `mod.rs` owns dispatch only, no host-specific logic
- `claude_code.rs` owns Claude-specific paths, event selection, detection
- `codex.rs` owns Codex-specific paths, event selection, detection
- `config.rs` owns JSON parsing, merge planning, validation (shared)
- `manifest.rs` owns `host_integrations` array in install manifest (shared)

## Differences from Claude Code

| Aspect | Claude Code | Codex |
|--------|-------------|-------|
| Config file | `settings.json` | `hooks.json` |
| Config dir | `.claude/` | `.codex/` |
| Global dir | `~/.claude/` | `~/.codex/` |
| Alternative config | `.claude/settings.local.json` | `config.toml` inline `[hooks]` |
| SessionStart matcher | None (always fires) | `startup\|resume` (skip `clear`) |
| PostToolUse matcher | `Edit\|Write` | `Edit\|Write\|apply_patch` |
| PreCompact | Supported | Not available |
| Trust model | Always loads | Project trust gates project hooks |

## Volatility Note

**Codex hooks are experimental.** Per OpenAI documentation (May 2026):

> "Hooks are experimental and may change in future releases."

This slice:
- Isolates Codex-specific logic in `codex.rs`
- Uses shared config transformation for schema stability
- Documents this risk in TECH-DEBT.md

If Codex schema changes significantly, only `codex.rs` and the Codex hook definitions
need updates.

## Deliverables

1. `rmap integrate codex install` command
2. `rmap integrate codex remove` command
3. `rmap integrate codex status` command
4. Codex-specific hook definitions (minimal + full profiles)
5. Manifest recording for Codex integrations
6. Integration tests

## Success Criteria

- Minimal install installs SessionStart + Stop only
- Full install installs all 4 hooks
- Integration installs cleanly on fresh Codex setup
- Integration merges correctly with existing hooks
- `--from-stdin` correctly parses Codex JSON payloads
- All hooks execute and produce expected output
- Removal cleanly removes repo-graph hooks
- Project and global scopes work correctly
- Status correctly reports installed profile

## Out of Scope (CODEX-1)

- PreToolUse enforcement (future slice)
- PermissionRequest handling
- config.toml inline hooks (JSON file only for this slice)
- Trust-gated install prompts

## Sources

- [Hooks – Codex | OpenAI Developers](https://developers.openai.com/codex/hooks)
- [Configuration Reference – Codex | OpenAI Developers](https://developers.openai.com/codex/config-reference)
- [Advanced Configuration – Codex | OpenAI Developers](https://developers.openai.com/codex/config-advanced)
