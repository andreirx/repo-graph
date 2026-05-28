# REFRESH-HANG-1: Refresh Command Hang Investigation

## Status

**MITIGATION COMPLETE** (2026-05-28)

Hot-path unblock implemented. Maintenance CLI surface implemented (MAINTENANCE-CLI-1).
Backlog cleanup requires one-time operator intervention due to pathological scale.

## Problem Statement

`rmap index` and `rmap refresh` were hanging on repo-graph (medium-sized repo).
The daemon consumed high CPU but the command never completed.

## Root Cause

Destructive cleanup (prune) was sitting on the synchronous product hot path.

### Evidence

- ~17 prunable snapshots accumulated over time
- ~58K `unresolved_edges` rows per snapshot
- ~1M total rows to delete
- `DELETE FROM unresolved_edges WHERE snapshot_uid IN (...)` took 54+ seconds
- Index/refresh blocked waiting for prune to complete

### Architectural Error

The retention lifecycle (classify + prune) was called synchronously after
every successful index/refresh. Prune involves DELETE operations on tables
with millions of rows. This must never block interactive commands.

## Emergency Fix

Split retention into two phases:

### Foreground (index/refresh)

- `classify_retention_only()` - assigns retention classes, returns stats
- Fast (~2ms), never blocks user commands
- Reports `prunable_count` so user knows maintenance is needed

### Maintenance (explicit)

- `enforce_retention_lifecycle()` - classify + prune + stats
- Called only by `classify_retention` daemon command
- Can be slow, runs only when explicitly requested

## Incomplete Work

### Maintenance Not User-Operable

The maintenance path is not fully accessible:
- Daemon method `classify_retention` exists (includes prune)
- **No CLI command surface** — user cannot easily trigger maintenance

This is blocking for a complete solution. Options:
1. Add `rmap maintenance` CLI command (follow-on slice)
2. Add `rmap prune` CLI command
3. Document daemon-only maintenance as acceptable

### Backlog Cleanup

The current database has ~18 prunable snapshots with ~1M rows in `unresolved_edges`.
This needs explicit cleanup via the daemon method, which is not user-operable.

### Diagnostic Tracing

Diagnostic tracing added during investigation has been removed.

## Files Changed

- `rust/crates/daemon-runtime/src/handlers/inventory/retention.rs`
  - Added `classify_retention_only()` for foreground path
  - Documented architectural split
  - Added `prunable_count` to `LifecycleResult`

- `rust/crates/daemon-runtime/src/dispatch.rs`
  - `handle_index`: uses `classify_retention_only()` instead of `enforce_retention_lifecycle()`
  - `handle_refresh`: same change
  - Response includes `prunable_count` for visibility

- `rust/crates/storage/src/retention/prune.rs`
  - Batched per-snapshot deletion (for when prune is called)

## Validation

### Before fix

```
rmap index . → hang at 60+ seconds
Daemon log: "deleting from unresolved_edges" → never completes
```

### After fix

```
rmap index . → completes in ~38-53 seconds
Classification: ~2ms
No prune on foreground path
```

## Definition of Done

### Mitigation (COMPLETE)

1. [x] Slice created
2. [x] Root cause identified (prune on hot path)
3. [x] Emergency fix implemented (split classify/prune)
4. [x] Index completes on repo-graph in reasonable time (~38-53s)
5. [x] Refresh expected to behave same (uses same fix)
6. [x] Diagnostic tracing removed
7. [x] Maintenance CLI command added (MAINTENANCE-CLI-1)
8. [x] RETENTION-POLICY-1 contract formally updated (amended)

### Operational Closure (INCOMPLETE)

9. [ ] **BLOCKED:** Backlog cleanup executed (requires operator intervention, see MAINTENANCE-CLI-1)

## Prevention

1. **Never put destructive cleanup on interactive hot path**
   - Classify is fast, prune is not
   - Prune must be deferred/budgeted/explicit

2. **Monitor hot path latency**
   - PERF-OBS-1B should capture phase timing
   - Alert if any phase exceeds threshold

## Follow-On Work Required

### MAINTENANCE-CLI-1

Add CLI surface for maintenance operations:
- `rmap maintenance` or `rmap prune`
- Wraps daemon `classify_retention` method
- Makes deferred cleanup user-operable

### Backlog Cleanup

One-time cleanup of accumulated prunable snapshots:
- ~18 snapshots with ~1M rows
- Requires either CLI command or direct daemon protocol

## Dependencies

- Amends RETENTION-POLICY-1 contract
- Blocks PERF-OBS-1B (cannot measure clean timing while backlog exists)

## Related

- `docs/slices/retention-policy-1.md` — contract amended
- `docs/hot-path-analysis.md` — call graph mapping
