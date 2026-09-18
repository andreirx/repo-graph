<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "CPP-INCLUDE-ROOTS-1",
  "baselinePath": "docs/requirements/baselines/CPP-INCLUDE-ROOTS-1-INPUT-2.json",
  "parentRequirementIds": [
    "RG-REQ-001",
    "RG-REQ-002",
    "RG-REQ-004",
    "RG-REQ-005",
    "RG-REQ-006",
    "RG-REQ-009",
    "RG-REQ-011"
  ],
  "implements": [
    "RG-REQ-006-L03",
    "RG-REQ-001-L02"
  ],
  "preserves": [
    "RG-REQ-006-L01",
    "RG-REQ-006-L02",
    "RG-REQ-006-L04",
    "RG-REQ-005-L01",
    "RG-REQ-005-L02",
    "RG-REQ-005-L07",
    "RG-REQ-005-L09",
    "RG-REQ-004-L02",
    "RG-REQ-004-L07",
    "RG-REQ-004-L09",
    "RG-REQ-011-L03",
    "RG-REQ-011-L06",
    "RG-REQ-002-L01"
  ],
  "preservationObligationIds": [
    "P-CIR-01",
    "P-CIR-02",
    "P-CIR-03",
    "P-CIR-04"
  ],
  "changes": [
    "RG-REQ-002-L06",
    "RG-REQ-009-L04"
  ],
  "acceptanceBoundary": "The modules list / cycles / trust outputs of the freshly built rmap on fresh isolated indexes of poco (both the recorded base revision's binary and the candidate), the store-level include-resolution counts and resolved pair sets of those indexes, the byte-stability of fresh isolated indexes of leveldb (C++), nginx (C) and codegraph (TypeScript+Rust), a read-only COPY of the retained v0.18.0 poco store for the before-fingerprint, plus cargo test -p repo-graph-indexer (lib incl. include_resolver:: and derived_root*, parity) -p repo-graph-repo-index (cpp_include_roots) -p repo-graph-classification -p repo-graph-storage (modules_list_unresolved_import_count, retention_prune_benchmark_gate) -p repo-graph-rgr (modules_list, cycles) as named per check.",
  "candidatePaths": [
    "rust/crates/indexer/src/include_resolver.rs",
    "rust/crates/indexer/src/orchestrator.rs",
    "rust/crates/classification/src/types.rs",
    "rust/crates/storage/src/trust_impl.rs",
    "rust/crates/repo-index/tests/cpp_include_roots.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/CPP-INCLUDE-ROOTS-1/verification.json",
    "docs/assurance/CPP-INCLUDE-ROOTS-1/implementation-review.json"
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
      "checkId": "CIR-C01",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib derived_root 2>&1 | tee /tmp/cir-c01.txt | grep -E 'test result: ok\\. (9|[1-9][0-9]+) passed; 0 failed' && for t in derived_root_resolves_header_under_nested_include_dir derived_root_resolves_header_under_nested_inc_dir derived_root_ambiguous_when_two_derived_roots_hold_the_header derived_root_pooled_with_configured_root_is_ambiguous derived_root_same_directory_hit_still_wins derived_root_ignores_a_file_named_include derived_root_is_case_sensitive derived_root_angle_bracket_resolves derived_roots_are_deterministic_regardless_of_file_order; do grep -qE \"^test .*$t .* ok$\" /tmp/cir-c01.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the derived_root* unit tests in rust/crates/indexer/src/include_resolver.rs (section 2.3's evidence taxonomy, one test per row) plus any further regression test the family gains"
      },
      "expected": "exit 0: at least nine derived_root* tests pass with 0 failed AND each of the nine NAMED tests is listed as ok — the count is a lower bound so a regression test added under review does not turn a green family red; a missing named test or any failure fails the check."
    },
    {
      "checkId": "CIR-C02",
      "obligationIds": [
        "RG-REQ-006-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib include_resolver:: 2>&1 | tee /tmp/cir-c02.txt | grep -E 'test result: ok\\. ([3-9][0-9]|[1-9][0-9]{2,}) passed; 0 failed' && for t in same_directory_resolves same_directory_wins_over_include_root include_root_resolves inc_root_resolves src_include_root_resolves header_to_header_same_dir header_to_header_via_include_root subpath_include_resolves subpath_does_not_match_basename configured_root_resolves ambiguous_when_multiple_roots_have_same_header angle_bracket_resolves_via_conventional_root angle_bracket_skips_same_dir angle_bracket_unresolved_when_no_local_match angle_bracket_resolves_if_local_header_exists angle_bracket_ambiguous_when_multiple_matches quoted_still_checks_same_dir_first no_sibling_directory_magic no_suffix_guessing configured_and_conventional_both_matching_is_ambiguous; do grep -qE \"^test .*$t .* ok$\" /tmp/cir-c02.txt || { echo \"MISSING $t\"; exit 1; }; done && ! grep -q 'configured_root_wins_over_conventional' /tmp/cir-c02.txt && ! grep -q 'fn configured_root_wins_over_conventional' crates/indexer/src/include_resolver.rs",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the whole include_resolver test module (24 tests at the base revision; 9 new + 1 renamed expected): the 19 named pre-existing tests keep their names and assertions and construct their resolver through the derived-roots path; the test formerly named configured_root_wins_over_conventional (it asserts AMBIGUOUS, not a win) is renamed configured_and_conventional_both_matching_is_ambiguous with its body unchanged (section 2.4; decision D-TME-TEST-NAME-1's rule)"
      },
      "expected": "exit 0: at least 30 include_resolver tests pass with 0 failed; each of the 19 named pre-existing tests and the renamed test is listed as ok; the old misnomer is absent from both the run and the source. The pre-existing tests' behaviour is preserved (same-directory precedence, root-level include/inc/src/include still resolve, configured+conventional overlap is ambiguous, no sibling-directory magic, no suffix guessing)."
    },
    {
      "checkId": "CIR-C03",
      "obligationIds": [
        "RG-REQ-006-L01",
        "RG-REQ-006-L02",
        "RG-REQ-006-L04",
        "RG-REQ-005-L01",
        "RG-REQ-005-L02",
        "RG-REQ-005-L07",
        "RG-REQ-005-L09",
        "P-CIR-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib 2>&1 | tee /tmp/cir-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/cir-c03.txt && for t in rust_crate_import java_suffix aliased_named_import_uses_imported_name_for_lookup receiver_binding_ categorize_calls forward_decl_classification_reads_the_stamped_key ambiguous_name_stays_unresolved; do grep -qE \"^test .*$t.* ok$\" /tmp/cir-c03.txt || { echo \"MISSING $t\"; exit 1; }; done && git -C .. diff --quiet -- rust/crates/indexer/src/resolver.rs",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the whole indexer lib suite; the named families of the OTHER resolver stages (Rust crate, Java suffix, TS aliased import, C++ receiver binding, unresolved-call categorisation, forward-decl classification, unique-name ambiguity); git diff of rust/crates/indexer/src/resolver.rs"
      },
      "expected": "exit 0: the whole indexer lib suite is green, every named family is listed as ok, and rust/crates/indexer/src/resolver.rs is byte-identical to the base revision — no-behavior-change for the Rust/Java/TS import stages and for C++ call binding (RG-REQ-006-L01/L02/L04, RG-REQ-005-L01/L02/L07/L09; P-CIR-02)."
    },
    {
      "checkId": "CIR-C04",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --test parity 2>&1 | tee /tmp/cir-c04.txt | grep -E 'test result: ok\\.' && grep -E '^test parity_against_shared_indexer_fixture_corpus .* ok$' /tmp/cir-c04.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the shared indexer fixture corpus parity test (per-file extraction fixtures; no include/inc directory exists in the corpus, so include resolution cannot move it)"
      },
      "expected": "exit 0: parity_against_shared_indexer_fixture_corpus is listed as ok — extraction output remains identical (the slice changes resolution, not extraction)."
    },
    {
      "checkId": "CIR-C05",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02",
        "RG-REQ-002-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-repo-index --test cpp_include_roots 2>&1 | tee /tmp/cir-c05.txt | grep -E 'test result: ok\\. ([2-9]|[1-9][0-9]+) passed; 0 failed' && for t in indexed_cpp_include_resolves_through_nested_include_root indexed_cpp_include_ambiguous_across_two_include_roots_is_counted_not_bound; do grep -qE \"^test .*$t .* ok$\" /tmp/cir-c05.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the NEW source→store test file rust/crates/repo-index/tests/cpp_include_roots.rs: it writes its own fixture into a temp dir — (a) Foundation/include/Poco/Exception.h and Net/src/a.cpp containing #include \"Poco/Exception.h\" → after index_path the store holds ONE resolved IMPORTS edge from the Net/src/a.cpp FILE node to the Foundation/include/Poco/Exception.h FILE node and no unresolved imports_* row for it; (b) the same header duplicated under Util/include/Poco/ and included from Net/src/b.cpp → NO IMPORTS edge and exactly one unresolved_edges row with category imports_ambiguous_match for that include"
      },
      "expected": "exit 0: both named end-to-end tests pass — a nested include root resolves through extractor→indexer→storage, and a header present under two derived roots is counted as imports_ambiguous_match and never bound (no guessing between candidates)."
    },
    {
      "checkId": "CIR-C06",
      "obligationIds": [
        "RG-REQ-004-L02",
        "RG-REQ-004-L09",
        "RG-REQ-002-L06",
        "P-CIR-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-classification 2>&1 | tee /tmp/cir-c06a.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/cir-c06a.txt && grep -qE '^test .*is_imports_category_accepts_all_four_import_categories_and_rejects_others .* ok$' /tmp/cir-c06a.txt && ! grep -q 'is_imports_category_only_matches_imports_file_not_found' crates/classification/src/types.rs && cargo test -p repo-graph-storage modules_list_unresolved_import_count 2>&1 | tee /tmp/cir-c06b.txt | grep -E 'test result: ok\\. 1 passed; 0 failed' && grep -qE '^test .*modules_list_unresolved_import_count_includes_all_import_categories .* ok$' /tmp/cir-c06b.txt && ! grep -q 'includes_all_three_import_categories' crates/storage/src/trust_impl.rs && cargo test -p repo-graph-rgr modules_list 2>&1 | tee /tmp/cir-c06c.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/cir-c06c.txt && for t in two_crate_fixture_renders_a_to_b_edge_verbatim list_render_empty_edges_is_zero_state list_render_absent_edges_field_is_unavailable_with_reason; do grep -qE \"^test .*$t .* ok$\" /tmp/cir-c06c.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the classification crate suite (MODULES_LIST_UNRESOLVED_IMPORT_CATEGORIES now holds the four import categories; the test formerly named is_imports_category_only_matches_imports_file_not_found — its body never asserted an 'only' — is expanded to assert all four import categories and renamed is_imports_category_accepts_all_four_import_categories_and_rejects_others); the storage regression test renamed from modules_list_unresolved_import_count_includes_all_three_import_categories to modules_list_unresolved_import_count_includes_all_import_categories, now asserting that imports_ambiguous_match IS counted (section 2.5); the rgr modules-list render tests"
      },
      "expected": "exit 0: the headline unresolved-import count includes the ambiguous-match category (both renamed tests pass; both old names are gone from their sources); the module-edge derivation and zero-state render tests remain green — no-behavior-change for RG-REQ-004-L02 (edges derive only from resolved cross-module file→file imports) and RG-REQ-004-L09 (the zero-state sentence and its denominators; P-CIR-03)."
    },
    {
      "checkId": "CIR-C07",
      "obligationIds": [
        "RG-REQ-004-L07",
        "RG-REQ-011-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr cycles 2>&1 | tee /tmp/cir-c07a.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/cir-c07a.txt && for t in zero_state_states_module_and_edge_counts zero_state_names_empty_graph_when_no_resolved_edges; do grep -qE \"^test .*$t .* ok$\" /tmp/cir-c07a.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-storage retention_prune_benchmark_gate 2>&1 | tee /tmp/cir-c07b.txt | grep -E 'test result: ok\\. 1 passed; 0 failed'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the rgr cycles presentation tests (zero-state wording) and the storage retention benchmark gate"
      },
      "expected": "exit 0: the cycles zero-state tests and the retention benchmark gate remain green — no-behavior-change for the cycles walk/render (RG-REQ-004-L07) and for bounded retention (RG-REQ-011-L03); the poco store-growth cost itself is REPORTED in CIR-C14."
    },
    {
      "checkId": "CIR-C08",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-004-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/CPP-INCLUDE-ROOTS-1-ret && mkdir -p /private/tmp/CPP-INCLUDE-ROOTS-1-ret && cp \"$HOME/repo-graph-retained/audit-v0.18.0/databases/9858deeeefef8599.db\" /private/tmp/CPP-INCLUDE-ROOTS-1-ret/poco.db && D=/private/tmp/CPP-INCLUDE-ROOTS-1-ret/poco.db && R=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='IMPORTS'\") && NF=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_file_not_found'\") && AM=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_ambiguous_match'\") && OT=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category IN ('imports_wildcard','imports_ambiguous_suffix')\") && PU=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'imports_%' AND target_key LIKE 'Poco/%'\") && echo \"RETAINED resolved=$R not_found=$NF ambiguous=$AM other=$OT poco_prefixed_unresolved=$PU\" | tee /tmp/cir-retained.txt && test \"$R\" = 1702 && test \"$NF\" = 13703 && test \"$AM\" = 0 && test \"$PU\" -ge 1",
        "cwd": ".",
        "environment": "a COPY of the retained v0.18.0 poco store (read with sqlite3 only; the retained root itself is never served)",
        "inputs": "the retained audit root's poco database 9858deeeefef8599.db (v0.18.0, 2026-09-08); literals read by the manager on 2026-09-18: 1,702 resolved IMPORTS; 13,703 imports_file_not_found; 0 imports_ambiguous_match"
      },
      "expected": "exit 0: the v0.18.0 fingerprint matches exactly (1,702 resolved IMPORTS; 13,703 unresolved imports_file_not_found; 0 ambiguous) and at least one unresolved include names a Poco/… header — the defect RC-10 describes exists in the shipped store (the zero-state poco renders today is a faithful render of this never-resolved include graph)."
    },
    {
      "checkId": "CIR-C09",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "B=$(python3 -c \"import json;print(json.load(open('.agent-manager/slices/CPP-INCLUDE-ROOTS-1/status.json'))['candidateTracking']['baseRevision'])\") && rm -rf /private/tmp/CPP-INCLUDE-ROOTS-1-before && git worktree add --detach /private/tmp/CPP-INCLUDE-ROOTS-1-before $B && (cd /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust && cargo build --release -p repo-graph-rgr -p rmapd 2>&1 | tail -n1) && rm -rf /private/tmp/CPP-INCLUDE-ROOTS-1-poco-before && T0=$(date +%s) && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release:$PATH /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release/rmap index ) >/tmp/cir-index-before.txt 2>&1 && SECS=$(( $(date +%s) - T0 )) && D=$(ls /private/tmp/CPP-INCLUDE-ROOTS-1-poco-before/databases/*.db | head -n1) && BYTES=$(stat -f %z $D) && R=$(sqlite3 $D \"SELECT count(*) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid JOIN nodes t ON t.node_uid=e.target_node_uid WHERE e.type='IMPORTS' AND s.kind='FILE' AND t.kind='FILE'\") && ME=$(sqlite3 $D \"SELECT count(*) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid WHERE e.type='IMPORTS' AND s.kind!='FILE'\") && NF=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_file_not_found'\") && AM=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_ambiguous_match'\") && OT=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category IN ('imports_wildcard','imports_ambiguous_suffix')\") && PU=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'imports_%' AND target_key LIKE 'Poco/%'\") && sqlite3 $D \"SELECT substr(s.stable_key, instr(s.stable_key, ':')+1) || ' ' || substr(t.stable_key, instr(t.stable_key, ':')+1) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid JOIN nodes t ON t.node_uid=e.target_node_uid WHERE e.type='IMPORTS'\" | sort > /tmp/cir-poco-before-pairs.txt && echo \"BEFORE resolved=$R not_found=$NF ambiguous=$AM other=$OT poco_prefixed_unresolved=$PU total=$((R+NF+AM+OT)) module_edges=$ME secs=$SECS bytes=$BYTES\" | tee /tmp/cir-poco-before.txt && test \"$PU\" -ge 1",
        "cwd": ".",
        "environment": "a git worktree of the recorded base revision built once (before-binary and its rmapd); a fresh isolated index of poco in a throwaway root served over stdio with auto passes off",
        "inputs": "the poco checkout; .agent-manager/slices/CPP-INCLUDE-ROOTS-1/status.json for the base revision"
      },
      "expected": "exit 0: the defect reproduces at the base revision on a FRESH poco index (at least one unresolved include names a Poco/… header); the before numbers (FILE→FILE resolved IMPORTS, module-level IMPORTS edges, unresolved by category, their total, the sorted resolved source→target pair list, wall seconds, store bytes) are RECORDED in /tmp/cir-poco-before.txt, /tmp/cir-poco-before-pairs.txt and build-progress.md. No number other than poco_prefixed_unresolved ≥ 1 is predicted."
    },
    {
      "checkId": "CIR-C10",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02",
        "RG-REQ-002-L01",
        "RG-REQ-004-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo build --release -p repo-graph-rgr -p rmapd --manifest-path rust/Cargo.toml 2>&1 | tail -n1 && rm -rf /private/tmp/CPP-INCLUDE-ROOTS-1-poco && T0=$(date +%s) && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/tmp/cir-index-after.txt 2>&1 && SECS=$(( $(date +%s) - T0 )) && D=$(ls /private/tmp/CPP-INCLUDE-ROOTS-1-poco/databases/*.db | head -n1) && BYTES=$(stat -f %z $D) && R=$(sqlite3 $D \"SELECT count(*) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid JOIN nodes t ON t.node_uid=e.target_node_uid WHERE e.type='IMPORTS' AND s.kind='FILE' AND t.kind='FILE'\") && ME=$(sqlite3 $D \"SELECT count(*) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid WHERE e.type='IMPORTS' AND s.kind!='FILE'\") && NF=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_file_not_found'\") && AM=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category='imports_ambiguous_match'\") && OT=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category IN ('imports_wildcard','imports_ambiguous_suffix')\") && PU=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'imports_%' AND target_key LIKE 'Poco/%'\") && sqlite3 $D \"SELECT substr(s.stable_key, instr(s.stable_key, ':')+1) || ' ' || substr(t.stable_key, instr(t.stable_key, ':')+1) FROM edges e JOIN nodes s ON s.node_uid=e.source_node_uid JOIN nodes t ON t.node_uid=e.target_node_uid WHERE e.type='IMPORTS'\" | sort > /tmp/cir-poco-after-pairs.txt && echo \"AFTER resolved=$R not_found=$NF ambiguous=$AM other=$OT poco_prefixed_unresolved=$PU total=$((R+NF+AM+OT)) module_edges=$ME secs=$SECS bytes=$BYTES\" | tee /tmp/cir-poco-after.txt && RB=$(sed -E 's/^BEFORE resolved=([0-9]+) .*/\\1/' /tmp/cir-poco-before.txt) && TB=$(sed -E 's/^BEFORE .* total=([0-9]+) module_edges=.*/\\1/' /tmp/cir-poco-before.txt) && test \"$R\" -gt \"$RB\" && test $((R+NF+AM+OT)) = \"$TB\" && test \"$(comm -23 /tmp/cir-poco-before-pairs.txt /tmp/cir-poco-after-pairs.txt | wc -l | tr -d ' ')\" = 0 && grep -q '^Net/src/AbstractHTTPRequestHandler.cpp:FILE Foundation/include/Poco/Exception.h:FILE$' /tmp/cir-poco-after-pairs.txt && test \"$SECS\" -le 300",
        "cwd": ".",
        "environment": "the freshly built candidate rmap + rmapd; a fresh isolated index of poco in a throwaway root, stdio, auto passes off",
        "inputs": "the poco checkout; /tmp/cir-poco-before.txt and /tmp/cir-poco-before-pairs.txt from CIR-C09"
      },
      "expected": "exit 0 requires ALL of: (1) resolved IMPORTS strictly greater than before; (2) INCLUDE-SITE CONSERVATION — FILE→FILE resolved IMPORTS + unresolved imports_* rows after == the same sum before (the persisted MODULE→MODULE directory edges are counted separately as module_edges and REPORTED — they grow as includes resolve, by construction) (no include is dropped or invented: every newly resolved include left the unresolved set, RG-REQ-006-L03/RG-REQ-001-L02); (3) NEVER-FLIP — every resolved source→target pair of the before index is still resolved to the same target after (comm -23 empty), so adding roots only turned Unresolved into Resolved or Ambiguous; (4) the witness include Net/src/AbstractHTTPRequestHandler.cpp:20 `#include \"Poco/Exception.h\"` → Foundation/include/Poco/Exception.h is a resolved FILE→FILE edge (RC-10 named HTTPClientSession.cpp, which does not include that header in the indexed revision — OC-2); (5) index wall time ≤ 300 s (the smoke client window — a STOP condition otherwise). The magnitudes (how many resolved, how many ambiguous, the residual Poco/-prefixed unresolved, store bytes) are REPORTED, never predicted."
    },
    {
      "checkId": "CIR-C11",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-004-L02",
        "RG-REQ-004-L09",
        "RG-REQ-002-L06",
        "RG-REQ-002-L01",
        "RG-REQ-009-L04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -f /tmp/cir-modules-before.txt /tmp/cir-modules-after.json /tmp/cir-modules-after.txt /tmp/cir-trust-before.json /tmp/cir-trust-after.json && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release:$PATH /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release/rmap modules list ) 2>/dev/null > /tmp/cir-modules-before.txt && grep -q '^No cross-module dependencies detected\\.' /tmp/cir-modules-before.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" modules list --json ) 2>/dev/null > /tmp/cir-modules-after.json && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" modules list ) 2>/dev/null > /tmp/cir-modules-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release:$PATH /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release/rmap trust --json ) 2>/dev/null > /tmp/cir-trust-before.json && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust --json ) 2>/dev/null > /tmp/cir-trust-after.json && D=$(ls /private/tmp/CPP-INCLUDE-ROOTS-1-poco/databases/*.db | head -n1) && STORE_UNRES=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category IN ('imports_file_not_found','imports_ambiguous_match','imports_wildcard','imports_ambiguous_suffix')\") && python3 -c \"import json,sys\nd=json.load(open('/tmp/cir-modules-after.json'))\nedges=d['edges'] or []\nassert len(edges)>=1, 'no edges'\nname=lambda s: s.split(':')[-1].rstrip('/')\nnf=[e for e in edges if name(e['source'])=='Net' and name(e['target'])=='Foundation']\nassert nf, [ (e['source'],e['target']) for e in edges[:10]]\nassert d['unresolved_import_count']==int(sys.argv[1]), (d['unresolved_import_count'], sys.argv[1])\nprint('edges',len(edges),'| Net->Foundation import_count',nf[0]['import_count'],'| unresolved_import_count',d['unresolved_import_count'],'== store',sys.argv[1])\" \"$STORE_UNRES\" && grep -qE '^[0-9]+ cross-module dependenc(y|ies).* detected\\.' /tmp/cir-modules-after.txt && grep -q \"(${STORE_UNRES} imports unresolved)\" /tmp/cir-modules-after.txt && ! grep -q 'No cross-module dependencies detected' /tmp/cir-modules-after.txt && grep -q 'Net → Foundation (' /tmp/cir-modules-after.txt && python3 -c \"import json\nfor tag,f in (('BEFORE','/tmp/cir-trust-before.json'),('AFTER','/tmp/cir-trust-after.json')):\n    ig=json.load(open(f))['value']['reliability']['value']['import_graph']\n    assert isinstance(ig.get('level'),str) and isinstance(ig.get('reasons'),list), (tag, ig)\n    print('TRUST', tag, 'import_graph level', ig['level'], 'reasons', ig['reasons'])\"",
        "cwd": ".",
        "environment": "before-binary on the CIR-C09 index; candidate on the CIR-C10 index",
        "inputs": "the two poco indexes; the store's unresolved-import count over the four import categories"
      },
      "expected": "exit 0: the before-binary's `modules list` on the fresh before index renders the zero-state sentence (faithful to an empty edge list); the candidate's `modules list --json` on the fresh after index has a non-empty edge list containing a Net → Foundation edge (its import_count REPORTED), and its unresolved_import_count EQUALS the store's count over all four unresolved-import categories — ambiguity is COUNTED where the user reads the count (RG-REQ-006-L03 'unresolved and counted'; RG-REQ-002-L06 coverage stated; RG-REQ-002-L01 remains satisfied: no rendered edge without a stored resolved import); the human render shows the count line, the same '(N imports unresolved)' clause, the 'Net → Foundation (' row, and no zero-state sentence. trust --json before/after is captured by failure-propagating invocations (every capture file removed first) and both documents are parsed: the Import-graph axis (`value.reliability.value.import_graph`: `level` and its `reasons`, e.g. `unresolved_imports=N`) is printed for poco before and after and REPORTED with attribution (RG-REQ-009-L04 — a pre-authorised movement, never predicted)."
    },
    {
      "checkId": "CIR-C12",
      "obligationIds": [
        "RG-REQ-004-L07",
        "RG-REQ-004-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -f /tmp/cir-cyclesj-before.json /tmp/cir-cyclesj-after.json && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release:$PATH /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release/rmap cycles --json ) > /tmp/cir-cyclesj-before.json 2>/dev/null && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/poco' && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-poco RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-poco/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" cycles --json ) > /tmp/cir-cyclesj-after.json 2>/dev/null && python3 -c \"import json\nb=json.load(open('/tmp/cir-cyclesj-before.json'))\na=json.load(open('/tmp/cir-cyclesj-after.json'))\nsets=lambda d: sorted(sorted((n.get('qualified_name') or n['name']) for n in c['nodes']) for c in d['cycles'])\nprint('BEFORE count',b['count'],'module_edge_count',b['module_edge_count'],'sets',sets(b))\nprint('AFTER count',a['count'],'module_edge_count',a['module_edge_count'],'sets',sets(a))\nassert a['module_edge_count']>=1\"",
        "cwd": ".",
        "environment": "before-binary on the CIR-C09 index; candidate on the CIR-C10 index",
        "inputs": "the two poco indexes"
      },
      "expected": "exit 0: both `cycles --json` outputs parse; the candidate's module_edge_count is ≥ 1 (edges now exist); the cycle count and member sets before and after are REPORTED verbatim — whatever SCCs poco's real include graph has are rendered from resolved edges only (RG-REQ-004-L07), and the cycles zero-state, if any, carries its counts (RG-REQ-004-L09). No cycle count is predicted."
    },
    {
      "checkId": "CIR-C13",
      "obligationIds": [
        "RG-REQ-006-L01",
        "RG-REQ-006-L02",
        "RG-REQ-006-L04",
        "RG-REQ-005-L01",
        "P-CIR-01",
        "P-CIR-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "for R in leveldb nginx codegraph; do SRC='/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R; rm -rf /private/tmp/CPP-INCLUDE-ROOTS-1-$R-before /private/tmp/CPP-INCLUDE-ROOTS-1-$R; ( cd \"$SRC\" && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-$R-before RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release:$PATH /private/tmp/CPP-INCLUDE-ROOTS-1-before/rust/target/release/rmap index ) >/dev/null 2>&1 && ( cd \"$SRC\" && export RMAP_STATE_ROOT=/private/tmp/CPP-INCLUDE-ROOTS-1-$R RMAP_SOCKET_PATH=/private/tmp/CPP-INCLUDE-ROOTS-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/dev/null 2>&1 && Q=\"SELECT type, resolution, count(*) FROM edges GROUP BY type, resolution; SELECT category, count(*) FROM unresolved_edges GROUP BY category; SELECT count(*) FROM nodes; SELECT count(*) FROM files WHERE is_test=1;\" && sqlite3 $(ls /private/tmp/CPP-INCLUDE-ROOTS-1-$R-before/databases/*.db | head -n1) \"$Q\" > /tmp/cir-$R-before.txt && sqlite3 $(ls /private/tmp/CPP-INCLUDE-ROOTS-1-$R/databases/*.db | head -n1) \"$Q\" > /tmp/cir-$R-after.txt && cmp /tmp/cir-$R-before.txt /tmp/cir-$R-after.txt && echo \"$R byte-stable: $(wc -l < /tmp/cir-$R-after.txt) fact lines\" || exit 1; done",
        "cwd": ".",
        "environment": "fresh isolated indexes of leveldb (C++; exactly ONE include directory, the root-level include/ — the former literal root, now derived from the file list: the retained-root-level case), nginx (C; no include/inc directory) and codegraph (TypeScript + Rust; no include/inc directory; the audit's control repo) with the before-binary and the candidate, throwaway roots, stdio, auto passes off",
        "inputs": "the three checkouts; the before-binary from CIR-C09"
      },
      "expected": "exit 0: for all three corpora the edge counts by type and resolution, the unresolved counts by category, the node count and the is_test partition are byte-identical before/after — leveldb's only include directory is the root-level include/ that the former literal already covered, so deriving it changes no resolution (the retained-root-level case); nginx and codegraph have no include/inc directory and derive no root; and the Rust/Java/TypeScript/Python stages are untouched. A differing line is a STOP (no-behavior-change for RG-REQ-006-L01/L02/L04 and RG-REQ-005-L01; P-CIR-01 other ecosystems byte-stable; P-CIR-02 call binding untouched)."
    },
    {
      "checkId": "CIR-C14",
      "obligationIds": [
        "RG-REQ-011-L03",
        "RG-REQ-006-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "SB=$(sed -E 's/.*secs=([0-9]+) .*/\\1/' /tmp/cir-poco-before.txt) && SA=$(sed -E 's/.*secs=([0-9]+) .*/\\1/' /tmp/cir-poco-after.txt) && BB=$(sed -E 's/.*bytes=([0-9]+).*/\\1/' /tmp/cir-poco-before.txt) && BA=$(sed -E 's/.*bytes=([0-9]+).*/\\1/' /tmp/cir-poco-after.txt) && echo \"COST poco index secs before=$SB after=$SA; store bytes before=$BB after=$BA; ratio=$(python3 -c \"print(round($BA/max($BB,1),3))\")\" | tee /tmp/cir-cost.txt && test \"$SA\" -le 300",
        "cwd": ".",
        "environment": "the two poco indexes' recorded wall times and store sizes",
        "inputs": "/tmp/cir-poco-before.txt and /tmp/cir-poco-after.txt"
      },
      "expected": "exit 0: the candidate's poco index completes within the 300 s smoke window; wall time and store size before/after and their ratio are REPORTED (a cost the human reads, never predicted). Retention behaviour itself remains as before (its gate is CIR-C07) — no-behavior-change for RG-REQ-011-L03."
    },
    {
      "checkId": "CIR-C15",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-CIR-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "inspection",
        "subject": "isolation of every live proof and the cleanup of every throwaway root and the git worktree",
        "criterion": "every rmap invocation in CIR-C08..C14 ran under an RMAP_STATE_ROOT/RMAP_SOCKET_PATH inside /private/tmp/CPP-INCLUDE-ROOTS-1-* with RMAP_TRANSPORT=stdio and RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the retained root was only ever COPIED (CIR-C08), never served; the operator's real registry '$HOME/Library/Application Support/repo-graph/registry.json' has the SAME sha256 before the first proof and after the last (shasum -a 256, both values quoted in build-progress.md); at hand-off `ls -d /private/tmp/CPP-INCLUDE-ROOTS-1-*` lists nothing, `git worktree list` shows no /private/tmp/CPP-INCLUDE-ROOTS-1-before entry, and no rmapd from /private/tmp/CPP-INCLUDE-ROOTS-1-before or rust/target survives (pgrep -fl rmapd)",
        "inputs": "build-progress.md; the two registry digests; the final ls/worktree/pgrep outputs"
      },
      "expected": "the inspection finds every proof isolated, the real registry digest identical before/after, and no throwaway root, worktree or daemon left behind (no-behavior-change for RG-REQ-011-L06; P-CIR-04)."
    },
    {
      "checkId": "CIR-C16",
      "obligationIds": [
        "RG-REQ-006-L03",
        "RG-REQ-001-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "git diff --check && (cd rust && cargo fmt --check -p repo-graph-indexer -p repo-graph-classification -p repo-graph-storage -p repo-graph-repo-index && cargo clippy -p repo-graph-indexer -p repo-graph-classification -p repo-graph-storage -p repo-graph-repo-index --tests -- -D warnings 2>&1 | tail -n1) && diff <(git status --short -- rust | sort) <(printf ' M rust/crates/classification/src/types.rs\\n M rust/crates/indexer/src/include_resolver.rs\\n M rust/crates/indexer/src/orchestrator.rs\\n M rust/crates/storage/src/trust_impl.rs\\n?? rust/crates/repo-index/tests/cpp_include_roots.rs\\n')",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "git status/diff of the candidate; rustfmt and clippy over the four touched crates"
      },
      "expected": "exit 0: the diff is whitespace-clean, rustfmt-clean and clippy-clean with -D warnings, and the working tree under rust/ holds exactly the five candidate paths (four modified, one new) — nothing else moved."
    }
  ]
}
-->

# CPP-INCLUDE-ROOTS-1 — `#include` resolves through every `include/` and `inc/` directory the repository has

Status: ALLOCATED (2026-09-18; specified 2026-09-12) · Track: audit round six, Q3 (HIGH; never worked — deferred by the C include-resolution milestone v1.1); ratified order position 2 of the remaining queue (human 2026-09-14). CODE slice: the include resolver, its one construction site, the shared unresolved-import category set, one renamed storage regression test, one new source→store test. Increment 2 — the unique multi-segment path-suffix fallback — is RG-REQ-006-L11 (RATIFIED 2026-09-14, its own slice), not in this packet. Maturity: MATURE surfaces (`modules list`, `cycles`, `trust`). Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13). Every literal in the checks above was read from the product on 2026-09-18 (a fresh isolated poco index with the current binary; the retained v0.18.0 poco store; the current test sources) — not from memory.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-006-L03 (C/C++ includes resolve through per-module include roots derived from the indexed file list; ambiguity stays unresolved and counted; adding roots never flips an existing resolution), RG-REQ-001-L02 (structural IMPORTS edges are emitted where evidence supports them — RC-10 is one of the four open items under this L).

**Changes (pre-authorised, REPORTED never predicted):** RG-REQ-002-L06 — the `modules list` headline "(N imports unresolved)" and the zero-state denominator now COUNT `imports_ambiguous_match` (the shared category set gains the fourth import category; §2.5); RG-REQ-009-L04 — trust's Import-graph axis figures on poco move as unresolved includes fall (captured before/after in CIR-C11 with attribution).

**Preserves:** RG-REQ-006-L01/L02/L04 (the Rust, Java and TS/Python resolution stages — CIR-C03, CIR-C13), RG-REQ-005-L01/L02/L07/L09 (call binding: `resolver.rs` is not touched — CIR-C03), RG-REQ-004-L02 (module edges derive only from resolved cross-module file→file imports — CIR-C06), RG-REQ-004-L07 (cycles walk/render — CIR-C07, CIR-C12), RG-REQ-004-L09 (zero-state wording and denominators — CIR-C06, CIR-C11), RG-REQ-011-L03 (bounded retention — CIR-C07, cost reported in CIR-C14), RG-REQ-011-L06 (isolation — CIR-C15), RG-REQ-002-L01 (no rendered edge without a stored, evidence-backed import — CIR-C05, CIR-C10, CIR-C11).

**Preservation obligations (P-CIR-01..04, each bound to a check):** P-CIR-01 — non-C/C++ ecosystems are byte-stable (codegraph in CIR-C13); P-CIR-02 — C++ call binding is untouched (`resolver.rs` byte-identical; `receiver_binding_` family green — CIR-C03, CIR-C13); P-CIR-03 — the zero-state rule survives unchanged (`list_render_empty_edges_is_zero_state` — CIR-C06); P-CIR-04 — every proof isolated, the operator's registry untouched (CIR-C15).

## 1. Problem (ROOT-CAUSED — RC-10, docs/audits/2026-09-08-root-causes-v0.18.0.md)

`rust/crates/indexer/src/include_resolver.rs::IncludeResolver::resolve` (:99–180) tries, in order: same-directory (quoted includes only, authoritative), each `--include-root` (`configured_roots`, empty by default), then three REPO-ROOT-ANCHORED literals `include`, `inc`, `src/include` (`conventional_roots`, :62/:87 and the construction site `orchestrator.rs:868–871`). poco's headers live at `Foundation/include/Poco/*.h` — the repository has 35 directories named `include` (`find -type d -name include -o -name inc`, 2026-09-18), none reachable from the root — so `Net/src/AbstractHTTPRequestHandler.cpp:20`'s `#include "Poco/Exception.h"` (RC-10 named HTTPClientSession.cpp, which includes `Poco/Net/*` and other Foundation headers but not this one — OC-2) is unresolved (`imports_file_not_found`) and no module edge can be derived. Observed on the retained v0.18.0 store and on a fresh index today: 1,702 resolved IMPORTS, 13,703 unresolved (all `imports_file_not_found`, 0 ambiguous), `modules list` renders "No cross-module dependencies detected." with the 13,703 denominator — a faithful render of a never-resolved include graph. Never worked: the literal list is from the resolver's introducing commit 55f1ac6 (2026-04-23); `docs/milestones/c-include-resolution-v1.1.md:290–302` deferred build-system include detection. gstreamer is a SECOND cause (meson subprojects; headers never under `include/`) — L11's slice.

## 2. Contract

### 2.1 The fix, at the cause

1. **Derived roots.** `build_include_resolution_map(file_paths, …)` — which already receives the full indexed file list — derives the conventional roots from it: for every indexed path, every proper directory prefix whose LAST segment is exactly `include` or `inc` is a candidate root (`Foundation/include`, `inc`, `src/include`, `a/include/b/inc` all qualify; the three former literals are the special case "at the root"). A pure function `derive_include_roots(&[String]) -> Vec<String>` (sorted, deduplicated — a `BTreeSet` drained to a `Vec`) so the candidate order is deterministic for any file order (RG-REQ-001). `IncludeResolverConfig.conventional_roots: Vec<&'static str>` becomes `derived_roots: Vec<String>`; `orchestrator.rs` passes only `configured_roots` and the literal list disappears (a list that the rule subsumes would be a dead name — a defect, not a convenience). Cost: |roots| × |includes| hash lookups (poco: 35 × ~15k) — negligible.
2. **Resolution order unchanged.** Same-directory hit (quoted only) returns immediately → configured roots → derived roots, with configured and derived hits POOLED exactly as configured and conventional are pooled today; ≥ 2 distinct hits → `Ambiguous` (unresolved, counted as `imports_ambiguous_match`); 1 → `Resolved`; 0 → `Unresolved` (falls through to the v1.0 stages as today). Angle-bracket includes keep skipping same-directory and resolve through configured + derived roots (`angle_bracket_resolves_via_conventional_root` stays green with a derived root).
3. **No guessing is introduced.** A single-segment specifier must match a file exactly under a root (`no_suffix_guessing`); no sibling-directory search (`no_sibling_directory_magic`); no suffix fallback (L11). Root names are matched case-sensitively in lower case (`Include/`, `INC/` derive nothing — a stated, counted conservatism, not a guess).
4. **Never flip.** Adding roots can only turn Unresolved into Resolved or Ambiguous: a same-directory hit still returns first, and a header that resolved through the root-level literal still resolves through the same (now derived) root. CIR-C10 proves it as a SET inclusion of resolved source→target pairs, not as a count.

### 2.2 Evidence taxonomy — the derived-root rule's input shapes (one row = one bound test; the CALL-BINDING-RECEIVER-1 lesson)

| Input shape (indexed files; the include) | Outcome | Bound test (CIR-C01/C02/C05) |
|---|---|---|
| `Foundation/include/Poco/Exception.h`; `Net/src/a.cpp` includes `"Poco/Exception.h"` | Resolved to the header | `derived_root_resolves_header_under_nested_include_dir`; end-to-end `indexed_cpp_include_resolves_through_nested_include_root` |
| `lib/inc/api.h`; `src/main.c` includes `"api.h"` | Resolved | `derived_root_resolves_header_under_nested_inc_dir` |
| the same header under `Foundation/include/` AND `Util/include/` | Ambiguous, `imports_ambiguous_match`, no edge | `derived_root_ambiguous_when_two_derived_roots_hold_the_header`; end-to-end `indexed_cpp_include_ambiguous_across_two_include_roots_is_counted_not_bound` |
| header under a configured root AND under a derived root | Ambiguous (pooled, as today for configured + conventional) | `derived_root_pooled_with_configured_root_is_ambiguous`; the renamed `configured_and_conventional_both_matching_is_ambiguous` |
| header in the includer's own directory AND under a derived root | Resolved to the same-directory file (authoritative) | `derived_root_same_directory_hit_still_wins`; `same_directory_wins_over_include_root` |
| a FILE named `include` or `inc` (e.g. `docs/include`) | derives no root | `derived_root_ignores_a_file_named_include` |
| `Include/` or `INC/` directories | derive no root (conservative) | `derived_root_is_case_sensitive` |
| `<Poco/Exception.h>` (angle brackets) from `Net/src/a.cpp` | Resolved via the derived root; same-directory skipped | `derived_root_angle_bracket_resolves`; `angle_bracket_skips_same_dir` |
| the same file set in two different orders | identical derived roots, identical candidate order | `derived_roots_are_deterministic_regardless_of_file_order` |
| root-level `include/`, `inc/`, `src/include/` (the former literals) | still Resolved | `include_root_resolves`, `inc_root_resolves`, `src_include_root_resolves` (unchanged assertions) |
| repo whose ONLY include directory is the root-level `include/` (leveldb: `include/leveldb/*.h`) — the former literal, now derived | byte-identical store (same root, same resolutions) | CIR-C13 |
| repo with no `include`/`inc` directory at all (nginx, codegraph) | byte-identical store (no derived root) | CIR-C13 |

### 2.3 Where the rule lives

`derive_include_roots` and its unit tests in `include_resolver.rs`; the pre-existing tests keep their names and assertions and obtain their resolver through the derived path (`IncludeResolver::new(IncludeResolverConfig { configured_roots, derived_roots: derive_include_roots(&files) })` or an equivalent constructor in the same file). Abstraction one-liner: what — one pure function over the file list; users — `build_include_resolution_map` and the unit tests; axis — none new (the literal list was the previous, weaker instance of the same rule); rejected alternative — keeping the literal list beside derivation (dead, misleading name).

### 2.4 One rename (decision D-TME-TEST-NAME-1's rule)

`configured_root_wins_over_conventional` asserts `Ambiguous` with two candidates — its name contradicts its body (a name that does not match what it proves is a defect). It is renamed `configured_and_conventional_both_matching_is_ambiguous`, body unchanged; CIR-C02 binds the new identity and requires the old one absent.

A second misnomer sits beside the category set this slice changes: `is_imports_category_only_matches_imports_file_not_found` (`classification/src/types.rs`) asserts that file-not-found is accepted and that non-import categories are rejected — it never asserted an 'only', and the predicate accepts four import categories. It is EXPANDED to assert all four import categories (`ImportsFileNotFound`, `ImportsAmbiguousMatch`, `ImportsWildcard`, `ImportsAmbiguousSuffix`) and renamed `is_imports_category_accepts_all_four_import_categories_and_rejects_others`; CIR-C06 binds the new identity and requires the old one absent.

### 2.5 Ambiguity is COUNTED where the user reads the count

`MODULES_LIST_UNRESOLVED_IMPORT_CATEGORIES` (`classification/src/types.rs`) deliberately excluded `imports_ambiguous_match` "for byte-stability" when that category was empty on every corpus (only root literals existed, so no overlaps). RG-REQ-006-L03 says ambiguity "stays unresolved and counted"; with derived roots poco gains hundreds of ambiguous includes, and a headline that omitted them would tell the user "N unresolved" while N + ambiguous are unresolved. The set gains the fourth import category (it becomes exactly `is_imports_category`); the storage regression test that pinned the exclusion is renamed `modules_list_unresolved_import_count_includes_all_import_categories` and asserts inclusion. Corpora with no overlap (every non-C/C++ repo; leveldb; nginx) render byte-identically because their ambiguous-match count is 0 (CIR-C13). CIR-C11 binds the coherence: `modules list --json` `unresolved_import_count` == the store's count over the four import categories, and the human "(N imports unresolved)" clause carries the same N.

### 2.6 Outward proof (what a user of the product gains)

poco `modules list` stops saying "No cross-module dependencies detected." and lists real pairs led by `Net → Foundation (… file-level imports)`; `cycles` renders poco's real SCCs from resolved edges; the unresolved denominator falls to the true remainder (system headers such as `<vector>`, plus counted ambiguities). RC-10 measured on the retained copy ~9,893 newly resolved / 368 ambiguous / 3,442 system-header residual and ~50 cross-module pairs — those are the audit's expectations, NOT this packet's oracles: every magnitude is reported from the fresh indexes (report, never predict); the bound assertions are the invariants (§2.1.4 never-flip, include-site conservation, the witness include (`Net/src/AbstractHTTPRequestHandler.cpp:20`) resolved, the Net → Foundation edge present, count coherence).

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-006-L03 existing behaviour | same-dir precedence; pooled ambiguity; no sibling/suffix guessing; root-level literals | CIR-C02 (19 named pre-existing tests + the renamed one) |
| RG-REQ-006-L01/L02/L04, RG-REQ-005-L01/L02/L07/L09 (P-CIR-02) | other resolver stages; call binding | CIR-C03 (`resolver.rs` byte-identical; families green); CIR-C13 |
| RG-REQ-004-L02, RG-REQ-004-L09 (P-CIR-03) | edge derivation; zero-state wording | CIR-C06 |
| RG-REQ-004-L07 | cycles walk/render | CIR-C07; CIR-C12 (reported) |
| RG-REQ-011-L03 | retention bounded | CIR-C07 (benchmark gate); CIR-C14 (cost reported; STOP > 300 s) |
| RG-REQ-011-L06 (P-CIR-04) | isolation | CIR-C15 |
| Byte-stability corpora (P-CIR-01) | leveldb (root-level `include/` only — retained), nginx and codegraph (no include/inc directory) | CIR-C13 (OPERATOR-RUN after acceptance: vcmi, swupdate, sqlite, duckdb, OpenXcom, gstreamer counts) |

**Preservation obligations (machine-bound ids):**

| Id | Obligation | Proof |
|---|---|---|
| P-CIR-01 | Non-C/C++ ecosystems are byte-stable: codegraph's (TypeScript+Rust) fresh index is byte-identical before/after (edges by type/resolution, unresolved by category, nodes, is_test) — the include resolver's derived roots only add candidates for include specifiers and no non-C/C++ corpus in the proof set has an `include`/`inc` directory | CIR-C13 |
| P-CIR-02 | C++ call binding is untouched: `rust/crates/indexer/src/resolver.rs` is byte-identical to the base revision and the `receiver_binding_` family stays green; leveldb's fresh index (its only include directory is the root-level `include/`, already a root before) is byte-identical before/after | CIR-C03; CIR-C13 |
| P-CIR-03 | The module-edge zero-state rule survives unchanged: `list_render_empty_edges_is_zero_state` and `list_render_absent_edges_field_is_unavailable_with_reason` stay green; the before-binary's poco render still shows the zero-state sentence | CIR-C06; CIR-C11 |
| P-CIR-04 | Every live proof is isolated under /private/tmp/CPP-INCLUDE-ROOTS-1-*, the retained root is only copied, the operator's registry digest is identical before/after, and no root, worktree or daemon is left behind | CIR-C15 |

## 4. Stop conditions

Frozen: the resolution order, `--include-root`, the call resolver (`resolver.rs` untouched), the schema, the wire, exit codes. No CMake/meson parsing (strictly larger and recovers less — RC-10). No suffix fallback (L11's slice). If poco's candidate index exceeds the 300 s smoke window (CIR-C10/C14) → STOP and report (a cost decision for the human). STANDING HONESTY RULES (no `unwrap_or(0)`/`.ok()` swallowing a failure into a count). Unmet DoD → STOP. Do NOT commit. Nothing outside the five candidate paths.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

1. Failing tests FIRST: the nine `derived_root*` unit tests (§2.2), the rename (§2.4), the storage rename (§2.5), the two end-to-end tests (CIR-C05) — then `derive_include_roots`, the config change, the orchestrator line, the category set.
2. Chunked per-crate gates in the order CIR-C01 → C02 → C03 → C04 → C05 → C06 → C07 — NEVER `cargo test --workspace` (the operator's suite runs after acceptance). Every check that tees to a `/tmp/cir-*.txt` capture is executed exactly as allocated in THIS cycle so every capture postdates the sources it proves.
3. Live proofs in the order CIR-C08 → C09 → C10 → C11 → C12 → C13 → C14; build `-p rmapd` with `-p repo-graph-rgr` in BOTH trees and put each tree's target/release first in PATH so `rmap` spawns ITS OWN rmapd over stdio. Run every check to completion in the foreground (a long index is what the 120-minute budget is for).
4. CIR-C15 cleanup, CIR-C16 hygiene; hand off the evidence object.

## 6. Definition of done

All sixteen checks pass; §2.1.4 never-flip and include-site conservation hold on poco; the count coherence of §2.5 holds; leveldb/nginx/codegraph byte-stable; cost reported; nothing outside the five paths.

## 7. Follow-ups (not this slice)

CPP-INCLUDE-ROOTS-2 = RG-REQ-006-L11 (unique multi-segment suffix fallback; gstreamer/vcmi/duckdb). A residual to measure after this slice: includes resolvable only through a build-system-declared directory that is not named `include`/`inc` (meson `gst-libs`, CMake `target_include_directories`) — L11 recovers most; the rest is a stated limitation, not a guess.

CORPUS PATHS: poco, leveldb, nginx, codegraph at /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/<name>.

## 8. Oracle corrections (ledger: docs/assurance/CPP-INCLUDE-ROOTS-1/oracle-corrections.md)

- OC-1 (2026-09-18; CIR-C09/CIR-C10; carried as INPUT-2): the conservation oracle counted every IMPORTS edge, including the persisted MODULE→MODULE directory edges that grow as includes resolve (poco 15 → 492); corrected to FILE→FILE edges, module edges reported beside them; the greedy `sed` extractions anchored (`RB` at `^BEFORE resolved=`; `TB` at `^BEFORE .* total=([0-9]+) module_edges=` — the first OC-1 text anchored only `RB`, found by the PREP-3 review F-CIR-001 and amended 2026-09-18 in INPUT-2 before its approval). Found by the builder at cycle 1 (D-CIR-C10-ORACLE).
- OC-2 (2026-09-18; CIR-C10 and §1/§2.6; carried as INPUT-2): the witness include named `Net/src/HTTPClientSession.cpp` (from RC-10), which does not include `Poco/Exception.h` in the indexed revision; corrected to `Net/src/AbstractHTTPRequestHandler.cpp:20`, verified on the checkout.
