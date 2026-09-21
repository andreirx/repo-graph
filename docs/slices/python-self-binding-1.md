<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "PYTHON-SELF-BINDING-1",
  "baselinePath": "docs/requirements/baselines/PYTHON-SELF-BINDING-1-INPUT-2.json",
  "parentRequirementIds": [
    "RG-REQ-001",
    "RG-REQ-005",
    "RG-REQ-006",
    "RG-REQ-011"
  ],
  "implements": [
    "RG-REQ-005-L03",
    "RG-REQ-005-L02",
    "RG-REQ-001-L03"
  ],
  "preserves": [
    "RG-REQ-005-L01",
    "RG-REQ-006-L01",
    "RG-REQ-006-L02",
    "RG-REQ-006-L04",
    "RG-REQ-005-L07",
    "RG-REQ-005-L09",
    "RG-REQ-001-L09",
    "RG-REQ-011-L06"
  ],
  "preservationObligationIds": [
    "P-PSB-01",
    "P-PSB-02",
    "P-PSB-03",
    "P-PSB-04",
    "P-PSB-05"
  ],
  "changes": [],
  "acceptanceBoundary": "The `explain`, `callers` and `trust` outputs of the candidate rmap against the before binary on three pre-provisioned isolated `-before` roots (django, leveldb, FRAKTAG; old indexes) and on three fresh `-after` indexes built by the candidate, plus cargo test on repo-graph-python-extractor (whole), repo-graph-indexer (whole, incl. call_binding_receiver), repo-graph-classification (whole, incl. the parity corpus), repo-graph-agent (whole), repo-graph-storage (--lib), repo-graph-repo-index (--test integration), repo-graph-ts-extractor (--lib), repo-graph-trust (whole) and repo-graph-rgr (--lib presentation::trust), as named per check.",
  "candidatePaths": [
    "rust/crates/python-extractor/src/extractor.rs",
    "rust/crates/indexer/src/resolver.rs",
    "rust/crates/indexer/src/orchestrator.rs",
    "rust/crates/indexer/tests/call_binding_receiver.rs",
    "rust/crates/storage/src/indexer_impl.rs",
    "rust/crates/classification/src/types.rs",
    "rust/crates/classification/src/unresolved_classifier.rs",
    "rust/crates/agent/src/attribution.rs",
    "classification-parity-fixtures/classify__self-mro-ambiguous/input.json",
    "classification-parity-fixtures/classify__self-mro-ambiguous/expected.json",
    "rust/crates/repo-index/tests/integration.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/PYTHON-SELF-BINDING-1/verification.json",
    "docs/assurance/PYTHON-SELF-BINDING-1/implementation-review.json"
  ],
  "candidateExclusions": [
    {
      "pathPrefix": ".agent-manager/",
      "reason": "local relay state and raw run/progress evidence"
    },
    {
      "pathPrefix": "rust/target/",
      "reason": "reproducible Cargo build output"
    }
  ],
  "checks": [
    {
      "checkId": "PSB-C01",
      "obligationIds": [
        "RG-REQ-005-L03",
        "RG-REQ-005-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-python-extractor --lib 2>&1 | tee /tmp/psb-c01.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c01.txt && for t in self_call_in_a_class_carries_the_self_call_carrier cls_call_in_a_class_carries_the_self_call_carrier self_attribute_chain_call_carries_no_carrier self_call_outside_a_class_carries_no_carrier extracts_method_call extracts_class_with_superclass; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c01.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/python-extractor/src/extractor.rs `emit_call_edge` (:1065-1092; `metadata_json: None` at :1089): when `target_key` is exactly `self.<m>` or `cls.<m>` (ONE dot; `<m>` an identifier) AND `ctx.current_class` (:100-101, live during `extract_function` → `extract_calls_from_node` at :951) is `Some(C)`, the edge carries `metadata_json = {\"selfCall\": true, \"enclosingClass\": \"C\"}` (the TS precedent's shape at ts-extractor/src/extractor.rs:2179-2187); the target key is NOT rewritten. A 3+-part key (`self.extra.get`) and a self-call outside any class carry no metadata (byte-identical to today). NEW tests in the file's `mod tests`: `self_call_in_a_class_carries_the_self_call_carrier` (`class C:\\n    def f(self):\\n        self.g()` → target_key `self.g`, metadata selfCall true / enclosingClass `C`), `cls_call_in_a_class_carries_the_self_call_carrier`, `self_attribute_chain_call_carries_no_carrier` (`self.extra.get()` → `metadata_json` None), `self_call_outside_a_class_carries_no_carrier`; the existing `extracts_method_call` (:2136-2147, no class) and `extracts_class_with_superclass` (:2242-2252, metadata key `superclass` raw text) stay green"
      },
      "expected": "exit 0: the extractor records the one fact the resolver needs — this is a self/cls call inside class C — as evidence on the edge, and records nothing where the fact is absent (a chained attribute, a module-level function): binding evidence, never a rewrite (RG-REQ-005-L02)"
    },
    {
      "checkId": "PSB-C02",
      "obligationIds": [
        "RG-REQ-005-L03",
        "RG-REQ-005-L02",
        "RG-REQ-001-L03",
        "RG-REQ-005-L01",
        "P-PSB-01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib resolver 2>&1 | tee /tmp/psb-c02.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c02.txt && for t in self_call_binds_to_the_own_class_method_in_the_same_file self_call_binds_to_a_unique_ancestor_method self_call_same_depth_ancestor_collision_stays_unresolved_with_mro_candidates self_call_with_no_defining_ancestor_falls_through_unchanged self_call_same_name_ancestor_classes_stop_the_walk self_call_malformed_carrier_never_binds cls_call_binds_like_self superclass_metadata_parses_raw_python_base_lists ambiguous_name_stays_unresolved two_definitions_stay_ambiguous method_call_prototype_plus_definition_resolves_to_definition forward_decl_classification_reads_the_stamped_key categorize_calls_this_method categorize_calls_obj_method categorize_calls_function java_suffix_ambiguous_when_shaded rust_crate_import_leaf_module_file; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c02.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-indexer --test call_binding_receiver 2>&1 | tee /tmp/psb-c02b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c02b.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/indexer/src/resolver.rs: `ResolverNode` (:150-166) gains `superclasses: Vec<String>` parsed by a pure function `superclasses_from_metadata(metadata_json) -> Vec<String>` (the raw Python `superclass` text split on `,`, each part trimmed, parts containing `=` dropped, a `[...]` suffix stripped, the LAST dotted segment kept: `base.BaseHandler` → `BaseHandler`; `Base, metaclass=ABCMeta` → [`Base`]; `Generic[T]` → [`Generic`]; absent/unreadable → empty, never a guess); `ResolverIndex` (:173-201) gains `nodes_by_qualified_name: HashMap<String, Vec<ResolverNode>>` (populated at orchestrator.rs:929-933 beside `nodes_by_name`; every literal `ResolverIndex {` site the compiler names — 14 — gains the field). ONE new stage in `resolve_call_target`, inserted BEFORE the dotted fallback (:898) and gated ONLY on the carrier (`metadata_json.selfCall == true` and `enclosingClass` a non-empty string — any other present shape is a MALFORMED carrier: the stage does not run and the edge follows today's path): the own class = the unique CLASS node named `enclosingClass` in the CALLER's file (`node_uid_to_file_uid` of the source node); breadth-first over the superclass closure (depth 0 = the own class; each ancestor = the unique CLASS node of that simple name — 0 or >1 such nodes end that branch, stated; visited set; depth cap 16); at each depth the hits are `nodes_by_qualified_name[\"<Class>.<m>\"]` with subtype METHOD in that class node's file; exactly one hit at the first non-empty depth → `TargetResolution::Resolved`; two or more hits at that depth → the NEW terminal variant `TargetResolution::SelfCallAmbiguousMro(candidates)` (sorted `<Class>.<m>` strings; `resolve_edges`' exhaustive match gains its arm: category via `categorize_unresolved_edge` — `calls_obj_method_needs_type_info`, unchanged — and the cloned edge's `metadata_json` gains `\"mroCandidates\": [...]` beside the carrier, the first resolver-written metadata key, stated in §2.1); no hit at any depth → fall through to today's fallbacks unchanged. Tests in `mod tests` (the `make_node`/`make_qn_node` fixtures :1657/:1678 gain the field): the seven `self_call_*`/`cls_call_*` names + `superclass_metadata_parses_raw_python_base_lists`; the whole existing resolver suite incl. Q1's C++ family and the Gate B tests stays green; the `call_binding_receiver` integration suite stays green (RG-REQ-005-L01: the C-family gate at :1000 is untouched)"
      },
      "expected": "exit 0: a self/cls call binds only on hierarchy evidence — the class node in the caller's file and its stored superclass facts — a same-depth collision is declined and NAMED with its candidates (RG-REQ-001-L03), a malformed carrier never binds, and every other language's binding remains byte-identical (no behavior change on the C/C++ gates, TS `this.`, Java, Rust — preserved)"
    },
    {
      "checkId": "PSB-C03",
      "obligationIds": [
        "RG-REQ-005-L03",
        "RG-REQ-001-L03",
        "RG-REQ-005-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-classification 2>&1 | tee /tmp/psb-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c03.txt && for t in mro_candidates_carrier_yields_internal_candidate_with_the_self_call_ambiguous_mro_basis malformed_mro_candidates_carrier_falls_through_to_the_existing_rules basis_code_round_trips_self_call_ambiguous_mro; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c03.txt || { echo \"MISSING $t\"; exit 1; }; done && test -f ../classification-parity-fixtures/classify__self-mro-ambiguous/input.json && test -f ../classification-parity-fixtures/classify__self-mro-ambiguous/expected.json && grep -q 'self_call_ambiguous_mro' ../classification-parity-fixtures/classify__self-mro-ambiguous/expected.json && cargo test -p repo-graph-agent --lib attribution 2>&1 | tee /tmp/psb-c03b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c03b.txt && for t in every_basis_code_maps_to_its_expected_reader_class self_call_ambiguous_mro_is_own_code_unresolved; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c03b.txt || { echo \"MISSING $t\"; exit 1; }; done && grep -qE 'assert_eq!\\(EXPECTED\\.len\\(\\), 18' crates/agent/src/attribution.rs && grep -qE 'pub const CURRENT_CLASSIFIER_VERSION: u32 = 6;' crates/classification/src/types.rs",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/classification/src/types.rs: `UnresolvedEdgeBasisCode` (:386-436) gains `SelfCallAmbiguousMro` (wire `self_call_ambiguous_mro`; the vocabulary's own contract at :442-445 says additions need no version bump — `CURRENT_CLASSIFIER_VERSION` stays 6); the category vocabulary is UNTOUCHED (D-PSB-001). rust/crates/classification/src/unresolved_classifier.rs `classify_unresolved_edge` (:60-65): a new first rule — when `edge.metadata_json` carries `mroCandidates` as an array of ≥2 strings, return `classification: internal_candidate`, `basis_code: self_call_ambiguous_mro` (the collision is inside the reader's own class hierarchy — the `this`-receiver precedent at :66-71); a present-but-malformed `mroCandidates` (not an array, fewer than two entries, non-string entries) is ignored and the existing rules run (pinned by a test; the key is written and read inside one index run, never across a build boundary — stated). rust/crates/agent/src/attribution.rs: `attribution_class` (:140-173) adds `B::SelfCallAmbiguousMro` to the `OwnCodeUnresolved` arm (:158-165) — the ONE compile-forced site — and `EXPECTED` (:545-614) gains the pair, `EXPECTED.len()` 17 → 18 (:620): RG-REQ-005-L03's own verification criterion. NEW parity fixture classification-parity-fixtures/classify__self-mro-ambiguous/{input,expected}.json in the `classify__this-receiver` shape (metadata `{\"selfCall\":true,\"enclosingClass\":\"AdminPasswordChangeForm\",\"mroCandidates\":[\"SetPasswordMixin.validate_passwords\",\"SetUnusablePasswordMixin.validate_passwords\"]}`, category `calls_obj_method_needs_type_info` → expected `{\"classification\":\"internal_candidate\",\"basisCode\":\"self_call_ambiguous_mro\"}`); the harness (classification/tests/parity.rs) picks it up. `blast_radius.rs:75` (`_ => LocalLike`) and `unresolved_classifier.rs:572` (`_ => None`) are the correct fallbacks for the new basis — stated, not edited"
      },
      "expected": "exit 0: a declined MRO collision carries its own named basis, maps to the reader class `your own code (call target not resolved)`, round-trips on the wire, and the parity corpus pins it; the category vocabulary and the reader-class set remain unchanged — no behavior change to the categories and classes RG-REQ-005-L09's zero-state reads (preserved)"
    },
    {
      "checkId": "PSB-C04",
      "obligationIds": [
        "RG-REQ-005-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-storage --lib indexer_impl 2>&1 | tee /tmp/psb-c04.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c04.txt && for t in query_resolver_nodes_carries_python_superclasses_from_metadata; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c04.txt || { echo \"MISSING $t\"; exit 1; }; done && ! git diff HEAD -- crates/storage/src/indexer_impl.rs | grep -E '^\\+.*(SELECT|FROM nodes)'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/storage/src/indexer_impl.rs `query_resolver_nodes` (:377-401) already SELECTs `metadata_json` (:382-386) and reads only `forward_decl` from it (:395-397); the same local now also feeds `superclasses: superclasses_from_metadata(metadata_json.as_deref())` — NO SQL change (the negated grep asserts the SELECT is untouched), no schema change. NEW test in the file's `mod tests` (the `ResolverNode {` fixture literals at :1287/:1325/:1809/:2031 gain the field): a CLASS node inserted with `{\"superclass\":\"base.BaseHandler\"}` reads back `superclasses == [\"BaseHandler\"]`, one with `{\"superclass\":\"Base, metaclass=ABCMeta\"}` → [`Base`], one with no metadata → [] (the MockStorage literal at orchestrator.rs:2421 gains the field too)"
      },
      "expected": "exit 0: the stored superclass fact reaches the resolver through the read that already exists, with no schema or SQL change"
    },
    {
      "checkId": "PSB-C05",
      "obligationIds": [
        "RG-REQ-005-L03",
        "RG-REQ-001-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-repo-index --test integration 2>&1 | tee /tmp/psb-c05.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c05.txt && for t in index_python_binds_a_self_call_to_the_own_class_method index_python_extracts_symbols; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c05.txt || { echo \"MISSING $t\"; exit 1; }; done && git diff --quiet HEAD -- crates/repo-index/tests/fixtures",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the existing Python fixture rust/crates/repo-index/tests/fixtures/python/simple-app/ (UNCHANGED — the diff assertion): `src/service.py` calls `self._process_user(user)` inside `UserService.process` (own-class unique hit) and `src/app.py` calls `self._service.process()` (a 3-part key). NEW test `index_python_binds_a_self_call_to_the_own_class_method` in rust/crates/repo-index/tests/integration.rs beside `index_python_extracts_symbols` (:1003): after `index_into_storage`, a CALLS edge exists from `UserService.process` to `UserService._process_user`, and `self._service.process` is still an `unresolved_edges` row; the existing Python assertions (`>=` bounds at :1218-1229) stay green; a second `index_into_storage` of the same fixture into a fresh store yields the same CALLS edge count (RG-REQ-001-L09 for the new stage)"
      },
      "expected": "exit 0: the fix is visible end-to-end on the smallest corpus — one edge that was an unresolved row now binds, the attribute chain stays unresolved, and indexing twice yields the same graph: determinism remains as before (RG-REQ-001-L09 preserved, no behavior change)"
    },
    {
      "checkId": "PSB-C06",
      "obligationIds": [
        "P-PSB-01",
        "P-PSB-02",
        "P-PSB-03",
        "RG-REQ-006-L01",
        "RG-REQ-006-L02",
        "RG-REQ-006-L04",
        "RG-REQ-005-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer 2>&1 | tee /tmp/psb-c06.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06.txt && for t in rust_crate_import_leaf_module_file java_suffix_resolves_by_package_path java_suffix_ambiguous_when_shaded aliased_named_import_uses_imported_name_for_lookup namespace_import_member_resolves_to_target_module default_import_member_does_not_resolve_by_bare_method_name import_type_only_reads_injected_metadata forward_decl_classification_reads_the_stamped_key; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c06.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-classification 2>&1 | tee /tmp/psb-c06b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06b.txt && cargo test -p repo-graph-agent 2>&1 | tee /tmp/psb-c06c.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06c.txt && cargo test -p repo-graph-storage --lib 2>&1 | tee /tmp/psb-c06d.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06d.txt && cargo test -p repo-graph-python-extractor 2>&1 | tee /tmp/psb-c06e.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06e.txt && cargo test -p repo-graph-ts-extractor --lib 2>&1 | tee /tmp/psb-c06f.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06f.txt && for t in this_method_rewritten_to_class_name this_property_method_rewritten_via_class_binding; do grep -qE \"^test .*$t .* ok$\" /tmp/psb-c06f.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-trust 2>&1 | tee /tmp/psb-c06g.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06g.txt && cargo test -p repo-graph-rgr --lib presentation::trust 2>&1 | tee /tmp/psb-c06h.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/psb-c06h.txt && git diff --quiet HEAD -- crates/ts-extractor crates/java-extractor crates/cpp-extractor crates/c-extractor crates/trust crates/storage/src/call_resolution_reads.rs crates/enrichment",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the acceptance boundary of every crate the candidate touches, run to completion in the foreground: the whole indexer crate (unit + integration incl. the Q1 C++ family and the Rust/Java/TS import stages), the whole classification and agent crates, the storage lib, the whole python-extractor crate, the ts-extractor lib (the `this.` rewrite tests), the whole trust crate, rgr's trust presentation tests; and the byte-identity of every path this slice must NOT touch: the other extractors, the trust crate, `call_resolution_reads.rs` (`CALLS_CATEGORIES: [_; 4]` unchanged — the category vocabulary is untouched), the enrichment crate"
      },
      "expected": "exit 0: every other resolver stage, the C/C++ receiver invariant, the TS `this.` rewrite, forward-declaration handling, the trust computations and the category vocabulary remain exactly as before (no behavior change outside the Python self-call stage)"
    },
    {
      "checkId": "PSB-C07",
      "obligationIds": [
        "P-PSB-05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo fmt --check -p repo-graph-python-extractor -p repo-graph-indexer -p repo-graph-storage -p repo-graph-classification -p repo-graph-agent -p repo-graph-repo-index && cargo clippy -p repo-graph-python-extractor -p repo-graph-indexer -p repo-graph-storage -p repo-graph-classification -p repo-graph-agent -p repo-graph-repo-index --all-targets -- -D warnings > /tmp/psb-c07.txt 2>&1; rc=$?; tail -3 /tmp/psb-c07.txt; test $rc -eq 0",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rustfmt and clippy (deny warnings, all targets) over the six touched crates"
      },
      "expected": "exit 0: no formatting or lint debt enters with the candidate"
    },
    {
      "checkId": "PSB-C08",
      "obligationIds": [
        "RG-REQ-005-L03",
        "RG-REQ-005-L02",
        "RG-REQ-001-L03",
        "P-PSB-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/PYTHON-SELF-BINDING-1-django-after && mkdir -p /private/tmp/PYTHON-SELF-BINDING-1-django-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/psb-dj-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/psb-dj-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain BaseHandler.get_response > /tmp/psb-dj-explain-after.txt 2> /tmp/psb-dj-explain-after.txt.err ) || { echo \"RUN-FAILED /tmp/psb-dj-explain-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" callers BaseHandler.get_response > /tmp/psb-dj-callers-after.txt 2> /tmp/psb-dj-callers-after.txt.err ) || { echo \"RUN-FAILED /tmp/psb-dj-callers-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-before RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/PYTHON-SELF-BINDING-1-before-bin:$PATH\" \"/private/tmp/PYTHON-SELF-BINDING-1-before-bin/rmap\" explain BaseHandler.get_response > /tmp/psb-dj-explain-before.txt 2> /tmp/psb-dj-explain-before.txt.err ) || { echo \"RUN-FAILED /tmp/psb-dj-explain-before.txt\"; exit 1; } && grep -qE '^Callers \\(0\\)$' /tmp/psb-dj-explain-before.txt && grep -qE '^Callers \\(2\\)$' /tmp/psb-dj-explain-after.txt && grep -qE '^  - __call__ \\([^)]*\\)  django/core/handlers/wsgi\\.py:120$' /tmp/psb-dj-explain-after.txt && grep -qE '^  - __call__ \\([^)]*\\)  django/test/client\\.py:169$' /tmp/psb-dj-explain-after.txt && ! grep -qE 'asgi\\.py|exception\\.py|AsyncClientHandler' /tmp/psb-dj-explain-after.txt && grep -qE '^2 callers found$' /tmp/psb-dj-callers-after.txt && python3 - /private/tmp/PYTHON-SELF-BINDING-1-django-before /private/tmp/PYTHON-SELF-BINDING-1-django-after <<'PY'\nimport sqlite3, glob, json, sys\ndef store(root):\n    db = glob.glob(root + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\n    snap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\n    calls = k.execute(\"select count(*) from edges where snapshot_uid = ? and type = 'CALLS'\", (snap,)).fetchone()[0]\n    unres = {}\n    for sk, tk, line, cat, basis, cls, md in k.execute(\"select n.stable_key, u.target_key, u.line_start, u.category, u.basis_code, u.classification, u.metadata_json from unresolved_edges u join nodes n on u.source_node_uid = n.node_uid and n.snapshot_uid = u.snapshot_uid where u.snapshot_uid = ? and u.type = 'CALLS'\", (snap,)):\n        unres[(sk, tk, line)] = (cat, basis, cls, md)\n    callers = sorted(r[0] for r in k.execute(\"select s.stable_key from edges e join nodes s on e.source_node_uid = s.node_uid and s.snapshot_uid = e.snapshot_uid join nodes t on e.target_node_uid = t.node_uid and t.snapshot_uid = e.snapshot_uid where e.snapshot_uid = ? and e.type = 'CALLS' and t.qualified_name = 'BaseHandler.get_response'\", (snap,)))\n    return calls, unres, callers\nb_calls, b_un, b_callers = store(sys.argv[1]); a_calls, a_un, a_callers = store(sys.argv[2])\nis_self = lambda key: (key[1].startswith('self.') or key[1].startswith('cls.')) and key[1].count('.') == 1\nmoved = {k for k in b_un if k not in a_un}\nassert all(is_self(k) for k in moved), [k for k in moved if not is_self(k)][:5]\nassert not [k for k in a_un if k not in b_un], 'new unresolved rows appeared: ' + str([k for k in a_un if k not in b_un][:5])\nassert a_calls - b_calls == len(moved), (a_calls, b_calls, len(moved))\nchanged_basis = {k for k in a_un if a_un[k][1] != b_un[k][1]}\nmro = {k for k, v in a_un.items() if v[1] == 'self_call_ambiguous_mro'}\nassert changed_basis == mro, (len(changed_basis), len(mro))\nassert mro and all(a_un[k][2] == 'internal_candidate' and a_un[k][0] == 'calls_obj_method_needs_type_info' for k in mro)\nfor k in mro:\n    md = json.loads(a_un[k][3]); c = md['mroCandidates']\n    assert isinstance(c, list) and len(c) >= 2 and all(isinstance(x, str) for x in c), (k, md)\nassert any(k[1] == 'self.validate_passwords' and 'django/contrib/auth/forms.py' in k[0] and k[2] == 576 for k in mro), 'forms.py:576 collision missing'\nassert b_callers == [] and len(a_callers) == 2 and any('WSGIHandler.__call__' in c for c in a_callers) and any('ClientHandler.__call__' in c for c in a_callers), (b_callers, a_callers)\nassert not any('ASGIHandler' in c for c in a_callers)\nprint('seam ok: CALLS', b_calls, '->', a_calls, '| bound', len(moved), '| mro collisions', len(mro), '| unresolved', len(b_un), '->', len(a_un))\nPY",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/PYTHON-SELF-BINDING-1-django-before (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django by the base-revision binary on 2026-09-20, 18 s; served by the before binary /private/tmp/PYTHON-SELF-BINDING-1-before-bin/rmap{,d} copied from rust/target/release on the CLEAN tree before any candidate edit) and NEW /private/tmp/PYTHON-SELF-BINDING-1-django-after (fresh index by the candidate); both under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the after root STAYS",
        "inputs": "django — BEFORE (read from the product 2026-09-20): `explain BaseHandler.get_response` (django/core/handlers/base.py:138) prints `Callers (0)`; the store holds 60,414 resolved CALLS and 91,514 unresolved CALLS of which 26,461 are `self.`/`cls.` keys, every one `calls_obj_method_needs_type_info` / `no_supporting_signal`; both true callers are unresolved rows: `django/core/handlers/wsgi.py:124 response = self.get_response(request)` in `WSGIHandler.__call__` (`class WSGIHandler(base.BaseHandler)` :113 — stored `{\"superclass\":\"base.BaseHandler\"}`) and `django/test/client.py:186` in `ClientHandler.__call__` (`class ClientHandler(BaseHandler)` :158). AFTER: `Callers (2)` with exactly those two rows anchored at the CALLER's DECLARATION line — the row form is `  - <name> (<module>)  <path>:<caller line_start>` (`find_symbol_callers` selects `caller.line_start`, agent_impl.rs:1181-1187; leveldb's `  - Open (db)  db/db_impl.cc:1503` is `Status DB::Open(` declared at :1503): `__call__ … django/core/handlers/wsgi.py:120` (`def __call__(self, environ, start_response)` :120; the call site :124 is the seam's witness) and `__call__ … django/test/client.py:169` (declared :169; call site :186); `asgi.py:231` (`ASGIHandler`, `await self.get_response_async(request)`) and `client.py:241` (`AsyncClientHandler`) are NOT callers; exception.py's closure calls are not callers; `callers BaseHandler.get_response` prints `2 callers found`. The store-derived seam (python over BOTH stores, keyed by caller stable_key + target_key + line): every unresolved row that left the set is a 2-part `self.`/`cls.` key; no unresolved row appeared; resolved CALLS grew by exactly the number that left; the ONLY basis changes are the rows now carrying `self_call_ambiguous_mro` (classification `internal_candidate`, category unchanged, `mroCandidates` ≥ 2 strings) — among them `django/contrib/auth/forms.py:576 self.validate_passwords()` in `AdminPasswordChangeForm.clean` (`class AdminPasswordChangeForm(SetUnusablePasswordMixin, SetPasswordMixin, forms.Form)` :550; both mixins define `validate_passwords`, :107 and :166 — the L's same-depth rule declines where Python's MRO would pick the first base). The totals are REPORTED (manager simulation, file-scoped own class then same-name ancestors: ~4,020 bound — 2,490 in the own class, 1,530 through ancestors — and 4 same-depth collisions; the candidate's exact numbers come from the seam's print)"
      },
      "expected": "exit 0: the audit's witness answers — two real callers, no invented one — every binding is a self/cls row that left the unresolved set on hierarchy evidence (a wrong binding that removed any other row would fail the seam — RG-REQ-001-L03), and the declined collisions are named"
    },
    {
      "checkId": "PSB-C09",
      "obligationIds": [
        "P-PSB-02",
        "RG-REQ-006-L04",
        "RG-REQ-005-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/PYTHON-SELF-BINDING-1-leveldb-after && mkdir -p /private/tmp/PYTHON-SELF-BINDING-1-leveldb-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/psb-lv-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/psb-lv-index.txt\"; exit 1; } && rm -rf /private/tmp/PYTHON-SELF-BINDING-1-fraktag-after && mkdir -p /private/tmp/PYTHON-SELF-BINDING-1-fraktag-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/psb-frk-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/psb-frk-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-before RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/PYTHON-SELF-BINDING-1-before-bin:$PATH\" \"/private/tmp/PYTHON-SELF-BINDING-1-before-bin/rmap\" trust > /tmp/psb-lv-trust-before.txt 2> /tmp/psb-lv-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/psb-lv-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-leveldb-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/psb-lv-trust-after.txt 2> /tmp/psb-lv-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/psb-lv-trust-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-before RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/PYTHON-SELF-BINDING-1-before-bin:$PATH\" \"/private/tmp/PYTHON-SELF-BINDING-1-before-bin/rmap\" trust > /tmp/psb-frk-trust-before.txt 2> /tmp/psb-frk-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/psb-frk-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/psb-frk-trust-after.txt 2> /tmp/psb-frk-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/psb-frk-trust-after.txt\"; exit 1; } && grep -v '^Snapshot:' /tmp/psb-lv-trust-before.txt > /tmp/psb-lv-tb.txt && grep -v '^Snapshot:' /tmp/psb-lv-trust-after.txt > /tmp/psb-lv-ta.txt && diff /tmp/psb-lv-tb.txt /tmp/psb-lv-ta.txt && grep -v '^Snapshot:' /tmp/psb-frk-trust-before.txt > /tmp/psb-frk-tb.txt && grep -v '^Snapshot:' /tmp/psb-frk-trust-after.txt > /tmp/psb-frk-ta.txt && diff /tmp/psb-frk-tb.txt /tmp/psb-frk-ta.txt && python3 - /private/tmp/PYTHON-SELF-BINDING-1-leveldb-before /private/tmp/PYTHON-SELF-BINDING-1-leveldb-after /private/tmp/PYTHON-SELF-BINDING-1-fraktag-before /private/tmp/PYTHON-SELF-BINDING-1-fraktag-after <<'PY'\nimport sqlite3, glob, sys\ndef facts(root):\n    db = glob.glob(root + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\n    snap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\n    calls = k.execute(\"select count(*) from edges where snapshot_uid = ? and type = 'CALLS'\", (snap,)).fetchone()[0]\n    cats = sorted(k.execute(\"select category, basis_code, count(*) from unresolved_edges where snapshot_uid = ? and type = 'CALLS' group by 1, 2\", (snap,)).fetchall())\n    return calls, cats\nfor name, b, a in (('leveldb', sys.argv[1], sys.argv[2]), ('fraktag', sys.argv[3], sys.argv[4])):\n    fb, fa = facts(b), facts(a)\n    assert fb == fa, (name, fb, fa)\n    print(name, 'byte-stable: CALLS', fa[0], 'unresolved by category/basis', fa[1])\nPY",
        "cwd": ".",
        "environment": "pre-provisioned /private/tmp/PYTHON-SELF-BINDING-1-leveldb-before (1 s) and /private/tmp/PYTHON-SELF-BINDING-1-fraktag-before (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG, 1 s) by the base binary; NEW -after roots by the candidate; under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the after roots STAY",
        "inputs": "leveldb (C++: CALLS 3,488 resolved / 5,925 unresolved, all `calls_function_ambiguous_or_missing`) and FRAKTAG (TypeScript: 446 / 1,658 — obj_method 904, function 499, this_wildcard 146, this_method_needs_class_context 109; the TS `this.` rewrite route) reindexed by the candidate: the resolved CALLS count and the per-category/per-basis unresolved breakdown are identical to the before store, and `trust` is byte-identical apart from the `Snapshot:` line"
      },
      "expected": "exit 0: repositories without Python self-calls are byte-stable — the stage is gated on the carrier only Python writes (no behavior change for C++ and TypeScript; the receiver invariant and the TS import/`this.` stages preserved)"
    },
    {
      "checkId": "PSB-C10",
      "obligationIds": [
        "RG-REQ-005-L09",
        "RG-REQ-001-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-before RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/PYTHON-SELF-BINDING-1-before-bin:$PATH\" \"/private/tmp/PYTHON-SELF-BINDING-1-before-bin/rmap\" trust > /tmp/psb-dj-trust-before.txt 2> /tmp/psb-dj-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/psb-dj-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/PYTHON-SELF-BINDING-1-django-after RMAP_SOCKET_PATH=/private/tmp/PYTHON-SELF-BINDING-1-django-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/psb-dj-trust-after.txt 2> /tmp/psb-dj-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/psb-dj-trust-after.txt\"; exit 1; } && grep -qE \"^  - your code's calls 43% resolved \\(60414 of 140705 in-scope or unclassified\\)$\" /tmp/psb-dj-trust-before.txt && grep -qE '^  - your own code \\(call target not resolved\\): 42 references$' /tmp/psb-dj-trust-before.txt && python3 - /private/tmp/PYTHON-SELF-BINDING-1-django-after <<'PY'\nimport sqlite3, glob, re, sys\ndb = glob.glob(sys.argv[1] + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\nsnap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\nmro = k.execute(\"select count(*) from unresolved_edges where snapshot_uid = ? and type = 'CALLS' and basis_code = 'self_call_ambiguous_mro'\", (snap,)).fetchone()[0]\ncalls = k.execute(\"select count(*) from edges where snapshot_uid = ? and type = 'CALLS'\", (snap,)).fetchone()[0]\nt = open('/tmp/psb-dj-trust-after.txt').read()\nown = int(re.search(r\"your own code \\(call target not resolved\\): (\\d+) references\", t).group(1))\nres = re.search(r\"your code's calls (\\d+)% resolved \\((\\d+) of (\\d+) in-scope or unclassified\\)\", t)\nassert own == 42 + mro, (own, mro)\nassert int(res.group(2)) == calls and calls > 60414, (res.group(2), calls)\nprint('trust after: resolved', res.group(1) + '%', res.group(2), 'of', res.group(3), '| own-code unresolved', own, '(+', mro, 'mro collisions)')\nPY",
        "cwd": ".",
        "environment": "the two django roots and binaries of PSB-C08",
        "inputs": "django `trust` BEFORE: `your code's calls 43% resolved (60414 of 140705 in-scope or unclassified)`, `your own code (call target not resolved): 42 references`, `couldn't attribute: 80571 references`. AFTER (REPORTED, seam-checked): the resolved count equals the after store's CALLS edges and exceeds 60,414; the own-code bucket equals 42 + the number of `self_call_ambiguous_mro` rows (the only basis that moved into it); the trust caveat `call-graph resolution is at this build's ceiling for Python (no resolver exists)` still prints — it reads the ENRICHMENT language table (reader_context.rs:300-317, `token_enrichment_language`), which this slice does not change; its wording is a pre-existing residual (TRUST-CEILING-WORDING-1, §8), stated"
      },
      "expected": "exit 0: trust's numbers move only where the evidence moved — the resolved share up by the bound rows, the own-code bucket up by the named collisions — and the reader is told which counts changed and why"
    },
    {
      "checkId": "PSB-C11",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-PSB-05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "test \"$(grep -E '^registry-before [0-9a-f]{64}$' .agent-manager/slices/PYTHON-SELF-BINDING-1/build-progress.md | tail -1 | cut -d' ' -f2)\" = \"$(shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -d' ' -f1)\" && PIDS=$(pgrep -x rmapd | paste -sd, -); { [ -z \"$PIDS\" ] || ! ps -o command= -p \"$PIDS\" | grep -v '/\\.local/bin/rmapd' | grep -q .; } && ! test -d /private/tmp/PYTHON-SELF-BINDING-1-before-bin && ls -d /private/tmp/PYTHON-SELF-BINDING-1-django-after /private/tmp/PYTHON-SELF-BINDING-1-leveldb-after /private/tmp/PYTHON-SELF-BINDING-1-fraktag-after >/dev/null && ! git status --short | grep -vE '^[ MA?][ MA?] (rust/crates/python\\-extractor/src/extractor\\.rs|rust/crates/indexer/src/resolver\\.rs|rust/crates/indexer/src/orchestrator\\.rs|rust/crates/indexer/tests/call_binding_receiver\\.rs|rust/crates/storage/src/indexer_impl\\.rs|rust/crates/classification/src/types\\.rs|rust/crates/classification/src/unresolved_classifier\\.rs|rust/crates/agent/src/attribution\\.rs|classification\\-parity\\-fixtures/classify__self\\-mro\\-ambiguous/input\\.json|classification\\-parity\\-fixtures/classify__self\\-mro\\-ambiguous/expected\\.json|rust/crates/repo\\-index/tests/integration\\.rs)$' | grep -q . && git diff --check && ! grep -nE ' +$' classification-parity-fixtures/classify__self-mro-ambiguous/input.json classification-parity-fixtures/classify__self-mro-ambiguous/expected.json",
        "cwd": ".",
        "environment": "candidate tree, after every other check",
        "inputs": "the full sha256 of the operator registry recorded as `registry-before <digest>` in build-progress.md at step 0 equals the current one; no stray isolated daemon; the before-binary directory removed; the three `-after` roots kept for the manager's gate; `git status --short` (porcelain columns ` M`, `A `, `AM`, `??`) names only the 11 candidate paths; no trailing whitespace"
      },
      "expected": "exit 0: every proof was isolated (the operator's registry byte-identical — no behavior change outside the isolated roots), the tree holds only the allocated paths, and nothing the builder created outlives the run except the three roots the manager sweeps at closeout"
    }
  ]
}
-->

# PYTHON-SELF-BINDING-1 — `self.method()` binds through the class hierarchy or stays honestly unresolved

Status: SPECIFIED 2026-09-12; GROUNDED and re-verified on HEAD ed78de2 on 2026-09-20 (every seam read from the sources by a read-only investigation; every literal read from the product on three fresh isolated roots and from the django checkout; the predicate simulated over the django store) · Track: audit round six, Q9 (MEDIUM; RC-2 never worked). CODE slice, requires reindex of Python repos. Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13).

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-005-L03 (the breadth-first walk from the enclosing class over the stored superclass facts; first depth with exactly one METHOD binds; a same-depth collision stays unresolved with its own named basis — PSB-C01/C02/C03/C05/C08); RG-REQ-005-L02 (binding only on evidence: the extractor's self-call carrier plus the class node in the caller's file and its stored bases; a malformed carrier never binds — PSB-C01/C02); RG-REQ-001-L03 (the declined collision persists as an `unresolved_edges` row with a named basis and its candidates; the seam proves no other row left the unresolved set — PSB-C02/C03/C08/C10).

**Changes:** none allocated. Consequences REPORTED, never predicted: django's resolved CALLS count rises by exactly the rows that bound (seam), trust's `your code's calls N% resolved` rises with it, trust's `your own code (call target not resolved)` bucket grows by the named collisions (PSB-C10); `dead`'s substrate sees more fan-in (not asserted).

**Preserves:** RG-REQ-005-L01 (the C/C++ receiver invariant and the C-family gate at resolver.rs:1000 — PSB-C02/C06/C09), RG-REQ-006-L01/L02/L04 (the Rust, Java and TS/Python import stages — PSB-C06/C09), RG-REQ-005-L07 (forward-declaration handling — PSB-C02/C06), RG-REQ-005-L09 (the category/basis vocabulary the zero-state will read; the category axis untouched — PSB-C03/C06/C10), RG-REQ-001-L09 (indexing the fixture twice yields the same graph — PSB-C05), RG-REQ-011-L06 (isolation — PSB-C11).

**Decision D-PSB-001 / D-PSB-002 (ratified by the human 2026-09-21; docs/assurance/RG-BOOTSTRAP/decisions/D-PSB-001.md and D-PSB-002.md, both pinned by INPUT-2):** the 2026-09-12 L03 text said the collision "stays unresolved with its own category" while its verification criterion named `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` — a BASIS-CODE test; the category axis has no reader mapping at all (the seam investigation of 2026-09-20) and a new category fans out across eleven hand-maintained registries. The INPUT-1 document review raised D-PSB-002 (a requirement-text amendment is the human's); the human ratified option B and RG-REQ-005-L03 now says "its own basis code (`self_call_ambiguous_mro` … the row keeps the category `calls_obj_method_needs_type_info`) and its candidates persisted as evidence" (RG-BOOTSTRAP-INPUT-5). This allocation implements the ratified wording.

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-PSB-01 | The C/C++ receiver invariant (RG-REQ-005-L01) and both Q1 gates in `resolve_call_target` are untouched: the Q1 unit family and the `call_binding_receiver` integration suite green; leveldb byte-stable | PSB-C02; PSB-C06; PSB-C09 |
| P-PSB-02 | Every non-Python extractor and route is byte-stable: leveldb and FRAKTAG resolved/unresolved counts and per-category/per-basis breakdowns identical; `trust` identical apart from the snapshot line; the TS `this.` rewrite tests green; the other extractor crates and the trust crate untouched (diff --quiet) | PSB-C06; PSB-C09 |
| P-PSB-03 | Every existing test in the touched crates stays green | PSB-C06 |
| P-PSB-04 | No unresolved row other than a 2-part `self.`/`cls.` row leaves or enters the unresolved set on django; the only basis change is the new collision basis; resolved CALLS grow by exactly the rows that left (RG-REQ-001-L03: a wrong binding that removes a row is a violation) | PSB-C08 |
| P-PSB-05 | Isolation, cleanup, hygiene: registry digest unchanged, no stray daemon, only candidate paths, fmt/clippy clean | PSB-C07; PSB-C11 |

## 1. Problem (ROOT-CAUSED — RC-2; docs/audits/2026-09-08-root-causes-v0.18.0.md; re-verified on HEAD ed78de2, 2026-09-20)

`python-extractor/src/extractor.rs:1094-1109 get_call_target_name`'s `attribute` arm formats `object.attribute` verbatim, so `self.get_response(request)` becomes the string target `self.get_response`; `emit_call_edge` (:1065-1092) pushes it with `metadata_json: None` (:1089) although `ctx.current_class` (:100-101) is live at that point (:951). `indexer/src/resolver.rs`'s only receiver-aware branch is spelled `this.` (:931-938, 3+ parts); the dotted fallback (:898-946) then tries the bare method name — `pick_unambiguous(nodes_by_name.get("get_response"))` sees 8 nodes and declines; `categorize_unresolved_call` (:1445-1472) files the row as `calls_obj_method_needs_type_info`, and the classifier (`classification/src/unresolved_classifier.rs:60-143`) extracts the receiver `self`, finds no import, same-file symbol or runtime builtin named `self`, and returns `unknown` / `no_supporting_signal` — every Python self-call reads to the reader as "couldn't attribute", strictly worse than the TS `this.m()` sibling. No inheritance edge exists for Python: `extract_class` (:960-1003) stores the bases only as `metadata_json.superclass` — the raw parenthesised text (`"base.BaseHandler"`, `"Base, metaclass=ABCMeta"`, `"Generic[T]"`); `query_resolver_nodes` (storage/src/indexer_impl.rs:377-401) already SELECTs that metadata (:382-386) and reads only `forward_decl` from it; `ResolverNode` (:150-166) has no superclasses and `ResolverIndex` (:173-201) no qualified-name map. Outward (read from the product 2026-09-20): django `explain BaseHandler.get_response` (django/core/handlers/base.py:138) → `Callers (0)`; `callers` → `0 callers found`; the two true callers — `django/core/handlers/wsgi.py:124 response = self.get_response(request)` in `WSGIHandler.__call__` (`class WSGIHandler(base.BaseHandler)` :113) and `django/test/client.py:186` in `ClientHandler.__call__` (`class ClientHandler(BaseHandler)` :158) — are unresolved rows; 26,461 of django's 91,514 unresolved CALLS are `self.`/`cls.` keys (60,414 resolved). Never worked: the attribute arm dates to the extractor's introducing commit 4c9071e.

## 2. Contract

### 2.1 The fix, at the cause — evidence on the edge, facts on the node, one gated stage, one named basis

1. **Extractor (python-extractor/src/extractor.rs `emit_call_edge`).** When the key is exactly `self.<m>` / `cls.<m>` and `ctx.current_class == Some(C)`, the edge carries `{"selfCall": true, "enclosingClass": "C"}`; the key is NOT rewritten (`resolve_call_target` has no qualified lookup — the TS rewrite buys nothing here); chained keys and module-level self-calls carry nothing. The Python `self.`/`cls.` attribute skips (:301-303, :356-358) are unrelated and untouched.
2. **Superclass facts on the node (storage + indexer).** `ResolverNode.superclasses: Vec<String>` from `superclasses_from_metadata` (pure: split `,`, trim, drop `=` parts, strip `[…]`, last dotted segment; unreadable → empty — never a guess); parsed at storage/src/indexer_impl.rs:395 from the metadata local the read already binds (NO SQL change) and at the six literal sites the compiler names (storage :387, orchestrator.rs:2421 MockStorage, resolver.rs:1657/:1678/:2970, tests/call_binding_receiver.rs:51). `ResolverIndex.nodes_by_qualified_name` populated at orchestrator.rs:929-933; the fourteen `ResolverIndex {` literals gain the field (the compiler's list is the truth).
3. **One stage, gated on the carrier only (resolver.rs, before :898).** Own class = the unique CLASS node named `enclosingClass` in the caller's file; BFS over the superclass closure by simple name (an ancestor name matching 0 or >1 CLASS nodes ends that branch — cross-package and same-name bases stay unresolved, no guessing; visited set; depth cap 16); hits = METHOD nodes `<Class>.<m>` in that class node's file; first depth with exactly one hit → bound; two or more at that depth → `TargetResolution::SelfCallAmbiguousMro(candidates)` (a new variant; the exhaustive match in `resolve_edges` :339 gains its arm) — TERMINAL: the row keeps category `calls_obj_method_needs_type_info`, the cloned edge's metadata gains `"mroCandidates": [sorted "<Class>.<m>"]` (the first resolver-written metadata key; the extractor's keys are preserved beside it — stated); no hit → fall through to today's fallbacks unchanged (a self-call whose method name is globally unique still binds by the bare-name rule exactly as today). A carrier present in any other shape (non-bool `selfCall`, missing/non-string `enclosingClass`) is MALFORMED: the stage does not run — never a binding on malformed evidence (Q1's rule). One-liner for the variant: fixed outcomes, growing readers → sum type + exhaustive match; concrete users `resolve_edges`; rejected alternative a `reason` field on `CategorizedUnresolvedEdge` that would move basis selection into the orchestrator.
4. **One basis, one classifier rule, one reader arm (classification + agent).** `UnresolvedEdgeBasisCode::SelfCallAmbiguousMro` (`self_call_ambiguous_mro`; version stays 6 per types.rs:442-445); `classify_unresolved_edge`'s new first rule reads `mroCandidates` (array of ≥2 strings) → `internal_candidate` + the basis; a malformed `mroCandidates` is ignored and the existing rules run (pinned; written and read inside one index run); `attribution_class` maps the basis to `OwnCodeUnresolved` (`your own code (call target not resolved)`) — `EXPECTED` 17 → 18. `blast_radius.rs:75` (`_ => LocalLike`) and `unresolved_classifier.rs:572` (`_ => None`) are the correct fallbacks — stated, not edited. The category vocabulary, `CALLS_CATEGORIES`, trust's tables and the enrichment wire enum are untouched (D-PSB-001).
5. **Fixture corpus.** NEW `classification-parity-fixtures/classify__self-mro-ambiguous/` (the `classify__this-receiver` shape); the indexer parity corpus pins no edge counts (D11) and is untouched; the repo-index Python fixture is untouched and gains a test.
6. **Nothing else moves:** no schema, no wire change beyond the additive basis literal and the metadata keys, no CLI change, no category change, no other language's binding.

### 2.2 Evidence taxonomy (one row = one bound test)

| Input | Outcome | Bound test / check |
|---|---|---|
| `self.g()` / `cls.g()` inside class C | carrier `{selfCall:true, enclosingClass:"C"}` | PSB-C01 (two tests) |
| `self.extra.get()`; self-call at module level | no carrier | PSB-C01 (two tests) |
| own class defines `m` (same file) | bound at depth 0 | PSB-C02 `self_call_binds_to_the_own_class_method_in_the_same_file` |
| unique ancestor defines `m` | bound at depth n | PSB-C02 `self_call_binds_to_a_unique_ancestor_method` |
| two ancestors at one depth define `m` | unresolved, `mroCandidates` = both, category unchanged | PSB-C02 `…collision_stays_unresolved_with_mro_candidates` |
| no ancestor defines `m` | today's fallbacks, byte-identical | PSB-C02 `…falls_through_unchanged` |
| ancestor name matches two CLASS nodes | that branch ends; unresolved as today | PSB-C02 `…same_name_ancestor_classes_stop_the_walk` |
| carrier malformed (`selfCall: "yes"`, `enclosingClass` missing/non-string) | never binds; today's path | PSB-C02 `self_call_malformed_carrier_never_binds` |
| `superclass` raw text forms / absent | parsed list / empty | PSB-C02 `superclass_metadata_parses_raw_python_base_lists`; PSB-C04 |
| `mroCandidates` ≥2 strings / malformed | `internal_candidate` + `self_call_ambiguous_mro` / existing rules | PSB-C03 (two tests + the parity fixture) |
| the basis on the reader surface | `OwnCodeUnresolved`; EXPECTED 18 | PSB-C03 |
| fixture `self._process_user(user)` / `self._service.process()` | bound / unresolved; twice → same graph | PSB-C05 |
| corpus: django, leveldb, FRAKTAG | witnesses + seams; byte-stable | PSB-C08/C09/C10 |

### 2.3 Outward proof (what a user of the product gains)

django `explain BaseHandler.get_response` → `Callers (2)`: `__call__ (django/core/handlers)  django/core/handlers/wsgi.py:120` and `__call__ (django/test)  django/test/client.py:169` (rows anchor the caller's declaration; the call sites are :124 and :186) — the two real callers, not `ASGIHandler` (which awaits `get_response_async`), not exception.py's closures; `callers` → `2 callers found`; thousands of `self.m()` calls across django bind on hierarchy evidence (REPORTED by the seam); a real MRO collision — `django/contrib/auth/forms.py:576 self.validate_passwords()` where both `SetUnusablePasswordMixin` (:166) and `SetPasswordMixin` (:107) define it — stays unresolved and is named with both candidates; trust's resolved share rises and its own-code bucket names the collisions. leveldb and FRAKTAG unchanged to the byte.

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-005-L01, P-PSB-01 | the C-family gate or receiver evidence | PSB-C02 (`call_binding_receiver`; Q1 family); PSB-C06; PSB-C09 (leveldb) |
| RG-REQ-006-L01/L02/L04 | another resolver stage | PSB-C06 (`rust_crate_import_*`, `java_suffix_*`, the TS import tests) |
| TS `this.` | the rewrite | PSB-C06 (`this_method_rewritten_to_class_name`); PSB-C09 (FRAKTAG) |
| RG-REQ-005-L07 | forward-decl reads | PSB-C02/C06 (`forward_decl_classification_reads_the_stamped_key`) |
| RG-REQ-001-L03, P-PSB-04 | a wrong binding removing an unresolved row | PSB-C08 seam |
| RG-REQ-005-L09 | the category/basis vocabulary the zero-state reads | PSB-C03; PSB-C06 (`CALLS_CATEGORIES` untouched) |
| RG-REQ-001-L09 | determinism | PSB-C05 (index twice) |
| P-PSB-02/03 | any other extractor, trust, existing test | PSB-C06; PSB-C09 |
| RG-REQ-011-L06, P-PSB-05 | isolation | PSB-C11 |

## 4. Stop conditions

Frozen: the shared resolver's path for every other language (the stage keys on the carrier only Python writes), the category vocabulary and `CALLS_CATEGORIES`, the trust crate, the enrichment wire enum, schema (no column), the CLI, exit codes. No import-binding resolution of ancestor heads beyond the simple-name rule (a cross-package base stays unresolved — no guessing). Nothing outside the eleven candidate paths (a literal site or match arm the compiler names outside them is a FINDING — stop and report). STANDING HONESTY RULES (a malformed carrier never binds; a collision is named, never silently picked). Unmet DoD → STOP. Do NOT commit. Run every check to completion in the FOREGROUND; end your turn only with the evidence object.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

0. On the CLEAN tree: record `git rev-parse HEAD`; `(cd rust && cargo build --release --bin rmap --bin rmapd)` (warm cache) → `mkdir -p /private/tmp/PYTHON-SELF-BINDING-1-before-bin && cp rust/target/release/rmap rust/target/release/rmapd /private/tmp/PYTHON-SELF-BINDING-1-before-bin/`; `registry-before <full sha256>` in build-progress.md; verify the three `-before` roots exist (§7).
1. Failing tests FIRST (the §2.2 names): extractor carrier → superclass parse + node/index fields (compiler-listed literals) → the stage + variant + arm → basis + classifier rule + attribution arm + parity fixture → the repo-index test.
2. Chunked gates PSB-C01 → C02 → C03 → C04 → C05, then PSB-C06 (whole suites, foreground) and PSB-C07 — NEVER `cargo test --workspace`.
3. `(cd rust && cargo build --release --bin rmap --bin rmapd)` (candidate) → PSB-C08 (django, ~18 s index) → C09 (leveldb, FRAKTAG) → C10; every `/tmp/psb-*` capture from THIS cycle's run.
4. PSB-C11 (remove the before-bin dir; the three `-after` roots STAY), hand-off with the evidence object (each check's outcome NESTED under `outcome`; `supportingEvidence` non-empty).

## 6. Definition of done

All eleven checks pass; §2.3 holds on django; leveldb and FRAKTAG byte-stable; the report carries the code-under-analysis examples: the two caller sites quoted with file:line (wsgi.py:124, client.py:186), the callers' declarations the rows anchor (:120, :169) and their class declarations (:113, :158), the non-caller `asgi.py:231`, the collision `forms.py:576` with both definitions (:107, :166), one bound ancestor-depth example and one own-class example from the seam (quote the source lines), one `self.attr.m()` chain that stays unresolved (`django/contrib/gis/db/models/aggregates.py:34 self.extra.get`), one self-call whose base is external and stays unresolved (`tests/utils_tests/test_autoreload.py:447 self.assertIs` in `TestCheckErrors(SimpleTestCase)`), and the seam's numbers.

## 7. Corpus roots (pre-provisioned by the manager on 2026-09-20 with the base-revision binary; rebuild recipe)

- `/private/tmp/PYTHON-SELF-BINDING-1-django-before`: fresh `rmap index` of `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django` (18 s; stdio transport, auto passes off); `…-leveldb-before` (1 s); `…-fraktag-before`: fresh index of `/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG` (1 s).
- The builder creates the `-after` roots (fresh indexes by the candidate) and leaves them for the manager; the retained roots under `~/repo-graph-retained/` are never served; the operator registry is read only for its digest.

## 8. Follow-ups (not this slice)

PYTHON-BASES-EDGES-1: real inheritance edges for Python (today only the raw `superclass` string; Java emits `Extends`), which would let `explain <Class>` render bases/derived (Q6 increments 2–3 shape) and let the self-call walk use node identity for ancestors instead of the simple-name rule. PYTHON-ATTRIBUTE-CALLS-1: `self.attr.m()` chains (8,362 on django) need instance-attribute type evidence. TRUST-CEILING-WORDING-1: trust's `call-graph resolution is at this build's ceiling for Python (no resolver exists)` reads the enrichment table and will read as false beside resolved self-calls — reword to name enrichment. CLAIM-INVARIANT-1 (RG-REQ-005-L09's zero-state count on the call surfaces) unchanged.

## 9. Baseline history

- INPUT-1 (2026-09-20, reviewed, never approved): the document review (codex gpt-5.6-terra, cycle 1) raised D-PSB-002 — the allocation's basis-code design diverged from L03's then-wording "its own category" — and F-PSB-001 (callers rows anchor the caller's declaration line, wsgi.py:120 / client.py:169, not the call sites); superseded uncommitted.
- INPUT-2 (2026-09-21): first admission baseline — the same allocation under the ratified L03 wording (D-PSB-002 option B, human 2026-09-21; bootstrap RG-BOOTSTRAP-INPUT-5), with F-PSB-001 corrected; pins D-PSB-001 and D-PSB-002; generated from the four parent requirement files' source closure.
