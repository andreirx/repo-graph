# Root causes — audit round five (v0.17.0) — 2026-09-06

Five read-only investigations over the audit's defects (render site → data path → exact predicate →
regression-or-never-worked → smallest fix → verification). Full reports follow the synthesis. Basis: code
reading + `git log -S`/blame + ground-truth greps; no rmap/cargo runs. "UNDETERMINED" is stated where it applies.

## Synthesis — defects grouped by SHARED root cause

| Group | Root cause (one sentence) | Defects | Regression? | Fix layer |
|---|---|---|---|---|
| A. Rust/Java imports never resolve to files | `indexer/src/resolver.rs:376-418` has no stage mapping a Rust `use <crate>::…` specifier (emitted raw, `rust-extractor:259-267`) or a Java dotted FQN (`java-extractor:322-341`) to a file; every cross-crate/cross-package import lands in `unresolved_edges`, which modules-list/cycles/trust-suspicious never read, while trust's first-party attribution reads exactly those rows | D1, D10 (+ grpc-java `projectDir` unparsed in `settings_gradle.rs`) | Never worked (June smoke already 0; MODULE-EDGES-1 verified on C++ only) | resolver (reindex) + renderers' zero branches (`http_boundary.rs:483-503` hint conditioned only on HTTP link count; `cycles/mod.rs:224` prints nothing it knows) |
| B. Deps attribution matches the FULL dotted specifier exactly | `classification/src/unresolved_classifier.rs:457-494` + `signals.rs:18-20` exact-match; no Python head / npm subpath reduction at index time (only at query time, `normalize.rs`); slash target key vs dotted binding; `reconcile.rs:173-187` emits the negative without consulting the bindings loaded at `compose.rs:101` | D2, storybook 111/124 (partly), hedge note, django npm hidden by 10% gate | Never worked; HONESTY-GATE-1 relabeled (spec DoD allowed it; the false row is a canonized test fixture `deps_list.rs:1155-1195`) | query-time (no reindex) for the claim; index-time classifier (reindex, trust counts shift) for the resolution |
| C. C/C++ declarator walk stops at the macro token; bodiless `class X;` emits an unmarked symbol | `c-extractor:449-472` / `cpp-extractor:1684-1730` unwrap nested function_declarators and return the outer identifier (shape `M(name)(args)`) or ignore the ERROR child holding the real name (attribute-macro shape); `extract_type` :682-721 pushes forward decls with no flag; `nodes` has no decl column; `pick_unambiguous` drops every inheritance edge to a 70×-declared class | D3, D4 (+ dropped IMPLEMENTS edges, `explain` ambiguity ×71) | Never worked; CPP-SPAN-FIDELITY-1 scoped to the type path only | extractors + `metadata_json.forward_decl` + rank/resolver preference (reindex transition) |
| D. Headlines are lossy projections with no inclusion rule | three file-count bases (`file_versions` all rows / OWNS edges on FILE nodes with a `/` / manifest ownership incl. root `.`); dedup-by-file keeps a symbol's cx under a file label; `--full` marker is byte-equality with large while the group fallback is capped at a fixed 12; "project surfaces" = the catalog AFTER HTTP is lifted out; inference rows never join `files.is_test`; orient hides the type-only verdict whenever a test-only partition exists | D5, D12, D7, D6, D9, COH-2 orient drop, (N test) legend | D7 IS a regression (ECONOMY-2 8a6f1df); D12 and D7 are PINNED BY TESTS asserting the defect (`orient_density_tests.rs:412`, `orient_seg2_tests.rs:785`); rest pre-existing | render layer + one extra count query + inference row join |
| E. Seed documents for 1-line properties are ~90% their qualified name | `document.rs:25-56` = qualified_name + doc + span; properties get `doc_comment: None` (`ts-extractor:1023`); demotion covers callables only and needs a same-name impl | D11 | Design gap since SEED-CHUNK-1 | rank tier (SEED-CHUNK-3) |
| F. Isolated small causes | docs kind rule order + "license" = has-header (`classification.rs:47-112`, `lib.rs:261-268`); boundaries group renderer never reads the `line` it carries (`group.rs:145-160`, spec excluded groups); not-found path passes `repo_uid None` (`seed.rs:99-105`); model label per row (`seed.rs:215`); doctor seed probe always `passed: true`; `SMOKE_SKIP` never appends to `SKIPPED_REPOS` (`smoke-validation-repos.sh:301-312`) | D13, D14, cursor 47%, model 17.7%, doctor tone, manifest | none | each ≤ 1 file |
| G. Ratified designs whose HUMAN rendering drops the qualifier | `check` verdict is ceiling-relative by CHECK-SIGNAL-1; CLI DTO drops the wire `ceiling` marker (`check.rs:76-80`). Trust root posture is MEET over a LiveGraph that is resident only via a hidden dev command (D-T6). `dead` exit 2 + "error:" frozen 2026-04-27; no exit-code contract exists | D15, Posture: Unavailable 28/28, dead exit | by design | human ruling needed for trust/dead; check qualifier is additive |

## Where the audit's proposed queue was wrong
- "CLAIM-INVARIANT-1" as a single cross-command rule does not exist as a code seam: the disconfirming
  signals live in different tables (`unresolved_edges` vs `edges`), different crates, and for deps in a
  fourth bucket (`unknown`) that no headline counts. The honest zero-states are per-surface renderer
  changes fed by counts the handlers already hold; the real fixes are A/B/C resolution work.
- "asgiref is in the 419 unattributed" was false: unknown-classified edges are outside every headline term.
- "JAVA-MODULE-ATTRIBUTION-1" is not the D1 cause on kafka/hadoop (their modules DO own files); the cause is
  import resolution, shared with Rust. grpc-java additionally has the projectDir ownership defect.


---

# Root cause — claim layer (D1, D10, D8, D15) — investigator report, 2026-09-06

## D1 (CRITICAL) `modules list` "No cross-module dependencies detected" on Rust + Java repos
- RENDER SITE: rgr/src/presentation/modules_list.rs:294-308 (`render_edge_list`: edges.is_empty() → text); the
  hint is chosen in rgr/src/presentation/http_boundary.rs:483-503 (`render_modules_note`): `(degraded.is_none(),
  http link_count == Some(0))` → "hint: all imports are intra-module…". It checks ONLY the HTTP-boundary link
  count — never import-graph reliability, unresolved counts, or module count.
- DATA PATH (a) modules list: dispatch.rs:8975 handle_modules_list → module-queries/src/facts.rs:144-210 →
  storage/src/crud/module_edges_support.rs:41-75 `get_resolved_imports_for_snapshot`: `edges WHERE type='IMPORTS'
  AND resolution='static' AND both file_uids NOT NULL` = RESOLVED file→file imports only; joined to
  module_file_ownership by classification/src/module_edges.rs:152-231 (intra-module skip :190-194). Wire
  dispatch.rs:9176-9186 emits only edges; `facts.diagnostics` (imports_total/unowned/intra) computed but NOT
  serialized.
- DATA PATH (b) trust "internal crate/package → X: N": agent/src/attribution.rs:326-331 fed by
  storage/src/trust_impl.rs:440-585 `attribute_external_dependencies`: `unresolved_edges WHERE basis_code IN
  (SpecifierMatchesPackageDependency, ReceiverMatchesExternalImport, CalleeMatchesExternalImport)` matched by
  first_party_matcher (:170-205) against declared module display_names (cargo name canonicalisation).
  These are UNRESOLVED edges — the very imports (a) failed to resolve.
- ROOT CAUSE (c): the two surfaces read disjoint tables. On Rust and Java cross-crate/cross-package imports NEVER
  resolve to a file:
  * Rust: rust-extractor/src/extractor.rs:259-267 — non-relative `use` IMPORTS target_key is the RAW specifier
    (`repo_graph_storage::crud`); only crate::/super::/self:: get a `repo:src/<path>:FILE` key via
    rust_module_to_path (:409-418, prefixes `src/` from REPO root — no crate-root awareness). Resolver
    indexer/src/resolver.rs:376-418 stages: exact stable-key; file_resolution map (real paths + extensionless +
    index.ts/__init__.py, :799-836); C/C++ per-TU include map; repo-prefix fallback SKIPPED when target_key
    contains ':' (:404) — a `::` specifier always does. NO cargo-name→crate-dir stage exists (rg Rust|Java|Cargo
    in resolver.rs → comments only; git log resolver.rs: 10 commits, none Rust/Java-import related). The
    unresolved row is classified SpecifierMatchesPackageDependency (classification/src/unresolved_classifier.rs:
    143-145, because the path dep IS declared in Cargo.toml) — exactly what trust's first-party pass picks up.
  * Java: java-extractor/src/extractor.rs:322-341 — IMPORTS target_key is the dotted FQN; stage 4 constructs
    `repo:org.apache.kafka.common.Foo:FILE` which never exists (source-root prefix and .java unknown;
    strip_extension leaves .java intact, resolver.rs:908-909). kafka trust: 87,242 unresolved imports; hadoop
    184,959. Kafka's Gradle modules DO own files (clients 1811) — the cause is resolution, not ownership.
  * grpc-java EXTRA ownership defect: settings.gradle:97-101 uses `project(':grpc-api').projectDir =
    "$rootDir/api"`; indexer/src/settings_gradle.rs handles only `.name =` renames (:113,170-172; no projectDir)
    → 42 declared modules own 0 files, everything in root `grpc` (1624 files) — vacuously zero before the
    import question. Maven unparsed affects module DECLARATION only (hadoop gets inferred dirs), not the edge zero.
  * Trust's "Suspicious Modules (zero connectivity)" (trust_impl.rs:775-800, MODULE→MODULE IMPORTS in `edges`)
    AGREES with modules list — same edge kind; the disconfirming signal is only the unresolved_edges attribution.
- WHY NOT CONSULTED: same process (handle_modules_list :8975, handle_trust :5144), same storage; the
  first-party attribution is never called on the modules-list path; diagnostics not serialized; renderer gets
  only edges + http link count + rollups.
- REGRESSION? NO — never worked for Rust or Java. smoke-runs/2026-06-21T05-39-52Z/repo-graph-modules.txt already
  "0 cross-module dependencies detected" (three months before MODULE-EDGES-1). Non-zero runs are all C/C++/TS
  (leveldb/duckdb/swupdate/storybook). MODULE-EDGES-1 (2db7335) verified on VCMI (C++ includes); changed
  renderer/wire only. Raw-specifier Rust keys since 25c75ca (2026-04-27).
- SMALLEST FIX: (honesty) daemon passes unresolved-import count / trust's first_party_total on the modules-list
  response; the zero branch prints "0 RESOLVED cross-module imports; N imports unresolved (M attributed to this
  repo's own crates) — import resolution is LOW for <langs>" instead of the intra-module hint.
  (capability) resolver stage mapping `use <crate>::…` / `import <pkg>.<Class>` to a file via the declared
  module catalog (Cargo package name → crate dir src/lib.rs; Java FQN → suffix match **/org/apache/…/Foo.java);
  settings_gradle.rs parses projectDir.
- VERIFICATION: rgr/src/presentation/modules_list_tests.rs:254 (`list_render_no_http_links_keeps_meaningless_hint`
  — the test that PINS the wrong hint), :532 (`list_render_empty_edges_is_zero_state`). No test indexes a
  multi-crate workspace or a Java package layout and asserts ≥1 resolved cross-module edge
  (repo-index/tests/fixtures/rust/simple-crate is single-crate). Probe: two-crate workspace fixture (a→b path
  dep) + two-package Java fixture → modules list ≥1 edge; on the current build trust first_party_total>0 while
  modules-list edges==[].

## D10 `cycles` bare "No module-level cycles found."
- RENDER SITE: rgr/src/presentation/cycles/mod.rs:224-228.
- DATA PATH: dispatch.rs:2231 handle_cycles → livegraph_feed.rs:2835 cycles_auto_response → LiveGraph only when
  resident (kafka/repo-graph: Resident: no) else storage/src/queries.rs:1237-1272 find_cycles_cancellable
  (MODULE→MODULE IMPORTS in `edges`), written by indexer/src/orchestrator.rs:1315-1390 create_module_edges from
  all_resolved_import_pairs — the SAME resolved pairs as D1.
- ROOT CAUSE: same as D1 (empty resolved cross-module graph → Tarjan finds nothing).
- KNOWS BUT DOESN'T PRINT: response carries only cycles/count/ts_type_only_caveat/test_composition_note
  (cycles/mod.rs:68-90); the handler has module_import_edges (queries.rs:1360-1376) and module_qualified_names
  (dispatch.rs:2470-2488) in hand but sends neither.
- SMALLEST FIX: emit module_count + module_edge_count (+ unresolved-import count); zero branch prints "No
  module-level cycles found over N modules / M resolved import edges (K unresolved — see rmap trust)"; when M==0
  say the graph is EMPTY, not cycle-free.
- VERIFICATION: cycles/tests.rs:220 render_shows_no_cycles_message; rgr/tests/cycles_command.rs; the same
  fixtures with a deliberate a↔b import.

## D8 `explain ServiceDispatcher` "Callers (0)" at Confidence: medium
- RENDER SITE: rgr/src/presentation/explain_sections.rs:100-106; Confidence explain.rs:175.
- DATA PATH: agent/src/explain/mod.rs:326-356 → storage/src/agent_impl.rs:1054-1083 find_symbol_callers:
  `edges WHERE type='CALLS' AND target_node_uid = <that exact node>`.
- ROOT CAUSE: no edge kind ever targets a STRUCT node. Rust extractor emits CALLS only from call_expression
  (rust-extractor/src/extractor.rs:836-878); `ServiceDispatcher::new(...)` → target_key "ServiceDispatcher.new"
  (:886-888); resolve_call_target (indexer/src/resolver.rs:480-518) → pick_unambiguous(nodes_by_name["new"]) →
  ambiguous → None; binding fallback (:523-548) needs identifier == "ServiceDispatcher.new" → no. Even enriched,
  the target is the `new` METHOD node, never the struct. NO Instantiates emission in the Rust extractor
  (rg -c Instantiates → 0); struct literals emit nothing. `Callers (0)` is STRUCTURAL for every Rust struct.
- CONFIDENCE: mod.rs:497 derive_repo_confidence → agent/src/confidence.rs:6-25: repo-level call-resolution rate
  (<0.20 Low; ≤0.50 Medium) → 0.24 = Medium; min'd with posture (freshness/completeness). Per-symbol evidence
  never enters.
- LEXICAL COUNT: not available at render time — agent storage port has no name-based/unresolved-reference method;
  find_facts classes have no reference count. Computable from unresolved_edges.target_key LIKE '%Name%' but not
  wired. (UNDETERMINED: source of the grader's "146 references" — a repo grep, not a product figure.)
- REGRESSION? Never worked for Rust structs (since 25c75ca).
- SMALLEST FIX: in explain_symbol, when subtype is a type (STRUCT/ENUM/TRAIT/CLASS) and callers==0: print a
  references line = count of unresolved CALLS/IMPORTS rows naming the symbol + union of callers of its impl
  methods; per-symbol confidence → low when that count>0 and edges==0 (or "callers not tracked for structs").
- VERIFICATION: rgr/src/presentation/explain.rs:635,1061; rgr/tests/explain_command.rs (usage only; no struct
  success test). Probe: explain ServiceDispatcher non-zero reference count; explain ServiceDispatcher::new lists
  lib.rs:343/:404 after enrichment.

## D15 `check --full` PASS@Fresh (leveldb 27%) vs FAIL@Fresh (repo-graph 24%)
- RENDER SITE: rgr/src/presentation/check.rs:162-167 (`Verdict: {verdict}@{freshness}`), determine_verdict
  :178-188. The CLI ConditionEvidence DTO (check.rs:76-80) has only code/status/summary — the wire's
  `ceiling: Option<bool>` (agent/src/dto/signal.rs:743-755) is DROPPED at the CLI.
- ROOT CAUSE / BY DESIGN: agent/src/check/evaluate.rs:130-166 — degrading + CeilingFact::Ceiling{languages} →
  Pass with ceiling:true and the "reached this build's ceiling" summary; NoCeiling keeps Low→Fail (:191-197);
  reduce.rs:18-38 plain fold. Ratified: docs/slices/check-signal-1.md §2.1 ("ALL materially-present languages
  WITHOUT a resolution path → PASSING stated limitation"), §3 freezes verdict→exit mapping, §2.3 adds the JSON
  ceiling marker. Shipped b1c499f 2026-08-31 (leveldb moved Incomplete/Fail → Pass deliberately).
- DEFECT: presentational only — the human `Verdict:` token does not carry the qualifier the JSON carries.
- SMALLEST FIX: add `ceiling` to the CLI ConditionEvidence; when any passing condition has ceiling==true render
  "Verdict: PASS@Fresh (with stated limitation: call-graph ceiling for C++)"; exit code unchanged.
- VERIFICATION: agent/src/check/reduce.rs:390,443,636; rgr/src/presentation/check.rs:398-400,475;
  rgr/tests/check_command.rs. Probe: leveldb check --full --json shows ceiling:true; human line shows the qualifier.

## SUMMARY
D1+D10 = ONE root cause (resolver has no Rust-crate / Java-FQN → file stage; never worked; renderers compound it
with an unconditioned hint and a bare no-cycles that ignore counts available in-process) + grpc-java's projectDir
ownership defect. D8 = separate (no edge targets a struct node; repo-level confidence). D15 = by ratified design;
CLI drops the ceiling marker.

---

# Root cause — deps negatives (D2, storybook 111/124, hedge note, django npm) — 2026-09-06

## D2 (CRITICAL) django "no static import: asgiref"
- GROUND TRUTH: 42 sites (31 outside tests/): 29× `from asgiref.sync import …`, 11× `from asgiref.local import
  Local`, 1× asgiref.testing, 1× bare `import asgiref.sync`. ZERO undotted `import asgiref`. sqlparse is imported
  4× as bare `import sqlparse`. THAT SHAPE DIFFERENCE IS THE WHOLE STORY.
- RENDER: rgr/src/presentation/deps_list.rs:464-465, :594-597, caveat :323-329; label from
  declared_unobserved_basis HARDCODED "no_static_import_found" at daemon-runtime/src/deps_headline.rs:470.
  deps_list.rs:1155-1195 unit test ASSERTS "no static import: asgiref, tzdata" — the false negative is canonized.
- DATA PATH: python-extractor/src/extractor.rs:797-840 emit_from_import_binding → ImportBinding{identifier:
  sync_to_async, specifier:"asgiref.sync"} + Imports edge with target_key SLASH form "asgiref/sync"
  (python_specifier_to_target_key :522-527) + metadata.specifier "asgiref.sync"; calls → Calls edge target_key
  "sync_to_async". Manifest: repo-index/src/manifest_deps.rs:375-496 reads [project].dependencies, strips PEP 508 →
  "asgiref" (correct; test :589-616). Resolver → classifier: resolver.rs:691-700 ImportsFileNotFound; :726-750
  CallsFunctionAmbiguousOrMissing; orchestrator.rs:1049-1078 persists verdict.classification. Query:
  storage/src/crud/module_edges_support.rs:290-298 reads ONLY rows WHERE classification =
  'external_library_candidate'; module-queries/src/deps/compose.rs:96-107 + reconcile.rs:173-187 emits
  DeclaredButUnobserved for every declared name absent from observed_set.
- ROOT CAUSE R1 (index-time classifier, exact match on the FULL dotted specifier):
  classification/src/unresolved_classifier.rs classify_unresolved_import :137-188 → resolve_declared_dependency
  ("asgiref.sync", {asgiref,…}) :457-494: base_specifier splits only on "::" (:434-439); has_package_dependency is
  exact equality (signals.rs:18-20); the dotted branch (:477-492) skips declared names WITHOUT a dot (:480) →
  unknown() (:187). Calls edge sync_to_async: Rule 5b (:105-110) has_package_dependency(deps, "asgiref.sync") →
  exact miss → unknown() (:128). `import sqlparse` matches exactly → ExternalLibraryCandidate. So NONE of
  asgiref's ~42 import edges / ~136 call edges is external_library_candidate — they sit in the `unknown` bucket
  which deps list never reads.
- Is asgiref among the 419 unattributed? NO, by construction: compute_unattributed (deps_headline.rs:328-368) =
  total_external_imports − scoped − rejected, where total counts only external_library_candidate rows. asgiref
  is in a FOURTH bucket. What the 419 contains is UNDETERMINED (candidates: Admission::Skip locals compose.rs:171;
  imports from files without module_file_ownership :136); its printed reason "imported files outside a parsed
  manifest scope" (deps_headline.rs:357-358) is ASSERTED, not computed.
- LATENT LAYER R2 (query-time admission): even if classified external, compose gets specifier "asgiref/sync"
  (slash key, no metadata read): is_bound fails (resolve.rs:82-97), specifier_backed fails (:33-41),
  classify_observed → Package{"asgiref/sync"} (normalize.rs:102-105 splits on '.' only) → Admission::Rejected
  ("non-import fragment"). The compose fixture compose.rs:1050-1055 admits "asgiref.sync" — a target-key shape the
  Python extractor never produces (extractor.rs:2008-2018 asserts src/module/submod).
- WHY THE NEGATIVE ISN'T GATED (C1): reconcile.rs:173-187 consults only observed_set — not rejected fragments,
  not the unattributed remainder, not unknown-classified edges, and NOT the import bindings ALREADY LOADED at
  compose.rs:101 (file_signals.import_bindings_json holds "asgiref.sync" for every one of the 42 files). The
  negative is emitted while contradicting evidence is in memory.
- REGRESSION? No — never worked. Slash key 4c9071e 2026-04-28; Python deps view 4055d27 DEPS-LIST-REWRITE-1
  (2026-08-26) with normalize_python_specifier at QUERY time only; classifier never got a Python head reduction
  (only dotted logic is the Java branch, 78053eb). HONESTY-GATE-1 9ea23df RELABELED rather than attributed: spec
  honesty-gate-1.md:66-67 allowed "or labeled 'no static import found'"; build-1.md:28 records the false row as
  the passing proof; review-3 was an operator close-out.
- SMALLEST FIX: query-time, no reindex — in compose.rs after :101 push each non-relative binding.specifier into
  module_imports[owner(file)] (dedupe per file) so it flows classify_observed → normalize_python_specifier →
  "asgiref" and reconcile sees it observed (import_count = declaration sites; add additive
  import_declarations). Narrower: guard the negative — before reconcile.rs:177 check per-module normalized
  binding heads → DeclaredAndUsed with basis "import declaration". Index-time fix (Python head / npm subpath
  reduction in resolve_declared_dependency, mirroring normalize.rs; fix slash-vs-dot) is correct but changes
  trust/unresolved counts globally → own slice + reindex.
- VERIFICATION: failing tests to add — unresolved_classifier: edge_with_meta("asgiref/sync",
  {"specifier":"asgiref.sync"}), deps ["asgiref"] → ExternalLibraryCandidate (today Unknown); compose: bindings
  [(f,"sync_to_async","asgiref.sync")], declared ["asgiref"], zero external edges → DeclaredAndUsed. Existing
  reconcile.rs:414-447 feeds "asgiref" directly (bypasses the broken path). Probes (read-only copy): SELECT
  classification,count(*) FROM unresolved_edges WHERE target_key IN ('asgiref/sync','asgiref/local',
  'sync_to_async','async_to_sync','Local') GROUP BY 1 → all unknown; file_signals import_bindings_json LIKE
  '%"asgiref.sync"%' → ~29.

## storybook root "used 13 · no static import found 111" of 124 across 12
- Manifest set pinned: root (13) ∪ .github/scripts (2) ∪ 10 test-storybooks/**/package.json = 124 across 12
  (exact). scripts/ is its own module.
- npm MECHANISM (R3): ts-extractor/src/extractor.rs:1377-1400 emits an Imports EDGE only for relative+resolved
  imports; a bare package import yields a binding + observation, never an edge. So npm "used" evidence exists
  only when a call/instantiate edge's identifier is bound by an ES import whose RAW specifier EXACTLY equals a
  declared name. R4 require() is in the call-noise list (:2252) → no binding. R5 type-only imports → no evidence.
- SAMPLE (searched ts/tsx/mts/cts/js/mjs/cjs, node_modules excluded): storybook 0 bare / 21 subpath
  ('storybook/test') / 2 type-only → resolution failure; @storybook/react 3 bare / 2 subpath / 6 type-only →
  resolution failure; express/cors/morgan require() in server-kitchen-sink/server.js:1-3 → resolution failure;
  vitest 1 bare / 4 subpath ('vitest/config') → mostly failure; addon-a11y/docs/links referenced only as STRINGS
  in .storybook/main.* → genuinely no static import; husky/lint-staged/nx/typescript/cross-env/@types/*/
  ember-cli-* → genuinely un-imported tooling. Verdict: a MIX; the tooling block is honest, but a systematic
  resolution-failure class (subpath, type-only, CommonJS) from the same exact-match gap as D2. Exact split
  UNDETERMINED without --json.

## The hedge note (C2)
- deps_headline.rs:280-311 declared_unobserved_caveat, constant per ecosystem, emitted once at deps_list.rs:
  323-329; basis hardcoded :470. Names dynamic imports / config files — applies to neither asgiref nor
  'storybook/test'. CAN be per-row from data already loaded: (a) bindings whose normalized specifier equals the
  declared name → flip to used; (b) is_type_only bindings → "type-only import" (JSON carries it, ts-extractor:1371;
  UNDETERMINED whether ImportBindingJson module_edges_support.rs:348-361 preserves it — reads only
  identifier/specifier/is_relative); (c) dynamic import() observations: UNDETERMINED whether persisted; (d)
  config-string references are not extracted — the only cause the caveat should still name generically.

## django package.json "silently dropped" (R6/R7/C5)
- django HAS package.json (6 devDeps: biome, puppeteer, grunt…). Selection: reader_context.rs:121-128
  dominant_deps_ecosystem → python (2904 .py vs 111 .js); secondaries need ≥10% share
  (deps_ecosystem_presence.rs:167-207; MATERIAL_LANGUAGE_SHARE_NUM=10 reader_context.rs:175) → npm dropped, no
  other_ecosystems line; compose.rs:187 path_ecosystem filters npm out of the python view. The npm manifest count
  IS recorded at index time (repo-index/src/compose.rs:724) but only the dominant ecosystem's count is read. By
  design (ruling CS1-5 Option B) but SILENT. Fix: read present counts for all ecosystems; print one "also present,
  below materiality (npm: 1 manifest; --ecosystem npm)" line. LATENT HAZARD R7: two declared modules at canonical
  path "." (npm root package_json.rs:145-152; pyproject root); compose.rs:140/:212 keys module_keys by canonical
  path → last writer wins; `--ecosystem npm` on django may skip the row (UNDETERMINED which pass wins).

## SUMMARY
Resolution defects: R1 exact-match specifier, no Python head / npm subpath reduction at index time (explains D2
alone); R2 slash target key vs dotted binding specifier; R3 no Imports edge for bare npm specifiers (calls-only
evidence); R4 require() unbound; R5 type-only imports invisible; R6 10% ecosystem gate; R7 canonical-path
collision for two root manifests.
Claim-gate defects: C1 reconcile emits the negative without consulting the bindings in memory (HONESTY-GATE-1's
invariant violated); C2 hardcoded basis + fixed caveat; C3 the 419's reason is asserted not computed, and
unknown-classified edges are invisible to it; C4 the negative was canonized as expected output because the spec's
DoD accepted a relabel; C5 npm omitted with no "also present" line.

---

# Root cause — C/C++ names (D3, D4) — investigator report, 2026-09-06

## D3 — macro-wrapped function names shipped as the macro token
- SYMPTOM: 25 distinct complexity rows in the smoke carry a macro as the symbol name: hadoop 12× URI_FUNC,
  poco 9× PRIV (pcre2) + 3× PREFIX (expat), duckdb 1× ZSTD_ALLOW_POINTER_OVERFLOW_ATTR. (Correction: expat
  PREFIX rows are in poco; zstd is in duckdb.)
- EXTRACTION SITE: two identical declarator walks — C `rust/crates/c-extractor/src/extractor.rs:449-472`
  `extract_function_name` (unwraps function_declarator/pointer_declarator, returns the identifier);
  C++ `rust/crates/cpp-extractor/src/extractor.rs:1684-1730` (same + reference_declarator).
  Routing `rust/crates/indexer/src/routing.rs:193-194` (.c/.h → c-extractor).
- ROOT CAUSE (verified with tree-sitter-c/cpp 0.23.4 on the real files):
  Shape 1 `M(name)(args)`: tree-sitter parses `URI_FUNC(ToStringEngine)` as an INNER function_declarator whose
  single "parameter" is a bare type_identifier (the real name); the loop unwraps both declarators and lands on
  `URI_FUNC`. Corpus: hadoop uriparser 90 defs, poco pcre2 32, poco expat 28 — all stored under the macro name,
  colliding into :dupN keys.
  Shape 2 attribute macro before name (C++ zstd): `function_declarator(declarator: identifier
  "ZSTD_ALLOW_POINTER_OVERFLOW_ATTR", ERROR(identifier U32, identifier ZSTD_insertBtAndGetAllMatches),
  parameters)`; the real name is the last identifier inside an ERROR child before `parameters`; cpp:1699-1701
  takes the identifier branch and never inspects the ERROR.
- WHY CPP-SPAN-FIDELITY-1 DID NOT GENERALISE: its fix is entirely on the TYPE path (`walk_top_level` :487-493 →
  `extract_type` only when `leading_type_specifier` finds class/struct/enum; `type_name_and_macros` :811-866
  re-tokenises the header). Functions fall through to the untouched `extract_function_name` (blame: all lines
  from e0fa892 / b96635a — the original extractors). Spec §1-2 names only `class|struct [MACRO...] Name`;
  build-0.md: "no other-extractor changes". The C extractor was never in scope.
- REGRESSION? No — original behaviour since the first extractors; surfaced once class names stopped dominating.
- SMALLEST FIX: in both `extract_function_name`s: (a) in the unwrap loop, a function_declarator whose declarator is
  a function_declarator with exactly one type-only parameter → name = that type_identifier, outer identifier →
  `metadata_json.macro_tokens` (the additive key the class path already uses, :1133-1150); (b) after the loop, an
  ERROR child before `parameters` → name = its last identifier, earlier identifiers → macro_tokens. Unambiguous:
  a function returning a function is invalid C/C++; function-pointer returns go through parenthesized_declarator.
  Stable-key churn = one reindex transition (FIND-KIND-MISLABEL-1 / CPP-SPAN-FIDELITY-1 precedent).
- VERIFICATION: inline tests c-extractor :978-1031 (`extract_ok`), cpp-extractor :2379-2444 (`sym`; mirror the
  CPP-SPAN-FIDELITY-1 macro tests). Fixtures verbatim from corpus (URI_FUNC(ToStringEngine), PRIV(check_escape),
  PTRCALL PREFIX(prologTok), the 4-line zstd header) + negative `int (*getHandler(int x))(double)` → getHandler.
  Live: `find ToStringEngine` on hadoop; orient shows `UriRecompose.c:87 — ToStringEngine (cx 102)`.

## D4 — forward declarations indistinguishable from definitions, and outranking them
- (a) The extractor EMITS a symbol for bare `class X;`: top-level class_specifier with no body → `walk_top_level`
  :495 → `extract_type` :682-721 pushes unconditionally; `type_span_and_body_close` :901-955 reaches
  BodyProbe::None and emits the fragment extent as the span. Nothing marks decl vs definition (signature None,
  visibility Export). Not a regression (pre-slice `extract_class_like` did the same); spec §2.3 deliberately keeps
  bodiless specifiers as visible declarations.
- (b) NO decl flag on symbols: `nodes` columns (storage migrations/001-initial.sql:76-95) have none. `is_decl`
  lives only on `seed_vectors` (migration_034), computed at seed time (repo-graph-seed/src/pass.rs:192 via
  classify::is_declaration :736-744) and gated on `is_callable_subtype` (:721-726: FUNCTION/METHOD/CONSTRUCTOR/
  GETTER/SETTER only) — a CLASS chunk is never `(decl)`.
- (c) ORDERING: SQL pre-order storage/src/find_facts_reads.rs:157-167; real order
  daemon-runtime/src/find_facts/rank.rs:166-180 `(test_partition, kind_weight, match_quality, evidence_rank,
  qname_len, path, stable_key)`. All 71 CLASS rows tie through evidence_rank → `path ASC` decides (AI/…, client/…
  before lib/…). `FactSymbolRow` selects neither line_end nor col_end; a span-length heuristic would misclassify
  one-line `struct P { int x; };`.
- (d) IDENTITY (bigger than find): `storage/src/queries.rs:653-691 resolve_symbol` → 71 identical qualified_names
  → AmbiguousSymbol; `rgr/src/commands/graph.rs:108-125` prints all 71 uncapped. Edges:
  `indexer/src/resolver.rs:57-59,610-634` resolves IMPLEMENTS/INSTANTIATES by `nodes_by_name`;
  `pick_unambiguous` returns None when >1 candidate → EVERY `: public CGHeroInstance` inheritance edge is DROPPED
  as unresolved because of the 70 forward decls. (UNDETERMINED whether filter_by_edge_affinity narrows Implements
  by kind — not read past :644.)
- SMALLEST FIX: extractor writes additive `metadata_json {"forward_decl": true}` on the BodyProbe::ForwardDecl/
  None paths; `find_fact_symbols` selects metadata_json; `rank_key` inserts decl_rank between (c2) and (d);
  `classify::is_declaration` consults the flag so seeds label `(decl)` for types; resolver step 2 / `pick_unambiguous`
  prefer the non-decl candidate (restores the inheritance edges).
- VERIFICATION: cpp-extractor tests (`class Foo;\nclass Foo { int x; };` → two nodes, only the second lacks
  forward_decl); rank.rs rule test `definition_beats_forward_decl`; facts_render_tests. Live: `find CGHeroInstance`
  shows lib/mapObjects/CGHeroInstance.h:55 first; `explain CGHeroInstance` resolves.

---

# Root cause — counting/headline defects (D5, D12, D6, D7, D9, COH-2 orient drop) — 2026-09-06

## D5 (HIGH) three irreconcilable file totals per snapshot
- RENDER: orient header orient_sections.rs:107-113 (MODULE_SUMMARY.evidence.file_count); stats.rs:212,223-226
  "total_files: N (in directory groups)"; modules per-row module_shared.rs:63-72 "owned files"; orient group rows
  orient_sections.rs:546-557.
- THREE BASES: (1) indexed = storage/src/agent_impl.rs:256-260 COUNT(DISTINCT file_uid) FROM file_versions — no
  parse_status/language filter; includes CONFIG files (repo-index/src/compose.rs:1401-1445, parse_status
  "config"; names in indexer/src/routing.rs:76-100), CONTRACT .proto files (orchestrator.rs:322-352), and
  read-FAILED files (compose.rs:1343-1353). (2) in directory groups = storage/src/queries.rs:1608-1613 COUNT OWNS
  edges per MODULE node; OWNS only from orchestrator.rs:1329-1343 over file_to_module, filled (:902-910) ONLY for
  kind=="FILE" nodes and only when get_module_path is Some — resolver.rs:782-789 returns None when the path has
  NO '/' (root-level files); config/proto/failed have no FILE node. (3) owned = agent_orient_reads.rs:117-127 over
  module_file_ownership, written per manifest family with a language filter and root "." matching ALL files
  incl. root-level (compose.rs:2993-3000); families language-disjoint (npm JS/TS :3060-3067; pyproject python;
  gradle java/kotlin/scala).
- EXACT SET DIFFERENCES (verified on disk): grpc-java 1917−1627 = 290 = 96 config + 194 .proto (exact);
  zvec 239−234 = 5 = root eslint.config.js + 4 config (exact); FRAKTAG 92−85 = 7 config (exact). Σ modules >
  stats: django +1 root Gruntfile.js; amodx +1 playwright.config.ts; zap-engine +1 vite.config.ts (each exact:
  owned by root "." but excluded from OWNS); storybook +3 (found 2), vscode +5 (found 3) — UNDETERMINED residual
  1–2 without the live DB (candidate: a file whose directory has no MODULE node).
- COHERENCE-3 basis text: words truthful ("indexed", "in directory groups", "owned"), but no surface states the
  INCLUSION RULE (stats.rs:219-222 comment admits "EXCLUDES files no directory node owns" — never rendered);
  spec coherence-3.md:31-33 only required naming a basis; its own examples were more informative than shipped.
- REGRESSION? Not introduced (bases: dd80bf9 2026-04-13, 55e6b2d 2026-05-08); COHERENCE-3 55ab942 added labels
  only; check_repo.rs:54-90 seam test proves orient==check only.
- SMALLEST FIX: (a) orient header "1917 files indexed (1627 source with symbols; 290 config/contract
  tracked-only)" — one extra COUNT WHERE parse_status IN ('config','failed') OR extractor='contract-schema' in
  compute_repo_summary; (b) stats "(in directory groups; excludes N root-level and tracked-only files)" = indexed−Σ;
  (c) modules footer "Σ owned = 3015 includes 1 root-level file not in any directory group". Smaller alternative:
  root-level files become members of a "." MODULE node so bases 2 and 3 coincide.
- VERIFICATION: stats.rs:521-535; check_repo.rs:61; probe on a read-only copy: SELECT parse_status, extractor,
  COUNT(*) FROM file_versions GROUP BY 1,2 → 290 on grpc-java; files without '/' → 0.

## D12 complexity headline labels a symbol's cx against a file
- RENDER: orient_sections.rs:153-178 complexity_line: iterates per-SYMBOL top_complex (complexity.rs:105-125),
  label = entry.file (:165-171), dedup `seen.contains(&label)` (:172-174) keeps the FIRST symbol's cx under the
  file path, drops later symbols in the same file (Render 95) even when they outrank the next file. Detail
  :236-249 renders file:line — symbol (cx). PINNED by orient_density_tests.rs:410-450 ("file-rollup headline is
  unanchored").
- REGRESSION? No (dedup 63a56f7 2026-06-21 ORIENT-DENSITY-IMPL-1; ANCHORS-EVERYWHERE-1 left the headline as-is).
- SMALLEST FIX: keep the symbol on the headline "db_bench.cc — Run (cx 36)" (one-line change at :175), and
  either drop dedup (top-5 symbols) or add "(+1 more center in file)".
- VERIFICATION: update orient_density_tests.rs:412 and orient_tests.rs:422.

## D6 surfaces "0 project surfaces" + test-fixture consumer rows first
- RENDER: count surfaces.rs:283-298 format_count(self.count, "project surface"); HTTP section
  http_boundary.rs:160-280.
- ROOT CAUSE: count = results.len() AFTER dispatch.rs:7356-7361 filters OUT http_provider|http_consumer ("lifted
  out of the catalog") → "project surfaces" = the non-HTTP catalog (backend/cli/lib), structurally 0 on these
  repos; HTTP-SURFACE-COHERENCE-1 ba28968 renamed the noun instead of removing the line. ORDERING:
  http_boundary.rs:299 refs.sort_by(line_key), line_key (:334-345) starts with direction → "consumer" <
  "provider" lexically; is_test is field 6 → fixtures interleave, never sectioned; headline (:171-181) excludes
  fixtures (:99-120) while rows render all with [test] (:218-220) → 21 rows under "17".
- REGRESSION? COHERENCE-3 added the headline exclusion (per spec §2.2) but did not reorder rows; lexical
  direction sort from FINAL-POLISH-1 d9ff6b7. Not new; made visible.
- SMALLEST FIX: (1) surfaces.rs:283-298 omit the line when count==0 && http_present (or "no non-HTTP project
  surfaces"); (2) line_key = (is_test==Some(true), direction=="consumer", method, route, …) → providers,
  consumers, fixtures last; or a "test fixtures (excluded from counts):" sub-heading.
- VERIFICATION: http_boundary.rs:584, :557; surfaces.rs:705; add ordering assertion.

## D7 "--full identical to --budget large (nothing further to show)" beside "… and 30 more groups"
- RENDER: orient.rs:319-343: `if render_body(Large) == out → identical notice; else if budget_saturated() →
  complete`. Comment :323-329 says identical "takes PRECEDENCE … whether or not the repo is saturated".
- ROOT CAUSE: predicate is BYTE-EQUALITY with large. The elided list is the daemon-injected
  directory_group_fallback (orient_seg2.rs:302-360; footer :354-358) whose row count is FIXED at
  FALLBACK_TOP_N = 12 (daemon-runtime/src/orient_topology_fallback.rs:61,209) regardless of tier → --full cannot
  expand it → large == full. budget_saturated (orient_seg2.rs:373,409-419) DOES treat it as an elision but is
  never consulted because the identical branch wins. Contradicts economy-2.md:36-41 ("complete or says what it
  elided").
- REGRESSION? YES — introduced by ECONOMY-2 8a6f1df; PINNED by orient_seg2_tests.rs:754 and :785 (the latter
  asserts "and 41 more group" coexisting with the notice).
- SMALLEST FIX: when !budget_saturated(): "[--full identical to --budget large — N group rows elided by a fixed
  cap; see stats]"; current text only when saturated. Or make the fallback tier-aware (pass depth cap into
  orient_topology_fallback::build).
- VERIFICATION: flip orient_seg2_tests.rs:785-806.

## D9 Spring inference from a test fixture, no [test], no anchor; propagates into dead
- RENDER: inferences_render.rs:223-239 symbol_at ("name (basename:line)" or "name (basename)"); no is_test read.
  dead: dead_render.rs:130-155 framework_line from total_inferences.
- DATA PATH: storage/src/queries.rs:398-406 InferenceListRow from :2616-2619 SELECT … FROM inferences — NO join to
  files → no is_test. inferences_serve.rs:277-296 record_json: file from stable key (:165-176), line from
  value.line_start (:180-182 "Spring beans have no line"). Spring producer classification/src/spring_liveness.rs:
  233-245,259-266 writes {annotation, convention, reason} — no line_start (React writes it). Spring classification
  runs over query_all_nodes with no fixture filter (compose.rs:1570-1585). dead counts inferences.len() unfiltered
  (handlers/quality/dead_causes.rs:92-96).
- ROOT CAUSE: three omissions — row never carries files.is_test; Spring omits line_start though the node has a
  location; dead_causes counts every row.
- REGRESSION? Not new (INFERENCES-SURFACE-1 c9d9c8d); COHERENCE-2's test-partition never reached inferences/dead.
- SMALLEST FIX: storage query projects is_test (join files via file_uid from the key); record_json adds is_test;
  symbol_at appends [test]; dead_causes "Spring: 1 (1 in test fixtures)" or excludes fixtures; spring_liveness adds
  line_start.
- VERIFICATION: inferences_serve.rs tests :520-540 + is_test fixture; dead_render framework_line test.

## COHERENCE-2 partial — type-only label dropped in orient for vscode/repo-graph — CONFIRMED
- orient_sections.rs:318-321 draw_anchor = production None → true; Some → test_only==Some(0) && unknown==Some(0).
  Type-only label emitted only if draw_anchor (:390-405). vscode test_only=5, repo-graph=1 → anchor AND verdict
  suppressed. Introduced by ORIENT-CYCLES-DISAGREE-1 099d5a3 (anchor gate), inherited by COHERENCE-2 5051e42.
- FIX: read type_only from the first PRODUCTION cycle (carry first_production_index or per-cycle composition in
  the leaf) and render the label independent of draw_anchor. Test: orient_tests.rs:1049 add label assertion.

## SHARED ROOT CAUSES
1. Counts computed on different row sets, labelled by name only — no surface states the inclusion rule (D5).
2. Headlines are lossy projections of detail rows with no back-pointer (D12 dedup, D7 fixed cap, D6 filtered
   catalog rendered above the thing it excludes).
3. files.is_test reaches some surfaces but not the row DTOs feeding others (D6 ordering, D9 inferences/dead,
   COH-2 orient gate).
4. TWO defects are PINNED BY TESTS asserting the defective behaviour (orient_density_tests.rs:412,
   orient_seg2_tests.rs:785) — green gates cannot catch them.

---

# Root cause — lower tier (D11, dead exit, D13, D14, cursor bytes, LiveGraph/doctor, smoke manifest) — 2026-09-06

## D11 FRAKTAG seeds return interface properties, zero write-path symbols (SEED-CHUNK-3)
Four stacked mechanisms:
1. A TS interface property IS a symbol chunk: ts-extractor/src/extractor.rs:941-943 `property_signature` →
   extract_interface_property (:994-1027) pushes SYMBOL/Property with a span and HARD-CODES doc_comment: None
   (:1023). Seed corpus admits every SYMBOL with a span (storage/src/seed_impl.rs:67-78) — no subtype/length filter.
2. PRIMARY DRIVER — the embedded document is name-dominated: repo-graph-seed/src/document.rs:25-56
   build_chunk_document = qualified_name + doc + first 60 span lines. For `updatedAt` that is literally
   "ConversationSession.updatedAt\nupdatedAt: string;" (qualified_name = parent.name, extractor.rs:1006); for
   createSession it is the name + doc + 60 code lines with no lexical persist/disk. The static mean-pooled model
   (model2vec-rs) lets "conversation"/"session" dominate a 2-line doc and averages them down over 60 lines.
3. SEED-CHUNK-2 demotion cannot touch properties: classify.rs:719-724 is_callable_subtype = FUNCTION/METHOD/
   CONSTRUCTOR/GETTER/SETTER only → is_decl=false for PROPERTY; and rank.rs:89-135 sinks a decl only below an impl
   of the SAME (is_test, qualified_name) — a property has no impl counterpart (rank.rs:97-98).
4. Duplication is real source duplication: engine/src/core/ConversationManager.ts:27 exported interface + a local
   non-exported copy in ui/src/components/fraktag/ChatDialog.tsx:37. (createSession is :71, logTurn :133 — the
   audit's :68/:127 drifted.)
- ROADMAP :322-325 (SEED-CHUNK-3) cause confirmed but incomplete: add the name-dominated document mechanism and
  the extractor's doc_comment: None for properties.
- REGRESSION? No — design gap since per-symbol chunks (SEED-CHUNK-1).
- SMALLEST FIX: rank.rs kind/length-aware effective score — subtype==PROPERTY (or span ≤1 line, no doc) ranks as
  a "field" tier below any body-bearing chunk in its PARTITION (same mechanism as decl demotion, keyed on
  partition, not qualified_name); thread subtype/line count into SeedVectorEntry (pass.rs:194-204 has them). Do
  NOT exclude properties from the corpus.
- VERIFICATION: rank.rs unit test (0.47 PROPERTY sinks below 0.39 METHOD same partition); live sc2-persist re-run
  on the retained FRAKTAG root → createSession/logTurn in top 10, ≤2 [PROPERTY] rows.

## `dead` exit 2 + "error:" on a deliberate refusal
- SITE: rgr/src/commands/dead.rs:70 eprintln "error: `rmap dead` is disabled" … :98 ExitCode::from(2); comment
  :49 "DELIBERATELY DISABLED — 2026-04-27 (disable decision + exit code 2 FROZEN)".
- ROOT CAUSE: the refusal reuses the runtime-error code. The only exit contract is rgr/src/daemon_command.rs:38-52:
  0 success / 1 usage / 2 runtime error (daemon unavailable, repo not found, timeout) / 3 still running. A
  refused-by-policy command fits none. Other refusals: gate vacuous pass exit 0 (presentation/gate.rs:175-182);
  inferences n/a exit 0; NOTE django `orient --full` exited 2 on a non-error path (django-meta.json) — second
  command worth a look.
- Documented contract: none (no docs/contracts in repo-graph; README has no exit-code section; doctor --help 0/1;
  TECH-DEBT :1568-1569 check 0/1/2).
- REGRESSION? No (frozen 2026-04-27; unchanged by DEAD-CAUSES-1 e909190).
- SMALLEST FIX: (a) keep 2, drop the "error:" prefix, print the refusal to stdout as a policy verdict; or (b) new
  EXIT_REFUSED_BY_POLICY (e.g. 4) + document the codes in README. (b) touches a FROZEN decision → human ruling.
- VERIFICATION: unit test on run_dead output prefix; smoke meta dead.exit_code equals the documented value.

## D13 `docs list` kinds — "architecture" catch-all; "license" = has a license header
- SITE: doc-facts/src/classification.rs:47-112 classify_doc_kind, path-only, ordered: architecture.md/
  contributing.md/changelog.md OR path contains "docs/" or "design/" → Architecture (:73-80) BEFORE the
  docker-compose/.yaml Config rule (:95-101); every other .md/.rst/.txt → Architecture (:105-107) = the default.
  So docs/monitoring/docker-compose.yaml → architecture (docs/ wins), CHANGELOG.md → architecture (explicit),
  repo-graph "architecture 577" = the default bucket.
- hadoop "license": second content pass doc-facts/src/lib.rs:261-268 upgrades Architecture|Config → License when
  has_license_marker (classification.rs:15-45, "apache license" in first 2000 chars). Hadoop files carry the ASF
  header (verified). "license" means "has a license header", not "is a license document" — a semantic overload of
  DOCS-LIST-2 §2.
- REGRESSION? Partly: docs/ precedence original (83bfd8f); the License upgrade is DOCS-LIST-2 (527e611) and
  produces the new hadoop labels.
- SMALLEST FIX: (1) Config rule above the docs/ rule (extension beats directory); (2) License only when the marker
  is the document's body (LICENSE*/COPYING* or marker-and-nothing-else) or rename to licensed-header; (3) rename
  the .md fallthrough from Architecture to a neutral doc/prose kind.
- VERIFICATION: classification.rs tests (docs/x/docker-compose.yaml → Config; ASF-headed CONTRIBUTING.md not
  License); diff the "By kind" block on vscode/hadoop/repo-graph.

## D14 boundaries 0/772 line anchors; ×N collapses call sites
- Storage HAS a line per hit (migration_024.rs:43-46 line_start/line_end/col_*). DTO carries it
  (boundaries_list/mod.rs:53-59 `lineStart`, comment: "Deliberately NOT rendered in the HUMAN grouped view").
  Renderer boundaries_list/group.rs:145-160 builds cols from kinds/direction/scope/family/file only; ×N (:160,
  :198); `line` never read. boundaries_summary is aggregate-only.
- ROOT CAUSE: deliberate scope in ANCHORS-EVERYWHERE-1 (docs/slices/anchors-everywhere-1.md:33-37, :59: "grouped
  boundaries headline never carries a line"). The spec targeted "boundaries list rows" but the human list has NO
  ungrouped row mode (FINAL-POLISH-1 ×N rollup d9ff6b7 is the only human rendering) → the anchor landed only in
  JSON and `boundaries show`. ×13 on ximagepool.c = 13 distinct line_start hits squashed by the (file×direction)
  key (group.rs:330).
- REGRESSION? No.
- SMALLEST FIX: group.rs accumulates lines: BTreeSet<u64> per group (same pattern as kinds/scopes sets :216-) and
  renders "×13 @ 112,140,… (+8 more)" capped at 5 — a set, never one picked line, consistent with the spec.
  Summary stays line-free by design.
- VERIFICATION: boundaries_list/tests.rs ×3 test (:138-139) gains "@ l1,l2,l3"; spot-check 3 gstreamer lines.

## Cursor bytes — not-found fallback 47%; per-row model label 17.7%
- SITE (cursor): rgr/src/presentation/seed.rs:99-105 — Group B (symbol-not-found) passes repo_uid None →
  render_seed_chunk_candidate (:153-224) is composable only with Some(uid) (composable_cursor_kind :57-69); None
  → render_seed_chunk_next (:280-283) = the full `(cd … && rmap explain …)` line. ECONOMY-2's contract
  (economy-2.md:22-35) is scoped to a seed-bearing `find`; the not-found path was explicitly left byte-identical.
  A scope hole: the error path has no header to anchor to.
- SITE (model label): seed.rs:215 row template "(score, {source}, model {model}…)" per row by design (:165-167
  "the daemon's own VALIDATED source, never a hardcoded label"); 45 B × 10 rows.
- REGRESSION? No.
- SMALLEST FIX: (1) render_symbol_not_found_semantic emits the same pattern header once after "Semantic
  candidates …:" and passes Some(uid) (uid from the first candidate's stable_key prefix, or add repo_uid to the
  error data); (2) hoist "{source}, model {model}" into the seeds heading (find + not-found), keep per-row only
  when a row's model_id differs from the header's (preserves the honesty rule).
- VERIFICATION: seed.rs tests :448, :653, :724-764 pin the full form for None — update; cr1-callers-nf cursor
  lines ≤15%; eco-find-seeds model string once.

## Trust "Posture: Unavailable / Resident: no" 28/28; doctor [ok] on unavailable
A. LiveGraph residency: render trust.rs:128-136 (root MEET; "a cold LiveGraph reads Unavailable (D-T6)") +
   :227-242. Producer daemon-runtime/src/trust_coherence.rs:43 MEET over both halves; test :409-411 pins D-T6
   (cold LiveGraph degrades the overall posture even over a Fresh snapshot) — BY RATIFIED DESIGN.
   Residency requires state.rs:211-214 livegraph populated ONLY by dispatch.rs:428-429 livegraph_preload/
   refresh, sent solely by the hidden dev CLI `rmap dev livegraph-preload|livegraph-refresh` (graph.rs:198-207);
   refresh runs scip-typescript (RMAP_SCIP_TYPESCRIPT or PATH; livegraph_refresh.rs:58,74-78) on TS partitions
   only; index/refresh never touch it; in-memory only (lost on restart). The audit harness never sends the dev
   command, scip-typescript is not on PATH, 21/28 repos aren't TS → root posture is a CONSTANT carrying zero
   information about the repo.
   SMALLEST FIX: keep D-T6's MEET in JSON; human render prints the Half-B value with a qualifier when Half A is
   cold ("Posture: Fresh (snapshot) — current-state: unavailable (LiveGraph not loaded; requires
   scip-typescript + rmap dev livegraph-refresh)"); non-TS repos "not applicable (no TypeScript partition)".
   Touches a ratified decision → human ruling.
B. doctor: rgr/src/commands/doctor/seed.rs:27-31 unavailable → passed: true ("seeding is optional"); :138 all
   states passed: true; tone follows passed (doctor/mod.rs:81-94); the only [note] override is
   apply_degraded_enrichment_tone (summary.rs:131) — no seed equivalent. Only repo-graph-doctor.txt:35 shows
   "unavailable" (daemon writing); six say "not built yet"; 21 "present". Not a regression.
   FIX: ProbeTone::Note when seed.state ∈ {unavailable, absent, degraded} (mirror the enrichment tone); unit
   test like probe_output_default_tone_follows_health (mod.rs:491).

## Smoke manifest — linux in legacy_repos, absent from passed/failed/skipped
- SITE: scripts/smoke-validation-repos.sh:301-312 SMOKE_SKIP filter rebuilds ALL_NAMES/PATHS/CATEGORIES and
  drops the repo; never appends to SKIPPED_REPOS (populated only at :368-371 for a missing path). LEGACY_JSON
  (:521-524) built from the PRE-filter discovery list.
- FIX: `else SKIPPED_REPOS+=(name)` in the :301 block (move the init from :351 above :301); optional skip_reason
  map (env:SMOKE_SKIP vs path-missing). Add a summary check passed∪failed∪skipped == internal∪legacy (:580).
- VERIFICATION: SMOKE_SKIP=linux SMOKE_ONLY="linux leveldb" dry run → passed:[leveldb], skipped:[linux].
