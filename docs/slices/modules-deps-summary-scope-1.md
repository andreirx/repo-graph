<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "MODULES-DEPS-SUMMARY-SCOPE-1",
  "baselinePath": "docs/requirements/baselines/MODULES-DEPS-SUMMARY-SCOPE-1-INPUT-2.json",
  "parentRequirementIds": [
    "RG-REQ-002",
    "RG-REQ-004"
  ],
  "implements": [
    "RG-REQ-002-L03"
  ],
  "changes": [],
  "preserves": [
    "RG-REQ-002-L01",
    "RG-REQ-002-L02",
    "RG-REQ-002-L04",
    "RG-REQ-002-L05",
    "RG-REQ-002-L06",
    "RG-REQ-002-L07",
    "RG-REQ-002-L08",
    "RG-REQ-002-L09",
    "RG-REQ-002-L10",
    "RG-REQ-002-L11",
    "RG-REQ-004-L01",
    "RG-REQ-004-L02",
    "RG-REQ-004-L03",
    "RG-REQ-004-L04",
    "RG-REQ-004-L05",
    "RG-REQ-004-L06",
    "RG-REQ-004-L07",
    "RG-REQ-004-L08",
    "RG-REQ-004-L09",
    "RG-REQ-004-L10",
    "RG-REQ-004-L11",
    "RG-REQ-004-L12"
  ],
  "preservationObligationIds": [
    "P-MDSS-01",
    "P-MDSS-02",
    "P-MDSS-03",
    "P-MDSS-04",
    "P-MDSS-05"
  ],
  "candidatePaths": [
    "rust/crates/classification/src/module_edges.rs",
    "rust/crates/daemon-runtime/src/dispatch.rs",
    "rust/crates/daemon-runtime/src/import_partition_view.rs",
    "rust/crates/rgr/src/presentation/modules_deps.rs",
    "docs/cli/rmap-contracts.md"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/MODULES-DEPS-SUMMARY-SCOPE-1/verification.json",
    "docs/assurance/MODULES-DEPS-SUMMARY-SCOPE-1/implementation-review.json"
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
  "acceptanceBoundary": "A query-time slice: no index run, no INDEXER_VERSION change, no store change. Written under D-MDSS-SCOPE-1 (operator A, overridable) with the direction rule of SLICE_DOC 2.1 item 2. Precondition: the admission commit carries this document, its manifest and D-MDSS-SCOPE-1.md (MDSS-C03's porcelain admits only the five candidate paths). The classification suite, daemon-runtime --lib, the daemon-runtime consolidation witness and rgr --lib, with new tests by name and floors; fmt/clippy over the three touched crates; byte-identity outside the five paths and of the witness manifests; the field proof serving copies of the manager's before-roots with the candidate binary (leveldb `table`, poco `Foundation`: literal scoped figures per direction and view, every line outside the Summary block unchanged, unscoped byte-identity) and a before-binary built from HEAD answering the readers of the shared derivation byte-identically on the same stores.",
  "checks": [
    {
      "checkId": "MDSS-C01",
      "obligationIds": [
        "RG-REQ-002-L03",
        "RG-REQ-002-L11",
        "RG-REQ-004-L01",
        "RG-REQ-004-L02",
        "RG-REQ-004-L07",
        "P-MDSS-01",
        "P-MDSS-03",
        "P-MDSS-04",
        "RG-REQ-004-L12"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "cwd": "rust",
        "environment": "candidate tree; no index run — the slice is query-time only (no INDEXER_VERSION change); isolated rmap only (RMAP_STATE_ROOT/RMAP_SOCKET_PATH under the builder's own /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-run.* directory, RMAP_TRANSPORT=stdio, every background pass off) serving COPIES of the manager's before-roots, which are read-only; logs and captures under .agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/{logs,after}/",
        "command": "G=../.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/logs && mkdir -p $G && ( set -o pipefail; cargo test -p repo-graph-classification 2>&1 | tee $G/mdss-01.txt ) && grep -qE '^test result: ok\\.' $G/mdss-01.txt && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED|^error(\\[E[0-9]+\\])?: ' $G/mdss-01.txt && for t in per_module_counters_count_intra_module_imports_per_module per_module_counters_count_unowned_source_imports_by_target_module per_module_counters_count_unowned_target_imports_by_source_module derivation_outputs_other_than_per_module_equal_their_literals_in_every_view per_module_counters_count_only_the_imports_the_view_admits module_edge_carries_its_four_partition_counts_and_the_unknown_tally a_relation_in_two_partitions_counts_each_import_in_its_own_partition view_edge_counts_only_admitted_imports_and_their_source_files a_relation_only_through_excluded_imports_leaves_the_view_and_is_counted_in_the_remainder partitions_of_a_pair_outside_the_result_is_absent_never_zero; do grep -qE \"^test .*::$t \\.\\.\\. ok$\" $G/mdss-01.txt || grep -qE \"^test $t \\.\\.\\. ok$\" $G/mdss-01.txt || { echo \"MISSING $t\"; exit 1; }; done && test \"$(cargo test -p repo-graph-classification -- --list 2>/dev/null | grep -c ': test$')\" -ge 322 && test \"$(git -C .. diff -U0 HEAD -- rust/crates/classification/src/module_edges.rs | grep -cE '^-\\s*fn [a-z_0-9]+\\(')\" -eq 0",
        "inputs": "The WHOLE classification suite (317 listed at HEAD f69759f4 and at 4e7bd983, EXECUTED `-- --list` 2026-10-04; floor >= 322 = 317 + 5). NEW (5) in module_edges.rs, each through `derive_module_dependency_edges_in_view`. Fixture F4 has two owned modules A and X and one unowned file, and carries one import in EACH of the four partitions (production certain, test certain, production inferred, test inferred) for each shape: cross-module A→X and X→A, intra-module X→X, unowned source into X, X into an unowned target, and unowned to unowned. `per_module_counters_count_intra_module_imports_per_module` (an import whose source and target are owned by X counts in X's `intra_module` and in no other module's entry; module A, whose imports in F4 are all cross-module, has NO map entry under any view, because the map records only intra-module and unowned-endpoint imports and a missing entry means those three counters are zero, not that A has no imports — A's cross-module count stays on the edges' `import_count`); `per_module_counters_count_unowned_source_imports_by_target_module` (an unowned source into a file owned by X counts in X's `source_unowned_into`; an import with BOTH files unowned counts in no module's entry); `per_module_counters_count_unowned_target_imports_by_source_module` (a source owned by X into an unowned target counts in X's `target_unowned_from`); `derivation_outputs_other_than_per_module_equal_their_literals_in_every_view` (on F4 under each of `ImportView::DEFAULT`, `CERTAIN_WITH_TESTS`, `WITH_INFERRED` and `ALL`: the edges with `import_count` and `source_file_count`, `edge_partitions`, the remainder and the five repo-wide counters equal literals written by hand from F4's definition, not computed by the code under test — the outputs that `modules list`, `modules show`, `cycles`, trust, orient and `modules violations` consume); `per_module_counters_count_only_the_imports_the_view_admits` (on F4 under each of the four views, X's three counters include exactly the admitted partitions: production certain always; test certain only with include_tests; production inferred only with include_inferred — the RG-REQ-002-L11 inferred partition tested on its own; test inferred only under ALL). Keyed by canonical path, which is the key the handler's edge filter compares (dispatch.rs:8472-8486). No existing test function is deleted (the `git diff -U0` assertion), and every existing module_edges test keeps its assertions. RG-REQ-004-L12 (preserved, ratified by D-TEST-SCOPE-1): the import view the derivation partitions by is unchanged, held by the excluded-import rule of SLICE_DOC 2.1 item 1 (`module_edges.rs:218-268`, `import_partition.rs:420-439`): an import the view does not admit adds to no diagnostics counter (repo-wide or per-module) and to no edge's `import_count` or `source_file_count`; an excluded CROSS-MODULE import (both files owned, by different modules) is still counted in its relation's partition cell, which `edge_partitions` carries when the relation has at least one admitted import, and adds to the `imports` count of its partition's remainder group (the group whose flags would show it); a relation with no admitted import adds one to the `edges` count of exactly one group, the first group in the order tests, inferred, tests-and-inferred in which the relation has an excluded import (`import_partition.rs:429-436`), so a relation whose excluded imports fall in two groups (for example one test certain and one production inferred import) adds to both groups' `imports` and only to the tests group's `edges`; an excluded intra-module import or an excluded import with an unowned endpoint is in no partition cell and in no remainder. NEW: `per_module_counters_count_only_the_imports_the_view_admits` asserts that under `ImportView::DEFAULT` X's three counters count only the production certain imports; that `CERTAIN_WITH_TESTS` (`--include-tests`) adds exactly the test certain imports; that `WITH_INFERRED` (`--include-inferred`) adds exactly the production inferred imports; and that `ALL` adds all four partitions. `derivation_outputs_other_than_per_module_equal_their_literals_in_every_view` asserts, under all four views, that the edges, `edge_partitions` and the remainder equal hand-written literals that follow the rule: on F4 the relations A→X and X→A carry all four of their partition cells in `edge_partitions` in every view (each has an admitted production certain import), the remainder's `imports` count only the excluded imports of those two relations, with `edges` 0, and the excluded intra-module (X→X) and unowned-endpoint imports appear in no edge, no partition cell and no remainder group. EXISTING (TEST-EDGE-SCOPE-1B, present verbatim at HEAD 4e7bd983, each run by name in the command and kept with unchanged assertions): `module_edge_carries_its_four_partition_counts_and_the_unknown_tally`, `a_relation_in_two_partitions_counts_each_import_in_its_own_partition`, `view_edge_counts_only_admitted_imports_and_their_source_files`, `a_relation_only_through_excluded_imports_leaves_the_view_and_is_counted_in_the_remainder`, `partitions_of_a_pair_outside_the_result_is_absent_never_zero`. Between them they pin: an in-view edge keeps its excluded imports in `edge_partitions` while its `import_count` counts only the admitted ones, with the unknown-test-status tally (an import whose test status is unknown is admitted with production); the excluded imports of an in-view relation count in the remainder's `imports` with `edges` 0; an edge's `import_count`, `source_file_count` and the diagnostics count only admitted imports; a relation reachable only through excluded imports leaves the view, counts in the remainder's `imports` of each group its excluded imports fall in, and adds one to the `edges` of only the first such group in the order tests, inferred, tests-and-inferred; and a pair outside the view has no partition record (never a zero)."
      },
      "expected": "exit 0: the derivation returns, beside the five repo-wide counters it returns today, a per-module entry for exactly the modules to which an admitted intra-module or unowned-endpoint import adds, counting exactly the partitions the view admits (a module whose admitted imports are all cross-module has no entry; its cross-module figure stays on the edges); every preserved obligation this check carries is unchanged (no behavior change): the edges, `edge_partitions`, the remainder and the five repo-wide counter values equal their hand-written literals in all four views, and every existing module_edges test is preserved by name with unchanged assertions; the partition behaviour of RG-REQ-004-L12 is unchanged and follows the excluded-import rule of SLICE_DOC 2.1 item 1 (`module_edges.rs:218-268`, `import_partition.rs:420-439`): an import the view does not admit adds to no diagnostics counter (repo-wide or per-module) and to no edge's `import_count` or `source_file_count`; an excluded CROSS-MODULE import (both files owned, by different modules) is still counted in its relation's partition cell, which `edge_partitions` carries when the relation has at least one admitted import, and adds to the `imports` count of its partition's remainder group (the group whose flags would show it); a relation with no admitted import adds one to the `edges` count of exactly one group, the first group in the order tests, inferred, tests-and-inferred in which the relation has an excluded import (`import_partition.rs:429-436`), so a relation whose excluded imports fall in two groups (for example one test certain and one production inferred import) adds to both groups' `imports` and only to the tests group's `edges`; an excluded intra-module import or an excluded import with an unowned endpoint is in no partition cell and in no remainder; the five TEST-EDGE-SCOPE-1B partition tests pass by name with unchanged assertions"
    },
    {
      "checkId": "MDSS-C02",
      "obligationIds": [
        "RG-REQ-002-L03",
        "RG-REQ-002-L04",
        "RG-REQ-004-L01",
        "RG-REQ-004-L10",
        "P-MDSS-01",
        "P-MDSS-02",
        "P-MDSS-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "cwd": "rust",
        "environment": "candidate tree; no index run — the slice is query-time only (no INDEXER_VERSION change); isolated rmap only (RMAP_STATE_ROOT/RMAP_SOCKET_PATH under the builder's own /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-run.* directory, RMAP_TRANSPORT=stdio, every background pass off) serving COPIES of the manager's before-roots, which are read-only; logs and captures under .agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/{logs,after}/",
        "command": "G=../.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/logs && mkdir -p $G && ( set -o pipefail; cargo test -p repo-graph-daemon-runtime --lib 2>&1 | tee $G/mdss-02dr.txt ) && grep -qE '^test result: ok\\.' $G/mdss-02dr.txt && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED|^error(\\[E[0-9]+\\])?: ' $G/mdss-02dr.txt && for t in modules_deps_with_a_module_emits_that_modules_counters_with_scope_module modules_deps_module_cross_module_figure_equals_the_sum_of_the_filtered_edges_under_each_direction modules_deps_module_counters_count_only_the_imports_the_direction_selects modules_deps_without_a_module_emits_todays_repo_wide_counters_with_scope_repo modules_violations_diagnostics_are_unchanged modules_deps_include_tests_restores_test_imports modules_violations_keep_test_imports_and_count_inferred_ones_not_judged; do grep -qE \"^test .*::$t \\.\\.\\. ok$\" $G/mdss-02dr.txt || grep -qE \"^test $t \\.\\.\\. ok$\" $G/mdss-02dr.txt || { echo \"MISSING $t\"; exit 1; }; done && test \"$(cargo test -p repo-graph-daemon-runtime --lib -- --list 2>/dev/null | grep -c ': test$')\" -ge 889 && ( set -o pipefail; cargo test -p repo-graph-daemon-runtime --test consolidation_witness 2>&1 | tee $G/mdss-02w.txt ) && grep -qE '^test result: ok\\.' $G/mdss-02w.txt && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED|^error(\\[E[0-9]+\\])?: ' $G/mdss-02w.txt && grep -qE '^test result: ok\\. 15 passed; 0 failed' $G/mdss-02w.txt && for t in every_dispatch_arm_is_declared_in_manifest fact_class_declarations_are_valid reader_set_matches_sanctioned_list_on_head test_scaffolding_readers_are_cfg_test_gated_on_head; do grep -qE \"^test .*::$t \\.\\.\\. ok$\" $G/mdss-02w.txt || grep -qE \"^test $t \\.\\.\\. ok$\" $G/mdss-02w.txt || { echo \"MISSING $t\"; exit 1; }; done && ( set -o pipefail; cargo test -p repo-graph-rgr --lib 2>&1 | tee $G/mdss-02rgr.txt ) && grep -qE '^test result: ok\\.' $G/mdss-02rgr.txt && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED|^error(\\[E[0-9]+\\])?: ' $G/mdss-02rgr.txt && for t in deps_render_module_summary_states_the_module_and_direction_with_the_modules_figures deps_render_unfiltered_without_a_scope_key_renders_as_today deps_render_repo_wide_figures_beside_a_module_name_the_whole_repository deps_render_module_scope_without_a_module_name_states_that_reason deps_render_unknown_scope_prints_the_stored_value_marked deps_render_shows_summary deps_render_shows_edges deps_render_shows_edge_counts; do grep -qE \"^test .*::$t \\.\\.\\. ok$\" $G/mdss-02rgr.txt || grep -qE \"^test $t \\.\\.\\. ok$\" $G/mdss-02rgr.txt || { echo \"MISSING $t\"; exit 1; }; done && test \"$(cargo test -p repo-graph-rgr --lib -- --list 2>/dev/null | grep -c ': test$')\" -ge 1478",
        "inputs": "daemon-runtime `--lib` (884 at HEAD; floor >= 889 = 884 + 5), the daemon-runtime reviewed-boundary witness `--test consolidation_witness` (15 tests at HEAD, all must pass; four named) and rgr `--lib` (1473; floor >= 1478 = 1473 + 5). NEW daemon-runtime (5), in the `#[cfg(test)] mod tests` of import_partition_view.rs on its `mini_leveldb` dispatch fixture (extended there if a case needs an unowned file or an intra-module import): `modules_deps_with_a_module_emits_that_modules_counters_with_scope_module` (with a module the answer carries `diagnostics_scope: \"module\"` and `diagnostics` = that module's counters, `imports_total` = the sum of the four others); `modules_deps_module_cross_module_figure_equals_the_sum_of_the_filtered_edges_under_each_direction` (all / outbound / inbound: `imports_cross_module` = the sum of `results[].import_count`); `modules_deps_module_counters_count_only_the_imports_the_direction_selects` (outbound: `imports_source_unowned` = 0 and `imports_target_unowned` = X's; inbound: `imports_target_unowned` = 0 and `imports_source_unowned` = X's; `imports_intra_module` = X's under every direction); `modules_deps_without_a_module_emits_todays_repo_wide_counters_with_scope_repo` (no module: the five repo-wide values and `diagnostics_scope: \"repo\"`); `modules_violations_diagnostics_are_unchanged` (the `modules violations` diagnostics object equals the repo-wide derivation counters, with no `diagnostics_scope` key). EXISTING daemon by name: `modules_deps_include_tests_restores_test_imports`, `modules_violations_keep_test_imports_and_count_inferred_ones_not_judged`. NEW rgr (5) in modules_deps.rs, one per decode case of SLICE_DOC 2.1 item 3: `deps_render_module_summary_states_the_module_and_direction_with_the_modules_figures` (scope `module` with `module: X` renders `Summary (module X, <label>):` for each of the three `Queried:` labels, then the three lines with the answer's figures); `deps_render_unfiltered_without_a_scope_key_renders_as_today` (no `module` and no `diagnostics_scope`, and no `module` with `repo`: `Summary:` byte-identical to today); `deps_render_repo_wide_figures_beside_a_module_name_the_whole_repository` (`module: X` with `diagnostics_scope` absent — an older daemon, whose figures are repo-wide — or `repo`: the header is `Summary (whole repository, not module X):`, never `Summary:` under `Module: X` and never the scoped header); `deps_render_module_scope_without_a_module_name_states_that_reason` (`diagnostics_scope: \"module\"` without a `module` key: `Summary (diagnostics scope \"module\" but the answer names no module):`); `deps_render_unknown_scope_prints_the_stored_value_marked` (a value the build has no phrase for: `Summary (diagnostics scope \"<value>\" — not a scope this build reads):`). EXISTING rgr by name: `deps_render_shows_summary`, `deps_render_shows_edges`, `deps_render_shows_edge_counts`."
      },
      "expected": "exit 0: under a module filter the JSON diagnostics are the module's own counts over the imports the direction selects with `diagnostics_scope: module`, and the cross-module figure equals the rendered edges' sum in every direction; the human block under a module states the module and the direction in its header; repo-wide figures beside a module name are headed as the whole repository, a module scope without a module name states that reason, and an unknown scope prints as stored and marked; the witness passes; every preserved obligation this check carries is unchanged (no behavior change): the unscoped `Summary:` block and its JSON values, the edge rows, the remainder, the `modules violations` diagnostics, an older unfiltered envelope's render and the witness manifests are preserved by the existing tests, by name with unchanged assertions"
    },
    {
      "checkId": "MDSS-C03",
      "obligationIds": [
        "RG-REQ-002-L01",
        "RG-REQ-002-L05",
        "RG-REQ-002-L06",
        "RG-REQ-002-L07",
        "RG-REQ-002-L08",
        "RG-REQ-002-L09",
        "RG-REQ-002-L10",
        "RG-REQ-004-L03",
        "RG-REQ-004-L04",
        "RG-REQ-004-L05",
        "RG-REQ-004-L06",
        "RG-REQ-004-L08",
        "RG-REQ-004-L09",
        "P-MDSS-05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "cwd": "rust",
        "environment": "candidate tree; no index run — the slice is query-time only (no INDEXER_VERSION change); isolated rmap only (RMAP_STATE_ROOT/RMAP_SOCKET_PATH under the builder's own /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-run.* directory, RMAP_TRANSPORT=stdio, every background pass off) serving COPIES of the manager's before-roots, which are read-only; logs and captures under .agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/{logs,after}/",
        "command": "cargo fmt -p repo-graph-classification -p repo-graph-daemon-runtime -p repo-graph-rgr -- --check && ( set -o pipefail; cargo clippy -p repo-graph-classification -p repo-graph-daemon-runtime -p repo-graph-rgr --all-targets -- -D warnings 2>&1 | tail -3 ) && git -C .. diff --quiet HEAD -- rust/crates/module-queries rust/crates/trust rust/crates/storage rust/crates/indexer rust/crates/repo-index rust/crates/daemon-runtime/witness && git -C .. diff --check HEAD && test -z \"$(git -C .. status --porcelain -- . ':!.agent-manager' ':!rust/target' | awk '{print $2}' | grep -vxF -f <(printf '%s\\n' rust/crates/classification/src/module_edges.rs rust/crates/daemon-runtime/src/dispatch.rs rust/crates/daemon-runtime/src/import_partition_view.rs rust/crates/rgr/src/presentation/modules_deps.rs docs/cli/rmap-contracts.md))\" && git -C .. diff -U0 HEAD -- rust/crates/daemon-runtime/src/import_partition_view.rs | awk '/^@@/{split($2,a,\",\"); if (substr(a[1],2)+0 < 528) bad=1} END{exit bad}' && grep -q 'MODULES-DEPS-SUMMARY-SCOPE-1' ../docs/cli/rmap-contracts.md",
        "inputs": "SCOPE CONTROL, not an output oracle: fmt per touched crate (CLAUDE.md: `cargo fmt --check -p <touched crates>`); clippy -D warnings over the three touched crates (`--all-targets`); no source change in module-queries, trust, storage, indexer, repo-index or the daemon-runtime witness manifests (no re-index, no INDEXER_VERSION change, no manifest edit); whitespace; the porcelain restricted to the five candidate paths; import_partition_view.rs changed only at or below line 528 of HEAD (inside `#[cfg(test)] mod tests`, which opens at :527-528); the CLI contract gained the `modules deps` section citing MODULES-DEPS-SUMMARY-SCOPE-1. That the readers of the shared derivation answer as before is proven by MDSS-C01 (derivation literals) and MDSS-C04 (before-binary comparison), not by this check's file identity."
      },
      "expected": "exit 0: every preserved obligation this check carries is unchanged (no behavior change): the change is confined to the five candidate paths — no other source, storage schema, indexer, INDEXER_VERSION or witness manifest changes, so the store-reading, indexing and policy code those obligations rest on is the code at HEAD; import_partition_view.rs's non-test code is unchanged; the contract states that under a module filter the Summary is that module's counts over the imports the direction selects, with its universe in the header, and that the unscoped block is unchanged"
    },
    {
      "checkId": "MDSS-C04",
      "obligationIds": [
        "RG-REQ-002-L02",
        "RG-REQ-002-L03",
        "RG-REQ-002-L11",
        "RG-REQ-004-L01",
        "RG-REQ-004-L02",
        "RG-REQ-004-L07",
        "RG-REQ-004-L11",
        "P-MDSS-02",
        "P-MDSS-03",
        "P-MDSS-04",
        "P-MDSS-05",
        "RG-REQ-004-L12"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "cwd": "rust",
        "environment": "candidate tree; no index run — the slice is query-time only (no INDEXER_VERSION change); isolated rmap only (RMAP_STATE_ROOT/RMAP_SOCKET_PATH under the builder's own /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-run.* directory, RMAP_TRANSPORT=stdio, every background pass off) serving COPIES of the manager's before-roots, which are read-only, one copy per binary; a before-binary built once from `git worktree add --detach <unique>/wt HEAD`, where `<unique>` is a fresh `mktemp -d /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-before-src.XXXXXX` and the worktree is the admission commit, whose rust/ equals 4e7bd983's. The EXIT trap removes only what this run created: the run directory, the worktree (only after `git worktree add` succeeded, recorded by `WT_CREATED=1`) and the unique parent. A failed removal is printed as `CLEANUP FAILED: <path>` and turns a passing run into exit 1, never into success (CLAUDE.md: never stash or reset the working tree; delete only what you created); logs and captures under .agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/{logs,after}/",
        "command": "bash -eu -o pipefail -c 'export RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off RMAP_AUTO_RETENTION=off RMAP_SEED_VECTORS=off\nL=\"/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases\"\nB=\"/private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1\"\nSL=\"$PWD/../.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1\"\nE=\"$SL/after\"; mkdir -p \"$E/consumers\"\nW=$(mktemp -d \"$B-run.XXXXXX\")\nWTP=$(mktemp -d \"$B-before-src.XXXXXX\"); WT=\"$WTP/wt\"; WT_CREATED=0\ncleanup(){ rc=$?; bad=0\n  rm -rf \"$W\" || { echo \"CLEANUP FAILED: $W not removed\" >&2; bad=1; }\n  if [ \"$WT_CREATED\" = 1 ]; then git -C .. worktree remove --force \"$WT\" || { echo \"CLEANUP FAILED: worktree $WT not removed\" >&2; bad=1; }; fi\n  rmdir \"$WTP\" || { echo \"CLEANUP FAILED: $WTP not removed\" >&2; bad=1; }\n  if [ \"$rc\" -eq 0 ] && [ \"$bad\" -ne 0 ]; then exit 1; fi; exit \"$rc\"; }\ntrap cleanup EXIT\ncargo build --release -p repo-graph-rgr -p rmapd 2>&1 | tail -1\nRM=\"$PWD/target/release/rmap\"\ngit -C .. worktree add --detach \"$WT\" HEAD\nWT_CREATED=1\n(cd \"$WT/rust\" && cargo build --release -p repo-graph-rgr -p rmapd 2>&1 | tail -1)\nRB=\"$WT/rust/target/release/rmap\"\nfor r in leveldb poco; do\n  (cd \"$B-$r-before\" && find . -type f -name \"*.db\" | sort | xargs shasum -a 256) | diff - \"$SL/before/$r.db.sha\"\n  for side in cand base; do\n    cp -R \"$B-$r-before\" \"$W/$r-$side\"\n    python3 - \"$W/$r-$side\" <<\"PY\"\nimport json,sys,glob\nroot=sys.argv[1]; p=root+\"/registry.json\"; r=json.load(open(p)); dbs=glob.glob(root+\"/databases/*.db\"); assert len(dbs)==1,dbs\nfor e in r[\"repos\"]: e[\"db_path\"]=dbs[0]\njson.dump(r,open(p,\"w\"),indent=2)\nPY\n  done\ndone\nrun(){ local bin=\"$1\" root=\"$2\" dir=\"$3\"; shift 3; (cd \"$dir\" && RMAP_STATE_ROOT=\"$W/$root\" RMAP_SOCKET_PATH=\"$W/$root/d.sock\" \"$bin\" \"$@\" 2>>\"$E/stderr.txt\"); }\nc(){ local r=\"$1\"; shift; run \"$RM\" \"$r-cand\" \"$L/$r\" \"$@\"; }\nrun \"$RM\" leveldb-cand \"$L/leveldb\" modules deps > \"$E/leveldb-all.txt\"; diff \"$SL/before/leveldb-all.txt\" \"$E/leveldb-all.txt\"\nrun \"$RM\" poco-cand \"$L/poco\" modules deps > \"$E/poco-all.txt\"; diff \"$SL/before/poco-all.txt\" \"$E/poco-all.txt\"\nc leveldb modules deps table > \"$E/leveldb-table.txt\"\nc poco modules deps Foundation > \"$E/poco-foundation.txt\"\nc leveldb modules deps table --json > \"$E/leveldb-table-all.json\"\nc poco modules deps Foundation --json > \"$E/poco-foundation-all.json\"\nfor d in outbound inbound; do\n  c leveldb modules deps table --$d --json > \"$E/leveldb-table-$d.json\"\n  c poco modules deps Foundation --$d --json > \"$E/poco-foundation-$d.json\"\ndone\nc leveldb modules deps table --include-tests --json > \"$E/leveldb-table-include-tests.json\"\nc poco modules deps Foundation --include-inferred --json > \"$E/poco-foundation-include-inferred.json\"\nc poco modules deps Foundation --include-inferred --outbound --json > \"$E/poco-foundation-include-inferred-outbound.json\"\nc poco modules deps Foundation --include-tests --include-inferred --json > \"$E/poco-foundation-both.json\"\nc leveldb modules deps --json > \"$E/leveldb-repo.json\"\nc poco modules deps --json > \"$E/poco-repo.json\"\npython3 - \"$E\" \"$SL/before\" <<\"PY\"\nimport json,sys\nE,BF=sys.argv[1],sys.argv[2]\nK=(\"imports_total\",\"imports_cross_module\",\"imports_intra_module\",\"imports_source_unowned\",\"imports_target_unowned\")\ndef diag(t): return dict(zip(K,t))\nscoped={\"leveldb-table-all.json\":(65,46,19,0,0),\"leveldb-table-outbound.json\":(60,41,19,0,0),\"leveldb-table-inbound.json\":(24,5,19,0,0),\n \"leveldb-table-include-tests.json\":(85,62,23,0,0),\"poco-foundation-all.json\":(3446,1653,1751,27,15),\n \"poco-foundation-outbound.json\":(1766,0,1751,0,15),\"poco-foundation-inbound.json\":(3431,1653,1751,27,0),\n \"poco-foundation-include-inferred.json\":(3462,1653,1751,27,31),\"poco-foundation-include-inferred-outbound.json\":(1782,0,1751,0,31),\n \"poco-foundation-both.json\":(5345,2494,2793,27,31)}\nfor f,t in scoped.items():\n    d=json.load(open(E+\"/\"+f)); s=sum(r[\"import_count\"] for r in d[\"results\"])\n    assert d[\"diagnostics_scope\"]==\"module\",(f,d.get(\"diagnostics_scope\"))\n    assert d[\"diagnostics\"]==diag(t),(f,d[\"diagnostics\"],t)\n    assert d[\"diagnostics\"][\"imports_cross_module\"]==s,(f,\"edge sum\",s)\n    print(\"OK\",f,d[\"diagnostics\"])\nfor f,t in ((\"leveldb-repo.json\",(343,185,140,18,0)),(\"poco-repo.json\",(7852,2006,4624,1207,15))):\n    d=json.load(open(E+\"/\"+f)); assert d[\"diagnostics_scope\"]==\"repo\",f; assert d[\"diagnostics\"]==diag(t),(f,d[\"diagnostics\"]); print(\"OK\",f,d[\"diagnostics\"])\ndef split(path):\n    ls=open(path).read().split(\"\\n\"); i=[j for j,l in enumerate(ls) if l==\"Summary:\" or l.startswith(\"Summary (\")][0]\n    return ls[:i]+ls[i+4:], ls[i:i+4]\nfor b,a,want in ((\"leveldb-table-before.txt\",\"leveldb-table.txt\",[\"Summary (module table, all directions):\",\"  46 cross-module dependencies\",\"  19 intra-module imports\",\"  0 imports from unowned sources\"]),\n                 (\"poco-foundation-before.txt\",\"poco-foundation.txt\",[\"Summary (module Foundation, all directions):\",\"  1653 cross-module dependencies\",\"  1751 intra-module imports\",\"  27 imports from unowned sources\"])):\n    rb,_=split(BF+\"/\"+b); ra,sa=split(E+\"/\"+a)\n    assert sa==want,(a,sa); assert ra==rb,(a,\"a line outside the Summary block changed\"); print(\"OK\",a,sa)\nPY\nn=0\nfor r in leveldb poco; do\n  for q in \"modules list\" \"modules list --include-tests --include-inferred\" \"cycles\" \"cycles --include-tests --include-inferred\" \"modules violations\" \"trust\" \"orient\"; do\n    n=$((n+1)); f=\"$E/consumers/$r-$n\"\n    run \"$RB\" \"$r-base\" \"$L/$r\" $q > \"$f.base.txt\"\n    run \"$RM\" \"$r-cand\" \"$L/$r\" $q > \"$f.cand.txt\"\n    cmp \"$f.base.txt\" \"$f.cand.txt\"; echo \"same: $r $q ($(wc -l < \"$f.cand.txt\") lines)\"\n  done\ndone\nrun \"$RB\" leveldb-base \"$L/leveldb\" modules show table > \"$E/consumers/leveldb-show.base.txt\"; run \"$RM\" leveldb-cand \"$L/leveldb\" modules show table > \"$E/consumers/leveldb-show.cand.txt\"; cmp \"$E/consumers/leveldb-show.base.txt\" \"$E/consumers/leveldb-show.cand.txt\"; echo \"same: leveldb modules show table\"\nfor r in leveldb poco; do (cd \"$B-$r-before\" && find . -type f -name \"*.db\" | sort | xargs shasum -a 256) | diff - \"$SL/before/$r.db.sha\"; done\necho MDSS-C04-OK'",
        "inputs": "The outward proof, query-time only — NO index run: the candidate's release binary serves COPIES (registry `db_path` repointed) of the manager's before-roots `/private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-{leveldb,poco}-before` (byte copies of the retained v0.20.0 stores `1196d1380537d43e.db` sha256 c5bb3e8f…, `9858deeeefef8599.db` d23bffde…; durable copies `~/repo-graph-retained/MODULES-DEPS-SUMMARY-SCOPE-1-before/`; digests in `.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/before/*.db.sha`, asserted before and after). BEFORE (manager, HEAD f69759f4 binary, captures in `before/`): leveldb `modules deps table` prints `Summary:` `185 cross-module dependencies` / `140 intra-module imports` / `18 imports from unowned sources` (the repo-wide figures) while its 4 edges sum to 46; poco `modules deps Foundation` prints 2006 / 4624 / 1207 while its 24 edges sum to 1653. LITERALS (document author, EXECUTED 2026-10-04: a read-only SQL re-implementation of `derive_module_dependency_edges_in_view` over the before-stores — `module_candidates`, `module_file_ownership`, and the IMPORTS rows of `storage/src/import_partition_reads.rs:84-104`, latest ready snapshot, sqlite `immutable=1` — that reproduces the captured repo-wide figures and edge sums exactly), as {imports_total, imports_cross_module, imports_intra_module, imports_source_unowned, imports_target_unowned}, DEFAULT view: leveldb `table` all {65, 46, 19, 0, 0}, outbound {60, 41, 19, 0, 0}, inbound {24, 5, 19, 0, 0}; `table --include-tests` all {85, 62, 23, 0, 0}; poco `Foundation` all {3446, 1653, 1751, 27, 15}, outbound {1766, 0, 1751, 0, 15}, inbound {3431, 1653, 1751, 27, 0}; `Foundation --include-inferred` all {3462, 1653, 1751, 27, 31}, outbound {1782, 0, 1751, 0, 31} (the inferred-only witness: 16 inferred production imports from Foundation into unowned files, e.g. `Foundation/src/NumericString.cpp:27` `#include \"bignum-dtoa.cc\"`, stored `inferred`, basis `unique_basename`, target `dependencies/v8_double_conversion/src/bignum-dtoa.cc`, which no module owns); `Foundation --include-tests --include-inferred` all {5345, 2494, 2793, 27, 31}; repo-wide leveldb {343, 185, 140, 18, 0}, poco {7852, 2006, 4624, 1207, 15}. Oracles: (i) unscoped human text byte-identical to the before capture on both repositories; (ii) the scoped human Summary block is exactly the header and three lines bound in the script, and every other line of the scoped output equals the before capture (Queried, Module, edge rows, remainder, undetermined block); (iii) every scoped JSON answer: `diagnostics_scope == module`, `diagnostics` equals the literal, `imports_cross_module == Σ results.import_count`; unscoped: `diagnostics_scope == repo` and today's values; (iv) the readers of the shared derivation answer as before: a before-binary built from HEAD and the candidate binary serve separate copies of the same before-store, and their human answers to `modules list`, `modules list --include-tests --include-inferred`, `cycles`, `cycles --include-tests --include-inferred`, `modules violations`, `trust` and `orient` on leveldb and poco, and `modules show table` on leveldb, are byte-identical (`cmp`); (v) the before-roots untouched. Evidence in `.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/after/`; the run directory is removed on exit. RG-REQ-004-L12 (preserved, ratified by D-TEST-SCOPE-1) is held by the same literals and oracles, which follow the excluded-import rule of SLICE_DOC 2.1 item 1 (`module_edges.rs:218-268`, `import_partition.rs:420-439`): an import the view does not admit adds to no diagnostics counter (repo-wide or per-module) and to no edge's `import_count` or `source_file_count`; an excluded CROSS-MODULE import (both files owned, by different modules) is still counted in its relation's partition cell, which `edge_partitions` carries when the relation has at least one admitted import, and adds to the `imports` count of its partition's remainder group (the group whose flags would show it); a relation with no admitted import adds one to the `edges` count of exactly one group, the first group in the order tests, inferred, tests-and-inferred in which the relation has an excluded import (`import_partition.rs:429-436`), so a relation whose excluded imports fall in two groups (for example one test certain and one production inferred import) adds to both groups' `imports` and only to the tests group's `edges`; an excluded intra-module import or an excluded import with an unowned endpoint is in no partition cell and in no remainder. In the field: leveldb `table` with `--include-tests` gains 20 admitted imports ({65, 46, 19, 0, 0} → {85, 62, 23, 0, 0}): 16 cross-module test imports, which are exactly the default view's scoped remainder line `+16 imports from test files, not shown (1 cross-module dependency only through them) — --include-tests`, and 4 intra-module test imports, which no remainder states; poco `Foundation` with both flags gains 841 cross-module imports (the default remainder `+841 imports from test files`) and 1042 intra-module imports (in no remainder), and with `--include-inferred` 16 inferred imports into unowned targets (`imports_target_unowned` 15 → 31, in no remainder; the scoped default answer has no inferred remainder line because Foundation's cross-module figure is 1653 in both views). Oracle (ii) keeps the scoped remainder lines identical to the before capture. Oracle (i) compares only the UNSCOPED default `modules deps` human text, and it compares it with the manager's before capture (HEAD f69759f4 binary), not with a before-binary run; no `--include-tests --include-inferred` answer of `modules deps` is compared, and the scoped default Summary block is the intended change, bound by oracle (ii). Oracle (iv) is the before-binary comparison: the default and `--include-tests --include-inferred` answers of `modules list` and `cycles` (and the other readers it names) are byte-identical between the before-binary and the candidate, so a cycle that exists only through excluded partitions stays out of the default view as at HEAD."
      },
      "expected": "exit 0 and MDSS-C04-OK: leveldb `table` reads `Summary (module table, all directions):` with 46 / 19 / 0 and poco `Foundation` 1653 / 1751 / 27; every scoped JSON answer equals its literal in each direction and view, including the inferred-only `--include-inferred` figures; every preserved obligation this check carries is unchanged (no behavior change): the unscoped default `modules deps` human answers are byte-identical to the before captures (oracle (i), not a before-binary run), every line outside the scoped Summary block is unchanged, the repo-wide diagnostics keep today's values, the human answers of modules list, cycles, modules violations, trust, orient and modules show are byte-identical between the before-binary and the candidate on the same store, the before-roots are untouched, and the run's own directories and worktree are removed with no `CLEANUP FAILED` line; the partition behaviour of RG-REQ-004-L12 is unchanged: the default-view figures exclude the test-file and inferred imports bound in the literals, `--include-tests`, `--include-inferred` and both flags admit exactly their partitions, the scoped remainder lines (which state only excluded cross-module imports) are unchanged, and the default and both-flag answers of `modules list` and `cycles` are byte-identical to the before-binary"
    },
    {
      "checkId": "MDSS-C05",
      "obligationIds": [
        "RG-REQ-002-L03",
        "RG-REQ-002-L04",
        "RG-REQ-004-L01",
        "RG-REQ-004-L10",
        "P-MDSS-01",
        "P-MDSS-03",
        "P-MDSS-04"
      ],
      "owner": "reviewer",
      "method": {
        "kind": "inspection",
        "subject": "git diff HEAD -- the five candidate paths, read on the candidate tree",
        "criterion": "(1) the per-module counters are computed inside `derive_module_dependency_edges_in_view` in the same pass that computes the repo-wide counters — one computation (RG-REQ-004-L01); the map is written by the loop and read by nothing in the derivation, so the repo-wide counters, the edges, `edge_partitions` and the remainder are computed by the same statements as at HEAD; (2) `handle_modules_deps` takes X's intra-module and unowned counters from the derivation (never recounted from edges), applies the edge filter's predicate to every counter (outbound: source = X, so `imports_source_unowned` = 0; inbound: target = X, so `imports_target_unowned` = 0; all: both), takes the cross-module figure from the filtered edges' `import_count` sum, and sets `imports_total` to the sum of the four; a module absent from the map (no admitted intra-module or unowned-endpoint import adds to it) emits measured zeros for those three counters, never `null`, and its cross-module figure still comes from the filtered edges; (3) the only JSON change is the additive top-level `diagnostics_scope` key and, under a module, the scoped values of the existing keys; `modules violations` is untouched; (4) the renderer distinguishes the five decode cases of SLICE_DOC 2.1 item 3: scoped header in the `Queried:` label's words; unfiltered without the key or with `repo` byte-identical; repo-wide figures beside a module name headed `Summary (whole repository, not module X):`; `module` scope without a module name stating that reason; an unknown value printed as stored and marked — no case prints repo-wide figures under a plain `Summary:` beneath `Module: X`; (5) no `unwrap_or(0)`, `.ok()` swallow or default introduced on the rendering or decoding path (RG-REQ-002-L04); the new `diagnostics_scope` decode distinguishes absent (an older daemon: repo-wide) from an unknown value; (6) each phrase restates its predicate at the site named in SLICE_DOC 2.2 (D-AGENT-USEFULNESS-FRAME-1), and the contract section does the same; (7) import_partition_view.rs changes only inside its test module; dispatch.rs changes only at the diagnostics object of `handle_modules_deps` (the CLAUDE.md size guardrail); no new crate or module, no index-time change, no INDEXER_VERSION change.",
        "inputs": "The candidate's diff of the five paths; SLICE_DOC sections 2.1–2.3; D-MDSS-SCOPE-1; RG-REQ-002-L03/L04; RG-REQ-004-L01/L10."
      },
      "expected": "every criterion holds on the candidate; every preserved obligation this check carries is unchanged (no behavior change): the one module-edge computation, the unscoped output, the readers of the repo-wide diagnostics, the derivation outputs other than `per_module` and the edge rows are preserved"
    }
  ]
}
-->
## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)
Implements RG-REQ-002-L03 (every measured count states its universe) for the Summary block of `modules deps <module>`. Changes nothing ratified. Preserves the other ten Ls of RG-REQ-002 and every L of RG-REQ-004 (L01…L12).

**RG-REQ-004-L12 (preserved; ratified by D-TEST-SCOPE-1).** L12 governs the import view the changed derivation counts under: module edges are partitioned by the importing file's test status, and by resolution class through RG-REQ-002-L11; the default view is static/dynamic production imports and `--include-tests` and `--include-inferred` add their partitions. Excluded imports follow one rule (2.1 item 1): they add to no counter and no `import_count`; excluded cross-module imports stay in their relation's `edge_partitions` cell and add to the `imports` count of their partition's remainder group, stated with the flag that shows them; a relation with no admitted import adds one to the `edges` count of only the first group, in the order tests, inferred, tests-and-inferred, in which it has an excluded import (`import_partition.rs:429-436`); excluded intra-module and unowned-endpoint imports are in no partition cell and no remainder. Since INPUT-2 L12 is in both packets' obligation sets, in `preserves`, and in the `obligationIds` of MDSS-C01 and MDSS-C04; the view rule is a preservation statement of P-MDSS-03. The checks that hold it:
- MDSS-C01: the new `per_module_counters_count_only_the_imports_the_view_admits` and `derivation_outputs_other_than_per_module_equal_their_literals_in_every_view` on fixture F4 (one import in each of the four partitions, all four views, the inferred partition on its own), and the five existing TEST-EDGE-SCOPE-1B partition tests run by name with unchanged assertions (`module_edge_carries_its_four_partition_counts_and_the_unknown_tally`, `a_relation_in_two_partitions_counts_each_import_in_its_own_partition`, `view_edge_counts_only_admitted_imports_and_their_source_files`, `a_relation_only_through_excluded_imports_leaves_the_view_and_is_counted_in_the_remainder`, `partitions_of_a_pair_outside_the_result_is_absent_never_zero`).
- MDSS-C04: the field literals for `table` by default and with `--include-tests` (65 → 85 imports: 16 cross-module, which are the `+16` remainder, and 4 intra-module, in no remainder), `Foundation` by default and with `--include-inferred` (the inferred-only witness, `imports_target_unowned` 15 → 31) and with both flags; the scoped remainder line unchanged; the default and `--include-tests --include-inferred` answers of `modules list` and `cycles` byte-identical to the before-binary.

| P-id | Statement | Checks |
|---|---|---|
| P-MDSS-01 | One module-edge computation (RG-REQ-004-L01). The derivation returns five repo-wide counters, the edges and the remainder; their values are unchanged on every fixture. Trust reads the edges of the same derivation (`storage/src/trust_impl.rs:888`). `modules violations` reads its repo-wide diagnostics (`daemon-runtime/src/dispatch.rs:8688-8696`, through `module-queries/src/violations.rs:103/:166`). Both read as before. Every existing module_edges test is kept by name with unchanged assertions. | MDSS-C01, MDSS-C02, MDSS-C05 |
| P-MDSS-02 | Unscoped `modules deps` (no module) is byte-identical in human text. Its JSON is identical except for the additive `diagnostics_scope: "repo"` key. An unfiltered envelope from an older daemon (no `diagnostics_scope`) renders as today. | MDSS-C02, MDSS-C04 |
| P-MDSS-03 | Under a module filter, each line outside the Summary block is unchanged: `Queried:`, `Module:`, the edge rows, the remainder and the undetermined block. Each JSON key other than `diagnostics` and `diagnostics_scope` is unchanged. Under each `--include-tests` / `--include-inferred` view, the module's counters count exactly the imports that view admits, partition by partition. The view rule itself is RG-REQ-004-L12's and is unchanged: the default view admits static/dynamic production imports (an import whose test status is unknown is admitted with production), `--include-tests` adds the test certain partition, `--include-inferred` the production inferred partition, both flags all four. An excluded import adds to no counter and no edge's `import_count`; an excluded cross-module import stays in its relation's `edge_partitions` cell and adds to the `imports` count of its partition's remainder group; a relation with no admitted import adds one to the `edges` count of only the first group, in the order tests, inferred, tests-and-inferred, in which it has an excluded import (`import_partition.rs:429-436`); an excluded intra-module or unowned-endpoint import is in no partition cell and no remainder (2.1 item 1). A module-filtered envelope whose figures are not scoped to the module renders them under `Summary (whole repository, not module X):`. It never renders them under a plain `Summary:` beneath `Module: X` (review-0 F-1). | MDSS-C01, MDSS-C02, MDSS-C04, MDSS-C05 |
| P-MDSS-04 | Precise derived facts (review-0 F-3), in every view: the derivation's outputs other than the new `per_module` map are value-identical. These are the edges with `import_count` and `source_file_count`, `edge_partitions`, the remainder and the five repo-wide counters, and they are the outputs that `modules list`, `modules show`, `cycles`, trust, orient and `modules violations` consume. Proof: (i) source inspection shows the map is written by the loop and read by nothing in the derivation (MDSS-C05 (1)); (ii) hand-written literals on the four-partition fixture F4 under all four views (MDSS-C01); (iii) a before-binary built from HEAD and the candidate serve copies of the same before-store, and their human answers to those readers are byte-identical on leveldb and poco (MDSS-C04 (iv)). Unchanged files outside the candidate paths are scope control (MDSS-C03), not the oracle. | MDSS-C01, MDSS-C04, MDSS-C05 |
| P-MDSS-05 | No index-time change: no store is written. The before-roots are never served in place. | MDSS-C03, MDSS-C04 |

## 1. Problem (ROOT-CAUSED — RC-3, `docs/audits/2026-10-03-root-causes-v0.20.0.md`; re-verified 2026-10-04 on isolated copies of the retained v0.20.0 stores)
Code-under-analysis example — leveldb `rmap modules deps table`, as captured with the HEAD f69759f4 binary (`.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/before/leveldb-table-before.txt`):

```
Queried: all directions
Module: table

Summary:
  185 cross-module dependencies
  140 intra-module imports
  18 imports from unowned sources

4 dependency edges

  db -> table  (5 imports from 2 files)
  table -> include  (30 imports from 15 files)
  table -> port  (1 imports from 1 files)
  table -> util  (10 imports from 7 files)
+16 imports from test files, not shown (1 cross-module dependency only through them) — --include-tests
```

The three Summary figures are the REPO-WIDE ones: unfiltered `modules deps` prints the same three (`before/leveldb-all.txt`). The four rows below sum to 46. poco `modules deps Foundation` prints `2006 cross-module dependencies` / `4624 intra-module imports` / `1207 imports from unowned sources` above 24 rows that sum to 1653 (`before/poco-foundation-before.txt`). An agent that asks about one module reads the repository's totals under that module's name. The defect has been present since the module filter was added, because the Summary block is older than the filter.

Cause, verified on HEAD 4e7bd983 (the code under `rust/` is identical to f69759f4):
- `classification/src/module_edges.rs:218-264`, the import loop of `derive_module_dependency_edges_in_view`, counts each admitted import once, repo-wide. It drops intra-module and unowned imports with a `continue` at `:231`, `:242` and `:251`:
  ```rust
          let source_module = match ownership_index.get(import.source_file_uid.as_str()) {
              Some(m) => *m,
              None => {
                  if admitted {
                      diagnostics.imports_source_unowned += 1;
                  }
                  continue;
              }
          };
          …
          let target_module = match ownership_index.get(import.target_file_uid.as_str()) {
              Some(m) => *m,
              None => {
                  if admitted {
                      diagnostics.imports_target_unowned += 1;
                  }
                  continue;
              }
          };
          …
          if source_module == target_module {
              if admitted {
                  diagnostics.imports_intra_module += 1;
              }
              continue;
          }
  ```
  Because these imports never reach an edge, the handler cannot recount them per module from the edges.
- `module-queries/src/facts.rs:397` returns the counters as `facts.diagnostics`.
- `daemon-runtime/src/dispatch.rs`, `handle_modules_deps`, filters `facts.edges` by module and direction (`:8471-8487`). It then copies the counters unfiltered (`:8518-8524`):
  ```rust
              "diagnostics": {
                  "imports_total": facts.diagnostics.imports_total,
                  "imports_cross_module": facts.diagnostics.imports_cross_module,
                  "imports_intra_module": facts.diagnostics.imports_intra_module,
                  "imports_source_unowned": facts.diagnostics.imports_source_unowned,
                  "imports_target_unowned": facts.diagnostics.imports_target_unowned,
              },
  ```
- `rgr/src/presentation/modules_deps.rs:133-151` prints `Module: {}` and then the Summary from those counters:
  ```rust
          if let Some(ref module) = self.module {
              out.push_str(&format!("Module: {}\n", module));
          }

          // -- Summary from diagnostics --
          if let Some(ref diag) = self.diagnostics {
              out.push_str("\nSummary:\n");
              out.push_str(&format!(
                  "  {} cross-module dependencies\n",
                  diag.cross_module_edges
              ));
              out.push_str(&format!(
                  "  {} intra-module imports\n",
                  diag.intra_module_edges
              ));
              out.push_str(&format!(
                  "  {} imports from unowned sources\n",
                  diag.from_unowned_edges
              ));
  ```

Readers of `ModuleEdgeDiagnostics` (`git grep -n 'ModuleEdgeDiagnostics'` plus a search for field reads, 2026-10-04): the type is declared at `module_edges.rs:160` and built only by `::default()` at `:215`. No struct literal exists anywhere, so an additive field breaks no construction. It is carried by `module-queries/src/facts.rs:97` and `module-queries/src/violations.rs:29` (cloned at `:103`/`:166`; an additive field is cloned with the struct). Its fields are read only at `dispatch.rs:8519-8523` (`modules deps`) and `:8689-8695` (`modules violations`). Trust and orient do not read it: `storage/src/trust_impl.rs:888` calls the same derivation and reads `.edges` only. The rgr integration tests that touch `modules deps` JSON (`rgr/tests/daemon_dispatch.rs:3577`, `:3827`; `rgr/tests/cli_out_4_modules.rs:919`) assert only that the `diagnostics` key is present.

## 2. Contract
### 2.1 The fix, at the cause (D-MDSS-SCOPE-1, option A)
1. **Derivation** (`module_edges.rs`, the import loop). `ModuleEdgeDiagnostics` keeps its five repo-wide counters unchanged and gains `per_module: BTreeMap<String /* canonical path */, ModuleScopedCounts { intra_module, source_unowned_into, target_unowned_from }>` (all `u64`). The derivation fills the map in the same pass, over the imports the view admits:
   - an import whose source and target are owned by X adds 1 to X.intra_module;
   - an import with an unowned source into a file owned by X adds 1 to X.source_unowned_into (the source-unowned branch also looks up the target; it still `continue`s, and the repo-wide counter does not change);
   - an import from a file owned by X to an unowned target adds 1 to X.target_unowned_from;
   - an import whose source and target are both unowned adds to no entry.

   The key is the canonical path from `module_lookup`, which is the key that the handler's edge filter compares (`dispatch.rs:8472-8486`). Cross-module imports are already on the edges (`import_count`), so the map does not duplicate them. The map holds an entry for X only when an admitted import adds to one of X's three counters; a module whose admitted imports are all cross-module has no entry, and its cross-module figure comes from the filtered edges, never from the map.

   **Excluded imports (the rule this slice keeps, RG-REQ-004-L12 and RG-REQ-002-L11; `module_edges.rs:218-268`, `import_partition.rs:420-439`).** An import whose partition the view does not admit adds to no diagnostics counter, repo-wide or per-module, and to no edge's `import_count` or `source_file_count`. An excluded CROSS-MODULE import (both files owned, by different modules) is still counted in its relation's partition cell, which `edge_partitions` carries when the relation has at least one admitted import, and adds to the `imports` count of its partition's remainder group (the group whose flags would show it). The remainder's `edges` count is per relation, not per import: a relation with no admitted import adds one to the `edges` count of exactly one group, the first group in the order tests, inferred, tests-and-inferred in which the relation has an excluded import (`import_partition.rs:429-436`), so a relation whose excluded imports fall in two groups (for example one test certain and one production inferred import) adds to both groups' `imports` and only to the tests group's `edges`. An excluded intra-module import, or an excluded import with an unowned endpoint, leaves the loop before the aggregate (the three `continue`s at `:231/:242/:251`), so it is in no partition cell and in no remainder. The remainder line therefore states excluded cross-module imports only: leveldb `table`'s `+16 imports from test files` is its 16 cross-module test imports, and its 4 intra-module test imports are stated nowhere in the default view. `Default` makes an empty map, and no struct literal needs to change (§1).
2. **Handler** (`dispatch.rs`, `handle_modules_deps`). The change is in place, at the `"diagnostics"` object (`:8518-8524`) and its response. dispatch.rs is over the CLAUDE.md size guardrail, so it gets no new helper and no new test.
   - With a resolved module X, every counter counts the admitted imports that the answer's edge filter selects. The **direction rule** is the edge filter's own predicate: outbound means source = X, inbound means target = X, and all means either.
   - The resulting values are:
     - `imports_cross_module` = Σ `import_count` of the filtered edges;
     - `imports_intra_module` = X.intra_module, under every direction, because an intra-module import has source = target = X;
     - `imports_source_unowned` = X.source_unowned_into under all and inbound, and 0 under outbound (an outbound import of X has source X, so its source is never unowned);
     - `imports_target_unowned` = X.target_unowned_from under all and outbound, and 0 under inbound;
     - `imports_total` = the sum of the four.
   - The answer gains `diagnostics_scope: "module"`.
   - Without a module, the five values are today's and the answer gains `diagnostics_scope: "repo"`.
   - A module absent from the map has no admitted intra-module import and is the owned endpoint of no admitted import with an unowned endpoint, because the loop adds every such import to its owner's entry. Its three map-derived counters are therefore measured zeros, never `null`. The absence says nothing about the module's cross-module imports, which come from the filtered edges' `import_count`.
   - `modules violations` (`:8688-8696`) is untouched.
3. **Renderer** (`modules_deps.rs`). `ModulesDepsResponse` decodes `diagnostics_scope: Option<String>`, and `ImportDiagnostics` stays as it is. The header is decided by the pair (`diagnostics_scope`, `module`). There are five cases, each with its own rgr test (MDSS-C02) and each phrase in 2.2 (review-0 F-1):

   | `diagnostics_scope` | `module` | Header | Why |
   |---|---|---|---|
   | `module` | X | `Summary (module X, <direction label>):` | the figures are X's. The label uses the `Queried:` line's words (`:126-130`). |
   | absent or `repo` | absent | `Summary:` (byte-identical to today) | the whole repository; P-MDSS-02 |
   | absent or `repo` | X | `Summary (whole repository, not module X):` | the figures are repo-wide. A daemon that sends no `diagnostics_scope` predates this slice, and every such daemon copies the unfiltered `facts.diagnostics` (`dispatch.rs:8518-8524` at 4e7bd983). The block keeps them and states their universe. It never prints them as X's under a plain `Summary:` and never under the scoped header. |
   | `module` | absent | `Summary (diagnostics scope "module" but the answer names no module):` | a known scope whose subject the answer omits. The figures are printed and their module is not guessed. |
   | any other value | either | `Summary (diagnostics scope "{value}" — not a scope this build reads):` | a value with no phrase in this build, printed as stored (VISION Honesty Rules) |
4. **Tests' home.** The five new daemon tests go in the `#[cfg(test)] mod tests` of `daemon-runtime/src/import_partition_view.rs` (`:527-528` onward). That module holds the only dispatch fixture for `modules_deps` and `modules_violations` (`mini_leveldb`, `:812`; existing `modules_deps_include_tests_restores_test_imports`, `:910`). A test may extend the fixture there. No non-test line of that file changes (MDSS-C03).
5. **Contract** (`docs/cli/rmap-contracts.md`). Add a new section `### \`modules deps\` — module-scoped Summary (MODULES-DEPS-SUMMARY-SCOPE-1)` under "Command-Specific Contracts", beside `modules show` / `modules violations`. No `modules deps` section exists today. The section states:
   - under a module filter, the Summary counts the imports that the direction selects (the predicates of 2.2), with the universe in its header;
   - `diagnostics_scope` is `module` or `repo`;
   - without a filter, the block and the values are unchanged.

### 2.2 Wording (binding; D-AGENT-USEFULNESS-FRAME-1: each phrase restates the predicate at its site)
| Phrase | Predicate it restates | Site |
|---|---|---|
| `Summary (module {X}, all directions):` / `…, outbound only):` / `…, inbound only):` | every figure below counts the admitted imports, in the request's view, whose source is X (outbound), whose target is X (inbound), or either (all) | the edge filter `dispatch.rs:8472-8486`, applied to every counter by 2.1 item 2; the labels at `modules_deps.rs:126-130` |
| `{N} cross-module dependencies` (words unchanged) | imports whose source and target files are owned by different modules | `module_edges.rs:254-263` |
| `{N} intra-module imports` (unchanged) | imports whose source and target files are owned by the same module | `module_edges.rs:247-252` |
| `{N} imports from unowned sources` (unchanged) | imports whose source file has no module owner | `module_edges.rs:225-233` |
| `Summary (whole repository, not module {X}):` | the answer names module X and its `diagnostics_scope` is `repo`, or absent. Absent comes from a daemon older than this slice, whose `diagnostics` are the unfiltered derivation counters | the decode in `modules_deps.rs`; the older daemon's copy `dispatch.rs:8518-8524` at 4e7bd983 |
| `Summary (diagnostics scope "module" but the answer names no module):` | the answer's `diagnostics_scope` is `module` and it carries no `module` key | the decode in `modules_deps.rs` |
| `Summary (diagnostics scope "{value}" — not a scope this build reads):` | the answer carried a `diagnostics_scope` that this build has no phrase for; the value is printed as stored (VISION Honesty Rules) | the decode in `modules_deps.rs` |
| `Summary:` (unscoped, byte-identical) | the whole repository, stated by the absence of a `Module:` line and by `Queried:` | the human may rule `Summary (repo-wide):` instead (D-MDSS-SCOPE-1) |

The direction rule is the author's decision under option A, recorded here and overridable. D-MDSS-SCOPE-1 says how direction applies only to the cross-module figure. If intra and unowned counters ignored the direction, `Summary (module Foundation, outbound only):` would print `27 imports from unowned sources`, and all 27 of those imports point INTO Foundation. The header's predicate would then be false (D-AGENT-USEFULNESS-FRAME-1). The edge filter's predicate makes the header true for every line.

### 2.3 Evidence taxonomy
- Per-module counters, MDSS-C01's five tests:
  - intra, unowned-into and unowned-from;
  - each of the four partitions under each of the four views on fixture F4 (production/test × certain/inferred; the inferred partition on its own);
  - every other derivation output equal to its hand-written literal in every view.
- Handler, MDSS-C02's five daemon tests:
  - the module answer and its scope;
  - the cross-module figure equals the edge sum per direction;
  - the direction rule on the other counters;
  - no module gives today's values;
  - violations unchanged.
- Renderer: the five decode cases of 2.1 item 3, as MDSS-C02's five rgr tests, plus the three existing tests by name.
- Field, MDSS-C04:
  - literals per direction and view, including `--include-inferred`;
  - every line outside the Summary block unchanged;
  - the before-binary comparison of `modules list`, `cycles`, `modules violations`, trust, orient and `modules show`.
- Placement, single computation (the map is written and never read), phrases, swallow forms: MDSS-C05.

## 3. Regression watch
The repo-wide counters keep their values: `modules violations` reads them and so does the unscoped `modules deps` (MDSS-C02, MDSS-C04). Trust reads only the derivation's edges (`trust_impl.rs:888`). Orient reads `load_module_graph_facts`' edges (`orient_additive_fields.rs:155`, `:359`) and not the diagnostics. `module_edges.rs` is the shared producer of those readers, so their answers are proven unchanged by output, not by file identity. MDSS-C01 holds the derivation literals in all four views. MDSS-C04 (iv) compares the before-binary and the candidate on the same stores. JSON consumers of `modules deps`: without a module, the existing keys keep their values. Under a module, their values are the module's, and the additive `diagnostics_scope` key states that. The in-repo consumers assert only that the key is present (§1).

## 4. Stop conditions
Frozen: the derivation's edges and remainder, each reader except `modules deps`, storage, the indexer, `INDEXER_VERSION` and the witness manifests. STOP and report (DECISION_REQUIRED with options, reward and risk) if any of these occur:
- (a) the module-scoped cross-module figure cannot equal the filtered edges' sum under some direction (state why);
- (b) the per-module counters cannot be computed in the derivation's single pass without changing the edges or the remainder;
- (c) MDSS-C04's unscoped text differs from the before capture, or a line outside the scoped Summary block changes;
- (d) the daemon's envelope has a consumer that breaks on the additive key;
- (e) the daemon tests cannot reach the dispatch fixture without changing non-test code of `import_partition_view.rs`;
- (f) the consolidation witness fails. That is a STOP (its own text says so), never a manifest edit;
- (g) a reader's answer differs between the before-binary and the candidate in MDSS-C04 (iv). Report the diff and its cause. If the cause is run-to-run variation rather than the candidate, run the before-binary twice to show it; never narrow the compared set to make it pass.

## 5. Validation (ORDERED; write `.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/build-progress.md` after EACH step)
0. Record `git rev-parse HEAD` and confirm that the admission commit carries this document, its manifest and `D-MDSS-SCOPE-1.md`.
1. FIRST, the new tests of MDSS-C01 and MDSS-C02, each failing for the intended reason.
2. The fix (§2.1 items 1–4).
3. MDSS-C01, MDSS-C02 and MDSS-C03, verbatim from `rust/`, in the FOREGROUND of the turn.
4. The contract (item 5), then MDSS-C03 again.
5. MDSS-C04 verbatim. It serves copies and runs no index. It builds the before-binary once in a detached worktree at a fresh unique path (`mktemp -d /private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-before-src.XXXXXX`, then `/wt`). Its EXIT trap removes the worktree only if this run created it, and removes the run directory and the unique parent. A failed removal prints `CLEANUP FAILED: <path>` and fails the check; it is never reported as success. The working tree is never stashed or reset.
6. The hand-off:
   - every check by id, with its log under `.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/logs/` or `after/` and the line that proves it;
   - the before and after `table` and `Foundation` blocks;
   - the code-under-analysis quotes of §6.

MDSS-C05 is the reviewer's check. Never end the turn while a check runs.

## 6. Definition of done
MDSS-C01–C04 pass on the candidate (BUILDER-EXECUTED, logs named), and the reviewer records MDSS-C05.

Code-under-analysis outcome:
- leveldb `modules deps table`:
  - before: `Summary:` / `185 cross-module dependencies` / `140 intra-module imports` / `18 imports from unowned sources` under `Module: table`, whose 4 edges (`db -> table` 5, `table -> include` 30, `table -> port` 1, `table -> util` 10) sum to 46;
  - after: `Summary (module table, all directions):` / `46 cross-module dependencies` / `19 intra-module imports` / `0 imports from unowned sources`, with the edge rows and the `+16 imports from test files` remainder unchanged.
- poco `modules deps Foundation`: 2006 / 4624 / 1207 → 1653 / 1751 / 27.
- poco inferred-only witness: `Foundation/src/NumericString.cpp:27` `#include "bignum-dtoa.cc"`. It is stored `inferred` (basis `unique_basename`) into `dependencies/v8_double_conversion/src/bignum-dtoa.cc`, which no module owns. It is one of 16 such imports from Foundation. They count in Foundation's `imports_target_unowned` only under `--include-inferred` (15 → 31).
- `modules deps` with no filter is byte-identical on both repositories.
- The readers of the shared derivation answer identically to the before-binary on the same stores.
- The report quotes `dispatch.rs:8518-8524` before and after.

## 7. Corpus roots
Before-roots: `/private/tmp/MODULES-DEPS-SUMMARY-SCOPE-1-{leveldb,poco}-before`.
- They are byte copies of the retained v0.20.0 stores: `1196d1380537d43e.db` sha256 c5bb3e8fe56740f4…, latest ready snapshot `repo_01m3zqy350dh5xfat56bpetr8y/2026-10-03T02:01:51.642Z/3370d3a9`; `9858deeeefef8599.db` d23bffde8b4889c3…, snapshot `repo_01m3zr1g2fe63scy1rez9rszkb/2026-10-03T02:03:43.700Z/10963128`.
- Each has one registry entry. Durable copies are in `~/repo-graph-retained/MODULES-DEPS-SUMMARY-SCOPE-1-before/`.
- HEAD captures and digests are in `.agent-manager/slices/MODULES-DEPS-SUMMARY-SCOPE-1/before/`.

Rules:
- The roots are never served in place: MDSS-C04 copies them and repoints the copies' registry.
- The operator's registry is never read or written.

## 8. Follow-ups (not this slice)
- `Summary (repo-wide):` for the unscoped block, if the human rules it (D-MDSS-SCOPE-1).
- The line `N cross-module dependencies` counts IMPORTS that cross module boundaries (`imports_cross_module`), not dependency edges: beside `4 dependency edges` it reads `46 … dependencies`. The words are left unchanged here because changing them breaks the unscoped byte-identity this slice keeps. A wording slice can restate the predicate (`N imports across module boundaries`).
- The human block does not render `imports_target_unowned` (unchanged here).
- `ImportDiagnostics` (`modules_deps.rs:28-38`) decodes each counter with `#[serde(default)]`, so a missing key renders 0. This form exists before this slice and is adjacent to RG-REQ-002-L04. This slice does not add to it.

## 9. Baseline history and revision notes
INPUT-1 (2026-10-04).
- The manager wrote it from RC-3 and verified it live on the before-root copies (HEAD f69759f4).
- The document author (the PREP item) then revised it after verifying each anchor on HEAD 4e7bd983. The changes:
  - line anchors corrected: `:8471-8487` / `:8518-8524` / `:218-264`, continues at `:231/:242/:251`, `modules_deps.rs:133-151`;
  - the claim that trust and orient read the diagnostics is removed: trust reads `.edges` only, at `trust_impl.rs:888`;
  - `import_partition_view.rs` (test module only) added to the allocation, because it holds the only `modules_deps` dispatch fixture and dispatch.rs is over the size guardrail;
  - the consolidation witness added to MDSS-C02, as CLAUDE.md requires for a daemon crate;
  - `cargo fmt` set per touched crate;
  - the direction rule added (2.1 item 2, 2.2) with a fifth daemon test;
  - the unknown-scope marking added with a third rgr test;
  - MDSS-C04's `--direction <d>` corrected to `--outbound` / `--inbound` (`rgr/src/commands/modules/deps.rs:184-185` rejects `--direction` as an unknown flag);
  - the scoped figures bound as literals, computed by a read-only SQL re-implementation of the derivation that reproduces the captured repo-wide figures and edge sums exactly;
  - a line-by-line check outside the Summary block added;
  - a new contract section instead of "the `modules deps` section", which does not exist;
  - RG-REQ-004-L12's absence stated (§0).
- Revision 1 (2026-10-04), after review-0 (refinement-required):
  - F-1: the renderer decides the header from (`diagnostics_scope`, `module`) in five cases. A module-filtered answer from an older daemon is headed `Summary (whole repository, not module X):` instead of keeping the defect. `module` scope without a module states that reason. There are five rgr tests; the rgr floor is 1478.
  - F-2: L12 cannot enter the allocation until the operator adds it to the packet sets (validator EXECUTED). §0 lists the exact follow-on edit. The proof is widened: fixture F4 now covers all four partitions under all four views, with the inferred partition on its own, and MDSS-C04 adds the `--include-inferred` field literals on poco Foundation, including the inferred-only witness.
  - F-3: P-MDSS-04 is narrowed to the derivation's outputs other than `per_module`, proven three ways: source inspection (the map is never read), hand-written literals in four views (`derivation_outputs_other_than_per_module_equal_their_literals_in_every_view`, replacing `repo_wide_diagnostics_are_unchanged_beside_the_per_module_counters`), and a before-binary comparison of the readers' answers on the same stores (MDSS-C04 (iv)). MDSS-C03's file identity is now stated as scope control only.
- Revision 2 (2026-10-04), after review-1 (refinement-required):
  - Review-1 F-2: MDSS-C04's worktree is created at a unique `mktemp -d` path. The trap removes it only after `git worktree add` succeeded (`WT_CREATED=1`). A failed removal is printed as `CLEANUP FAILED` and turns exit 0 into exit 1; the previous `|| true` is gone. EXECUTED on a throwaway repository: success → exit 0 with nothing left; a failing `worktree add` → exit 128 with nothing removed that the run did not create; a locked worktree → `CLEANUP FAILED` lines and exit 1. The rule that closes the class: a proof's trap removes only paths recorded as created by this run, and every removal failure is a check failure.
  - Review-1 F-1: `D-TEST-SCOPE-1` is now pinned in the manifest (governance and `requiredDecisionIds`). RG-REQ-004-L12 itself still cannot enter `preserves`, because the packets' obligation sets lack it; that is an operator action (§0).
INPUT-2 (2026-10-04): the operator's packet omitted the ratified RG-REQ-004-L12 (module edges partitioned by the importing file's test status; the default view is production) from both obligation sets — a manager error (the id lists were written as L01..L11 without reading the requirement's leaves); review-2 (decision MDSS-L12-TRACE, option A). INPUT-2 adds L12 to the review and implementation sets, to `preserves`, to MDSS-C01/MDSS-C04's obligations, and pins its ratifying record D-TEST-SCOPE-1; the author binds L12's preservation to the partition tests already named.
- INPUT-2 author revision (2026-10-04), after review-2 (MDSS-L12-TRACE → A): RG-REQ-004-L12's preservation is bound in MDSS-C01 and MDSS-C04. MDSS-C01's `inputs` name what the new partition tests assert per view and add the five existing TEST-EDGE-SCOPE-1B partition tests (present verbatim at HEAD 4e7bd983) to the command's by-name list; MDSS-C04's `inputs` tie the `--include-tests` and `--include-inferred` literals, the remainder line and the before-binary comparison to the view rule; both `expected` say the partition behaviour is unchanged. The P-table names L12 in P-MDSS-03 (the row that already states the per-view counting) instead of a new row, so the P-id set and every check's obligation list stay as allocated. §0's open item and §8's operator bullet are replaced by the resolved statement; the history above is kept as written.
- Revision 4 (2026-10-04), after review-3 (refinement-required; both findings are one class — a count or remainder described by more than the population the derivation records). The rule that closes it: every count, map entry and remainder is described by the exact population the loop records at `module_edges.rs:218-268` (`import_partition.rs:420-439` for the remainder). F-MDSS-01: a missing `per_module` entry now means zero admitted intra-module and unowned-endpoint imports for that module, not zero imports (2.1 items 1 and 2); MDSS-C01 `expected` names entries for exactly the modules those imports add to, and the intra test asserts that F4's module A (cross-module imports only) has no entry; the cross-module figure stays on the filtered edges. F-MDSS-02: the excluded-import rule is stated once in 2.1 item 1 and applied verbatim in MDSS-C01 `inputs`/`expected`, MDSS-C04 `inputs`, §0 and P-MDSS-03, and MDSS-C05 criterion (2) carries the same map-entry meaning — excluded imports add to no counter and no `import_count`; excluded cross-module imports stay in `edge_partitions` and the remainder; excluded intra-module and unowned-endpoint imports are in neither. The field figures confirm it: `table` `--include-tests` +16 cross-module (the `+16` remainder) and +4 intra-module (no remainder); `Foundation` both flags +841 cross-module (the `+841` remainder) and +1042 intra-module (no remainder). The four-view literal checks stay.
- Revision 5 (2026-10-04), after review-4 (refinement-required, F-MDSS-02 residual; the same class as revision 4: a remainder described by more than the population the derivation records). The rule that closes it: the remainder's `imports` count is per import and its `edges` count is per relation. Each excluded cross-module import adds to the `imports` count of its partition's group. A relation with no admitted import adds one to the `edges` count of only the first group, in the order tests, inferred, tests-and-inferred, in which it has an excluded import (`import_partition.rs:429-436`, `ImportRemainder::from_relations`). This is now stated in 2.1 item 1, MDSS-C01 `inputs` (twice) and `expected`, MDSS-C04 `inputs`, the §0 L12 paragraph and P-MDSS-03. MDSS-C04's comparison sentence is corrected: oracle (i) compares only the unscoped default `modules deps` human text with the manager's before capture; oracle (iv) compares `modules list`, `cycles` (default and both flags) and the other named readers between the before-binary and the candidate; no both-flag `modules deps` answer is compared; the scoped default Summary is the intended change (oracle (ii)). MDSS-C04 `expected` names oracle (i)'s subject in the same way. No test, field figure, command, check id or path changed.
