# CACHE-SEMANTICS-1: Extracted Facts as Rebuildable Cache

## Status

QUEUED — specification complete, not yet implemented.

## Problem Statement

The codebase treats extracted facts (nodes, edges, inferences) with the same semantic weight as user-authored declarations. This conflation:

1. Makes pruning feel risky ("what if we delete something important?")
2. Blocks cache invalidation strategies
3. Conflates "extraction bug" with "data loss"

The fix is semantic, not mechanical: reframe Tier B data as explicitly rebuildable cache.

## Scope

### In Scope

- Document Tier B tables as rebuildable cache in code comments
- Define cache invalidation strategy (mechanism TBD during slice — options include per-table version columns, snapshot-level cache epoch, or extractor version in snapshot metadata)
- Define invalidation triggers (extractor upgrade, schema change)
- Implement retention policy: current + parent + baseline
- Add `prune_old_snapshots` storage method (but don't auto-call yet)

### Out of Scope

- Automatic pruning on refresh (defer to later slice)
- Physical DB separation (Tier A vs Tier B files)
- Backing store changes (still SQLite)
- Live graph implementation (that's LIVE-GRAPH-1)

## Definition of Done

1. Doc comments on all Tier B tables state "rebuildable cache"
2. Cache invalidation mechanism chosen and documented (snapshot-level epoch vs per-table versioning vs other)
3. `snapshots` table has `retention_class` column (current/parent/baseline/prunable)
4. Storage crate exposes `mark_snapshot_retention()` and `prune_prunable_snapshots()`
5. Daemon marks retention classes during refresh
6. `rmap doctor` reports prunable snapshot count

## Validation Plan

1. Index a repo, refresh twice
2. Verify 3 snapshots exist with distinct retention classes
3. Call `prune_prunable_snapshots()` directly in test
4. Verify only current + parent remain
5. Reindex and confirm no semantic loss

## Dependencies

- STORAGE-ARCH-1 (defines Tier B table list)
- PERF-OBS-1 (baseline metrics before pruning)

## Files in Scope

- `rust/crates/storage/src/migrations/` — add retention_class column (+ versioning mechanism if per-table)
- `rust/crates/storage/src/crud/snapshots.rs` — retention marking
- `rust/crates/storage/src/queries.rs` — pruning query
- All Tier B table definitions — doc comments
- `rust/crates/daemon-runtime/src/refresh.rs` — mark retention on refresh

## Design Decisions to Make During Slice

1. **Versioning granularity**: Snapshot-level cache epoch (simpler, one version per snapshot) vs per-table extractor version columns (finer invalidation, more schema churn)?
2. **Invalidation trigger**: On extractor version mismatch, invalidate whole snapshot or specific tables?
3. **Baseline selection**: User-explicit baseline marking or implicit "most recent ready before current"?

## Estimated Effort

Medium — schema change + semantic reframing, but no query path changes.
