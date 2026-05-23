# CURRENT_SLICE.md

## Current Priority

No active slice. See ROADMAP.md for queued work.

---

## Recently Completed

**TS-IMPORT-RESOLUTION-1:** TypeScript aliased and namespace import resolution — COMPLETE (2026-05-23)

See `docs/slices/ts-import-resolution-1.md`.

Phase completion:
1. **Phase 1** — Aliased named import resolver fix (uses `imported_name`)
2. **Phase 2** — `ImportKind` enum (`Named`, `Default`, `Namespace`)
3. **Phase 3** — Namespace import member resolution + conservative default handling
   - Namespace: `ns.member` → lookup `member` in target module
   - Default: `obj.member` → NOT resolved (prevents false positives)

Self-index resolution: 18.8% → 20.0% (+1.2pp). Modest gain — repo-graph uses few
aliased/namespace imports for internal calls.

**LEGACY-CONTRACT-MIGRATION-1:** Full slice — COMPLETE (2026-05-23)

All 7 legacy commands migrated to REG-1 daemon contract.

Sub-slice completion:
1. **1A** — Shared CLI support — COMPLETE (2026-05-22)
2. **1B** — Quality family (churn, hotspots, risk, coverage) — COMPLETE (2026-05-22)
3. **1C** — Governance family (assess, violations) — COMPLETE (2026-05-22)
4. **1D** — Inventory family (policy) — COMPLETE (2026-05-23)

**LEGACY-CONTRACT-MIGRATION-1D:** Inventory family — COMPLETE (2026-05-23)

Delivered:
- `handlers/inventory/policy.rs` — policy fact queries (216 lines)
- CLI command migrated to daemon_command pattern
- 5 inventory handler unit tests
- 14 policy CLI tests (arg parsing)

Corpus validation (EXECUTED on leveldb + sqlite):
- RETURN_FATE: 524 facts, JSON envelope correct
- BEHAVIORAL_MARKER: 1 fact (retry loop), evidence structure correct
- STATUS_MAPPING: 0 facts, hint displayed
- --file, --callee, --fate filters: all verified
- Empty results: exit 0, hint message
- Repo-not-found: exit 2, actionable hint

**LEGACY-CONTRACT-MIGRATION-1C:** Governance family — COMPLETE (2026-05-22)

Delivered:
- `handlers/governance/assess.rs` — quality policy assessment (write operation)
- `handlers/governance/violations.rs` — unified violations (declared + discovered)
- `handlers/support.rs` — shared handler utilities
- CLI commands migrated to daemon_command pattern
- 5 governance handler unit tests

Write-path validation: assess persists assessments via QualityPolicyRunner.
JSON parity: structured envelope with counts.
Human parity: summary output with policy evaluation results.

**LEGACY-CONTRACT-MIGRATION-1B:** Quality family — COMPLETE (2026-05-22)

Delivered:
- `handlers/quality/` (churn.rs, hotspots.rs, risk.rs, coverage.rs, support.rs)
- All handler files under 500-line guardrail
- Coverage matcher in classification crate with pure DTOs
- 15 quality handler unit tests

**LEGACY-CONTRACT-MIGRATION-1A:** Shared CLI support — COMPLETE (2026-05-22)

Delivered `rust/crates/rgr/src/daemon_command.rs`:
- `resolve_repo_from_cwd()` — cwd → canonical path
- `DaemonError` — classified error type (Unavailable, RepoNotFound, Timeout, RuntimeError)
- `execute_daemon_request()` — core request wrapper
- `execute_repo_request()` — convenience wrapper with repo resolution
- `print_daemon_error()` — error printing with actionable hints
- `output_json()` / `output_result()` — JSON vs human-render branch
- `run_daemon_command()` — full command execution helper
- Exit code constants (EXIT_SUCCESS=0, EXIT_USAGE_ERROR=1, EXIT_RUNTIME_ERROR=2)

15 unit tests covering repo resolution, error classification, display formatting.
427 rgr lib tests pass. Existing REG-1 commands unaffected.

---

## Queued

Candidates (see ROADMAP.md):
- **CURSOR-1:** Cursor MCP/rules integration
- **PERF-OBS-1:** Performance observability baseline (gate to Storage Architecture Track)

---

## Recently Implemented

**ORIENT-BUG-1: Module Count Mismatch** — COMPLETE (2026-05-21)

Read-model fix: orient and trust counts now align via shared `module_candidates` source.
Refresh performance regression (RMAPD-PERF-2) fixed: batched copy-forward queries.
Bug fixed: off-by-one in `:FILE` key extraction (-5 → -4).
Transport timeout bug (EAGAIN) separated to RMAP-IO-1.

See `docs/slices/orient-bug-1-module-count.md`.

**RMAP-IO-1: Client Transport Timeout Classification** — COMPLETE (2026-05-21)

macOS socket timeout (EAGAIN / os error 35) now classified as `Timeout` instead of `ReadFailed`.
Error message: "daemon response timed out after 300s" instead of cryptic os error.

See `docs/slices/rmap-io-1.md`.

**SHOW-DETAIL-AUDIT-1: Unexercised Detail Command Audit** — COMPLETE (2026-05-21)

Final closure pass for CLI output track. Exercised remaining detail commands:
- `surfaces show` — GOOD
- `boundaries show` — GOOD (fixed DTO mismatch)
- `resource readers` — GOOD
- `resource writers` — GOOD

Fixed: boundaries show DTO had `boundary` field but daemon sends `detail`, plus camelCase rename annotations.

Slice doc: `docs/slices/show-detail-audit-1.md`

---

## Recently Implemented

**MODULE-BOUNDARY-FIX-1: Module/Boundary Command DTO Fixes** — COMPLETE (2026-05-21)

Fixed 3 DTO mismatches discovered in CLI-AUDIT-1:
1. `boundaries summary` - Rewrote DTO to match daemon's category-specific field names and string-array filesWithBoundaries
2. `boundaries list` - Added serde rename annotations for all camelCase daemon fields
3. `modules deps` - Added serde rename for source/target and diagnostics fields

Slice doc: `docs/slices/module-boundary-fix-1.md`

---

## Recently Implemented

**CLI-AUDIT-1: Cross-Repo Full Surface Audit** — COMPLETE (2026-05-20, reconciled 2026-05-21)

Systematic human-output review across 35+ commands × 14 repos.

Summary: `docs/audits/cli-audit-1/summary.md`

Reconciled results:
- Phase 1: 70/70 cells (66 GOOD, 4 EMPTY_HONEST)
- Phase 2: ~56 cells sampled, format GOOD
- Phase 3: ~56 cells (24 GOOD, 10 EMPTY_HONEST, 4 CAPTURED, ~18 NOT RUN)
- Phase 4-5: ~39 cells (9 GOOD, 2 EMPTY_HONEST, 16 LEGACY, 12 NOT RUN)

Defects found and resolved:
- boundaries summary (DTO mismatch) — FIXED in MODULE-BOUNDARY-FIX-1
- boundaries list (missing details) — FIXED in MODULE-BOUNDARY-FIX-1
- boundaries show (DTO mismatch) — FIXED in SHOW-DETAIL-AUDIT-1
- modules deps (missing module names) — FIXED in MODULE-BOUNDARY-FIX-1
- 7 legacy commands need daemon migration (deferred)

**SMOKE-1: Validation Harness Cleanup** — COMPLETE (2026-05-20)

Fixed bash empty array handling in smoke-rmap.sh. Validated 4 commands (gate, orient,
trust, stats) across 12 repos with zero failures. Harness operational.

**CLI-OUT-7: Governance Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-7-governance.md`

Delivered:
- Human renderers for 3 governance commands
- assess (Group 1): fixture-validated
- violations (Group 2): fixture-validated
- gate (Group 3): corpus-validated via live daemon (django, repo-graph)
- Domain verdicts preserved exactly: PASS, FAIL, WAIVED, MISSING_EVIDENCE, UNSUPPORTED

**CLI-OUT-6: Quality/Risk Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-6-quality.md`

Delivered:
- Human renderers for 4 quality/risk commands
- churn, hotspots (Group 1): time-window semantics, ranking surfaces
- risk (Group 2): join metadata, no invented verdict labels
- coverage (Group 3): backend-bounded diagnostic samples, write-command contract
- All groups corpus-validated where data exists

**CLI-OUT-5: Inventory/Policy Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-5-inventory.md`

Delivered:
- Human renderers for 6 inventory and policy commands
- docs list, extract
- resource list, readers, writers
- policy (STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)
- Groups 1-3: corpus-validated where data exists, fixture-validated otherwise

**CLI-OUT-4: Module/Architecture Output** — COMPLETE (2026-05-20)

Slice doc: `docs/slices/cli-out-4-modules.md`

Delivered:
- Human renderers for 11 read-side architecture commands
- modules list, show, files, unowned, deps, violations
- surfaces list, show
- boundaries list, show, summary
- Groups 1-3: corpus-validated (OpenXcom, django, duckdb)
- Groups 4-5: empty-case corpus-validated, populated-case fixture-validated

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

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)
