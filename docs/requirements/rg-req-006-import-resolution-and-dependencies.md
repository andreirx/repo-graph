<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-006",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "primary-use-case" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/slices/import-resolution-rust-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/import-resolution-java-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/deps-classifier-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/honesty-gate-1.md", "fragment": "5-definition-of-done" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-006-L01", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L02", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L03", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L04", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L05", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L06", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L07", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L08", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L09", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L10", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L11", "parentId": "RG-REQ-006" },
    { "id": "RG-REQ-006-L12", "parentId": "RG-REQ-006" }
  ]
}
-->
# RG-REQ-006 — Imports resolve to the file that defines them, and a dependency is called used only on evidence

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Primary Use Case](../VISION.md#primary-use-case) ("how modules relate to each other"); [VISION — Honesty Rules](../VISION.md#honesty-rules); the Certainty Model's Layer 0 (unresolved-edge preservation with classification); the ratified contracts of TS-IMPORT-RESOLUTION-1 and the IMPORTS-* family, TYPE-ONLY-IMPORTS-1, IMPORT-RESOLUTION-RUST-1, IMPORT-RESOLUTION-JAVA-1, DEPS-LIST-REWRITE-1, DEPS-ATTRIB-2, HONESTY-GATE-1, DEPS-CLASSIFIER-1/1B; the C include-resolution milestone v1.1; the v0.18.0 root causes RC-6, RC-7, RC-10.

## High-level requirement

An import statement in any shipped language shall resolve to the file that defines its target when the language's rules determine one, shall otherwise be preserved unresolved with a named basis and counted wherever a resolution-dependent zero renders, and `rmap deps list` shall call a declared dependency used, type-only, config-referenced or unobserved only on the evidence of the importing code of the same ecosystem — naming every ecosystem present and every manifest it did or did not read.

**Scope:** import resolution stages, the unresolved-edge classifier, dependency manifests and `deps list`. Module-edge derivation from resolved imports is RG-REQ-004.

**High-level acceptance:** all L entries hold on the fixture corpora and on django, storybook, kafka, grpc-java, hadoop, poco, gstreamer, leveldb, vcmi, repo-graph.

## Low-level requirements

### RG-REQ-006-L01 — Rust `use <crate>::…` resolves to the defining file

Split on `::`, canonicalise `_`→`-`, match a declared Cargo package, then probe `<crate_root>/src/<segs>.rs` → `<segs>/mod.rs` → shorten → `src/lib.rs` → `src/main.rs`; first hit in the identity map wins; a pure function over the file list. Custom `[[bin]] path`, `package = "…"` renames and `[lib] path` stay unparsed and stated with corpus counts.

**Verification criterion:** `indexer/src/resolver.rs` `rust_crate_import_*` family; `rgr/src/presentation/modules_list_tests.rs::two_crate_fixture_renders_a_to_b_edge_verbatim`; field: repo-graph 129 cross-crate edges, leveldb byte-stable.

**Evidence (v0.18.0):** OBSERVED MET (c71879f; unresolved 7,528 → 4,789).

### RG-REQ-006-L02 — Java `import a.b.C` resolves by unique path suffix; wildcard and ambiguity are named and counted

Try `a/b/C.java`, shorten the trailing segment for nested classes; one match resolves; several → basis `ambiguous_suffix`; `pkg.*` → basis `wildcard_import`; the suffix must not match mid-filename; Gradle `projectDir` relocations are honoured; Maven manifests render "present but not parsed (N pom.xml)".

**Verification criterion:** `indexer/src/resolver.rs` `java_suffix_*` and `java_import_wildcard_is_named_basis`; `modules_list_tests.rs` gradle_projectdir cases; field: kafka 218 module edges, hadoop hdfs → common 11,874, grpc-java 193.

**Evidence (v0.18.0):** OBSERVED MET (dc07527); grpc's edge list uses a different identifier space than its table (RG-REQ-004).

### RG-REQ-006-L03 — C/C++ includes resolve through per-module include roots

Candidate include roots shall be derived from the indexed file list — every directory named `include`/`inc` at any depth — in addition to same-directory and `--include-root`; ambiguity stays unresolved and counted; system headers stay unresolved; adding roots may turn Unresolved into Resolved or Ambiguous, never flip an existing resolution.

**Verification criterion:** `indexer/src/include_resolver.rs` tests incl. `no_sibling_directory_magic` and `no_suffix_guessing` (no guessing is introduced); field: poco `modules list` shows `Net → Foundation` (616) among ~50 pairs, unresolved 13,703 → ~3,810 (3,442 system headers); duckdb +2,277; OpenXcom +153; leveldb/vcmi/nginx/swupdate/sqlite byte-stable.

**Evidence (v0.18.0):** NOT MET — RC-10 (never worked; the v1.1 milestone deferred build-system include detection; three root-anchored literals only). Queue Q3 increment 1. Requires reindex; poco gains ~6× IMPORTS edges (cost to measure). The suffix fallback for meson-style layouts is L11, a separately decidable increment.

### RG-REQ-006-L04 — TypeScript and Python resolve what the language resolves, leaving usage evidence

TS: tsconfig `paths` (with `extends`), relative (extensionless, `index.ts`), then package; aliased named imports look up the imported name; `import * as X` scopes member lookup to the module and a default import does not; a bare-package import emits an IMPORTS edge (unresolved external candidate); `require('x')` literals bind; `import type` binds flagged type-only and the flag survives the storage read. Python: the IMPORTS `target_key` agrees with the binding specifier (dotted); extensionless and `__init__.py` shortcuts.

**Verification criterion:** `indexer/src/resolver.rs` (`aliased_named_import_uses_imported_name_for_lookup`, `namespace_import_member_resolves_to_target_module`, `default_import_member_does_not_resolve_by_bare_method_name`, `file_resolution_*`, `import_type_only_reads_injected_metadata`, `…malformed_carrier_is_unreadable_not_absent`).

**Evidence (v0.18.0):** OBSERVED MET for the stages (TS-IMPORT-RESOLUTION-1, TYPE-ONLY-IMPORTS-1, DEPS-CLASSIFIER-1B); the repo-wide TS resolution RATE is UNKNOWN (no probe isolates it).

### RG-REQ-006-L05 — Unresolved imports are preserved with a classification and counted in every resolution-dependent zero-state

`modules list` renders `N cross-module dependencies (M imports unresolved)`; the "all imports are intra-module" hint appears only when M = 0; `cycles`' zero branch names its population and the resolved edge count; a failed read renders unavailable-with-reason, never zero.

**Verification criterion:** `modules_list_tests.rs` (`list_render_empty_edges_is_zero_state`, `list_render_absent_edges_field_is_unavailable_with_reason`, `list_render_unresolved_read_failure_does_not_blame_older_daemon`, hint-suppression cases); `cycles/tests.rs` zero-state tests.

**Evidence (v0.18.0):** OBSERVED MET (audit keep-and-imitate: the resolution-gap note).

### RG-REQ-006-L06 — A declared dependency is used when any of its modules is imported, via one shared head reduction

Python `a.b.c` → `a` (PEP 503 normalised), npm `@scope/pkg/sub` → `@scope/pkg`, Rust `a::b` → `a`, Java unchanged — ONE function used by the index-time classifier and the query-time normaliser. The manifest-side `-`→`_` transform is a different family and stays separate.

**Verification criterion:** `classification/src/dep_reduce.rs` tests; `unresolved_classifier.rs::asgiref_call_site_classifies_external_via_reduced_binding_specifier`; `module-queries/src/deps/normalize.rs` family; field: django asgiref used (51 import sites, 131 call sites); storybook used 16 → 39.

**Evidence (v0.18.0):** OBSERVED MET (09071cf, d333214).

### RG-REQ-006-L07 — Every dependency row carries a computed basis

Rows render `used (N import sites, M call sites)`, `type-only import`, `referenced in config only (<file>)`; `no static import found` renders only when the package head appears in no binding, no import edge and no unknown-classified edge; the word "unused" renders only with an established basis; a fixture asserting a negative must be ground-truthed across every source extension.

**Verification criterion:** `rgr/src/presentation/deps_list.rs` (`asgiref_reads_used_with_computed_basis_tzdata_stays_unobserved`, `established_basis_renders_the_word_unused`, `unknown_resolution_state_renders_as_unknown_not_clean`, `false_zero_cannot_render_as_full_coverage`); `module-queries/src/deps/reconcile.rs` (`import_sites_and_call_sites_split`, `declared_type_only_import_is_type_only_not_unobserved`, `value_use_beats_type_only`, `manifest_unavailable_degrades_to_unknown`).

**Evidence (v0.18.0):** OBSERVED MET (deps list A-/A-/A-/A on Rust/TS).

### RG-REQ-006-L08 — The observed side is partitioned by the importing file's ecosystem, and the skipped references are named

Under `--ecosystem <eco>`, the observed references shall be gated by the same `path_ecosystem` predicate that gates the declared side; references from other ecosystems are counted and named ("13,956 of 13,967 external references are Python-file imports, outside this npm view — see `--ecosystem python`"), never folded into "outside a parsed manifest scope"; the `none-detected` view is exempt.

**Verification criterion:** a compose test asserting the observed-side gate and the `cross_ecosystem` count (to be added); field: django `--ecosystem npm` undeclared 102 → 0, builtins unchanged; repo-graph "294 of 17654", FRAKTAG "30 of 733", storybook provably byte-stable from the stores.

**Evidence (v0.18.0):** NOT MET — RC-6 (the observed filter never existed; visibility regressed via DEPS-CLASSIFIER-1B). Queue Q5.

### RG-REQ-006-L09 — A reader-absence claim names only languages without a reader and lists the manifests that were parsed

The "no dependency-manifest reader for <languages> on this build" sentence shall name only languages for which no reader exists; when manifests of other ecosystems were parsed, the header lists them with counts and the flag that renders them ("13 manifests of other ecosystems were parsed (4 cargo, 1 npm, 8 gradle) — see `deps list --ecosystem cargo`"); the ≥10%-of-source materiality gate on the default view stays.

**Verification criterion:** a unit test on the reader note filter (to be added); field: gstreamer names C/C++ only and points at cargo/npm/gradle; `--ecosystem cargo` renders its 4 Cargo modules; nginx/poco/OpenXcom/sqlite/leveldb byte-identical.

**Evidence (v0.18.0):** NOT MET — RC-7 (never worked: the language list is unfiltered; the same store row records 13 parsed manifests). Queue Q5.

### RG-REQ-006-L10 — Every materially present ecosystem is named; an unreadable manifest kind is a stated capability limit

Attributed ecosystems render; a secondary material one is named with its manifest count and flag; Maven renders "not parsed on this build (N pom.xml present)"; a manifest path rendered is always the file actually parsed; "governs no indexed source" renders only when the subtree truly has zero indexed files; `modules_without_manifest_context: 0` beside nonzero external imports is unrepresentable.

**Verification criterion:** `deps_list.rs` (`maven_capability_limit_names_the_gap_and_suppresses_downgraded`, `present_but_unparsed_manifest_renders_honest_no_record_never_assumed_no_source`, `provenance_unavailable_renders_note_not_fabricated_path`); `compose.rs` attach tests.

**Evidence (v0.18.0):** OBSERVED MET for Maven and provenance; FAILS on gstreamer (L09) and leaks on django (L08).

### RG-REQ-006-L11 — A unique multi-segment path-suffix fallback for includes no root resolves

STATUS: RATIFIED 2026-09-14 (human, option A): adopted for specifiers of two or more path segments, unique match only; a non-unique match stays unresolved; single-segment names never guess. Its own slice, separate from CPP-INCLUDE-ROOTS-1.

Where no include root resolves a ≥2-segment specifier, a UNIQUE path-suffix match over the indexed file list shall resolve it (the `build_java_suffix_index` shape); a non-unique match stays unresolved; single-segment specifiers are never suffix-matched.

**Verification criterion:** a resolver test that a unique 2-segment suffix resolves and a duplicated one stays unresolved; `no_suffix_guessing` stays green for single segments; field: gstreamer unresolved 22,392 → ~12,600 with cross-module edges; vcmi +300; duckdb +1,618; nothing that resolves today changes.

**Evidence (v0.18.0):** NOT MET — RC-10's measured second cause (meson subprojects, headers never under `include/`).

### RG-REQ-006-L12 — `rmap imports <file>` shows the file's imports as resolved rows and classified unresolved rows

`imports <file>` shall list each resolved import with its target file and each unresolved import with its classification and basis, state the resolved/unresolved counts, render a zero-state that names the file's language and the reader coverage, and carry the same facts under `--json`.

**Verification criterion:** `rgr/tests/cli_out_3_drilldown.rs` imports cases (human/JSON pairs); a test asserting unresolved rows carry their classification (to be added); the CLI contract `docs/cli/rmap-contracts.md` imports section.

**Evidence (v0.18.0):** NOT MET — the default per-file listing is an INNER JOIN on the target node (`storage/src/queries.rs:1726-1771`), so unresolved/external specifiers held in `unresolved_edges` are ABSENT from `imports <file>` while the same file's `map` section splits resolved from external/unresolved by contract; no test asserts per-row classification; the JSON pair test (`cli_out_3_drilldown.rs`) is `#[ignore]`d and asserts key presence only (extraction 2026-09-12; review finding: the CLI outcome had no obligation).

## Preservation obligations named by the ratifying specifications

- Wire protocol additive only; storage schema additive migrations only (the `is_type_only` plumbing is the precedent); exit codes.
- Manifest parsers' output shapes additive; Maven parsing out of scope (name the absence).
- A new resolver stage runs only when earlier stages miss; non-target extractors byte-stable.
- `module_file_ownership` is the attribution basis (query over it; do not change ownership here).
- The ≥10% secondary-ecosystem materiality gate; trust computation and witness/union/reconciliation; the CYCLES-B byte-parity certificate.
- `--include-root` remains the escape hatch; poco's 3,442 system-header includes stay unresolved.
- `config.rs`'s `-`→`_` manifest-name transform stays separate from the shared head reduction.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
