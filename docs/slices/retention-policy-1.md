# RETENTION-POLICY-1: Automatic Pruning on Refresh

## Status

COMPLETE (2026-05-27)

## Validation

Field-validated on `/tmp/test-retention`:
- Index + 3 refreshes → 4 snapshots created
- Auto-pruning reduced to 2 snapshots (current + baseline_auto)
- Daemon log shows: `retention: pruned 1 snapshot(s) for repo ...` (twice)

Pre-existing repo issue resolved by migration 029 (see Known Issues).

## Problem Statement

CACHE-SEMANTICS-1 established the semantic model:
- `derived_cache_epoch` for cache validity
- retention classes (current, parent, baseline_auto, baseline_user, prunable)
- stale-epoch exclusion from protected roles
- classification logic

But classification without operational consequence leaves storage pressure untouched. The `prune_prunable_snapshots()` method exists but is never called automatically. Prunable snapshots accumulate indefinitely.

This slice wires policy into behavior: classification triggers pruning.

## Scope

### In Scope

- Auto-prune prunable snapshots after index/refresh
- Observability: pruned count in daemon response, rmap doctor, rmap perf
- Tests proving protection invariants hold under pruning
- Daemon log output for pruning operations

### Out of Scope

- CLI commands for manual pruning (daemon methods sufficient)
- Configurable retention depth (fixed at current + parent + baseline)
- Physical Tier A / Tier B file separation
- State root separation (STATE-ROOT-SEPARATION-1)

## Definition of Done

1. [x] Auto-classify after index/refresh stays in place
2. [x] Auto-prune prunable snapshots after classification
3. [x] Never prune protected classes:
   - `current`
   - `parent`
   - `baseline_auto`
   - `baseline_user`
4. [x] Stale-epoch snapshots become prunable and are eligible for cleanup
5. [x] Pruning observable in:
   - [x] `rmap doctor` — shows "(N prunable)" in storage probe
   - [x] `rmap perf` — retention stats include prunable count
   - [x] Daemon command response — includes `retention.pruned_count`
   - [x] Daemon logs — logs "retention: pruned N snapshot(s) for repo X"
6. [x] Tests prove:
   - [x] No protected snapshot can be deleted by prune operation (19 retention tests)
   - [x] Stale-epoch snapshots are reclaimed (`classify_then_prune_reclaims_stale_epochs`)
   - [x] Marking/unmarking baselines changes retention safely (CACHE-SEMANTICS-1)

## Implementation Plan

### Phase 1: Wire pruning into handlers

In `handle_index` and `handle_refresh`:
1. After `classify_repo_retention()` succeeds
2. Call `prune_prunable_snapshots()` for the repo
3. Capture pruned count
4. Include in response JSON

### Phase 1.5: Transactional safety and code organization

**Transaction boundaries:**
- `classify_repo_retention()` — single transaction (all class assignments atomic)
- `prune_prunable_snapshots()` — single transaction (orphan cleanup + delete atomic)
- `enforce_retention_lifecycle()` — sequenced, not single-transaction atomic

If classification commits and prune fails, the repo has correct retention classes
but pruning is deferred. Next lifecycle run will complete pruning. This is a
safe intermediate state, not corruption.

**Code organization:**
- `retention.rs` (878 lines) split into module directory:
  - `retention/types.rs`: RetentionClass, RetentionStats, CURRENT_CACHE_EPOCH
  - `retention/classify.rs`: classification logic (transactional)
  - `retention/prune.rs`: prune logic (transactional)
  - `retention/tests/`: split by concern (types, classify, epoch, baseline, prune, lifecycle)

### Phase 2: Observability

- `rmap doctor`: Already shows "(N prunable)" — after pruning this should typically be 0
- `rmap perf`: Already shows retention stats — verify prunable count updates
- Daemon response: Add `pruned_count` field to index/refresh results
- Logs: Add info-level log for "pruned N snapshots for repo X"

### Phase 3: Safety tests

Storage-level tests:
- `prune_cannot_delete_current_snapshot`
- `prune_cannot_delete_parent_snapshot`
- `prune_cannot_delete_baseline_auto_snapshot`
- `prune_cannot_delete_baseline_user_snapshot`
- `prune_deletes_stale_epoch_snapshots`

Integration test (optional):
- Full refresh cycle → verify only current + parent + baselines remain

## Existing Infrastructure

From CACHE-SEMANTICS-1:
- `prune_prunable_snapshots(repo_uid)` — deletes all prunable snapshots for repo
- `classify_repo_retention(repo_uid)` — assigns retention classes
- `mark_stale_epochs_prunable(repo_uid)` — marks stale epochs as prunable
- `get_retention_stats()` — returns class counts
- 15 retention tests already verify classification correctness

## Risk Assessment

**Low risk.** The protection semantics are already enforced:
- `classify_repo_retention()` only marks excess snapshots as prunable
- Protected classes are assigned before prunable
- `prune_prunable_snapshots()` only deletes WHERE retention_class = 'prunable'

The only new behavior is calling an existing safe method automatically.

## Known Issues

1. ~~**Pre-existing repos may fail pruning**~~ **RESOLVED by migration 029.**
   Migration 029 (`029-repair-orphan-fks`) automatically cleans up orphan FK
   references in tables without `ON DELETE CASCADE`. This runs automatically
   when the daemon starts, so existing repos are repaired transparently.
   
2. **Large repo refresh hangs**: Refresh on repos like `leveldb` (1.7GB DB) hangs
   indefinitely. This is a separate performance regression, not related to retention
   policy. Tracked separately — not a blocker for this slice.

## Dependencies

- CACHE-SEMANTICS-1 (COMPLETE)

## Files in Scope

- `rust/crates/daemon-runtime/src/handlers/inventory/index.rs` — add prune call
- `rust/crates/daemon-runtime/src/handlers/inventory/refresh.rs` — add prune call
- `rust/crates/storage/src/retention.rs` — add safety tests
- `rust/crates/rgr/src/commands/doctor.rs` — verify observability
- `rust/crates/rgr/src/commands/perf.rs` — verify observability

## Estimated Effort

Small — wiring existing methods, adding observability, adding tests.
