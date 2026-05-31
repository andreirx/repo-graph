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

**XPART-PROVE-1 (cross-partition traversal / ST3) — SPLIT; 1A+1B EXECUTED. ST3 NARROWED → CLOSED for the LiveGraph stage with degraded classes (not globally retired).**
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

**ST3 NARROWED → CLOSED for the LiveGraph stage (not globally retired).** Declaration-map-backed
**named** package-boundary traversal is proven. **XPART-ST3-BOUNDARY-DECISION (2026-05-31)**
classifies the two residuals as **degraded answer-classes, not blockers**: anonymous structural
members (`typeLiteralNN`) → `Unavailable`/`AnonymousStructuralMember`; no-declaration-map /
complex-`exports` packages → `Partial`/`Unavailable`/`UnreconciledExportSurface`. **Safety rule:
`null`=unknown, never empty** (an unaddressable target is unknown, not known-zero). Each residual
carries a named upgrade slice (positional/VLQ; Basis 2). Specs `docs/slices/xpart-prove-1.md` +
`xpart-prove-1b.md` + `xpart-st3-boundary-decision.md`; evidence `docs/audits/xpart-prove-1/`
(incl. `findings-1b.md`); probe `rust/tools/xpart-probe` (`export_alias.rs`).

**REFRESH-PROBE-1 (refresh model / cost) — EXECUTED → VERDICT B (two-speed refresh).** Whole-partition
SCIP indexing dominates and exceeds the synchronous A budget on every measured partition (FRAKTAG
engine ~1.9s chain; amodx plugins ~3.0s); no-op ≈ edit (refresh unit is the partition). C not
indicated (seconds, tooling stable). **Workload shape:** bursts MUST coalesce (8.4× waste, K=8);
provider public-API edits invalidate **only referencing consumers** (precise, ~3.5s cascade =
dist-rebuild + provider + consumer reindex); cross-partition xref/alias recompute ~21ms → the slow
path is **indexer-bound**, not repo-graph-bound. **Runtime contract:** serve last-good epoch + AST
fast delta + `Stale`/`PrecisionPending`, coalesce bursts, atomic epoch swap, keep last-good on
failure, never `Exact`-empty. **Constraints:** burst proves coalescing mandatory but NOT the final
window (runtime tunes later); fanout invalidation must use affected exported-symbol refs + degrade
conservatively when uncertain; xref/alias negligibility is TS-package-boundary-only (do not
generalize to all languages / huge workspaces). Spec `docs/slices/refresh-probe-1.md`; evidence
`docs/audits/refresh-probe-1/findings.md`; probe `rust/tools/refresh-probe`.

**RUST-INGEST-PROVE-1 (Rust SCIP ingestion support boundary) — EXECUTED → GO WITH CAVEATS.** Rust
enters Stage C only as **per-crate, async (B-very-slow-async), SCIP-backed, degraded (PARTIAL/BETA)**.
Per-crate stable ~29–32s p95 (N=3, storage/indexer/rgr, 0 panics); **whole-workspace UNSUPPORTED**
(rust-analyzer panics ~32.5s). Identity **~94–96% SCIP-synthesized fallback** (no value-level AST
join) → **no TS/C parity**. Cross-crate resolution works (other-crate/stdlib/external). Refresh =
**background per-crate, never synchronous**; last-good epoch served, `Stale`/`PrecisionPending` until
done; explicit refresh = operator fallback. **Support boundary** (per-crate only; whole-ws
unsupported; B-very-slow-async; fallback-identity dominant; deterministic dedup+aliases;
def-not-in-document tolerated if bounded/surfaced + not corrupting public symbols/call-graph) +
**residuals** (public-duplicate audit; def-not-in-document impact audit — both before PRODUCTION) in
`docs/slices/rust-ingest-prove-1.md`; evidence `docs/audits/rust-ingest-prove-1/findings.md`; probe
`rust/tools/rust-ingest-probe`. Honesty correction: the spike's "72% local" is NOT reproduced
(measured distinct-local 15–23%).

**STAGE B COMPLETE.** All four strategic-trigger probes done — CJOIN (ST1), XPART + boundary (ST3),
REFRESH (B two-speed), RUST-INGEST (GO-with-caveats). **Strategic risks are bounded, not erased.
Stage C may begin with scoped support contracts.**

**STAGE C STARTED.** **STAGE-C-ENTRY-DECISION** recorded (`docs/architecture/stage-c-entry-decision.md`):
the trust/freshness/identity vocabulary + the order `TRUST-MODEL-REBASE-1 → LIVEGRAPH-RUNTIME-1 →
QUERY-MIGRATION-1 → VALUE-JOIN-1`. **TRUST-MODEL-REBASE-1 IMPLEMENTED** — crate
`repo-graph-trust-model` (pure-domain; 7 types AnswerClass / FreshnessState / IdentityBasis /
DegradationReason / LanguageSupport / ProvenanceBasis / QueryCompleteness; **query-contextual**
`classify_answer` — no global basis completeness; `AnswerEnvelope` smart constructors; 13 invariant
tests green; maturity **PROTOTYPE**, not PRODUCTION until consumed). The existing `repo-graph-trust`
(v1 SQLite reporting) is untouched. Spec `docs/slices/trust-model-rebase-1.md`.

**Next: LIVEGRAPH-RUNTIME-1 (spec-first).** In-memory runtime ONLY; consumes `repo-graph-trust-model`;
**partition residency + epoch state + answer-class degradation are first-class.** **NO query
migration, NO warm-cache persistence, NO callers/callees CLI behavior** in this slice — query
migration comes AFTER the runtime substrate exists. Stage C order then continues:
QUERY-MIGRATION-1 → VALUE-JOIN-1.

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
