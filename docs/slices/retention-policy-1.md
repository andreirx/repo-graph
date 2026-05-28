# RETENTION-POLICY-1: Retention Policy Lifecycle

## Status

**CONTRACT AMENDED** (2026-05-28)

Originally: Auto-prune prunable snapshots after index/refresh (COMPLETE 2026-05-27).

Amended by REFRESH-HANG-1: Foreground auto-prune replaced with classify-only.
Prune is now deferred to explicit maintenance.

See "Contract Amendment" section below.

## Contract Amendment (REFRESH-HANG-1)

### What Changed

The original contract was:
- **Foreground auto-prune** after every index/refresh

The current contract is:
- **Foreground classify-only** after index/refresh (~2ms)
- **Deferred prune** via explicit maintenance command

### Why

Prune can delete 1M+ rows from `unresolved_edges`. On repos with accumulated
stale snapshots, this took 60+ seconds and blocked interactive commands.

Destructive cleanup must not sit on the synchronous product hot path.

### Current State

| Operation | Foreground | Maintenance |
|-----------|------------|-------------|
| Classify retention | Yes | Yes |
| Prune prunable | No | Yes |
| Response includes prunable_count | Yes | N/A |

### Incomplete Work

The maintenance path is not fully user-operable:
- Daemon method `classify_retention` exists (includes prune)
- **No CLI command surface yet** — user cannot easily trigger maintenance

This is acknowledged technical debt. Follow-on slice required:
- MAINTENANCE-CLI-1: Explicit maintenance command for prune

## Original Validation (2026-05-27)

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

But classification without operational consequence leaves storage pressure untouched.

## Scope

### Current Behavior (After Amendment)

- **Classify retention** after index/refresh (foreground, fast)
- **Report prunable_count** in response so user knows maintenance is needed
- Prune available via daemon method (not CLI-exposed)

### Original In Scope (Partially Implemented)

- [x] Auto-classify after index/refresh
- [x] Observability: prunable count in daemon response, rmap doctor, rmap perf
- [x] Tests proving protection invariants hold under pruning
- [x] Daemon log output for pruning operations (when prune runs)
- [~] Auto-prune after index/refresh — **AMENDED: classify-only on foreground**

### Out of Scope

- CLI commands for manual pruning — **NOW REQUIRED as follow-on**
- Configurable retention depth (fixed at current + parent + baseline)
- Physical Tier A / Tier B file separation
- State root separation (STATE-ROOT-SEPARATION-1)

## Definition of Done

1. [x] Auto-classify after index/refresh stays in place
2. [~] Auto-prune prunable snapshots after classification — **AMENDED: deferred**
3. [x] Never prune protected classes:
   - `current`
   - `parent`
   - `baseline_auto`
   - `baseline_user`
4. [x] Stale-epoch snapshots become prunable and are eligible for cleanup
5. [x] Pruning observable in:
   - [x] `rmap doctor` — shows "(N prunable)" in storage probe
   - [x] `rmap perf` — retention stats include prunable count
   - [x] Daemon command response — includes `retention.pruned_count` and `prunable_count`
   - [x] Daemon logs — logs "retention: pruned N snapshot(s) for repo X" (when prune runs)
6. [x] Tests prove:
   - [x] No protected snapshot can be deleted by prune operation (19 retention tests)
   - [x] Stale-epoch snapshots are reclaimed (`classify_then_prune_reclaims_stale_epochs`)
   - [x] Marking/unmarking baselines changes retention safely (CACHE-SEMANTICS-1)

## Implementation

### Current Architecture

**Foreground path (index/refresh):**
```
index/refresh success
  → classify_retention_only()
    → classify_repo_retention()  [fast, ~1ms]
    → get_retention_stats()
    → return {pruned_count: 0, prunable_count: N, stats}
```

**Maintenance path (daemon method):**
```
classify_retention command
  → enforce_retention_lifecycle()
    → classify_repo_retention()
    → prune_prunable_snapshots()  [slow, can be 60s+]
    → get_retention_stats()
    → return {pruned_count: N, prunable_count: 0, stats}
```

### Transaction Boundaries

- `classify_repo_retention()` — single transaction (all class assignments atomic)
- `prune_prunable_snapshots()` — per-snapshot transactions (batched to avoid giant deletes)
- `enforce_retention_lifecycle()` — sequenced, not single-transaction atomic

If classification commits and prune fails, the repo has correct retention classes
but pruning is deferred. Next lifecycle run will complete pruning.

### Code Organization

- `retention/types.rs`: RetentionClass, RetentionStats, CURRENT_CACHE_EPOCH
- `retention/classify.rs`: classification logic (transactional)
- `retention/prune.rs`: prune logic (per-snapshot batched)
- `retention/tests/`: split by concern

## Known Issues

1. **Maintenance not CLI-accessible** — daemon method exists but no CLI surface.
   User must either:
   - Use daemon protocol directly (not practical)
   - Wait for MAINTENANCE-CLI-1 slice

2. **Backlog accumulation** — repos indexed before REFRESH-HANG-1 fix may have
   many prunable snapshots. One-time cleanup via daemon method needed.

## Dependencies

- CACHE-SEMANTICS-1 (COMPLETE)
- REFRESH-HANG-1 (emergency amendment)

## Follow-On Work

- **MAINTENANCE-CLI-1**: Add `rmap maintenance` or `rmap prune` CLI command
  to make deferred cleanup user-operable.

## Files in Scope

- `rust/crates/daemon-runtime/src/handlers/inventory/retention.rs`
- `rust/crates/daemon-runtime/src/dispatch.rs`
- `rust/crates/storage/src/retention/`
