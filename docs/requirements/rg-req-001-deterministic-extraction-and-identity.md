<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-001",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "the-core" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "agent-operating-model" },
    { "kind": "document-section", "path": "docs/architecture/artifact-contract-model.md", "fragment": "truth-classes" },
    { "kind": "document-section", "path": "docs/slices/symbol-identity-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/cpp-declarators-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-001-L01", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L02", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L03", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L04", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L05", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L06", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L07", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L08", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L09", "parentId": "RG-REQ-001" },
    { "id": "RG-REQ-001-L10", "parentId": "RG-REQ-001" }
  ]
}
-->
# RG-REQ-001 — Extraction is deterministic, identity is stable, and nothing extracted is silently lost

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — The Core](../VISION.md#the-core) commitment 1 ("facts computed from source, reproducible, never model output; the same snapshot always yields the same answer, and the answer has provenance"); the Certainty Model's Layer 0 (file inventory, symbol extraction, structural edges, unresolved-edge preservation with classification, stable keys); [VISION — Agent Operating Model](../VISION.md#agent-operating-model) decision rules 3–4 (never compare non-comparable snapshots; never erase superseded records); [Artifact contract model — Truth Classes](../architecture/artifact-contract-model.md#truth-classes); `docs/architecture/schema.txt` and `versioning-model.txt`; the ratified contracts of KEY-NAMESPACE-REPO-RELATIVE-1, INDEX-BASIS-1, IS-TEST-RUST-1/CPP-1, SYMBOL-IDENTITY-1, CPP-DECLARATORS-1.

## High-level requirement

Indexing a repository shall produce a Layer-0 record — files, symbols, structural edges and the references it could not bind — that is reproducible from the same source, carries a stable identity an agent can hand back to the product, states which commit it describes, and preserves every unresolved reference with a named classification rather than dropping or guessing it.

**Scope:** the index-time substrate (extractors, resolver, storage schema, identity and provenance). How the substrate is bound into relationships is RG-REQ-005/006; how it is rendered is RG-REQ-003 and RG-REQ-012.

**High-level acceptance:** all L entries hold on the fixture parity corpora and the smoke corpus; a reproducibility proof (L09) exists as an executed check, not an inference.

## Low-level requirements

### RG-REQ-001-L01 — Symbol extraction per shipped language

For every file classified TypeScript/JavaScript, Rust, Java, Python, C or C++, an index shall emit one FILE node and one SYMBOL node per declared symbol (function, method, class, constant, type) carrying path, `line_start`/`line_end`, kind, subtype and visibility. A C/C++ function is named by its declarator identifier, never by a macro token; where tree-sitter recovery of a macro-bearing member leaves two parenthesised identifiers between the type and the body or terminator, the node is stored once, marked `identity: undetermined` with both candidates (RG-REQ-002-L11), never named by the macro alone and never dropped — a field whose trailing macro argument names a data member of the class is a data member; a forward declaration is stored as a declaration (`metadata_json.forward_decl`), not a definition (amended 2026-09-23, D-CERTAINTY-MARK-1 after D-CAM-01 R1–R4).

**Verification criterion:** `rust/crates/repo-index/tests/integration.rs` (`index_rust_crate_extracts_symbols`, `index_python_extracts_symbols`, visibility and language-isolation cases); `indexer/tests/parity.rs::parity_against_shared_indexer_fixture_corpus`; CPP-SPAN-FIDELITY-1 and CPP-DECLARATORS-1 tests (`forward_decl_classification_reads_the_stamped_key`; macro-name cases); field: vcmi `find CGHeroInstance` renders the definition first and six `(decl)` rows.

**Evidence (v0.18.0):** OBSERVED MET for the six languages.

### RG-REQ-001-L02 — Structural edges with a non-null target and a stated resolution

An index shall emit IMPORTS, CALLS, IMPLEMENTS and INSTANTIATES edges; every served edge shall have a non-null target node, a `resolution` of `static | dynamic | inferred`, and the extractor that produced it. An edge is emitted only when the binding is supported by evidence (RG-REQ-005-L02); an evidence-less binding is a defect even when it happens to be right.

**Verification criterion:** `indexer/tests/parity.rs`; `repo-index/tests/integration.rs` (`cross_crate_use_resolves_to_defining_file`, `cross_package_java_import_resolves_by_suffix`, `index_python_imports_resolve`); `indexer/src/resolver.rs::ambiguous_name_stays_unresolved`; corpus assertion: zero receiver-bearing CALLS self-loops (RG-REQ-005-L01).

**Evidence (v0.18.0):** PARTLY MET — RC-1 (C++ receiver calls bound to the enclosing class), RC-2 (Python `self.` never binds), RC-3 (C++ IMPLEMENTS anchored on the FILE node), RC-10 (per-module include roots) are open.

### RG-REQ-001-L03 — Unresolved references are preserved and classified, never dropped or guessed

Every extracted reference that cannot be bound shall persist as an `unresolved_edges` row with `target_key`, `type`, `category`, `classification`, `classifier_version`, `basis_code`, source anchor and `observed_at`; an ambiguous or wildcard reference stays unresolved with a named basis and is counted. A wrong binding that removes a row from the unresolved set is a violation of this L, not a resolution gain.

**Verification criterion:** `storage/src/trust_impl.rs` (`count_unresolved_by_classification_groups_correctly`, `query_unresolved_edges_returns_typed_samples_with_visibility`, `unresolved_call_sites_returns_per_site_caller_and_raw_target`); `classification/tests/parity.rs`; `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` (a new category needs a reader mapping).

**Evidence (v0.18.0):** PARTIALLY MET — the storage/classification contract holds; RC-1 violates this L on every C++ repo (155 leveldb rows left the unresolved set into self-loops). Review finding 2026-09-12: not MET while a wrong binding removes unresolved rows.

### RG-REQ-001-L04 — Stable keys are format v2 and repo-relative

Every node shall carry a `stable_key` of the form `<repo_uid>:<repo-relative-path>[#<qualified_name>]:<KIND>[:<subtype>]`, unique within a snapshot and identical for the same construct regardless of the package root through which the file was reached. A change to the key format is an identity change and requires the versioning contract and migration path first.

**Verification criterion:** `cpp-extractor::file_node_has_correct_stable_key`; `java-extractor::constructor_stable_key_no_duplication`, `nested_constructor_stable_key`; `state-bindings/tests/stable_key.rs`; `idx_nodes_snapshot_key` UNIQUE.

**Evidence (v0.18.0):** OBSERVED MET within a snapshot; across snapshots the `:dupN` ordinal is AST-preorder and renumbers on insertion (TECH-DEBT "Symbol Identity — Stable Key Contract") — a known limit, stated, not a semantic identity model.

### RG-REQ-001-L05 — Any identity the product prints can be handed back to it

A symbol row printed by `find` shall resolve unchanged through `explain`, `callers` and `callees`: the resolver accepts the exact stable key, the full qualified name, a qualified suffix (`<sep><query>`, `<sep>` in `::`/`.`) and the short name; more than one hit lists up to five candidates with their files instead of "not found".

**Verification criterion:** `agent/tests/explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver`; `agent/tests/orient_symbol_focus.rs::symbol_resolves_by_stable_key`; `storage/src/queries.rs` resolve_symbol suffix tests.

**Evidence (v0.18.0):** OBSERVED MET (SYMBOL-IDENTITY-1, f5cfe1e). Preservation: `resolve_symbol` is delegated to SQLite by the orient decorator while `resolve_symbol_name` stays name-only for orient and the LiveGraph parity certificate — the two are not to be unified.

### RG-REQ-001-L06 — Facts state which commit they describe and how far the tree has moved

`orient`, `check` and `explain` shall render `index basis: <sha7>` and the drift since it ("HEAD is N commits ahead; M files changed, K of them indexed"); a non-git directory, an unborn repository, a pre-basis snapshot and a git failure each render a named unknown with its reason — never a blank and never a zero. The product's own exhaust (RG-REQ-011-L07) never counts as drift. A snapshot records the toolchain stamp written by its latest full index or refresh (`toolchain_json`: the per-language extractor versions and the indexer version); the stamp does not assert that this toolchain produced every copied-forward fact. On the first request for a repository in a daemon process, the daemon compares the running versions with the latest ready snapshot's stamp. When they differ, or the stamp is missing, malformed or structurally incomplete, it schedules one background FULL index of that repository unless `RMAP_AUTO_REINDEX=off`; the job starts when no index, refresh or rebuild of that repository is running and no other automatic re-index is running, without requiring another request. The full index creates a new snapshot, so queries keep serving the previous ready snapshot until the new one is ready; it never uses the in-place `rmap repo rebuild`. A failed automatic re-index is not retried within the process. While the stamp differs or is unreadable, `orient` and `check` render one line beside the index-basis line. A differing stamp renders `index toolchain differs from running rmap (<component> <snapshot_version> → <current_version>[, …])`; an unreadable stamp renders `toolchain status unknown (<reason>)`. The suffix is `— re-index queued`, `— re-indexing in the background`, `— run rmap repo rebuild <path>` or `— re-index failed (<reason>); run rmap repo rebuild <path>`, according to the re-index state. In the `check --json` envelope's `value`, `toolchain_staleness` is `{"state":"current"}` or has `state` equal to `stale` or `unknown` and `reindex` equal to `queued`, `running`, `disabled` or `failed`. A `stale` value has a non-empty `differences` list of `{"component":<name>,"snapshot_version":<stamped>,"current_version":<running>}` objects; an `unknown` value has `reason` for the unreadable stamp. Either value has `reindex_failure` with the job-failure reason if and only if `reindex` is `failed`. The line is a signal, not an account of which facts are old and not a verdict: it does not change `check`'s result or exit code. Toolchain versions are not an invalidation input for refresh; a refresh stamps the running toolchain and does not track facts it copied forward. When the automatic full index is triggered and completes successfully, it re-extracts the repository into the new ready snapshot (human rulings 2026-09-26, D-STALE-SIGNAL-1: "how many times do I have to say I don't care about stale indexes and that's a signal to either delete and reindex or just refresh if possible"; "yes, lazy auto-reindex on first use"). (Supersedes the carried-forward provenance rule of D-REFRESH-STALE-2 and the INCOMPLETE verdict of INPUT-9.)

**Verification criterion:** `daemon-runtime/tests/index_basis.rs` and `index_basis_failures.rs`; `agent/src/check/reduce.rs::index_drift_incomplete_when_basis_unknown`; field: every orient/explain capture ends with `index basis: <sha>; working tree clean (no drift since index)` or the drift line. Toolchain staleness: TOOLCHAIN-STALENESS-1 shall prove, through the daemon with an isolated state root: a store whose stamp differs from the running versions starts exactly one background full index on the first request, keeps serving the previous ready snapshot with the `re-indexing in the background` line while it runs, and reports `current` with no line once it completes; a second request during the run starts no second index; a first request arriving during an index or refresh of that repository, or while another repository's automatic re-index runs, renders `re-index queued` and the queued job starts without a second request; a request during `rmap repo rebuild` retains the named rebuilding refusal and queues a post-rebuild stamp check without requiring another request, starting an automatic full index only if the resulting ready stamp differs or is unreadable; `RMAP_AUTO_REINDEX=off` starts none and renders the rebuild suffix; a failing re-index renders the failed suffix with its reason and is not retried in the process; a missing, a malformed and a structurally incomplete stamp each render `unknown` and start the re-index; the three `check --json` shapes; `check`'s verdict and exit code unchanged in every state; the exact human lines in `rgr` orient and check presentation tests. Field: a store indexed by v0.19.0, first queried by the next binary, re-indexes in the background and then reports current.

**Evidence (v0.18.0):** OBSERVED MET for index-basis and drift reporting only; lazy automatic re-index and its toolchain-status reporting remain unverified.

### RG-REQ-001-L07 — Test code is partitioned structurally

`files.is_test` shall be set from the test markers the language defines (Rust `#[cfg(test)]` inclusion chain, C++ gtest markers) and the routing path patterns (`__tests__`, `.test.`, `.spec.`, `test/`, `tests/`), with `testsuite/` added by TEST-EDGE-SCOPE-1A; adding another pattern is a catalog change. A file whose path contains `test`, `tests`, `testing`, `tester`, or `testutil` as a case-insensitive token separated by `/`, `.`, `_`, `-`, or a CamelCase boundary (so `TestCaller.h` qualifies), but matches no listed convention or marker (a test helper, a test framework's own headers, a test utility shipped in a production package), has test status UNDETERMINED (RG-REQ-002-L11): it stays in the production partition, surfaces that partition by test status state the count ("N files whose test status can't be determined — open them and look inside"), and `explain <file>` says "test status: can't determine — open it and look inside". The fact survives a no-change refresh unchanged. Every ranked or counted surface that partitions by test status reads it (RG-REQ-004-L12, RG-REQ-007-L03, RG-REQ-009-L01, RG-REQ-010-L06). (Amended 2026-09-26 by the human's ruling, D-TEST-UNDETERMINED-1: "pick whatever is easiest and just mark them for opening — with thoughtful wording like 'can't determine — open it and look inside'".)

**Verification criterion:** `repo-index/tests/is_test_rust_reproduction.rs` and `is_test_cpp_reproduction.rs` (`*_drives_is_test_not_filename`, `structural_is_test_survives_no_change_refresh`); to be added by TEST-EDGE-SCOPE-1A: a routing test per listed convention including `testsuite/`, an undetermined test per test-word form, and the rendered count and per-file wording; field: poco `**/testsuite/**` flagged test (788 files across poco at v0.19.0), poco `CppUnit/include/CppUnit/Test.h` and leveldb `util/testutil.cc` undetermined.

**Evidence (v0.18.0):** OBSERVED MET for the existing stored `is_test` fact and its seed and surface consumers; `testsuite/` classification, UNDETERMINED marking, and the required wording remain unverified; complexity ranking remains open (RC-9).

### RG-REQ-001-L08 — Generated and vendored files are marked at index time

A file produced by a generator (first-lines marker such as `@generated`, `DO NOT EDIT`, a Bison or tree-sitter banner) or living under a vendored path segment (`vendor`, `third_party`, `dependencies`, `node_modules`, `site-packages`, …) shall be flagged in the file inventory (`files.is_generated`; the vendored predicate in an inner crate the core can call). Any ranked surface that drops such rows states how many it dropped and the flag that includes them.

**Verification criterion:** unit tests for the marker sniff and the segment rule; corpus: `SELECT count(*) FROM files WHERE is_generated=1` > 0 on poco and codegraph; the docs, seeds and complexity surfaces read the same flags.

**Evidence (v0.18.0):** NOT MET — RC-9: every construction site hard-codes `is_generated: false` (0 rows on every store); the vendored list lives in an adapter crate and lacks `dependencies`. Queue Q7. Populating the flag changes the seed corpus (a stated behaviour change) and requires a reindex.

### RG-REQ-001-L09 — The same source yields the same graph; non-comparable snapshots say so

Indexing the same tree twice shall produce an identical graph (same node and edge sets, same stable keys) and byte-identical deterministic surfaces after normalizing snapshot identity and freshness tokens; each snapshot shall record the toolchain that produced it (schema version, per-language extractor versions, indexer version, `stable_key_format`, measurement semantics); comparing snapshots whose compatibility-relevant versions differ shall yield NOT_COMPARABLE, never a number.

**Verification criterion:** an executed reindex-twice equality check on a fixture (to be added — none exists); `scripts/byte-compare-five-surfaces.sh` against a baseline binary; `gate/src/compute.rs::quality_assessment_not_comparable_causes_incomplete_exit_2`; a write-side test that `toolchain_json` is stamped at index time (to be added).

**Evidence (v0.18.0):** UNKNOWN for whole-index reproducibility — the load-bearing VISION commitment has only proxy coverage (fixture parity harnesses, pure-function determinism). This is the first verification gap to close for this H.

### RG-REQ-001-L10 — Every persisted fact family declares its certainty contract

Each artifact family shall have exactly one truth class, a refresh policy, an identity basis, a degradation mode and a provenance kind, registered in code; a Layer 2–4 family shall never be served with Layer-0 provenance; a new family fails the registry tripwire until it is described.

**Verification criterion:** `artifact-contracts/tests/completeness.rs` (`every_family_has_a_contract`, `family_count_matches_expected`, `stable_families_count`); `artifact-contracts/tests/coherence.rs` (`extracted_facts_have_direct_provenance`, `layer_ordering_is_consistent`, `recompute_policy_matches_truth_kind`).

**Evidence (v0.18.0):** OBSERVED MET at the registry level.

## Preservation obligations named by the ratifying specifications

- Stable key format v2 and `(snapshot_uid, stable_key)` uniqueness; `stable_key_format` as a compatibility version; declarations match only on equal `stable_key_format`.
- Schema shape of `files`/`nodes`/`edges`/`unresolved_edges`; `edges.target_node_uid NOT NULL`; schema changes are additive migrations.
- The canonical edge-type and resolution-value vocabularies; `UnresolvedEdgeCategory` serialized names.
- Obligation semantic-match tuple `(method, target, threshold, operator)` and immediate-prior-version inheritance (`versioning-model.txt`).
- `check` JSON is CI-facing: additive conditions only; `STALE_FILES` retained beside `UNPARSED_FILES` for one release (INDEX-BASIS-1).
- Tree-sitter grammar versions: a bump is a DECISION_REQUIRED.
- Non-target extractors stay byte-stable under any single-language fix.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
