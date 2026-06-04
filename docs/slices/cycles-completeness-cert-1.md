# CYCLES-COMPLETENESS-CERT-1: a module-import cycle completeness certificate + evaluator

Slice ID: CYCLES-COMPLETENESS-CERT-1
Status: **IMPLEMENTED (2026-06-04), support-first.** The certificate type + pure evaluator +
`LiveGraph::module_cycle_live_state` snapshot landed (`fbd899b`). Conservative-safe: no baseline provider
exists yet, so it returns `UnknownBaselineMissing` everywhere -> SQLite fallback -> the default is UNCHANGED.
Does NOT unblock the migration alone (the Complete path needs the baseline follow-ups). See **Completion**.
D1–D6 as recommended. SUPPORT + TESTS ONLY
(pure type/evaluator; NO SQLite reads in the evaluator; NO daemon default behaviour change; NO durable cache;
NO migration). Baseline absent -> explicit `UnknownBaselineMissing` (never faked from loaded state). Records
explicitly that this slice does NOT unblock the default migration alone. A SUPPORT slice: a CERTIFICATE TYPE + an
EVALUATOR that decides whether the LiveGraph covers the WHOLE module-import-cycle graph for a repo, WITHOUT
consulting SQLite per query. Goal is a certificate/policy, NOT a query feature. NO default flip, NO SQLite
deletion, NO raw decommission, NO package resolver (unless this spec proves it required — it does not).
Depends: MODULE-AGGREGATION-1 / MODULE-CYCLES-* (the cycle surface), the LiveGraph slot/epoch/language state,
IMPORTS-XPART-ENUMERATION-1 (the expected-partition baseline — a Complete-path prerequisite). Track: Stage D.

## Why (the blocker CYCLES-DEFAULT-MIGRATION-1 deferred onto)
```text
The cycles default cannot go LiveGraph-first because a whole-graph answer cannot be certified COMPLETE from
the LiveGraph's self-reported Exact+Fresh (repo-graph: Exact yet missing a non-TS Rust cycle). The only
current check is compare-vs-SQLite EVERY call (keeps the dependency). This slice builds a CERTIFICATE the
default can consult instead: valid+Complete -> serve LiveGraph (no per-query SQLite); else -> labelled SQLite
fallback.
```

## Grounding (EXECUTED 2026-06-04) — what the evaluator can vs cannot read
```text
LIVE state (all accessible, NO SQLite): per loaded partition -> Slot.{epoch, status(freshness), language};
  resident ir.partition.{build_inputs_hash, indexer, indexer_version} (producer fingerprint);
  import_observations_by_module() (the observation classes); module_import_cycles().scope.
NOT accessible from the LiveGraph (the GAP): the EXPECTED partition set (which partitions SHOULD exist --
  the LiveGraph only knows what is LOADED; F2 no-enumeration) and the repo's LANGUAGE COMPOSITION (does the
  repo have non-TS sources with import/cycle semantics? the LiveGraph is TS-only and blind to it).
=> `Complete` requires PROVING NEGATIVES (no missing partition, no unrepresented non-TS source). Live state
   ALONE can only DETECT some incompleteness; it cannot reach Complete without a recorded BASELINE.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — certificate scope (the brief)
```text
PER (repo) x (language: TS) x (partition set: the loaded set vs the expected set) x (query family:
MODULE-IMPORT CYCLES ONLY). Not file-import, not callers/callees -- a certificate is specific to the
module-import-cycle question (a different completeness shape per family). RECOMMENDED as written.
```

### D2 — the BASELINE source (THE crux)
```text
The evaluator needs a BASELINE to reach Complete: (i) the EXPECTED partition set, (ii) whether the repo has
NON-TS cycle sources. Neither is in the LiveGraph.
A. BUILD the evaluator + a BASELINE INTERFACE now; the baseline is an INPUT supplied by prerequisites --
   (i) from IMPORTS-XPART-ENUMERATION-1 (whole-repo partition discovery), (ii) from a ONE-TIME index-time
   AUDIT (SQLite language composition recorded at registration/index -- "SQLite for audit/training, NOT
   per-query"). Without a baseline the evaluator is CONSERVATIVE (never Complete -> Unknown/Incomplete ->
   SQLite fallback; SAFE).                                                                  [RECOMMENDED]
B. Try to reach Complete from LIVE STATE ALONE.   REJECTED: cannot prove the negatives (repo-graph proves
   a TS-Exact LiveGraph can still miss a non-TS cycle).
RECOMMENDATION: A. This slice ships the TYPE + EVALUATOR + the baseline interface; it correctly returns
Incomplete/Unknown today (the migration stays gated, safely), and the Complete path LIGHTS UP when the
baseline prerequisites land. HONEST: this slice ALONE does not enable the migration -- it builds the
certificate the migration will consume + the conservative-safe evaluator.
```

### D3 — certificate states + evaluator mapping (the brief's 5 states)
```text
CompleteForModuleImportCycles      : baseline present AND loaded partitions == expected set AND ALL loaded
                                     are TS + Fresh AND repo has NO non-TS cycle source AND every import
                                     observation is RESOLVED or REPRESENTED (no bare unrepresented class).
IncompleteMissingPartitions        : expected set (baseline) has partitions NOT loaded (or non-Fresh).
IncompleteUnsupportedLanguage      : the baseline flags a non-TS cycle-source language (or a loaded
                                     partition is non-TS).
IncompleteImportClasses            : an import observation class is unrepresented as a cycle-relevant gap
                                     (e.g. PackageExternal/Dynamic that could close a hidden ring) -- the
                                     captured graph may be missing edges.
UnknownBaselineMissing             : NO baseline supplied -> CANNOT certify (the explicit no-baseline state;
                                     never faked from loaded state). -> SQLite fallback (D6).
Unknown                            : indeterminate (reserved). -> SQLite fallback (D6).
RECOMMENDATION: this exact set (UnknownBaselineMissing explicit, per the ratified constraint) + the
precedence (baseline-missing first; then missing-partitions / unsupported-language structural; import-classes
next; Complete only when all clear).
CONSERVATISM (honest): the import-classes check flags IncompleteImportClasses on ANY uncaptured class
(PackageExternal / Dynamic / StaticUnresolved-not-overlay-resolved). So a TS repo WITH package imports
evaluates IncompleteImportClasses even though its module cycles MAY be exact (the measurement showed amodx
exact) -- the cert cannot know that WITHOUT the compare, so it errs to Incomplete -> SQLite fallback (safe).
Complete therefore needs NOT JUST "TS-only" but "no uncaptured import class" -> on real repos that points back
to IMPORTS-PACKAGE-RESOLUTION-1 (to CAPTURE package imports, lifting the conservative block), even though
package imports were NOT a divergence cause (READINESS-1). Recorded so the gap is explicit, not implicit.
```

### D4 — invalidation (the brief)
```text
The certificate is INVALIDATED by ANY of: a partition REFRESH epoch bump (Slot.epoch / xref epoch); the repo
REGISTRATION/INDEX epoch (a re-index changes the SQLite baseline); a partition source_inputs_hash change; a
producer fingerprint change (indexer/version); a language-support change. KEYED so that a stale "Complete"
can never be served. (build_inputs_hash + indexer/version are on the resident ir.partition; epoch on the
Slot; the baseline's index epoch comes from the audit.) RECOMMENDED.
```

### D5 — storage (the brief)
```text
IN-MEMORY ONLY first: the certificate is computed/cached in the daemon's LiveGraph runtime state, recomputed
on invalidation. NO durable on-disk certificate cache unless SEPARATELY ratified (a durable cache adds a
trust surface -- a persisted "Complete" that could outlive its inputs). RECOMMENDED in-memory only.
```

### D6 — trust behavior (the brief)
```text
Complete certificate    -> the LiveGraph MAY serve the default module cycles (the eventual migration).
Incomplete / Unknown    -> SQLite fallback REQUIRED (never the LiveGraph default).
HARD RULE: NEVER an Exact "no module cycle" answer WITHOUT a Complete certificate (the exact silent-drop the
READINESS verdict forbids). The certificate does NOT itself change the default this slice (the migration is
the separate, gated CYCLES-DEFAULT-MIGRATION-1); it provides the predicate that gate needs.
```

## Acceptance (EXECUTED later)
```text
1. a `ModuleCycleCompletenessCertificate` type (the 5 states) + a pure EVALUATOR over (live LiveGraph state,
   baseline) with unit tests for EACH state, incl. precedence (missing-partition vs unsupported-language vs
   import-classes vs Complete vs Unknown) and Unknown-without-baseline.
2. invalidation: a state-changing event (refresh epoch, re-index epoch, hash/fingerprint, language) busts a
   cached certificate (unit-tested).
3. CONSERVATIVE today: with NO enumeration/audit baseline, the evaluator returns Unknown/Incomplete for ALL
   repos -> SQLite fallback everywhere -> the default is UNCHANGED + safe (no silent drop).
4. (when the baseline prerequisites exist) the measured TS-only repos (amodx/hexmanos/zap-engine) evaluate
   CompleteForModuleImportCycles; repo-graph evaluates IncompleteUnsupportedLanguage (the non-TS Rust cycle).
5. full gate (workspace test, clippy -D warnings, fmt); `--engine livegraph|compare`, the readiness harness,
   and the SQLite default all unchanged.
```

## Out of scope (hard guardrails)
```text
No default flip (CYCLES-DEFAULT-MIGRATION-1, gated on a Complete certificate). No SQLite deletion, no raw
decommission, no package resolver. No per-query SQLite (the whole point: certify WITHOUT it; SQLite is
audit/training-time only). No durable certificate cache (D5). The baseline SOURCES (enumeration + the
index-time audit) are PREREQUISITES, specced/built separately.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. repo-graph-livegraph (or a support module): the certificate TYPE + the pure EVALUATOR(live snapshot,
   baseline) -> state, with the D3 precedence + D4 invalidation keying; a `BaselineInput` interface (expected
   partition set + non-TS-source flag + index epoch). Unit tests for every state (synthetic baselines).
2. daemon wiring (read-only): compute the certificate from the repo's LiveGraph + the (currently ABSENT)
   baseline; expose it for diagnostics (e.g. surface in the compare/JSON). It returns Unknown/Incomplete
   until the baseline exists -> no behaviour change to the default.
3. docs: completion + the explicit statement that the Complete path is gated on the baseline prerequisites.
```

## Completion (implemented 2026-06-04, EXECUTED)

Commits: `98850a8` (spec) + `fbd899b` (impl).

### What landed
```text
repo-graph-livegraph/module_cycle_cert.rs (new, pure -- no SQLite/IO):
  - ModuleCycleCompleteness (D3, 6 states incl. explicit UnknownBaselineMissing) + as_str() +
    permits_livegraph_default() (D6: Complete-only).
  - LiveCycleState / LivePartition / ObservationClassSummary (the pure live snapshot) + BaselineInput (D2
    interface: expected partition ids + non-TS evidence + repo index epoch + language-support version).
  - evaluate_module_cycle_completeness(live, baseline?) -> state (precedence: baseline-missing ->
    missing-partitions -> unsupported-language -> import-classes -> Complete; NEVER Complete without baseline).
  - certificate_inputs_fingerprint (D4: all invalidation keys -> any change busts a cached certificate).
repo-graph-livegraph/lib.rs: LiveGraph::module_cycle_live_state() -- read-only snapshot; a partition is
  fresh ONLY if resident; has_unresolved_after_overlay EXCLUDES overlay-resolved StaticUnresolved (captured).
```

### Constraints honored
```text
SUPPORT + TESTS ONLY: no daemon wiring, no default behaviour change, no durable cache, no SQLite read in the
evaluator, no migration. Baseline absent -> UnknownBaselineMissing (never faked). CONSERVATIVE by design.
```

### Acceptance outcomes
```text
1. type + pure evaluator + unit tests per state (incl. precedence + Unknown-without-baseline)  PASS (7 tests).
2. invalidation: epoch / source_inputs_hash / producer fingerprint / repo index epoch / language version each
   busts the inputs fingerprint                                                                  PASS.
3. CONSERVATIVE today (no baseline provider exists) -> the evaluator returns UnknownBaselineMissing for every
   real repo -> SQLite fallback -> default UNCHANGED + safe                                       PASS (by construction; UnknownBaselineMissing test).
4. (when the baseline exists) a clean TS LiveGraph + matching baseline -> Complete; a non-resident/non-TS/
   uncaptured-import case -> the matching Incomplete state                                        PASS (livegraph snapshot->evaluate test).
5. full gate                                                                                      PASS (see evidence).
```

### Validation evidence
```text
EXECUTED: cargo test -p repo-graph-livegraph (23 module_cycle tests: compare + 7 cert + the snapshot->evaluate);
  cargo test --workspace (220 binaries ok, 0 failures); clippy --workspace --all-targets -- -D warnings
  (clean); cargo fmt --all -- --check (clean). NO live validation (support+tests only; no daemon change).
```

### Does NOT unblock the migration (explicit)
```text
This slice builds the certificate MECHANISM only. With NO baseline provider today the evaluator returns
UnknownBaselineMissing everywhere -> SQLite fallback -> the `rmap cycles` default is UNCHANGED. The Complete
path (and the migration it gates) lights up ONLY with the two follow-up baseline providers below.
```

## Follow-up (the baseline prerequisites + the unblocked migration)
```text
- IMPORTS-XPART-ENUMERATION-1                : whole-repo partition discovery (the expected-partition baseline).
- CYCLES-COMPLETENESS-AUDIT-1                : the one-time index-time non-TS / language-composition audit
                                              (SQLite for audit/training -> recorded baseline).
- CYCLES-DEFAULT-MIGRATION-1 (un-deferred)   : once a Complete certificate is reachable, the LiveGraph-first
                                              default WITHOUT compare-every-call.
```

## References
- `rust/crates/repo-graph-livegraph/src/lib.rs` (Slot epoch/status/language; ir.partition fingerprint; observations; module_import_cycles scope)
- `docs/slices/cycles-default-migration-1.md` (the DEFERRED migration this unblocks; the P1/P2/P3 analysis)
- `docs/slices/module-cycles-default-readiness-1.md` (the YELLOW measurement; repo-graph non-TS evidence)
- `docs/slices/imports-xpart-enumeration-1.md` (F2 — the expected-partition baseline prerequisite)
