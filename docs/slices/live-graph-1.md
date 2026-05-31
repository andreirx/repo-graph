# LIVE-GRAPH-1: In-Memory Current Snapshot Graph

## Status

REVISED by `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
(EXTRACTION-SUBSTRATE-ADR-1). The in-memory `LiveGraph` struct and parity intent
below stand. Two assumptions change: (1) the loader source is SCIP-derived facts
mapped to canonical stable keys, not SQLite `nodes`/`edges`; (2) residency is
per-partition, not one-per-registered-repo. No longer far-future — now on the
active substrate track. Not yet implemented.

## Problem Statement

Graph traversal commands (callers, callees, path, cycles) query SQLite for every edge lookup. For large repos, this means:

- Repeated index scans on `edges` table
- Row materialization overhead per traversal step
- No memoization across related queries in same session

An in-memory graph representation for the current snapshot would eliminate I/O for hot-path traversals.

## Scope

### In Scope

- Define `LiveGraph` struct: node adjacency, edge lookup, symbol index
- Implement loader from SQLite (current snapshot only)
- Add `LiveGraph` to daemon state (one per registered repo)
- Validate parity: compare results from LiveGraph vs SQLite for test queries
- No command migration yet (that's LIVE-GRAPH-2)

### Out of Scope

- Multi-snapshot graphs (only current snapshot)
- Historical comparison queries
- Policy/declaration queries (still SQLite)
- Graph mutation (read-only projection)
- Persistence of LiveGraph (rebuilt on startup)

## Definition of Done

1. `LiveGraph` struct exists in `rust/crates/daemon-runtime/src/live_graph.rs`
2. Loader populates from `nodes` + `edges` tables for a snapshot
3. Supports: `outgoing_edges(node_uid)`, `incoming_edges(node_uid)`, `node_by_key(stable_key)`
4. Daemon loads LiveGraph after successful index/refresh
5. Parity test: 100 random path queries match SQLite results
6. Memory overhead documented for 3 reference repos

## Validation Plan

1. Index repo-graph, load LiveGraph
2. Run 100 random callers/callees queries via both paths
3. Assert identical results
4. Measure memory delta (daemon RSS with/without LiveGraph)
5. Measure load time from SQLite

## Dependencies

- STORAGE-ARCH-1 (defines Tier C)
- CACHE-SEMANTICS-1 (optional but recommended — clarifies cache vs live)

## Files in Scope

- `rust/crates/daemon-runtime/src/live_graph.rs` — new module
- `rust/crates/daemon-runtime/src/state.rs` — add LiveGraph to DaemonState
- `rust/crates/daemon-runtime/src/handlers/index.rs` — trigger load after index
- `rust/crates/daemon-runtime/src/handlers/refresh.rs` — trigger reload after refresh
- `rust/crates/daemon-runtime/tests/` — parity tests

## Data Structures (Sketch)

```rust
pub struct LiveGraph {
    /// node_uid → NodeEntry
    nodes: HashMap<String, NodeEntry>,
    /// stable_key → node_uid
    key_index: HashMap<String, String>,
    /// source_node_uid → Vec<EdgeEntry>
    outgoing: HashMap<String, Vec<EdgeEntry>>,
    /// target_node_uid → Vec<EdgeEntry>
    incoming: HashMap<String, Vec<EdgeEntry>>,
    /// Snapshot this graph represents
    snapshot_uid: String,
}

pub struct NodeEntry {
    pub node_uid: String,
    pub stable_key: String,
    pub kind: String,
    pub name: String,
    pub file_uid: Option<String>,
}

pub struct EdgeEntry {
    pub edge_uid: String,
    pub source_node_uid: String,
    pub target_node_uid: String,
    pub edge_type: String,
}
```

## Estimated Effort

Medium — new subsystem, but read-only projection with clear boundaries.
