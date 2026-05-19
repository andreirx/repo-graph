# RMAPD-PERF-1: Large Repo Timeout Investigation

**Status:** STATS QUERY FIXED, TIMEOUT CLASS MITIGATED  
**Type:** Bug / Performance  
**Priority:** After CLI-OUT-2B  
**Discovered:** CLI-OUT-2A audit (2026-05-18)  
**Mitigated:** 2026-05-18  
**Resolved:** 2026-05-19

## Problem Statement

Large repositories failed with daemon timeouts during `stats` queries.

## Root Cause (OBSERVED)

The `compute_module_stats` query in `storage/src/queries.rs` had **three correlated subqueries** in the SELECT clause that ran once per module.

Each subquery:
1. Scanned OWNS edges to find files owned by module
2. Joined to nodes to get file_uid
3. Scanned symbols to count those in matching files

**Complexity:** O(modules × edges × symbols)

For django (3019 files, ~78K symbols), this resulted in a **760-second query** (12.7 minutes).

### Evidence

Instrumentation added (`--features perf-trace`):

**Before fix:**
| Repo | Files | Query Time |
|------|-------|------------|
| OpenXcom | 733 | 10,343ms |
| django | 3,019 | 760,594ms (12.7 min) — **client timeout** |

**After fix:**
| Repo | Files | Query Time (1st run) | Query Time (2nd run) |
|------|-------|---------------------|----------------------|
| OpenXcom | 733 | 334ms | 101ms |
| buildroot | 645 | 131ms | 114ms |
| django | 3,019 | 2,981ms | 2,846ms |
| duckdb | 5,109 | 5,537ms | 4,612ms |
| grpc-java | 1,821 | 893ms | 574ms |

**Django improvement: 760,594ms → 2,981ms (255x speedup)**

## Fix Applied

Rewrote `compute_module_stats` to eliminate correlated subqueries using CTEs:

1. `module_files` CTE: compute module→file mapping once
2. `file_stats` CTE: aggregate symbol stats per file once (single pass)
3. `module_symbol_stats` CTE: roll up file stats to module level
4. Main query joins pre-computed aggregates

**New complexity:** O(edges + symbols + modules)

See `storage/src/queries.rs` line 1049 for the rewritten query with documentation.

## Mitigation Still in Place

The following mitigations from initial investigation remain:

### A. Increased Read Timeout (connection.rs)

`READ_TIMEOUT_SECS` increased from 30s to 300s. This provides headroom for genuinely long operations and is harmless to keep.

### B. Pre-Computation Heartbeat (dispatch.rs)

Progress emission before heavy computation in read handlers. Provides visibility during long operations.

## Orient Query Performance (OBSERVED)

Orient timings measured during investigation:

| Repo | Files | orient |
|------|-------|--------|
| OpenXcom | 733 | 1,779ms |
| buildroot | 645 | 369ms |
| django | 3,019 | 9,038ms |
| duckdb | 5,109 | 15,007ms |
| grpc-java | 1,821 | 10,248ms |

All complete under the original 30-second timeout.

**Trust query timings were NOT measured in this investigation.** Trust is assumed
acceptable based on prior operational testing, not instrumented proof.

## Definition of Done (Stats Query)

- [x] Instrumentation added (`--features perf-trace`)
- [x] Stats query root cause identified with evidence (OBSERVED)
- [x] Query rewritten to eliminate correlated subqueries
- [x] Stats performance validated on full corpus
- [x] Unit tests pass (3 `module_stats` tests)

## Not Done (Broader Timeout Class)

- [ ] Trust query timings measured
- [ ] Other heavy queries (cycles, Tarjan SCC) profiled
- [ ] Mid-query keepalive for future long operations
- [ ] Indexing phase instrumentation

## Files Changed

- `rust/crates/storage/src/queries.rs` — rewritten `compute_module_stats` query
- `rust/crates/daemon-runtime/src/dispatch.rs` — timing instrumentation
- `rust/crates/daemon-runtime/Cargo.toml` — `perf-trace` feature flag
- `scripts/dev-install-local.sh` — `CARGO_FEATURES` env var support

## Verification

```bash
# All repos complete stats in under 6 seconds
CARGO_FEATURES="repo-graph-daemon-runtime/perf-trace" ./scripts/dev-install-local.sh
cd /path/to/django && rmap stats  # ~3s
cd /path/to/duckdb && rmap stats  # ~5.5s
grep "^\[PERF\] stats:" ~/Library/Logs/repo-graph/daemon.log
```

## Status Classification

**STATS QUERY FIXED** — stats pathology identified and eliminated with proof.

**TIMEOUT CLASS MITIGATED** — current corpus operationally unblocked. The
300-second timeout and heartbeat emission remain as defensive measures.
Future heavy operations could still encounter timeout issues if they
exceed 300 seconds without emitting a line.

This is not a universal/permanent performance resolution. One real
pathological query was found and fixed. The whole class of future
long-running read operations is not mathematically "solved."
