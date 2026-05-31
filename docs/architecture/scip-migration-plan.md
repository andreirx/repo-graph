# SCIP Substrate Migration — Risk-Driven Slice Plan

Status: PROPOSED (execution spine for the substrate pivot)
Depends: `docs/architecture/adr/adr-extraction-substrate-scip-first.md`,
`docs/slices/scip-ingest-ir-1.md`
Date: 2026-05-30

## Purpose and principle

This plan sequences the substrate migration **by risk, not by capability**. Each
slice is defined by the risk it retires, a go/no-go exit criterion, and a
documented retreat. Two rules govern ordering:

1. **Front-load strategic-trigger risks onto the thinnest viable foundation.** The
   risks that can still overturn the plan are retired first, with minimal sunk cost,
   before the full runtime is built on top.
2. **Every retreat narrows scope; none kills the plan.** Per the risk analysis, the
   failure modes constrain the envelope (e.g., C/C++ becomes graph-only) — they do
   not invalidate SCIP-first. The plan is designed for the ~60% likely outcome
   (asymmetric: TS strongest, C/C++ graph-strong/value-constrained, Rust messier,
   two-speed refresh likely), not the best case.

This is also a re-centering, not a rewrite: the daemon/CLI/transport shell, A1/A2
policy, stable-key model, tree-sitter value extractors, and trust-as-product-concept
survive. The execution core (raw-graph-in-SQLite → LiveGraph + compiler-grade
ingestion + derived/authority persistence) is what changes.

## Risk register (from the risk analysis)

| ID | Risk | Severity | Strategic-retreat trigger |
|----|------|----------|---------------------------|
| RK1 | AST↔SCIP join reliability, esp. C/C++ (macros, generated, expanded ranges) | High | **ST1** |
| RK2 | Call-edge derivation quality from occurrences + syntax | High (contained) | — |
| RK3 | Compiler-grade refresh cost at real repo scale | High | **ST2** |
| RK4 | Partition granularity + residency policy | High (tunable) | partial **ST3** |
| RK5 | Rust substrate robustness (panic/dups/slow/local-heavy) | Med-High | **ST4** |
| RK6 | Warm-cache representation + coherence cost | Medium (downstream) | — |
| — | Cross-partition traversal completeness/complexity | High | **ST3** |

Strategic-retreat triggers (the fault lines): ST1 C/C++ join too unreliable; ST2
compiler-grade refresh too slow even per-partition; ST3 cross-partition traversal
too incomplete to preserve trust; ST4 Rust/C toolchain friction makes "living
working code" too narrow. The plan retires ST1–ST4 in Stage B.

## Decision dependencies (D1–D5 leans this plan assumes)

D3 = mixed-mode (C/C++ semantic graph authoritative; value facts raw-anchored
unless strongly joined). D4 = separate AST value pass with explicit join contract.
D1 = IR in-memory only (warm cache is a later projection). D2 = graded edge model,
strict default query semantics. D5 = deterministic canonicalization, alternates
preserved as provenance. **Ratified 2026-05-30**; resolved in `docs/slices/scip-ingest-ir-1.md`.

---

## Stage A — Thin foundation (unavoidable prerequisite; keep minimal)

### SCIP-INGEST-IR-1 (design; exists)
Ratify D1–D5. No code. Exit: design signed off.

### INGEST-CORE-1 — core IR + stable-key mapper + scip-typescript adapter
- **Retires:** identity mechanism (stable-key synthesis), basic call derivation (RK2
  on the easy language). The canary: if derivation is noisy on TS, it will be worse
  everywhere.
- **Step forward:** the domain IR (`graph_ir`, `stable_key`, `provenance`), the
  stable-key mapper over SCIP symbols, the first producer adapter, reference +
  candidate-call derivation. In-memory only; one TS partition (fraktag/engine, SCIP
  already captured). No LiveGraph runtime, no warm cache, no decommission.
- **Go/no-go:** canonical stable keys populated from SCIP; references + candidate
  calls derived; stable keys byte-deterministic across two reindexes; in-memory
  `callers(X)` matches hand-checked truth for N≥30 symbols.
- **Retreat:** if stable-key synthesis is non-deterministic or call derivation is
  noisy on TS → rework the R1/R2 model before any language scales. Cheap, early.
- **Excludes (hard scope ceiling — keep this tiny):** NO LiveGraph residency manager,
  NO warm cache, NO non-TS adapters beyond a stub trait, NO broad value-layer
  integration, NO query migration beyond the in-memory proof harness, NO partition
  eviction, NO provenance beyond what the proof needs. One language, one partition
  model, one ingestion path, one stable-key path, one call-derivation path. If a
  capability is not required to prove identity + call derivation on one TS partition,
  it is out of this slice. Sprawl here defeats Stage B's purpose.

---

## Stage B — Retire the four strategic triggers on the thin foundation

### CJOIN-PROVE-1 — C/C++ AST↔SCIP join reliability  (ST1 / RK1 — highest severity, first)
- **Step forward:** scip-clang adapter + tree-sitter C/C++ value-fact extraction over
  leveldb; attempt the (file,range) join under D3 mixed-mode.
- **Measure:** % of value facts strongly joined to symbol identity vs raw-anchored
  fallback; macro/preprocessor-expanded range mismatch rate; which fact classes join
  reliably vs never.
- **Go/no-go:** a measured join-success rate and a clear mixed-mode boundary. Decide:
  is the C/C++ value-layer envelope (symbol-joined where strong, raw-anchored+labeled
  otherwise) acceptable?
- **Retreat (ST1):** if join is unreliable AND raw-anchored fallback is insufficient
  → C/C++ ships **graph-only** (references/calls), value-layer deferred. "Living
  working code" narrows to TS/Rust + C-graph. Documented narrowing.
- **Status (2026-05-31): ST1 range-only / terminal-name-mismatch risk RETIRED.** CJOIN-PROVE-1
  provided the leveldb fixture/probe setup; CJOIN-PROVE-2 amended its verdict and (macro-heavy
  nginx 95.9% name-confirmed; the range+terminal-name guard) retired the terminal-name-mismatch
  class. **Same-name overload / signature / template-instantiation ambiguity remains deferred
  hardening** (terminal-name correspondence is necessary, not sufficient).
  **Production rule:** a C/C++ value fact attaches to SCIP identity only when **range
  containment AND name correspondence agree**; otherwise raw-source-anchored. Range-only
  joining is **forbidden** (silently misattaches 15.1% on C++ annotation-macro code).

### XPART-PROVE-1 — cross-partition traversal semantics  (ST3)

**EXECUTED (2026-05-31), split into 1A + 1B. ST3 NARROWED, not retired.** 1A
(`docs/slices/xpart-prove-1.md`) proved the answer-class contract; 1B
(`docs/slices/xpart-prove-1b.md`) proved declaration-map-backed **named** package-boundary
identity reconciliation (FRAKTAG named surface 78/78 reconciled, 0 misattachment, 0 silent miss).
**ST3 remains OPEN** for two residuals: anonymous structural members (`typeLiteralNN` is
compilation-unit-relative, unstable across indexes even same-file) and packages without
declaration maps / with complex `exports` (Basis 2). **Resolved by XPART-ST3-BOUNDARY-DECISION
(2026-05-31)** (`docs/slices/xpart-st3-boundary-decision.md`): both residuals are documented
**degraded answer-classes** (`null`=unknown, never empty), **not blockers**; ST3 is **closed for
the LiveGraph stage** with those classes, each carrying a named upgrade slice (positional/VLQ;
Basis 2). **REFRESH-PROBE-1 is next.**

- **Step forward:** minimal 2-partition in-memory load (TS api + engine) + an
  always-resident global cross-reference index (symbol → partition); `callers` /
  `path` spanning partitions.
- **Answer-class contract (must be specified, not discovered):** define the exact
  target query surfaces (`callers`, `path`) and, for each, the allowed answer classes
  when a referenced partition is NOT resident — choose and specify per surface:
  (a) exact via the always-resident global xref alone; (b) exact only after
  load-on-demand of referenced partitions; (c) partial with explicit, machine-readable
  degradation; (d) forced eager load of referenced partitions. **Forbidden: silent
  incompleteness** (an answer that looks complete but is not).
- **Go/no-go:** results exact when all referenced partitions are resident; when not,
  the answer falls into a *declared* class carrying an explicit degradation marker —
  never silent partial; global xref index size and build cost bounded and measured.
  The answer-class half is retired only when the completeness contract is written down and
  enforced (1A); the cross-partition identity half (1B) and its residuals are separate — ST3 is
  NOT globally retired by this section.
- **Retreat (ST3):** if stitching is too complex/incomplete to preserve trust →
  fall back to load-all-partitions-of-a-repo-per-query (sacrifice memory benefit,
  keep correctness) or residency-scoped answers with explicit degradation.

### REFRESH-PROBE-1 — compiler-grade refresh cost at scale  (ST2 / RK3)

**EXECUTED (2026-05-31) → VERDICT B (two-speed refresh).** Whole-partition SCIP indexing dominates
and exceeds the synchronous A budget on every measured partition (FRAKTAG engine ~1.9s chain; amodx
plugins ~3.0s); no-op ≈ edit (refresh unit = partition). C not indicated (seconds, tooling stable).
Bursts MUST coalesce (8.4× waste, K=8); provider public-API edits invalidate only **referencing**
consumers (precise ~3.5s cascade = dist-rebuild + provider + consumer reindex); cross-partition
xref/alias recompute ~21ms → the slow path is **indexer-bound**, not repo-graph-bound. Runtime
contract + claim constraints (burst window not yet set; fanout uses affected exported-symbol refs;
xref/alias negligibility is TS-package-boundary-only) in `docs/slices/refresh-probe-1.md`; evidence
`docs/audits/refresh-probe-1/findings.md`. **ST2/RK3 refresh-model risk retired (B).**
**RUST-INGEST-PROVE-1 is next** (last open Stage B risk).

- **Measure workflow shape, not just wall-clock:** (1) cost per edit (single-file edit
  -> partition reindex); (2) cost per partition unit (package / crate / TU-group);
  (3) cost under a **bursty edit loop** (rapid successive edits, as in real agent/dev
  work); (4) **edit fan-out** — do common edits hit small partitions or force large
  ones; (5) **blocking model** — is the user blocked synchronously, or can they work on
  stale/approximate state while compiler-grade truth catches up. Targets: amodx
  (8-package TS), a large repo-graph crate, duckdb C if feasible.
- **Go/no-go:** the measures produce a decision on whether two-speed is required **in
  practice** (not theory) for interactive workflows, with the bursty-loop and
  blocking-model dimensions documented. Collecting only per-partition wall-clock =
  the risk is NOT retired.
- **Retreat (ST2):** if too slow → two-speed model (tree-sitter fast delta — already
  retained for the value layer — + SCIP slower truth, with explicit "approximation
  pending re-index" degradation). Known fallback; adds runtime complexity, so measure
  before committing.

### RUST-INGEST-PROVE-1 — Rust per-crate robustness at breadth  (ST4 / RK5)
- **Step forward:** per-crate ingestion + duplicate-symbol dedup (D5 deterministic
  canonicalization: lib > bin > test; alternates preserved as provenance) across ALL
  repo-graph crates (only `storage` proven in the spike).
- **Frame as setting the support boundary, not "make Rust work":** the slice must
  output an explicit **Rust support contract** — per-crate only (whole-workspace
  unsupported) yes/no; the exact dedup / canonical-identity rule (D5); the
  "definition-not-in-document" loss tolerance; and the minimum ingestion-quality bar
  for self-host. A bounded contract, not an open-ended "improve Rust" track.
- **Go/no-go:** all repo-graph crates ingest per-crate without fatal errors; dedup
  deterministic across reindex; loss rate quantified against the stated tolerance; the
  support contract is written down.
- **Retreat (ST4):** if quality is below bar → Rust is second-class (self-host with
  caveats); TS/C are the primary supported substrates. Taken explicitly, not drifted
  into.

**End of Stage B = all four fault lines characterized.** Proceed to build the runtime
only with the strategic envelope known.

---

## Stage C — Build the runtime (strategic risks now characterized)

### TRUST-MODEL-REBASE-1 — rebuild trust from first principles  (FIRST in Stage C; gates query credibility)
unresolved-rate is dead, and the runtime is heterogeneous (graded calls, C raw-anchored
facts, Rust second-class possibility, residency-dependent completeness). Trust is being
rebuilt from scratch, so it is first-class and must precede any claim that a migrated
query surface is credible. Consumes Stage B evidence. Define the new trust axes and
reporting contract:
- build/index coverage (did the producer run; which files indexed)
- provenance/toolchain completeness (indexer + versions captured)
- join strength / attachment quality (from CJOIN-PROVE-1)
- call-edge confidence (graded, from D2)
- partition residency/completeness (from XPART-PROVE-1's answer-class contract)
- freshness/staleness (compiler-grade vs approximation; from REFRESH-PROBE-1)
Per-partition, per-language, graded. **Exit:** a trust contract downstream query
surfaces MUST honor; zero dependence on unresolved-rate. Axis *population* lands as the
runtime/value slices below implement their inputs; the *contract* is fixed here so
trust does not sprawl across and contaminate later slices.

### LIVEGRAPH-RUNTIME-1 — residency manager + coherence state
Load/unload/evict under a memory ceiling; partition lifecycle; freshness/coherence
state machine; honest reporting (not-loaded / partial / stale / approximation /
compiler-grade). Concretizes RK4 using Stage B evidence, and **emits the residency /
freshness states the TRUST-MODEL-REBASE-1 contract consumes.** Exit: partitions load
and evict under ceiling; queries carry honest residency/freshness labels.

### QUERY-MIGRATION-1 — re-point callers/callees/path/cycles to LiveGraph
Strict default (D2): default traversal uses syntax-confirmed CALLS only (preserves the
Layer-1 deterministic claim); graded REFERENCES carried underneath, surfaced on request
with labels. Honors the TRUST-MODEL-REBASE-1 contract and the XPART answer-class
contract. SQLite fallback during transition. Exit: traversal LiveGraph-driven, parity
on confirmed edges, strict-default + explicit expanded mode, trust-labeled.

### VALUE-JOIN-1 — AST value-layer extractor as a separate pass (D4)
Boundaries/state/framework/contracts/quality, joined to canonical identity where strong
(D3 mixed-mode), raw-anchored+labeled otherwise. tree-sitter formally demoted from graph
backbone to value producer. Feeds the join-strength trust axis. Exit: value facts attach
to canonical keys where joinable; honest raw-anchored fallback elsewhere.

---

## Stage D — Persistence and decommission (downstream, lower severity)

### PARTITIONED-WARM-CACHE-ARCH-1 (design) + WARM-CACHE-1 (impl) — RK6
Format decision (rkyv / Cap'n Proto / embedded KV) + partitioned binary write/load +
coherence/versioning. Done AFTER the runtime is credible (D1=A kept the IR in-memory
until here). Exit: warm restart beats reindex; invalidation correct; domain model not
bent around cache. **Retreat:** if too costly → defer; accept reindex-on-restart
(fine for small/medium repos; large repos pay cold start). Runtime works without it.

### COHERENCE-LAYER-1 — check/trust/orient mixed live + persisted
The two-store coherence contract (SQLite authority/derived + binary warm cache).

### RAW-DECOMMISSION-1 — retire raw relational graph
Remove `nodes`/`edges`/`unresolved_edges` writes, the resolver/classifier, and the
raw-graph retention burden — **last**, only after parity is proven. This is when the
old storage center dies.

---

## Relationship to the ADR

This plan details the ADR Implementation list by inserting the Stage-B probe slices
(CJOIN-PROVE-1, XPART-PROVE-1, REFRESH-PROBE-1, RUST-INGEST-PROVE-1) ahead of the
runtime build, so the four strategic triggers are retired on a thin foundation. The
ADR's SCIP-INGEST-IR-1 → PARTITIONED-WARM-CACHE-ARCH-1 → QUERY-MIGRATION-1 →
COHERENCE-LAYER-1 spine is preserved and expanded here.

## What survives / what dies (re-centering, not rewrite)

Survives: daemon/transport/CLI shell, A1/A2 policy, stable-key model, tree-sitter
value extractors, trust-as-concept, governance (declarations/waivers/baselines/
aliases), long-lived daemon. Dies/demoted: homegrown cross-file resolver,
unresolved-edge machinery, classifier built on it, raw relational graph as truth,
DB-coupled traversal logic, raw-graph retention/prune burden. Estimated ~30–40% of
the product shape survives; the central index/query/storage engine changes
fundamentally.
