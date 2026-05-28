# MAINTENANCE-CLI-1: Maintenance CLI Command

## Status

**IMPLEMENTATION COMPLETE** (2026-05-28)

**Operationally incomplete:** Cannot clear pathological repo-graph backlog (20 snapshots,
1.17M rows) within 900s timeout. Normal operation (1-2 snapshots) works. One-time operator
cleanup required for existing backlog.

## Problem Statement

REFRESH-HANG-1 split retention lifecycle into:
- Foreground: `classify_retention_only()` (~2ms, on every index/refresh)
- Maintenance: `enforce_retention_lifecycle()` (slow, includes prune)

The maintenance path was implemented in the daemon but had no CLI surface,
leaving users with no way to clean up prunable snapshot backlogs.

## Solution

Added `rmap maintenance prune` CLI command that:
1. Connects to daemon
2. Calls `classify_retention` method (includes classify + prune)
3. Reports results (pruned_count, retention stats)
4. Supports `--json` output for programmatic use

## Usage

```bash
# Prune prunable snapshots for current repo
rmap maintenance prune

# JSON output
rmap maintenance prune --json
```

## Technical Debt

### Extended Timeout Workaround

The prune operation can exceed the default 300s daemon timeout on repos with
large backlogs. This was addressed by:

1. Adding `request_with_timeout()` method to `Transport` trait
2. Using 900s timeout for the maintenance prune operation

This is a workaround. The proper fix is:

- Have the daemon emit progress events during prune
- Progress events would reset the client read timeout
- Users would see incremental feedback ("pruned 3/18 snapshots...")

### Why Progress Emission is Needed

The prune operates per-snapshot:
```rust
for snapshot_uid in &snapshot_uids {
    // Delete dependent rows
    // Delete snapshot
    // Commit transaction
}
```

Each snapshot deletion could emit a progress event, keeping the connection
alive and providing feedback. This requires passing a `ProgressEmitter`
through the storage layer.

### Files Changed for Timeout Workaround

- `rust/crates/rgr/src/daemon_client/transport.rs`
  - Added `request_with_timeout()` to `Transport` trait

- `rust/crates/rgr/src/daemon_client/socket_transport.rs`
  - Implemented `request_with_timeout()` with temporary timeout change

- `rust/crates/rgr/src/daemon_client/stdio_transport.rs`
  - Implemented `request_with_timeout()` (no-op, stdio has no timeout)

- `rust/crates/rgr/src/daemon_client/mod.rs`
  - Exposed `request_with_timeout()` on `DaemonClient`

- `rust/crates/rgr/src/commands/maintenance.rs`
  - Uses `PRUNE_TIMEOUT_SECS = 900` for prune operations

## Files Changed

### New Files

- `rust/crates/rgr/src/commands/maintenance.rs` - Command handler
- `rust/crates/rgr/tests/maintenance_command.rs` - Tests
- `docs/slices/maintenance-cli-1.md` - This document

### Modified Files

- `rust/crates/rgr/src/commands/mod.rs` - Added `mod maintenance` and export
- `rust/crates/rgr/src/main.rs` - Added `maintenance` command dispatch

## Test Coverage

### Unit Tests (no daemon needed)

- `maintenance_no_subcommand_shows_usage`
- `maintenance_help_shows_usage`
- `maintenance_h_shows_usage`
- `maintenance_unknown_subcommand_is_error`
- `maintenance_prune_help_shows_usage`
- `maintenance_prune_unknown_flag_is_error`

### Daemon-unavailable Tests

- `maintenance_prune_fails_when_daemon_unavailable`
- `maintenance_prune_json_fails_when_daemon_unavailable`

### Integration Tests (require running daemon, marked #[ignore])

- `maintenance_prune_noop_when_nothing_prunable`
- `maintenance_prune_actual_prune_when_backlog_exists`
- `maintenance_prune_protected_snapshots_preserved`
- `maintenance_prune_json_output_format`
- `maintenance_prune_human_output_format`
- `maintenance_prune_duration_reported`

## Definition of Done

### Implementation (COMPLETE)

1. [x] `rmap maintenance prune` command implemented
2. [x] Human-readable output format
3. [x] JSON output format (`--json`)
4. [x] Help text (`--help`, `-h`)
5. [x] Timeout extended for long-running prune (900s)
6. [x] Unit tests for CLI parsing
7. [x] Daemon-unavailable tests
8. [x] Integration test stubs
9. [x] Slice documentation
10. [x] REFRESH-HANG-1 updated

### Operational Validation (INCOMPLETE)

11. [ ] **BLOCKED:** Backlog cleanup on repo-graph (requires operator intervention)
12. [ ] **BLOCKED:** Validate prune on clean state

## Follow-On Work

### MAINTENANCE-PROGRESS-1 (Recommended)

Add progress emission during prune:
- Pass `ProgressEmitter` to storage layer
- Emit after each snapshot deletion
- Remove extended timeout workaround
- Provide user feedback during long prunes

### Backlog Cleanup (Emergency Operator Workaround)

The repo-graph database has accumulated 20 prunable snapshots with ~1.17M rows
in `unresolved_edges`. Even with 900s timeout, CLI prune exceeds this.

**This is an emergency operator workaround, not the product path.** After cleanup,
`rmap maintenance prune` will function normally for typical operations.

**One-time manual cleanup procedure:**

```bash
# Stop daemon to avoid conflicts
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/bijuterie.repo-graph.daemon.plist

# Direct SQL cleanup (time: ~5-10 minutes)
sqlite3 ~/Library/Application\ Support/repo-graph/databases/401063a802312008.db "
  -- Delete unresolved_edges for prunable snapshots
  DELETE FROM unresolved_edges WHERE snapshot_uid IN (
    SELECT snapshot_uid FROM snapshots
    WHERE repo_uid = 'repo_01ks2tmxsne9y13en1sfsq9ed1'
      AND retention_class = 'prunable'
  );
  
  -- Delete from other orphan tables
  DELETE FROM boundary_provider_facts WHERE snapshot_uid IN (
    SELECT snapshot_uid FROM snapshots
    WHERE repo_uid = 'repo_01ks2tmxsne9y13en1sfsq9ed1'
      AND retention_class = 'prunable'
  );
  -- (repeat for boundary_consumer_facts, boundary_links, etc.)
  
  -- Delete the snapshots
  DELETE FROM snapshots
  WHERE repo_uid = 'repo_01ks2tmxsne9y13en1sfsq9ed1'
    AND retention_class = 'prunable';
"

# Restart daemon
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/bijuterie.repo-graph.daemon.plist

# Validate: prune should now be a no-op
rmap maintenance prune
```

**Post-cleanup behavior:** Normal prune operations (1-2 snapshots, ~60K rows) complete
in <30 seconds. The 900s timeout is sufficient for typical cases.

### PERF-PRUNE-1 (Future)

For repos that accumulate large backlogs:
- Consider drop/recreate indices during bulk prune
- Or use DELETE with LIMIT batching with commits
- Or implement background prune with progress reporting

## Related

- `docs/slices/refresh-hang-1.md` - Root cause and split
- `docs/slices/retention-policy-1.md` - Retention contract
