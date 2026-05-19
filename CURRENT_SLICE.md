# CURRENT_SLICE.md

## Current Priority

**CLI-OUT-4: Module/Architecture Output** — IN PROGRESS

Slice doc: `docs/slices/cli-out-4-modules.md` (to be created)

Human renderers for modules list, modules files, modules deps, modules violations,
surfaces list, surfaces show, boundaries list, boundaries show, boundaries summary.

---

## Recently Implemented

**CLI-OUT-3: Graph Drilldown Output** — IMPLEMENTED (2026-05-19)

Slice doc: `docs/slices/cli-out-3-drilldown.md`
Review packet: `docs/audits/cli-out-3/review-packet.md`

Delivered:
- Human renderer for `callers` and `callees` (shared `graph_edges.rs` module)
- Human renderer for `path` with query-term-preserving header
- Human renderer for `imports` with depth and resolution
- `--json` flag for machine mode on all commands
- Structured `AmbiguousSymbol` error handling with daemon data payload
- CLI renders numbered match list with disambiguation hint
- Validated on 3-repo corpus (OpenXcom, django, duckdb)

**CLI-OUT-2C: Stats Renderer** — IMPLEMENTED (2026-05-19)

Slice doc: `docs/slices/cli-out-2c-stats-renderer.md`

Delivered:
- Human renderer for `stats` with full sorted sections
- No arbitrary top-N clipping or threshold-based labeling
- Sections: Summary, By size, By fan-in, By fan-out, By distance
- `--json` flag for machine mode
- Validated on 5-repo corpus

---

## Recently Fixed

**RMAPD-PERF-1: Stats Query Pathology** — STATS FIXED (2026-05-19)

Slice doc: `docs/slices/rmapd-perf-1-timeout.md`

Stats root cause (OBSERVED): `compute_module_stats` had correlated subqueries
with O(modules × edges × symbols) complexity.

Fix: Rewrote query with CTEs. Django stats improved from 760s to 3s (255x speedup).

Not proven: Trust, cycles, other query performance. Timeout class mitigated,
not universally solved.

---

## Recently Validated

**CLI-OUT-2B: First-Contact Discovery Output** — VALIDATED (2026-05-18)

Slice doc: `docs/slices/cli-out-2b-output-redesign.md`
Review packet: `docs/audits/cli-out-2b/review-packet.md`

Delivered:
- Human renderer for `orient` with repo name, cycle topology, evidence-bearing degradation
- Human renderer for `trust` with resolution rates, reliability breakdown
- Human renderer for `cycles` with topology
- Validated on 5-repo corpus (OpenXcom, buildroot, django, duckdb, grpc-java)

---

## Handoff Complete

**CLI-OUT-2A: Cross-Repo Output Audit** — HANDOFF COMPLETE

Audit sufficient to drive first implementation wave. Findings in `docs/audits/cli-out-2a/`.

---

## Bug Slices

**ORIENT-BUG-1: Module Count Mismatch** — QUEUED

Orient shows 2-17 modules, trust shows 19-240+. Data/query bug.
See `docs/slices/orient-bug-1-module-count.md`.

---

## Output Program Wave Model

| Wave | Slice | Commands | Status |
|------|-------|----------|--------|
| 1 | CLI-OUT-2B | orient, trust, cycles, check | VALIDATED |
| 1b | CLI-OUT-2C | stats | IMPLEMENTED |
| 2 | CLI-OUT-3 | callers, callees, path, imports | IMPLEMENTED |
| 3 | CLI-OUT-4 | modules *, surfaces *, boundaries * | IN PROGRESS |
| 4 | CLI-OUT-5 | docs *, resource *, policy | QUEUED |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | QUEUED |
| 6 | CLI-OUT-7 | violations, gate, assess | QUEUED |

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)
