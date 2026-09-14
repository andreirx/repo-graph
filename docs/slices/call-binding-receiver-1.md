<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "CALL-BINDING-RECEIVER-1",
  "baselinePath": "docs/requirements/baselines/CALL-BINDING-RECEIVER-1-INPUT-3.json",
  "parentRequirementIds": [
    "RG-REQ-001",
    "RG-REQ-002",
    "RG-REQ-004",
    "RG-REQ-005",
    "RG-REQ-006",
    "RG-REQ-007",
    "RG-REQ-009",
    "RG-REQ-011",
    "RG-REQ-012",
    "RG-REQ-013"
  ],
  "implements": [
    "RG-REQ-005-L01",
    "RG-REQ-005-L02",
    "RG-REQ-002-L01",
    "RG-REQ-009-L03",
    "RG-REQ-001-L03"
  ],
  "preserves": [
    "RG-REQ-001-L02",
    "RG-REQ-001-L04",
    "RG-REQ-001-L05",
    "RG-REQ-005-L05",
    "RG-REQ-005-L06",
    "RG-REQ-005-L07",
    "RG-REQ-005-L09",
    "RG-REQ-006-L01",
    "RG-REQ-006-L02",
    "RG-REQ-006-L04",
    "RG-REQ-007-L03",
    "RG-REQ-012-L02",
    "RG-REQ-004-L07",
    "RG-REQ-011-L06",
    "RG-REQ-013-L07"
  ],
  "preservationObligationIds": [
    "P-CBR-01",
    "P-CBR-02",
    "P-CBR-03",
    "P-CBR-04"
  ],
  "changes": [
    "RG-REQ-009-L04",
    "RG-REQ-005-L08"
  ],
  "acceptanceBoundary": "The callers/callees/explain/trust/cycles outputs of the freshly built rmap on fresh isolated indexes of leveldb (both the recorded base revision's binary and the candidate), the byte-stability of fresh isolated indexes of nginx (C) and codegraph (TypeScript+Rust), the retained v0.18.0 leveldb store copy for the before-fingerprint, plus cargo test -p repo-graph-indexer (lib, parity, call_binding_receiver) -p repo-graph-cpp-extractor -p repo-graph-c-extractor -p repo-graph-classification -p repo-graph-agent -p repo-graph-storage -p repo-graph-algorithms -p repo-graph-rgr -p repo-graph-daemon-runtime as named per check.",
  "candidatePaths": [
    "rust/crates/cpp-extractor/src/extractor.rs",
    "rust/crates/indexer/src/resolver.rs",
    "rust/crates/indexer/tests/call_binding_receiver.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/CALL-BINDING-RECEIVER-1/verification.json",
    "docs/assurance/CALL-BINDING-RECEIVER-1/implementation-review.json"
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
      "checkId": "CBR-C01",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-005-L02",
        "RG-REQ-002-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib receiver_binding_ 2>&1 | grep -E 'test result: ok\\. 5 passed; 0 failed'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the five write-first resolver tests named in section 2.5: receiver_binding_indirect_receiver_binds_to_its_declared_type, receiver_binding_explicit_this_binds_to_enclosing_class, receiver_binding_unknown_receiver_type_stays_unresolved_and_counted, receiver_binding_receiverless_ambiguous_call_outside_class_stays_unresolved, receiver_binding_indirect_receiver_never_binds_to_caller_itself"
      },
      "expected": "exit 0: exactly 5 tests with the receiver_binding_ prefix run and pass (grep matches the 'test result' line for that filter). Fewer than 5 means a section-2.5 test is missing; any failure fails the check."
    },
    {
      "checkId": "CBR-C02",
      "obligationIds": [
        "RG-REQ-005-L02",
        "RG-REQ-005-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib enclosing_class_preference_ 2>&1 | grep -E 'test result: ok\\. 1 passed; 0 failed'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the enclosing_class_preference_ test family in rust/crates/indexer/src/resolver.rs"
      },
      "expected": "exit 0: exactly ONE enclosing_class_preference_ test remains and passes — enclosing_class_preference_gated_off_for_non_cpp (preserved). The two defect-pinning tests enclosing_class_preference_resolves_ambiguous_method_by_caller_container and enclosing_class_preference_leaves_outside_caller_unresolved are REMOVED (replaced by the section-2.5 tests). 2 or 3 passed means a defect-pinning test survived."
    },
    {
      "checkId": "CBR-C03",
      "obligationIds": [
        "RG-REQ-005-L02",
        "RG-REQ-005-L07",
        "RG-REQ-006-L01",
        "RG-REQ-006-L02",
        "RG-REQ-006-L04",
        "RG-REQ-001-L03",
        "RG-REQ-007-L03",
        "P-CBR-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --lib 2>&1 | tee /tmp/cbr-indexer-lib.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/cbr-indexer-lib.txt && grep -cE '^test .*(ambiguous_name_stays_unresolved|two_definitions_stay_ambiguous|one_definition_plus_n_forward_decls_resolves_to_the_definition|method_call_prototype_plus_definition_resolves_to_definition|forward_decl_classification_reads_the_stamped_key|forward_decl_malformed_metadata_is_unreadable_not_a_definition|rust_crate_import_|java_suffix_|aliased_named_import_uses_imported_name_for_lookup|namespace_import_member_resolves_to_target_module|file_resolution_|categorize_).* ok$' /tmp/cbr-indexer-lib.txt | awk '{exit !($1>=12)}'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the indexer lib test suite; the preserved resolver tests named in section 3"
      },
      "expected": "exit 0: the whole indexer lib suite is green AND at least 12 of the named preserved tests (the unique-name, decl/def, forward-decl, Rust/Java/TS import stages and categorize_* families) are listed as ok — proving the shared resolver's other stages are untouched."
    },
    {
      "checkId": "CBR-C04",
      "obligationIds": [
        "RG-REQ-005-L02",
        "RG-REQ-001-L03",
        "P-CBR-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --test parity 2>&1 | tee /tmp/cbr-parity.txt | grep -E 'test result: ok\\.' && grep -E '^test parity_against_shared_indexer_fixture_corpus .* ok$' /tmp/cbr-parity.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "indexer-parity-fixtures/ at the repo root; rust/crates/indexer/tests/parity.rs"
      },
      "expected": "exit 0: the shared indexer fixture corpus parity test passes unchanged (resolution categorisation scenarios incl. resolution__categorize__obj-method and __this-method)."
    },
    {
      "checkId": "CBR-C05",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-001-L02",
        "P-CBR-01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-cpp-extractor --lib receiver_type_ 2>&1 | grep -E 'test result: ok\\. 3 passed; 0 failed' && cargo test -p repo-graph-cpp-extractor --lib 2>&1 | grep -E '^test result: ok\\.' && cargo test -p repo-graph-c-extractor --lib 2>&1 | grep -E '^test result: ok\\.'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the three write-first extractor tests of section 2.1: receiver_type_field_declaration_records_declared_type, receiver_type_local_declaration_records_declared_type, receiver_type_this_call_carries_this_receiver; the whole cpp-extractor and c-extractor lib suites"
      },
      "expected": "exit 0: exactly 3 receiver_type_ tests pass; the cpp-extractor and c-extractor lib suites stay green (the C extractor is untouched — its suite is the byte-level guard)."
    },
    {
      "checkId": "CBR-C06",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-002-L01",
        "RG-REQ-001-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-indexer --test call_binding_receiver 2>&1 | tee /tmp/cbr-fixture.txt | grep -E 'test result: ok\\.' && ! grep -E ' 0 passed' /tmp/cbr-fixture.txt",
        "cwd": "rust",
        "environment": "candidate tree; the test writes its own C++ fixture (class A { void run(); }; class B { A* a_; X* x_; void run(); } with a_->run(), this->run(), x_->run() where X is not indexed) into a temp directory and indexes it through the same path rust/crates/indexer/tests/parity.rs uses",
        "inputs": "rust/crates/indexer/tests/call_binding_receiver.rs (new)"
      },
      "expected": "exit 0 with at least one test passed: on the indexed fixture, callers A::run == [B::run]; callees B::run contains A::run and B::run (the this->run() self-call is the ONE legitimate self-loop) and nothing else; the x_->run() call is an unresolved_edges row with a calls_* category (counted); the corpus assertion — CALLS self-loops whose metadata carries a receiver other than \"this\" — returns 0."
    },
    {
      "checkId": "CBR-C07",
      "obligationIds": [
        "RG-REQ-001-L03",
        "RG-REQ-005-L05",
        "RG-REQ-005-L06",
        "RG-REQ-005-L09",
        "RG-REQ-004-L07",
        "RG-REQ-012-L02",
        "RG-REQ-013-L07",
        "P-CBR-03",
        "P-CBR-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-classification 2>&1 | grep -E '^test result: ok\\.' | head -n1 && cargo test -p repo-graph-agent --lib attribution 2>&1 | grep -E '^test result: ok\\.' && cargo test -p repo-graph-agent --test explain_symbol 2>&1 | grep -E '^test result: ok\\.' && cargo test -p repo-graph-storage --lib resolve_symbol 2>&1 | grep -E '^test result: ok\\.' && cargo test -p repo-graph-algorithms --lib scc 2>&1 | grep -E '^test result: ok\\.' && cargo test -p repo-graph-rgr --test dead_command --test exit_code_contract 2>&1 | grep -cE '^test result: ok\\.' | awk '{exit !($1>=2)}' && cargo test -p repo-graph-daemon-runtime --lib callgraph_cert 2>&1 | grep -E '^test result: ok\\.'",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the crates whose behaviour section 3 preserves: classification (categories, parity), agent (attribution mapping every_basis_code_maps_to_its_expected_reader_class; explain_symbol), storage (resolve_symbol), graph-algorithms (self_loop_not_counted_as_cycle_by_size), rgr (dead_command_is_disabled; exit_code_contract), daemon-runtime (callgraph_cert)"
      },
      "expected": "exit 0: every named suite green, which is the no-behavior-change oracle for the obligations this check alone carries — RG-REQ-004-L07: cycle semantics unchanged, a size-1 self-loop is never a cycle (scc.rs self_loop_not_counted_as_cycle_by_size); RG-REQ-005-L06: a miss never renders Confidence: high (explain_symbol tests unchanged); RG-REQ-012-L02 and RG-REQ-013-L07 (P-CBR-03): the exit-code contract and `dead`'s policy refusal (verdict on stdout, exit 4) are unchanged (exit_code_contract, dead_command_is_disabled); RG-REQ-005-L05: symbol lookup by key/qualified name/suffix unchanged (storage resolve_symbol); RG-REQ-001-L03 / RG-REQ-005-L09: classification categories and the reader mapping unchanged — this slice adds NO UnresolvedEdgeCategory variant (it reuses calls_obj_method_needs_type_info for the unresolved rows), so every_basis_code_maps_to_its_expected_reader_class proves the mapping untouched; P-CBR-04: the LiveGraph callgraph certificate's meaning is preserved (callgraph_cert tests green; the builder states in build-progress.md whether the cert compares edge SETS across engines). A red anywhere is a STOP, not a fix-forward."
    },
    {
      "checkId": "CBR-C08",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-009-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-ret && mkdir -p /private/tmp/CALL-BINDING-RECEIVER-1-ret/databases && cp /Users/apple/repo-graph-retained/audit-v0.18.0/databases/1196d1380537d43e.db* /private/tmp/CALL-BINDING-RECEIVER-1-ret/databases/ && D=/private/tmp/CALL-BINDING-RECEIVER-1-ret/databases/1196d1380537d43e.db && test \"$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%\\\"receiver\\\"%'\")\" = 155 && test \"$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS'\")\" = 3261 && test \"$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%'\")\" = 6152 && test \"$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges u JOIN nodes n ON n.node_uid=u.source_node_uid WHERE n.qualified_name='leveldb::DB::Open' AND u.target_key='Recover' AND u.line_start=1511\")\" = 1 && sqlite3 $D \"SELECT n.qualified_name, e.line_start, e.metadata_json FROM edges e JOIN nodes n ON n.node_uid=e.source_node_uid WHERE e.type='CALLS' AND e.source_node_uid=e.target_node_uid AND n.qualified_name='leveldb::DBImpl::Recover'\" | grep -F 'leveldb::DBImpl::Recover|324|{\"calleeName\":\"Recover\",\"receiver\":\"versions_\"}' && rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-ret",
        "cwd": ".",
        "environment": "a throwaway COPY of the retained v0.18.0 leveldb store file (never the original); read with sqlite3 only, no daemon",
        "inputs": "~/repo-graph-retained/audit-v0.18.0/databases/1196d1380537d43e.db (leveldb, one snapshot)"
      },
      "expected": "exit 0: the BEFORE facts are exactly as root-caused on the retained store — 155 receiver-bearing CALLS self-loops, 3261 resolved CALLS, 6152 unresolved calls_* rows, the unresolved DB::Open→Recover row at line 1511, and the self-loop row DBImpl::Recover line 324 with receiver versions_. This is the defect's fingerprint; the check pins the baseline the movement is measured from."
    },
    {
      "checkId": "CBR-C09",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-001-L03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "B=$(python3 -c \"import json;print(json.load(open('.agent-manager/slices/CALL-BINDING-RECEIVER-1/status.json'))['candidateTracking']['baseRevision'])\") && rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-before && git worktree add --detach /private/tmp/CALL-BINDING-RECEIVER-1-before $B && (cd /private/tmp/CALL-BINDING-RECEIVER-1-before/rust && cargo build --release -p repo-graph-rgr -p rmapd 2>&1 | tail -n1) && rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-ldb-before && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release:$PATH /private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release/rmap index ) >/tmp/cbr-index-before.txt 2>&1 && D=$(ls /private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/databases/*.db | head -n1) && SB=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%\\\"receiver\\\"%'\") && CB=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS'\") && UB=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%'\") && echo \"BEFORE self=$SB calls=$CB unresolved=$UB total=$((CB+UB))\" | tee /tmp/cbr-before.txt && test \"$SB\" -ge 1",
        "cwd": ".",
        "environment": "a git worktree of the recorded base revision built once (before-binary and its rmapd); a fresh isolated index of leveldb in a throwaway root served over stdio with auto passes off",
        "inputs": "the leveldb checkout at ../legacy-codebases/leveldb; the before-binary"
      },
      "expected": "exit 0: the defect reproduces at the base revision on a FRESH index (receiver-bearing self-loops ≥ 1); the before numbers self/calls/unresolved/total are RECORDED in /tmp/cbr-before.txt and in build-progress.md. No number other than self ≥ 1 is predicted."
    },
    {
      "checkId": "CBR-C10",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-009-L03",
        "RG-REQ-002-L01",
        "RG-REQ-001-L03",
        "RG-REQ-001-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo build --release -p repo-graph-rgr -p rmapd --manifest-path rust/Cargo.toml 2>&1 | tail -n1 && rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-ldb && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/tmp/cbr-index-after.txt 2>&1 && D=$(ls /private/tmp/CALL-BINDING-RECEIVER-1-ldb/databases/*.db | head -n1) && SA=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%\\\"receiver\\\"%'\") && SAA=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid\") && CA=$(sqlite3 $D \"SELECT count(*) FROM edges WHERE type='CALLS'\") && UA=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%'\") && echo \"AFTER receiver-self=$SA all-self=$SAA calls=$CA unresolved=$UA total=$((CA+UA))\" | tee /tmp/cbr-after.txt && test \"$SA\" = 0 && TB=$(sed -E 's/.*total=([0-9]+).*/\\1/' /tmp/cbr-before.txt) && test $((CA+UA)) = $TB",
        "cwd": ".",
        "environment": "the freshly built candidate rmap + rmapd; a fresh isolated index of leveldb in a throwaway root, stdio, auto passes off",
        "inputs": "the leveldb checkout; /tmp/cbr-before.txt from CBR-C09"
      },
      "expected": "exit 0 requires BOTH invariants: (1) receiver-bearing CALLS self-loops == 0 on the candidate's fresh leveldb index; (2) CALL-SITE CONSERVATION — resolved CALLS + unresolved calls_* rows after == the same sum before (no call site is dropped or invented: every self-loop removed became either an evidence-bound edge to a different target or a counted unresolved row, RG-REQ-001-L03). all-self (genuine this->/recursive self-calls) is REPORTED, not predicted. The direction of resolved-vs-unresolved is REPORTED, never predicted (decision D-TME-MOVEMENT-1's lesson)."
    },
    {
      "checkId": "CBR-C11",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-002-L01",
        "RG-REQ-005-L08",
        "RG-REQ-001-L04",
        "RG-REQ-001-L05",
        "RG-REQ-005-L05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" callers leveldb::DBImpl::Recover --json ) > /tmp/cbr-callers.json 2>/dev/null && python3 -c \"import json,re,sys\nd=json.load(open('/tmp/cbr-callers.json'))\nrows=d['callers']\nq=sorted(set(r['qualified_name'] for r in rows))\nassert q==['leveldb::DB::Open'], q\nr=rows[0]\nassert r['file']=='db/db_impl.cc' and r['line']==1503 and r['edge_type']=='CALLS', r\nassert re.match(r'^repo_[a-z0-9]+:db/db_impl\\\\.cc#leveldb::DB::Open:SYMBOL:METHOD$', r['stable_key']), r['stable_key']\nassert d['count']==len(rows)\nprint('callers ok', r['stable_key'])\"",
        "cwd": ".",
        "environment": "the candidate rmap on the CBR-C10 isolated leveldb index",
        "inputs": "the captured callers --json document (keys: target, callers[], count, backend_used, fallback_reason)"
      },
      "expected": "exit 0: the callers of leveldb::DBImpl::Recover are exactly {leveldb::DB::Open} (leveldb's source has ONE call site, db_impl.cc:1511 `impl->Recover(&edit, &save_manifest)`); the self-row leveldb::DBImpl::Recover is gone; the row anchors db/db_impl.cc:1503 — the caller NODE's line, which is what the renderer prints today (the call-site anchoring half of RG-REQ-005-L08 is OUT OF SCOPE here: follow-up CALLERS-ANCHOR-1); the stable_key is format v2 and repo-relative (no-behavior-change for RG-REQ-001-L04 and RG-REQ-001-L05: the identity format and the hand-back of a printed identity are unchanged)."
    },
    {
      "checkId": "CBR-C12",
      "obligationIds": [
        "RG-REQ-002-L01",
        "RG-REQ-005-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" callees leveldb::DBImpl::Recover --json ) > /tmp/cbr-callees.json 2>/dev/null && python3 -c \"import json\nd=json.load(open('/tmp/cbr-callees.json'))\nassert isinstance(d.get('callees'), list), list(d.keys())\nrows=[r['qualified_name'] for r in d['callees']]\nassert d['target']['qualified_name']=='leveldb::DBImpl::Recover', d['target']\nassert 'leveldb::DBImpl::Recover' not in rows, rows\nassert 'leveldb::VersionSet::AddLiveFiles' in rows and 'leveldb::VersionSet::MarkFileNumberUsed' in rows, rows\nprint('callees ok', len(rows))\" && D=$(ls /private/tmp/CALL-BINDING-RECEIVER-1-ldb/databases/*.db | head -n1) && test \"$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges u JOIN nodes n ON n.node_uid=u.source_node_uid WHERE n.qualified_name='leveldb::DBImpl::Recover' AND u.target_key='Recover' AND u.line_start=324 AND u.category IN ('calls_function_ambiguous_or_missing','calls_obj_method_needs_type_info')\")\" = 1",
        "cwd": ".",
        "environment": "the candidate rmap on the CBR-C10 isolated leveldb index",
        "inputs": "the captured callees --json document"
      },
      "expected": "exit 0 (the assertion reads the `callees` list only — the `target` object echoes the focus symbol and must not be scanned): callees of leveldb::DBImpl::Recover no longer include leveldb::DBImpl::Recover itself (the fabricated self-row is gone) and still include the unique-name `versions_->` members that bound before (VersionSet::AddLiveFiles, MarkFileNumberUsed — unique-name binding preserved); the ambiguous `versions_->Recover(save_manifest)` call at db_impl.cc:324 is an unresolved_edges row with a known category — COUNTED, not bound to either candidate. Corrected by D-CBR-XFILE-1: `versions_` is declared in db_impl.h and the C++ extractor is per-file, so this call cannot be typed within this slice's candidate paths; cross-file receiver typing is CALL-BINDING-RECEIVER-2. The honest remainder is the required outcome here."
    },
    {
      "checkId": "CBR-C13",
      "obligationIds": [
        "RG-REQ-001-L03",
        "RG-REQ-005-L09",
        "RG-REQ-005-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "D=$(ls /private/tmp/CALL-BINDING-RECEIVER-1-ldb/databases/*.db | head -n1) && test \"$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges u JOIN nodes n ON n.node_uid=u.source_node_uid WHERE n.qualified_name='leveldb::DB::Open' AND u.target_key='Recover'\")\" = 0 && test \"$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%' AND category NOT IN ('calls_function_ambiguous_or_missing','calls_obj_method_needs_type_info')\")\" = 0 && sqlite3 $D \"SELECT category, count(*) FROM unresolved_edges WHERE category LIKE 'calls_%' GROUP BY category\" | tee /tmp/cbr-unresolved-after.txt && RA=$(sqlite3 $D \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%' AND metadata_json LIKE '%\\\"receiver\\\"%'\") && RB=$(sqlite3 $(ls /private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/databases/*.db | head -n1) \"SELECT count(*) FROM unresolved_edges WHERE category LIKE 'calls_%' AND metadata_json LIKE '%\\\"receiver\\\"%'\") && echo \"receiver-bearing unresolved before=$RB after=$RA\" | tee -a /tmp/cbr-unresolved-after.txt",
        "cwd": ".",
        "environment": "sqlite3 on the CBR-C09 and CBR-C10 isolated store files",
        "inputs": "the two fresh leveldb stores"
      },
      "expected": "exit 0: the DB::Open→Recover row has LEFT the unresolved set because it is now bound (not dropped — CBR-C10's conservation covers that); every remaining unresolved call carries one of the two existing categories (calls_function_ambiguous_or_missing, calls_obj_method_needs_type_info) — this slice adds no category; the per-category counts and the receiver-bearing unresolved counts before/after are REPORTED (the honest remainder of section 2.4)."
    },
    {
      "checkId": "CBR-C14",
      "obligationIds": [
        "RG-REQ-009-L03",
        "RG-REQ-009-L04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust ) > /tmp/cbr-trust-after.txt 2>/dev/null && grep -E \"your code's calls [0-9]+% resolved \\([0-9]+ of [0-9]+ in-scope or unclassified\\)\" /tmp/cbr-trust-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release:$PATH /private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release/rmap trust ) > /tmp/cbr-trust-before.txt 2>/dev/null && grep -E \"your code's calls [0-9]+% resolved\" /tmp/cbr-trust-before.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust --json ) > /tmp/cbr-trustj-after.json 2>/dev/null && CA=$(sed -E 's/.*calls=([0-9]+).*/\\1/' /tmp/cbr-after.txt) && UA=$(sed -E 's/.*unresolved=([0-9]+).*/\\1/' /tmp/cbr-after.txt) && python3 -c \"import json,sys\nd=json.load(open('/tmp/cbr-trustj-after.json'))\nf={}\nst=[d]\nwhile st:\n o=st.pop()\n if isinstance(o,dict):\n  for k in ('resolved_calls','unresolved_calls','call_resolution_rate'):\n   if k in o and k not in f: f[k]=o[k]\n  st.extend(o.values())\n elif isinstance(o,list): st.extend(o)\nassert f['resolved_calls']==$CA and f['unresolved_calls']==$UA, (f,$CA,$UA)\nprint('trust json consistent with store', f)\"",
        "cwd": ".",
        "environment": "the before-binary on the CBR-C09 index and the candidate on the CBR-C10 index",
        "inputs": "the captured trust texts and the candidate's trust --json (fields resolved_calls, unresolved_calls, call_resolution_rate under value/resolution/value)"
      },
      "expected": "exit 0: the candidate's trust headline renders the real line shape and its --json resolved_calls/unresolved_calls equal the store counts of CBR-C10 (the percentage is computed from evidence-bound edges only). The before and after headline lines are REPORTED side by side in build-progress.md with the movement attributed (self-loops removed; receiver-typed bindings added); the DIRECTION and the LEVEL are not predicted — a headline that moves when fabricated edges are removed is the required outcome, whichever way the new evidence moves it."
    },
    {
      "checkId": "CBR-C15",
      "obligationIds": [
        "RG-REQ-002-L01",
        "RG-REQ-005-L08",
        "RG-REQ-005-L06",
        "RG-REQ-001-L05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl::Recover ) > /tmp/cbr-explain.txt 2>/dev/null && grep -qE '^Callers \\(1\\)$' /tmp/cbr-explain.txt && grep -qE '^  - Open \\(db\\)  db/db_impl\\.cc:1503$' /tmp/cbr-explain.txt && ! grep -qE '^  - Recover \\(db\\)  db/db_impl\\.cc:292$' /tmp/cbr-explain.txt && grep -qE '^Confidence: (low|medium|high)$' /tmp/cbr-explain.txt",
        "cwd": ".",
        "environment": "the candidate rmap on the CBR-C10 isolated leveldb index",
        "inputs": "the captured explain text (today: 'Callers (1)' followed by '  - Recover (db)  db/db_impl.cc:292')"
      },
      "expected": "exit 0: explain's Callers section lists exactly one caller, Open (db) at db/db_impl.cc:1503, and the self-row is gone; a Confidence line is present (RG-REQ-005-L06 untouched). The identity explain prints is the one callers accepted (CBR-C11) — one symbol identity."
    },
    {
      "checkId": "CBR-C16",
      "obligationIds": [
        "RG-REQ-001-L02",
        "RG-REQ-006-L04",
        "RG-REQ-007-L03",
        "RG-REQ-001-L03",
        "P-CBR-01",
        "P-CBR-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "for R in nginx codegraph; do case $R in nginx) SRC='/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/nginx';; codegraph) SRC='/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/codegraph';; esac; rm -rf /private/tmp/CALL-BINDING-RECEIVER-1-$R-before /private/tmp/CALL-BINDING-RECEIVER-1-$R; ( cd \"$SRC\" && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-$R-before RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release:$PATH /private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release/rmap index ) >/dev/null 2>&1 && ( cd \"$SRC\" && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-$R RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/dev/null 2>&1 && Q=\"SELECT type, resolution, count(*) FROM edges GROUP BY type, resolution; SELECT category, count(*) FROM unresolved_edges GROUP BY category; SELECT count(*) FROM nodes; SELECT count(*) FROM files WHERE is_test=1;\" && sqlite3 $(ls /private/tmp/CALL-BINDING-RECEIVER-1-$R-before/databases/*.db | head -n1) \"$Q\" > /tmp/cbr-$R-before.txt && sqlite3 $(ls /private/tmp/CALL-BINDING-RECEIVER-1-$R/databases/*.db | head -n1) \"$Q\" > /tmp/cbr-$R-after.txt && cmp /tmp/cbr-$R-before.txt /tmp/cbr-$R-after.txt && echo \"$R byte-stable: $(wc -l < /tmp/cbr-$R-after.txt) fact lines\"; done",
        "cwd": ".",
        "environment": "fresh isolated indexes of nginx (C) and codegraph (TypeScript + Rust; the audit's control repo) with the before-binary and the candidate, in throwaway roots, stdio, auto passes off",
        "inputs": "../legacy-codebases/nginx; ../legacy-codebases/codegraph"
      },
      "expected": "exit 0: for BOTH corpora the edge counts by type and resolution, the unresolved counts by category, the node count and the is_test partition are byte-identical before/after — C emits no receiver (the C extractor is untouched) and the receiver-type stage is gated on metadata only the C++ extractor emits, so TypeScript and Rust cannot move. A differing line is a STOP (no-behavior-change for RG-REQ-001-L02, RG-REQ-006-L04, RG-REQ-007-L03: structural edges, TS/Python resolution and the test partition are byte-identical)."
    },
    {
      "checkId": "CBR-C17",
      "obligationIds": [
        "RG-REQ-004-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release:$PATH /private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release/rmap cycles --json ) > /tmp/cbr-cyclesj-before.json 2>/dev/null && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" cycles --json ) > /tmp/cbr-cyclesj-after.json 2>/dev/null && python3 -c \"import json\nb=json.load(open('/tmp/cbr-cyclesj-before.json'))\na=json.load(open('/tmp/cbr-cyclesj-after.json'))\nsets=lambda d: sorted(sorted(n['qualified_name'] for n in c['nodes']) for c in d['cycles'])\nassert sets(b)==sets(a), (sets(b), sets(a))\nassert b['module_edge_count']==a['module_edge_count'] and b['count']==a['count'], (b['module_edge_count'], a['module_edge_count'])\nassert a['count']>=1\nprint('cycle member sets identical:', sets(a), '| module edges', a['module_edge_count'])\" && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release:$PATH /private/tmp/CALL-BINDING-RECEIVER-1-before/rust/target/release/rmap cycles ) 2>/dev/null | grep -vE '^(warning|note):|^Snapshot:| -> ' > /tmp/cbr-cycles-before.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/CALL-BINDING-RECEIVER-1-ldb RMAP_SOCKET_PATH=/private/tmp/CALL-BINDING-RECEIVER-1-ldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" cycles ) 2>/dev/null | grep -vE '^(warning|note):|^Snapshot:| -> ' > /tmp/cbr-cycles-after.txt && cmp /tmp/cbr-cycles-before.txt /tmp/cbr-cycles-after.txt",
        "cwd": ".",
        "environment": "before-binary on the CBR-C09 index; candidate on the CBR-C10 index",
        "inputs": "the two captured cycles outputs (stdio warning/note lines stripped)"
      },
      "expected": "exit 0: the cycle MEMBER SETS are identical before and after — for every cycle in `rmap cycles --json`, the sorted `nodes[].qualified_name` list is the same on the before-binary's index and the candidate's (leveldb today: one 4-module cycle {db, helpers/memenv, table, util}) — and `module_edge_count` and `count` are equal; additionally the human `rmap cycles` text is identical once the per-index `Snapshot:` line and the walk-ring line (` -> `) are stripped. Corrected by D-CBR-XFILE-1: the walk ring's entry point differs between two identical fresh indexes because the daemon iterates module-import edges without a stable order — a pre-existing determinism defect filed as CYCLES-WALK-DETERMINISM-1, not an effect of this slice (RG-REQ-004-L07 preserved: import cycles do not read CALLS; size-1 self-loops were never cycles)."
    },
    {
      "checkId": "CBR-C18",
      "obligationIds": [
        "RG-REQ-011-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "inspection",
        "subject": "isolation of every live proof and the cleanup of every throwaway root and the git worktree",
        "criterion": "every rmap invocation in CBR-C08..C17 ran under an RMAP_STATE_ROOT/RMAP_SOCKET_PATH inside /private/tmp/CALL-BINDING-RECEIVER-1-* with RMAP_TRANSPORT=stdio and RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the operator's real registry '$HOME/Library/Application Support/repo-graph/registry.json' has the SAME sha256 before the first proof and after the last (shasum -a 256, both values quoted in build-progress.md); at hand-off `ls -d /private/tmp/CALL-BINDING-RECEIVER-1-*` lists nothing, `git worktree list` shows only the main tree, and no rmapd from /private/tmp/CALL-BINDING-RECEIVER-1-before or rust/target survives (pgrep -fl rmapd)",
        "inputs": "build-progress.md; the two registry digests; the final ls/worktree/pgrep outputs"
      },
      "expected": "the inspection finds every proof isolated, the real registry digest identical before/after, and no throwaway root, worktree or daemon left behind (no-behavior-change for RG-REQ-011-L06)."
    },
    {
      "checkId": "CBR-C19",
      "obligationIds": [
        "RG-REQ-005-L01",
        "RG-REQ-002-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "git diff --check && test \"$(git status --short | grep -vE '^(\\?\\?| M) (rust/crates/cpp-extractor/src/extractor.rs|rust/crates/indexer/src/resolver.rs|rust/crates/indexer/tests/call_binding_receiver.rs)$' | wc -l | tr -d ' ')\" = 0 && git status --short",
        "cwd": ".",
        "environment": "candidate tree at hand-off",
        "inputs": "git status / git diff --check"
      },
      "expected": "exit 0: whitespace-clean and ONLY the three candidatePaths changed or added. Any other path (a fixture directory, a golden file, a docs edit) is out of scope — STOP and report rather than widen."
    }
  ]
}
-->
# CALL-BINDING-RECEIVER-1 — a C++ call binds to the receiver's type or stays unresolved; never to the caller's own class by default

Status: ALLOCATED (2026-09-14; specified 2026-09-12) · Track: audit round six, Q1 (CRITICAL; REGRESSION from CPP-DECLARATORS-1 5c3ec2d); ratified order position 1 of the remaining queue (human 2026-09-14). CODE slice: cpp-extractor + indexer resolver + one new integration test; the two defect-pinning resolver tests are replaced by five named tests. Maturity: MATURE surfaces (`callers`, `callees`, `explain`, `trust`). Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13). Every literal in the checks above was read from the product's real output on 2026-09-14 (retained v0.18.0 leveldb store; current binary) — not from memory.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-005-L01 (indirect receiver never binds to the enclosing class without receiver-type evidence), RG-REQ-005-L02 (binding only on evidence; enclosing-type preference receiverless/explicit-`this` only), RG-REQ-002-L01 (no surface asserts a relationship the store does not hold — the CALLS half), RG-REQ-009-L03 (resolution % counts only evidence-bound edges), RG-REQ-001-L03 (unresolved rows preserved — restore the rows the regression consumed).

**Changes (evidence moves, pre-authorised — REPORTED, never predicted):** RG-REQ-009-L04 — trust's Call-graph reliability line and the headline `your code's calls N% resolved (X of Y in-scope or unclassified)` MOVE on every C++ repo (leveldb today: 37%, 3261 of 8767): 155 fabricated self-loops leave the resolved set and receiver-typed bindings enter it; the direction is measured, not predicted (D-TME-MOVEMENT-1's lesson). RG-REQ-005-L08 — callers/callees rows gain the real caller (`leveldb::DB::Open`, rendered at the caller node's line db/db_impl.cc:1503) and lose the self-row; the call-site-anchoring half of L08 (1511, the edge's line) is OUT OF SCOPE — follow-up CALLERS-ANCHOR-1. NOT a change any more: `dead` — it is refused by policy (RG-REQ-013-L07, exit 4) and no surface renders its substrate, so the "more dead candidates" consequence is unobservable today and is not claimed; the observable is the self-loop count (CBR-C10).

**Preserves (the regression watch, §3):** RG-REQ-001-L02/L04/L05, RG-REQ-005-L05/L06/L07/L09, RG-REQ-006-L01/L02/L04 (the shared resolver's other stages), RG-REQ-007-L03, RG-REQ-012-L02, RG-REQ-004-L07 (cycles), RG-REQ-011-L06 (isolation), RG-REQ-013-L07 (`dead` refusal shape), and every non-C/C++ extractor byte-stable. Preservation obligations P-CBR-01..04 are declared in the §3 table.

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-08-root-causes-v0.18.0.md` RC-1, store-verified)

Outward: `callers leveldb::DBImpl::Recover` answers "itself"; the real caller `DB::Open` (db_impl.cc:1511) is missing; `callees` lists itself; `explain` shows the same. Receiver-bearing CALLS self-loops on the retained stores: 0 before → 155 leveldb / 497 OpenXcom / 836 vcmi / 1,098 poco / 2,522 duckdb / 321 gstreamer after; codegraph 0. Each loop gives its symbol fan-in ≥ 1 (shielding it from `dead`) and inflates trust's calls-resolved %.

Cause: `cpp-extractor/src/extractor.rs:1648 extract_call` emits the BARE field name as `target_key` and stores the receiver only as metadata (`{"calleeName","receiver"}`, comment :1657 "NOT consumed here"); `indexer/src/resolver.rs:951-962` (CPP-DECLARATORS-1 §2.3 amended) applies `enclosing_class_preference` (:1113) to every ambiguous C-family pool — picking the candidate in the CALLER's class without reading the receiver — so `versions_->Recover(...)` inside `DBImpl::Recover` binds to itself, and `impl->Recover(...)` inside `DB::Open` (container `leveldb::DB`, no candidate) is dropped as unresolved. The two tests that pinned the feature (`resolver.rs:1723` — a fictional `leveldb::DBImpl::Open`; `:1763` — encodes the dropped outside caller as intended) are wrong premises.

## 2. Contract

1. **Receiver types become extractor facts (C++).** In `cpp-extractor`, the `field_declaration` arm (:763) records `(enclosing class, member name) → declared type` (strip `*`, `&`, `const`, template arguments; keep the qualified name when written); the per-function local-variable map generalises `ctx.local_stream_types` (CPP-SB-1 D3, cleared at :1621) to any declared local type. A `field_expression` call carries `metadata_json.receiverType` (the resolved declared type, qualified where known) beside the existing `receiver`; `this->m()` / `(*this).m()` / an unqualified `m()` carry `receiver: "this"` or no receiver. Metadata keys are additive; no new column.
2. **A receiver-type binding stage in the shared resolver.** In `resolve_call_target`, BEFORE the bare-name singleton at :942 and gated on the edge carrying `receiverType`: look up `<receiverType>::<target_key>` in the qualified-name/stable-key index; on a unique hit bind; on a miss with IMPLEMENTS edges available, walk the receiver type's bases (BFS, first depth with exactly one hit); otherwise fall through to the existing stages.
3. **The invariant (as corrected by the adjudicator):** an INDIRECT receiver (any receiver other than `this`/explicit self) never binds to the caller's enclosing class without receiver-type evidence. `enclosing_class_preference` runs ONLY for receiverless and explicit-`this` calls. The C-family gate stays; C emits no receiver and is untouched.
4. **Honest remainder.** A receiver-bearing call whose receiver type is unknown or whose target is ambiguous stays unresolved with the EXISTING category `calls_obj_method_needs_type_info` and is COUNTED. No new category: a distinct one would touch `classification/src/types.rs` and `agent/src/attribution.rs`, outside this slice's candidate paths (reviewer finding F-CBR-002). Recovering rows that RC-1 had consumed is the point, not a loss.
5. **Tests rewritten at the cause — NEW identities, bound by CBR-C01/C02.** `resolver.rs` tests `enclosing_class_preference_resolves_ambiguous_method_by_caller_container` (:1723, a fictional caller) and `enclosing_class_preference_leaves_outside_caller_unresolved` (:1763, encodes defect (b)) are REMOVED; `enclosing_class_preference_gated_off_for_non_cpp` stays. Five tests with the `receiver_binding_` prefix replace them — (a) `receiver_binding_indirect_receiver_binds_to_its_declared_type`, (b) `receiver_binding_explicit_this_binds_to_enclosing_class`, (c) `receiver_binding_unknown_receiver_type_stays_unresolved_and_counted`, (d) `receiver_binding_receiverless_ambiguous_call_outside_class_stays_unresolved`, (e) `receiver_binding_indirect_receiver_never_binds_to_caller_itself` (the invariant guard). Three extractor tests with the `receiver_type_` prefix pin §2.1: `receiver_type_field_declaration_records_declared_type`, `receiver_type_local_declaration_records_declared_type`, `receiver_type_this_call_carries_this_receiver`. The corpus assertion lives in the new integration test `rust/crates/indexer/tests/call_binding_receiver.rs` (CBR-C06), which writes its own C++ fixture into a temp directory — no fixture files in the tree. Substance: (a) class `A { void run(); }`, class `B { A* a_; void run() { a_->run(); } }` → `callers A::run` = `B::run`, `callees B::run` = `A::run`, no self-loop; (b) `B::run() { run(); }` and `this->run()` → `B::run` (explicit self binds to the enclosing class); (c) `B { X* x_; void run() { x_->run(); } }` with `X` not indexed → unresolved, counted, not bound to `B::run`; (d) a receiverless ambiguous call from outside any class stays unresolved. A corpus assertion test over an indexed fixture: `SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%receiver%' AND metadata_json NOT LIKE '%"receiver":"this"%'` = 0.
6. **Outward proof on leveldb (fresh isolated indexes, `RMAP_TRANSPORT=stdio`; before-binary from a git worktree of the recorded base revision):** `callers leveldb::DBImpl::Recover --json` = exactly {`leveldb::DB::Open`} rendered at db/db_impl.cc:1503 (the caller node's line — today's renderer; leveldb has ONE call site, db_impl.cc:1511) (CBR-C11); `callees` excludes itself, keeps the unique-name `versions_->` members, and carries `versions_->Recover()` as a counted unresolved row — its receiver is declared in db_impl.h, out of a per-file extractor's sight (D-CBR-XFILE-1; cross-file typing = CALL-BINDING-RECEIVER-2) (CBR-C12); receiver-bearing self-loops = 0 and resolved+unresolved call sites CONSERVED before→after (CBR-C10); the DB::Open→Recover row has left the unresolved set by binding (CBR-C13); trust's headline before/after REPORTED with attribution and its --json consistent with the store (CBR-C14); `explain` Callers = Open (db) db/db_impl.cc:1503 (CBR-C15). Before-fingerprint on the retained v0.18.0 store copy: 155 self-loops / 3261 resolved / 6152 unresolved / DB::Open→Recover unresolved at 1511 / self-loop row line 324 receiver versions_ (CBR-C08).

## 3. Regression watch — what this slice must NOT change, and the test that proves it

| Preserved L | What would regress | Proof (existing test / write-first / corpus) |
|---|---|---|
| RG-REQ-005-L02, RG-REQ-001-L02 | unique-name binding and decl/def filtering for every language | `resolver.rs::ambiguous_name_stays_unresolved`, `one_definition_plus_n_forward_decls_resolves_to_the_definition`, `method_call_prototype_plus_definition_resolves_to_definition`; `indexer/tests/parity.rs::parity_against_shared_indexer_fixture_corpus` |
| RG-REQ-006-L01/L02/L04 | the Rust crate, Java suffix, TS/Python import stages of the same resolver | `rust_crate_import_*`, `java_suffix_*`, `aliased_named_import_uses_imported_name_for_lookup`, `namespace_import_member_resolves_to_target_module`, `file_resolution_*` — all green, untouched |
| RG-REQ-005-L07 | forward-decl flag read and `(decl)` ranking | `forward_decl_classification_reads_the_stamped_key`, `forward_decl_malformed_metadata_is_unreadable_not_a_definition`, `find_facts/rank.rs::definition_beats_forward_decl` |
| RG-REQ-005-L05 | symbol resolution by key/qualified name/suffix (SYMBOL-IDENTITY-1) — this slice touches call BINDING, not lookup | `storage/src/queries.rs` resolve_symbol suffix tests; `agent/tests/explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver` |
| RG-REQ-001-L03, RG-REQ-005-L09 | unresolved classification and reader mapping | `categorize_*` tests; `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` (a new category needs a mapping) |
| RG-REQ-001-L02 non-C/C++ | every other extractor byte-stable | CALLS/IMPORTS edge counts and `unresolved_edges` counts identical before/after on isolated indexes of FRAKTAG (TS) and repo-graph's `rust/crates/gate` fixture or kafka's `clients` subtree (Java) — store SQL, copies only |
| C extractor | C emits no receiver; unchanged | nginx or sqlite fixture edge counts identical; `include_resolver.rs` tests untouched |
| RG-REQ-004-L07 | cycles exclude size-1 self-loops anyway | `graph-algorithms/src/scc.rs::self_loop_not_counted_as_cycle_by_size` stays; `rmap cycles` on leveldb byte-identical |
| LiveGraph callgraph certificate / union serve | the cert admits `EdgeType::Calls`; fewer, truer edges must not turn it RED spuriously | `daemon-runtime/src/callgraph_cert` tests green; if the cert compares edge SETS across engines, both engines see the same new set (the resolver is shared) — state the observation |
| RG-REQ-012-L02, RG-REQ-013-L07 | exit codes; `dead` refusal shape unchanged (`rmap dead` is refused by policy, exit 4; nothing renders its substrate) | `exit_code_contract.rs`; `dead_command.rs::dead_command_is_disabled` (CBR-C07) |
| RG-REQ-011-L06 | isolation | every `rmap` call under `RMAP_STATE_ROOT`/`RMAP_SOCKET_PATH` in a throwaway root; registry sha256 of the real root identical before/after (cite both) (CBR-C18) |
| P-CBR-01 | The C extractor emits no receiver and is untouched: nginx's fresh index is byte-identical before/after (edges by type/resolution, unresolved by category, nodes, is_test) | CBR-C16 (nginx); CBR-C05 (c-extractor suite) |
| P-CBR-02 | The receiver-type binding stage is gated on `receiverType` metadata that only the C++ extractor emits, so TypeScript, Rust, Java and Python binding cannot move: codegraph (TS+Rust) byte-identical before/after; the resolver's other stages' tests untouched | CBR-C16 (codegraph); CBR-C03; CBR-C04 |
| P-CBR-03 | `dead` stays refused by policy with the same verdict shape and exit code 4; the exit-code contract is unchanged | CBR-C07 (dead_command, exit_code_contract) |
| P-CBR-04 | The LiveGraph callgraph certificate keeps its meaning: both engines read the one shared resolver, so fewer-and-truer CALLS edges cannot split them; the cert's unit tests stay green and the builder states whether the cert compares edge SETS across engines | CBR-C07 (callgraph_cert) |

A preserved row whose proof is missing blocks the DoD; a preserved row whose proof fails is a STOP, not a fix-forward.

## 4. Stop conditions

Frozen: wire protocol (metadata keys additive), storage schema (no new column; a new `UnresolvedEdgeCategory` variant is additive and must be mapped), exit codes, `find`'s semantics, the LiveGraph↔SQLite parity certificate, `resolve_symbol_name`/`resolve_symbol` routing (ruling B), tree-sitter grammar version, every non-C/C++ extractor. If the receiver-type stage needs IMPLEMENTS edges anchored on the CLASS node (today they are anchored on the FILE — RC-3), implement the direct `<receiverType>::<name>` lookup only and record the base-walk as dependent on EXPLAIN-TYPE-SECTIONS-1 step 2 — do NOT re-anchor IMPLEMENTS here. If the LiveGraph callgraph cert turns RED for a reason other than the intended edge change, STOP + DECISION_REQUIRED. STANDING HONESTY RULES (no `unwrap_or(0)`/`.ok()`/`unwrap_or_default` on any fallible read; a malformed `receiverType` is unreadable-with-reason, not absent). Unmet DoD → STOP + DECISION_REQUIRED. Do NOT commit.

Follow-ups filed, NOT in this slice: CALL-BINDING-RECEIVER-2 (cross-file receiver typing: persist per-class member types as extractor facts and build a resolve-time field-type index in the orchestrator/ResolverIndex, so header-declared members such as `versions_` bind — D-CBR-XFILE-1); CYCLES-WALK-DETERMINISM-1 (the cycles walk ring's entry point changes between identical indexes — order module-import edge iteration deterministically; RG-REQ-001); CALLERS-ANCHOR-1 (callers/callees rows anchor the CALL SITE — the edge's line — per RG-REQ-005-L08; today the caller node's line renders); the `dead` substrate has no surface while `dead` is refused (RG-REQ-013-L07) — nothing to file.

## 5. Validation (SYNCHRONOUS; ORDERED; `build-progress.md` written after EACH step)

1. Failing tests FIRST (§2.5 a–d + the corpus assertion on a two-class fixture); then the extractor facts; then the resolver stage; then the invariant guard.
2. Chunked gates: `cargo test -p repo-graph-cpp-extractor`, `-p repo-graph-indexer`, `-p repo-graph-classification`, `-p repo-graph-agent` (attribution), `-p repo-graph-storage --lib resolve_symbol`; `cargo fmt`/`clippy` per crate. NEVER `cargo test --workspace` (the operator's suite runs it after approval).
3. Live proof on the SMALLEST corpus, in the order CBR-C08 (retained-copy fingerprint) → CBR-C09 (before: worktree of the recorded base revision, built once, fresh isolated leveldb index) → CBR-C10..C15 (candidate: fresh isolated leveldb index) → CBR-C17 (cycles). `rmapd` is a separate crate: build `-p rmapd` with `-p repo-graph-rgr` in BOTH trees and put each tree's target/release first in PATH for its own invocations, or a stale daemon masks the fix.
4. Byte-stability proofs CBR-C16: nginx (C) and codegraph (TypeScript+Rust) — fresh isolated indexes with both binaries; counts compared with cmp, not full diffs. (Java and Python are covered by P-CBR-02's gating argument plus CBR-C03/C04; no large Java index in the builder window.)
5. Cleanup of isolated roots; `build-N.md` with EXECUTED/OBSERVED/NOT RUN labels per step.

5b. Isolation and cleanup are a check (CBR-C18): quote the real registry sha256 before the first proof and after the last; remove every /private/tmp/CALL-BINDING-RECEIVER-1-* root and the worktree.

OPERATOR-RUN after acceptance: the receiver-bearing self-loop count on OpenXcom/vcmi/poco/duckdb/gstreamer from fresh isolated indexes (too large for the builder window) and the full gate suite (`agent-manager/scripts/repo-graph-gates.sh`).

## 6. Definition of done

Every check CBR-C01..C19 passes AS WRITTEN — a check whose command cannot execute or match as written is reported `execution-failed` with the evidence attached, never `passed` on a substitute (decision D-TME-VALIDATION-STALE-1); every preserved row in §3 has its proof EXECUTED and green; the two pre-authorised movements are reported with before/after numbers and attribution; no non-C/C++ fact moved; gates green. Reviewer checks the §3 table row by row — a positive verdict without it is incomplete.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; kafka at ../legacy-codebases/kafka; nginx/sqlite at ../legacy-codebases/<name>; repo-graph is THIS repo.

## Oracle corrections (ratified procedure, docs/MANAGER.md § Oracle corrections; ledger: docs/assurance/CALL-BINDING-RECEIVER-1/oracle-corrections.md)

- OC-1 (2026-09-14; carried as INPUT-2): stage-3 admission refused INPUT-1 — preserved obligations RG-REQ-001-L02/L04/L05 and RG-REQ-011-L06 "lack an explicit no-behavior-change oracle" (the runtime requires a check's `expected` to say no-behavior-change / remain / preserve for every preserved L; document review does not run that parser — agent-manager TD-019). Text-only: CBR-C11, CBR-C16 and CBR-C18 `expected` gained the marker (C18: British "behaviour" → "behavior"); the runtime's parser, run by the manager before re-review, additionally required each P-CBR-0n to be listed in a check's obligationIds — added to CBR-C05/C16 (P-CBR-01), C03/C04/C16 (P-CBR-02), C07 (P-CBR-03, P-CBR-04); reviewer finding F-CBR-OC1 (PREP-2 cycle 2): CBR-C07's `expected` matched the marker only through the substring `remain` in "honest remainder" — rewritten to state each preserved behaviour explicitly. No allocation set, check id, requirement text or candidate path changed.
- OC-2 (2026-09-14, decision D-CBR-XFILE-1; carried as INPUT-3 with review because it changes what a check asserts): CBR-C12 corrected from "callees include VersionSet::Recover" (unattainable within the candidate paths — header-declared member, per-file extractor) to "self-row gone; unique-name `versions_->` members kept; `versions_->Recover()` at :324 a counted unresolved row", with the assertion reading the `callees` list only (PREP-3 author finding: the whole-document scan also collected the focus symbol echoed in `target`); CBR-C17 corrected from byte-identical text to identical cycle MEMBER SETS via `cycles --json` (plus equal module_edge_count/count and the Snapshot/walk-stripped text) — the walk entry is nondeterministic today (CYCLES-WALK-DETERMINISM-1; PREP-3 reviewer finding F-PREP3-3 required the member-set comparison the decision names). Follow-ups named in §4.
