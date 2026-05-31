# CURRENT_SLICE.md

## Current Priority

**EXTRACTION-SUBSTRATE-PIVOT** — SCIP-first L0/L1 substrate.

Committed direction: `docs/architecture/adr/adr-extraction-substrate-scip-first.md`
(EXTRACTION-SUBSTRATE-ADR-1).

Substrate viability gate **RETIRED**: TypeScript GO, C/C++ GO, Rust GO-with-caveats.
Evidence in `docs/audits/scip-{ts-parity,clang,rust}-spike-1/`.

SCIP-INGEST-IR-1 is DESIGN READY (D1-D5 resolved). **INGEST-CORE-1 is IMPLEMENTED**
(2026-05-31): `repo-graph-ir` + `repo-graph-scip-ingest` — value-level canonical
identity, strict edge derivation (Calls/References/FileScopeReference), materialized
FILE nodes + bubble-up caller resolution (no dangling endpoints), narrow
constructor/getter name reconciliation. A 10-group off-target acceptance harness is
green, plus an ignored engine regression. Closure evidence: `docs/slices/ingest-core-1.md`.

**Current priority: Stage B probes.** CJOIN-PROVE-1 + **CJOIN-PROVE-2 EXECUTED → ST1
RETIRED.** Clean-C++ value attachment + macro-heavy nginx **95.9% name-confirmed**, under the
**name-correspondence guard**: a C/C++ value fact attaches to SCIP identity only when range
containment AND name correspondence agree; **range-only joining is forbidden** (silently
misattaches 15.1% on C++ annotation-macro code). CJOIN-PROVE-1's 92.3% amended to **77.1%**
name-guarded strong attach. Specs `docs/slices/cjoin-prove-{1,2}.md`; evidence
`docs/audits/cjoin-prove-{1,2}/`; probe `rust/tools/cjoin-probe`.

**XPART-PROVE-1 (cross-partition traversal / ST3) — SPLIT; 1A+1B EXECUTED. ST3 NARROWED, NOT retired.**
EXECUTED on two FRAKTAG partitions (api + engine) via `rust/tools/xpart-probe`:
- **XPART-PROVE-1A (answer-class semantics) — PASS.** Under a source-aligned api capture all
  six `callers` cases returned a typed `AnswerClass` (Exact / Partial / Unavailable / Stale)
  with explicit reasons; **no silent-empty path**. Ratified default holds: xref-exact where the
  always-resident global xref is sufficient (per-partition counts), else
  partial-with-explicit-degradation; load-on-demand opt-in only.
- **XPART-PROVE-1B (dist↔src export-surface reconciliation) — EXECUTED, PASS (conditional).**
  Consumer resolves the dependency through its **published interface** (`dist/index.d.ts/...`)
  while the provider is indexed from **source** (`src/index.ts/...`): raw SCIP equality misses
  **95/95** api→engine refs. The `export_alias` layer (`DeclarationMapExact`: `.d.ts.map`
  `sources[]` + descriptor-exact reconstruction, asserted unique in `engine.scip`) reconciles the
  **named public API surface 78/78** (0 ambiguous, 0 misattachment, 0 silent miss); the six
  answer-class cases then PASS over the **dist** capture (target `BaseNode#id {api:2, engine:81}`).
  **Residuals keep ST3 open:** (1) anonymous structural members — 17 `typeLiteralNN` members are
  compilation-unit-relative, unstable across indexes **even in source-path** (`api-src` is
  95/78/17) → stay `Unresolved`, need positional/VLQ or explicit non-addressable degrade; (2)
  packages without declaration maps / complex `exports` (Basis 2 deferred). **No silent rewrite** —
  every alias carries basis + provenance (D3). **Answer-class precision rule:** `Exact` only for
  complete-basis symbols; an `Unresolved`/`Ambiguous`-dependent answer is `Partial`/`Unavailable`,
  never `Exact`.

**ST3 NARROWED, not retired:** declaration-map-backed **named** package-boundary traversal is
proven; ST3 stays open for anonymous structural members and no-declaration-map / complex-`exports`
packages. Specs `docs/slices/xpart-prove-1.md` + `xpart-prove-1b.md`; evidence
`docs/audits/xpart-prove-1/` (incl. `findings-1b.md`); probe `rust/tools/xpart-probe`
(`export_alias.rs`). **Next: XPART-ST3-BOUNDARY-DECISION** — decide whether the two residuals
block Stage-B ST3 closure for LiveGraph or become documented degraded answer-classes; **REFRESH-
PROBE-1 and RUST-INGEST-PROVE-1 follow after**, not before (migration plan treats ST3 as one unit).

Stage C runtime work (TRUST-MODEL-REBASE-1, LiveGraph/query/value-join) stays **gated behind
Stage B probe evidence** and must not begin before it exists.

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
