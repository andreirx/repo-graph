# REG-1: Repo Registry and CWD Auto-Discovery

**Status:** PLANNING  
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
# Normal path (daemon resolves everything from cwd)
rmap index .
rmap orient
rmap check
rmap explain src/foo.ts

# Explicit repo selection (multi-repo scenarios)
rmap orient --repo my-alias
rmap orient --repo-path /abs/path/to/repo

# Diagnostic escape hatch (debug/recovery only, not documented as primary)
rmap orient --db /path/to/db --repo-uid <uid>
```

## Architectural Decisions (Locked)

| ID | Decision | Choice |
|----|----------|--------|
| D1 | Registry location | `registry.json` in platform data dir, daemon-owned, atomic-write |
| D2 | repo_uid format | Opaque stable generated ID (`repo_<ulid>`), never changes for registry entry lifetime |
| D3 | Worktree semantics | Separate registry entry per worktree path (W1 model) |
| D4 | CWD resolution | CLI sends canonicalized cwd to daemon; daemon resolves by registered ancestry |
| D5 | Alias support | Optional human-friendly names, unique per registry |
| D6 | repo_uid visibility | Internal only; visible in `--json` output and `doctor`, never required as input |
| D7 | Explicit override path | Advanced `--db`/`--repo-uid` flags for diagnostic targeting; old positional `<db_path> <repo_uid>` syntax removed |
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

```bash
# Set alias during index
rmap index . --alias my-app

# Use alias instead of path
rmap orient --repo my-app
```

Constraints:
- Aliases must be unique within registry
- Alias is optional (canonical_path is always the primary key)
- Alias can be changed: `rmap repo alias <path> <new-alias>`

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

**Retained as escape hatch:**
- `--repo-uid <uid>` flag for diagnostic/recovery scenarios
- Not documented in README or primary help text
- Documented in `rmap <cmd> --help` under "Advanced Options"

## CLI Contract Changes (D7)

### Commands Affected

| Command | Old Signature | New Signature |
|---------|--------------|---------------|
| `index` | `<repo_path> <db_path>` | `[repo_path]` (default: `.`) |
| `refresh` | `<db_path> <repo_uid>` | `[--repo <alias_or_path>]` |
| `orient` | `<db_path> <repo_uid>` | `[--repo <alias_or_path>]` |
| `check` | `<db_path> <repo_uid>` | `[--repo <alias_or_path>]` |
| `explain` | `<db_path> <repo_uid> <target>` | `<target> [--repo <alias_or_path>]` |
| `callers` | `<db_path> <repo_uid> <symbol>` | `<symbol> [--repo <alias_or_path>]` |
| `callees` | `<db_path> <repo_uid> <symbol>` | `<symbol> [--repo <alias_or_path>]` |
| ... | ... | ... |

### New Flags (All Query Commands)

```
--repo <alias_or_path>    Use specific repo (alias or path)
--repo-path <path>        Use repo at explicit path
--db <path>               [Advanced] Use explicit database file
--repo-uid <uid>          [Advanced] Use explicit repo_uid
```

### Flag Precedence

1. `--db` + `--repo-uid` (explicit escape hatch, bypasses registry)
2. `--repo-path` (resolve path, query registry)
3. `--repo` (resolve alias or path, query registry)
4. CWD resolution (default)

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

- [ ] Add `RepoRegistry` struct to daemon-runtime
- [ ] Implement registry persistence (load/save JSON, atomic write)
- [ ] Add `resolve_repo` request handler
- [ ] Add `list_repos` request handler
- [ ] Modify `index` to use registry (generate repo_uid, allocate db_path, create entry)
- [ ] Modify `refresh` to resolve via registry
- [ ] Add repo management requests (alias, remove, info)

### CLI Changes

- [ ] Add `--repo`, `--repo-path` flags to all query commands
- [ ] Add `--db`, `--repo-uid` escape hatch flags (advanced/diagnostic only)
- [ ] Implement CWD canonicalization (send to daemon for resolution)
- [ ] Add `rmap repo` subcommand group
- [ ] Implement `rmap repo list`
- [ ] Implement `rmap repo alias`
- [ ] Implement `rmap repo remove`
- [ ] Implement `rmap repo info`
- [ ] Update `index` to register repo (no db_path arg)
- [ ] Update all query commands to use resolution chain

### Documentation Changes

- [ ] Update README with new contract
- [ ] Update `docs/cli/rmap-contracts.md`
- [ ] Update slice docs referencing CLI contract

### Testing

- [ ] Registry persistence tests (load, save, atomic write)
- [ ] CWD resolution tests (exact match, ancestor match, not found)
- [ ] Alias uniqueness tests
- [ ] `rmap repo` subcommand tests
- [ ] `rmap index` creates registry entry test

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
