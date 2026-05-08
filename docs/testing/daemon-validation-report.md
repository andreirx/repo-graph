# Daemon Validation Report (D6)

Status: COMPLETE
Date: 2026-05-07 (initial), 2026-05-08 (enrich added)
Validator: Claude Code

## Summary

The `rmap daemon` mode has been validated for functional parity, protocol correctness,
and coordination behavior. All core operations work correctly through the daemon transport.

## Validation Scope

### Methods Validated

| Method | Category | Status |
|--------|----------|--------|
| `ping` | Utility | PASS |
| `echo` | Utility | PASS |
| `load_repo` | Lifecycle | PASS |
| `unload_repo` | Lifecycle | PASS |
| `list_repos` | Lifecycle | PASS |
| `callers` | Graph query | PASS |
| `callees` | Graph query | PASS |
| `imports` | Graph query | PASS |
| `index` | Write operation | PASS |
| `refresh` | Write operation | PASS |
| `enrich` | Write operation | PASS |
| `orient` | Agent service | PASS |
| `check` | Agent service | PASS |
| `explain` | Agent service | PASS |

### Validation Repos

- `repo-graph` (self-index) — primary validation target

## Functional Parity Checks

### Write Operations

**`index`:**
- Creates new repo in database
- Returns `repo_uid` and `snapshot_uid`
- Progress events emitted: initializing, scanning, extracting, persisting
- Final response contains expected fields

**`refresh`:**
- Updates existing repo snapshot
- Requires DB write lock + repo refresh lock
- Progress events emitted correctly
- Snapshot status transitions to READY on success

**`enrich`:**
- Resolves receiver types via language-specific resolvers
- All parameters supported: snapshot_uid, languages, limit, promote, force, dry_run, jdtls_path
- Result JSON matches CLI `EnrichOutput` contract:
  - `by_language` as tuple array: `[["rust", {...}], ...]`
  - `top_failure_reasons` as tuple array: `[["reason", count], ...]`
  - No `dry_run` field in output (CLI parity)
- Progress phases: initializing, resolving, complete

### Agent Services

**`orient`:**
- Returns `rgr.agent.v1` schema envelope
- Focus parameter resolves correctly (file, path, symbol, repo)
- Budget parameter affects signal/limit truncation
- Signals include: IMPORT_CYCLES, MODULE_SUMMARY, SNAPSHOT_INFO, trust signals
- Limits include: MODULE_DATA_UNAVAILABLE, COMPLEXITY_UNAVAILABLE, GATE_NOT_CONFIGURED

**`check`:**
- Returns pre-action safety assessment
- Gate evaluation when configured
- Trust signals surfaced

**`explain`:**
- Returns deep-dive on target
- Callers/callees/imports sections populated
- Budget affects depth

### Graph Queries

**`callers`/`callees`:**
- Returns direct callers/callees of symbol
- Edge type filtering works (CALLS, INSTANTIATES)
- Target resolution via storage

**`imports`:**
- Returns imports for file
- File stable key construction correct

## Protocol Correctness

### NDJSON Transport

- Request/response correlation via `id` field: PASS
- Progress events before final response: PASS
- No stray output between requests: PASS
- Unknown method returns `UnknownMethod` error: PASS
- Missing params returns `InvalidRequest` error: PASS

### Progress Streaming

- Progress events include `id`, `progress` object
- Progress object has `phase`, `current`, `total`
- Final response has `id`, `result` (or `error`)
- Event ordering: all progress, then result

### Abort Checkpoints

- Transport failure (channel close) triggers abort: PASS
- `ComposeError::Aborted` propagates correctly: PASS
- Snapshot transitions to FAILED on abort during persist: PASS
- Abort is checkpoint-granular (documented limitation)

## Coordination Behavior

### Database-Level Coordination

- Single writer per database: PASS
- Write operations acquire `DbRuntime` write lock
- Multiple databases load independently

### Repo-Level Coordination

- Reader/writer semantics via `RepoCoordinator`: PASS
- Write operations acquire refresh lock after DB lock
- Concurrent reads do not block

### Multi-DB Isolation

- Different databases with same repo_uid are separate: PASS
- Composite identity (db_path + repo_uid) at API boundary

## Test Coverage

### Unit Tests

- `daemon-policy`: 45 tests (state machine, coordinator)
- `daemon-transport`: 33 tests (transport, progress, abort)

### Integration Tests

- `daemon_dispatch.rs`: 36 tests
  - Parameter validation for all methods
  - Error handling (repo not found, missing params)
  - End-to-end flows (index → load → refresh)
  - Progress streaming verification
  - Agent service contract validation
  - Enrich contract shape validation (4 tests)

## Known Limitations

1. **Abort granularity:** Checkpoint-granular, not instruction-granular. Between checkpoints,
   batch writes may complete partially. Mitigated by snapshot FAILED status.

2. **Cancellation (D5c):** Not implemented. Client-initiated cancel requires token threading.
   Deferred — will reuse abort checkpoint seam.

3. **No socket transport:** NDJSON over stdin/stdout only. No Unix socket or TCP mode.

## Deltas from CLI

### Intentional Differences

None. Daemon methods return identical JSON shapes to CLI commands.

### Superset Fields

- `callers`/`callees` include `target` field (CLI omits)
- `dead` includes `kind_filter` field (CLI omits)

These are additions, not contract breaks.

## Conclusion

The daemon is operationally ready for:
- Multi-agent read access to shared databases
- Serialized write operations (index, refresh, enrich)
- Agent discovery services (orient, check, explain)

D5c (cancellation) is deferred but not blocking for current use cases.
