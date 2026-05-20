# CURRENT_SLICE.md

## Current Priority

**CLI-OUT-5: Inventory/Policy Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-5-inventory.md`

Human renderers for 6 inventory and policy commands:
- docs list, extract
- resource list, readers, writers
- policy

### Group Status

| Group | Commands | Status |
|-------|----------|--------|
| 1 | docs list, extract | COMPLETE (2026-05-20) |
| 2 | resource list, readers, writers | COMPLETE (2026-05-20) |
| 3 | policy | COMPLETE (2026-05-20) |

### Implementation Order

1. **Documentation inventory** — `docs list`, `docs extract` — smallest coherent family
2. **Resource inventory** — `resource list`, `resource readers`, `resource writers` — same vocabulary
3. **Policy introspection** — `policy` — different semantic class

### Structural Assessment

Command file sizes (all under 500-line guardrail):
- `commands/docs.rs` — 179 lines
- `commands/resource.rs` — 249 lines
- `commands/policy.rs` — 264 lines

No refactoring required before adding renderer logic.

### Presentation Module Plan

- `presentation/docs.rs` — list + extract renderers (single file, two functions)
- `presentation/resources.rs` — list + readers/writers renderers
- `presentation/policy.rs` — STATUS_MAPPING + BEHAVIORAL_MARKER + RETURN_FATE renderers

### Contract Note

`policy` command does NOT use REG-1 contract. It requires explicit `db_path` and
`repo_uid` arguments. This is preserved; no migration to daemon planned.

---

## Recently Implemented

**CLI-OUT-4: Module/Architecture Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-4-modules.md`

Delivered:
- Human renderers for 11 read-side architecture commands
- modules list, show, files, unowned, deps, violations
- surfaces list, show
- boundaries list, show, summary
- Groups 1-3: corpus-validated (OpenXcom, django, duckdb)
- Groups 4-5: empty-case corpus-validated, populated-case fixture-validated

Review packets:
- `docs/audits/cli-out-4/group-4-surfaces-review.md`
- `docs/audits/cli-out-4/group-5-boundaries-review.md`

**CLI-OUT-3: Graph Drilldown Output** — IMPLEMENTED (2026-05-19)

Slice doc: `docs/slices/cli-out-3-drilldown.md`
Review packet: `docs/audits/cli-out-3/review-packet.md`

Delivered:
- Human renderer for `callers` and `callees` (shared `graph_edges.rs` module)
- Human renderer for `path` with query-term-preserving header
- Human renderer for `imports` with depth and resolution
- `--json` flag for machine mode on all commands
- Validated on 3-repo corpus (OpenXcom, django, duckdb)

**CLI-OUT-2C: Stats Renderer** — IMPLEMENTED (2026-05-19)

Slice doc: `docs/slices/cli-out-2c-stats-renderer.md`

Delivered:
- Human renderer for `stats` with full sorted sections
- No arbitrary top-N clipping or threshold-based labeling
- `--json` flag for machine mode
- Validated on 5-repo corpus

---

## Recently Fixed

**RMAPD-PERF-1: Stats Query Pathology** — STATS FIXED (2026-05-19)

Slice doc: `docs/slices/rmapd-perf-1-timeout.md`

Stats root cause (OBSERVED): `compute_module_stats` had correlated subqueries
with O(modules * edges * symbols) complexity.

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
| 3 | CLI-OUT-4 | modules (6), surfaces (2), boundaries (3) | COMPLETE |
| 4 | CLI-OUT-5 | docs (2), resource (3), policy (1) | COMPLETE |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | QUEUED |
| 6 | CLI-OUT-7 | violations, gate, assess | QUEUED |

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)
