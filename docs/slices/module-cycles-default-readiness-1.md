# MODULE-CYCLES-DEFAULT-READINESS-1: measure module-cycle compare across a real repo set

Slice ID: MODULE-CYCLES-DEFAULT-READINESS-1
Status: **RATIFIED (2026-06-03). Implementation in progress.** Spec ratified with the B-control
refinement: control B (repo-graph) is the EXPECTED BOUNDARY OUTCOME, not a hard invariant — if repo-graph
has no meaningful TS LiveGraph partition set / producer path, it is a BOUNDARY-CONTROL OBSERVATION (NOT RUN
/ boundary-only), not a migration-readiness sample. Only control A invalidates the run. A MEASUREMENT slice: run the existing
`--engine compare --kind module-import` across a real repo set, collect the divergence histogram, and emit a
`rmap cycles` default-migration READINESS VERDICT (GREEN / YELLOW / RED). NO default flip, NO decommission,
NO deletion, NO new resolver logic, NO code. MEASURE before building IMPORTS-PACKAGE-RESOLUTION-1.
Depends: MODULE-CYCLES-CLI-1 (the compare + sidecar), MODULE-CYCLES-COMPARE-CLASSIFY-1 (the classifier),
SQLITE-RAW-DECOMMISSION-READINESS-4 (the gate this measures). Track: Stage D, audit/measurement.

## Goal
```text
Replace the ASSUMPTION (READINESS-4: "package resolution is likely the dominant gap") with the OBSERVED
divergence distribution. The classifier exists; use it. Produce, per repo, the {matched, missing-by-class,
extra, Unknown} counts + coverage, then a READINESS verdict against the ratified rules. The verdict TELLS
us the next slice (targeted resolver vs labeled-degradation flip vs more measurement) instead of guessing.
```

## Grounding (EXECUTED 2026-06-03)
```text
The compare is live: `rmap cycles --engine compare --kind module-import [--json]` -> SQLite MODULE cycles
  PRIMARY + `livegraph_module_compare` { sqlite_count, livegraph_count, livegraph_class, matched,
  livegraph_subset, missing_in_livegraph:[{cycle,divergence}], extra_in_livegraph:[...] } + a
  `.rgr/livegraph-compare/module-<ts>.json` sidecar. Divergence classes (CLASSIFY-1): MissingDueTo
  {PackageExternal, DynamicImport, StaticUnresolved, UnloadedOrNonTsPartition}, ModuleIdentityMismatch,
  UnexpectedExtraInLiveGraph, UnknownDivergence.
Coverage is NOT in the compare response; gather it from: the refresh aggregate (total/succeeded partitions)
  + `rmap cycles --engine livegraph --kind module-import --json` -> scope.{intra_partition, cross_partition,
  xpart_edge_count}.
Local TS candidates (OBSERVED, tsconfig + package.json): amodx (3 pkg.json), hexmanos (3), zap-engine (2).
  fraktag/glamCRM/zap-squad have NO root tsconfig (less clearly TS). legacy-codebases are non-TS. Viability
  (does scip-typescript index it? node_modules present? any module cycle?) is confirmed AT RUN time.
```

## Spec

### 1. Repo set
```text
A CONTROL  — xpart-monorepo fixture (TS, multi-partition): the EXACT case. Expect matched=1, missing=0,
             extra=0. Confirms the pipeline is sound (a non-empty/non-exact here is a measurement bug).
B BOUNDARY CONTROL — repo-graph itself (RUST, non-TS): the EXPECTED boundary outcome is that the LiveGraph
             has NO TS partitions, so SQLite module cycles classify MissingDueToUnloadedOrNonTsPartition
             (the non-TS boundary is REPORTED, not silently dropped). This is the EXPECTED outcome, NOT a
             hard invariant: if repo-graph has no meaningful TS LiveGraph partition set / producer path, it
             is recorded as a BOUNDARY-CONTROL OBSERVATION (NOT RUN / boundary-only) -- NOT a
             migration-readiness sample, and it does NOT invalidate the run.
C REAL TS  — at least one local TS repo, in priority order: amodx, then hexmanos, then zap-engine. Use the
             FIRST that (a) scip-typescript indexes and (b) has >= 1 SQLite module cycle. If NONE qualify ->
             record NOT RUN for category C explicitly (no inferred result).
```

### 2. Procedure per repo (EXECUTED or NOT RUN, labelled)
```text
1. SQLite baseline: `rmap index <repo>` (register + index). Record snapshot + SQLite module-cycle count.
2. LiveGraph: refresh ALL relevant TS partitions: `rmap dev livegraph-refresh --repo <repo>
   --source-root <pkg-1> ... --source-root <pkg-N>` (the repo's tsconfig'd package roots; for a single-
   package repo, the repo root). Record the aggregate {total, succeeded, failed, degraded}.
3. Compare: `rmap cycles --engine compare --kind module-import --json`. Capture the sidecar.
4. Coverage: `rmap cycles --engine livegraph --kind module-import --json` -> scope (intra/cross/xpart edges).
5. Collect, per repo: matched; missing TOTAL + BY CLASS; extra; UnknownDivergence count;
   UnexpectedExtraInLiveGraph count; coverage {partition_count (repo TS packages), loaded_partition_count
   (refresh succeeded), xpart_edge_count}.
PRODUCER / VIABILITY failures (scip-typescript absent, tsconfig/node_modules broken, index error) -> that
   repo is NOT RUN with the reason; the measurement does NOT fabricate or infer its histogram.
```

### 3. Verdict rules (per the brief)
```text
GREEN  (silent default flip OK) ONLY IF, across the WHOLE run set:
         UnknownDivergence == 0  AND  UnexpectedExtraInLiveGraph == 0  AND  missing == 0.
YELLOW (labeled-degradation default OK) ONLY IF:
         UnknownDivergence == 0  AND  UnexpectedExtraInLiveGraph == 0  AND  every missing cycle is in an
         EXPLAINABLE/degradable class (MissingDueTo{PackageExternal,DynamicImport,StaticUnresolved,
         UnloadedOrNonTsPartition}); ModuleIdentityMismatch counts as RED (the identity rule diverged).
RED    otherwise (any Unknown, any extra, any unexplained/identity divergence).
The verdict is over the run set; a NOT RUN repo does NOT contribute to GREEN (cannot prove absence of
divergence on an unmeasured repo) -- at most the run yields a YELLOW/RED bounded by what WAS measured.
```

### 4. Expected outcome (hypothesis, to confirm or refute)
```text
Likely RED or YELLOW, NOT GREEN (the captured graph is relative+ext/index only; real TS repos use package
imports). IF the dominant missing class is MissingDueToPackageExternal (or path-alias surfacing as
StaticUnresolved) -> the next slice is the TARGETED resolver IMPORTS-PACKAGE-RESOLUTION-1. IF dominated by
DynamicImport or UnloadedOrNonTsPartition -> a different next step. IF any UnexpectedExtra/Unknown -> fix the
derivation/classifier FIRST (a correctness bug, not a completeness gap). The HISTOGRAM decides; do not
pre-commit to package resolution.
```

### 5. Evidence law
```text
Every repo result is labelled EXECUTED (commands run, sidecar captured) or NOT RUN (with the reason). NO
inferred histograms, NO extrapolated counts. The verdict cites only EXECUTED repos. Control A is the HARD
pipeline-soundness check: A must be EXACT (matched=1, missing=0, extra=0); a deviation there INVALIDATES the
run (measurement bug) before any verdict. Control B is a BOUNDARY observation only: its expected outcome is
all-MissingDueToUnloadedOrNonTsPartition, but a non-meaningful B (no TS LiveGraph / no producer path) is
recorded boundary-only/NOT RUN and does NOT invalidate the run.
```

## Out of scope (hard guardrails)
```text
NO `rmap cycles` default flip (this PRODUCES the readiness evidence; a flip is a separate ratified slice).
NO new resolver logic (package/path-alias/dynamic). NO decommission, NO deletion, NO code change beyond the
measurement harness/doc. The compare is DIAGNOSTIC; it changes no default answer.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. a measurement harness (scripts/measure-module-cycle-readiness.sh) driving procedure (2) over the repo
   set, emitting a per-repo {matched/missing-by-class/extra/Unknown/coverage} table + the GREEN/YELLOW/RED
   verdict; control repos A,B assert the soundness invariants.
2. RUN it; record EXECUTED/NOT RUN per repo + the histogram + the verdict in the completion doc.
3. docs: the verdict + the dominant missing class -> the recommended next slice (resolver vs flip-readiness
   vs fix-correctness).
```

## Follow-up (verdict-dependent; NOT chosen here)
```text
- IMPORTS-PACKAGE-RESOLUTION-1   : if MissingDueToPackageExternal/path-alias dominates.
- CYCLES-DEFAULT-MIGRATION-1     : if YELLOW + labeled-degradation is acceptable (a ratified flip).
- a derivation/classifier fix    : if any UnexpectedExtra or Unknown appears (correctness before completeness).
```

## References
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_cycle_compare_response` + the sidecar)
- `rust/crates/repo-graph-livegraph/src/module_cycle_compare.rs` (the classifier + the divergence vocabulary)
- `scripts/compare-module-cycles.sh` (the fixture compare harness this generalizes)
- `docs/slices/sqlite-raw-decommission-readiness-4.md` (the gate + the 6 audit answers this measures)
- `docs/slices/module-cycles-compare-classify-1.md` (the class semantics)
