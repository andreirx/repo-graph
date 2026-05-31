# CURRENT_SLICE.md

## Current Priority

**EXTRACTION-SUBSTRATE-PIVOT** — SCIP-first L0/L1 substrate.

Committed direction: `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
(EXTRACTION-SUBSTRATE-ADR-1).

Substrate viability gate **RETIRED**: TypeScript GO, C/C++ GO, Rust GO-with-caveats.
Evidence in `docs/audits/scip-{ts-parity,clang,rust}-spike-1/`.

SCIP-INGEST-IR-1 is DESIGN READY (D1-D5 resolved). Current priority: **INGEST-CORE-1**
(first code slice; spec `docs/slices/ingest-core-1.md`, implementation pending sign-off).
IR design: the repo-graph-centered ingestion IR
over SCIP. See `docs/slices/scip-ingest-ir-1.md`. First-class requirements: canonical
stable-key mapping over SCIP IDs, call-graph derivation from occurrences + syntax
context, AST<->SCIP correlation contract, Rust per-crate mode + duplicate-symbol
dedup, C/C++ build-root/context provenance.

The remaining spike measures (precise CALLS parity, multi-config C, all-crates Rust,
M3, M4b) are validation tracks for the IR slice, not blockers. Warm-cache format and
refresh model remain deferred until after the IR.

Execution spine (risk-driven): `docs/architecture/scip-migration-plan.md`. Stages:
A thin foundation (SCIP-INGEST-IR-1 design → INGEST-CORE-1) → B retire the four
strategic-trigger risks on that foundation (CJOIN-PROVE-1 C/C++ join, XPART-PROVE-1
cross-partition, REFRESH-PROBE-1 refresh-at-scale, RUST-INGEST-PROVE-1) → C runtime
(LiveGraph/query/value-join/trust) → D persistence + raw decommission. Each slice
carries a go/no-go and a documented retreat that narrows scope, never kills the plan.

Refresh model, partition granularity, and warm-cache format remain deferred to
migration-plan Stages B–D. They were never gated on the viability spikes (TS/C/Rust),
which are complete and retired the gate. Do not treat them as settled.

---

## Superseded / Deprioritized

**BACKLOG-REMEDIATION-1** — WITHDRAWN. Pathological prune of rebuildable SQLite
raw-graph backlog is no longer a product concern. Per STORAGE-ARCH-1 Tier B
semantics and the SCIP-first ADR, those local DBs are disposable derived cache;
the remediation is operator reset (delete affected DBs, reindex), not heroic
prune. See `docs/slices/backlog-remediation-1.md`.

Its abandoned progress-emitter working-tree edits (`ProgressEmitter` /
`prune_prunable_snapshots_with_progress` / `OperationAborted` across `dispatch.rs`,
`retention.rs`, `error.rs`, `prune.rs`) were **reverted from the working tree
(2026-05-30)**; they were uncommitted and left daemon-runtime/storage
non-compiling. **The daemon (`rmapd`) and dev-install are existing shipped
capability** (committed HEAD, RMAPD/MAC/LINUX) and were unaffected — only the dirty
working tree was.

**PERF-OBS-1B** — DEPRIORITIZED. Timing the outgoing SQLite raw-graph substrate is
low value now. Meaningful timing comes from the SCIP spike (M5) on the incoming
substrate.

---

## In Progress

**REFRESH-HANG-1:** Refresh command hang — MITIGATION COMPLETE (2026-05-28)

See `docs/slices/refresh-hang-1.md`.

Completed:
- [x] Hot-path unblock (index completes in ~38-53s)
- [x] Classification on foreground (~2ms)
- [x] Maintenance CLI command (MAINTENANCE-CLI-1)
- [x] RETENTION-POLICY-1 contract amended

Incomplete:
- [~] Backlog cleanup — MOOT under SCIP-first pivot (operator reset, not prune; see Superseded section)

---

## Recently Completed

**MAINTENANCE-CLI-1:** Maintenance CLI command — IMPLEMENTATION COMPLETE (2026-05-28)

See `docs/slices/maintenance-cli-1.md`.

Implemented:
- `rmap maintenance prune` command
- Human and JSON output formats
- Extended timeout (900s)
- Tests for CLI parsing and daemon-unavailable cases

**Operationally incomplete:** Cannot clear pathological backlog within timeout.
Blocked by BACKLOG-REMEDIATION-1.

**HOT-PATH-ANALYSIS-1:** Hot-path mapping artifact — COMPLETE (2026-05-28)

See `docs/hot-path-analysis.md`.

**STATE-ROOT-SEPARATION-1:** Authority vs Sandbox-Local State Boundaries — COMPLETE (2026-05-28)

**RETENTION-POLICY-1:** Retention lifecycle — AMENDED (2026-05-28)

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
