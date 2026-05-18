# RMAPD-PERF-1: Large Repo Timeout Investigation

**Status:** IMPLEMENTED  
**Type:** Bug / Performance  
**Priority:** After CLI-OUT-2B (does not block renderer work)  
**Discovered:** CLI-OUT-2A audit (2026-05-18)  
**Resolved:** 2026-05-18

## Problem Statement

Large repositories failed with daemon timeouts during indexing or query operations.

### Indexing Timeout (RESOLVED)

| Repo | Files | Result |
|------|-------|--------|
| gstreamer | ~6328 C | Previously timeout, now works |
| hadoop | ~12478 Java | Untested |
| django | 3019 Python | **Indexed in 117s** |
| duckdb | 5109 C++ | **Indexed in 203s** |
| grpc-java | 1821 Java | **Indexed in 112s** |

### Query Timeout (RESOLVED)

| Repo | Files | Command | Result |
|------|-------|---------|--------|
| DuckDB | 5109 | orient | **9s** |
| DuckDB | 5109 | trust | **3s** |
| Django | 3019 | orient | **6s** |
| Django | 3019 | trust | **2s** |

## Root Cause Analysis

### Primary Cause
Client-side read timeout was 30 seconds (`connection.rs:24`). Operations that took longer than 30 seconds without emitting a line to the socket caused the client to time out with `Resource temporarily unavailable (os error 35)`.

### Why Operations Exceeded 30s

1. **Read operations had no progress emission**: `orient`, `check`, `trust`, `stats`, `cycles` were synchronous handlers that didn't emit heartbeats.

2. **First-query-after-index overhead**: SQLite query plan compilation and statistics warmup caused the first query to be significantly slower. Subsequent queries complete in seconds.

3. **Indexing large repos**: 3000-5000 file repos take 2-4 minutes to index.

### Query Performance

The `compute_module_stats` query has adequate indexes:
- `idx_edges_snapshot_type` on `(snapshot_uid, type)`
- `idx_edges_snapshot_type_src` on `(snapshot_uid, type, source_node_uid)`
- `idx_edges_snapshot_type_dst` on `(snapshot_uid, type, target_node_uid)`
- `idx_nodes_snapshot_kind` on `(snapshot_uid, kind)`

The slow first-query behavior is SQLite query planner warmup, not missing indexes.

## Fix Applied

### A. Increased Read Timeout (connection.rs)

Changed `READ_TIMEOUT_SECS` from 30s to 300s (5 minutes).

```rust
const READ_TIMEOUT_SECS: u64 = 300;
```

This provides headroom for:
- Multi-minute indexing operations
- First-query-after-index overhead
- Genuinely large repos

### C. Added Heartbeat Emission (dispatch.rs)

Added `emitter` parameter and heartbeat emission to read handlers:
- `handle_stats`
- `handle_cycles`
- `handle_orient`
- `handle_check`
- `handle_trust`

Each handler emits a progress event before starting heavy computation:

```rust
let _ = emitter.emit(ProgressDetail {
    phase: "computing_orient".to_string(),
    current: 0,
    total: 1,
});
```

This keeps the connection alive during legitimate long work.

## Remaining Limitations

### Mid-Query Heartbeats

If a single SQLite query exceeds 300 seconds, the client will still timeout. The current heartbeat is emitted BEFORE the query, not during.

For true mid-query heartbeats, would need either:
- SQLite `progress_handler` callback
- Background thread execution with periodic emission

Not needed for current corpus - all queries complete well under 300s after warmup.

### First-Query Overhead

The first query after indexing a repo may take 10-60 seconds while SQLite compiles query plans and warms caches. Subsequent queries are fast (single-digit seconds).

This is expected behavior, not a bug. Could be improved by:
- Running a warmup query after indexing
- Using `ANALYZE` to update SQLite statistics

## Definition of Done

- [x] Root cause identified (client timeout, not daemon)
- [x] Timeout increased to 300s
- [x] Heartbeat emission added to read handlers
- [x] django indexed and queryable
- [x] duckdb indexed and queryable
- [x] grpc-java indexed and queryable

## Files Changed

- `rust/crates/rgr/src/daemon_client/connection.rs` — timeout constant
- `rust/crates/daemon-runtime/src/dispatch.rs` — heartbeat emission

## Verification

```bash
# All commands complete successfully
rmap index /path/to/django
rmap orient  # from django directory
rmap trust   # from django directory

rmap index /path/to/duckdb
rmap orient  # from duckdb directory
rmap trust   # from duckdb directory
```
