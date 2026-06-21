# BACKLOG-REMEDIATION-1: Pathological Backlog Recovery

## Status

**WITHDRAWN** (2026-05-29) — superseded by EXTRACTION-SUBSTRATE-ADR-1
(`docs/architecture/adr/adr-extraction-substrate-scip-first.md`).

This slice optimized prune of rebuildable SQLite raw-graph backlog
(`unresolved_edges`, etc.). Under the SCIP-first pivot, raw graph leaves SQLite
entirely and those facts become disposable derived cache. The remediation for the
current bloated DBs is operator reset (delete affected DBs, reindex), not a prune
optimization. Retained for historical record. Original status: ACTIVE (2026-05-29).

## Problem Statement

The repo-graph database has accumulated 20 prunable snapshots with ~1.17M rows in
`unresolved_edges`. The current `rmap maintenance prune` implementation times out
at 900s because:

1. No progress emission during prune
2. Client read timeout fires before prune completes
3. Manual SQL is documented as workaround, but that is not a product path

This blocks:
- PERF-OBS-1B (timing instrumentation requires clean steady-state baseline)
- Confidence in maintenance path for future backlogs

## Solution

Add progress emission during prune so that:
1. Each snapshot deletion emits a progress event
2. Progress events reset the client read timeout
3. Prune can run indefinitely while providing user feedback
4. No timeout increase required

## Implementation

### Phase 1: Progress emission in daemon

The daemon's `handle_classify_retention` already has access to the request ID
for response construction. The prune operation needs to emit progress after
each snapshot.

**Option A: Callback injection**

Pass a progress callback to `prune_prunable_snapshots()`:

```rust
// In daemon handler
let progress_fn = |current: usize, total: usize| {
    emitter.emit(ProgressDetail {
        phase: "pruning".to_string(),
        current: current as i64,
        total: Some(total as i64),
    })
};

storage.prune_prunable_snapshots_with_progress(repo_uid, progress_fn)?;
```

**Option B: Iterator pattern**

Have prune return an iterator that yields after each snapshot:

```rust
for result in storage.prune_snapshots_iter(repo_uid) {
    let (current, total) = result?;
    emitter.emit(...)?;
}
```

**Recommendation:** Option A (callback) is simpler and matches existing patterns.

### Phase 2: Storage layer change

Add `prune_prunable_snapshots_with_progress()` to `StorageConnection`:

```rust
pub fn prune_prunable_snapshots_with_progress<F>(
    &self,
    repo_uid: &str,
    progress_fn: F,
) -> Result<i64, StorageError>
where
    F: Fn(usize, usize) -> Result<(), String>,
{
    // Collect prunable snapshot UIDs
    let snapshot_uids = self.get_prunable_snapshot_uids(repo_uid)?;
    let total = snapshot_uids.len();
    
    for (i, snapshot_uid) in snapshot_uids.iter().enumerate() {
        // Delete this snapshot
        self.delete_snapshot(snapshot_uid)?;
        
        // Emit progress
        progress_fn(i + 1, total)?;
    }
    
    Ok(total as i64)
}
```

### Phase 3: Client already handles progress

The client's `request()` method already handles progress events:

```rust
// In connection.rs read loop
if response.progress.is_some() {
    continue;  // Skip progress, wait for final response
}
```

Progress events reset the read timeout naturally because each `read_line()` call
completes when a progress line arrives.

## Files to Change

1. `rust/crates/storage/src/retention/prune.rs`
   - Add `prune_prunable_snapshots_with_progress()`
   - Keep existing `prune_prunable_snapshots()` as non-progress wrapper

2. `rust/crates/daemon-runtime/src/handlers/inventory/retention.rs`
   - Update `handle_classify_retention` to use progress-enabled prune
   - Wire emitter through to storage call

3. `rust/crates/daemon-transport/src/dispatch.rs`
   - Verify `ProgressEmitter` is accessible in handler context
   - May need to pass emitter to retention handler

## Definition of Done

1. [ ] Progress emission implemented in daemon prune path
2. [ ] repo-graph backlog cleared via `rmap maintenance prune`
3. [ ] Post-remediation retention stats verified (prunable_count = 0)
4. [ ] No manual SQL required
5. [ ] Slice documentation complete

## Validation Plan

```bash
# Before: verify backlog exists
rmap maintenance prune --json 2>&1 | head -5
# Should timeout or show large prunable_count

# After fix: prune should complete with progress
rmap maintenance prune
# Expected: "pruned 20 snapshot(s)" with progress feedback

# Verify clean state
rmap maintenance prune --json
# Expected: pruned_count: 0, retention.total matches protected classes
```

## Technical Debt Closure

This slice closes the technical debt documented in MAINTENANCE-CLI-1:
- "Progress emission during prune (MAINTENANCE-PROGRESS-1)"

After this slice:
- MAINTENANCE-CLI-1 becomes operationally complete
- REFRESH-HANG-1 operational closure achieved
- PERF-OBS-1B can proceed with clean baseline

## Related

- `docs/slices/maintenance-cli-1.md` - CLI surface (implementation complete)
- `docs/slices/refresh-hang-1.md` - Root cause and mitigation
- `docs/slices/retention-policy-1.md` - Retention lifecycle contract
