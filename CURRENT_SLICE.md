# CURRENT_SLICE.md

## Current Priority

> **STATUS (2026-06-20): Stage D — SQLite raw decommission. COHERENCE-LAYER-1 DONE; the Option-B producer
> investigation CLOSED (NO-GO → Option A); **SQLITE-RAW-DECOMMISSION-1 RATIFIED as a BOUNDED partial-decommission
> CONTRACT** (Option A; `docs/slices/sqlite-raw-decommission-1.md`) — `unresolved_edges` + diagnostics retained
> SQLite-labelled FOREVER, `nodes`/`edges` bounded-partial with the retirement IMPL **DEFERRED** on prerequisites;
> a FULL `nodes`/`edges`/`unresolved_edges` retirement is PROVEN partially impossible (the trust contributor's
> unresolved-call fields have no current-state SCIP source). **PREREQ-1's focus-resolution lever is now CLOSED at
> HEAD `7fff04e`:** the COHERENCE-LEAF-SERVE arc shipped — focus-resolution producer `ccaad68`, consumed by orient
> (`765583b`; SYMBOL-focus `nodes`-free on green) + explain (`9e6077c`; SYMBOL-focus `nodes`-free + relevance-ranked
> callers). The achievable `nodes`-free-on-green surface is BANKED (the 6 drilldowns + orient SYMBOL + explain
> SYMBOL); the PERMANENT SQLite floor + PREREQ-2 + the bounded retirement IMPL are DEFERRED (diminishing returns).
> Next BUILD: P1 marginal fastpaths and/or P2 non-TS coverage — still an OPEN governance call.**
> Stages A–C are COMPLETE; Stage D (persistence + raw decommission) is in progress. Since this document's
> body was last rewritten (2026-06-01), the warm-cache chain shipped (WARM-CACHE-1 + daemon-wiring +
> valuefacts + producer-absent), the imports + cycles + **stats** LiveGraph **default fastpaths** landed,
> callers/callees/path went **lazy**, the **SQLITE-RAW-DECOMMISSION-READINESS-1..7** audits ran, and
> **STATS-LIVEGRAPH-1 SHIPPED** (spec `f6046ab` + impl `28ed216`) as the **6th** SQLite-free migrated
> default (`stats` served from LiveGraph via the IR symbol-attributes cert-fastpath, byte-preserving).
> **THEN THE WHOLE COHERENCE LAYER LANDED:** the ratified `CoherenceEnvelope<T>` contract
> (COHERENCE-LAYER-1 `6ed17b8` + multi-source-leaf amendment `5129f44`) and all four per-command builds —
> **orient `2fd4478`** (+ the `repo-graph-coherence` support crate), **check `3e76271`**, **explain
> `82b6557`**, **trust `dc55114`** (then the Option-B investigation `7d4b3bb`, then the COHERENCE-LEAF-SERVE arc → **HEAD `7fff04e`**). orient/check/explain/trust now each serve a `CoherenceEnvelope`
> with honest per-signal provenance/trust/freshness + labelled SQLite fallback (explain serves green leaf
> VALUES from the LiveGraph; trust adds a LiveGraph posture beside the retained v1 hybrid; orient
> no-loss-labels its 4 LG-first leaves; check folds a MEET-freshness verdict). This was the last command
> class with NO LiveGraph path.
> **HONEST SCOPE — coherence did NOT remove the eager SQLite read** [OBSERVED, first-hand: dispatch.rs base
> use case unconditional in all four handlers]: the base use case still reads SQLite (incl. `nodes`/`edges`
> for orient/explain/trust) every call; the envelope is assembled ON TOP. SQLite-FREE served count stays
> **6/10**. So coherence is a SERVING + OUTPUT-HONESTY milestone, NOT an eager-read elimination.
> **THE OPTION-B PRODUCER INVESTIGATION CLOSED (NO-GO → Option A); the decommission goal is RE-BOUNDED.**
> readiness-9 recommended Option B (eliminate the four coherence eager reads) as the incremental path to the
> eventual global `nodes`/`edges` drop, leaving A-vs-B open. The four-commit arc that followed proved that path
> is bounded by a substrate boundary: ORIENT-SQLITE-FREE-1 (`e10a455`) + EXPLAIN-SQLITE-FREE-1 (`f3237f9`) found
> orient and explain are both producer-gated on ONE shared trust-core source; TRUST-SUMMARY-LIVEGRAPH-1
> (`94fc506`) found that producer NEEDS-EXTENSION (`IrEdge` is resolved-only; unresolved calls are dropped at
> SCIP ingest); SCIP-UNRESOLVED-CALL-PROBE-1 (`7d4b3bb`) returned **NO-GO** — scip-typescript emits no
> occurrence for an unresolved call target (paired evidence: SCIP-recoverable 0 ≠ homegrown `unresolved_edges`
> 3, structurally inverted). Operator ratified **Option A**: keep the homegrown `unresolved_edges` (+
> diagnostics) SQLite-LABELLED (the TRUST-LIVEGRAPH-1 Half-B shape).
> **VISION-level boundary (readiness-10):** the trust contributor's unresolved-call fields are RED **BY DESIGN**
> — no current-state SCIP source — so a FULL `edges`/`unresolved_edges` decommission for that contributor is
> IMPOSSIBLE, not pending. (This does NOT refute SCIP as substrate; it bounds the narrow claim that SCIP can
> SOURCE a parity unresolved-call count.) The **A-vs-B question is RESOLVED for B**: Option B survives only as a
> marginal partial that flips NO deletion gate. The other gates stay RED-pending-work (non-TS ceiling; drilldown
> fallbacks; imports/cycles/stats cert builds; the 31 non-graph tables).
> **NEXT is an OPEN governance call** (recommend, not ratified): `docs/slices/
> sqlite-raw-decommission-readiness-10.md` §"Next priorities" lays out a P1–P4 matrix (marginal partial
> fastpaths / non-TS coverage / a BOUNDED partial decommission / pivot off). P3 (the bounded contract) is RATIFIED
> and PREREQ-1 (the (b)-leaf serve / focus-resolution lever) has since SHIPPED + CLOSED (`7fff04e`); P1/P2 remain
> unchosen. The next track is NOT chosen here.
> [INFERRED priority; OBSERVED-backed — git HEAD=`7fff04e` chain (the COHERENCE-LEAF-SERVE arc) + readiness-10,
> which supersedes readiness-9.]
> The running log below is historical narrative through 2026-06-01; for the present state trust this banner and
> the **Stage D order** line below.

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

**Stage B probes — COMPLETE (historical log; see the STATUS banner above for the present priority).** CJOIN-PROVE-1 + **CJOIN-PROVE-2 EXECUTED → ST1
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
`classify_answer` — no global basis completeness; `AnswerEnvelope` smart constructors; 22 invariant
tests green (amended ×3: residency `missing_partitions`, `Partial` by non-`Fresh` freshness, **+D1
`contributing_languages` set** in QUERY-MIGRATION-1); maturity **PROTOTYPE**, not PRODUCTION until
consumed). The existing `repo-graph-trust`
(v1 SQLite reporting) is untouched. Spec `docs/slices/trust-model-rebase-1.md`.

**LIVEGRAPH-RUNTIME-1 IMPLEMENTED** — crate `repo-graph-livegraph` (in-memory; deps `repo-graph-ir`
+ `repo-graph-trust-model` only). Partition residency + per-partition epoch + `XrefEpoch` + the
always-resident xref + trust-labelled `callers` (`AnswerEnvelope` via the vocabulary). D1 accept+swap
(no indexers); D2 explicit load/unload; D3 per-partition epoch + contributing-epochs; D5 `callers`
only. **`callers` is SCIP-dependent → refresh-pending is `Partial`+`PrecisionPending`, never
`Exact`.** 8 tests green at closure (the 7 D5 cases + atomic swap; now 17 after QUERY-MIGRATION-1
added `callees` + the language union). The build surfaced + fixed TWO
`repo-graph-trust-model` amendments (residency `missing_partitions`; `Partial` justified by
non-`Fresh` freshness; 20 trust tests). Spec `docs/slices/livegraph-runtime-1.md`.

**QUERY-MIGRATION-1 IMPLEMENTED** — `repo-graph-livegraph` now serves `callers` + `callees` (D2;
`path` deferred) through the trust vocabulary, headless (D3; no shipped CLI), extending the runtime
crate (D4). **D1 language metadata:** `AnswerEnvelope` carries `contributing_languages:
BTreeSet<LanguageSupport>` — a multi-partition answer reports the UNION of contributing language
maturities; the prior last-wins collapse is gone. `callees` requires the target's defining partition
resident (the always-resident xref retains only incoming adjacency — ratified asymmetry;
summary-level `callees` deferred). `callers`/`callees` share one `finalize_envelope` (SCIP-dependent
refresh → `Partial`, residency → `Partial`). 17 livegraph tests (4 language-union + 5 callees-core +
8 prior); 22 trust tests. Scope rule: this migrated query SEMANTICS onto the headless Test API, NOT
the shipped CLI. Spec `docs/slices/query-migration-1.md`.

**VALUE-JOIN-1 IMPLEMENTED** — `repo-graph-livegraph` value-layer facts: `load_value_facts` (D6
separate channel — NOT on `PartitionIr`) + `value_facts(symbol)` trust-labelled (`SymbolOwnership`).
D1 cyclomatic complexity only; D2 attach owned only when the basis is `SymbolOwnership`-complete, else
RawAnchored; D3 raw-anchored preserved (value true, only ownership degraded — the key rule); D4 TS
only (C/C++ → VALUE-JOIN-CXX-1, Rust deferred); D5 `value_facts` only; D7 epoch-bound facts
(swap-without-reload → `Stale`). An AST-owned fact stays `Exact` under `PrecisionPending` via
`NotScipDependent` (the invariant-6 path callers/callees avoid). 8 value-fact tests (25 livegraph
total). `repo-graph-ir` + `repo-graph-trust-model` UNTOUCHED. The real complexity→`ValueFact` adapter
belongs in the Stage-D wiring layer (dependency direction). Spec `docs/slices/value-join-1.md`.

**STAGE C COMPLETE** (TRUST-MODEL-REBASE-1 → LIVEGRAPH-RUNTIME-1 → QUERY-MIGRATION-1 → VALUE-JOIN-1 all
implemented). **LIVEGRAPH-INTEGRATION-1A IMPLEMENTED** — crate `repo-graph-livegraph-feed` (the outer adapter
depending on BOTH `repo-graph-scip-ingest` + `repo-graph-livegraph` — the dep direction the runtime
must never invert). `feed_partition` converts a real `IngestOutcome` → `load_partition` +
complexity→`ValueFact`→`load_value_facts`. Proven against the committed real `synthetic/index.scip`
(real producer output, NOT hand-built): `value_facts(Circle.describe)` → Exact; real callers/callees
non-empty + trust-labelled; epoch-bound (swap→Stale). 3 integration tests; `repo-graph-livegraph` +
`repo-graph-ir` + `repo-graph-scip-ingest` UNTOUCHED. **Residual:** multi-partition real data NOT
proven (single committed partition) → LIVEGRAPH-INTEGRATION-XPART-1. Spec
`docs/slices/livegraph-integration-1a.md`.

**LIVEGRAPH-INTEGRATION-1B IMPLEMENTED + LIVE-VALIDATED (phase-1, 2026-06-01)** — flag-gated `--engine
sqlite|livegraph|compare` on shipped `rmap callers/callees` (default sqlite byte-identical) + hidden
`rmap dev livegraph-preload` (S1: Rust-only `DaemonClient`, no TypeScript). Daemon `RepoState` holds a
preloaded `LiveGraph`; livegraph serves with SQLite fallback; compare writes a classified
`.rgr/livegraph-compare/<ms>.json` sidecar. Validated live (`dev-install-local.sh` + the six `rmap`
calls on the synthetic pilot): default unchanged, livegraph hit the populated graph
(`resolution: livegraph`), compare sidecars `Exact` (SCIP keys byte-equal to SQLite via repo_uid). 65
daemon + 444 rgr tests; clippy/fmt clean. Spec `docs/slices/livegraph-integration-1b.md`.

**DATAFLOW-HOTPATH-MAP-1 DELIVERED (2026-06-01)** — `docs/architecture/dataflow-hotpath-map.md` (10
sections): the source→SCIP→ingest→`PartitionIr`→value-facts→LiveGraph→`AnswerEnvelope`→persistence data
shapes, authority/rebuildability, epochs, hot paths (pipeline is **indexer-bound**: SCIP ~1.9–3.0s/TS
partition vs xref ~21ms), copy points, and implications for 1C + warm-cache (**serialize `PartitionIr`
only; rebuild the rest**; key interning is the dominant allocation target).

**LIVEGRAPH-INTEGRATION-1C COMPLETE (synchronous daemon-owned production, 2026-06-01)** — all 7
build-order steps: producer discovery (D0), the six D6 failure classes, `rmap dev livegraph-refresh`,
and the SYNCHRONOUS success path (Option 1: producer runs inline via `std::process::Command`; write
lock only for the swap; real `build_inputs_hash`). Live-validated: `livegraph-refresh` →
`refreshed: true` (the daemon ran `scip-typescript` itself, NO preload — 15 nodes/5 value facts);
`--engine livegraph` callers/callees served from the refresh-populated LiveGraph; default sqlite
unchanged; `ProducerUnavailable` tested; failure keeps last-good (swap only on Ok). Producer
provisioned dev-only (pinned `scip-typescript@0.4.0` under a local Node-18 wrapper — 0.4.0 crashes on
Node 22; `RMAP_SCIP_TYPESCRIPT` via launchd; not committed). **Non-blocking async deferred
(`DaemonState` is `!Send`) → DAEMON-ASYNC-REFRESH-1; PRODUCER-COMPAT-1 = 0.4.0⊥Node22.** Spec
`docs/slices/livegraph-integration-1c.md`.

**PARTITIONED-WARM-CACHE-ARCH-1 RATIFIED (2026-06-01)** — warm-cache architecture (non-authoritative,
safe-to-delete, validated-before-load). D1 persist `PartitionIr` only; D2 bincode first (format
subordinate to the validation envelope); D7 + a ValueFacts sidecar (independent — its failure never
invalidates the graph cache); D8 cache-side mirror DTOs (**NO serde in `repo-graph-ir`**); D3–D6 cache
key / manifest validation / atomic write / refresh interaction. Spec
`docs/slices/partitioned-warm-cache-arch-1.md`.

**Stage D order (updated 2026-06-20):** 1B ✓ → DATAFLOW ✓ → 1C ✓ → WARM-CACHE-ARCH ✓ → WARM-CACHE-1 ✓
(support crate: DTO round-trips, manifest, atomic write) → WARM-CACHE-DAEMON-WIRING-1 ✓ →
WARM-CACHE-VALUEFACTS-1 ✓ → WARM-CACHE-PRODUCER-ABSENT-1 ✓ → imports + cycles LiveGraph default
fastpaths ✓ + lazy callers/callees/path ✓ → SQLITE-RAW-DECOMMISSION-READINESS-1..7 (audits) →
STATS-LIVEGRAPH-1 ✓ (spec `f6046ab` + impl `28ed216`; the 6th SQLite-free default — `stats` served from
LiveGraph via the IR symbol-attributes substrate, SQLite fallback intact, byte-preserving) →
COHERENCE-LAYER-1 ✓ (contract `6ed17b8` + amendment `5129f44`; four per-command impls: orient `2fd4478`
[+ `repo-graph-coherence` support crate] / check `3e76271` / explain `82b6557` / trust `dc55114`) →
SQLITE-RAW-DECOMMISSION-READINESS-9 ✓ (post-coherence recompute; gate RED) →
**Option-B producer investigation (CLOSED, NO-GO):** ORIENT-SQLITE-FREE-1 ✓ (`e10a455`; orient producer-gated,
deferred) → EXPLAIN-SQLITE-FREE-1 ✓ (`f3237f9`; explain producer-gated, same trust-core source) →
TRUST-SUMMARY-LIVEGRAPH-1 ✓ (`94fc506`; the shared producer is NEEDS-EXTENSION) → SCIP-UNRESOLVED-CALL-PROBE-1 ✓
(`7d4b3bb`; **NO-GO** — SCIP carries no unresolved-call disposition → operator ratified **Option A**, keep
homegrown `unresolved_edges` SQLite-labelled) → SQLITE-RAW-DECOMMISSION-READINESS-10 ✓ (end-of-arc re-baseline;
the SCIP unresolved-call boundary; supersedes readiness-9) →
**SQLITE-RAW-DECOMMISSION-1 (terminal; bounded CONTRACT RATIFIED 2026-06-14, Option A —
`docs/slices/sqlite-raw-decommission-1.md`; `unresolved_edges` + diagnostics retained-forever, `nodes`/`edges`
bounded-partial, retirement IMPL DEFERRED on prereqs; gate 1 permanently-partial)** →
**PREREQ-1 COHERENCE-LEAF-SERVE arc** (`docs/slices/coherence-leaf-serve-1.md`): FOCUS-RESOLUTION-LIVEGRAPH-IMPL
`ccaad68` (LiveGraph focus resolver + no-loss cert) → COHERENCE-LEAF-SERVE-IMPL-1 `765583b` (orient SYMBOL-focus
`nodes`-free on green) → COHERENCE-LEAF-SERVE-IMPL-2 `9e6077c` (explain SYMBOL-focus `nodes`-free + relevance-
ranked callers) → **PREREQ-1 focus-resolution lever CLOSED + decommission CHECKPOINTED `7fff04e` (HEAD)**
(PREREQ-2 + the retirement IMPL DEFERRED; the permanent SQLite floor stands). Next BUILD: P1/P2 — OPEN.
[OBSERVED: git HEAD=`7fff04e` chain; `git show -s` subjects for `ccaad68`/`765583b`/`9e6077c`/`7fff04e`;
`docs/slices/{orient,explain}-sqlite-free-1.md` + `trust-summary-livegraph-1.md` +
`scip-unresolved-call-probe-1.md` + `sqlite-raw-decommission-readiness-10.md` + `coherence-leaf-serve-1.md`.]

The WARM-CACHE-1 slice doc header still reads "DESIGN — building"; that header is itself stale (the
crate shipped and was extended — `repo_graph_version` key fix `7b0eb4c`, warm-cache schema bumps in the
imports thread, and the IMPLEMENTED dependents above). Correcting that slice doc is out of scope here
(slice docs are read-only for this reconciliation).

The remaining spike measures (precise CALLS parity, multi-config C, all-crates Rust,
M3, M4b) are validation tracks for the IR slice, not blockers. The IR shipped
(INGEST-CORE-1), so "after the IR" has passed: the refresh model and warm-cache
format are now DECIDED, not deferred (see REFRESH-PROBE-1 and
PARTITIONED-WARM-CACHE-ARCH-1 above, and the status note below).

Execution spine (risk-driven): `docs/architecture/scip-migration-plan.md`. Stages:
A thin foundation (SCIP-INGEST-IR-1 design → INGEST-CORE-1) → B retire the four
strategic-trigger risks on that foundation (CJOIN-PROVE-1 C/C++ join, XPART-PROVE-1
cross-partition, REFRESH-PROBE-1 refresh-at-scale, RUST-INGEST-PROVE-1) → C runtime
(LiveGraph/query/value-join/trust) → D persistence + raw decommission. Each slice
carries a go/no-go and a documented retreat that narrows scope, never kills the plan.

Refresh model, partition granularity, and warm-cache format were resolved across
migration-plan Stages B–D — they are now DECIDED, not deferred. They were never gated
on the viability spikes (TS/C/Rust), which are complete and retired the gate.
[OBSERVED — recorded above in this document and in `docs/ROADMAP.md` §"Honesty":]
- Refresh model: REFRESH-PROBE-1 → two-speed Verdict B (serve last-good epoch + AST
  fast delta, coalesce bursts; the refresh unit is the partition).
- Warm-cache format: PARTITIONED-WARM-CACHE-ARCH-1 RATIFIED (2026-06-01) — bincode
  under a validation envelope; WARM-CACHE-1 shipped and was extended in Stage D.
- Partition granularity: the partition is the refresh unit (REFRESH-PROBE-1); the
  exact coalescing window is runtime-tuned, not a remaining design gate.

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
