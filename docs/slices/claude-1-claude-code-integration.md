# CLAUDE-1: Claude Code Integration

Status: PLANNED
Depends: HOST-1, HOOK-1
Track: Distribution / Install / Host Integration

**Execution order note:** Follows MAC-1 in rollout sequence (macOS-first), but MAC-1
is not a build dependency. This slice implements HOST-1 contract for Claude Code
specifically. Platform installers (MAC-1, LINUX-1) call `rmap integrate claude-code`.

## Objective

Implement repo-graph integration with Claude Code via lifecycle hooks. This slice
produces the Claude Code-specific hook configuration and integration tooling.

## Assumptions to Verify

**The following are based on Anthropic documentation as of 2024. Verify against
actual Claude Code behavior before implementation.**

- Hook event names and schema
- Environment variable names and contents
- Timeout behavior and defaults
- Matcher schema for PostToolUse
- Config file merge behavior

Sources:
- https://docs.anthropic.com/en/docs/claude-code/hooks-guide
- https://docs.anthropic.com/en/docs/claude-code/hooks

## Claude Code Hook Model

Claude Code exposes lifecycle hooks via `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [...],
    "UserPromptSubmit": [...],
    "PreToolUse": [...],
    "PostToolUse": [...],
    "PreCompact": [...],
    "Stop": [...],
    "SubagentStop": [...]
  }
}
```

Each hook is an array of hook entries. Entries execute in order.

## Hook Entry Schema

```json
{
  "command": "shell command to execute",
  "timeout": 30000,
  "matcher": {
    "tool_name": ["Edit", "Write"]
  }
}
```

- `command`: Shell command (string)
- `timeout`: Milliseconds before timeout (optional, default varies)
- `matcher`: Filter for when hook runs (optional, event-specific)

## Environment Variables

Claude Code provides context via environment variables:

| Variable | Event | Description |
|----------|-------|-------------|
| `CLAUDE_SESSION_ID` | All | Unique session identifier |
| `CLAUDE_PROJECT_PATH` | All | Project root path |
| `TOOL_NAME` | PostToolUse | Name of tool that was used |
| `TOOL_INPUT` | PostToolUse | Tool input (JSON) |
| `TOOL_OUTPUT` | PostToolUse | Tool output (may include file paths) |
| `PROMPT_TEXT` | UserPromptSubmit | User prompt content |

## Payload Transport Contract

**Problem:** Passing large or multiline payloads (prompt text, tool output) as shell
command arguments is fragile due to quoting, escaping, and length limits.

**Solution:** Hook commands read payloads from environment variables directly, not
from command-line arguments.

| Payload | Transport | Consumed by |
|---------|-----------|-------------|
| Project path | Env var `CLAUDE_PROJECT_PATH` | All hooks |
| Session ID | Env var `CLAUDE_SESSION_ID` | All hooks |
| Prompt text | Env var `PROMPT_TEXT` | `prompt-submit` |
| Tool output | Env var `TOOL_OUTPUT` | `post-edit` |
| Tool name | Env var `TOOL_NAME` | `post-edit` |

Hook commands use `--from-env` flag to indicate environment variable consumption:

```bash
rmap hook session-start --from-env
# Reads: CLAUDE_PROJECT_PATH, CLAUDE_SESSION_ID

rmap hook prompt-submit --from-env
# Reads: CLAUDE_PROJECT_PATH, PROMPT_TEXT

rmap hook post-edit --from-env
# Reads: CLAUDE_PROJECT_PATH, TOOL_OUTPUT, TOOL_NAME
```

## repo-graph Hook Configuration

### Full Configuration

```json
{
  "hooks": {
    "SessionStart": [
      {
        "command": "rmap hook session-start --from-env",
        "timeout": 30000
      }
    ],
    "UserPromptSubmit": [
      {
        "command": "rmap hook prompt-submit --from-env",
        "timeout": 10000
      }
    ],
    "PostToolUse": [
      {
        "matcher": {
          "tool_name": ["Edit", "Write", "MultiEdit"]
        },
        "command": "rmap hook post-edit --from-env",
        "timeout": 60000
      }
    ],
    "PreCompact": [
      {
        "command": "rmap hook pre-compact --from-env",
        "timeout": 10000
      }
    ],
    "Stop": [
      {
        "command": "rmap hook stop --from-env",
        "timeout": 30000
      }
    ]
  }
}
```

### Minimal Configuration

For users who want less integration:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "command": "rmap hook session-start --from-env",
        "timeout": 30000
      }
    ],
    "Stop": [
      {
        "command": "rmap hook stop --from-env",
        "timeout": 30000
      }
    ]
  }
}
```

## Configuration Locations

### Global Configuration

`~/.claude/settings.json`

Applies to all projects without project-level config.

### Project Configuration

`.claude/settings.json` in project root.

Overrides global config for that project.

## Integration Commands

### rmap integrate claude-code

```
$ rmap integrate claude-code [--global|--project] [--minimal]

Options:
  --global    Install to ~/.claude/settings.json (default)
  --project   Install to ./.claude/settings.json
  --minimal   Only session-start and stop hooks
  --dry-run   Show changes without applying
  --force     Overwrite existing repo-graph hooks
```

**Behavior:**

1. Check for existing config file
2. If exists, check for existing hooks
3. Show planned changes
4. Create backup
5. Merge hooks into config
6. Verify config is valid JSON
7. Record in install manifest

### rmap integrate --remove claude-code

```
$ rmap integrate --remove claude-code [--global|--project]
```

**Behavior:**

1. Find config file
2. Remove repo-graph hooks only
3. Restore from backup if available
4. Update install manifest

### rmap integrate --status claude-code

```
$ rmap integrate --status claude-code

Claude Code Integration Status

Global (~/.claude/settings.json):
  Status: installed
  Hooks:
    ✓ SessionStart: rmap hook session-start
    ✓ PostToolUse: rmap hook post-edit (Edit, Write, MultiEdit)
    ✓ PreCompact: rmap hook pre-compact
    ✓ Stop: rmap hook stop
  Backup: ~/.claude/settings.json.rmap-backup
  Installed: 2024-01-15T10:30:00Z

Project (./.claude/settings.json):
  Status: not installed
```

## Merging with Existing Hooks

### Existing Hooks Present

If Claude Code config already has hooks, merge rather than replace.

**Strategy:**
1. Prepend repo-graph hooks (run first)
2. Preserve all existing hooks
3. Avoid duplicates

**Example:**

Before:
```json
{
  "hooks": {
    "SessionStart": [
      {"command": "my-custom-setup.sh", "timeout": 5000}
    ]
  }
}
```

After:
```json
{
  "hooks": {
    "SessionStart": [
      {"command": "rmap hook session-start --repo \"$CLAUDE_PROJECT_PATH\"", "timeout": 30000},
      {"command": "my-custom-setup.sh", "timeout": 5000}
    ]
  }
}
```

### repo-graph Hooks Already Present

If repo-graph hooks are already installed:

```
repo-graph hooks already installed in ~/.claude/settings.json
  Installed version: 0.1.0
  Current version: 0.2.0

Options:
  [1] Update to current version
  [2] Keep existing
  [3] Remove repo-graph hooks

Choice:
```

## Hook Output Handling

### stdout

Hook stdout is captured and may be injected into context (depends on Claude Code version).

repo-graph hooks output:
- Human-readable by default
- JSON with `--json` flag
- Compact summaries suitable for context injection

### stderr

Hook stderr goes to Claude Code's log.

repo-graph hooks log errors to:
- stderr (captured by Claude Code)
- `~/.local/share/rmap/logs/hooks.log`

### Exit Codes

| Code | Meaning | Claude Code Behavior |
|------|---------|---------------------|
| 0 | Success | Continue |
| 1 | Warning | Continue (log warning) |
| 2 | Error | Continue (log error) |

Note: In informational mode, hooks never block Claude Code operation.
Enforcement mode (future) may change this.

## Testing

### Unit Tests

- Config file parsing
- Hook merging logic
- Backup/restore logic
- Environment variable handling

### Integration Tests

- Fresh integration on clean config
- Integration with existing hooks
- Integration update
- Integration removal
- Project vs global scope

### End-to-End Tests

Requires Claude Code installation:

1. Install integration
2. Start Claude Code session
3. Verify session-start hook runs
4. Make edits
5. Verify post-edit hook runs
6. End session
7. Verify stop hook runs

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

Try:
  chmod u+w ~/.claude/settings.json
```

### Claude Code Not Detected

```
Warning: Claude Code installation not detected
  No ~/.claude directory
  No Claude.app in /Applications

Integration will create ~/.claude/settings.json
Proceed anyway? [y/N]
```

## Version Compatibility

### Claude Code Hook API Changes

If Claude Code changes its hook API:

1. Detect Claude Code version (if possible)
2. Use appropriate hook schema
3. Warn if version unknown

### repo-graph Hook Command Changes

Hook commands should be stable. If commands change:

1. New version uses new commands
2. `rmap integrate --update` updates hook commands
3. Old commands continue working (deprecated)

## Documentation

### User-Facing

Add to repo-graph docs:

```markdown
## Claude Code Integration

repo-graph integrates with Claude Code via lifecycle hooks.

### Quick Start

```bash
# Install repo-graph with Claude Code integration
curl -fsSL https://raw.githubusercontent.com/{OWNER}/repo-graph/main/scripts/install.sh | bash

# Or add integration to existing installation
rmap integrate claude-code
```

### What It Does

- **Session Start:** Orients the agent on repo structure before any action
- **Post Edit:** Refreshes index after file changes
- **Pre Compact:** Checkpoints state before context compaction
- **Stop:** Validates and summarizes at task completion

### Configuration

Global: `~/.claude/settings.json`
Project: `./.claude/settings.json`

### Removing Integration

```bash
rmap integrate --remove claude-code
```
```

## Out of Scope (CLAUDE-1)

- Claude Code API changes tracking
- SubagentStop hook (same as Stop for now)
- PreToolUse hook (future enforcement)
- Custom hook timeout configuration

## Deliverables

1. `rmap integrate claude-code` command
2. `rmap integrate --remove claude-code` command
3. `rmap integrate --status claude-code` command
4. Hook configuration templates
5. Merge logic for existing configs
6. Backup/restore logic
7. Integration tests
8. User documentation

## Success Criteria

- Integration installs cleanly on fresh Claude Code setup
- Integration merges correctly with existing hooks
- All hooks execute and produce expected output
- Removal cleanly restores previous state
- Project and global scopes work correctly
