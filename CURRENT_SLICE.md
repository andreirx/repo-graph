# CURRENT_SLICE.md

## Current Priority

**PERF-OBS-1B:** Timing instrumentation (phase breakdown, global vs sandbox).

Per ROADMAP.md Storage Architecture Track sequence. Blocked pending backlog remediation.

---

## Blocking Issue

**BACKLOG-REMEDIATION:** repo-graph database has 20 prunable snapshots with ~1.17M rows.

`rmap maintenance prune` exists but cannot complete within 900s timeout on this pathological backlog.

**Status:** Operator intervention required (one-time).

**Emergency workaround:** See `docs/slices/maintenance-cli-1.md` for manual SQL cleanup procedure.
This is an emergency operator workaround, not the product path. After cleanup, `rmap maintenance prune`
will function normally (typical prunes: 1-2 snapshots, <30s).

**Next action:** Execute manual cleanup, then validate `rmap maintenance prune` on clean state.

---

## In Progress

**REFRESH-HANG-1:** Refresh command hang — MITIGATION COMPLETE (2026-05-28)

See `docs/slices/refresh-hang-1.md`.

Root cause: Destructive prune on synchronous hot path.
Fix: Split retention into classify-only (foreground) + deferred prune (maintenance).

Completed:
- [x] Hot-path unblock (index completes in ~38-53s)
- [x] Classification on foreground (~2ms)
- [x] Maintenance CLI command (MAINTENANCE-CLI-1)
- [x] RETENTION-POLICY-1 contract amended

Incomplete:
- [ ] Backlog cleanup executed (requires operator intervention)

---

## Recently Completed

**MAINTENANCE-CLI-1:** Maintenance CLI command — IMPLEMENTATION COMPLETE (2026-05-28)

See `docs/slices/maintenance-cli-1.md`.

Implemented:
- `rmap maintenance prune` command
- Human and JSON output formats
- Extended timeout (900s) for large prunes
- Tests for CLI parsing and daemon-unavailable cases

**Operationally incomplete:** Cannot clear pathological repo-graph backlog within timeout.
Normal operation (1-2 snapshots) will work. Backlog requires one-time operator cleanup.

Technical debt:
- Progress emission during prune (MAINTENANCE-PROGRESS-1) would allow unlimited prune duration

**HOT-PATH-ANALYSIS-1:** Hot-path mapping artifact — COMPLETE (2026-05-28)

See `docs/hot-path-analysis.md`.

Mapped call graphs for: index, refresh, orient, check, trust, callers, path, cycles.

**STATE-ROOT-SEPARATION-1:** Authority vs Sandbox-Local State Boundaries — COMPLETE (2026-05-28)

See `docs/slices/state-root-separation-1.md`.

A1/A2/B tier model validated. A1 blocking tested, A2/B sandbox writes allowed.

**RETENTION-POLICY-1:** Retention lifecycle — AMENDED (2026-05-28)

See `docs/slices/retention-policy-1.md`.

Original: Auto-prune after index/refresh.
Amended: Foreground classify-only + deferred prune via `rmap maintenance prune`.

---

## Queued

Candidates (see ROADMAP.md):
- **PERF-OBS-1B:** Timing instrumentation (blocked by backlog cleanup)
- **CURSOR-1:** Cursor MCP/rules integration

---

## Output Program Wave Model

| Wave | Slice | Commands | Status |
|------|-------|----------|--------|
| 1 | CLI-OUT-2B | orient, trust, cycles, check | VALIDATED |
| 1b | CLI-OUT-2C | stats | IMPLEMENTED |
| 2 | CLI-OUT-3 | callers, callees, path, imports | IMPLEMENTED |
| 3 | CLI-OUT-4 | modules (6), surfaces (2), boundaries (3) | COMPLETE |
| 4 | CLI-OUT-5 | docs (2), resource (3), policy (1) | COMPLETE |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | COMPLETE |
| 6 | CLI-OUT-7 | violations, gate, assess | COMPLETE |
