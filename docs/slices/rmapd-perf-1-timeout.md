# RMAPD-PERF-1: Large Repo Timeout Investigation

**Status:** QUEUED  
**Type:** Bug / Performance  
**Priority:** After CLI-OUT-2B (does not block renderer work)  
**Discovered:** CLI-OUT-2A audit (2026-05-18)

## Problem Statement

Large repositories fail with daemon timeouts during indexing or query operations.

### Indexing Timeout

| Repo | Files | Result |
|------|-------|--------|
| gstreamer | ~6328 C | Timeout during `index` |
| hadoop | ~12478 Java | Timeout during `index` |

Error: `error: failed to read response: Resource temporarily unavailable (os error 35)`

### Query Timeout

| Repo | Files | Command | Result |
|------|-------|---------|--------|
| DuckDB | 5109 | stats | Timeout after 30s |
| DuckDB | 5109 | check | Timeout after 31s |
| Django | 3019 | stats | Timeout after 30s |
| Django | 3019 | check | Timeout after 31s |

Smaller repos (OpenXcom 733, Buildroot 645, grpc-java 1909) complete all commands.

## Symptoms

- Error occurs after ~30 seconds
- "Resource temporarily unavailable (os error 35)" suggests socket read timeout
- Daemon process continues running (not crashed)
- The operation may still be in progress server-side

## Root Cause Hypothesis

The client-side socket read has a 30-second timeout. For large repos:
- Indexing C code is slow (header expansion, macro processing)
- stats/check queries may be O(n) or worse on module/edge count

## Investigation Required

1. Identify where the 30-second timeout is set
2. Determine if indexing is actually slow or just appears slow
3. Profile stats/check queries on large repos
4. Consider: streaming responses, progress feedback, configurable timeout

## Why This Is Not a Renderer Issue

The renderer never receives a response. The timeout occurs at the transport layer.

CLI-OUT-2B should not attempt to fix this. The renderer cannot render
a response that never arrives.

## Options

1. **Increase timeout** — Simple but masks underlying perf issues
2. **Streaming response** — Complex but enables progress feedback
3. **Query optimization** — Address actual bottlenecks
4. **Async indexing** — Index in background, notify on completion

## Definition of Done

- [ ] Root cause identified (timeout location, actual operation time)
- [ ] Decision on fix approach
- [ ] Implementation
- [ ] gstreamer and hadoop can be indexed
- [ ] DuckDB and Django stats/check complete

## Files Likely in Scope

- `rust/crates/rgr/src/` (client socket handling)
- `rust/crates/daemon-runtime/src/` (server request handling)
- `rust/crates/repo-index/src/` (indexing performance)
- `rust/crates/module-queries/src/` (stats query performance)

## Files Out of Scope

- `rust/crates/rgr/src/presentation/` (renderer)
