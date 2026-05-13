# HOST-1: Host Integration Contract

Status: IMPLEMENTED
Depends: DIST-1
Track: Distribution / Install / Host Integration

## Objective

Define the contract for integrating repo-graph with agent host environments (Claude Code,
Codex CLI, Cursor). This is a design slice — it produces specifications that host-specific
implementation slices (CLAUDE-1, CODEX-1, CURSOR-1) build against.

## Host Classification

Agent hosts differ in their integration models. Do not force them into one pattern.

### Lifecycle Hook Hosts

These hosts expose event-driven lifecycle hooks that execute shell commands at specific
points in the agent workflow.

**Claude Code:**
- Hook events: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PreCompact`, `Stop`, `SubagentStop`
- Config: `.claude/settings.json` in project or `~/.claude/settings.json` global
- Hook format: shell command strings with environment variables

**Codex CLI:**
- Hook events: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`
- Config: `hooks.json` in project or `~/.codex/hooks.json` global
- Hook format: shell command strings

### Protocol/Rules Hosts

These hosts use different integration mechanisms — not lifecycle hooks.

**Cursor:**
- Integration: MCP (Model Context Protocol) server + Rules/AGENTS.md
- No lifecycle hooks in the Claude Code sense
- Integration is tool exposure + context rules, not event-driven commands

## Core Decisions

### D1: Thin Shim Model

Host-specific hook files are minimal shims. Policy lives in `rmap hook` commands.

**Rationale:**
- Keeps host-specific wiring thin and auditable
- Core logic is testable independent of host environment
- Supports multiple hosts without duplicating policy
- Updates to policy logic don't require re-patching host configs

**Contract:**
- Hook shims call `rmap hook <event> [args]`
- Shims pass relevant environment variables and context
- Shims do not contain conditional logic or policy decisions
- All policy decisions are in `rmap hook` implementation

### D2: Detect + Explicit Activation

Never silently rewrite developer automation configuration.

**Detection outputs:**
```
Detected agent hosts:
  [1] Claude Code
      Global config: ~/.claude/settings.json (exists)
      Project config: ./.claude/settings.json (not found)
      Status: No repo-graph hooks installed
  
  [2] Codex CLI
      Global config: ~/.codex/hooks.json (exists)
      Status: No repo-graph hooks installed
  
  [3] Cursor
      MCP config: ~/.cursor/mcp.json (exists)
      Status: No repo-graph MCP server configured
```

**Activation requires explicit user action:**
```
$ rmap integrate claude-code
This will modify: ~/.claude/settings.json
Backup will be created at: ~/.claude/settings.json.rmap-backup

Changes to be made:
  + Add SessionStart hook: rmap hook session-start
  + Add PostToolUse hook for Edit/Write: rmap hook post-edit
  + Add Stop hook: rmap hook stop

Proceed? [y/N]: y
```

### D3: Backup Before Patch

Always create backup before modifying host configuration.

**Backup naming:** `{original}.rmap-backup`
**Backup with timestamp for multiple:** `{original}.rmap-backup.{timestamp}`

**Backup is recorded in install manifest.**

### D4: Rollback Support

Every integration must be reversible.

**Default removal behavior: Surgical removal**

Removal surgically removes repo-graph hooks while preserving any non-repo-graph hooks
the user may have added after installation. This is safer than backup restore because
it doesn't destroy user changes made after repo-graph was installed.

```
$ rmap integrate claude-code remove
Claude Code integration removed
  Config: ~/.claude/settings.json
  Removed: 2 events
  Note: Non-repo-graph hooks preserved
```

**Backup purpose:**

The backup (created at install time) serves as:
1. Emergency recovery if removal corrupts the file
2. Reference for what the config looked like before repo-graph
3. Manual restore option if user explicitly wants pre-install state

**Manual backup restore (if needed):**

```
$ cp ~/.claude/settings.json.rmap-backup ~/.claude/settings.json
```

### D5: Project vs Global Scope

Support both project-level and user-global integrations.

**Project-level:** Integration config in project directory
- `.claude/settings.json` (Claude Code)
- `.codex/hooks.json` (Codex)
- Affects only that project

**Global-level:** Integration config in user home
- `~/.claude/settings.json` (Claude Code)
- `~/.codex/hooks.json` (Codex)
- Affects all projects without project-level config

**Default:** Global integration (affects all projects)
**Override:** `rmap integrate --project claude-code`

## Hook Event Mapping

### SessionStart

**When:** Agent session begins or resumes
**Purpose:** Orient agent on current repo state before any action
**`rmap hook` command:** `rmap hook session-start`

**Actions:**
1. Verify DB exists or prompt for index
2. Run `rmap trust` (lightweight)
3. Run `rmap orient`
4. Inject summary into context
5. Point agent at `CURRENT_SLICE.md` if present

### UserPromptSubmit (PrePrompt)

**When:** Before user prompt is processed
**Purpose:** Inject task-relevant context based on prompt content
**`rmap hook` command:** `rmap hook prompt-submit --prompt "$PROMPT"`

**Actions:**
1. Classify prompt (feature/bug/refactor/validation)
2. If code-relevant, run minimal orientation
3. Append relevant context (trust, surfaces, boundaries)
4. Optionally warn if preconditions missing

### PostToolUse (Edit/Write)

**When:** After file edit operations complete
**Purpose:** Keep index fresh, detect impact
**`rmap hook` command:** `rmap hook post-edit --files <paths>`

**Actions:**
1. Collect changed file paths from tool output
2. Run incremental refresh or mark dirty
3. Detect affected artifact families
4. Optionally rerun targeted checks

### PreCompact

**When:** Before context compaction
**Purpose:** Checkpoint state that would be lost
**`rmap hook` command:** `rmap hook pre-compact`

**Actions:**
1. Persist session summary
2. Capture changed files list
3. Capture current DB path
4. Capture trust/check summary
5. Update `CURRENT_SLICE.md` or session state file

### Stop

**When:** Agent task completion
**Purpose:** Validate and report
**`rmap hook` command:** `rmap hook stop`

**Actions:**
1. Run required validation commands
2. Produce validation transcript
3. Compare before/after if relevant
4. Warn if required checks not run

## Claude Code Hook Schema

**Schema source:** https://code.claude.com/docs/en/hooks (verified 2026-05-13)

**Transport:** Claude Code passes hook input as **JSON on stdin**, not environment
variables. Hook commands must use `--from-stdin` flag.

**Structure:** Hooks use matcher groups with nested `hooks` arrays. The `type` field
is required. Timeout is in **seconds**, not milliseconds.

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

**Matcher patterns:** String-based, not object. Use `|` for alternatives, regex for patterns.

## Codex Hook Schema

```json
{
  "hooks": {
    "SessionStart": {
      "command": "rmap hook session-start",
      "timeout": 30000
    },
    "PostToolUse": {
      "matcher": ["Edit", "Write"],
      "command": "rmap hook post-edit --files \"$CHANGED_FILES\"",
      "timeout": 60000
    },
    "Stop": {
      "command": "rmap hook stop",
      "timeout": 30000
    }
  }
}
```

## Cursor Integration Model

Cursor does not use lifecycle hooks. Integration is via:

1. **MCP Server:** Expose `rmap` commands as MCP tools
2. **Rules:** Project rules in `.cursor/rules` or `AGENTS.md`

**MCP tools to expose:**
- `rmap_orient` — orientation bundle
- `rmap_trust` — trust snapshot
- `rmap_callers` — caller lookup
- `rmap_callees` — callee lookup
- `rmap_boundaries` — boundary surfaces
- `rmap_check` — validation

**Rules content:**
```markdown
## repo-graph Integration

This project uses repo-graph for code intelligence.

Before making changes:
1. Call `rmap_orient` to understand current state
2. Check `rmap_trust` for known gaps

After making changes:
1. Call `rmap_check` to validate impact
```

## Host Detection

### Claude Code Detection

```
Locations to check:
  ~/.claude/settings.json (global)
  ./.claude/settings.json (project)

Detection criteria:
  - File exists
  - Valid JSON
  - May have existing hooks (check for conflicts)
```

### Codex Detection

```
Locations to check:
  ~/.codex/hooks.json (global)
  ./.codex/hooks.json (project)

Detection criteria:
  - File exists OR ~/.codex/ directory exists
  - Valid JSON if exists
```

### Cursor Detection

```
Locations to check:
  ~/.cursor/mcp.json (MCP config)
  ./.cursor/rules (project rules)

Detection criteria:
  - Cursor installation detected
  - MCP config location known
```

## Conflict Handling

### Existing Hooks

If host config already has hooks, merge rather than replace.

```
Existing hooks detected in ~/.claude/settings.json:
  SessionStart: "my-custom-script.sh"

Options:
  [1] Prepend repo-graph hooks (run before existing)
  [2] Append repo-graph hooks (run after existing)
  [3] Replace existing hooks (backup preserved)
  [4] Skip this host

Choice: 1
```

### Hook Name Conflicts

If `rmap hook` command already present, skip or update.

```
repo-graph hooks already installed in ~/.claude/settings.json
  Installed version: 0.1.0
  Current version: 0.2.0

Options:
  [1] Update hooks to current version
  [2] Keep existing hooks
  [3] Remove repo-graph hooks

Choice: 1
```

## Hook Input Transport

Hosts differ in how they provide context to hooks. This contract supports multiple
transport mechanisms, normalized internally.

### Transport Modes

| Host | Transport | Flag | Input Source |
|------|-----------|------|--------------|
| Claude Code | stdin JSON | `--from-stdin` | JSON payload on stdin |
| Codex | Environment vars | `--from-env` | Environment variables |
| Manual/Testing | Explicit args | `--db`, `--repo` | Command-line arguments |

### Claude Code Transport (stdin JSON)

Claude Code passes a JSON object on stdin containing session context:

```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit",
  "tool_input": {...},
  "tool_output": "..."
}
```

**Environment variables available (limited):**
- `CLAUDE_PROJECT_DIR` — project root directory
- `CLAUDE_ENV_FILE` — path to persist env vars (SessionStart only)

### Codex Transport (Environment Variables)

Codex provides context via environment variables (to be verified in CODEX-1):

- `CODEX_PROJECT_PATH` — project root path
- `CODEX_SESSION_ID` — session identifier
- `CHANGED_FILES` — edited file paths (PostToolUse)
- `PROMPT` — user prompt content (UserPromptSubmit)

### Internal Normalization

All transport modes normalize to a common `HookContext` structure inside `rmap hook`
commands. The policy handlers are transport-agnostic.

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  --from-stdin   │     │   --from-env    │     │  --db/--repo    │
│  (stdin JSON)   │     │  (env vars)     │     │  (explicit)     │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                        ┌─────────────────┐
                        │   HookContext   │
                        │  (normalized)   │
                        └────────┬────────┘
                                 ▼
                        ┌─────────────────┐
                        │  Policy Handler │
                        └─────────────────┘
```

**Contract:**
- Shims use the appropriate transport flag (`--from-stdin` or `--from-env`)
- `rmap hook` commands parse input according to the flag
- Core policy code uses only the normalized `HookContext`
- New host support requires only a new transport adapter, not policy changes

## Error Handling in Hooks

Hooks should fail gracefully — agent workflow continues.

**Hook failure behavior:**
- Log error to `~/.local/share/rmap/logs/hooks.log`
- Return non-zero exit code
- Do not block agent operation (informational mode)

**Future enforcement mode:**
- Specific hooks can be marked as blocking
- Failure prevents agent from proceeding
- Requires explicit opt-in

## Out of Scope (HOST-1)

- Implementation details for specific hosts (CLAUDE-1, CODEX-1, CURSOR-1)
- `rmap hook` command implementation (HOOK-1)
- Enforcement mode policy (future slice)

## Deliverables

1. This contract document (normative)
2. Hook event mapping specification
3. Host detection specification
4. Conflict resolution specification
5. Environment variable specification

## Success Criteria

- Contract is complete enough to implement CLAUDE-1, CODEX-1, CURSOR-1
- Thin shim model is unambiguous
- Backup/rollback behavior is specified
- Conflict handling is explicit
