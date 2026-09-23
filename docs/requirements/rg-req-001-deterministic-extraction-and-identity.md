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

`orient`, `check` and `explain` shall render `index basis: <sha7>` and the drift since it ("HEAD is N commits ahead; M files changed, K of them indexed"); a non-git directory, an unborn repository, a pre-basis snapshot and a git failure each render a named unknown with its reason — never a blank and never a zero. The product's own exhaust (RG-REQ-011-L07) never counts as drift. A snapshot also states which TOOLCHAIN produced it. When the running binary's per-language extractor version or indexer version differs from the snapshot's recorded `toolchain_json`, `orient` and `check` shall render one line per stale toolchain family beside the index-basis line: `<family> facts from <snapshot_version> (current <current_version>) — run rmap repo rebuild <path> to refresh`. `check` shall report `TOOLCHAIN_STALENESS` as `INCOMPLETE` when any family is stale or the state is unknown. In the `check --json` envelope's `value`, `toolchain_staleness` shall be exactly one of three shapes: `{"state":"current"}`; `{"state":"stale","families":[…]}` where `families` is a non-empty list with one `{"family":<name>,"snapshot_version":<recorded>,"current_version":<running>}` object per stale family — a per-language extractor family by its language name (e.g. `{"family":"C++","snapshot_version":"cpp-core:0.1.0","current_version":"cpp-core:0.2.0"}`) and, when the indexer version differs, the family `indexer` (e.g. `{"family":"indexer","snapshot_version":"indexer:1.0.0","current_version":"indexer:1.1.0"}`, rendered `indexer facts from indexer:1.0.0 (current indexer:1.1.0) — run rmap repo rebuild <path> to refresh`); or `{"state":"unknown","reason":"<reason>"}`. Missing, malformed or structurally incomplete `toolchain_json` is `unknown`, renders `toolchain status unknown (<reason>) — run rmap repo rebuild <path> to refresh`, and is never treated as current. Except when a changed-config widening re-extracts its scope, delta refresh copies forward files whose content hashes match; toolchain versions are not an invalidation input. Toolchain status shall conservatively reflect the provenance of the facts served, not merely the running toolchain stamped on a refresh snapshot. The refresh snapshot's `toolchain_json` retains `extractors` and `indexer` as the running toolchain (which extracted the changed files). When files are copied forward, the additive key `carried_forward` contains the union of the parent's stamped and carried-forward versions only for the extractor families represented by copied files, and the corresponding indexer versions whenever any file is copied. An extractor family is evaluated only when the served snapshot contains files of that family; an evaluated family is stale when its running version differs from its snapshot stamp or any retained version for that family. The indexer is stale when its running version differs from its snapshot stamp or any retained indexer version. For each stale family, `snapshot_version` is the lowest differing semantic version (the version after the family's `name:` prefix), breaking semantic-version ties by the full version string. Missing, malformed, structurally incomplete or unparseable required provenance yields `unknown` with a reason, never `current`. A refresh cannot report `current` merely because it stamped the running toolchain. A full index — `rmap repo rebuild <path>` (RG-REQ-011-L04) — writes no `carried_forward` and so clears carried-forward staleness; until it runs the stale facts are served with that line, never silently (RATIFIED 2026-09-23 by the human: option B of D-REFRESH-STALE-1 and option B of D-REFRESH-STALE-2).

**Verification criterion:** `daemon-runtime/tests/index_basis.rs` and `index_basis_failures.rs`; `agent/src/check/reduce.rs::index_drift_incomplete_when_basis_unknown`; field: every orient/explain capture ends with `index basis: <sha>; working tree clean (no drift since index)` or the drift line. Toolchain staleness: TOOLCHAIN-STALENESS-1 shall prove the stale, current and unknown (`toolchain_json` absent and malformed) states through daemon `orient` and `check` payloads; `rgr` orient and check presentation tests shall prove the exact human lines; `check --json` tests shall prove the three exact `toolchain_staleness` shapes and exit 2 for stale or unknown; a daemon test in which a binary whose C++ extractor version differs from the store's refreshes a repository with one unchanged C++ file shall show `orient`, `check` and `check --json` stale (the carried-forward version as `snapshot_version`, exit 2) until `rmap repo rebuild <path>` completes; a two-refresh test (index under version A, refresh under B changing one file and copying another, refresh under A copying both) shall still report B as a stale carried-forward version; a refresh that deletes the last file of one family while copying a file of another shall not report the deleted family; a malformed parent `carried_forward` shall yield `unknown`; a full index of a tree with no file of some family, queried by a binary whose extractor for that family differs from the stamp, shall not report that family; and a full index shall write no `carried_forward`; field: a refresh of a v0.19.0 store by the next binary retains stale status until `rmap repo rebuild <path>` completes.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-001-L07 — Test code is partitioned structurally

`files.is_test` shall be set from language structure (Rust `#[cfg(test)]` inclusion chain, C++ gtest markers, the per-language conventions recorded in the routing rules), never from a filename alone, and shall survive a no-change refresh unchanged. Every ranked or counted surface that partitions by test status reads this fact (RG-REQ-007-L03, RG-REQ-009-L01, RG-REQ-010-L06).

**Verification criterion:** `repo-index/tests/is_test_rust_reproduction.rs` and `is_test_cpp_reproduction.rs` (`*_drives_is_test_not_filename`, `structural_is_test_survives_no_change_refresh`).

**Evidence (v0.18.0):** OBSERVED MET as a stored fact; consumed by seeds and surfaces, not yet by complexity ranking (RC-9).

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
