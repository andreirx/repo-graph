<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "TRUST-MODULE-EDGES-1",
  "baselinePath": "docs/requirements/baselines/TRUST-MODULE-EDGES-1-INPUT-5.json",
  "parentRequirementIds": [
    "RG-REQ-001",
    "RG-REQ-002",
    "RG-REQ-003",
    "RG-REQ-004",
    "RG-REQ-009",
    "RG-REQ-011",
    "RG-REQ-012"
  ],
  "implements": [
    "RG-REQ-002-L02",
    "RG-REQ-004-L01",
    "RG-REQ-009-L02",
    "RG-REQ-009-L04"
  ],
  "preserves": [
    "RG-REQ-001-L03",
    "RG-REQ-002-L03",
    "RG-REQ-002-L08",
    "RG-REQ-003-L03",
    "RG-REQ-004-L02",
    "RG-REQ-004-L03",
    "RG-REQ-004-L04",
    "RG-REQ-004-L09",
    "RG-REQ-004-L10",
    "RG-REQ-004-L11",
    "RG-REQ-009-L03",
    "RG-REQ-009-L06",
    "RG-REQ-009-L07",
    "RG-REQ-011-L06",
    "RG-REQ-012-L01"
  ],
  "preservationObligationIds": [
    "P-TME-01",
    "P-TME-02",
    "P-TME-03"
  ],
  "changes": [],
  "acceptanceBoundary": "The trust and modules-deps outputs of the freshly built rmap on isolated throwaway copies of the retained stores (~/repo-graph-retained/audit-v0.18.0/{repo-graph,kafka,hadoop,FRAKTAG,vcmi}), plus cargo test -p repo-graph-storage -p repo-graph-trust -p repo-graph-rgr --lib presentation::trust::tests -p repo-graph-agent.",
  "candidatePaths": [
    "rust/crates/storage/src/trust_impl.rs",
    "rust/crates/trust/src/rules.rs",
    "rust/crates/rgr/src/presentation/trust_tests.rs",
    "rust/crates/rgr/src/presentation/trust.rs",
    "rust/crates/rgr/tests/trust_module_edges_seam.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/TRUST-MODULE-EDGES-1/verification.json",
    "docs/assurance/TRUST-MODULE-EDGES-1/implementation-review.json"
  ],
  "candidateExclusions": [
    { "pathPrefix": ".agent-manager/", "reason": "local relay state and raw run/progress evidence" },
    { "pathPrefix": "rust/target/", "reason": "reproducible Cargo build output" }
  ],
  "checks": [
    {
      "checkId": "TME-C01",
      "obligationIds": ["RG-REQ-002-L02", "RG-REQ-009-L02", "RG-REQ-004-L01"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --test trust_module_edges_seam", "cwd": "rust", "environment": "cargo workspace under rust/ with the compute_module_stats fan-subquery change applied", "inputs": "the write-first seam integration test rust/crates/rgr/tests/trust_module_edges_seam.rs; the two-crate and twin-names module fixtures" },
      "expected": "exit 0; the write-first seam test (write-first; proposed name trust_module_fans_equal_modules_deps) is GREEN and asserts that trust --json modules[].fan_in/fan_out equal modules deps per-module fan counts for every rendered module on both the two-crate and the twin-names fixtures, so a module with rendered edges can never be flagged zero-connectivity; a divergence on either fixture fails the check."
    },
    {
      "checkId": "TME-C02",
      "obligationIds": ["RG-REQ-004-L01", "RG-REQ-009-L02"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-storage", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "storage/src/trust_impl.rs compute_module_stats unit tests (compute_module_stats_returns_fan_in_fan_out_file_count, compute_module_stats_excludes_modules_with_no_owned_files), updated to the re-sourced fan counts" },
      "expected": "exit 0; TrustStorageRead::compute_module_stats derives fan_in as COUNT(DISTINCT source module candidate) and fan_out as COUNT(DISTINCT target module candidate) over resolved file-to-file IMPORTS aggregated through module_file_ownership to module candidates on both endpoints (the same set modules deps renders), with the nodes kind='MODULE' join removed and file_count still taken from ownership; the two named unit tests assert nonzero fans for a connected candidate and are GREEN. A prefix-LIKE bridge (double-counting nested candidates) fails the intent."
    },
    {
      "checkId": "TME-C03",
      "obligationIds": ["RG-REQ-009-L04", "RG-REQ-009-L02"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-trust --lib count_suspicious_modules_matches_all_criteria && cargo test -p repo-graph-rgr --lib presentation::trust::tests::suspicious_modules_basis_in_reader_frame_no_internal_wording", "cwd": "rust", "environment": "cargo workspace under rust/ with the two defect-pinning tests rewritten", "inputs": "trust/src/rules.rs::count_suspicious_modules_matches_all_criteria (a repo-graph-trust crate lib test, NOT under presentation::trust::tests) and rgr/src/presentation/trust_tests.rs::suspicious_modules_basis_in_reader_frame_no_internal_wording, rewritten to the true behaviour" },
      "expected": "exit 0 requires BOTH invocations GREEN (the && fails on either). The first runs count_suspicious_modules_matches_all_criteria in the repo-graph-trust crate as its own invocation (a trailing `presentation::trust::tests` filter would exclude it, since it lives in rules.rs, not that rgr module) and asserts a module with rendered edges is never flagged suspicious_zero_connectivity. The second runs suspicious_modules_basis_in_reader_frame_no_internal_wording (renamed from suspicious_modules_state_basis_and_point_at_stats per D-TME-TEST-NAME-1, because the rewritten assertions are the opposite of the old name; trust.rs declares trust_tests.rs as `#[path = \"trust_tests.rs\"] mod tests`, so its filter path is presentation::trust::tests::*; this exact-function filter selects 1 test), rewritten so the false 'cross-check stats' basis sentence is replaced by a true one or removed. The live alias_resolution_suspicion behaviour (the downgrade persists where >= 3 genuinely zero-connectivity candidates remain, per D-TME-MOVEMENT-1) is asserted by TME-C05/TME-C06, not by this unit check. Either test failing fails the check."
    },
    {
      "checkId": "TME-C04",
      "obligationIds": ["RG-REQ-009-L02", "RG-REQ-004-L01"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-agent", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "the repo-graph-agent check/orient reliability paths that consume trust module stats" },
      "expected": "exit 0; the agent crate's trust-consuming paths (check reduce, orient reliability) stay green with the re-sourced module fan counts; any newly failing agent test is a regression to report, not to suppress."
    },
    {
      "checkId": "TME-C05",
      "obligationIds": ["RG-REQ-009-L02", "RG-REQ-004-L01", "RG-REQ-002-L02", "RG-REQ-009-L04"],
      "owner": "builder",
      "method": { "kind": "command", "command": "export RMAP_STATE_ROOT=/private/tmp/TRUST-MODULE-EDGES-1-rg RMAP_SOCKET_PATH=/private/tmp/TRUST-MODULE-EDGES-1-rg/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && rust/target/release/rmap trust > /tmp/tme-trust.txt && grep -q 'Suspicious Modules (zero connectivity)' /tmp/tme-trust.txt && grep -Eq 'Import-graph: LOW \\(alias resolution suspected; [0-9]+ unresolved imports\\)' /tmp/tme-trust.txt && grep -Fq 'Change-impact: LOW (alias resolution suspected)' /tmp/tme-trust.txt && rust/target/release/rmap trust --json > /tmp/tme-trustj.json && python3 -c \"import json,sys\nd=json.load(open('/tmp/tme-trustj.json'))\nar=False; at=False\nst=[(None,d)]\nwhile st:\n key,o=st.pop()\n if isinstance(o,dict):\n  r=o.get('reasons')\n  if isinstance(r,list) and 'alias_resolution_suspicion' in r: ar=True\n  if key=='alias_resolution_suspicion' and o.get('triggered') is True: at=True\n  for k,v in o.items(): st.append((k,v))\n elif isinstance(o,list):\n  for x in o: st.append((key,x))\nsys.exit(0 if (ar and at) else 1)\"", "cwd": ".", "environment": "freshly built candidate rmap at rust/target/release/rmap; /private/tmp/TRUST-MODULE-EDGES-1-rg is an isolated throwaway COPY of ~/repo-graph-retained/audit-v0.18.0/repo-graph (copied, never the original) served with RMAP_TRANSPORT=stdio and auto passes off; operator real registry untouched", "inputs": "the copied repo-graph snapshot store; the captured human `rmap trust` text and the captured `rmap trust --json` document" },
      "expected": "exit 0 requires every && condition to hold. Corrected to the measured truth (decision D-TME-MOVEMENT-1): on the candidate's HUMAN `rmap trust` output for the repo-graph store copy (reader frame, not machine tokens — the axis reasons are humanized by agent::reliability::humanize_reason), `grep -q` proves the 'Suspicious Modules (zero connectivity)' section IS present — it now lists EXACTLY the six GENUINELY zero-connectivity module candidates (the `rust/` umbrella directory, `tools`, `scripts`, two test-fixture directories, and the leaf crates `detectors` and `git`), down from 48, with no crate that `modules deps` renders an edge for appearing (the exact six-candidate enumeration and the per-module `trust --json modules[]` fan equality to `modules deps` are the named-record inspection TME-C06); `grep -E` proves the reader line 'Import-graph: LOW (alias resolution suspected; <n> unresolved imports)' is present and `grep -F` proves the reader line 'Change-impact: LOW (alias resolution suspected)' is present — the humanized alias-suspicion phrase (not the machine token), which PERSISTS because six genuinely-disconnected candidates remain and `detect_alias_resolution_suspicion` fires at >= 3 (its inability to distinguish unresolved imports from genuine isolation is the separate root cause ALIAS-SUSPICION-1, out of scope by D-TME-MOVEMENT-1). Then the MACHINE reason is verified in `rmap trust --json`: the python3 walk recurses the whole envelope (robust to nesting) and exits 0 only when some reliability-axis `reasons` array contains the machine token 'alias_resolution_suspicion' AND some object sitting under the key 'alias_resolution_suspicion' has \"triggered\": true (the key itself is always present in triggered_downgrades, so a substring search would be wrong; the walk distinguishes the ever-present key from a genuine trigger). A missing Suspicious section, a connected crate appearing in it, an absent/changed humanized Import-graph or Change-impact reader line, or an absent alias reason/trigger in JSON flips a grep or the python exit and the chain exits non-zero. (Per-module fan equality to modules deps and the exact six-candidate enumeration are the named-record inspection TME-C06.)"
    },
    {
      "checkId": "TME-C06",
      "obligationIds": ["RG-REQ-009-L02", "RG-REQ-004-L01"],
      "owner": "builder",
      "method": { "kind": "inspection", "subject": "the candidate rmap's per-module fan counts and triggered downgrades on isolated throwaway COPIES of the retained repo-graph, kafka, hadoop, FRAKTAG and vcmi stores (~/repo-graph-retained/audit-v0.18.0/*, copied never opened)", "criterion": "corrected to the measured truth (decision D-TME-MOVEMENT-1): on the repo-graph copy, every module in the captured `rmap trust --json` modules[] has fan_in and fan_out equal to that module's per-module counts in the captured `rmap modules deps --json` (records /tmp/tme-trust-repo-graph.json and /tmp/tme-deps-repo-graph.json), the count of module candidates with fan_in>0 OR fan_out>0 is 53 of 61, and the 'Suspicious Modules (zero connectivity)' section lists exactly the six genuinely-disconnected candidates (the `rust/` umbrella directory, `tools`, `scripts`, two test-fixture directories, and the leaf crates `detectors` and `git`) with no crate that `modules deps` renders an edge for; on the kafka copy rows-with-fan>0 = 60 of 65, on the hadoop copy 6 of 9, on the FRAKTAG copy 0 of 4 (FRAKTAG has zero cross-module resolved imports — 185 intra of 210 — so 0 connected is the TRUE value, not a defect), on the vcmi copy 13 of 16; the `alias_resolution_suspicion` downgrade is triggered on the repo-graph, kafka and FRAKTAG copies (>= 3 genuinely-disconnected candidates each: 6, 5, 3) and is ABSENT on the hadoop and vcmi copies only (2 each, below the >= 3 threshold); no verdict LEVEL is asserted to change beyond this reason movement, and any observed level change is reported as a finding rather than tuned away", "inputs": "the five throwaway store copies; the per-repo captured `rmap trust --json` and `rmap modules deps --json` records" },
      "expected": "the inspection finds repo-graph trust fan_in/fan_out equal modules deps per module with 53 of 61 connected and the Suspicious section listing exactly the six genuinely-disconnected candidates (no crate with rendered edges appears); rows-with-fan>0 = kafka 60/65, hadoop 6/9, FRAKTAG 0/4 (true zero — genuinely zero cross-module resolved imports), vcmi 13/16; and the alias downgrade present on repo-graph/kafka/FRAKTAG (>= 3 remain) and absent on hadoop/vcmi only (2 each). A per-module fan mismatch on repo-graph, a wrong connected count on any repo, a connected crate appearing in repo-graph's Suspicious section, or a wrong alias-downgrade presence pattern (absent where >= 3 remain, or present where fewer remain) fails the check."
    },
    {
      "checkId": "TME-C07",
      "obligationIds": ["RG-REQ-004-L02", "RG-REQ-004-L03", "RG-REQ-004-L04", "RG-REQ-004-L09", "RG-REQ-004-L10"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --lib presentation::modules_list::tests", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "modules_list_tests.rs (including two_crate_fixture_renders_a_to_b_edge_verbatim and modules_list_rendered_file_totals_sum_to_the_check_indexed_basis)" },
      "expected": "exit 0; presentation::modules_list::tests is GREEN (modules_list.rs declares modules_list_tests.rs as `#[path = \"modules_list_tests.rs\"] mod tests`, so the module filter path is presentation::modules_list::tests; it selects 57 tests). modules list/deps already read classification::module_edges::derive_module_dependency_edges (the correct set), so this slice must not move them — the unit-level no-behavior-change proof for RG-REQ-004-L02/L03/L04/L09/L10; the byte-level proof is TME-C07B."
    },
    {
      "checkId": "TME-C07B",
      "obligationIds": ["RG-REQ-004-L02", "RG-REQ-004-L03", "RG-REQ-004-L04", "RG-REQ-004-L09", "RG-REQ-004-L10"],
      "owner": "builder",
      "method": { "kind": "command", "command": "export RMAP_STATE_ROOT=/private/tmp/TRUST-MODULE-EDGES-1-rg RMAP_SOCKET_PATH=/private/tmp/TRUST-MODULE-EDGES-1-rg/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && BEFORE=/private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap && AFTER=rust/target/release/rmap && \"$BEFORE\" modules list > /tmp/tme-modlist-before.txt && \"$AFTER\" modules list > /tmp/tme-modlist-after.txt && cmp /tmp/tme-modlist-before.txt /tmp/tme-modlist-after.txt && \"$BEFORE\" modules deps > /tmp/tme-deps-before.txt && \"$AFTER\" modules deps > /tmp/tme-deps-after.txt && cmp /tmp/tme-deps-before.txt /tmp/tme-deps-after.txt && \"$BEFORE\" modules violations > /tmp/tme-viol-before.txt && \"$AFTER\" modules violations > /tmp/tme-viol-after.txt && cmp /tmp/tme-viol-before.txt /tmp/tme-viol-after.txt", "cwd": ".", "environment": "candidate rmap at rust/target/release/rmap; before-binary at /private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap built by `git worktree add /private/tmp/TRUST-MODULE-EDGES-1-before HEAD` (base revision f8a0e6b) then `cargo build --release`; /private/tmp/TRUST-MODULE-EDGES-1-rg is the isolated repo-graph store copy; operator registry untouched", "inputs": "the isolated repo-graph store copy; captured modules list/deps/violations output before and after" },
      "expected": "each of the three cmp invocations exits 0, i.e. byte-identical `modules list`, `modules deps` and `modules violations` human output between the base-revision before-binary and the candidate — the byte-level no-behavior-change proof for RG-REQ-004-L02/L03/L04/L09/L10. Any cmp difference exits non-zero and fails the check."
    },
    {
      "checkId": "TME-C08",
      "obligationIds": ["RG-REQ-009-L03", "RG-REQ-009-L06", "RG-REQ-009-L07"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --lib presentation::trust::tests", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "trust_tests.rs resolution family (render_resolution_is_reader_frame_in_scope) and first-party family (first_party_workspace_crate_renders_internal_not_library_call)" },
      "expected": "exit 0; presentation::trust::tests (trust.rs declares trust_tests.rs as `#[path = \"trust_tests.rs\"] mod tests`, so the module filter path is presentation::trust::tests; it selects 37 tests, including render_resolution_is_reader_frame_in_scope and first_party_workspace_crate_renders_internal_not_library_call) is GREEN — the unit-level proof that call-resolution %, reader-frame resolution and first-party classification are unchanged (no-behavior-change for RG-REQ-009-L03/L06/L07); the byte-level 'calls N% resolved' proof is TME-C08B."
    },
    {
      "checkId": "TME-C08B",
      "obligationIds": ["RG-REQ-009-L03", "RG-REQ-009-L06", "RG-REQ-009-L07"],
      "owner": "builder",
      "method": { "kind": "command", "command": "export RMAP_STATE_ROOT=/private/tmp/TRUST-MODULE-EDGES-1-rg RMAP_SOCKET_PATH=/private/tmp/TRUST-MODULE-EDGES-1-rg/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && BEFORE=/private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap && AFTER=rust/target/release/rmap && \"$BEFORE\" trust > /tmp/tme-trust-before.txt && \"$AFTER\" trust > /tmp/tme-trust-after.txt && grep -iE 'calls [0-9]+% resolved' /tmp/tme-trust-before.txt > /tmp/tme-cr-before.txt && grep -iE 'calls [0-9]+% resolved' /tmp/tme-trust-after.txt > /tmp/tme-cr-after.txt && cmp /tmp/tme-cr-before.txt /tmp/tme-cr-after.txt", "cwd": ".", "environment": "candidate rmap at rust/target/release/rmap; before-binary at /private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap (git worktree of base revision f8a0e6b, cargo build --release); /private/tmp/TRUST-MODULE-EDGES-1-rg is the isolated repo-graph store copy; operator registry untouched", "inputs": "the isolated repo-graph store copy; the trust 'calls N% resolved' line captured before and after" },
      "expected": "the two `grep -iE 'calls [0-9]+% resolved'` extractions each match at least one line (a missing line makes grep exit non-zero and fails the chain) and cmp exits 0 — the trust 'calls N% resolved' line is byte-identical between base-revision before-binary and candidate, proving the fix touches module fan counts only (no-behavior-change for RG-REQ-009-L03/L06/L07)."
    },
    {
      "checkId": "TME-C09",
      "obligationIds": ["RG-REQ-009-L04"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-trust --lib coherent_tests && cargo test -p repo-graph-rgr --lib presentation::trust::tests::render_headline_is_the_snapshot_posture", "cwd": "rust", "environment": "cargo workspace under rust/", "inputs": "trust/src/coherent_tests.rs (all) and trust_tests.rs::render_headline_is_the_snapshot_posture" },
      "expected": "exit 0 requires BOTH invocations GREEN (the && fails on either): all coherent_tests pass and presentation::trust::tests::render_headline_is_the_snapshot_posture (the rgr filter path; trust_tests.rs is mounted as `mod tests`; this exact-function filter selects 1 test) is GREEN, so the AnswerEnvelope invariants and the snapshot headline posture are unchanged and only the downgrade REASON lines move (that reason movement is asserted by TME-C03) — the implemented part of RG-REQ-009-L04 changes reasons without disturbing the envelope."
    },
    {
      "checkId": "TME-C10",
      "obligationIds": ["P-TME-01"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-trust --lib coherent_tests::root_never_exceeds_the_weakest_leaf", "cwd": "rust", "environment": "cargo workspace under rust/", "inputs": "trust/src/coherent_tests.rs::root_never_exceeds_the_weakest_leaf and its siblings" },
      "expected": "exit 0; root_never_exceeds_the_weakest_leaf is GREEN — the trust root posture computed as MEET over the weakest leaf in JSON is byte-unchanged (D-T6 preserved; no-behavior-change)."
    },
    {
      "checkId": "TME-C11",
      "obligationIds": ["RG-REQ-002-L08"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --lib presentation::trust::tests::render_omits_raw_pipeline_diagnostic_sections_from_human_surface", "cwd": "rust", "environment": "cargo workspace under rust/ with the replacement basis sentence in place", "inputs": "the replaced 'cross-check stats' basis sentence and render_omits_raw_pipeline_diagnostic_sections_from_human_surface" },
      "expected": "exit 0; presentation::trust::tests::render_omits_raw_pipeline_diagnostic_sections_from_human_surface (the rgr filter path; trust_tests.rs is mounted as `mod tests`; this exact-function filter selects 1 test) is GREEN — the replacement basis sentence describes the reader's subject and leaks no internal pipeline diagnostic (no node-identity, SQL-join or serving-engine wording), preserving RG-REQ-002-L08."
    },
    {
      "checkId": "TME-C12",
      "obligationIds": ["P-TME-02"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-trust --lib overlay && cargo test -p repo-graph-rgr --test dead_command", "cwd": "rust", "environment": "cargo workspace under rust/", "inputs": "trust/src/overlay.rs tests (overlay_from_report_with_no_degradation and siblings) and rgr/tests/dead_command.rs (dead_command_is_disabled, dead_exact_results)" },
      "expected": "exit 0 requires BOTH invocations GREEN (the && fails on either): the trust overlay tests and the dead_command tests are GREEN — dead's unresolved_import_pressure input and the overlay level (LOW) are unchanged because this slice does not touch unresolved-import pressure (P-TME-02 preserved)."
    },
    {
      "checkId": "TME-C13",
      "obligationIds": ["P-TME-03", "RG-REQ-004-L11"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --lib presentation::orient::tests::budget_trades_depth_small_subset_of_full && cargo test -p repo-graph-storage --lib compute_module_stats_symbol_count_is_all_symbols_not_exports", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "orient_tests.rs::budget_trades_depth_small_subset_of_full (orient.rs mounts orient_tests.rs as `#[path = \"orient_tests.rs\"] mod tests`, so the filter path is presentation::orient::tests::*; this exact-function filter selects 1 test — it renders orient at --full and asserts the `--full` Degradation block's `Import-graph reliability is LOW` axis line, exercising orient_reliability.rs::render_degradation → format_reliability_axis → the shared reliability::humanize_reason that inherits this slice's reason-wording change); queries.rs::compute_module_stats_symbol_count_is_all_symbols_not_exports (the STATS directory-node population, out of scope L11)" },
      "expected": "exit 0 requires BOTH invocations GREEN (the && fails on either): budget_trades_depth_small_subset_of_full is GREEN — orient's --full Degradation section still renders the Import-graph reliability axis (that section renders the Import-graph/Change-impact axes through the shared reliability::humanize_reason, the exact path that inherits this slice's reason-wording change), proving orient reliability rendering is not broken and inherits ONLY the section-0 wording change; and compute_module_stats_symbol_count_is_all_symbols_not_exports is GREEN (the stats directory-node population is unchanged). The prior `presentation::orient_reliability` module filter selected ZERO tests (orient_reliability.rs and orient_reliability_caveats.rs carry no test module — verified by source) and is replaced by this non-empty oracle. The byte-level stats proof is TME-C13B (P-TME-03 preserved; RG-REQ-004-L11 out of scope)."
    },
    {
      "checkId": "TME-C13B",
      "obligationIds": ["P-TME-03", "RG-REQ-004-L11"],
      "owner": "builder",
      "method": { "kind": "command", "command": "export RMAP_STATE_ROOT=/private/tmp/TRUST-MODULE-EDGES-1-rg RMAP_SOCKET_PATH=/private/tmp/TRUST-MODULE-EDGES-1-rg/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && BEFORE=/private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap && AFTER=rust/target/release/rmap && \"$BEFORE\" stats > /tmp/tme-stats-before.txt && \"$AFTER\" stats > /tmp/tme-stats-after.txt && cmp /tmp/tme-stats-before.txt /tmp/tme-stats-after.txt", "cwd": ".", "environment": "candidate rmap at rust/target/release/rmap; before-binary at /private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap (git worktree of base revision f8a0e6b, cargo build --release); /private/tmp/TRUST-MODULE-EDGES-1-rg is the isolated repo-graph store copy; operator registry untouched", "inputs": "the isolated repo-graph store copy; captured stats human output before and after" },
      "expected": "cmp exits 0 — the `stats` human output (its module rows come from queries.rs::compute_module_stats, the directory-node population that is RG-REQ-004-L11's identifier space) is byte-identical between base-revision before-binary and candidate, proving the grpc/stats identifier space is untouched (RG-REQ-004-L11 out of scope; P-TME-03 preserved). Any difference fails the check."
    },
    {
      "checkId": "TME-C14",
      "obligationIds": ["RG-REQ-003-L03", "RG-REQ-002-L03"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --lib presentation::orient::tests::header_renders_indexed_source_tracked_only_split", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "orient_tests.rs::header_renders_indexed_source_tracked_only_split" },
      "expected": "exit 0; presentation::orient::tests::header_renders_indexed_source_tracked_only_split (orient.rs declares orient_tests.rs as `#[path = \"orient_tests.rs\"] mod tests` — distinct from the sibling orient_density_tests.rs mounted as `mod density_tests` — so the filter path is presentation::orient::tests::*; this exact-function filter selects 1 test) is GREEN — the unit-level proof that the orient header still names its indexed/source/tracked-only inclusion universe (no-behavior-change for RG-REQ-003-L03 and RG-REQ-002-L03); the byte-level file/symbol-total proof is TME-C14B."
    },
    {
      "checkId": "TME-C14B",
      "obligationIds": ["RG-REQ-003-L03", "RG-REQ-002-L03"],
      "owner": "builder",
      "method": { "kind": "command", "command": "export RMAP_STATE_ROOT=/private/tmp/TRUST-MODULE-EDGES-1-rg RMAP_SOCKET_PATH=/private/tmp/TRUST-MODULE-EDGES-1-rg/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && BEFORE=/private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap && AFTER=rust/target/release/rmap && \"$BEFORE\" orient > /tmp/tme-orient-before.txt && \"$AFTER\" orient > /tmp/tme-orient-after.txt && grep -i 'files indexed' /tmp/tme-orient-before.txt > /tmp/tme-oh-before.txt && grep -i 'files indexed' /tmp/tme-orient-after.txt > /tmp/tme-oh-after.txt && cmp /tmp/tme-oh-before.txt /tmp/tme-oh-after.txt && \"$BEFORE\" stats --json > /tmp/tme-statsj-before.json && \"$AFTER\" stats --json > /tmp/tme-statsj-after.json && grep -Eo '\\\"indexed_file_count\\\"[[:space:]]*:[[:space:]]*[0-9]+' /tmp/tme-statsj-before.json > /tmp/tme-tf-before.txt && grep -Eo '\\\"indexed_file_count\\\"[[:space:]]*:[[:space:]]*[0-9]+' /tmp/tme-statsj-after.json > /tmp/tme-tf-after.txt && cmp /tmp/tme-tf-before.txt /tmp/tme-tf-after.txt", "cwd": ".", "environment": "candidate rmap at rust/target/release/rmap; before-binary at /private/tmp/TRUST-MODULE-EDGES-1-before/rust/target/release/rmap (git worktree of base revision f8a0e6b, cargo build --release); /private/tmp/TRUST-MODULE-EDGES-1-rg is the isolated repo-graph store copy; operator registry untouched", "inputs": "the isolated repo-graph store copy; the orient 'files indexed' header line and the stats --json indexed_file_count field captured before and after" },
      "expected": "both cmp invocations exit 0 (each grep must first match — a missing line/field makes grep exit non-zero and fails the chain): the orient 'files indexed' header line and the stats --json indexed_file_count count are byte-identical between base-revision before-binary and candidate, proving the fix changes no file total (no-behavior-change for RG-REQ-003-L03 and RG-REQ-002-L03). The indexed_file_count extraction uses an extended-regex whitespace-tolerant pattern ('\"indexed_file_count\"[[:space:]]*:[[:space:]]*[0-9]+') because graph.rs serializes stats --json with serde_json::to_string_pretty (a space follows the colon: \"indexed_file_count\": <n>), so a compact-form grep would never match the emitted field. Note the full orient body is NOT compared because its reliability caveat wording is an intended section-0 change."
    },
    {
      "checkId": "TME-C15",
      "obligationIds": ["RG-REQ-012-L01"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-rgr --test exit_code_contract", "cwd": "rust", "environment": "cargo workspace under rust/", "inputs": "rgr/tests/exit_code_contract.rs" },
      "expected": "exit 0; exit_code_contract passes — the process exit codes of trust, modules and stats are unchanged (no-behavior-change for RG-REQ-012-L01)."
    },
    {
      "checkId": "TME-C16",
      "obligationIds": ["RG-REQ-001-L03"],
      "owner": "builder",
      "method": { "kind": "command", "command": "cargo test -p repo-graph-storage --lib count_unresolved_by_classification_groups_correctly", "cwd": "rust", "environment": "cargo workspace under rust/ with the change applied", "inputs": "storage/src/trust_impl.rs::count_unresolved_by_classification_groups_correctly" },
      "expected": "exit 0; count_unresolved_by_classification_groups_correctly is GREEN — the unit-level proof that unresolved-edge classification grouping is unchanged (no-behavior-change for RG-REQ-001-L03); the stored-row preservation proof is TME-C16B."
    },
    {
      "checkId": "TME-C16B",
      "obligationIds": ["RG-REQ-001-L03"],
      "owner": "builder",
      "method": { "kind": "inspection", "subject": "the unresolved_edges rows and their per-classification counts in the isolated repo-graph store copy, read once under the base-revision before-binary and once under the candidate binary (compute_module_stats reads resolved IMPORTS only and never reads or writes unresolved_edges)", "criterion": "the total unresolved_edges row count and the per-classification group counts are identical between the before-binary readout and the candidate-binary readout (records /tmp/tme-unresolved-before.txt and /tmp/tme-unresolved-after.txt); any changed total or per-classification count fails the check", "inputs": "the isolated repo-graph store copy; the two unresolved-edge count readouts (before-binary vs candidate)" },
      "expected": "the inspection finds the unresolved_edges row count and every per-classification count identical before and after — every unresolved reference is preserved and classified exactly as before, since this slice's SQL touches only resolved IMPORTS fan aggregation (no-behavior-change for RG-REQ-001-L03). A differing total or class count fails the check."
    },
    {
      "checkId": "TME-C17",
      "obligationIds": ["RG-REQ-011-L06"],
      "owner": "builder",
      "method": { "kind": "inspection", "subject": "every built-rmap invocation in the outward proofs and the operator's real state root before and after the run", "criterion": "every built-rmap proof ran under RMAP_STATE_ROOT and RMAP_SOCKET_PATH pointing at throwaway copies with RMAP_AUTO_ENRICH=off and RMAP_AUTO_RETENTION=off; the operator real registry sha256 is byte-identical before and after the whole run; every throwaway root created was deleted", "inputs": "the operator registry file digest captured before and after; the list of throwaway state roots created and removed" },
      "expected": "the inspection finds every proof ran isolated, the operator real registry sha256 is byte-identical before and after, and no throwaway root was left behind (no-behavior-change for RG-REQ-011-L06)."
    }
  ]
}
-->
# TRUST-MODULE-EDGES-1 — trust's module connectivity reads the edge set `modules list` renders

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q2 (CRITICAL; REGRESSION from ORIENT-BUG-1 28126a2, 2026-05-22). CODE slice: one SQL statement in `storage/src/trust_impl.rs::compute_module_stats` + its tests + one cross-surface seam test. Maturity: MATURE (`trust`, `orient` reliability, `assess`). Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-009-L02 (trust's module connectivity reads the edge set `modules list` renders), RG-REQ-004-L01 (trust and modules read one module-edge computation), RG-REQ-002-L02 (the trust↔modules coherence seam), RG-REQ-009-L04 (a reason never outlives its cause — the false `alias_resolution_suspicion` reason goes).

**Changes (pre-authorised)** — corrected to the measured truth by decision D-TME-MOVEMENT-1 (the original prediction "48 → 0; the alias downgrade disappears" was the manager's over-prediction from RC-5): `trust` on repo-graph's "Suspicious Modules (zero connectivity)" section drops from 48 rows to the 6 GENUINELY zero-connectivity module candidates — the `rust/` umbrella directory, `tools`, `scripts`, two test-fixture directories, and the leaf crates `detectors` and `git`. Every crate that `modules list`/`modules deps` renders an edge for leaves the list, satisfying RG-REQ-009-L02 (a module with rendered edges is never listed as zero-connectivity). Because 6 genuinely-disconnected candidates remain and `detect_alias_resolution_suspicion` fires at ≥ 3, the `alias_resolution_suspicion` downgrade PERSISTS on repo-graph: the human Import-graph line stays `LOW (alias resolution suspected; 4903 unresolved imports)` and the human Change-impact line stays `LOW (alias resolution suspected)` (the humanized text `agent::reliability::humanize_reason` emits for the `alias_resolution_suspicion` machine token, which remains a triggered downgrade in `trust --json`). The ≥ 3 threshold cannot distinguish unresolved imports from genuine isolation — that is the separate root cause ALIAS-SUSPICION-1, filed as a follow-up and NOT in this slice. The LEVEL does not change while unresolved imports > 0 (`rules.rs:215-243`). Across the proof corpus the suspicious lists shrink to the genuinely-disconnected candidates — kafka 5, hadoop 2, FRAKTAG 3 (FRAKTAG has zero cross-module resolved imports by fact: 185 intra of 210), vcmi 2 — and the alias downgrade PERSISTS where ≥ 3 remain (repo-graph 6, kafka 5, FRAKTAG 3) and CLEARS where fewer remain (hadoop 2, vcmi 2). `orient`'s reliability caveat and `assess` inherit the wording.

**Explicitly NOT in scope (separate obligation RG-REQ-004-L11):** `stats`' module rows (`queries.rs::compute_module_stats`, the directory-node population) and the grpc table/edge-list identifier space. Module IDENTITY does not change (MODULES-IDENTITY-2 §3); only the source of the fan counts.

**Preserves (§3):** RG-REQ-004-L02/L03/L04/L09/L10 (modules list/deps unchanged), RG-REQ-009-L03/L06/L07 (the other trust computations), RG-REQ-002-L08 (reader-frame labels), RG-REQ-003-L03 (file universe), RG-REQ-001-L03 (unresolved rows untouched), RG-REQ-012-L01 (exit codes), RG-REQ-011-L06 (isolation), D-T6 JSON semantics, the envelope invariants of RG-REQ-009-L04 other than the downgrade-reason lines this slice changes, `dead`'s overlay. RG-REQ-009-L04 is IMPLEMENTED (its reason lines move), not preserved.

## 1. Problem (ROOT-CAUSED — RC-5, measured on the retained stores)

`trust` lists `rust/crates/agent`, `boundary-interaction`, `contract-schema` (48 of repo-graph's 61 module candidates) as zero-connectivity on the snapshot where `modules list` renders 129 edges among them; kafka 65/65, hadoop 8/9, FRAKTAG 3/4; rows with any fan > 0 = **0** on every Cargo/Gradle/Maven/TS layout. `trust_impl.rs:755-822 compute_module_stats` rows FROM `module_candidates` but takes fan_in/fan_out from MODULE→MODULE IMPORTS edges between per-DIRECTORY nodes joined by `m.qualified_name = mc.canonical_root_path` — the crate ROOT (`rust/crates/agent`, owns 0 files) while the edges attach to the LEAF directory (`rust/crates/agent/src`, fan_in 14). The join misses, COALESCE yields 0, `rules.rs:392` fires on `file_count >= 2`, and `alias_resolution_suspicion` downgrades Import-graph and Change-impact. Introduced by 28126a2 (ORIENT-BUG-1 moved the row source and left the fan subqueries keyed on directory nodes); CONTRADICTION-SWEEP-1 later added the "cross-check stats" basis line — a wording patch that names the wrong identity out loud. `modules list` reads `derive_module_dependency_edges` over `module_file_ownership` + resolved file→file IMPORTS keyed by `canonical_root_path` — the correct set.

## 2. Contract

1. **One edge computation.** `compute_module_stats` computes fan_in = COUNT(DISTINCT source module candidate) and fan_out = COUNT(DISTINCT target module candidate) over resolved file→file IMPORTS edges whose endpoints are attributed through `module_file_ownership` to different module candidates — the same set `derive_module_dependency_edges` renders — and drops the `nodes kind='MODULE'` join. A prefix-`LIKE` bridge is REJECTED (double-counts nested candidates; wrong population). `file_count` stays from ownership.
2. **The seam.** A test asserts, on the two-crate fixture and on the twin-names fixture, that `trust --json modules[].fan_in/fan_out` equal `modules deps`' per-module counts; `rules.rs::count_suspicious_modules_matches_all_criteria` and `trust_tests.rs::suspicious_modules_basis_in_reader_frame_no_internal_wording` are rewritten to the true behaviour (a module with rendered edges is never suspicious); the "cross-check stats" basis sentence is replaced by a true one or removed.
3. **Honest verdict movement.** The packet pre-states: Import-graph stays LOW wherever `unresolved_imports_count > 0`; the `alias_resolution_suspicion` reason persists where >= 3 genuinely zero-connectivity candidates remain (repo-graph 6, kafka 5, FRAKTAG 3) and clears where fewer remain (hadoop 2, vcmi 2) — D-TME-MOVEMENT-1. A repo with zero unresolved imports may go HIGH — say so if observed, never claim it otherwise.
4. **Outward proof on repo-graph (isolated copy of the retained store or an isolated index; `RMAP_TRANSPORT=stdio`):** `trust`'s "Suspicious Modules (zero connectivity)" section lists exactly the 6 genuinely-disconnected candidates named in §0 — no crate that `modules deps` renders an edge for appears (down from 48); `trust --json modules[]` fans match `modules deps` per module (`rust/crates/agent` connected: fan_out ≥ 1, fan_in ≥ 2 from daemon-runtime/storage); the Import-graph and Change-impact lines read exactly as §0 Changes (the `alias_resolution_suspicion` downgrade persists because 6 ≥ 3); the store assertion rows-with-fan>0 = 53 of 61. Cross-repo, on copies of kafka, hadoop, FRAKTAG, vcmi: rows-with-fan>0 = kafka 60/65, hadoop 6/9, FRAKTAG 0/4 (genuinely zero cross-module resolved imports — 0 is the true value), vcmi 13/16; the alias downgrade persists on kafka and FRAKTAG (≥ 3 genuinely-disconnected) and clears on hadoop and vcmi (2 each, below the ≥ 3 threshold).

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-004-L02/L03/L04/L09/L10 | `modules list`, `modules deps`, `modules violations` byte-identical (they already read the correct set) | `modules_list_tests.rs` all green; human outputs of `modules list`/`deps` on repo-graph byte-identical before/after (worktree before-binary, same isolated store) |
| RG-REQ-009-L03/L06/L07 | call-resolution %, reader-frame resolution, first-party classification untouched | `trust_tests.rs` resolution and first-party families green; trust's "calls N% resolved" line byte-identical before/after |
| RG-REQ-009-L04 | envelope invariants; snapshot headline posture | `trust/src/coherent_tests.rs` (all); `trust_tests.rs::render_headline_is_the_snapshot_posture` |
| D-T6 (P-TME-01) | root posture MEET in JSON unchanged | `coherent_tests.rs::root_never_exceeds_the_weakest_leaf` and siblings |
| RG-REQ-002-L08 | no internal diagnostic leaks into the new basis sentence | review of the replaced sentence against the reader-frame rule; `render_omits_raw_pipeline_diagnostic_sections_from_human_surface` |
| `dead` overlay (P-TME-02) | `unresolved_import_pressure` unchanged (level stays LOW) | `trust/src/overlay.rs` tests; `dead_command.rs` substrate pins |
| `orient`/`assess`/`stats` reliability axes (P-TME-03) | inherit the wording change only | `orient_reliability*` tests green; `stats` module rows UNCHANGED (L11 is separate — assert `stats` human output byte-identical) |
| RG-REQ-003-L03 | file universe untouched | `header_renders_indexed_source_tracked_only_split` |
| RG-REQ-012-L01 | exit codes | `exit_code_contract.rs` |
| RG-REQ-011-L06 | isolation | throwaway roots; registry sha256 unchanged |

The named preservation obligations bound by the allocation metadata block:

| Preservation obligation | What must remain true | Proof (check) |
|---|---|---|
| P-TME-01 | D-T6 JSON semantics: the trust root posture is the MEET over the weakest leaf and stays byte-unchanged in `trust --json` — this slice moves only the downgrade REASON lines, never the root posture computation. | TME-C10 |
| P-TME-02 | `dead`'s overlay is unchanged: its `unresolved_import_pressure` input and the overlay level (LOW) do not move, because this slice touches only resolved-IMPORTS fan aggregation, not unresolved-import pressure. | TME-C12 |
| P-TME-03 | `orient`/`assess`/`stats` reliability axes inherit ONLY the section-0 reason-wording change; `stats`' module rows (the directory-node population, RG-REQ-004-L11, out of scope) stay byte-identical. | TME-C13, TME-C13B |

## 4. Stop conditions

Frozen: module identity computation (MODULES-IDENTITY-2), `module-graph-contract.txt` (query-time derivation, no persistence), storage schema, wire protocol (JSON fields additive), D-T6, exit codes, `stats`' module population (out of scope — RG-REQ-004-L11). By decision D-TME-MOVEMENT-1 the `detect_alias_resolution_suspicion` ≥ 3 heuristic is OUT OF SCOPE — this slice re-sources only the fan counts, and the alias downgrade legitimately persists wherever ≥ 3 genuinely-disconnected candidates remain; the heuristic's recalibration is the separate follow-up ALIAS-SUSPICION-1. If the derived edge set cannot be computed inside `compute_module_stats`'s SQL without persisting module edges, STOP + DECISION_REQUIRED (the contract forbids persistence). If any verdict LEVEL changes on the four proof repos, report it as a finding — do not adjust rules to hold it. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Do NOT commit.

## 5. Validation (SYNCHRONOUS; ORDERED; `build-progress.md` after EACH step)

1. Failing tests FIRST: the seam test (two-crate + twin-names fixtures), the two rewritten tests.
2. The SQL change; chunked gates `cargo test -p repo-graph-storage`, `-p repo-graph-trust`, `-p repo-graph-rgr --lib presentation::trust::tests`, `-p repo-graph-agent`.
3. Outward proof (§2.4) on a COPY of the retained repo-graph store (`~/repo-graph-retained/audit-v0.18.0`, copy it — never open the original) served with `RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`, before-binary from a `git worktree`; cross-repo SQL on copies of kafka/hadoop/FRAKTAG/vcmi stores.
4. Byte-stability proofs (§3: modules list/deps, stats, calls-resolved line).
5. Cleanup; `build-N.md` with evidence labels.

## 6. Definition of done

§2.4 outputs as stated on repo-graph (the "Suspicious Modules (zero connectivity)" section lists exactly the 6 genuinely-disconnected candidates; no crate with rendered edges appears; fans equal `modules deps` per module); the seam test exists and is green; the two defect-pinning tests rewritten; rows-with-fan>0 = repo-graph 53/61, kafka 60/65, hadoop 6/9, FRAKTAG 0/4 (true zero), vcmi 13/16; the alias downgrade persists where ≥ 3 genuinely-disconnected candidates remain (repo-graph, kafka, FRAKTAG) and clears on hadoop and vcmi; every §3 row EXECUTED green; the verdict-level statement (LOW stays) recorded; the alias heuristic follow-up filed as ALIAS-SUSPICION-1 (out of scope, decision D-TME-MOVEMENT-1); gates green.

CORPUS PATHS: repo-graph is THIS repo; kafka, hadoop at ../legacy-codebases/<name>; FRAKTAG at ../FRAKTAG; retained stores under ~/repo-graph-retained/audit-v0.18.0 (copies only).

## Allocation amendments for INPUT-4 and INPUT-5 (2026-09-13)

Text-only corrections to validation oracles; no change to the allocation sets, the checks' intent, or the ratified behaviour:

1. TME-C03 `expected` and §2 item 3 no longer claim the alias_resolution_suspicion reason disappears — superseded by D-TME-MOVEMENT-1 (reviewer finding F-TME-001, PREP-3 cycle 2).
2. TME-C08B grep token `calls resolved` -> `calls [0-9]+% resolved` (and the prose naming that line reads "calls N% resolved") — the trust line reads "your code's calls N% resolved (…)" (D-TME-VALIDATION-STALE-1).
3. TME-C14B token `total_files` -> `indexed_file_count` — the field that exists in `stats --json` (D-TME-VALIDATION-STALE-1).
4. The >= 3 alias heuristic remains out of scope: follow-up ALIAS-SUSPICION-1 (RC-11).

INPUT-5 (D-TME-TEST-NAME-1): the render test rewritten by this slice is renamed `suspicious_modules_state_basis_and_point_at_stats` -> `suspicious_modules_basis_in_reader_frame_no_internal_wording`; TME-C03's exact filter and §2 item 2 name the new identity. Text-only; no behaviour change.
