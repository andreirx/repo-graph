# CACHE-SEMANTICS-1: Extracted Facts as Rebuildable Cache

## Status

COMPLETE (2026-05-27) — all Definition of Done items verified.

**Gate passed:** PERF-OBS-1A volume baseline confirms authority tiny (44 rows), cache dominates (1.6M rows).

## Implementation Summary

- Migration 028 adds `derived_cache_epoch` and `retention_class` columns to snapshots
- Storage crate exposes retention management via `rust/crates/storage/src/retention.rs`
- **Whole-snapshot invalidation enforced**: `classify_repo_retention()` excludes stale-epoch snapshots from protected roles (current/parent/baseline_auto). Stale snapshots are always marked prunable unless they are user baselines.
- Valid epoch defined as: `derived_cache_epoch == CURRENT_CACHE_EPOCH` OR `NULL` (legacy)
- Daemon auto-classifies retention after index/refresh operations
- `rmap perf` shows retention stats (current, parent, baseline_auto, baseline_user, prunable, unclassified, stale_epoch)
- `rmap doctor` reports prunable count
- Daemon methods (no CLI surface yet):
  - `classify_retention` — manual re-classification
  - `mark_baseline` — mark a snapshot as user baseline
  - `unmark_baseline` — remove user baseline marking
- **15 storage tests** verify retention classification, pruning, stale epoch exclusion, user baseline preservation, and mark_baseline invariant maintenance

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

1. [x] Doc comments on all Tier B tables state "rebuildable cache"
   - See `rust/crates/storage/src/metrics.rs` — classify_table() documents Tier A as "MUST preserve", Tier B as "rebuildable cache"
2. [x] Cache invalidation mechanism chosen and documented
   - Decision: snapshot-level epoch (simpler, whole-snapshot invalidation)
   - `derived_cache_epoch` column stores version string (e.g., "1.0")
   - Current epoch defined in `retention.rs::CURRENT_CACHE_EPOCH`
3. [x] `snapshots` table has `retention_class` column
   - Added by migration 028
   - Values: current, parent, baseline_auto, baseline_user, prunable
4. [x] Storage crate exposes retention methods
   - `mark_snapshot_retention()` — set retention class for a snapshot
   - `classify_repo_retention()` — auto-classify all snapshots for a repo
   - `prune_prunable_snapshots()` — delete prunable snapshots
   - `get_retention_stats()` — get retention class counts
   - `mark_stale_epochs_prunable()` — mark stale-epoch snapshots as prunable
5. [x] Daemon marks retention classes during index/refresh
   - Auto-classifies in `handle_index` and `handle_refresh` dispatch handlers
   - Manual re-classification via `classify_retention` daemon method
6. [x] `rmap doctor` reports prunable snapshot count
   - Shows "(N prunable)" in storage probe when count > 0

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

## Design Decisions Made

1. **Versioning granularity**: Snapshot-level cache epoch
   - Decision: Single `derived_cache_epoch` column on snapshots table
   - Rationale: Simpler implementation, aligns with whole-snapshot invalidation model
   - Trade-off: Cannot invalidate individual tables, but extraction is fast enough that full re-extraction is acceptable

2. **Invalidation trigger**: Whole-snapshot invalidation
   - Decision: On epoch mismatch, entire snapshot is considered stale
   - Rationale: No half-valid snapshot states, simpler reasoning
   - Implementation: `classify_repo_retention()` excludes stale-epoch snapshots from candidate pools for current/parent/baseline_auto; only valid-epoch snapshots are considered for protected roles
   - Verified by tests `stale_epoch_snapshots_cannot_become_protected` and `stale_epoch_parent_skipped_for_valid_epoch_grandparent`

3. **Baseline selection**: Hybrid approach
   - Decision: Automatic baseline (`baseline_auto`) + user-explicit (`baseline_user`)
   - `baseline_auto`: Most recent valid-epoch ready snapshot that is neither current nor parent
   - `baseline_user`: Explicit user marking (preserved across auto-classification)
   - User marking via `mark_baseline` / `unmark_baseline` **daemon methods only**
   - No CLI surface for baseline marking yet (deferred to future slice)
   - Rationale: Sensible defaults with user override capability at daemon level

## Deferred Items (Not Closure Blockers)

- **CLI surface for baseline marking**: Daemon methods exist; CLI commands deferred
- **Automatic pruning on refresh**: RETENTION-POLICY-1
- **Physical Tier A / Tier B separation**: Future architectural evolution
- **Timing instrumentation**: PERF-OBS-1B

## Estimated Effort

Medium — schema change + semantic reframing, but no query path changes.
