# CURRENT_SLICE.md

## Current Priority

**CLI-OUT-4: Module/Architecture Output** — IN PROGRESS (Groups 1-4 complete)

Slice doc: `docs/slices/cli-out-4-modules.md`

Human renderers for 11 read-side architecture commands:
- modules list, show, files, unowned, deps, violations
- surfaces list, show
- boundaries list, show, summary

Excludes `modules boundary` (legacy write command).

### Group Status

| Group | Commands | Status |
|-------|----------|--------|
| 1 | modules list, show | COMPLETE (2026-05-20) |
| 2 | modules files, unowned | COMPLETE (2026-05-20) |
| 3 | modules deps, violations | COMPLETE (2026-05-20) |
| 4 | surfaces list, show | COMPLETE (2026-05-20, empty-case corpus, populated-case fixture) |
| 5 | boundaries list, show, summary | QUEUED |

### Group 1 Deliverables

Module catalog/detail (list, show):
- `presentation/module_shared.rs` — shared formatting helpers (109 lines)
- `presentation/modules_list.rs` — list DTO + renderer (264 lines)
- `presentation/modules_show.rs` — show DTO + renderer (454 lines)
- Unit tests: 26 (8 + 7 + 11)
- Daemon dispatch tests: 7
- CLI integration tests: 7 (opt-in)
- Corpus validated: OpenXcom, django, duckdb

### Group 2 Deliverables

Ownership inventory (files, unowned):
- `presentation/module_inventory.rs` — DTOs + renderers (422 lines)
- `commands/modules/files.rs` — updated with --json + human mode (154 lines)
- `commands/modules/unowned.rs` — updated with --json + human mode (138 lines)
- Unit tests: 14 in module_inventory.rs
- Daemon dispatch tests: 5 (3 files + 2 unowned, pre-existing)
- CLI integration tests: 5 (opt-in)

### Group 3 Deliverables

Dependency/violation analysis (deps, violations):
- `presentation/modules_deps.rs` — deps DTO + renderer (263 lines)
- `presentation/modules_violations.rs` — violations DTO + renderer (319 lines)
- `commands/modules/deps.rs` — updated with --json + human mode (182 lines)
- `commands/modules/violations.rs` — updated with --json + human mode (373 lines)
- Unit tests: 15 (7 deps + 8 violations)
- Daemon dispatch tests: 8 (5 deps + 3 violations)
- CLI integration tests: 4 (opt-in)

Note: Split into separate files because `modules deps` (relationship reporting)
and `modules violations` (policy breach surface) have different change axes.

### Group 4 Deliverables

Architectural surfaces (surfaces list, show):
- `presentation/surfaces.rs` — DTOs + renderers (594 lines total)
- `commands/surfaces.rs` — updated with --json + human mode (342 lines)
- Unit tests: 14 (7 list + 7 show)
- CLI integration tests: 4 tests
- Review packet: `docs/audits/cli-out-4/group-4-surfaces-review.md`
- Handles degradation warning when surfaces not populated
- Deterministic ordering: (kind, name, uid) for list, (evidence_kind, path) for show

**500-line guardrail note:** `presentation/surfaces.rs` exceeds 500 lines (594).
Kept as single file because list/show share surface identity domain, same actor,
same degradation model. Split not required unless change axes diverge.

**Corpus validation note:** All indexed repos (OpenXcom, django, duckdb) have 0
surfaces (C++/Python codebases). Empty-case and error-path behavior validated.
Populated-case covered by unit tests with synthetic data only.

### Next: Group 5

Architectural boundaries (boundaries list, show, summary):
- Evaluate `commands/boundaries.rs` refactor (472 lines, approaching guardrail)
- `presentation/boundaries.rs` — DTOs + renderers
- `boundaries list` human renderer + `--json` flag
- `boundaries show` human renderer + `--json` flag
- `boundaries summary` human renderer + `--json` flag

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
| 3 | CLI-OUT-4 | modules (6), surfaces (2), boundaries (3) | IN PROGRESS |
| 4 | CLI-OUT-5 | docs *, resource *, policy | QUEUED |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | QUEUED |
| 6 | CLI-OUT-7 | violations, gate, assess | QUEUED |

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)
