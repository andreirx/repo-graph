# Hot-Path Analysis

## Overview

This document maps the execution surfaces for product-critical commands.
Generated 2026-05-28 using `rmap callees` introspection.

---

## Index Command (`rmap index`)

### Entry Point

```
ServiceDispatcher.handle_index
File: rust/crates/daemon-runtime/src/dispatch.rs:1254
```

### Call Graph

```
handle_index
├── get_string_param                          # param extraction
├── get_or_create_db_runtime_for_new_db       # DB setup
├── index_path_with_progress                  # MAIN WORK
│   ├── open_or_create_storage                # open SQLite
│   └── index_into_storage_with_progress      # orchestration
│       ├── prepare_repo_inputs               # SCANNING PHASE (~6s)
│       │   ├── scan_repo                     # filesystem walk
│       │   ├── detect_language               # per-file classification
│       │   ├── extract_cargo_modules         # Cargo.toml parsing
│       │   ├── extract_npm_modules           # package.json parsing
│       │   ├── extract_pyproject_modules     # pyproject.toml parsing
│       │   ├── extract_gradle_modules        # settings.gradle parsing
│       │   ├── detect_inferred_modules_*     # module boundary inference
│       │   └── resolve_*                     # config resolution
│       │
│       ├── [extractor initialization]        # tree-sitter setup (~0.1ms)
│       │   ├── TsExtractor.initialize
│       │   ├── CExtractor.initialize
│       │   ├── CppExtractor.initialize
│       │   ├── JavaExtractor.initialize
│       │   ├── PythonExtractor.initialize
│       │   └── RustExtractor.initialize
│       │
│       ├── ensure_repo                       # repo row upsert
│       │
│       ├── index_repo                        # EXTRACTION PHASE (~35s)
│       │   ├── run_pipeline                  # per-file extraction
│       │   ├── build_extension_routing_table # language routing
│       │   ├── dispatch_recompute_relationships # edge computation
│       │   ├── index_proto_files             # protobuf indexing
│       │   ├── build_toolchain_json          # toolchain metadata
│       │   └── run_java_mapping              # Java-specific mapping
│       │
│       └── [postpass persistence]            # PERSISTENCE PHASE (~2s)
│           ├── persist_read_failures
│           ├── persist_config_file_versions
│           ├── persist_metrics
│           ├── persist_spring_liveness_inferences
│           ├── persist_policy_facts
│           ├── persist_boundary_interactions
│           ├── persist_ts_boundary_interactions
│           ├── persist_cargo_modules
│           ├── persist_npm_modules
│           ├── persist_express_surfaces
│           ├── persist_react_inferences
│           ├── persist_pyproject_modules
│           ├── persist_gradle_modules
│           └── persist_inferred_modules
│
├── load_repo                                 # load into daemon state
│
└── classify_retention_only                   # RETENTION PHASE (~2ms)
    ├── classify_repo_retention               # assign retention classes
    └── get_retention_stats                   # return stats
```

### Timing Profile (repo-graph, ~1100 files)

| Phase | Time | Notes |
|-------|------|-------|
| Scanning | ~6s | Filesystem + config parsing |
| Extractors init | ~0.1ms | Tree-sitter setup |
| Extraction | ~35s | Per-file AST parsing + edge computation |
| Persistence | ~2s | SQLite batch inserts |
| Auto-load | ~2ms | Load into daemon cache |
| Retention classify | ~2ms | Fast, no prune |
| **Total** | **~53s** | |

### Heavy Loops

1. `prepare_repo_inputs` iterates all files for classification
2. `index_repo.run_pipeline` calls extractors per-file (1100 files)
3. Persistence does batch INSERT per table

### Lock Boundaries

- `_db_write_guard` held for entire index operation
- Released only after index completes

---

## Refresh Command (`rmap refresh`)

### Entry Point

```
ServiceDispatcher.handle_refresh
File: rust/crates/daemon-runtime/src/dispatch.rs:1480
```

### Call Graph

```
handle_refresh
├── resolve_alias_or_path
├── get_or_create_db_runtime
├── acquire_refresh                           # lock acquisition
├── refresh_path_with_progress                # MAIN WORK
│   ├── open_storage
│   └── refresh_into_storage_with_progress
│       ├── prepare_repo_inputs               # SCANNING PHASE
│       ├── [extractor initialization]
│       ├── compute_delta                     # compare with parent
│       ├── copy_forward_phase                # COPY-FORWARD (can be slow)
│       │   ├── copy_nodes
│       │   ├── copy_edges
│       │   ├── copy_measurements
│       │   ├── copy_inferences
│       │   └── copy_boundaries
│       ├── extract_changed_files             # only changed files
│       └── [postpass persistence]
│
└── classify_retention_only                   # RETENTION PHASE (~2ms)
```

### Performance Notes

- Copy-forward can be slow on large repos with many unchanged files
- RMAPD-PERF-2 addressed batching issues
- REFRESH-HANG-1 removed prune from hot path

---

## Orient Command (`rmap orient`)

### Entry Point

```
ServiceDispatcher.handle_orient
File: rust/crates/daemon-runtime/src/dispatch.rs:2069
```

### Call Graph

```
handle_orient
├── resolve_and_load_repo_with_display_name
├── acquire_read                              # read lock only
├── run_orient_query                          # MAIN QUERY
│   └── [storage queries]
└── TrustOverlaySummary.has_degradation       # trust overlay
```

### Notes

- Read-only, no write locks
- Performance depends on query complexity
- Trust overlay adds overhead

---

## Check Command (`rmap check`)

### Entry Point

```
ServiceDispatcher.handle_check
```

### Call Graph

```
handle_check
├── resolve_and_load_repo
├── acquire_read
├── get_snapshot_uid
├── query_nodes_for_check                     # node query
├── query_edges_for_check                     # edge query
└── [dead code detection]
```

---

## Trust Command (`rmap trust`)

### Entry Point

```
ServiceDispatcher.handle_trust
```

### Notes

- Compute-intensive trust propagation
- Multiple storage queries
- Caches results in trust overlay

---

## Callers/Callees Commands

### Entry Point

```
ServiceDispatcher.handle_callers  (dispatch.rs:693)
ServiceDispatcher.handle_callees  (dispatch.rs:...)
```

### Call Graph

```
handle_callers
├── resolve_symbol                            # symbol lookup
├── parse_ambiguous_matches                   # handle ambiguity
├── find_direct_callers                       # MAIN QUERY
│   └── [SQL JOIN on edges table]
└── acquire_read
```

### Notes

- Main work is SQL query
- Performance depends on edge density

---

## Path Command (`rmap path`)

### Entry Point

```
ServiceDispatcher.handle_path
```

### Notes

- BFS/DFS through edge graph
- Can be slow on deep call chains
- Uses bidirectional search optimization

---

## Cycles Command (`rmap cycles`)

### Entry Point

```
ServiceDispatcher.handle_cycles
```

### Notes

- Tarjan's SCC algorithm
- Full graph traversal
- Can be slow on large graphs

---

## Fan-Out Points

### High Fan-Out (called from many places)

1. `StorageConnection.connection()` - all DB access
2. `emit_progress()` - 15+ calls in index path
3. `ErrorDetail.invalid_request()` - all error paths

### Known Bottlenecks

1. **scan_repo** - filesystem walk, can be slow on large repos
2. **run_pipeline** - per-file extraction, dominates index time
3. **copy_forward** - refresh delta computation
4. **prune_prunable_snapshots** - removed from hot path (REFRESH-HANG-1)

---

## Recommendations

### Index Optimization Targets

1. **Parallelization** - per-file extraction could be parallel
2. **Incremental** - use file hashes to skip unchanged
3. **Streaming** - don't load all files into memory

### Refresh Optimization Targets

1. **Delta detection** - already optimized via parent comparison
2. **Copy-forward batching** - already addressed (RMAPD-PERF-2)
3. **Lazy extraction** - only extract what's queried

### Query Optimization Targets

1. **Index coverage** - ensure hot queries use indices
2. **Prepared statements** - cache SQL plans
3. **Result streaming** - avoid loading all results at once

---

## Files Reference

| Component | File | LOC |
|-----------|------|-----|
| dispatch.rs | daemon-runtime/src/dispatch.rs | ~2500 |
| compose.rs | repo-index/src/compose.rs | ~3600 |
| orchestrator.rs | indexer/src/orchestrator.rs | ~1300 |
| queries.rs | storage/src/queries.rs | ~1000 |
| retention.rs | daemon-runtime/.../retention.rs | ~300 |
