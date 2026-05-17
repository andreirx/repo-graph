# REG-1: Repo Registry and CWD Auto-Discovery

**Status:** IN PROGRESS  
**Priority:** BLOCKING (CLI contract debt)  
**Type:** Support Module + CLI Contract  

## Problem Statement

The current CLI exposes internal storage concepts that should be daemon-internal:

```bash
# Current (leaky) contract
rmap index ./path/to/repo ./repo.db
rmap orient ./repo.db pmc/2026-05-15T13:20:55.279Z/bf171385
```

Users must:
1. Choose and manage database file paths
2. Know and type internal `repo_uid` identifiers
3. Track the mapping between repos, databases, and UIDs

This contradicts the daemon-native product story. The daemon owns repo state;
the CLI should not expose SQLite plumbing.

## Target Contract

```bash
# Implemented: CWD resolution (daemon resolves repo from cwd)
rmap index .
rmap orient
rmap check
rmap explain src/foo.ts
```

**Deferred (not implemented in current REG-1):**
```bash
# Explicit repo selection (multi-repo scenarios) — DEFERRED
rmap orient --repo my-alias
rmap orient --repo-path /abs/path/to/repo

# Diagnostic escape hatch — DEFERRED
rmap orient --db /path/to/db --repo-uid <uid>
```

CWD resolution covers the primary use case. Override flags can be added later
if multi-repo scenarios require them.

## Architectural Decisions (Locked)

| ID | Decision | Choice |
|----|----------|--------|
| D1 | Registry location | `registry.json` in platform data dir, daemon-owned, atomic-write |
| D2 | repo_uid format | Opaque stable generated ID (`repo_<ulid>`), never changes for registry entry lifetime |
| D3 | Worktree semantics | Separate registry entry per worktree path (W1 model) |
| D4 | CWD resolution | CLI sends canonicalized cwd to daemon; daemon resolves by registered ancestry |
| D5 | Alias support | Optional human-friendly names, unique per registry |
| D6 | repo_uid visibility | Internal only; visible in `--json` output and `doctor`, never required as input |
| D7 | Explicit override path | Old positional `<db_path> <repo_uid>` syntax removed; advanced `--db`/`--repo-uid` flags DEFERRED |
| D8 | Database location | `<platform_data_dir>/databases/<hash>.db` (daemon-managed) |

## Registry Data Model (D1)

The daemon owns the registry. In-memory state is persisted to `registry.json` with
atomic writes (write to temp file, rename). The registry contains metadata only —
no graph data.

### Registry Entry

```json
{
  "canonical_path": "/Users/alice/projects/my-app",
  "alias": "my-app",
  "db_path": "/Users/alice/.local/share/rmap/databases/a1b2c3d4.db",
  "repo_uid": "repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4",
  "last_indexed_at": "2026-05-15T10:30:00.000Z",
  "last_snapshot_uid": "repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4/2026-05-15T10:30:00.000Z/abc123",
  "loaded": true
}
```

### Registry File

Location: `~/.local/share/rmap/registry.json` (Linux) or 
`~/Library/Application Support/repo-graph/registry.json` (macOS)

```json
{
  "version": 1,
  "repos": [
    { "canonical_path": "...", "alias": "...", "db_path": "...", ... }
  ]
}
```

### In-Memory State

Daemon loads registry on startup. Additional fields in memory:
- `loaded`: whether repo is currently loaded into daemon's in-memory graph
- `connection_count`: active CLI connections querying this repo

## Database Path Management (D8)

Databases live in a standard location, not user-specified paths.

| Platform | Database Directory |
|----------|-------------------|
| Linux | `~/.local/share/rmap/databases/` |
| macOS | `~/Library/Application Support/repo-graph/databases/` |

Database filename: hash of canonical repo path (e.g., `sha256(path)[0:16].db`)

Benefits:
- User never sees or manages DB paths
- No path collisions
- Consistent backup/cleanup location
- Portable across machines if registry is recreated

## Canonical Path Resolution

When a repo path is provided (explicitly or via cwd), resolve to canonical form:

1. Resolve to absolute path
2. Follow symlinks
3. Normalize (remove `..`, `.`, trailing slashes)
4. Result is the registry key

Example:
```
Input: ./my-app
CWD: /Users/alice/projects
Canonical: /Users/alice/projects/my-app
```

## CWD Resolution Algorithm (D4)

When CLI is invoked without explicit repo:

```
1. CLI gets cwd, canonicalizes it (resolve symlinks, absolute path)
2. CLI sends canonicalized path to daemon: resolve_repo(path)
3. Daemon resolves by registry lookup (see below)
4. Daemon returns (db_path, repo_uid) or "not indexed"
```

### Daemon Resolution

Daemon receives canonical path, resolves by ancestry:

1. If exact match in registry: return entry
2. Otherwise, find longest registered ancestor prefix:
   - For path `/Users/alice/projects/my-app/src/core/auth.ts`
   - Check `/Users/alice/projects/my-app/src/core` — not registered
   - Check `/Users/alice/projects/my-app/src` — not registered
   - Check `/Users/alice/projects/my-app` — registered, return entry
3. If no ancestor match: return "not indexed" error

The CLI does NOT walk markers. The daemon owns the registry and resolves
all paths against registered canonical paths.

## Alias Support (D5)

Optional human-friendly names for repos.

**Implemented:**
```bash
# Set alias during index
rmap index . --alias my-app

# Manage aliases
rmap repo alias <path> <new-alias>
rmap repo list  # shows aliases
```

**Deferred:**
```bash
# Use alias for query commands — DEFERRED (requires --repo flag)
rmap orient --repo my-app
```

Constraints:
- Aliases must be unique within registry
- Alias is optional (canonical_path is always the primary key)
- Query commands currently resolve from CWD only; `--repo` flag is deferred

## repo_uid Visibility (D6)

`repo_uid` is a stable opaque identifier generated once when a repo is first indexed.
Format: `repo_<ulid>` (e.g., `repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4`). Never changes for the
lifetime of the registry entry.

`snapshot_uid` is per-index: `<repo_uid>/<iso-timestamp>/<short-hash>`

**Visible in:**
- `rmap doctor --json` output
- `rmap repo list --json` output
- Debug logs
- Error messages (for support/debugging)

**Never required as input for:**
- Normal orient/check/explain commands
- Any documented primary workflow

**Escape hatch (DEFERRED):**
- `--repo-uid <uid>` flag for diagnostic/recovery scenarios — not yet implemented
- When implemented: not documented in README, only in `rmap <cmd> --help` under "Advanced Options"

## CLI Contract Changes (D7)

### Commands Affected

| Command | Old Signature | Implemented Now | Deferred |
|---------|--------------|-----------------|----------|
| `index` | `<repo_path> <db_path>` | `[repo_path]` (default: `.`) | `--alias` works |
| `refresh` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `orient` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `check` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `explain` | `<db_path> <repo_uid> <target>` | `<target>` (cwd-based) | `[--repo <alias_or_path>]` |
| `callers` | `<db_path> <repo_uid> <symbol>` | `<symbol>` (cwd-based) | `[--repo <alias_or_path>]` |
| `callees` | `<db_path> <repo_uid> <symbol>` | `<symbol>` (cwd-based) | `[--repo <alias_or_path>]` |
| `path` | `<db_path> <repo_uid> <from> <to>` | `<from> <to>` (cwd-based) | `[--repo <alias_or_path>]` |
| `imports` | `<db_path> <repo_uid> <file>` | `<file>` (cwd-based) | `[--repo <alias_or_path>]` |
| `cycles` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `stats` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `trust` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |
| `gate` | `<db_path> <repo_uid>` | (no args, cwd-based) | `[--repo <alias_or_path>]` |

### New Flags (All Query Commands)

**DEFERRED (2026-05-17):** The explicit override flags below were originally planned
but are deferred. CWD resolution covers the primary use case. Override flags are
escape hatches for multi-repo scenarios; they can be added later if needed.

```
--repo <alias_or_path>    Use specific repo (alias or path)     [DEFERRED]
--repo-path <path>        Use repo at explicit path             [DEFERRED]
--db <path>               [Advanced] Use explicit database file [DEFERRED]
--repo-uid <uid>          [Advanced] Use explicit repo_uid      [DEFERRED]
```

### Current Resolution (Implemented)

All query commands resolve repo from CWD via daemon registry.
No explicit repo selection flags are implemented.

### Flag Precedence (When Implemented)

1. `--db` + `--repo-uid` (explicit escape hatch, bypasses registry)
2. `--repo-path` (resolve path, query registry)
3. `--repo` (resolve alias or path, query registry)
4. CWD resolution (default) — **currently the only implemented path**

## Daemon Protocol Additions

### New Request: `resolve_repo`

```json
{"method": "resolve_repo", "params": {"path": "/abs/path/to/repo"}}
```

Response (success):
```json
{
  "result": {
    "canonical_path": "/abs/path/to/repo",
    "alias": "my-app",
    "db_path": "/Users/alice/.local/share/rmap/databases/a1b2c3.db",
    "repo_uid": "repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4",
    "last_indexed_at": "2026-05-15T10:30:00.000Z",
    "last_snapshot_uid": "repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4/2026-05-15T10:30:00.000Z/abc123"
  }
}
```

Response (not found):
```json
{
  "error": {
    "code": "RepoNotIndexed",
    "message": "repo not indexed: /abs/path/to/repo",
    "hint": "run: rmap index /abs/path/to/repo"
  }
}
```

### New Request: `list_repos`

```json
{"method": "list_repos", "params": {}}
```

Response:
```json
{
  "result": {
    "repos": [
      {"canonical_path": "...", "alias": "...", "last_indexed_at": "..."},
      ...
    ]
  }
}
```

### Modified Request: `index`

Old:
```json
{"method": "index", "params": {"repo_path": "...", "db_path": "..."}}
```

New:
```json
{"method": "index", "params": {"repo_path": "...", "alias": "my-app"}}
```

Daemon:
1. Canonicalizes repo_path
2. Generates db_path in standard location
3. Creates registry entry
4. Performs index
5. Returns repo_uid (for logging/debug, not user input)

## New CLI Commands

### `rmap repo list`

List all indexed repos.

```bash
$ rmap repo list
ALIAS       PATH                              LAST INDEXED
my-app      /Users/alice/projects/my-app      2026-05-15 10:30
backend     /Users/alice/projects/backend     2026-05-14 09:15

$ rmap repo list --json
[{"canonical_path": "...", "alias": "...", ...}]
```

### `rmap repo alias`

Set or change alias.

```bash
$ rmap repo alias /Users/alice/projects/my-app my-app
Alias set: my-app -> /Users/alice/projects/my-app

$ rmap repo alias . backend
Alias set: backend -> /Users/alice/projects/backend
```

### `rmap repo remove`

Remove repo from registry (optionally delete database).

```bash
$ rmap repo remove my-app
Removed from registry: my-app
Database retained: /Users/alice/.local/share/rmap/databases/a1b2c3.db

$ rmap repo remove my-app --delete-db
Removed from registry: my-app
Database deleted: /Users/alice/.local/share/rmap/databases/a1b2c3.db
```

### `rmap repo info`

Show details for a repo.

```bash
$ rmap repo info
Repo: /Users/alice/projects/my-app
Alias: my-app
Repo UID: repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4
Database: /Users/alice/.local/share/rmap/databases/a1b2c3.db
Last indexed: 2026-05-15 10:30:00
Last snapshot: repo_01JV9Y8YJ6Y6D6W8K7M4N2P3Q4/2026-05-15T10:30:00.000Z/abc123
Loaded: yes
```

## Migration Path

There is no legacy DB adoption path. CLI migration is re-index + new invocation shape.
Old databases are disposable cache. Old positional `<db_path> <repo_uid>` syntax is removed.

### User workflow

1. Install/update `rmap`
2. Run `rmap index .` in each repo
3. Use `rmap orient`, `rmap check`, `rmap explain ...`
4. Delete old `.db` files whenever convenient

### Non-goals

- Importing old databases
- Preserving old local cache state
- Multi-repo DB migration
- Any form of legacy DB adoption

Old DBs are regenerable artifacts. Re-indexing takes seconds (minutes for very large repos).
Do not architect around cache nostalgia.

## Implementation Checklist

### Daemon Changes

- [x] Add `RepoRegistry` struct to daemon-runtime
- [x] Implement registry persistence (load/save JSON, atomic write)
- [x] Add `resolve_repo` request handler
- [x] Add `list_repos` request handler
- [x] Modify `index` to use registry (generate repo_uid, allocate db_path, create entry)
- [x] Modify `refresh` to resolve via registry
- [x] Add repo management requests (alias, remove, info)
- [x] Add `resolve_and_load_repo()` helper for REG-1 pattern

### CLI Changes — Command Migration

**Migrated to REG-1 (cwd-based, no db_path/repo_uid):**
- [x] `surfaces list`
- [x] `surfaces show`
- [x] `boundaries list`
- [x] `boundaries show`
- [x] `boundaries summary`
- [x] `boundaries links`
- [x] `modules list`
- [x] `modules show`
- [x] `modules files`
- [x] `modules deps`
- [x] `modules violations`
- [x] `modules unowned`
- [x] `orient`
- [x] `check`
- [x] `explain`
- [x] `callers`
- [x] `callees`
- [x] `path`
- [x] `imports`
- [x] `cycles`
- [x] `stats`
- [x] `trust`
- [x] `gate`
- [x] `deps` (family: list, why, drift)
- [x] `docs`
- [x] `contracts`
- [x] `inferences`
- [x] `resource`
- [x] `dead` (disabled, not legacy — false positive rates)

**Still legacy (db_path + repo_uid required):**
- [ ] `violations` (top-level, separate from `modules violations`)

**Legacy write operations (not planned for REG-1):**
- [ ] `modules boundary`
- [ ] `assess`
- [ ] `policy`
- [ ] `enrich`
- [ ] `quality/*` (churn, coverage, hotspots, metrics, risk)
- [ ] `declare/*` (all declaration commands)

### CLI Changes — Infrastructure

- [x] Implement DaemonClient for socket communication
- [x] Implement CWD canonicalization (send to daemon for resolution)
- [x] Add `rmap repo` subcommand group
- [x] Implement `rmap repo list`
- [x] Implement `rmap repo alias`
- [x] Implement `rmap repo remove`
- [x] Implement `rmap repo info`
- [ ] Add `--repo`, `--repo-path` flags to remaining commands
- [ ] Add `--db`, `--repo-uid` escape hatch flags (advanced/diagnostic only)

### Documentation Changes

- [ ] Update README with new contract
- [ ] Update `docs/cli/rmap-contracts.md`
- [ ] Update slice docs referencing CLI contract

### Testing

- [x] Registry persistence tests (load, save, atomic write)
- [x] CWD resolution tests (exact match, ancestor match, not found)
- [x] Alias uniqueness tests
- [x] `rmap repo` subcommand tests
- [x] `rmap index` creates registry entry test
- [x] daemon_dispatch.rs tests for migrated commands (87 tests)

**Ignored Tests Audit (71 remaining after cleanup):**

| Category | Count | Status |
|----------|-------|--------|
| gate_command.rs | 43 | Real implementations using old CLI contract - needs migration |
| dead_command.rs | 11 | Command disabled (not REG-1 related) - keep ignored |
| index_contract_summary.rs | 9 | Real implementations using old CLI contract |
| declare_* tests | 8 | Legacy write operations - keep ignored |

**Completed:**
- [x] Delete 46 stub tests (orient, path, imports, stats, explain, trust, edge_type_filter, envelope_contract)

**Remaining:**
- [ ] Migrate gate_command.rs tests to daemon_dispatch.rs (43 tests)
- [ ] Migrate index_contract_summary.rs tests to REG-1 contract (9 tests)

## Definition of Done

1. `rmap index .` works without specifying db_path (daemon allocates)
2. `rmap orient` from within indexed repo works without any arguments
3. `repo_uid` never appears in normal `--help` output
4. `rmap repo list` shows all indexed repos
5. Old positional `<db_path> <repo_uid>` syntax removed (intentional CLI break)
6. All existing tests pass (with updated signatures)
7. README shows only new contract

## Open Questions

1. **Moved repos:** What happens if user moves repo directory?
   - Proposal: Registry entry becomes stale; `rmap repo list` shows warning;
     user runs `rmap repo remove` + `rmap index` at new location.

No other open questions. Multi-worktree is resolved (D3: W1 model).
Shared databases are explicitly not supported (re-index on new machine).
