# STDIO-STATE-ROOT-1: Sandbox-Writable State Root for Stdio Transport

**Status:** COMPLETE (2026-05-26)
**Depends on:** STDIO-TRANSPORT-1 (complete)

## Problem

Evidence from glamCRM Codex shell (2026-05-26):

```
note: socket connection denied (sandbox), using stdio transport
error: failed to open database: attempt to write a readonly database
```

Transport fallback works. The sandboxed `rmapd --stdio` subprocess inherits
Codex filesystem restrictions. It cannot write to the normal per-user state root:

```
~/Library/Application Support/repo-graph/databases/...
```

That path is outside the sandbox's writable roots. SQLite opens read-write
for migrations/WAL, fails with `SQLITE_READONLY`.

## Root Cause

The launchd daemon runs outside the sandbox with full filesystem access.
The stdio subprocess spawned inside the sandbox inherits restricted access.

Same binary, different execution context, incompatible state-root assumptions.

## Design Decision

**Option B: Automatic sandbox-local temp state root**

When stdio transport is activated due to EPERM/EACCES:
1. Honor explicit `RMAP_STATE_ROOT` if set
2. Otherwise, spawn `rmapd --stdio` with injected sandbox-writable root

Sandbox-writable root: `/private/tmp/repo-graph-agent/<uid>`

### Why This Architecture

- Sandbox stdio mode is a fallback execution mode
- Its state should be treated as sandbox-local derived cache, not global authority
- Temp-root isolation keeps the architecture honest
- Avoids polluting user repos
- Fits the layered fact model (sandbox state is ephemeral orientation data)

### What This Means

| Context | State Root | Shared? |
|---------|-----------|---------|
| launchd daemon | `~/Library/Application Support/repo-graph/` | Yes (global) |
| stdio (explicit) | `RMAP_STATE_ROOT` if set | User-controlled |
| stdio (sandbox fallback) | `/private/tmp/repo-graph-agent/<uid>/` | No (isolated) |

Agents using stdio sandbox mode get isolated state. They may need to reindex
if temp is cleaned. This is acceptable for orientation use cases.

## Implementation

### 1. State Root Environment Variable

`rmapd` already respects `RMAP_STATE_ROOT` for storage location.
Verify this works and document it.

### 2. Client-Side Injection

In `StdioTransport::spawn()`:
- If spawning due to EPERM/EACCES fallback (not explicit `RMAP_TRANSPORT=stdio`)
- And `RMAP_STATE_ROOT` is not already set
- Inject `RMAP_STATE_ROOT=/private/tmp/repo-graph-agent/<uid>` into subprocess env

### 3. Sandbox Root Creation

Before spawning:
- Ensure `/private/tmp/repo-graph-agent/<uid>` exists
- Create with mode 0700 (user-only)

### 4. Doctor Output

`rmap doctor` should report:
- `state_root: <path>` (active state root)
- `state_root_mode: global | sandbox-local | override` (why this root)

### 5. Warning on Fallback

When sandbox fallback activates, emit:
```
note: socket connection denied (sandbox), using stdio transport
note: using sandbox-local state root: /private/tmp/repo-graph-agent/<uid>
```

## Deliverables

| Item | Description |
|------|-------------|
| State root injection | StdioTransport spawns with sandbox-writable root |
| Override precedence | Explicit `RMAP_STATE_ROOT` takes priority |
| Sandbox root creation | Create temp directory with correct permissions |
| Doctor probe | Report active state root and mode |
| Warning output | Inform user when sandbox-local state is used |
| glamCRM validation | Full command suite works from Codex shell |

## Out of Scope

- Workspace-local state (repo pollution, lifecycle complexity)
- Read-only global state access (split-brain risk)
- Syncing sandbox state to global daemon (architecture violation)

## Definition of Done

1. `rmap index .` succeeds from glamCRM Codex shell
2. `rmap check` succeeds from glamCRM Codex shell
3. `rmap orient` succeeds from glamCRM Codex shell
4. `rmap doctor` shows `state_root_mode: sandbox-local`
5. Explicit `RMAP_STATE_ROOT` override works
6. All tests pass

## Technical Notes

### Why /private/tmp

- `/private/tmp` is writable from Codex sandbox
- `/tmp` symlinks to `/private/tmp` on macOS
- Using absolute `/private/tmp` avoids symlink resolution issues
- Per-uid subdirectory prevents cross-user conflicts

### State Isolation Implications

Sandbox stdio mode state is:
- Not shared with launchd daemon
- Not persisted across temp cleanup
- Specific to the sandbox session

This is acceptable because:
- Agent orientation is ephemeral
- Reindexing is fast for typical repos
- The alternative (no orientation) is worse

### Future Consideration

If agents need persistent cross-session state, consider:
- Explicit state root configuration in agent integration
- Workspace-local state (with lifecycle management)
- State export/import commands

Not in this slice.
