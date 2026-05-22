# LIVE-GRAPH-2: Migrate Graph Commands to LiveGraph

## Status

QUEUED — specification complete, not yet implemented.

## Problem Statement

With LiveGraph infrastructure in place (LIVE-GRAPH-1), the first command family can migrate from SQLite to in-memory traversal. This proves the architecture and establishes the migration pattern.

## Scope

### In Scope

- Migrate `callers` command to use LiveGraph
- Migrate `callees` command to use LiveGraph
- Migrate `path` command to use LiveGraph
- Fallback to SQLite if LiveGraph unavailable (e.g., cold start race)
- Performance comparison: LiveGraph vs SQLite latency

### Out of Scope

- `cycles` command (more complex SCC algorithm — defer to LIVE-GRAPH-3)
- `dead` command (reachability from roots — defer to LIVE-GRAPH-3)
- Policy/governance commands (remain SQLite)
- Multi-snapshot queries (remain SQLite)

## Definition of Done

1. `callers`, `callees`, `path` use LiveGraph when available
2. Graceful fallback to SQLite if LiveGraph not loaded
3. No user-visible behavior change (same output format)
4. Latency improvement documented (before/after on reference repos)
5. All existing CLI tests pass

## Validation Plan

1. Run existing `callers_command.rs`, `callees_command.rs`, `path_command.rs` tests
2. Measure latency on repo-graph: 10 callers queries before/after
3. Verify fallback: stop daemon, query before LiveGraph loads, confirm SQLite path works
4. Smoke test on 3 validation repos

## Dependencies

- LIVE-GRAPH-1 (LiveGraph infrastructure)
- PERF-OBS-1 (baseline latency metrics)

## Files in Scope

- `rust/crates/daemon-runtime/src/handlers/callers.rs`
- `rust/crates/daemon-runtime/src/handlers/callees.rs`
- `rust/crates/daemon-runtime/src/handlers/path.rs`
- `rust/crates/daemon-runtime/src/live_graph.rs` — add traversal methods
- `rust/crates/rgr/tests/callers_command.rs` — verify unchanged
- `rust/crates/rgr/tests/callees_command.rs` — verify unchanged
- `rust/crates/rgr/tests/path_command.rs` — verify unchanged

## Migration Pattern

```rust
// In handler:
async fn handle_callers(state: &DaemonState, req: CallersRequest) -> Result<CallersResponse> {
    // Try LiveGraph first
    if let Some(graph) = state.live_graph_for_repo(&req.repo_uid) {
        return callers_from_live_graph(graph, &req);
    }
    
    // Fallback to SQLite
    callers_from_storage(&state.storage, &req)
}
```

## Performance Expectations

In-memory graph traversal is expected to reduce latency materially compared to SQLite lookups for multi-hop queries. Exact improvement depends on:
- Graph density
- Query depth
- SQLite cache state
- Index efficiency

Actual latency reduction to be measured during implementation. Do not assume specific multipliers until measured on reference repos.

## Estimated Effort

Small-Medium — handlers already exist, just swapping data source.
