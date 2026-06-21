# CYCLES-COMPLETENESS-AUDIT-1: produce the BaselineInput for the module-cycle certificate

Slice ID: CYCLES-COMPLETENESS-AUDIT-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-04), read-only (D1–D4 + B ratified).** The baseline PROVIDER
for CYCLES-COMPLETENESS-CERT-1 (`40225d9`): a builder + a READ-ONLY `rmap dev cycle-completeness-audit`
diagnostic. xpart fixture -> `CompleteForModuleImportCycles` (live); repo-graph + the three real repos ->
honest `IncompleteMissingPartitions` with the non-TS evidence carried in the report (live). Still NOT a
default migration. See **Completion**. NO default flip, NO deletion, NO raw decommission, NO durable
certificate cache.
Depends: CYCLES-COMPLETENESS-CERT-1 (the `BaselineInput` type + the pure evaluator), the SQLite index
(language inventory + module cycles), the readiness harness (the repo set). Track: Stage D, audit/provider.

## Goal
```text
Generate the `BaselineInput` for the module-import-cycle certificate from CURRENT-TRUTH sources (SQLite
language inventory + TS partition discovery) at an AUDIT BOUNDARY, so the runtime evaluator (which stays
SQLite-free) can reach Complete/Incomplete{Language,...} instead of UnknownBaselineMissing. This is the
missing input -- not the migration (still gated on the evaluator + the daemon consuming the baseline).
```

## Grounding (EXECUTED 2026-06-04)
```text
SQLite has a `language` column (storage/types.rs:368) -> the language INVENTORY (which files/nodes are
non-TS) is queryable at audit time. SQLite find_cycles("module") gives the MODULE cycle baseline (and which
cycles are non-TS, by their modules' file languages). TS partition roots are discoverable on the FILESYSTEM
(tsconfig dirs -- the readiness harness already does `find tsconfig.json`). BaselineInput (CERT-1) currently
has 4 fields; the brief adds a 5th (import_completeness_policy_version) -> a small CERT-1 amendment.
```

## THE BOUNDARY (the stop condition, presented as a matrix — NOT triggered)
```text
                         | AUDIT (baseline generation)        | RUNTIME (certificate evaluator)
SQLite access            | MAY read SQLite (language inv. +    | MUST be SQLite-FREE (reads the cached
                         | module-cycle baseline) AT BOUNDARY  | BaselineInput only)
when it runs             | index/refresh boundary OR a dev    | per query
                         | diagnostic command (NOT per query) |
output                   | a BaselineInput value (+ a report) | a ModuleCycleCompleteness state
coupling                 | audit -> SQLite (allowed)          | evaluator -> SQLite = FORBIDDEN (the stop)
=> The audit reads SQLite at the boundary and PRODUCES a plain BaselineInput; the runtime evaluator consumes
   that value and NEVER touches SQLite. The stop condition (runtime coupled to SQLite) is NOT triggered. IF a
   ratified design instead made the evaluator call SQLite per query, THAT would be the stop -- this spec
   forbids it.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — baseline SOURCE
```text
A. existing SQLite / indexer metadata (language inventory + module-cycle baseline).        [RECOMMENDED for
                                                                                            non-TS evidence]
B. filesystem / project discovery (tsconfig dirs).                                          [for partitions]
C. explicit config / manifest.
RECOMMENDATION: A for CURRENT TRUTH (the SQLite language inventory is the authoritative "what languages does
this repo have"), ISOLATED as an AUDIT INPUT (read at the boundary, never the runtime). B (filesystem
tsconfig discovery) supplies the EXPECTED TS PARTITION SET (the same discovery the readiness harness uses; it
also overlaps the F2 enumeration the migration needs to LOAD them). C deferred (no manifest format yet). So:
A (non-TS evidence) + B (expected partitions), both audit-time. The runtime stays SQLite-free (the boundary).
```

### D2 — what the BaselineInput must contain (the brief; amends CERT-1)
```text
expected_partition_ids          (B: filesystem tsconfig roots, repo-relative)
has_non_ts_cycle_source         (A: SQLite -> any non-TS FILE/module with import/cycle semantics)
repo_index_epoch                (A: the SQLite snapshot/index epoch)
language_support_version         (the daemon language-support policy version)
import_completeness_policy_version  (NEW -- the captured-graph import-class policy version; AMEND CERT-1's
                                  BaselineInput + its inputs fingerprint to carry it, for invalidation)
RECOMMENDATION: as listed; the 5th field is a small CERT-1 amendment (additive).
```

### D3 — how to detect UNSUPPORTED-LANGUAGE cycles
```text
A. SQLite file LANGUAGE inventory: any FILE node with language != TypeScript (with import/cycle relevance)
   -> has_non_ts_cycle_source = true. SIMPLE + conservative.                                  [RECOMMENDED]
B. SQLite MODULE-cycle table: find_cycles("module") -> any cycle whose modules' files are non-TS -> a
   concrete non-TS module cycle (corroboration / precision).                                   [corroboration]
C. an extractor/language coverage map (which languages the LiveGraph CAN represent).            [policy input]
RECOMMENDATION: A as the primary signal (any non-TS source -> not certifiable), B to CORROBORATE/EXPLAIN
(name the actual non-TS module cycles, e.g. repo-graph's Rust ring), C as the policy that defines "supported"
(TS today). Conservative: presence of ANY non-TS import/cycle source -> has_non_ts_cycle_source.
```

### D4 — runtime use (the brief)
```text
The baseline is generated at the AUDIT BOUNDARY (index/refresh OR a dev diagnostic command), NOT per query;
the runtime certificate evaluator consumes a CACHED IN-MEMORY BaselineInput (CERT-1 D5: in-memory only).
This slice's SCOPE (the brief's "may only write a harness/report if durable storage is too large"): the
BASELINE BUILDER (a function: SQLite handle + repo path -> BaselineInput) + a DEV DIAGNOSTIC harness/command
that runs the builder + the evaluator over the repo set and REPORTS the certificate per repo. The runtime
per-query CACHING + the migration's consumption are FOLLOW-UPS (no default change here). [RECOMMENDED]
```

## Validation (EXECUTED later)
```text
- xpart-monorepo fixture: builder -> baseline {expected=[packages/a,packages/b], non_ts=false}; the LiveGraph
  (both loaded, overlay-resolved) -> evaluator returns CompleteForModuleImportCycles.
- repo-graph: builder -> baseline includes non-TS evidence (the Rust files/module cycle); evaluator returns
  IncompleteUnsupportedLanguage (corroborated by the SQLite non-TS module cycle).
- amodx / hexmanos / zap-engine: builder -> baseline (expected TS partitions, non_ts=false IF TS-only);
  evaluator returns Complete IF the LiveGraph is exact + fully loaded + no uncaptured import class -- ELSE the
  evidence-labelled Incomplete state (e.g. IncompleteImportClasses if package imports present; recorded, not
  hidden). (READINESS-1 showed cycles exact but those repos have package imports -> likely
  IncompleteImportClasses under the conservative cert; that is the HONEST outcome.)
- full gate; the evaluator + default `rmap cycles` behaviour UNCHANGED (this slice only PROVIDES the baseline).
```

## Out of scope (hard guardrails)
```text
NO default flip, NO deletion, NO raw decommission, NO durable certificate cache (in-memory only unless
separately ratified). NO runtime SQLite in the evaluator (the boundary). NO package resolver. The migration
(consume the baseline to serve LiveGraph) stays the separate, gated CYCLES-DEFAULT-MIGRATION-1.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. CERT-1 amendment: BaselineInput gains import_completeness_policy_version (+ its inputs fingerprint).
2. a baseline BUILDER (daemon-runtime, audit-boundary): (SQLite storage + repo path) -> BaselineInput --
   filesystem tsconfig discovery (expected partitions) + a SQLite language-inventory query (non-TS evidence)
   + the snapshot/index epoch + the policy versions. SQLite read is AUDIT-time only.
3. a dev diagnostic command/harness (e.g. `rmap dev cycle-completeness --repo X` or a script) that runs the
   builder + LiveGraph::module_cycle_live_state + evaluate_module_cycle_completeness -> a per-repo report
   (baseline + certificate state). Run it over the repo set; record EXECUTED results.
4. docs: completion + the per-repo certificate table; explicit "still not a migration".
```

## Completion (implemented + live-validated 2026-06-04, EXECUTED)

Commits: `44fbbd8` (spec) + `40225d9` (impl). Ratified D1–D4 + the CERT-1 amendment + **B (read-only audit,
accept honest precedence)**.

### What landed
```text
CERT-1 amendment: BaselineInput gains import_completeness_policy_version (+ inputs fingerprint).
storage/queries.rs: distinct_file_languages(repo_uid) -- the D3-A language inventory (closed code-only vocab).
daemon-runtime/cycle_completeness_audit.rs (new, the audit boundary):
  - classify_non_ts_languages (D3-A, A chosen): non-null language outside {typescript,tsx,javascript,jsx} ->
    has_non_ts_cycle_source. CONSERVATIVE; no false Complete.
  - discover_tsconfig_dirs (D1): bounded std::fs walk -> expected TS partition roots (no new dependency).
  - index_epoch_from_snapshot: deterministic FNV-1a -> repo_index_epoch (re-index busts the certificate).
  - cycle_completeness_audit_response: READ-ONLY -- discover + read SQLite inventory + snapshot the CURRENT
    LiveGraph + evaluate + report (observed/non-TS languages, loaded + MISSING expected partitions, observation
    classes, D3-B SQLite-vs-LiveGraph module-cycle corroboration). Never refreshes/loads/mutates.
dispatch.rs: handle_cycle_completeness_audit (read-only) + route. rgr: `rmap dev cycle-completeness-audit --repo`.
```

### Boundary (the stop condition) — held
```text
The audit reads SQLite (distinct_file_languages, find_cycles) + the filesystem (tsconfig discovery) AT THE
DIAGNOSTIC BOUNDARY. The certificate evaluator stays SQLite-free (consumes BaselineInput + the in-memory
snapshot). The command is READ-ONLY: it never refreshes/loads/mutates the LiveGraph. NOT triggered.
```

### Live validation (EXECUTED 2026-06-04, dev-install-local.sh; `rmap dev cycle-completeness-audit`)
```text
REPO          HEADLINE                       expected/loaded/missing  observed langs                non_ts        non_ts_corrob  sqlite/lg cycles
xpart fixture CompleteForModuleImportCycles  2 / 2 / 0                [typescript]                  []            false          1 / 1
              (permits_livegraph_default=TRUE; both packages loaded, overlay-resolved, no package/dynamic)
repo-graph    IncompleteMissingPartitions    10 / 0 / 10             [c,cpp,java,javascript,jsx,    [c,cpp,java,  TRUE           6 / 0
                                                                      python,rust,tsx,typescript]    python,rust]
amodx         IncompleteMissingPartitions    8 / 0 / 8              [javascript,tsx,typescript]    []            (n/a)          3 / 0
hexmanos      IncompleteMissingPartitions    3 / 0 / 3             [java,javascript,tsx,typescript] [java]        TRUE           1 / 0
zap-engine    IncompleteMissingPartitions    4 / 0 / 4             [python,rust,tsx,typescript]    [python,rust] TRUE           1 / 0
```

### Acceptance outcomes (vs the ratified B rules)
```text
1. xpart fixture -> Complete                                                                    PASS (live).
2. repo-graph -> an incomplete headline WITH non-TS evidence + missing partitions per precedence PASS (live:
   IncompleteMissingPartitions; non_ts=[c,cpp,java,python,rust]; has_non_ts=true; corroborated; 10 missing).
3. real TS repos: record ACTUAL state, do not coerce                                             PASS (live: all
   three IncompleteMissingPartitions read-only; languages recorded; NOT loaded/coerced).
4. report surfaces observed languages + has_non_ts + non_ts_corroborated + missing partitions    PASS.
5. evaluator SQLite-free; no default change; no durable cache                                     PASS.
6. IncompleteUnsupportedLanguage / IncompleteImportClasses / UnknownBaselineMissing / precedence  PASS (unit:
   the 7 cert tests + 6 audit tests; the live read-only path cannot isolate these on unloaded real repos).
Gate: workspace tests ok / 0 failures; clippy -D warnings clean; fmt clean.
```

### Honest divergence from the spec's prediction (recorded)
```text
The spec PREDICTED real TS repos would certify `IncompleteImportClasses`. The ACTUAL live read-only state is
`IncompleteMissingPartitions` for ALL of them -- because B is read-only and their LiveGraphs had NO partitions
loaded (loaded=0), so the EARLIEST blocking state (missing partitions) wins by precedence. Reaching the
import-class axis live would require LOADING every discovered partition first, which the ratified rules forbid
for real repos ("record actual state; do not coerce"; "do not self-refresh"). The import-class axis is proven
by the cert UNIT tests instead. This is the correct B behaviour, not a defect: precedence is part of the
certificate contract, and a diagnostic must not mutate the state it certifies.

Over-discovery is REAL + surfaced (not hidden): repo-graph discovered 10 tsconfig dirs incl. the root
`default` + nested TEST FIXTURES (`test/fixtures/typescript/*`, the in-repo xpart packages) + `tools/rgistr`.
All 10 are listed in `missing_expected_partitions`. The narrower "project-root" discovery heuristic was
explicitly rejected (scoped-discovery hack); the conservative over-discovery -> MissingPartitions is honest.
```

### Does NOT unblock the migration alone (explicit)
```text
The certificate can now reach `Complete` (proven live on the fixture) -- but only for a fully-loaded, pure-TS,
package-import-free repo. The default `rmap cycles` is UNCHANGED (this slice adds only a read-only dev
diagnostic). The migration additionally needs: the daemon RUNTIME wiring (cache the BaselineInput + a query
consumes it) and, for real repos to reach `Complete`, the import-class policy refinement
(IMPORTS-PACKAGE-RESOLUTION-1) so resolved package imports stop forcing `IncompleteImportClasses`.
```

## Follow-up
```text
- CYCLES-COMPLETENESS-LANGUAGE-PRECISION-1 (recorded; ratified A over A' here): OPTIONALLY narrow the non-TS
  evidence to IMPORT-BEARING non-TS files (join edges->source file->language) instead of the whole file
  inventory, IF the broad inventory rule proves too conservative (blocks repos whose non-TS code has no
  imports). Not needed until evidence shows A over-blocks; the live runs did not surface that.
- the daemon RUNTIME wiring: cache the BaselineInput per repo (in-memory), invalidate by the D2 keys, and
  let a query read it -- the prerequisite for the migration's serve-path.
- CYCLES-DEFAULT-MIGRATION-1 (un-deferred): consume a Complete certificate to serve LiveGraph without compare.
- IMPORTS-PACKAGE-RESOLUTION-1: the import-class refinement so a RESOLVED package import stops forcing
  IncompleteImportClasses -- the lever that lets real (package-importing) repos reach Complete.
```

## References
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (BaselineInput + the evaluator this feeds)
- `rust/crates/storage/src/queries.rs` + `types.rs:368` (the `language` inventory; `find_cycles("module")`)
- `docs/slices/cycles-completeness-cert-1.md` (the certificate this provides the baseline for)
- `docs/slices/module-cycles-default-readiness-1.md` (the repo set + the exact/package-import evidence)
- `scripts/measure-module-cycle-readiness.sh` (the harness this extends)
