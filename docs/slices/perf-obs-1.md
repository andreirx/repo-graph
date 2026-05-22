# PERF-OBS-1: Storage Performance Observability

## Status

QUEUED — specification complete, not yet implemented.

## Problem Statement

Before migrating to the three-tier storage architecture (STORAGE-ARCH-1), we need baseline metrics to:
1. Identify actual bottlenecks vs assumed ones
2. Measure improvement after changes
3. Prioritize which commands to migrate first

Currently, performance understanding is anecdotal (e.g., "refresh takes 6 minutes on large repos").

## Scope

### In Scope

- Table size metrics (row counts, disk bytes)
- Command latency instrumentation (wall time, DB time)
- Memory footprint by repo (daemon RSS)
- Refresh/index timing breakdown
- Copy-forward volume (rows touched)
- Per-layer row counts

### Out of Scope

- Automated alerting/dashboards
- Historical trend storage
- External monitoring integration
- Query plan analysis (defer to PERF-OBS-2 if needed)

## Definition of Done

1. `rmap stats` includes table row counts grouped by tier
2. `rmap doctor` reports DB file size and snapshot count
3. Daemon logs command execution time at INFO level
4. `--timing` flag on commands reports breakdown (parse, query, format)
5. Documentation of baseline metrics for 3 reference repos (small, medium, large)

## Validation Plan

1. Run `rmap stats` on repo-graph, amodx, django
2. Verify row counts match expectations per tier
3. Run 5 commands with `--timing` and confirm breakdown appears
4. Compare before/after metrics for a known-slow operation

## Dependencies

- STORAGE-ARCH-1 (defines tier classification)

## Files in Scope

- `rust/crates/rgr/src/commands/doctor.rs` — add size metrics
- `rust/crates/rgr/src/commands/stats.rs` — add tier breakdown
- `rust/crates/storage/src/queries.rs` — add metrics queries
- `rust/crates/daemon-runtime/src/handlers/` — add timing logs

## Estimated Effort

Small — instrumentation only, no architectural changes.
