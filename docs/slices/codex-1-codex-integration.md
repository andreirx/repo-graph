# CODEX-1: Codex CLI Integration

Status: PLANNED
Depends: HOST-1, HOOK-1
Track: Distribution / Install / Host Integration

**Execution order note:** Follows CLAUDE-1 in rollout sequence, but CLAUDE-1 is not
a build dependency. This slice implements HOST-1 contract for Codex CLI specifically.

## Objective

Implement repo-graph integration with OpenAI Codex CLI via lifecycle hooks.

## Assumptions to Verify (CRITICAL)

**Codex hook API appears less mature than Claude Code. The following details are
inferred from GitHub issues and partial documentation. Must be verified against
actual Codex CLI behavior before implementation.**

- Hook event names (may differ from Claude Code)
- Environment variable names (may differ from Claude Code)
- Single-hook-per-event vs array model
- Config file location and format
- Timeout behavior
- Matcher schema (if any)

Sources (verify currency):
- https://github.com/openai/codex
- GitHub issues: #16301, #16226, #14754

**Implementation should include discovery/probe phase to verify actual behavior
on installed Codex CLI before generating config.**

## Codex Hook Model

Codex CLI exposes lifecycle hooks via `hooks.json`:

```json
{
  "hooks": {
    "SessionStart": {...},
    "UserPromptSubmit": {...},
    "PreToolUse": {...},
    "PostToolUse": {...},
    "Stop": {...}
  }
}
```

Note: Codex hook model is similar to Claude Code but with some differences:
- Single hook per event (not array)
- Slightly different environment variables
- Less mature documentation

## Hook Entry Schema

```json
{
  "command": "shell command to execute",
  "timeout": 30000,
  "matcher": ["Edit", "Write"]
}
```

- `command`: Shell command (string)
- `timeout`: Milliseconds before timeout
- `matcher`: Array of tool names (PostToolUse only)

## Environment Variables

| Variable | Event | Description |
|----------|-------|-------------|
| `CODEX_SESSION_ID` | All | Session identifier |
| `CODEX_PROJECT_PATH` | All | Project root path |
| `TOOL_NAME` | PostToolUse | Tool that was used |
| `CHANGED_FILES` | PostToolUse | Space-separated file paths |
| `PROMPT` | UserPromptSubmit | User prompt |

**Note:** Variable names may differ from Claude Code. Verify against current Codex docs.

## Payload Transport Contract

Same as CLAUDE-1: hook commands read payloads from environment variables via `--from-env`,
not from command-line arguments.

**Codex-specific adaptation:** If Codex uses different variable names than Claude Code,
the `rmap hook` commands detect the host environment and read from the appropriate
variables. Variable name mapping is handled internally.

## repo-graph Hook Configuration

### Full Configuration

```json
{
  "hooks": {
    "SessionStart": {
      "command": "rmap hook session-start --from-env",
      "timeout": 30000
    },
    "UserPromptSubmit": {
      "command": "rmap hook prompt-submit --from-env",
      "timeout": 10000
    },
    "PostToolUse": {
      "matcher": ["Edit", "Write"],
      "command": "rmap hook post-edit --from-env",
      "timeout": 60000
    },
    "Stop": {
      "command": "rmap hook stop --from-env",
      "timeout": 30000
    }
  }
}
```

### Minimal Configuration

```json
{
  "hooks": {
    "SessionStart": {
      "command": "rmap hook session-start --from-env",
      "timeout": 30000
    },
    "Stop": {
      "command": "rmap hook stop --from-env",
      "timeout": 30000
    }
  }
}
```

## Configuration Locations

### Global

`~/.codex/hooks.json`

### Project

`.codex/hooks.json` in project root.

## Integration Commands

### rmap integrate codex

```
$ rmap integrate codex [--global|--project] [--minimal]

Options:
  --global    Install to ~/.codex/hooks.json (default)
  --project   Install to ./.codex/hooks.json
  --minimal   Only session-start and stop hooks
  --dry-run   Show changes without applying
  --force     Overwrite existing repo-graph hooks
```

### rmap integrate --remove codex

```
$ rmap integrate --remove codex [--global|--project]
```

### rmap integrate --status codex

```
$ rmap integrate --status codex

Codex Integration Status

Global (~/.codex/hooks.json):
  Status: installed
  Hooks:
    ✓ SessionStart: rmap hook session-start
    ✓ PostToolUse: rmap hook post-edit (Edit, Write)
    ✓ Stop: rmap hook stop
  Backup: ~/.codex/hooks.json.rmap-backup
  Installed: 2024-01-15T10:30:00Z

Project (./.codex/hooks.json):
  Status: not installed
```

## Merging with Existing Hooks

Since Codex uses single hooks per event (not arrays), merging is different:

### Strategy: Chain Commands

If existing hook present, chain with `&&`:

Before:
```json
{
  "hooks": {
    "SessionStart": {
      "command": "my-setup.sh",
      "timeout": 5000
    }
  }
}
```

After:
```json
{
  "hooks": {
    "SessionStart": {
      "command": "rmap hook session-start --repo \"$CODEX_PROJECT_PATH\" && my-setup.sh",
      "timeout": 35000
    }
  }
}
```

### Alternative: Wrapper Script

For complex existing hooks, create wrapper:

```bash
#!/bin/bash
# ~/.codex/rmap-wrapper.sh
rmap hook session-start --repo "$CODEX_PROJECT_PATH"
# Original hook
my-setup.sh
```

## Codex-Specific Considerations

### Hook API Maturity

Codex hook API may be less stable than Claude Code. Document:
- Tested Codex version
- Known limitations
- Workarounds

### Environment Variable Differences

Create adapter in `rmap hook` commands to handle:
- Different variable names
- Different data formats

```bash
# In rmap hook session-start
REPO_PATH="${CLAUDE_PROJECT_PATH:-${CODEX_PROJECT_PATH:-$(pwd)}}"
```

### PreCompact Not Available

Codex may not have PreCompact event. Omit from configuration.

## Testing

### Integration Tests

- Fresh integration on clean Codex setup
- Integration with existing hooks
- Command chaining works
- Integration removal

### End-to-End Tests

Requires Codex CLI installation:

1. Install integration
2. Start Codex session
3. Verify hooks execute
4. End session

## Error Handling

### Codex Not Installed

```
Warning: Codex CLI not detected
  No ~/.codex directory
  'codex' command not found

Integration will create ~/.codex/hooks.json
Proceed anyway? [y/N]
```

### Hook Conflicts

```
Existing hook found for SessionStart
  Current: my-setup.sh

Options:
  [1] Chain commands (rmap runs first)
  [2] Chain commands (rmap runs after)
  [3] Replace existing hook (backup preserved)
  [4] Skip this event

Choice:
```

## Known Limitations

Based on Codex GitHub issues:

1. **Single hook per event** — cannot have multiple independent hooks
2. **Variable parity gaps** — some Claude Code variables may not exist
3. **SubagentStop not available** — no subagent distinction
4. **Less stable API** — may change between versions

Document these and workarounds.

## Out of Scope (CODEX-1)

- Codex API version detection
- PreCompact hook (not available in Codex)
- Custom hook configuration UI

## Deliverables

1. `rmap integrate codex` command
2. `rmap integrate --remove codex` command
3. `rmap integrate --status codex` command
4. Hook configuration templates
5. Command chaining merge logic
6. Integration tests
7. Known limitations documentation

## Success Criteria

- Integration installs on Codex setup
- Hooks execute correctly
- Chaining with existing hooks works
- Removal restores previous state
- Limitations are documented
