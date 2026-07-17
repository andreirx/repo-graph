# RECON-DESIGN-1 — the reconciliation layer: one graph from two witnesses (SPEC)

Status: SPEC SLICE — analysis + design only, NO code changes (2026-07-17)
Track: Reconciliation (ENGINE-CONSOLIDATION-1 §8b, ratified direction: reconciliation over
adjudication) · Builder: claude-fable-5 (architecture judgment)
Inputs: RECON-SPIKE-1 §5 (the classified fixture divergence: 9 canonical edges, 7
SCIP-only — ALL semantic reference kinds the pipeline never models [new-expression ctor
calls, this.field reads, property/class incoming refs], 0 pipeline-only, 0 identity
mismatch, adoption byte-equal) · LIVEGRAPH-PARTIAL-FIX-1 (exhaustive walks are now
panic-free) · the EC-1 §4 fact-class table + §8c interim rule.

## 1. The design question

Two witnesses observe the code: the syntax pipeline (all languages, works on
non-compiling code, produces the RED-floor disposition) and SCIP semantic ingestion
(covered languages only, compiler-verified, richer reference kinds). The ratified
direction: reconcile — a union graph with per-edge witness provenance, aggregates over
the reconciled graph, divergence itself a surfaced fact. This spec makes that concrete
and ratifiable.

## 2. Deliverable — written INTO this doc by the builder

**§3 The witness model.** Per-edge provenance representation: witnessed-by-both (the
highest-confidence class — two independent derivations), SCIP-only ("semantic resolution,
compiler-verified" basis), pipeline-only ("syntactic extraction — works where compilation
fails" basis). Handle: multiplicity; reference-KIND asymmetry (the spike proved SCIP
models kinds the pipeline has NO vocabulary for — ctor-via-new, field reads, property/
class incoming refs: are these NEW edge kinds in the union, or a SCIP-only enrichment
tier? decide with honesty rationale); identity (adoption is byte-equal per the spike —
state the assumption + the guard if it breaks).

**§4 The no-second-witness regime (the DOMINANT case).** For uncovered languages/
partitions (all of ../legacy-codebases: C/C++/Java/Python today) the union degrades to
pipeline-only — design how provenance/confidence renders there WITHOUT noise (a repo with
no SCIP coverage must not scream "single witness" on every edge; coverage is a repo/
partition-level fact, per METRIC-LANG-COVERAGE's pattern). The reconciliation must be
a strict generalization: zero-SCIP repos behave exactly as today.

**§5 Serving + aggregates.** How callers/callees serve from the union (replacing the
GREEN/RED binary: what happens to the cert — does it become the witness-agreement
computation itself?); how FC2a-agg computes over the reconciled graph (superseding the
EC-1 §8c interim rule — state the transition); how the divergence RATE surfaces
(doctor/trust — the promotion-funnel pattern); how pipeline-unresolved/SCIP-resolved
sites land as labeled Layer-2 facts without touching the RED-floor denominator.

**§6 Validation design + milestones.** Independently shippable milestones, each
smoke-gateable; the real-TS-corpus validation gap stated honestly (legacy-codebases has
no substantial TS — the deployment-target monorepo run with RMAP_CALLGRAPH_DIFF is the
scale validation; name what it must confirm before which milestone).

**§7 DECISION_REQUIRED list** — every ratification-class choice with alternatives +
trade-offs against the VISION (certainty model, labels-speak-reader's-language,
change-cost doctrine).

## 3. Stop conditions

- NO code/schema changes — this doc is the only modified file.
- The RED floor is untouchable (disposition = pipeline-only forever); the reconciliation
  NEVER silently alters the trust ratio (Layer-2 landing only, labeled).
- Contradictions with ratified decisions (EC-1 §8, W-B epochs, the interim rule) surface
  as DECISION_REQUIRED, never silently reinterpreted.
- Do NOT commit.

## 4. Validation

§3-§7 exist in this doc; every proposal cites the spike's classified evidence or
architecture source (path:line); `git status` shows only this file; the no-second-witness
regime is designed against at least two named legacy-codebases repos' realities (read-only
inspection permitted).
