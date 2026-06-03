# MODULE-CYCLES-COMPARE-CLASSIFY-1: evidence-backed classification of missing module-cycle divergences

Slice ID: MODULE-CYCLES-COMPARE-CLASSIFY-1
Status: **RATIFIED (2026-06-03). Implementation in progress.** Ratified: D1 pure support module; **D2=A
LiveGraph-evidence-only** (no SQLite edge reads); D3 refined vocabulary; D4 conservative precedence
(Unknown over a guess); D5 `import_observations_by_module()` + `ObservationView`. NO default flip /
decommission / deletion. Replace the fixed `UnknownDivergence`
that MODULE-CYCLES-CLI-1 (D4=A) assigns to every MISSING SQLite module cycle with EVIDENCE-BACKED cause
classes WHERE the LiveGraph evidence explains the gap. Classification only — NO default migration, NO raw
decommission, NO deletion, NO default flip.
Depends: MODULE-CYCLES-CLI-1 (the compare + sidecar), MODULE-AGGREGATION-1 (`module_cycle_compare` +
`module_import_cycles`), `repo-graph-ir` (`ImportObservation` / `ImportResolution`). Track: Stage D.

## Goal
```text
For each SQLite module cycle the LiveGraph LACKS (the `missing_in_livegraph` entries), assign a cause class
backed by LiveGraph EVIDENCE (the cycle's modules' non-captured import observations + residency + module
identity), instead of a blanket UnknownDivergence. This turns the compare sidecar into an actionable map of
WHY the captured FILE graph misses each module ring — the evidence a future `rmap cycles` default-migration
decision needs (Unknown=0 + no extras). This slice ONLY classifies; it does not migrate or decide.
```

## Grounding (EXECUTED 2026-06-03)
```text
Compare sidecar (MODULE-CYCLES-CLI-1): ModuleCycleCompareReport { sqlite_count, livegraph_count,
  livegraph_class, matched, livegraph_subset, missing_in_livegraph: [{cycle:[module paths], divergence}],
  extra_in_livegraph: [...] }. `cycle` is a canonical SET of module paths (not the ring ORDER, not the edges).
ImportResolution (repo-graph-ir) = StaticResolved | StaticUnresolved | PackageExternal | DynamicUnsupported.
  (NO "DynamicObserved" — dynamic is DynamicUnsupported.) Only StaticResolved becomes a captured FILE edge;
  the other three are OBSERVATIONS that explain a NON-captured import.
ImportObservation = { source_file (repo-relative importing file -> module = dirname), raw_specifier,
  resolution, is_re_export, is_type_only, is_side_effect }. Held in each RESIDENT partition's IR; the
  LiveGraph has NO public observations accessor yet (D5 adds one).
KEY (stop-condition) finding: the cause of a missing cycle is classifiable from LiveGraph EVIDENCE ALONE
  (the cycle's modules' non-captured observations + residency + the compare qualified names) WITHOUT reading
  SQLite's MODULE->MODULE edges. So no SQLite internals beyond the compare-only qualified names are needed
  -> the stop condition is NOT triggered. EXACT-edge pinpointing (which specific module edge the LiveGraph
  lacks) WOULD need the SQLite module edges; that is the REJECTED alternative in D2 (presented, not taken).
```

## Ratified decisions (2026-06-03) — every cell filled

### D1 — classifier location (brief-dictated)
```text
A PURE support module (no daemon/SQLite/IO dep) in repo-graph-livegraph, alongside `module_cycle_compare`.
Input: ONE missing module cycle (module-path set) + the LiveGraph import OBSERVATIONS grouped by module +
the LiveGraph resident module set + residency/language per module. Output: a `ModuleCycleDivergence`. The
daemon compare GATHERS the inputs from the LiveGraph and calls the pure classifier (replacing the fixed
UnknownDivergence). RECOMMENDED (matches the brief).
```

### D2 — classification APPROACH (the load-bearing decision; the stop-condition matrix)
```text
A. LIVEGRAPH-EVIDENCE-ONLY (per-cycle-module observation profile): for a missing cycle's module SET,
   examine the non-captured observations of those modules' resident files + their residency + identity. NO
   SQLite module edges read (only the compare-only qualified names). Evidence-backed but PER-MODULE-PROFILE,
   not exact-edge: it explains "this ring's modules have a <class> non-captured import that accounts for the
   gap", not "edge M1->M2 is missing because of import X".                          [RECOMMENDED]
B. SQLITE-EDGE-EXACT: also read SQLite's MODULE->MODULE IMPORTS edges to pinpoint the EXACT missing edge,
   then find the precise importing file + observation. More precise; but reads SQLite internals BEYOND the
   compare-only qualified names — the slice's STOP condition.                        [REJECTED — stop matrix]
MATRIX:
  precision        A: per-module profile (good enough to class the cause)   B: exact missing edge
  SQLite internals A: none (qualified names only)                           B: MODULE IMPORTS edges (more)
  honesty          A: classes the cause WHERE evidence is unambiguous, else Unknown    B: can attribute exactly
  cost / risk      A: pure, contained, testable                            B: couples classifier to SQLite
RECOMMENDATION: A. It satisfies the brief ("where possible", "do not infer without evidence") with NO extra
SQLite coupling. B is a larger, SQLite-coupled effort; defer it unless exact-edge attribution is required
(then it is its own ratified decision). << This is the matrix the stop condition asked for. >>
```

### D3 — the divergence VOCABULARY (refine the MODULE-AGGREGATION-1 enum)
```text
The current `ModuleCycleDivergence` (module_cycle_compare.rs) is coarse:
  MissingInLiveGraphDueToPackageOrDynamicImport | MissingInLiveGraphDueToUnresolvedImport |
  ModuleIdentityMismatch | UnexpectedExtraInLiveGraph | UnknownDivergence.
REFINE the MISSING causes to the brief's set (keep extra/identity/unknown):
  MissingDueToPackageExternal          (a cycle module imports a PACKAGE name; PackageExternal observation)
  MissingDueToDynamicImport            (a cycle module has a DynamicUnsupported observation)
  MissingDueToStaticUnresolved         (a relative import whose target resolves to ANOTHER cycle module but
                                        is not captured — StaticUnresolved with the cross-module target)
  MissingDueToUnloadedOrNonTsPartition (a cycle module's files are NON-resident or non-TS -> not analyzable)
  ModuleIdentityMismatch               (a SQLite cycle module has NO LiveGraph counterpart, but a near
                                        variant exists -> the dirname identity diverged on this repo)
  UnexpectedExtraInLiveGraph           (extra LiveGraph cycle -> overclaim; unchanged)
  UnknownDivergence                    (evidence does not unambiguously explain it -> the honest default)
RECOMMENDED. Splits Package vs Dynamic; renames Unresolved -> StaticUnresolved; adds the non-resident class.
TRADE-OFF: a public enum change; the MODULE-CYCLES-CLI-1 compare sidecar's `divergence` strings change for
missing entries (the FIXTURE stays EMPTY so its sidecar is unaffected; real-repo sidecars gain finer strings).
```

### D4 — mapping + PRECEDENCE + the honesty bar
```text
For a missing cycle (module set C):
 1. RESIDENCY first: if ANY module in C has no resident/TS files -> MissingDueToUnloadedOrNonTsPartition
    (the ring cannot even be analyzed; most fundamental).
 2. IDENTITY: if a module in C has no LiveGraph module counterpart but a near-variant exists (e.g. a parent/
    child dir, or a path differing only by normalization) -> ModuleIdentityMismatch.
 3. CONFIRMED static-unresolved: if a resident file in some Mi has a StaticUnresolved relative import whose
    NORMALIZED target path lands in ANOTHER cycle module Mj -> MissingDueToStaticUnresolved (exact evidence:
    the bridging import is identified).
 4. HEURISTIC package/dynamic: else, if the cycle's modules have PackageExternal observations ->
    MissingDueToPackageExternal; DynamicUnsupported -> MissingDueToDynamicImport. The target module is NOT
    confirmable (a package name / dynamic has no repo path without package resolution), so this is assigned
    ONLY when it is the SOLE non-captured class across C (one dominant cause); MIXED package+dynamic with no
    confirmed bridge -> UnknownDivergence (do not guess).
 5. else UnknownDivergence.
HONESTY BAR (the brief's "do not infer without evidence"): a CONFIRMED class (1,2,3) is assigned on direct
evidence; a HEURISTIC class (4) only when unambiguous; ALL ambiguity -> UnknownDivergence. Favor Unknown over
a wrong attribution. RECOMMENDED.
```

### D5 — observation access (LiveGraph accessor)
```text
Add a read accessor `LiveGraph::import_observations_by_module() -> BTreeMap<String, Vec<ObservationView>>`
(module path -> the resident observations whose source_file dirname is that module; `ObservationView` = a
small owned view { raw_specifier, resolution, is_re_export, is_type_only } so the pure classifier needs no IR
types). Plus residency/TS per module from the existing slot data. The daemon compare passes these to the pure
classifier. RECOMMENDED. (The classifier stays IO-free + unit-testable on synthetic inputs.)
```

## Validation (EXECUTED later)
```text
1. synthetic UNIT tests for EACH cause (pure classifier): a package-only ring -> PackageExternal; a
   dynamic-only ring -> DynamicImport; a confirmed cross-module StaticUnresolved -> StaticUnresolved; a
   non-resident module -> UnloadedOrNonTsPartition; a near-variant identity -> ModuleIdentityMismatch; mixed/
   no evidence -> UnknownDivergence; an extra LiveGraph cycle stays UnexpectedExtraInLiveGraph.
2. fixture (xpart-monorepo): the compare stays EXACT (missing=[] -> the classifier is never invoked; the
   sidecar is byte-empty of divergences). compare-module-cycles.sh still PASS.
3. real repo with module cycles: the compare sidecar's missing entries get finer classes; record the class
   histogram. Unknown may remain. DO NOT claim default-migration readiness until Unknown=0 AND no extras.
4. full gate (workspace test, clippy -D warnings, fmt); default `rmap cycles` byte-unchanged.
```

## Out of scope (hard guardrails)
```text
NO `rmap cycles` default flip. NO raw decommission. NO deletion. NO package/path-alias/dynamic RESOLUTION
(this classifies the GAP; closing it is IMPORTS-PACKAGE-RESOLUTION-1). NO SQLite MODULE-edge reading (D2=A;
exact-edge attribution is the rejected B). NO exact-cause guess without evidence (D4 honesty bar).
```

## Build contract (PROPOSED — gated on ratification)
```text
1. repo-graph-livegraph: refine ModuleCycleDivergence (D3); a PURE classifier
   `classify_missing_module_cycle(cycle, observations_by_module, resident_modules, ...) -> ModuleCycleDivergence`
   (D2=A, D4 precedence); `import_observations_by_module` accessor + ObservationView (D5). Unit tests per cause.
2. daemon (livegraph_feed module_cycle_compare_response): replace the fixed `UnknownDivergence` for missing
   entries with the classifier output; extra entries unchanged. Sidecar gains the finer strings.
3. validation: synthetic unit tests; compare-module-cycles.sh fixture stays exact; document the real-repo
   procedure + the Unknown=0/no-extra default-migration gate.
4. docs: completion + (if a real repo is staged) the class histogram.
```

## Follow-up slices
```text
- IMPORTS-PACKAGE-RESOLUTION-1 : resolve package names / tsconfig path aliases -> closes the
  MissingDueToPackageExternal gap (the dominant real-repo cause), shrinking Unknown toward 0.
- MODULE-CYCLES-COMPARE-EXACT-1 : (only if needed) D2=B exact-edge attribution via SQLite MODULE edges.
- CYCLES-DEFAULT-MIGRATION-1 : flip `rmap cycles` default — gated on Unknown=0 + no extras (this slice's evidence).
```

## References
- `rust/crates/repo-graph-livegraph/src/module_cycle_compare.rs` (the divergence enum + compare to refine)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`module_import_cycles`; the slots' `import_observations`)
- `rust/crates/repo-graph-ir/src/lib.rs` (`ImportObservation` / `ImportResolution` — the evidence)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_cycle_compare_response` — the consumer)
- `docs/slices/module-cycles-cli-1.md` (the compare + sidecar this enriches)
