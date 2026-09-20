<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "EXPLAIN-TYPE-SECTIONS-1",
  "baselinePath": "docs/requirements/baselines/EXPLAIN-TYPE-SECTIONS-1-INPUT-1.json",
  "parentRequirementIds": [
    "RG-REQ-001",
    "RG-REQ-002",
    "RG-REQ-003",
    "RG-REQ-005",
    "RG-REQ-010",
    "RG-REQ-011",
    "RG-REQ-012"
  ],
  "implements": [
    "RG-REQ-005-L04"
  ],
  "preserves": [
    "RG-REQ-005-L05",
    "RG-REQ-005-L06",
    "RG-REQ-003-L01",
    "RG-REQ-010-L01",
    "RG-REQ-012-L07",
    "RG-REQ-012-L04",
    "RG-REQ-001-L04",
    "RG-REQ-011-L06",
    "RG-REQ-002-L08"
  ],
  "preservationObligationIds": [
    "P-ETS-01",
    "P-ETS-02",
    "P-ETS-03",
    "P-ETS-04",
    "P-ETS-05"
  ],
  "changes": [
    "RG-REQ-012-L06"
  ],
  "acceptanceBoundary": "The `explain` human and JSON outputs of the candidate rmap against the before binary (both built at the recorded base revision / the candidate) on FOUR pre-provisioned isolated roots — leveldb and django (fresh indexes), FRAKTAG and vcmi (copies of the retained audit-v0.18.0 stores) — plus cargo test -p repo-graph-storage (--test agent_impl and --lib), -p repo-graph-agent (whole crate), -p repo-graph-daemon-runtime --lib (WHOLE unit suite) and the two explain route-consistency integration suites, -p repo-graph-rgr --lib, as named per check.",
  "candidatePaths": [
    "rust/crates/agent/src/storage_port.rs",
    "rust/crates/agent/src/explain/mod.rs",
    "rust/crates/agent/src/dto/signal.rs",
    "rust/crates/agent/src/lib.rs",
    "rust/crates/agent/src/aggregators/complexity_tests.rs",
    "rust/crates/agent/tests/common/mod.rs",
    "rust/crates/agent/tests/explain_symbol.rs",
    "rust/crates/storage/src/agent_impl.rs",
    "rust/crates/storage/tests/agent_impl.rs",
    "rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs",
    "rust/crates/daemon-runtime/src/orient_serve/tests.rs",
    "rust/crates/daemon-runtime/src/explain_serve_tests/spy.rs",
    "rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs",
    "rust/crates/rgr/src/presentation/explain_sections.rs",
    "rust/crates/rgr/src/presentation/explain.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/EXPLAIN-TYPE-SECTIONS-1/verification.json",
    "docs/assurance/EXPLAIN-TYPE-SECTIONS-1/implementation-review.json"
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
      "checkId": "ETS-C01",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-001-L04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-storage --test agent_impl 2>&1 | tee /tmp/ets-c01.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c01.txt && for t in list_members_of_type_returns_direct_members_only list_members_of_type_orders_focus_file_first_then_other_files_by_path_and_line list_members_of_type_marks_forward_declarations find_file_importers_lists_importing_files_with_owning_module find_file_importers_is_empty_for_a_file_nobody_imports list_symbols_in_file_returns_ordered_entries; do grep -qE \"^test .*$t .* ok$\" /tmp/ets-c01.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "storage integration tests in rust/crates/storage/tests/agent_impl.rs (the file that already tests `list_symbols_in_file_returns_ordered_entries`, kept green): NEW `list_members_of_type_returns_direct_members_only` (fixture nodes `A`, `A::m1`, `A::m2`, `A::Inner`, `A::Inner::m3`, `A.m4`-style dotted sibling in another file; the read for `A` returns m1, m2, Inner (direct members: one more `::` or `.` segment and NO further separator), never `A::Inner::m3`), `…orders_focus_file_first_then_other_files_by_path_and_line`, `…marks_forward_declarations` (a member node whose metadata `forward_decl` is true is returned with `forward_decl: true`; unreadable metadata is an error, never defaulted), NEW `find_file_importers_lists_importing_files_with_owning_module` (IMPORTS edges whose TARGET is the FILE node of the given path → distinct importing file paths with their OWNS-edge module_path, ordered by path) and `…is_empty_for_a_file_nobody_imports`"
      },
      "expected": "exit 0: the two new SQLite reads select exactly what §2.1.1 states — direct members by qualified-name containment (no schema change, no new column; stable keys untouched — RG-REQ-001-L04 preserved as no behavior change) and reverse IMPORTS edges into a FILE node grouped by the OWNS-edge module — with deterministic order"
    },
    {
      "checkId": "ETS-C02",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-005-L05",
        "RG-REQ-010-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-agent 2>&1 | tee /tmp/ets-c02.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c02.txt && for t in explain_type_focus_emits_members_and_referenced_by explain_function_focus_emits_no_members_or_referenced_by explain_type_focus_members_count_is_the_pre_truncation_total explain_symbol_no_file_only_sections explain_resolves_qualified_suffix_through_shared_resolver; do grep -qE \"^test .*$t .* ok$\" /tmp/ets-c02.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the whole agent crate (unit + integration; the fakes `FakeStorage` and `FakeAgentStorage` gain the two REQUIRED port methods). NEW in rust/crates/agent/tests/explain_symbol.rs: `explain_type_focus_emits_members_and_referenced_by` (a CLASS focus on the fake yields EXPLAIN_MEMBERS with the fake's members and EXPLAIN_REFERENCED_BY with the fake's importers grouped by module — top_modules like Callers' `group_by_module`), `explain_function_focus_emits_no_members_or_referenced_by` (a FUNCTION focus emits neither code — byte-identical signal set to today), `explain_type_focus_members_count_is_the_pre_truncation_total` (with 20 members and Budget::Small the evidence `count` is 20 while `items` is capped by `items_cap` and `items_truncated`/`items_omitted_count` say so); the existing `explain_symbol_no_file_only_sections` (a symbol focus still never carries EXPLAIN_SYMBOLS/EXPLAIN_IMPORTS/EXPLAIN_FILES) and `explain_resolves_qualified_suffix_through_shared_resolver` (resolution routing untouched — RG-REQ-005-L05) stay green; seed-candidate behaviour untouched (RG-REQ-010-L01 preserved: `semantic_no_match_*` paths unchanged)"
      },
      "expected": "exit 0: a type focus gains the two additive signals, a non-type focus is unchanged (no behavior change for FUNCTION/METHOD), the count is budget-invariant, resolution and seed tiers remain untouched"
    },
    {
      "checkId": "ETS-C03",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-012-L07",
        "RG-REQ-012-L04",
        "RG-REQ-005-L06",
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr --lib presentation::explain 2>&1 | tee /tmp/ets-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c03.txt && for t in explain_members_section_anchors_file_line_and_marks_decl explain_members_section_omits_absent_line_never_zero explain_members_over_cap_renders_more_line_and_full_uncaps explain_referenced_by_section_names_count_top_modules_and_files explain_type_zero_callers_line_says_a_type_is_not_called explain_function_zero_callers_line_is_unchanged explain_members_rows_render_only_from_carried_evidence tier0_symbols_section_anchors_present_line_and_omits_absent tier0_symbols_section_never_emits_zero_line tier1_callers_anchor_file_line_when_both_present no_high_confidence_beside_no_match semantic_no_match_renders_labeled_candidates_in_human_mode explain_cycles_absent_walk_renders_unordered_no_arrows; do grep -qE \"^test .*$t .* ok$\" /tmp/ets-c03.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rgr render tests in rust/crates/rgr/src/presentation/explain.rs (JSON-fixture style like `tier0_symbols_section_anchors_present_line_and_omits_absent`): NEW `explain_members_section_anchors_file_line_and_marks_decl` (`Members (N)` header; row `  - <name> (<subtype>)  <path>:<line>`; a `forward_decl: true` item renders `(<subtype>, decl)`), `…omits_absent_line_never_zero` (no line or line 0 → bare row through the shared `anchor()` chokepoint), `…over_cap_renders_more_line_and_full_uncaps` (16 items: 15 rows + `  ... (1 more)`; `--full` renders all), `explain_referenced_by_section_names_count_top_modules_and_files` (`Referenced by (N files)`; a `top modules:` line from `top_modules`; file rows capped at 15 with the same overflow line), `explain_type_zero_callers_line_says_a_type_is_not_called` (identity `subtype` ∈ CLASS|STRUCT|ENUM|INTERFACE|TRAIT and callers count 0 → `Callers (0) — a type is not called; see Members / Referenced by`; the same for Callees), `explain_function_zero_callers_line_is_unchanged` (subtype FUNCTION → `Callers (0)` exactly as today), `explain_members_rows_render_only_from_carried_evidence` (a malformed item — non-string name, non-integer line — renders `members unreadable on this snapshot`, never a fabricated or silently shorter list); the existing anchor/tier tests, `no_high_confidence_beside_no_match`, `semantic_no_match_renders_labeled_candidates_in_human_mode` and Q4's `explain_cycles_absent_walk_renders_unordered_no_arrows` stay green"
      },
      "expected": "exit 0: every member row anchors `path:line` from the carried evidence only (RG-REQ-012-L07), caps change length never counts (RG-REQ-012-L04), the type zero-line reads `a type is not called; see Members / Referenced by` (RG-REQ-005-L04) only for type subtypes, and the confidence, seed-candidate and cycles renderings remain unchanged (no behavior change)"
    },
    {
      "checkId": "ETS-C04",
      "obligationIds": [
        "RG-REQ-005-L05",
        "P-ETS-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib explain_serve 2>&1 | tee /tmp/ets-c04.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c04.txt && for t in type_focus_members_and_referenced_by_are_sqlite_delegated_on_green m2_parity_explain_path_focus_equals_sqlite_with_nonempty_cycle m2_parity_explain_file_focus_equals_sqlite; do grep -qE \"^test .*$t .* ok$\" /tmp/ets-c04.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-daemon-runtime --lib orient_serve 2>&1 | tee /tmp/ets-c04b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c04b.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "NEW rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs::`type_focus_members_and_referenced_by_are_sqlite_delegated_on_green`: on the green fixture with the M-2 decorator wrapped in the `ServeSpy`, `run_explain` on a CLASS focus yields a NON-EMPTY EXPLAIN_MEMBERS and an EXPLAIN_REFERENCED_BY whose values equal the bare SQLite serve's (the spy's explicit delegating overrides for the two new methods record the calls as allowed reads; the six (b) methods still panic on the spy) — the ruling-B shape (`resolve_symbol`, `count_symbol_definitions_by_name`): SQLite-delegated, recorded, never silently empty; the two M-2 explain parity certificates and the orient_serve suite (PartialSpy/M2Spy gain delegating impls) stay green"
      },
      "expected": "exit 0: the two new reads are honestly `{sqlite}`-served through the decorator on a green LiveGraph (the nodes-free-on-green invariant is amended for these two reads exactly as ruling B amended it for resolution — recorded in §2.1.4), the parity certificates unchanged (no behavior change on every other leaf — P-ETS-02)"
    },
    {
      "checkId": "ETS-C05",
      "obligationIds": [
        "P-ETS-02",
        "P-ETS-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib 2>&1 | tee /tmp/ets-c05.txt | grep -E '^test result: ok\\. [0-9]+ passed; 0 failed' && ! grep -E '^test .* FAILED$' /tmp/ets-c05.txt && cargo test -p repo-graph-storage --lib 2>&1 | tee /tmp/ets-c05b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c05b.txt && cargo test -p repo-graph-rgr --lib 2>&1 | tee /tmp/ets-c05c.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ets-c05c.txt && cargo test -p repo-graph-daemon-runtime --test explain_cycle_walk_route_consistency --test cycle_honesty_route_consistency 2>&1 | tee /tmp/ets-c05d.txt | grep -cE '^test result: ok\\.' | grep -qx 2",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the WHOLE daemon-runtime unit suite, the whole storage and rgr lib suites, and Q4's two explain route-consistency integration suites — the acceptance boundary of every crate the candidate touches, run to completion in the foreground"
      },
      "expected": "exit 0: no behavior change anywhere else in the daemon, storage or renderer crates (every other explain section, orient, cycles, deps, trust remain as before)"
    },
    {
      "checkId": "ETS-C06",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-012-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before:$PATH\" \"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap\" explain leveldb::DBImpl > /tmp/ets-ldb-class-before.txt 2> /tmp/ets-ldb-class-before.txt.err ) || { echo \"RUN-FAILED /tmp/ets-ldb-class-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl > /tmp/ets-ldb-class-after.txt 2> /tmp/ets-ldb-class-after.txt.err ) || { echo \"RUN-FAILED /tmp/ets-ldb-class-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl --full > /tmp/ets-ldb-class-full.txt 2> /tmp/ets-ldb-class-full.txt.err ) || { echo \"RUN-FAILED /tmp/ets-ldb-class-full.txt\"; exit 1; } && ! grep -qE '^Members' /tmp/ets-ldb-class-before.txt && grep -qE '^Members \\(73\\)$' /tmp/ets-ldb-class-after.txt && grep -qE '^  - Recover \\(METHOD\\)  db/db_impl.cc:292$' /tmp/ets-ldb-class-full.txt && grep -qE '^  \\.\\.\\. \\(58 more\\)$' /tmp/ets-ldb-class-after.txt && grep -qE '^Referenced by \\(9 files\\)$' /tmp/ets-ldb-class-after.txt && grep -qE '^Callers \\(0\\) — a type is not called; see Members / Referenced by$' /tmp/ets-ldb-class-after.txt && grep -qE '^Callers \\(0\\)$' /tmp/ets-ldb-class-before.txt",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb by the base-revision binary on 2026-09-20, 1 s; served with the before binary /private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap{,d} — copied from rust/target/release on the CLEAN tree BEFORE any candidate edit — and the candidate binary, both under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off)",
        "inputs": "leveldb `explain leveldb::DBImpl` (the class, definition db/db_impl.h:29) on the SAME index: BEFORE (read from the product 2026-09-20) prints `Callers (0)`, `Callees (0)`, a cycle line and trust — nothing about the class; the store holds 73 direct member nodes `leveldb::DBImpl::<name>` (CONSTRUCTOR/DESTRUCTOR/METHOD, declarations in db/db_impl.h and definitions in db/db_impl.cc — e.g. `Recover` defined at db/db_impl.cc:292) and 9 files import db/db_impl.h (db 8, helpers/memenv 1); AFTER: `Members (73)` with 15 rows + `  ... (58 more)` in the default view and every row under `--full`, `Referenced by (9 files)`, and the type zero-line"
      },
      "expected": "exit 0: the class answer describes the class — 73 anchored members (decl rows marked), 9 referencing files, the honest zero-line — where before it said nothing; counts are the store's, read from the same index"
    },
    {
      "checkId": "ETS-C07",
      "obligationIds": [
        "P-ETS-01",
        "RG-REQ-005-L05",
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before:$PATH\" \"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap\" explain leveldb::DBImpl::Recover > /tmp/ets-ldb-method-before.txt 2> /tmp/ets-ldb-method-before.txt.err ) || { echo \"RUN-FAILED /tmp/ets-ldb-method-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl::Recover > /tmp/ets-ldb-method-after.txt 2> /tmp/ets-ldb-method-after.txt.err ) || { echo \"RUN-FAILED /tmp/ets-ldb-method-after.txt\"; exit 1; } && diff /tmp/ets-ldb-method-before.txt /tmp/ets-ldb-method-after.txt && grep -qE '^Callers \\(1\\)$' /tmp/ets-ldb-method-after.txt && ! grep -qE '^Members|^Referenced by|a type is not called' /tmp/ets-ldb-method-after.txt",
        "cwd": ".",
        "environment": "same root/binaries as ETS-C06",
        "inputs": "leveldb `explain leveldb::DBImpl::Recover` (a METHOD focus) on the same index: the whole output byte-identical before/after — `Callers (1)` / `- Open (db)  db/db_impl.cc:1503`, the callees, the cycles block (Q4's ring), trust — and no Members/Referenced-by/type zero-line"
      },
      "expected": "exit 0: a non-type focus is byte-identical (no behavior change — P-ETS-01); resolution and the cycles block untouched"
    },
    {
      "checkId": "ETS-C08",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-012-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before:$PATH\" \"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap\" explain BaseHandler > /tmp/ets-dj-class-before.txt 2> /tmp/ets-dj-class-before.txt.err ) || { echo \"RUN-FAILED /tmp/ets-dj-class-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain BaseHandler > /tmp/ets-dj-class-after.txt 2> /tmp/ets-dj-class-after.txt.err ) || { echo \"RUN-FAILED /tmp/ets-dj-class-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain BaseHandler --json > /tmp/ets-dj-class-after.json 2> /tmp/ets-dj-class-after.json.err ) || { echo \"RUN-FAILED /tmp/ets-dj-class-after.json\"; exit 1; } && grep -qE '^Members \\(10\\)$' /tmp/ets-dj-class-after.txt && grep -qE '^  - load_middleware \\(METHOD\\)  django/core/handlers/base.py:27$' /tmp/ets-dj-class-after.txt && ! grep -q 'load_middleware\\.' /tmp/ets-dj-class-after.txt && grep -qE '^Referenced by \\(1 files\\)$' /tmp/ets-dj-class-after.txt && python3 -c \"import json\nd=json.load(open('/tmp/ets-dj-class-after.json'))\nsig=[s for s in d['value']['signals'] if s['value'].get('code')=='EXPLAIN_MEMBERS'][0]['value']\nrb=[s for s in d['value']['signals'] if s['value'].get('code')=='EXPLAIN_REFERENCED_BY'][0]['value']\nassert sig['evidence']['count']==10 and rb['evidence']['count']==1, (sig['evidence']['count'], rb['evidence']['count'])\nprint('json members',sig['evidence']['count'],'referenced_by',rb['evidence']['count'])\"",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/EXPLAIN-TYPE-SECTIONS-1-django (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django, 22 s); before/candidate binaries as in ETS-C06",
        "inputs": "django `explain BaseHandler` (Python; definition django/core/handlers/base.py:21): the store holds 10 direct methods (`BaseHandler.load_middleware` at :27, `adapt_method_mode` :106, `get_response` :138, …) and 24 NESTED nodes such as `BaseHandler.load_middleware.get_response` that the direct-member rule must exclude; 1 file imports django/core/handlers/base.py (django/test); JSON carries the same counts as the human text (the coherence envelope `value.signals[].value.{code,evidence}` shape the existing explain JSON uses — read it, do not assume)"
      },
      "expected": "exit 0: Python types list their methods (the fix is not C++-gated), nested closures are not members, JSON and human agree on the counts (RG-REQ-012-L06, additive fields reported)"
    },
    {
      "checkId": "ETS-C09",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-005-L05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before:$PATH\" \"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap\" explain ConversationManager > /tmp/ets-frk-class-before.txt 2> /tmp/ets-frk-class-before.txt.err ) || { echo \"RUN-FAILED /tmp/ets-frk-class-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain ConversationManager > /tmp/ets-frk-class-after.txt 2> /tmp/ets-frk-class-after.txt.err ) || { echo \"RUN-FAILED /tmp/ets-frk-class-after.txt\"; exit 1; } && grep -qE '^File: packages/engine/src/core/ConversationManager.ts:61$' /tmp/ets-frk-class-after.txt && grep -qE '^Members \\(8\\)$' /tmp/ets-frk-class-after.txt && grep -qE '^  - createSession \\(METHOD\\)  packages/engine/src/core/ConversationManager.ts:71$' /tmp/ets-frk-class-after.txt && grep -qE '^Referenced by \\(1 files\\)$' /tmp/ets-frk-class-after.txt && ! grep -qE '^Members' /tmp/ets-frk-class-before.txt",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag: a COPY of the retained audit-v0.18.0 FRAKTAG store (databases/e444686289ee2bf9.db, registry filtered); before/candidate binaries as in ETS-C06",
        "inputs": "FRAKTAG `explain ConversationManager` BY NAME (TypeScript; SYMBOL-IDENTITY-1's earlier 'lists its methods' was a FILE-focus answer): the store holds 8 direct members (`constructor` :62, `createSession` :71, `listSessions` :103, `logTurn` :133, …) and 1 importing file; the focus resolves by name to packages/engine/src/core/ConversationManager.ts:61 (RG-REQ-005-L05 unchanged)"
      },
      "expected": "exit 0: a TypeScript class explained BY NAME lists its members and referencing files — the proof that the fix is neither path-focus nor C++-gated"
    },
    {
      "checkId": "ETS-C10",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-012-L04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before:$PATH\" \"/private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/rmap\" explain CGHeroInstance > /tmp/ets-vcmi-class-before.txt 2> /tmp/ets-vcmi-class-before.txt.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance > /tmp/ets-vcmi-class-after.txt 2> /tmp/ets-vcmi-class-after.txt.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance --json > /tmp/ets-vcmi-class-after.json 2> /tmp/ets-vcmi-class-after.json.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-after.json\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance --budget small --json > /tmp/ets-vcmi-class-small.json 2> /tmp/ets-vcmi-class-small.json.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-small.json\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance --full --json > /tmp/ets-vcmi-class-fulljson.json 2> /tmp/ets-vcmi-class-fulljson.json.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-fulljson.json\"; exit 1; } && grep -qE '^File: lib/mapObjects/CGHeroInstance.h:55$' /tmp/ets-vcmi-class-after.txt && grep -qE '^Members \\(135\\)$' /tmp/ets-vcmi-class-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance --full > /tmp/ets-vcmi-class-full.txt 2> /tmp/ets-vcmi-class-full.txt.err ) || { echo \"RUN-FAILED /tmp/ets-vcmi-class-full.txt\"; exit 1; } && grep -qE '^  - getFactionID \\(METHOD\\)  lib/mapObjects/CGHeroInstance.cpp:81$' /tmp/ets-vcmi-class-full.txt && grep -qE '^  \\.\\.\\. \\(120 more\\)$' /tmp/ets-vcmi-class-after.txt && grep -qE '^Referenced by \\(178 files\\)$' /tmp/ets-vcmi-class-after.txt && grep -qE '^  top modules: ' /tmp/ets-vcmi-class-after.txt && grep -qE '^Callers \\(0\\) — a type is not called; see Members / Referenced by$' /tmp/ets-vcmi-class-after.txt && python3 -c \"import json\ndef cnt(p,code):\n    d=json.load(open(p)); s=[s for s in d['value']['signals'] if s['value'].get('code')==code][0]['value']; return s['evidence']['count'], len(s['evidence']['items'])\nfor code in ('EXPLAIN_MEMBERS','EXPLAIN_REFERENCED_BY'):\n    a=cnt('/tmp/ets-vcmi-class-after.json',code); s=cnt('/tmp/ets-vcmi-class-small.json',code); f=cnt('/tmp/ets-vcmi-class-fulljson.json',code)\n    assert a[0]==s[0]==f[0], (code,a,s,f); assert s[1]<=a[1]<=f[1]==f[0], (code,a,s,f); print(code,'count',a[0],'items small/medium/full',s[1],a[1],f[1])\"",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi: a COPY of the retained audit-v0.18.0 vcmi store (databases/c956217ff5c253d9.db, snapshot …/2834d519 — the audit's own snapshot; registry filtered); before/candidate binaries as in ETS-C06",
        "inputs": "vcmi `explain CGHeroInstance` (C++; 71 CLASS nodes of that name — 70 forward declarations + the definition at lib/mapObjects/CGHeroInstance.h:55, which the resolver already picks, RG-REQ-005-L07): the store holds 135 direct member nodes `CGHeroInstance::<name>` (declared in lib/mapObjects/CGHeroInstance.h:176 and, as the stored node, defined at lib/mapObjects/CGHeroInstance.cpp:81 — vcmi's in-class prototypes are not separate nodes, so the members are the definitions plus in-header inline members; header rows come first by the ordering rule, the .cpp definitions after, so `getFactionID (METHOD)  lib/mapObjects/CGHeroInstance.cpp:81` is asserted under `--full` and the default view shows 15 rows + `... (120 more)`) and 178 files importing lib/mapObjects/CGHeroInstance.h (top-level dirs lib 61, client 57, test 27, mapeditor 10, server 9 — the `top modules:` line names the OWNS-edge modules, REPORTED); across `--budget small`, the default and `--full` the JSON `count` of both new signals is identical while `items` length is monotonic and equals `count` under `--full`"
      },
      "expected": "exit 0: the audit's flagship probe now describes the class — 135 anchored members (header-first, then the .cpp definitions), 178 referencing files with their top modules, the honest zero-line — and budgets change length, never the counts (RG-REQ-012-L04)"
    },
    {
      "checkId": "ETS-C11",
      "obligationIds": [
        "P-ETS-03",
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "for t in ldb-class dj-class frk-class vcmi-class; do diff <(sed -n '/^Import cycles/,/^Trust/p' /tmp/ets-$t-before.txt) <(sed -n '/^Import cycles/,/^Trust/p' /tmp/ets-$t-after.txt) || { echo \"CYCLES-BLOCK-MOVED $t\"; exit 1; }; diff <(sed -n '/^Trust/,$p' /tmp/ets-$t-before.txt) <(sed -n '/^Trust/,$p' /tmp/ets-$t-after.txt) || { echo \"TRUST-BLOCK-MOVED $t\"; exit 1; }; diff <(sed -n '1,/^Confidence/p' /tmp/ets-$t-before.txt) <(sed -n '1,/^Confidence/p' /tmp/ets-$t-after.txt) || { echo \"HEADER-MOVED $t\"; exit 1; }; done",
        "cwd": ".",
        "environment": "the captures of ETS-C06/C08/C09/C10",
        "inputs": "on all four type probes, the header (Repo/Target/Kind/File/Confidence), the Import-cycles block (Q4's ring or unordered form) and the Trust block are byte-identical before/after; only the Callers/Callees zero-lines and the two NEW sections differ"
      },
      "expected": "exit 0: every existing explain section renders exactly as before on a type focus (no behavior change — P-ETS-03); the cycles block keeps Q4's rule (RG-REQ-003-L01)"
    },
    {
      "checkId": "ETS-C12",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-ETS-05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "test \"$(shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -c1-16)\" = \"$(grep -E '^registry-before ' .agent-manager/slices/EXPLAIN-TYPE-SECTIONS-1/build-progress.md | tail -n1 | awk '{print $2}')\" && PIDS=$(pgrep -x rmapd | paste -sd, -) && { test -z \"$PIDS\" || test \"$(ps -o command= -p \"$PIDS\" | grep -vc '/\\.local/bin/rmapd')\" = 0; } && ls -d /private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb /private/tmp/EXPLAIN-TYPE-SECTIONS-1-django /private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag /private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi >/dev/null && ! ls -d /private/tmp/EXPLAIN-TYPE-SECTIONS-1-before 2>/dev/null && git worktree list | grep -vc 'm-r2-baseline-worktree\\|\\[main\\]' | grep -qx 0",
        "cwd": ".",
        "environment": "the operator's real registry (digest only); the four pre-provisioned roots stay for the manager's closeout sweep; the before-binary dir is removed after the last proof",
        "inputs": "build-progress.md records `registry-before <16-hex>` (the value printed by `shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -c1-16`) BEFORE the first proof and the same command's value after cleanup; every rmap ran under RMAP_STATE_ROOT=/private/tmp/EXPLAIN-TYPE-SECTIONS-1-* with stdio and the auto passes off; the only surviving `rmapd` is the operator's ~/.local/bin one; no extra worktree"
      },
      "expected": "exit 0: no behavior change outside the isolated roots — the operator's registry digest remains identical before and after, no candidate/before daemon survives, the four proof roots remain for the manager, the before dir is gone, no extra worktree"
    },
    {
      "checkId": "ETS-C13",
      "obligationIds": [
        "RG-REQ-005-L04",
        "RG-REQ-002-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "git diff --check && (cd rust && cargo fmt --check -p repo-graph-agent -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-rgr && cargo clippy -p repo-graph-agent -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-rgr --tests -- -D warnings > /tmp/ets-clippy.log 2>&1 && tail -n1 /tmp/ets-clippy.log) && diff <(git status --short -- rust | sort) <(printf ' M rust/crates/agent/src/aggregators/complexity_tests.rs\\n M rust/crates/agent/src/dto/signal.rs\\n M rust/crates/agent/src/explain/mod.rs\\n M rust/crates/agent/src/lib.rs\\n M rust/crates/agent/src/storage_port.rs\\n M rust/crates/agent/tests/common/mod.rs\\n M rust/crates/agent/tests/explain_symbol.rs\\n M rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs\\n M rust/crates/daemon-runtime/src/explain_serve_tests/spy.rs\\n M rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs\\n M rust/crates/daemon-runtime/src/orient_serve/tests.rs\\n M rust/crates/rgr/src/presentation/explain.rs\\n M rust/crates/rgr/src/presentation/explain_sections.rs\\n M rust/crates/storage/src/agent_impl.rs\\n M rust/crates/storage/tests/agent_impl.rs\\n' | sort) && ! grep -n 'fn list_members_of_type' rust/crates/agent/src/storage_port.rs | grep -q 'Ok(Vec::new())' && grep -c 'fn list_members_of_type' rust/crates/agent/src/storage_port.rs rust/crates/storage/src/agent_impl.rs rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs rust/crates/daemon-runtime/src/explain_serve_tests/spy.rs rust/crates/daemon-runtime/src/orient_serve/tests.rs rust/crates/agent/tests/common/mod.rs rust/crates/agent/src/aggregators/complexity_tests.rs | awk -F: '{s+=$2} END {exit !(s>=8)}'",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "git status/diff of the candidate; rustfmt + clippy -D warnings over the four touched crates; the exact set of fifteen modified paths and nothing else; the two new port methods are REQUIRED (no defaulted body in the trait) and implemented in every implementor (trait + StorageConnection + decorator + ServeSpy + PartialSpy + M2Spy + FakeAgentStorage + FakeStorage = ≥8 occurrences of `fn list_members_of_type`)"
      },
      "expected": "exit 0: whitespace-, rustfmt- and clippy-clean; the working tree under rust/ holds exactly the fifteen candidate paths; no defaulted read that could render an empty Members section as verified (the dormant-capability shape); the new sentences carry no internal label — the reader-facing labels remain as today, no behavior change for RG-REQ-002-L08"
    }
  ]
}
-->

# EXPLAIN-TYPE-SECTIONS-1 — `explain <Type>` describes the type: members, bases, derived, referenced by

Status: SPECIFIED 2026-09-12; increment 1 GROUNDED and re-verified on HEAD 45cc329 on 2026-09-20 (grounding basis, corrected 2026-09-20 per review finding F2: the BEFORE-state product output below — `Callers (0)`/`Callees (0)`, the header lines, nothing about the type — was read from the base-revision binary on the isolated roots that day; the direct-member and importing-file counts (73/10/8/135 and 9/1/1/178) are store queries; the AFTER-state `Members`/`Referenced by` renderings are acceptance TARGETS for this still-unbuilt increment, not product output observed at HEAD) · Track: audit round six, Q6 (HIGH; RC-3 never worked, language-wide). THREE increments, each independently shippable and reviewed: (1) RENDER — members + referenced-by from existing rows, every language, no reindex — THIS packet; (2) C++ ANCHOR — base-clause edges on the type node; (3) C++ MACRO RECOVERY — base clauses on macro-decorated declarations. Increments 2–3 are packeted after 1 ships. Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13).

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements (increment 1):** RG-REQ-005-L04 — the members half, the referenced-by half, AND the type zero-line `Callers (0) — a type is not called; see Members / Referenced by` (all three are RG-REQ-005-L04's own text; L04 stays PARTIALLY MET until increments 2–3 deliver its bases/derived half — the allocation says so, it does not predict them).

**Not implemented here — deferred (corrected 2026-09-20 per review finding F1):** RG-REQ-005-L09 — the classified unresolved-call COUNT beside a zero (`0 resolved callers; N unresolved calls name x`) — is NOT delivered by this increment; it is CLAIM-INVARIANT-1's work, deferred there in full (no half is done here). The 2026-09-12 draft mis-credited the type zero-line to L09; that sentence is RG-REQ-005-L04's own text and is now attributed to L04 above. L09 stays in the manifest `reviewObligationIds` as a reviewed-but-deferred obligation (the obligation set is preserved), and is absent from `implements`/`preserves`/`changes` because the increment neither implements, preserves nor changes it.

**Changes (pre-authorised, REPORTED never predicted):** RG-REQ-012-L06 — the explain JSON gains two additive signals (`EXPLAIN_MEMBERS`, `EXPLAIN_REFERENCED_BY`); the human text renders the same counts and rows (ETS-C08, ETS-C10).

**Preserves:** RG-REQ-005-L05 (resolution routing — ruling B untouched; the two new reads are SQLite-delegated exactly as `resolve_symbol` is; ETS-C02/C04/C07/C09), RG-REQ-005-L06 (confidence — ETS-C03), RG-REQ-003-L01 (Q4's cycles block — ETS-C03/C07/C11), RG-REQ-010-L01 (seed candidates — ETS-C02/C03), RG-REQ-012-L07 (anchors from one store — ETS-C03/C06), RG-REQ-012-L04 (budgets change length, never counts — ETS-C03/C10), RG-REQ-001-L04 (stable keys, no schema change — ETS-C01), RG-REQ-011-L06 (isolation — ETS-C12), RG-REQ-002-L08 (no internal label in the new sentences — ETS-C13).

**Why RG-REQ-002-L07 is NOT in this increment's implements (the 2026-09-12 spec listed it):** the spec's "Base classes: not recorded from a type focus on this build (… increment 2)" would read identically on every repo and offers a remedy the user cannot run — both forbidden (the human's map-is-not-the-territory rule; L07's "a remedy with no implementation is never offered"). Increment 1 omits bases/derived entirely: nothing claimed, nothing fabricated; increment 2 renders them from real edges.

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-ETS-01 | A non-type focus (FUNCTION/METHOD/CONSTRUCTOR/…) renders byte-identically: leveldb `explain leveldb::DBImpl::Recover` whole output unchanged; no Members/Referenced-by/type zero-line | ETS-C02; ETS-C07 |
| P-ETS-02 | The LiveGraph parity certificates and every other explain/orient leaf are unchanged; the two new reads are SQLite-delegated through the M-2 decorator and RECORDED by the spy (ruling-B shape), never a defaulted empty read | ETS-C04; ETS-C05; ETS-C13 |
| P-ETS-03 | On a type focus every EXISTING section — header, Import cycles (Q4's rule), Trust — is byte-identical before/after; only the zero-lines and the two new sections differ | ETS-C11 |
| P-ETS-04 | Every existing storage/rgr/daemon test remains green | ETS-C05 |
| P-ETS-05 | Every proof isolated; the operator's registry digest unchanged; no daemon or before-dir left; the four proof roots kept for the manager | ETS-C12 |

## 1. Problem (ROOT-CAUSED — RC-3; docs/audits/2026-09-08-root-causes-v0.18.0.md; re-verified on HEAD 45cc329, 2026-09-20)

`rust/crates/agent/src/explain/mod.rs:466-698 explain_symbol` emits IDENTITY (:484), CALLERS (:521), CALLEES (:550), CYCLES (:586, module-context only), BOUNDARY/GATE (module-context), TRUST (:654), the dormant MEASUREMENTS (:662) — for every language and subtype; a CLASS is never called → `Callers (0) / Callees (0)` by construction. `rust/crates/rgr/src/presentation/explain_sections.rs:164-185 render_signal_section` has no members/bases/derived/referenced-by arm (the audit cited :84-97; the dispatch moved). The only member listing is `EXPLAIN_SYMBOLS` inside `explain_file` (mod.rs:757, `storage.list_symbols_in_file` — a FILE listing reachable only by exact path; `agent/tests/explain_symbol.rs:72 explain_symbol_no_file_only_sections` forbids it on a symbol focus, so members MUST be a new code). Outward, read from the product on 2026-09-20 on isolated roots: leveldb `explain leveldb::DBImpl` (definition db/db_impl.h:29), django `explain BaseHandler` (django/core/handlers/base.py:21), FRAKTAG `explain ConversationManager` by NAME (packages/engine/src/core/ConversationManager.ts:61) and vcmi `explain CGHeroInstance` (lib/mapObjects/CGHeroInstance.h:55) all print `Callers (0)`, `Callees (0)`, a cycle line and trust — nothing about the type. The data exists in the stores: direct member nodes by qualified-name containment (DBImpl 73, BaseHandler 10 — plus 24 nested closure nodes such as `BaseHandler.load_middleware.get_response` that must be excluded, ConversationManager 8, CGHeroInstance 135), and IMPORTS edges into the type's FILE node (db/db_impl.h 9 files; base.py 1; ConversationManager.ts 1; CGHeroInstance.h 178 — lib 61, client 57, test 27, mapeditor 10, server 9). `parent_node_uid` is None in every extractor; containment rides in `qualified_name`. There is NO reverse-import read today (`find_file_imports`, storage/src/agent_impl.rs:1441-1470, is forward-only) — a new SQL is required, not a reuse. C++ base-clause edges are anchored on the FILE (`cpp-extractor/src/extractor.rs:1387 source_node_uid: ctx.file_node_uid`; the audit cited :1288) and dropped on macro-decorated declarations (:797-799) — increments 2–3.

## 2. Contract (increment 1)

### 2.1 The fix, at the cause — two required port reads, two additive signals, two renderer sections, one zero-line

1. **Two REQUIRED `AgentStorageRead` methods (`rust/crates/agent/src/storage_port.rs`, the explain-focus block at :960), no defaulted body** — a defaulted `Ok(Vec::new())` would let a test double or the spy render an empty Members section as if verified (the dormant-capability shape); the compiler's exhaustiveness lists every implementor: `StorageConnection` (storage/src/agent_impl.rs), `OrientServeDecorator` (daemon-runtime/src/orient_serve/storage_port_impl.rs — plain delegations to `self.inner`, the ruling-B shape of `resolve_symbol` :210-216), `ServeSpy` (explain_serve_tests/spy.rs — explicit DELEGATING overrides recorded as allowed reads, never panicking), `PartialSpy` and `M2Spy` (orient_serve/tests.rs), `FakeAgentStorage` (agent/tests/common/mod.rs — served from new fixture maps) and `FakeStorage` (agent/src/aggregators/complexity_tests.rs — `Ok(Vec::new())` with a comment naming why the double has none).
   - `list_members_of_type(snapshot_uid, qualified_name) -> Result<Vec<AgentMemberEntry>, AgentStorageError>`; `AgentMemberEntry { name, qualified_name, subtype: Option<String>, file: String, line_start: Option<u64>, forward_decl: bool }`. SQL over `nodes ⋈ files` in the snapshot: `kind = 'SYMBOL'` and `qualified_name` is `<qn>::<name>` or `<qn>.<name>` where `<name>` contains neither `::` nor `.` (direct members only — `BaseHandler.load_middleware.get_response` is excluded; `A::Inner` is included, `A::Inner::m3` is not); `forward_decl` = `json_extract(metadata_json,'$.forward_decl')` read as the tri-state `count_symbol_definitions_by_name` already reads (:1086-1108): absent → false, `true` → true, unreadable → an `AgentStorageError`, never a default. Order: rows whose file is the focus's file first by `line_start`, then other files by path then line (an out-of-line C++ definition such as `leveldb::DBImpl::Recover` at db/db_impl.cc:292 follows the header's declarations).
   - `find_file_importers(snapshot_uid, file_path) -> Result<Vec<AgentFileImporter>, AgentStorageError>`; `AgentFileImporter { file: String, module_path: Option<String> }`: the inverse of `find_file_imports` — IMPORTS edges whose TARGET is the FILE node of `file_path`, projecting the SOURCE file's path, distinct, joined to its owning module by the same `OWNS`-edge join `find_symbol_callers` uses (:1177 → `module_path`), ordered by path.
2. **`explain_symbol` (mod.rs) pushes two additive signals when `is_type_subtype(context.subtype)` (:324-326: CLASS|STRUCT|ENUM|INTERFACE|TRAIT)**, after the callees block (:557) and before the module-context block (:559): `Signal::explain_members(ExplainMembersEvidence { count, items, items_truncated, items_omitted_count })` with `ExplainMemberItem { name, subtype, file, line, forward_decl }` (count = the pre-truncation total; items capped by the existing `items_cap(budget)` :39-46 via `truncate_items`), and `Signal::explain_referenced_by(ExplainReferencedByEvidence { count, top_modules: Vec<ModuleCountEvidence>, items, items_truncated, items_omitted_count })` with `ExplainReferencedByItem { file, module }` (count = distinct importing files; `top_modules` via the existing `group_by_module` :1015-1028, `TOP_MODULES_N = 3`, on `AgentFileImporter.module_path`). New `SignalCode::ExplainMembers` / `ExplainReferencedBy` registered in every exhaustive arm (`as_str` :319-329, `tier_priority` :370-380 — take the ordinals right after `ExplainCallees` and renumber the tail; `descriptor` :393-425 as `(Explain, Low)`; `SignalEvidence` :1030-1040 + `Serialize` :1063-1073 + `variant_name` :1102-1112), constructors in the explain block (:1676ff, `SourceRef::ExplainPipeline`), re-exports in `agent/src/lib.rs:118-136`. Coherence needs NO change: `explain/coherent.rs::base_source` (:162-174) defaults every non-LG-first code to `{sqlite}`, which is the truth for these two reads.
3. **Renderer (`rgr/src/presentation/explain_sections.rs` :164-185 + `explain.rs`).** Two new arms. `Members (N)` then rows `  - <name> (<subtype>)  <path>:<line>` (a `forward_decl: true` item renders `(<subtype>, decl)`; the anchor goes through the shared `anchor()` chokepoint — absent or 0 line ⇒ bare row, never `:0`); default view shows 15 rows then `  ... (K more)` (the existing overflow literal), `--full` shows all. `Referenced by (N files)` (always the word `files`, even for 1), then `  top modules: <m1> (<c1>), <m2> (<c2>), <m3> (<c3>)` from `top_modules`, then file rows `  - <path>` capped at 15 with the same overflow line. Malformed evidence (non-string name, non-integer line, non-array items) renders `members unreadable on this snapshot` / `referenced-by unreadable on this snapshot` — never a fabricated or silently shorter list (the Q4 rule). **Type zero-line, renderer-side, no wire change:** when the identity's `subtype` (already on the wire, `ExplainIdentityEvidence.subtype`; add the accessor beside `get_identity_info` :368-390) is a type subtype and the Callers/Callees count is 0, the header line renders `Callers (0) — a type is not called; see Members / Referenced by` (same for Callees); every other subtype renders `Callers (0)` exactly as today.
4. **Nodes-free-on-green, amended for these two reads exactly as ruling B amended it for resolution:** the decorator delegates both to SQLite unconditionally; the spy records them as allowed reads; `type_focus_members_and_referenced_by_are_sqlite_delegated_on_green` proves the section is NON-EMPTY through the decorator on the green fixture (a defaulted empty read would pass the spy silently — that is why the methods are required and the test asserts non-emptiness). Recorded here as the choice §3 of the 2026-09-12 spec asked to state.
5. **Nothing else moves:** the header, Import-cycles (Q4), Trust and every non-type focus are byte-identical (ETS-C07/C11); no schema, no stable-key, no exit-code, no resolution change; bases/derived are omitted (no section, no sentence) until increment 2.

### 2.2 Evidence taxonomy (one row = one bound test)

| Input | Outcome | Bound test / check |
|---|---|---|
| store: `A::m1`, `A::m2`, `A::Inner`, `A::Inner::m3` for focus `A` | m1, m2, Inner — never m3 | `list_members_of_type_returns_direct_members_only` (ETS-C01) |
| members in the focus file and in another file | focus file first by line, then other files by path, line | `list_members_of_type_orders_focus_file_first_then_other_files_by_path_and_line` (ETS-C01) |
| member node metadata `forward_decl: true` / absent / unreadable | true / false / `AgentStorageError` | `list_members_of_type_marks_forward_declarations` (ETS-C01) |
| IMPORTS edges into the file's FILE node from two files in two modules / none | two importers with module_path / empty | `find_file_importers_lists_importing_files_with_owning_module`, `…is_empty_for_a_file_nobody_imports` (ETS-C01) |
| CLASS focus on the fake | EXPLAIN_MEMBERS + EXPLAIN_REFERENCED_BY present with the fake's rows | `explain_type_focus_emits_members_and_referenced_by` (ETS-C02) |
| FUNCTION focus | neither code; signal set byte-identical | `explain_function_focus_emits_no_members_or_referenced_by` (ETS-C02); ETS-C07 |
| 20 members under Budget::Small | count 20, items capped, truncation fields set | `explain_type_focus_members_count_is_the_pre_truncation_total` (ETS-C02); ETS-C10 |
| render: item with line / without line / line 0 / `forward_decl` | `name (subtype)  path:line` / bare / bare / `(subtype, decl)` | `explain_members_section_anchors_file_line_and_marks_decl`, `…omits_absent_line_never_zero` (ETS-C03) |
| render: 16 items, default vs `--full` | 15 rows + `... (1 more)` / all rows | `explain_members_over_cap_renders_more_line_and_full_uncaps` (ETS-C03) |
| render: referenced-by evidence | `Referenced by (N files)`, `top modules:` line, capped file rows | `explain_referenced_by_section_names_count_top_modules_and_files` (ETS-C03) |
| identity subtype CLASS, callers 0 / subtype FUNCTION, callers 0 | the type zero-line / `Callers (0)` unchanged | `explain_type_zero_callers_line_says_a_type_is_not_called`, `explain_function_zero_callers_line_is_unchanged` (ETS-C03) |
| malformed members/referenced-by evidence | `members unreadable on this snapshot` / `referenced-by unreadable on this snapshot`, no partial list | `explain_members_rows_render_only_from_carried_evidence` (ETS-C03) |
| green LiveGraph + M-2 decorator + spy, CLASS focus | non-empty Members/Referenced-by equal to the bare SQLite serve; spy records the reads | `type_focus_members_and_referenced_by_are_sqlite_delegated_on_green` (ETS-C04) |

### 2.3 Outward proof (what a user of the product gains)

vcmi `explain CGHeroInstance`: `Members (135)` anchored at `lib/mapObjects/CGHeroInstance.h:<line>` (header members first, then the definitions in lib/mapObjects/CGHeroInstance.cpp — `getFactionID (METHOD)  lib/mapObjects/CGHeroInstance.cpp:81` under `--full`), `Referenced by (178 files)` with its top modules, `Callers (0) — a type is not called; see Members / Referenced by` — where today it says nothing about the class. leveldb `explain leveldb::DBImpl`: `Members (73)` (declarations in db/db_impl.h, definitions in db/db_impl.cc such as `Recover (METHOD)  db/db_impl.cc:292`), `Referenced by (9 files)`. django `explain BaseHandler`: `Members (10)` (`load_middleware (METHOD)  django/core/handlers/base.py:27`, …; no nested closures), `Referenced by (1 files)`. FRAKTAG `explain ConversationManager` BY NAME: `Members (8)` (`createSession (METHOD)  packages/engine/src/core/ConversationManager.ts:71`, …). `explain leveldb::DBImpl::Recover` byte-identical.

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-005-L05, P-ETS-02 | resolution routing; the spy's six (b) methods; parity certificates | ETS-C02 (`explain_resolves_qualified_suffix_through_shared_resolver`); ETS-C04 |
| RG-REQ-005-L06 | confidence beside no-match | ETS-C03 (`no_high_confidence_beside_no_match`) |
| RG-REQ-003-L01 | the cycles block | ETS-C03 (`explain_cycles_absent_walk_renders_unordered_no_arrows`); ETS-C07; ETS-C11 |
| RG-REQ-010-L01 | seed candidates | ETS-C02; ETS-C03 (`semantic_no_match_renders_labeled_candidates_in_human_mode`) |
| RG-REQ-012-L07 | an invented line | ETS-C03 (tier tests + the members tests); ETS-C06 |
| RG-REQ-012-L04 | a count that changes with budget | ETS-C02; ETS-C10 |
| RG-REQ-001-L04 | schema / stable-key change | ETS-C01 (no new column; `qualified_name` only) |
| P-ETS-01 | a FUNCTION/METHOD focus moves | ETS-C02; ETS-C07 |
| P-ETS-03 | header / cycles / trust on a type focus move | ETS-C11 |
| P-ETS-04 | any existing test | ETS-C05 |
| RG-REQ-002-L08 | an internal label in the new sentences | ETS-C13 |
| RG-REQ-011-L06, P-ETS-05 | isolation | ETS-C12 |

## 4. Stop conditions

Frozen: wire shapes other than the two additive signals, storage schema (no new column — the member query uses `qualified_name`), exit codes, the resolution routing (`resolve_symbol` delegated, `resolve_symbol_name` name-only), the six LiveGraph-served (b) methods and the parity certificates, `explain_file`'s sections, the cycles block, orient. No bases/derived section or sentence in this increment. No defaulted body for the two new port methods. Nothing outside the fifteen candidate paths (another implementor of `AgentStorageRead` that the compiler names is a FINDING — stop and report it, do not edit). STANDING HONESTY RULES (unreadable metadata is an error; a malformed row is `unreadable`, never dropped). Unmet DoD → STOP. Do NOT commit. Run every check to completion in the FOREGROUND; end your turn only with the evidence object.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

0. On the CLEAN tree: record `git rev-parse HEAD`; `(cd rust && cargo build --release --bin rmap --bin rmapd)` (a no-op on the warm cache) → `mkdir -p /private/tmp/EXPLAIN-TYPE-SECTIONS-1-before && cp rust/target/release/rmap rust/target/release/rmapd /private/tmp/EXPLAIN-TYPE-SECTIONS-1-before/`; `registry-before <digest>` in build-progress.md; verify the four roots exist (§7; rebuild a missing one per §7 and say so).
1. Failing tests FIRST (the §2.2 names): storage reads → port trait + the seven implementors → signals/DTOs → `explain_symbol` → renderer sections + zero-line → the delegation test.
2. Chunked gates ETS-C01 → C02 → C03 → C04, then ETS-C05 (whole suites, foreground) — NEVER `cargo test --workspace`.
3. `(cd rust && cargo build --release --bin rmap --bin rmapd)` (candidate) → live proofs ETS-C06 → C07 → C08 → C09 → C10 → C11 on the pre-provisioned roots, both binaries on the SAME root; every `/tmp/ets-*` capture from THIS cycle's run.
4. ETS-C12 cleanup (remove the before dir; the four roots STAY), ETS-C13 hygiene, hand-off with the evidence object (each check's outcome NESTED under `outcome`; `supportingEvidence` non-empty).

## 6. Definition of done

All thirteen checks pass; §2.3 holds on vcmi, leveldb, django, FRAKTAG; `explain leveldb::DBImpl::Recover` byte-identical; the report carries the code-under-analysis examples (three real member declarations with file:line per corpus, e.g. vcmi `lib/mapObjects/CGHeroInstance.h:176` (declaration) / `lib/mapObjects/CGHeroInstance.cpp:81` (the stored definition node), django `django/core/handlers/base.py:27`, FRAKTAG `packages/engine/src/core/ConversationManager.ts:71`, leveldb `db/db_impl.cc:292`; three real `#include`/`import` sites that make a Referenced-by row, quoted from the source with file:line).

## 7. Corpus roots (pre-provisioned by the manager on 2026-09-20 with the base-revision binary; rebuild recipe)

- `/private/tmp/EXPLAIN-TYPE-SECTIONS-1-leveldb`: fresh index of `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb` (`rmap index`, 1 s, stdio, auto passes off). `/private/tmp/EXPLAIN-TYPE-SECTIONS-1-django`: fresh index of `…/legacy-codebases/django` (22 s).
- `/private/tmp/EXPLAIN-TYPE-SECTIONS-1-fraktag`: `databases/e444686289ee2bf9.db` copied from `~/repo-graph-retained/audit-v0.18.0/databases/` + `registry.json` = the retained registry filtered to the FRAKTAG entry with `db_path` rewritten. `/private/tmp/EXPLAIN-TYPE-SECTIONS-1-vcmi`: `databases/c956217ff5c253d9.db` (412 MB; snapshot …/2834d519, the audit's) the same way.
- The retained root itself is NEVER served (a serving daemon writes into it — lesson 2026-09-07). The operator registry is read only for its digest.

## 8. Follow-ups (not this slice)

Increment 2 (C++ ANCHOR: `extract_base_clause` emits the IMPLEMENTS edge from the type node — extractor.rs:1387; bases/derived sections; the IMPLEMENTS shape histogram loses SOURCE→CLASS; consumers keyed on the FILE anchor re-checked) and increment 3 (MACRO RECOVERY: base tokens inside ERROR nodes on `class DLL_LINKAGE X : …`, extractor.rs:797-799). The unresolved-call count beside a zero (RG-REQ-005-L09 in full — no half is done in this slice; deferred to CLAIM-INVARIANT-1). A `parent_node_uid`-based containment (today None in every extractor; containment rides in `qualified_name`).
