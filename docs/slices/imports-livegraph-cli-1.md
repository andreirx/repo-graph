# IMPORTS-LIVEGRAPH-CLI-1: explicit LiveGraph import-query surface

Slice ID: IMPORTS-LIVEGRAPH-CLI-1
Status: **RATIFIED (D1=A, D2=C, D3=A, D4=C, D5=invariants, D6=C — 2026-06-06). BUILD IN PROGRESS.** Expose the
captured/classified import graph + import evidence ALREADY BUILT (the six import-classification slices) through an
explicit LiveGraph surface on `rmap imports`. NO default migration (unless separately ratified), NO raw
decommission, NO SQLite deletion, NO workspace package edge, NO new resolver logic. This is a READ-MODEL /
PROJECTION over existing LiveGraph data, plus a daemon response, plus a CLI surface + renderer.
Depends: the import-classification thread (PACKAGE-RESOLUTION-1 / TSCONFIG-PATHS-1 / PACKAGE-EXTERNAL-EVIDENCE-1 /
DYNAMIC-CLASSIFICATION-1 / RELATIVE-RESOLUTION-COMPLETE-1 / ASSET-AND-LITERAL-EXT-1) and the `--engine` flag
pattern on `cycles`. Track: Stage D, QUERY-MIGRATION-1 (the `imports` surface; sequenced before COHERENCE-LAYER-1).

## Why now (priority path)
```text
READINESS-2 ratified the cycle default as DEFERRED (SQLite default, explicit LiveGraph). The import-classification
substrate is COMPLETE + live-validated on amodx but has NO user-facing place to land: `rmap imports` is
SQLite-only and shows none of the classification (benign external/asset, workspace-local-unedgeable, alias/dynamic
resolution, unresolved evidence). Roadmap order puts QUERY-MIGRATION surfaces (imports/stats) BEFORE
COHERENCE-LAYER (orient/check). `imports` is the highest-value remaining QUERY-MIGRATION surface because it is the
direct consumer of the substrate just built. The RED workspace-package edge is NOT reopened (no new evidence).
```

## Grounding (EXECUTED 2026-06-06) — both engines mapped

### Current SQLite `rmap imports`
```text
CLI:      run_imports() @ rust/crates/rgr/src/commands/graph.rs:738. ONE positional <file_path> (required),
          --json flag. Repo from cwd (REG-1). NO --engine flag (SQLite-only).
Daemon:   "imports" -> handle_imports() @ daemon-runtime/src/dispatch.rs:1082. Params {repo, file}.
Storage:  find_imports(snapshot_uid, source_stable_key) @ storage/src/queries.rs:1256. Reads edges
          (type='IMPORTS') JOIN nodes JOIN files. SINGLE-FILE: filtered by source node = the queried file.
Response: ImportsResponse { file, imports: Vec<ImportEntry> } @ rgr/src/presentation/imports.rs:54.
          ImportEntry { node_id, symbol, kind, subtype, file, line, column, edge_type, resolution,
          evidence: Vec<String>, depth }. Human renderer render_human(); JSON = generic to_string_pretty.
Granularity: FILE-level, DIRECT imports (depth hardcoded 1). One file in -> its import edges out.
```

### LiveGraph import data (what exists to project)
```text
RAW OBSERVATION:  ImportObservation @ repo-graph-ir/src/lib.rs:164 { source_file, raw_specifier, resolution:
                  ImportResolution, is_re_export, is_type_only, is_side_effect, external_node_modules }.
RESOLUTION enum:  StaticResolved (the ONLY class that becomes an IMPORTS edge) | StaticUnresolved |
                  PackageExternal | DynamicUnsupported.
EDGE BASIS:       AstImport | AstImportFileInventoryResolved (xpart relative) | AstImportTsconfigPathResolved
                  (alias) | AstDynamicImportResolved (literal dynamic).
RESOLVED EDGES (FILE->FILE), in TWO collections:
  (a) intra-partition StaticResolved -> the partition's EdgeType::Imports edges (node-resolved within partition).
  (b) cross-partition -> LiveGraph.xpart_overlay: Vec<ResolvedImportEdgeCandidate> @ livegraph/src/lib.rs:210
      { src_file_key, dst_file_key, basis, raw_specifier, resolved_repo_path }. Built by rebuild_xpart_overlay
      (relative + tsconfig alias + literal-relative dynamic).
CLASSIFICATION:   module_cycle_live_state() -> LiveCycleState { partitions, observation_classes:
                  ObservationClassSummary } @ livegraph/src/lib.rs:1337. The match on ImportResolution
                  (lib.rs:1392) FOLDS observations into 7 REPO-WIDE BOOLEANS -- it does NOT retain per-observation
                  detail.
SUMMARY (7 flags) @ module_cycle_cert.rs:80:
  BENIGN  (reported, NOT blocking): has_external_nonlocal, has_asset_nonrelevant.
  BLOCKS:  has_workspace_local_unedgeable, has_unresolved_package, has_alias_unresolved, has_dynamic_unresolved,
           has_unresolved_after_overlay. evaluate_module_cycle_completeness blocks iff any BLOCKS flag set.
EXISTING FEED PATTERN: livegraph_feed.rs module_import_cycles_response emits the trust envelope
  { backend_used, kind, scope, answer_class, freshness, missing_partitions, degradation_reasons }.
```

### The two gaps this spec must close (no new resolver logic)
```text
GAP-A (enumeration): the snapshot retains only the 7 SUMMARY BOOLEANS, not WHICH imports are workspace-local /
  unresolved / benign. The acceptance criteria need per-observation EVIDENCE. Close by refactoring the existing
  classification match into a per-observation LABELLER (emit one label per observation), then the booleans become
  a FOLD over the labels -> ONE source of truth, zero rule change, zero divergence risk.
GAP-B (query unit): SQLite find_imports is SINGLE-FILE (requires source_stable_key); the acceptance criteria read
  REPO-WIDE (show ALL amodx evidence). So --engine livegraph may need a DIFFERENT arg contract than --engine
  sqlite on the same command -> forced as D6 (not decided here).
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Surface
```text
                       | mechanism                          | pro                                  | con
A. imports --engine    | mirror cycles' --engine flag on    | consistent w/ cycles; explicit;      | imports' single-file arg vs the
   livegraph [LEAN]    | the existing `imports` command     | discoverable; one import family      | repo-wide evidence need (-> D6)
B. new `import-        | separate top-level command         | clean evidence/query split           | fragments the import family; 2
   evidence`           |                                    |                                      | commands to learn; new surface
C. dev-only diagnostic | hidden/debug subcommand            | no user-facing commitment            | defeats the goal (a USER-FACING
                       |                                    |                                      | landing for the substrate)
RECOMMENDATION: A. Mirror the ratified `cycles --engine livegraph` pattern. Explicit-first (no default flip, D3).
```

### D2 — Scope (what the surface shows)
```text
                       | shows                              | pro                                  | con
A. FILE->FILE edges    | resolved edges only (intra +       | pure Layer-0/1 graph facts           | hides the completeness evidence
   only                | xpart overlay)                     |                                      | (the user explicitly wants it)
B. classified non-edge | externals/assets/workspace-local/  | shows the evidence                   | hides the actual resolved graph
   observations only   | unresolved only                    |                                      |
C. both, SEPARATED     | an EDGES section (facts) + an      | edges = facts, observations =        | needs the per-observation
   [LEAN]              | OBSERVATIONS section (evidence),   | evidence; matches the Fact           | projection (GAP-A) -- a read-model
                       | distinct certainty per section     | Certainty Model; full picture        | refactor (no rule change)
RECOMMENDATION: C. Edges are graph facts (Layer 0-1); observations are completeness evidence. MECHANISM: EDGES =
  (intra-partition StaticResolved IMPORTS edges) UNION (xpart_overlay) -- pure projection of existing collections.
  OBSERVATIONS = the per-observation labeller over the NON-edge classes (GAP-A refactor; reuses the exact existing
  predicates). The two render in SEPARATE sections; an external/asset NEVER appears as an edge (D5).
```

### D3 — Default
```text
                       | pro                                          | con
A. keep SQLite default | consistent w/ READINESS-2 D2=C; no behaviour | LiveGraph imports only via explicit flag
   [LEAN]              | change; explicit opt-in                      |
B. auto LiveGraph w/   | surfaces the new data by default             | changes default behaviour; needs the
   fallback            |                                              | certificate predicate; contradicts the
                       |                                              | just-ratified no-default-flip
RECOMMENDATION: A. Strongly. The user constraint is explicit: "No default migration yet unless separately ratified."
  --engine sqlite remains the default; --engine livegraph is opt-in. A default flip is a SEPARATE future slice.
```

### D4 — Output
```text
                       | pro                                          | con
A. human compact only  | readable                                     | loses the full evidence (the point)
B. JSON full only      | complete                                     | not human-friendly
C. JSON-first FULL     | JSON carries full per-observation evidence + | two renderers to maintain
   evidence + human-   | edges + trust envelope; human = digestible   |
   readable-not-       | (counts per class + edges + compact evidence |
   exhaustive [LEAN]   | summary, NOT every observation)              |
RECOMMENDATION: C. JSON is the evidence-complete contract (every edge + every classified observation + the trust
  envelope). Human renderer shows: edge count + per-class observation counts + the edge list + a compact evidence
  block (e.g. "workspace-local-unedgeable: 3 imports [list]"); it need NOT print every benign external.
```

### D5 — Trust (INVARIANTS, not a choice — the response shape MUST enforce)
```text
1. DISTINGUISH captured edges from observations: separate sections + an explicit per-item class/certainty field.
   Edges carry a `basis` (AstImportFileInventoryResolved / ...Tsconfig... / ...Dynamic...); observations carry a
   `class` (external_nonlocal | asset_nonrelevant | workspace_local_unedgeable | unresolved_package |
   alias_unresolved | dynamic_unresolved | unresolved_after_overlay) + a `blocking: bool`.
2. NEVER present a benign external or asset as a graph edge. external_nonlocal / asset_nonrelevant appear ONLY in
   the observations section, NEVER as a FILE->FILE edge.
3. workspace_local_unedgeable is shown as BLOCKING evidence (blocking=true), with a note it is the RED
   src-vs-dist case (no edge yet; potential missing module-cycle edge).
4. The response carries the trust envelope mirrored from module_import_cycles_response: backend_used="livegraph",
   freshness, missing_partitions, degradation_reasons. The module-cycle completeness is named EXPLICITLY after its
   SOURCE (the module-cycle certificate) -- NOT a generic import-query completeness claim: `module_cycle_
   completeness` + `module_cycle_answer_class` + `module_cycle_import_scope`. The claim is scoped to
   "complete/incomplete for MODULE-CYCLE-RELEVANT captured import evidence", NEVER "the import listing is complete
   for all possible imports". (RATIFIED wording correction, 2026-06-06 -- Fact Certainty Model: do not present a
   narrow certificate as a broad claim.)
```

### D6 — Query unit + grouping (the GAP-B fork — NOT pre-decided)
```text
                       | shape                              | pro                                  | con
A. single-file         | `imports <file> --engine           | symmetric with --engine sqlite;      | does NOT show repo-wide evidence
   (mirror SQLite)      | livegraph` -> that file's edges +  | drilldown; small output              | (workspace-local across the repo);
                       | observations                       |                                      | acceptance criteria read repo-wide
B. repo-wide           | `imports --engine livegraph` (no   | matches the acceptance criteria      | DIVERGES from the SQLite engine's
                       | file arg) -> whole import graph +  | (amodx shows ALL evidence); the      | required-file contract on the SAME
                       | all observations                   | completeness picture                 | command; large output
C. both: optional file | file arg -> that file; no arg ->   | superset; drilldown AND repo-wide;   | more surface to spec/test; the arg
   filter [LEAN]        | repo-wide                           | satisfies acceptance + symmetry      | contract differs by engine (file
                       |                                    |                                      | REQUIRED for sqlite, OPTIONAL for lg)
GROUPING (recommendation, not contested): FILE-level (by source file), with each file's partition/module id as
  METADATA. The cycle certificate is module-level but that is `cycles`; `imports` is FILE->FILE by nature.
RECOMMENDATION: C. An OPTIONAL file filter. No arg + --engine livegraph -> repo-wide (the acceptance path); a
  file arg -> filter to that file (drilldown symmetry). NOTE the honest divergence: --engine sqlite REQUIRES the
  file; --engine livegraph makes it OPTIONAL. If you prefer strict arg symmetry, choose A (single-file) and defer
  repo-wide to a follow-up; acceptance then shifts to a per-file probe.
```

## Acceptance (to verify post-build, EXECUTED)
```text
1. xpart fixture: shows resolved FILE->FILE import edges (the a<->b cycle's edges) under --engine livegraph.
2. amodx --engine livegraph shows, in SEPARATED sections:
   - resolved relative / alias / dynamic FILE->FILE edges (with basis);
   - benign externals + benign assets (observations, blocking=false, NOT edges);
   - workspace-local-unedgeable (observations, blocking=true, RED note);
   - NO generic "unknown package" noise (has_unresolved_package=false after the recent external-evidence fixes).
3. --engine sqlite default UNCHANGED (byte-for-byte same output as today for the same file query).
4. The per-observation labeller and the 7 summary booleans AGREE (the booleans are a fold over the labels) --
   a unit test asserts: OR-fold(labels) == ObservationClassSummary for a fixture set.
5. Trust invariants (D5) hold: no external/asset in the edges section; workspace-local flagged blocking; trust
   envelope present.
Gate: cargo test --workspace; clippy --workspace --all-targets -- -D warnings; cargo fmt --all -- --check.
Live: ./scripts/dev-install-local.sh (or the manual rmapd restart), then the xpart + amodx probes above.
```

## Out of scope (hard guardrails)
```text
NO default migration (D3=A; a flip is a separate ratified slice), NO raw decommission, NO SQLite deletion, NO
workspace package edge (RED until new evidence), NO new resolver logic (this is a PROJECTION over existing
resolved edges + existing classification predicates), NO transitive import closure (depth>1), NO new asset/package
rules, NO module-level cycle re-derivation (that is `cycles`).
```

## Build contract (PROPOSED — gated on D1–D6 ratification)
```text
1. livegraph: extract the per-observation classification (the existing match in module_cycle_live_state) into a
   pure `classify_observation(obs, overlay_resolved) -> ObservationClass` labeller. Re-express
   ObservationClassSummary as a FOLD over the labels (GAP-A; one source of truth; unit test asserts agreement).
2. livegraph: a read-model `live_import_view()` projecting EDGES = intra-partition StaticResolved IMPORTS edges
   UNION xpart_overlay (FILE->FILE + basis + raw_specifier), and OBSERVATIONS = the labelled non-edge entries
   (source_file + raw_specifier + class + blocking). Optional file filter (D6).
3. daemon: an `imports` engine branch (mirror the cycles --engine routing) producing a livegraph response with
   the EDGES section, the OBSERVATIONS section, the per-class counts, and the trust envelope (D5).
4. CLI (rgr): `imports --engine livegraph [<file>]` parse + a presentation renderer (human compact + JSON full,
   D4). --engine sqlite stays the default (D3).
5. live + gate + completion doc.
Stop if: the per-observation labels CANNOT reproduce the summary booleans (would mean a hidden rule) -> surface
  before proceeding. Stop if D6 repo-wide output proves to need a new traversal not already materialised.
```

## References
- `rust/crates/rgr/src/commands/graph.rs:738` (`run_imports` — the CLI arg shape to extend)
- `rust/crates/rgr/src/presentation/imports.rs:54` (`ImportsResponse` — the renderer to mirror)
- `rust/crates/daemon-runtime/src/dispatch.rs:1082` (`handle_imports` — the engine branch point)
- `rust/crates/storage/src/queries.rs:1256` (`find_imports` — the SQLite single-file path, unchanged)
- `rust/crates/repo-graph-livegraph/src/lib.rs:1337` (`module_cycle_live_state` — the classification to refactor)
- `rust/crates/repo-graph-livegraph/src/lib.rs:210` (`xpart_overlay` — the resolved cross-partition edges)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs:80` (`ObservationClassSummary` — the fold target)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_import_cycles_response` — the trust envelope to mirror)
- `docs/slices/imports-asset-and-literal-ext-1.md` (the import-classification completion this surfaces)
- `docs/slices/cycles-default-migration-readiness-2.md` (READINESS-2: SQLite default, explicit LiveGraph — D3 basis)
