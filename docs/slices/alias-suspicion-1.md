<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "ALIAS-SUSPICION-1",
  "baselinePath": "docs/requirements/baselines/ALIAS-SUSPICION-1-INPUT-2.json",
  "parentRequirementIds": [
    "RG-REQ-002",
    "RG-REQ-009",
    "RG-REQ-011"
  ],
  "implements": [
    "RG-REQ-009-L04",
    "RG-REQ-002-L08"
  ],
  "preserves": [
    "RG-REQ-009-L02",
    "RG-REQ-009-L06",
    "RG-REQ-009-L07",
    "RG-REQ-011-L06"
  ],
  "preservationObligationIds": [
    "P-AS-01",
    "P-AS-02",
    "P-AS-03",
    "P-AS-04"
  ],
  "changes": [],
  "acceptanceBoundary": "The `trust` human and JSON outputs (and `deps list`) of the candidate rmap against the before binary on four pre-provisioned isolated `-before` roots (FRAKTAG, kafka, vcmi, repo-graph itself) and on four fresh `-after` indexes built by the candidate, plus cargo test on repo-graph-trust (whole, incl. the parity corpus), repo-graph-storage (--lib), repo-graph-rgr (--lib whole and the trust_module_edges_seam integration test), repo-graph-agent (--lib reliability) and repo-graph-daemon-runtime (--lib trust_coherence), as named per check.",
  "candidatePaths": [
    "rust/crates/trust/src/storage_port.rs",
    "rust/crates/storage/src/trust_impl.rs",
    "rust/crates/trust/src/rules.rs",
    "rust/crates/trust/src/service.rs",
    "rust/crates/trust/src/types.rs",
    "rust/crates/trust/tests/parity.rs",
    "trust-parity-fixtures/rules__detect-alias-resolution__triggered/input.json",
    "trust-parity-fixtures/rules__detect-alias-resolution__triggered/expected.json",
    "trust-parity-fixtures/rules__detect-alias-resolution__not-triggered/input.json",
    "trust-parity-fixtures/rules__detect-alias-resolution__not-triggered/expected.json",
    "trust-parity-fixtures/report__diagnostics-with-calls/expected.json",
    "rust/crates/rgr/src/presentation/trust.rs",
    "rust/crates/rgr/src/presentation/trust_tests.rs",
    "rust/crates/rgr/tests/trust_module_edges_seam.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/ALIAS-SUSPICION-1/verification.json",
    "docs/assurance/ALIAS-SUSPICION-1/implementation-review.json"
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
      "checkId": "AS-C01",
      "obligationIds": [
        "RG-REQ-009-L04",
        "RG-REQ-009-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-storage --lib trust_impl 2>&1 | tee /tmp/as-c01.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c01.txt && for t in compute_module_stats_counts_unresolved_alias_imports_per_module compute_module_stats_empty_snapshot compute_module_stats_returns_fan_in_fan_out_file_count compute_module_stats_excludes_modules_with_no_owned_files; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c01.txt || { echo \"MISSING $t\"; exit 1; }; done && ! git diff HEAD -- crates/storage/src/trust_impl.rs | grep -E \"^-.*resolution = 'static'\"",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/storage/src/trust_impl.rs `compute_module_stats` (:755-852): the derived-edge CTEs (`file_owner`, `resolved_imports` over `e.resolution = 'static'`, `module_pairs`) are UNCHANGED (the negated grep: no removed line touches the resolved-imports predicate); ONE more LEFT JOIN — a CTE counting `unresolved_edges` rows with `type = 'IMPORTS'` and `basis_code = 'specifier_matches_project_alias'` per `module_file_ownership.module_candidate_uid` — fills the new `TrustModuleStats.alias_unresolved_imports: u64` (rust/crates/trust/src/storage_port.rs:46-53; the two literal sites :396 and trust_impl.rs:842 and the six service.rs fixture literals gain the field). NEW test `compute_module_stats_counts_unresolved_alias_imports_per_module` (beside the three existing `compute_module_stats_*` tests at :1602-1775, using the `unresolved_edges` insert helpers at :1140/:1176): two modules with owned files, one with two alias-basis unresolved IMPORTS rows and one relative-import row → `alias_unresolved_imports` 2 and 0; fans/file_count unchanged"
      },
      "expected": "exit 0: the module stats read carries, per module, how many imports failed through a project alias — from the same ownership join the fans use (RG-REQ-009-L02: one edge set, one ownership) — with the resolved-edge derivation byte-identical (no behavior change to fan-in/fan-out/file_count)"
    },
    {
      "checkId": "AS-C02",
      "obligationIds": [
        "RG-REQ-009-L04",
        "RG-REQ-009-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-trust --lib rules 2>&1 | tee /tmp/as-c02.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c02.txt && for t in alias_suspicion_fires_on_one_alias_isolated_module alias_suspicion_silent_when_isolated_modules_have_no_failed_alias_imports alias_suspicion_reason_names_each_module_with_its_count_in_path_order zero_connectivity_predicate_excludes_root_empty_and_single_file_modules count_suspicious_modules_matches_all_criteria import_low_with_alias_suspicion change_low_if_alias_suspicion; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c02.txt || { echo \"MISSING $t\"; exit 1; }; done && ! git grep -qE 'fn alias_suspicion_triggered_at_3|fn alias_suspicion_not_triggered_below_3' -- crates/trust",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/trust/src/rules.rs: `ModuleForSuspicionCheck` (:407-413) gains `alias_unresolved_imports: u64`; the five-condition zero-connectivity test becomes ONE pure function `is_zero_connectivity(&ModuleForSuspicionCheck) -> bool` (fan 0/0, file_count ≥ 2, name not `.`/empty) used by `count_suspicious_zero_connectivity_modules` (:392-405, unchanged semantics) AND by the service's row flag (AS-C03) — the duplicated predicate at service.rs:529-533 is removed; NEW `alias_isolated_modules(&[ModuleForSuspicionCheck]) -> Vec<(String, u64)>` = the zero-connectivity modules with `alias_unresolved_imports ≥ 1`, sorted by path; `detect_alias_resolution_suspicion` (:190-207) now takes that list: triggered iff it is non-empty, ONE reason `alias_isolated_modules=<path>(<n>)[,<path>(<n>)…]` (the bare `>= 3` count rule at :194 and the reason `suspicious_zero_connectivity_modules=N` are retired — a reason must name its cause, RG-REQ-009-L04). Tests: the two retired names (`alias_suspicion_triggered_at_3`, `…not_triggered_below_3`) are REPLACED by the four new names (the negated grep proves the old identities are gone — never keep a false name); `count_suspicious_modules_matches_all_criteria` (:861; its five literals gain the field) stays; `import_low_with_alias_suspicion` / `change_low_if_alias_suspicion` (bool inputs) unchanged"
      },
      "expected": "exit 0: the alias downgrade fires only when a zero-connectivity module has imports that failed through a project alias, names every such module with its count, and stays silent for modules that are merely isolated; the zero-connectivity predicate exists once (RG-REQ-009-L02's list and the downgrade can no longer disagree)"
    },
    {
      "checkId": "AS-C03",
      "obligationIds": [
        "RG-REQ-009-L04",
        "RG-REQ-009-L02",
        "RG-REQ-002-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-trust --lib service 2>&1 | tee /tmp/as-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c03.txt && for t in module_row_note_is_alias_candidate_only_with_failed_alias_imports module_row_note_is_isolated_without_failed_alias_imports module_zero_connectivity_flag_uses_the_shared_predicate root_module_not_flagged_suspicious; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c03.txt || { echo \"MISSING $t\"; exit 1; }; done && ! git grep -qE 'fn module_suspicious_zero_connectivity_flagged' -- crates/trust && ! grep -nE 'm\\.fan_in == 0' crates/trust/src/service.rs",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/trust/src/service.rs: the rule wiring (:294-308) maps `alias_unresolved_imports` into `ModuleForSuspicionCheck` and passes `alias_isolated_modules(...)` to `detect_alias_resolution_suspicion`; the row flag (:526-548) calls `rules::is_zero_connectivity` (the inline five-condition copy is gone — the negated grep) and `trust_notes` becomes `[\"alias_resolution_candidate\"]` ONLY when the module has failed alias imports, else `[\"isolated\"]` (a reader-frame note: the module is isolated, nothing suspected); `ModuleTrustRow` (rust/crates/trust/src/types.rs:312-320) gains `alias_unresolved_imports: u64` (literals service.rs:538, rgr trust_tests.rs:93). `module_suspicious_zero_connectivity_flagged` (:1552) is RENAMED to `module_zero_connectivity_flag_uses_the_shared_predicate` (its assertion on the note changes); two new note tests; `root_module_not_flagged_suspicious` stays"
      },
      "expected": "exit 0: the per-module note names alias candidacy only where the evidence exists and genuine isolation as isolation — the same predicate feeds the list and the downgrade (RG-REQ-009-L02), and a note no longer outlives its cause (RG-REQ-009-L04) in the reader's terms (RG-REQ-002-L08)"
    },
    {
      "checkId": "AS-C04",
      "obligationIds": [
        "RG-REQ-009-L04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-trust --test parity 2>&1 | tee /tmp/as-c04.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c04.txt && grep -q 'aliasIsolatedModules' ../trust-parity-fixtures/rules__detect-alias-resolution__triggered/input.json && grep -q 'alias_isolated_modules=packages/ui(55)' ../trust-parity-fixtures/rules__detect-alias-resolution__triggered/expected.json && grep -q '\"reasons\": \\[\\]' ../trust-parity-fixtures/rules__detect-alias-resolution__not-triggered/expected.json",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/trust/tests/parity.rs: the dispatcher (:138, :160-164 `dispatch_detect_alias_resolution`) reads `aliasIsolatedModules: [{\"path\", \"count\"}]` instead of `suspiciousModuleCount` and calls `detect_alias_resolution_suspicion(&alias_isolated)`; `FixtureMockStorage::compute_module_stats` (:315, `Ok(self.module_stats.clone())`) is UNTOUCHED — the detect-alias fixtures dispatch straight to `dispatch_detect_alias_resolution` and never read module_stats, and the struct's `#[serde(default)]` keeps every other fixture's `module_stats` deserializing; the two fixtures are rewritten: `…__triggered` input `[{\"path\":\"packages/ui\",\"count\":55}]` → expected `{\"triggered\":true,\"reasons\":[\"alias_isolated_modules=packages/ui(55)\"]}`; `…__not-triggered` input `[]` → `{\"triggered\":false,\"reasons\":[]}`"
      },
      "expected": "exit 0: the parity corpus pins the new contract — the reason string names the module and its count — and the old count-only contract is gone"
    },
    {
      "checkId": "AS-C05",
      "obligationIds": [
        "RG-REQ-002-L08",
        "RG-REQ-009-L04",
        "RG-REQ-009-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr --lib presentation::trust 2>&1 | tee /tmp/as-c05.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c05.txt && for t in suspicious_modules_rows_name_failed_alias_imports_only_where_they_exist downgrades_block_states_the_alias_reason_in_reader_frame suspicious_modules_basis_in_reader_frame_no_internal_wording render_shows_reliability_levels render_carries_per_section_source_labels render_drops_the_livegraph_posture_section; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c05.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-rgr --test trust_module_edges_seam 2>&1 | tee /tmp/as-c05b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c05b.txt && for t in trust_module_fans_equal_modules_deps; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c05b.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rust/crates/rgr/src/presentation/trust.rs: `render_suspicious_modules` (:623-654) keeps its heading and basis sentence and renders a flagged module with failed alias imports as `  - <module> — N imports through a project alias did not resolve` and a merely isolated one as `  - <module>`; `render_downgrades` (:689-697) renders the alias line reader-facing as `  - Alias resolution suspected — <module> (N imports through a project alias did not resolve)[; …]` from the reason's `path(n)` pairs, with NO `alias_resolution_suspicion:` key prefix on human output (RG-REQ-002-L08) — the raw reason token `suspicious_zero_connectivity_modules=N` and the raw rule-key prefix are both gone from human `trust`; the machine key `alias_resolution_suspicion` remains only in `--json`. The three sibling downgrade bullets keep their raw `key:` prefix (untouched — §8 residual). The axis lines (:376-377) and `agent/src/reliability.rs:541-542`'s humanizer are untouched (the JSON/axis token `alias_resolution_suspicion` is unchanged — orient, deps and stats inherit unchanged). NEW test `downgrades_block_states_the_alias_reason_in_reader_frame` asserts the reader-facing bullet AND that no `alias_resolution_suspicion` key appears in the human downgrades block. NEW render tests in trust_tests.rs (the `ModuleTrustRow` literal :93 gains the field); the existing four stay green; `rgr/tests/trust_module_edges_seam.rs::assert_fans_agree` (:66) consumes the real `compute_module_stats` result (`m.path`/`m.fan_out`/`m.fan_in` only — it never constructs `TrustModuleStats`, so it needs no edit for the new field) and still proves fans == `modules deps`"
      },
      "expected": "exit 0: the reader sees which isolated module has failed alias imports and how many, and the downgrade reason reads in the reader's frame; the fan seam with `modules deps` remains (no behavior change to fans or the list membership)"
    },
    {
      "checkId": "AS-C06",
      "obligationIds": [
        "P-AS-01",
        "P-AS-02",
        "P-AS-03",
        "RG-REQ-009-L06",
        "RG-REQ-009-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-trust 2>&1 | tee /tmp/as-c06.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c06.txt && cargo test -p repo-graph-rgr --lib 2>&1 | tee /tmp/as-c06b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c06b.txt && cargo test -p repo-graph-storage --lib 2>&1 | tee /tmp/as-c06c.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c06c.txt && cargo test -p repo-graph-agent --lib reliability 2>&1 | tee /tmp/as-c06d.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c06d.txt && for t in humanize_other_reasons_unchanged; do grep -qE \"^test .*$t .* ok$\" /tmp/as-c06d.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-daemon-runtime --lib trust_coherence 2>&1 | tee /tmp/as-c06e.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/as-c06e.txt && git diff --quiet HEAD -- crates/agent/src/reliability.rs crates/trust/src/overlay.rs crates/trust/src/coherent.rs crates/daemon-runtime crates/rgr/src/presentation/stats.rs crates/rgr/src/presentation/orient_reliability.rs crates/classification",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the acceptance boundary: the whole trust crate (unit + parity), the whole rgr lib, the storage lib, agent's reliability humanizer tests, daemon-runtime's trust_coherence; and the byte-identity of every path the slice must NOT touch — the humanizer, the overlay (degradation flag token), the coherent envelope, daemon-runtime, stats and orient reliability renderers, the classification crate"
      },
      "expected": "exit 0: the reliability vocabulary (RG-REQ-009-L06), the first-party rules (RG-REQ-009-L07), the overlay token, orient/deps/stats inheritance and every existing test remain exactly as before (no behavior change outside the alias predicate, its evidence and its two rendered lines)"
    },
    {
      "checkId": "AS-C07",
      "obligationIds": [
        "P-AS-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo fmt --check -p repo-graph-trust -p repo-graph-storage -p repo-graph-rgr && cargo clippy -p repo-graph-trust -p repo-graph-storage -p repo-graph-rgr --all-targets -- -D warnings > /tmp/as-c07.txt 2>&1; rc=$?; tail -3 /tmp/as-c07.txt; test $rc -eq 0",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rustfmt and clippy (deny warnings, all targets) over the three touched crates"
      },
      "expected": "exit 0: no formatting or lint debt enters with the candidate"
    },
    {
      "checkId": "AS-C08",
      "obligationIds": [
        "RG-REQ-009-L04",
        "RG-REQ-002-L08",
        "RG-REQ-009-L02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/ALIAS-SUSPICION-1-fraktag-after && mkdir -p /private/tmp/ALIAS-SUSPICION-1-fraktag-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/as-frk-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/as-frk-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-fraktag-before RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-fraktag-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/ALIAS-SUSPICION-1-before-bin:$PATH\" \"/private/tmp/ALIAS-SUSPICION-1-before-bin/rmap\" trust > /tmp/as-frk-trust-before.txt 2> /tmp/as-frk-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/as-frk-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/as-frk-trust-after.txt 2> /tmp/as-frk-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/as-frk-trust-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust --json > /tmp/as-frk-trust-after.json 2> /tmp/as-frk-trust-after.json.err ) || { echo \"RUN-FAILED /tmp/as-frk-trust-after.json\"; exit 1; } && grep -qE '^  - alias_resolution_suspicion: suspicious_zero_connectivity_modules=3$' /tmp/as-frk-trust-before.txt && grep -qE '^  - Import-graph: LOW \\(alias resolution suspected; 183 unresolved imports\\)$' /tmp/as-frk-trust-after.txt && grep -qE '^  - Alias resolution suspected — packages/ui \\(55 imports through a project alias did not resolve\\)$' /tmp/as-frk-trust-after.txt && grep -qE '^  - packages/ui — 55 imports through a project alias did not resolve$' /tmp/as-frk-trust-after.txt && grep -qE '^  - packages/engine$' /tmp/as-frk-trust-after.txt && grep -qE '^  - packages$' /tmp/as-frk-trust-after.txt && ! grep -q 'suspicious_zero_connectivity_modules' /tmp/as-frk-trust-after.txt && ! grep -q 'alias_resolution_suspicion' /tmp/as-frk-trust-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-fraktag-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-fraktag-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/as-frk-deps-after.txt 2> /tmp/as-frk-deps-after.txt.err ) && grep -q 'alias/workspace resolution is downgraded on this index' /tmp/as-frk-deps-after.txt && python3 - /private/tmp/ALIAS-SUSPICION-1-fraktag-after /tmp/as-frk-trust-after.json <<'PY'\nimport sqlite3, glob, json, sys, collections\nroot, jf = sys.argv[1], sys.argv[2]\ndb = glob.glob(root + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\nsnap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\nmods = {r[0]: r[1] for r in k.execute(\"select module_candidate_uid, canonical_root_path from module_candidates where snapshot_uid = ?\", (snap,))}\nown = {r[0]: r[1] for r in k.execute(\"select file_uid, module_candidate_uid from module_file_ownership where snapshot_uid = ?\", (snap,))}\nfiles = collections.Counter(own.values()); fan_in = collections.Counter(); fan_out = collections.Counter(); pairs = set()\nfor sf, tf in k.execute(\"select s.file_uid, t.file_uid from edges e join nodes s on e.source_node_uid = s.node_uid and s.snapshot_uid = e.snapshot_uid join nodes t on e.target_node_uid = t.node_uid and t.snapshot_uid = e.snapshot_uid where e.snapshot_uid = ? and e.type = 'IMPORTS' and e.resolution = 'static'\", (snap,)):\n    a, b = own.get(sf), own.get(tf)\n    if a and b and a != b: pairs.add((a, b))\nfor a, b in pairs: fan_out[a] += 1; fan_in[b] += 1\nalias = collections.Counter()\nfor m, n in k.execute(\"select o.module_candidate_uid, count(*) from unresolved_edges u join nodes n on u.source_node_uid = n.node_uid and n.snapshot_uid = u.snapshot_uid join module_file_ownership o on o.file_uid = n.file_uid and o.snapshot_uid = u.snapshot_uid where u.snapshot_uid = ? and u.type = 'IMPORTS' and u.basis_code = 'specifier_matches_project_alias' group by 1\", (snap,)): alias[m] = n\nzero = {mods[m]: alias[m] for m in mods if fan_in[m] == 0 and fan_out[m] == 0 and files[m] >= 2 and mods[m] not in ('.', '')}\nexpected_alias = {p: n for p, n in zero.items() if n > 0}\nd = json.load(open(jf)); v = d['value']\nrows = {m['qualified_name']: m for m in v['modules']['value']}\nflagged = {q: m for q, m in rows.items() if m['suspicious_zero_connectivity']}\nassert set(flagged) == set(zero), (sorted(flagged), sorted(zero))\nfor q, m in flagged.items():\n    assert m['alias_unresolved_imports'] == zero[q], (q, m['alias_unresolved_imports'], zero[q])\n    assert m['trust_notes'] == (['alias_resolution_candidate'] if zero[q] > 0 else ['isolated']), (q, m['trust_notes'])\ndg = v['triggered_downgrades']['value']['alias_resolution_suspicion']\nassert dg['triggered'] == bool(expected_alias), (dg, expected_alias)\nif expected_alias:\n    assert dg['reasons'] == ['alias_isolated_modules=' + ','.join(f'{p}({n})' for p, n in sorted(expected_alias.items()))], (dg['reasons'], expected_alias)\nelse:\n    assert dg['reasons'] == [], dg\nprint('seam ok: zero-connectivity', sorted(zero.items()), '| alias-isolated', sorted(expected_alias.items()), '| triggered', dg['triggered'])\nPY",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/ALIAS-SUSPICION-1-fraktag-before (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG by the base-revision binary on 2026-09-21, 0 s; served by the before binary /private/tmp/ALIAS-SUSPICION-1-before-bin/rmap{,d} copied from rust/target/release on the CLEAN tree) and NEW /private/tmp/ALIAS-SUSPICION-1-fraktag-after (fresh index by the candidate); under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the after root STAYS",
        "inputs": "FRAKTAG (TypeScript; 5 modules, 3 zero-connectivity: `packages`, `packages/engine`, `packages/ui`) — BEFORE: `alias_resolution_suspicion: suspicious_zero_connectivity_modules=3`, three bare rows. The store: `packages/ui` has 55 unresolved IMPORTS with basis `specifier_matches_project_alias` (`packages/ui/src/components/ui/button.tsx` `import … from \"@/lib/utils\"`, `…/IngestionDialog.tsx` `@/components/ui/dialog` — the tsconfig alias TS-ALIAS-RESOLUTION-1 will resolve); `packages/engine` and `packages` have none. AFTER: the alias reason is TRUE and named — `packages/ui (55 imports through a project alias did not resolve)`; the row `packages/ui — 55 …`; `packages` and `packages/engine` plain (isolated); Import-graph LOW unchanged (the reason is real here); the store-derived seam (python recomputes fans over resolved static IMPORTS through `module_file_ownership` and the per-module alias-basis counts) equals the JSON `modules[].alias_unresolved_imports`, `trust_notes` and `triggered_downgrades.alias_resolution_suspicion.reasons`"
      },
      "expected": "exit 0: where the alias explanation is true, the reader is told which module and how many imports — and which isolated modules carry no such evidence"
    },
    {
      "checkId": "AS-C09",
      "obligationIds": [
        "RG-REQ-009-L04",
        "P-AS-02",
        "P-AS-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/ALIAS-SUSPICION-1-kafka-after && mkdir -p /private/tmp/ALIAS-SUSPICION-1-kafka-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/as-kf-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/as-kf-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-before RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/ALIAS-SUSPICION-1-before-bin:$PATH\" \"/private/tmp/ALIAS-SUSPICION-1-before-bin/rmap\" trust > /tmp/as-kf-trust-before.txt 2> /tmp/as-kf-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/as-kf-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/as-kf-trust-after.txt 2> /tmp/as-kf-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/as-kf-trust-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust --json > /tmp/as-kf-trust-after.json 2> /tmp/as-kf-trust-after.json.err ) || { echo \"RUN-FAILED /tmp/as-kf-trust-after.json\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-before RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/ALIAS-SUSPICION-1-before-bin:$PATH\" \"/private/tmp/ALIAS-SUSPICION-1-before-bin/rmap\" deps list > /tmp/as-kf-deps-before.txt 2> /tmp/as-kf-deps-before.txt.err ) || { echo \"RUN-FAILED /tmp/as-kf-deps-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-kafka-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-kafka-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/as-kf-deps-after.txt 2> /tmp/as-kf-deps-after.txt.err ) || { echo \"RUN-FAILED /tmp/as-kf-deps-after.txt\"; exit 1; } && grep -qE '^  - Import-graph: LOW \\(alias resolution suspected; 44704 unresolved imports\\)$' /tmp/as-kf-trust-before.txt && grep -qE '^  - Import-graph: LOW \\(44704 unresolved imports\\)$' /tmp/as-kf-trust-after.txt && grep -qE '^  - Change-impact: LOW \\(registry/factory patterns detected\\)$' /tmp/as-kf-trust-after.txt && ! grep -q 'alias resolution' /tmp/as-kf-trust-after.txt && grep -qE '^Suspicious Modules \\(zero connectivity\\)' /tmp/as-kf-trust-after.txt && grep -qE '^  - generator$' /tmp/as-kf-trust-after.txt && ! grep -q 'through a project alias' /tmp/as-kf-trust-after.txt && grep -q 'alias/workspace resolution is downgraded on this index' /tmp/as-kf-deps-before.txt && ! grep -q 'alias/workspace resolution is downgraded on this index' /tmp/as-kf-deps-after.txt && sed 's/; alias\\/workspace resolution is downgraded on this index//' /tmp/as-kf-deps-before.txt > /tmp/as-kf-deps-b.txt && diff /tmp/as-kf-deps-b.txt /tmp/as-kf-deps-after.txt && rm -rf /private/tmp/ALIAS-SUSPICION-1-vcmi-after && mkdir -p /private/tmp/ALIAS-SUSPICION-1-vcmi-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-vcmi-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-vcmi-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/as-vc-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/as-vc-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-vcmi-before RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-vcmi-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/ALIAS-SUSPICION-1-before-bin:$PATH\" \"/private/tmp/ALIAS-SUSPICION-1-before-bin/rmap\" trust > /tmp/as-vc-trust-before.txt 2> /tmp/as-vc-trust-before.txt.err ) || { echo \"RUN-FAILED /tmp/as-vc-trust-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-vcmi-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-vcmi-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/as-vc-trust-after.txt 2> /tmp/as-vc-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/as-vc-trust-after.txt\"; exit 1; } && grep -v '^Snapshot:' /tmp/as-vc-trust-before.txt | sed -E 's/repo_[0-9a-z]+/repo_X/g' > /tmp/as-vc-tb.txt && grep -v '^Snapshot:' /tmp/as-vc-trust-after.txt | sed -E 's/repo_[0-9a-z]+/repo_X/g' > /tmp/as-vc-ta.txt && diff /tmp/as-vc-tb.txt /tmp/as-vc-ta.txt && python3 - /private/tmp/ALIAS-SUSPICION-1-kafka-after /tmp/as-kf-trust-after.json <<'PY'\nimport sqlite3, glob, json, sys, collections\nroot, jf = sys.argv[1], sys.argv[2]\ndb = glob.glob(root + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\nsnap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\nmods = {r[0]: r[1] for r in k.execute(\"select module_candidate_uid, canonical_root_path from module_candidates where snapshot_uid = ?\", (snap,))}\nown = {r[0]: r[1] for r in k.execute(\"select file_uid, module_candidate_uid from module_file_ownership where snapshot_uid = ?\", (snap,))}\nfiles = collections.Counter(own.values()); fan_in = collections.Counter(); fan_out = collections.Counter(); pairs = set()\nfor sf, tf in k.execute(\"select s.file_uid, t.file_uid from edges e join nodes s on e.source_node_uid = s.node_uid and s.snapshot_uid = e.snapshot_uid join nodes t on e.target_node_uid = t.node_uid and t.snapshot_uid = e.snapshot_uid where e.snapshot_uid = ? and e.type = 'IMPORTS' and e.resolution = 'static'\", (snap,)):\n    a, b = own.get(sf), own.get(tf)\n    if a and b and a != b: pairs.add((a, b))\nfor a, b in pairs: fan_out[a] += 1; fan_in[b] += 1\nalias = collections.Counter()\nfor m, n in k.execute(\"select o.module_candidate_uid, count(*) from unresolved_edges u join nodes n on u.source_node_uid = n.node_uid and n.snapshot_uid = u.snapshot_uid join module_file_ownership o on o.file_uid = n.file_uid and o.snapshot_uid = u.snapshot_uid where u.snapshot_uid = ? and u.type = 'IMPORTS' and u.basis_code = 'specifier_matches_project_alias' group by 1\", (snap,)): alias[m] = n\nzero = {mods[m]: alias[m] for m in mods if fan_in[m] == 0 and fan_out[m] == 0 and files[m] >= 2 and mods[m] not in ('.', '')}\nexpected_alias = {p: n for p, n in zero.items() if n > 0}\nd = json.load(open(jf)); v = d['value']\nrows = {m['qualified_name']: m for m in v['modules']['value']}\nflagged = {q: m for q, m in rows.items() if m['suspicious_zero_connectivity']}\nassert set(flagged) == set(zero), (sorted(flagged), sorted(zero))\nfor q, m in flagged.items():\n    assert m['alias_unresolved_imports'] == zero[q], (q, m['alias_unresolved_imports'], zero[q])\n    assert m['trust_notes'] == (['alias_resolution_candidate'] if zero[q] > 0 else ['isolated']), (q, m['trust_notes'])\ndg = v['triggered_downgrades']['value']['alias_resolution_suspicion']\nassert dg['triggered'] == bool(expected_alias), (dg, expected_alias)\nif expected_alias:\n    assert dg['reasons'] == ['alias_isolated_modules=' + ','.join(f'{p}({n})' for p, n in sorted(expected_alias.items()))], (dg['reasons'], expected_alias)\nelse:\n    assert dg['reasons'] == [], dg\nprint('seam ok: zero-connectivity', sorted(zero.items()), '| alias-isolated', sorted(expected_alias.items()), '| triggered', dg['triggered'])\nPY",
        "cwd": ".",
        "environment": "pre-provisioned /private/tmp/ALIAS-SUSPICION-1-kafka-before (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/kafka, 100 s) and /private/tmp/ALIAS-SUSPICION-1-vcmi-before (48 s) by the base binary; NEW -after roots by the candidate; under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the after roots STAY",
        "inputs": "kafka (Java; 67 modules; 5 zero-connectivity candidates `.github`, `committer-tools`, `docker`, `generator`, `release` — none has a single alias-basis unresolved import; `generator` has 217 `no_supporting_signal` rows) — BEFORE: `Import-graph: LOW (alias resolution suspected; 44704 unresolved imports)`, `Change-impact: LOW (alias resolution suspected; registry/factory patterns detected)`, the downgrade line with `=5`. AFTER: the alias reason is GONE (`Import-graph: LOW (44704 unresolved imports)` — LOW stays for its true cause; `Change-impact: LOW (registry/factory patterns detected)`), the five modules still listed as isolated, plain; `deps list`'s footnote clause `alias/workspace resolution is downgraded on this index` (the caveat literal is assembled in daemon-runtime/src/deps_headline.rs:300 `declared_unobserved_caveat` and rendered as an opaque JSON string by rgr/src/presentation/deps_list.rs; the `Downgraded` posture that gates it is derived by daemon-runtime/src/dispatch.rs:6946-6961 from the trust overlay's `alias_resolution_suspicion` degradation flag — overlay CODE untouched, its OUTPUT follows the trigger) is PRESENT before and ABSENT after, the rest of `deps list` byte-identical — a reason that outlived its cause on a second surface, now gone (OC-1: the INPUT-1 oracle wrongly asserted byte-identity). vcmi (C++; 2 candidates `CI`, `android`, no alias rows; the alias reason never fired) — `trust` byte-identical apart from the `Snapshot:` line and the per-index repo_uid embedded in the `registry_pattern_suspicion` reason, normalized to `repo_X` on both sides (OC-1; the control). The kafka seam equals the JSON"
      },
      "expected": "exit 0: a reason that named a cause the store never had disappears — from trust and from the deps footnote that inherits it — the level stays where its real cause keeps it, and a repository the old rule never touched remains byte-stable (no behavior change where nothing was wrong)"
    },
    {
      "checkId": "AS-C10",
      "obligationIds": [
        "RG-REQ-009-L04",
        "RG-REQ-002-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "rm -rf /private/tmp/ALIAS-SUSPICION-1-repo-graph-after && mkdir -p /private/tmp/ALIAS-SUSPICION-1-repo-graph-after && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index > /tmp/as-rg-index.txt 2>&1 ) || { echo \"INDEX-FAILED /tmp/as-rg-index.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust > /tmp/as-rg-trust-after.txt 2> /tmp/as-rg-trust-after.txt.err ) || { echo \"RUN-FAILED /tmp/as-rg-trust-after.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" trust --json > /tmp/as-rg-trust-after.json 2> /tmp/as-rg-trust-after.json.err ) || { echo \"RUN-FAILED /tmp/as-rg-trust-after.json\"; exit 1; } && grep -qE '^  - Alias resolution suspected — rust \\(1 import through a project alias did not resolve\\)$' /tmp/as-rg-trust-after.txt && grep -qE '^  - rust — 1 import through a project alias did not resolve$' /tmp/as-rg-trust-after.txt && grep -qE '^  - rust/crates/detectors$' /tmp/as-rg-trust-after.txt && grep -qE '^  - tools$' /tmp/as-rg-trust-after.txt && ! grep -q 'suspicious_zero_connectivity_modules' /tmp/as-rg-trust-after.txt && ! grep -q 'alias_resolution_suspicion' /tmp/as-rg-trust-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph' && export RMAP_STATE_ROOT=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after RMAP_SOCKET_PATH=/private/tmp/ALIAS-SUSPICION-1-repo-graph-after/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/as-rg-deps-after.txt 2> /tmp/as-rg-deps-after.txt.err ) && grep -q 'alias/workspace resolution is downgraded on this index' /tmp/as-rg-deps-after.txt && python3 - /private/tmp/ALIAS-SUSPICION-1-repo-graph-after /tmp/as-rg-trust-after.json <<'PY'\nimport sqlite3, glob, json, sys, collections\nroot, jf = sys.argv[1], sys.argv[2]\ndb = glob.glob(root + '/databases/*.db')[0]; k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)\nsnap = k.execute(\"select snapshot_uid from snapshots order by created_at desc limit 1\").fetchone()[0]\nmods = {r[0]: r[1] for r in k.execute(\"select module_candidate_uid, canonical_root_path from module_candidates where snapshot_uid = ?\", (snap,))}\nown = {r[0]: r[1] for r in k.execute(\"select file_uid, module_candidate_uid from module_file_ownership where snapshot_uid = ?\", (snap,))}\nfiles = collections.Counter(own.values()); fan_in = collections.Counter(); fan_out = collections.Counter(); pairs = set()\nfor sf, tf in k.execute(\"select s.file_uid, t.file_uid from edges e join nodes s on e.source_node_uid = s.node_uid and s.snapshot_uid = e.snapshot_uid join nodes t on e.target_node_uid = t.node_uid and t.snapshot_uid = e.snapshot_uid where e.snapshot_uid = ? and e.type = 'IMPORTS' and e.resolution = 'static'\", (snap,)):\n    a, b = own.get(sf), own.get(tf)\n    if a and b and a != b: pairs.add((a, b))\nfor a, b in pairs: fan_out[a] += 1; fan_in[b] += 1\nalias = collections.Counter()\nfor m, n in k.execute(\"select o.module_candidate_uid, count(*) from unresolved_edges u join nodes n on u.source_node_uid = n.node_uid and n.snapshot_uid = u.snapshot_uid join module_file_ownership o on o.file_uid = n.file_uid and o.snapshot_uid = u.snapshot_uid where u.snapshot_uid = ? and u.type = 'IMPORTS' and u.basis_code = 'specifier_matches_project_alias' group by 1\", (snap,)): alias[m] = n\nzero = {mods[m]: alias[m] for m in mods if fan_in[m] == 0 and fan_out[m] == 0 and files[m] >= 2 and mods[m] not in ('.', '')}\nexpected_alias = {p: n for p, n in zero.items() if n > 0}\nd = json.load(open(jf)); v = d['value']\nrows = {m['qualified_name']: m for m in v['modules']['value']}\nflagged = {q: m for q, m in rows.items() if m['suspicious_zero_connectivity']}\nassert set(flagged) == set(zero), (sorted(flagged), sorted(zero))\nfor q, m in flagged.items():\n    assert m['alias_unresolved_imports'] == zero[q], (q, m['alias_unresolved_imports'], zero[q])\n    assert m['trust_notes'] == (['alias_resolution_candidate'] if zero[q] > 0 else ['isolated']), (q, m['trust_notes'])\ndg = v['triggered_downgrades']['value']['alias_resolution_suspicion']\nassert dg['triggered'] == bool(expected_alias), (dg, expected_alias)\nif expected_alias:\n    assert dg['reasons'] == ['alias_isolated_modules=' + ','.join(f'{p}({n})' for p, n in sorted(expected_alias.items()))], (dg['reasons'], expected_alias)\nelse:\n    assert dg['reasons'] == [], dg\nprint('seam ok: zero-connectivity', sorted(zero.items()), '| alias-isolated', sorted(expected_alias.items()), '| triggered', dg['triggered'])\nPY",
        "cwd": ".",
        "environment": "NEW isolated root /private/tmp/ALIAS-SUSPICION-1-repo-graph-after: a fresh index of the CANDIDATE tree of this repository by the candidate (~10 s); STAYS",
        "inputs": "repo-graph itself (61 modules; 6 zero-connectivity candidates `rust`, `rust/crates/detectors`, `rust/crates/git`, `rust/crates/repo-index/tests/fixtures/rust/simple-crate`, `scripts`, `tools`): exactly ONE alias-basis row exists — `rust/crates/repo-index/tests/fixtures/typescript/classifier-repo/src/index.ts` importing `@/lib/missing`, a deliberately unresolvable TEST FIXTURE owned by the `rust` umbrella module. AFTER: the alias reason stays, now honestly scoped to `rust (1 import through a project alias did not resolve)` — the RESIDUAL this slice states rather than special-cases (a fixture repository inside the repository; the reader sees the module and the count and can judge); the other five rows plain; the seam equals the JSON. Singular/plural: `1 import` / `N imports`"
      },
      "expected": "exit 0: repo-graph's own alias note names its one true cause instead of six false ones; a fixture-caused residual is visible and countable, never hidden"
    },
    {
      "checkId": "AS-C11",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-AS-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "test \"$(grep -E '^registry-before [0-9a-f]{64}$' .agent-manager/slices/ALIAS-SUSPICION-1/build-progress.md | tail -1 | cut -d' ' -f2)\" = \"$(shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -d' ' -f1)\" && PIDS=$(pgrep -x rmapd | paste -sd, -); { [ -z \"$PIDS\" ] || ! ps -o command= -p \"$PIDS\" | grep -v '/\\.local/bin/rmapd' | grep -q .; } && ! test -d /private/tmp/ALIAS-SUSPICION-1-before-bin && ls -d /private/tmp/ALIAS-SUSPICION-1-fraktag-after /private/tmp/ALIAS-SUSPICION-1-kafka-after /private/tmp/ALIAS-SUSPICION-1-vcmi-after /private/tmp/ALIAS-SUSPICION-1-repo-graph-after >/dev/null && ! git status --short | grep -vE '^[ MA?][ MA?] (rust/crates/trust/src/storage_port\\.rs|rust/crates/storage/src/trust_impl\\.rs|rust/crates/trust/src/rules\\.rs|rust/crates/trust/src/service\\.rs|rust/crates/trust/src/types\\.rs|rust/crates/trust/tests/parity\\.rs|trust\\-parity\\-fixtures/rules__detect\\-alias\\-resolution__triggered/input\\.json|trust\\-parity\\-fixtures/rules__detect\\-alias\\-resolution__triggered/expected\\.json|trust\\-parity\\-fixtures/rules__detect\\-alias\\-resolution__not\\-triggered/input\\.json|trust\\-parity\\-fixtures/rules__detect\\-alias\\-resolution__not\\-triggered/expected\\.json|trust\\-parity\\-fixtures/report__diagnostics\\-with\\-calls/expected\\.json|rust/crates/rgr/src/presentation/trust\\.rs|rust/crates/rgr/src/presentation/trust_tests\\.rs|rust/crates/rgr/tests/trust_module_edges_seam\\.rs)$' | grep -q . && git diff --check",
        "cwd": ".",
        "environment": "candidate tree, after every other check",
        "inputs": "the full sha256 of the operator registry recorded as `registry-before <digest>` in build-progress.md at step 0 equals the current one; no stray isolated daemon; the before-binary directory removed; the four `-after` roots kept for the manager's gate; `git status --short` (porcelain ` M`, `A `, `AM`, `??`) names only the 14 candidate paths (no new files in this slice); no trailing whitespace"
      },
      "expected": "exit 0: every proof was isolated (the operator's registry byte-identical — no behavior change outside the isolated roots), the tree holds only the allocated paths"
    }
  ]
}
-->

# ALIAS-SUSPICION-1 — trust's "alias resolution suspected" names a real cause or disappears

Status: SPECIFIED and GROUNDED 2026-09-21 on HEAD da29560 (every seam read from the sources by a read-only investigation; every literal read from the product on four fresh isolated roots and their stores) · Track: audit round six follow-up RC-11 (found by TRUST-MODULE-EDGES-1's implementation, 2026-09-13; ratified order Q9 → ALIAS-SUSPICION-1 → SEED-DOCUMENT-1). CODE slice: trust (rule, service, types, parity), storage (one read extended), rgr (two rendered lines). Requires reindex of nothing — the read is recomputed at query time from persisted rows. Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-009-L04 — "a reason shall never outlive its cause": the `alias_resolution_suspicion` downgrade fires only for zero-connectivity modules whose imports failed through a project alias, names each such module with its count, and is silent for modules that are merely isolated; the per-module note says `alias_resolution_candidate` only with that evidence and `isolated` otherwise (AS-C02/C03/C04/C08/C09/C10). RG-REQ-002-L08 — the rendered downgrade line and the module rows read in the reader's frame (`Alias resolution suspected — packages/ui (55 imports through a project alias did not resolve)`) with NO internal-diagnostic label on human output: the human downgrade bullet drops BOTH the raw reason token `suspicious_zero_connectivity_modules=N` AND the raw rule-key prefix `alias_resolution_suspicion:` (the rule identifier is itself an internal label the L forbids); the machine key `alias_resolution_suspicion` survives only in `--json` `triggered_downgrades` (documented routing metadata) (AS-C05/C08/C10). The three sibling downgrade keys (`framework_heavy_suspicion`, `registry_pattern_suspicion`, `missing_entrypoint_declarations`) still prefix the human block with their raw key — a pre-existing L08 residual OUTSIDE this slice's allocated alias cause, named as a follow-up (§8).

**Changes:** none allocated. REPORTED consequences: on kafka the Import-graph and Change-impact reasons lose the false alias clause (their LOW levels stay for their true causes); on FRAKTAG and repo-graph the alias reason stays and is now scoped to one module each; `deps list`'s footnote clause `alias/workspace resolution is downgraded on this index` (dispatch.rs:6946-6961 derives it from the overlay's degradation flag; the overlay CODE is unchanged) follows the trigger: it disappears on kafka and stays on FRAKTAG and repo-graph (AS-C08/C09/C10 assert it) — no requirement line owns this footnote today (noted for the catalog).

**Preserves:** RG-REQ-009-L02 (the list and the downgrade read ONE edge set through ONE zero-connectivity predicate — the duplicated inline copy in the service is removed; the fan seam with `modules deps` stays green — AS-C01/C02/C03/C05), RG-REQ-009-L06 (one caveat vocabulary — the humanizer and the axis token are untouched — AS-C06), RG-REQ-009-L07 (first-party rules untouched — AS-C06), RG-REQ-011-L06 (isolation — AS-C11).

**Decision D-AS1-001 (operator, recorded here; the human may override at closeout):** the trigger threshold. The old rule fired on a COUNT of zero-connectivity modules (≥ 3, calibrated in RC-5's all-zero world; a bare literal at rules.rs:194). The new rule fires on EVIDENCE: at least one zero-connectivity module with ≥ 1 unresolved import whose basis is `specifier_matches_project_alias` — the only basis that literally means "an alias path did not resolve" (`relative_import_target_unresolved` and the Rust crate heuristic name other resolution gaps and stay in the `unresolved imports=N` clause that already holds Import-graph at LOW). Rejected alternatives: keep a count threshold over evidenced modules (no corpus reaches 3 — the rule would never fire, including on FRAKTAG where 55 alias imports really failed); count every unresolved import as evidence (every module has external imports — the rule would fire everywhere, as today). Reader outcome per corpus is in §2.3.

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-AS-01 | fan-in/fan-out/file_count and the zero-connectivity membership of the Suspicious Modules list are byte-identical (the resolved-edge derivation untouched; one predicate) | AS-C01; AS-C02; AS-C05 (seam test); AS-C09 (vcmi control) |
| P-AS-02 | the axis token `alias_resolution_suspicion`, the humanizer, the overlay flag's CODE and every inheriting renderer (orient, deps list, stats) are untouched — only the trigger's evidence and two trust lines change; inheriting OUTPUT moves exactly where the trigger flips (kafka's deps footnote) and nowhere else | AS-C06 (diff --quiet); AS-C08/C09/C10 (deps footnote present/absent as the trigger) |
| P-AS-03 | every existing test in the touched crates stays green; a repository the old rule never touched (vcmi) is byte-stable | AS-C06; AS-C09 |
| P-AS-04 | isolation, cleanup, hygiene | AS-C07; AS-C11 |

## 1. Problem (ROOT-CAUSED — RC-11, docs/audits/2026-09-08-root-causes-v0.18.0.md:154-159; re-verified on HEAD da29560, 2026-09-21)

`rust/crates/trust/src/rules.rs:190-207 detect_alias_resolution_suspicion(suspicious_module_count)` fires when the number of zero-connectivity modules is `>= 3` (bare literal :194) and emits `suspicious_zero_connectivity_modules=N`; the count comes from `count_suspicious_zero_connectivity_modules` (:392-405) over `ModuleForSuspicionCheck { qualified_name, fan_in, fan_out, file_count }` (:407-413) — no fact about WHY a module is isolated reaches the rule. The same five-condition predicate is written a second time in `service.rs:526-548` for the per-module row flag and its note `alias_resolution_candidate`. The trigger feeds `compute_import_graph_reliability` (:216-231, pushes the reason independently of `unresolved_imports=N`) and `compute_change_impact_reliability` (:320-332); the human render prints the humanized axis clause `alias resolution suspected` (agent/src/reliability.rs:541-542; rgr trust.rs:376-377) and the RAW token in the downgrades block (trust.rs:689-697). The store never had a per-module alias fact on the trust path: `compute_module_stats` (storage/src/trust_impl.rs:755-852) derives fans from resolved static IMPORTS through `module_file_ownership` only, and no trust read counts unresolved IMPORTS per module (`call_resolution_reads.rs:303` groups CALLS; `queries.rs:2921` lists per-file imports by prefix). MEASURED on fresh isolated indexes (2026-09-21, HEAD binary): repo-graph 6 zero-connectivity candidates → `Import-graph: LOW (alias resolution suspected; 4936 unresolved imports)` / `Change-impact: LOW (alias resolution suspected)`, yet only ONE alias-basis unresolved import exists in all six (`rust/crates/repo-index/tests/fixtures/typescript/classifier-repo/src/index.ts` → `@/lib/missing`, a deliberately unresolvable test fixture in the `rust` umbrella module); kafka 5 candidates (`.github`, `committer-tools`, `docker`, `generator`, `release`) → the same two reasons with ZERO alias-basis rows; FRAKTAG 3 candidates → the reason is TRUE for exactly one of them, `packages/ui` (55 imports through the tsconfig alias `@/…`, e.g. `packages/ui/src/components/ui/button.tsx` `import { cn } from "@/lib/utils"`), and false for `packages` and `packages/engine`; vcmi (2) and hadoop (2) never fire — by count alone. The rule cannot distinguish "this module's alias imports did not resolve" from "nothing imports this module and it imports nothing" (RC-11), so the reason outlives a cause it never verified (RG-REQ-009-L04) and the reader reads a raw token (RG-REQ-002-L08).

## 2. Contract

### 2.1 The fix, at the cause — one fact on the module row, one evidence-based trigger, one predicate, two reader-frame lines

1. **The fact (storage → trust port).** `TrustModuleStats` gains `alias_unresolved_imports: u64`; `compute_module_stats`' SQL gains one LEFT JOIN (a CTE counting `unresolved_edges` rows with `type = 'IMPORTS'` and `basis_code = 'specifier_matches_project_alias'` per `module_file_ownership.module_candidate_uid`); the resolved-edge CTEs are byte-identical. The new field takes `#[serde(default)]` (the `top_external_types` precedent in trust/src/types.rs) so pre-slice `TrustModuleStats` fixture JSON that omits it still deserializes. Construction literals the compiler forces to add the field (`git grep 'TrustModuleStats {'` = 1 def storage_port.rs:46 + 8 literals): storage trust_impl.rs:842 (the row mapper), trust storage_port.rs:396, service.rs:1555/1562/1588/1605/1612/1619 (six fixtures). `rgr/tests/trust_module_edges_seam.rs::assert_fans_agree` (:66) and parity's `FixtureMockStorage::compute_module_stats` (:315, `Ok(self.module_stats.clone())`) only CONSUME `TrustModuleStats` (field reads / clone) — neither constructs one, so the field addition does not touch them.
2. **One predicate, shared.** `rules::is_zero_connectivity(&ModuleForSuspicionCheck) -> bool` (fan 0/0, file_count ≥ 2, name not `.`/empty) — used by the counter and by the service's row flag; the inline copy at service.rs:529-533 is deleted (RG-REQ-009-L02: the list and the downgrade cannot disagree).
3. **The trigger.** `alias_isolated_modules(&[ModuleForSuspicionCheck]) -> Vec<(String, u64)>` = zero-connectivity modules with `alias_unresolved_imports ≥ 1`, sorted by path; `detect_alias_resolution_suspicion(&alias_isolated)` is triggered iff non-empty, with ONE reason `alias_isolated_modules=<path>(<n>)[,…]`. The `>= 3` literal and the `suspicious_zero_connectivity_modules=N` reason are retired; the two tests that pinned them are RENAMED to their new identities (§2.2). `compute_import_graph_reliability` / `compute_change_impact_reliability` keep their boolean input.
4. **The note.** `ModuleTrustRow` gains `alias_unresolved_imports: u64`; `trust_notes` = `["alias_resolution_candidate"]` iff that count ≥ 1, else `["isolated"]` (additive vocabulary on the JSON `trust_notes`).
5. **Two rendered lines (rgr trust.rs).** Suspicious Modules section (heading and basis sentence unchanged): `  - <module> — N imports through a project alias did not resolve` (singular `1 import`) for evidenced modules, `  - <module>` otherwise. Downgrades block (`render_downgrades`): the alias bullet becomes reader-facing — `  - Alias resolution suspected — <module> (N imports through a project alias did not resolve)[; …]` parsed from the reason's `path(n)` pairs — with NO `alias_resolution_suspicion:` key prefix on human output (RG-REQ-002-L08: the raw rule key is an internal-diagnostic label; capital `A` opens the bullet sentence). The machine key stays in `--json` (`triggered_downgrades.alias_resolution_suspicion`, `reasons: ["alias_isolated_modules=…"]`) and the axis clause (`Import-graph: LOW (alias resolution suspected; …)`) is unchanged (already humanized). The other three downgrade bullets keep their raw `key:` prefix (untouched — a separate pre-existing L08 residual, §8). The axis lines and the humanizer are untouched.
6. **Parity.** The two `rules__detect-alias-resolution__*` fixtures carry the new input (`aliasIsolatedModules: [{path, count}]`) and reason.
7. **Nothing else moves:** no schema, no CLI, no overlay token, no orient/deps/stats change, no first-party or coherence change.

### 2.2 Evidence taxonomy (one row = one bound test)

| Input | Outcome | Bound test / check |
|---|---|---|
| module with 2 alias-basis + 1 relative-basis unresolved imports / module with none | `alias_unresolved_imports` 2 / 0; fans unchanged | AS-C01 `compute_module_stats_counts_unresolved_alias_imports_per_module` |
| one zero-connectivity module with alias count ≥ 1 | triggered; reason `alias_isolated_modules=<path>(<n>)` | AS-C02 `alias_suspicion_fires_on_one_alias_isolated_module` |
| three zero-connectivity modules, all alias count 0 | not triggered, no reason | AS-C02 `alias_suspicion_silent_when_isolated_modules_have_no_failed_alias_imports` |
| two evidenced modules | one reason, both named, path order | AS-C02 `alias_suspicion_reason_names_each_module_with_its_count_in_path_order` |
| root `.`, empty name, single-file, fan > 0 | never zero-connectivity | AS-C02 `zero_connectivity_predicate_excludes_root_empty_and_single_file_modules`; `count_suspicious_modules_matches_all_criteria` |
| row note with / without alias evidence | `alias_resolution_candidate` / `isolated` | AS-C03 (two tests) |
| row flag and counter | same predicate | AS-C03 `module_zero_connectivity_flag_uses_the_shared_predicate` |
| render: evidenced row / plain row / downgrade line | the three literals of §2.1.5 | AS-C05 (two tests) |
| parity fixtures | new input and reason | AS-C04 |
| corpora: FRAKTAG (true alias cause), kafka (no alias cause), repo-graph (one fixture cause), vcmi (never fired) | §2.3 | AS-C08/C09/C10 |

### 2.3 Outward proof (what a user of the product gains)

kafka `trust`: `Import-graph: LOW (alias resolution suspected; 44704 unresolved imports)` → `Import-graph: LOW (44704 unresolved imports)`; `Change-impact: LOW (alias resolution suspected; registry/factory patterns detected)` → `Change-impact: LOW (registry/factory patterns detected)`; the five isolated modules still listed, plain. FRAKTAG: the reason stays and says why — the human downgrade bullet reads `Alias resolution suspected — packages/ui (55 imports through a project alias did not resolve)` (no `alias_resolution_suspicion:` key prefix on human output; the key stays in `--json`); `packages` and `packages/engine` are listed as isolated, not suspected. repo-graph: `rust (1 import through a project alias did not resolve)` — a test fixture inside the repository, visible and countable (the residual this slice states). vcmi: byte-identical.

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-009-L02, P-AS-01 | fans or list membership; the list and downgrade diverging | AS-C01 (derivation untouched); AS-C02/C03 (one predicate); AS-C05 (`trust_module_fans_equal_modules_deps`) |
| RG-REQ-009-L06 | the caveat vocabulary / humanizer | AS-C06 (`humanize_other_reasons_unchanged`; reliability.rs untouched) |
| RG-REQ-009-L07 | first-party rules | AS-C06 (whole trust crate) |
| P-AS-02 | the overlay token or an inheriting surface | AS-C06 (diff --quiet); AS-C09 (`deps list` byte-identical) |
| P-AS-03 | any existing test; a repo the rule never touched | AS-C06; AS-C09 (vcmi) |
| RG-REQ-011-L06, P-AS-04 | isolation | AS-C11 |

## 4. Stop conditions

Frozen: the resolved-edge derivation, the axis token and humanizer, the overlay flag, orient/deps/stats renderers, `compute_import_graph_reliability`'s inputs, schema, CLI. No second basis code counted as alias evidence (a relative-import gap is not an alias gap — stated, not widened). Nothing outside the thirteen candidate paths (a literal site or inheriting surface the compiler or a test names outside them is a FINDING). STANDING HONESTY RULES. Unmet DoD → STOP. Do NOT commit. Run every check to completion in the FOREGROUND; end your turn only with the evidence object.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

0. On the CLEAN tree: record `git rev-parse HEAD`; `(cd rust && cargo build --release --bin rmap --bin rmapd)` (warm cache) → `mkdir -p /private/tmp/ALIAS-SUSPICION-1-before-bin && cp rust/target/release/rmap rust/target/release/rmapd /private/tmp/ALIAS-SUSPICION-1-before-bin/`; `registry-before <full sha256>` in build-progress.md; verify the four `-before` roots exist (§7).
1. Failing tests FIRST (§2.2 names): storage read + struct field (compiler-listed literals) → predicate + trigger + parity fixtures → service wiring + note + `ModuleTrustRow` → render lines.
2. Chunked gates AS-C01 → C02 → C03 → C04 → C05, then AS-C06 (whole suites, foreground) and AS-C07 — NEVER `cargo test --workspace`.
3. `(cd rust && cargo build --release --bin rmap --bin rmapd)` (candidate) → AS-C08 (FRAKTAG, 0 s) → C09 (kafka ~100 s + vcmi ~48 s) → C10 (repo-graph ~10 s); every `/tmp/as-*` capture from THIS cycle's run.
4. AS-C11 (remove the before-bin dir; the four `-after` roots STAY), hand-off with the evidence object (each check's outcome NESTED under `outcome`; `supportingEvidence` non-empty).

## 6. Definition of done

All eleven checks pass; §2.3 holds on all four corpora; the report carries the code-under-analysis examples: FRAKTAG's alias import sites quoted with file:line (`packages/ui/src/components/ui/button.tsx` `@/lib/utils`, `…/IngestionDialog.tsx` `@/components/ui/dialog`) and the module's count; repo-graph's one fixture import (`rust/crates/repo-index/tests/fixtures/typescript/classifier-repo/src/index.ts` → `@/lib/missing`); one kafka isolated module and why no alias import exists there (e.g. `generator`'s 217 `no_supporting_signal` rows); the BEFORE/AFTER trust lines per corpus.

## 7. Corpus roots (pre-provisioned by the manager on 2026-09-21 with the base-revision binary at da29560; rebuild recipe)

- `/private/tmp/ALIAS-SUSPICION-1-{fraktag,kafka,vcmi,repo-graph}-before`: fresh `rmap index` of `/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG` (0 s), `…/legacy-codebases/kafka` (100 s), `…/legacy-codebases/vcmi` (48 s) and this repository (10 s); stdio transport, auto passes off.
- The builder creates the `-after` roots (fresh indexes by the candidate) and leaves them for the manager; the retained roots under `~/repo-graph-retained/` are never served; the operator registry is read only for its digest.

## 8. Follow-ups (not this slice)

CLASSIFIER-PYTHON-BASIS-1: Python bare imports on kafka (`release/release.py` `gpg`, `committer-tools/refresh_collaborators.py` `bs4`) are classified `internal_candidate` with the Rust-named basis `rust_crate_internal_module_heuristic` — a misattribution into trust's own-code bucket (found by this grounding). RELATIVE-IMPORT-ISOLATION-1: `rust/crates/detectors` (56 `relative_import_target_unresolved` rows) and `rust/crates/git` (4) are isolated by a relative-import gap the alias rule deliberately does not count — a second, honestly named reason if wanted. TS-ALIAS-RESOLUTION-1 (D-ECH-002 direction 2) resolves FRAKTAG's 55 alias imports and thereby clears this slice's true FRAKTAG reason — the reason then dies with its cause. The `deps list` marker reads the overlay token (dispatch.rs:6946-6961) — an undocumented inheriting surface, unchanged here. DOWNGRADE-LABELS-1: the `Triggered Downgrades` human block still prefixes the three non-alias downgrades (`framework_heavy_suspicion`, `registry_pattern_suspicion`, `missing_entrypoint_declarations`) with their raw snake_case rule key (`render_downgrades`, rgr trust.rs) — the same RG-REQ-002-L08 internal-label class this slice retires for the alias bullet; humanizing every downgrade label is a separate render slice, deliberately not folded in here (this slice fixes only its allocated alias cause).

## 9. Baseline history

- INPUT-1 (2026-09-21): first admission baseline. Its first implementation admission: the builder delivered the whole fix in one cycle with every product-facing assertion of AS-C01..C08 and C10 green; the runtime's checkpoint refused the candidate on ONE path outside the allocation (the whole-report parity fixture, which pins the serialized `ModuleTrustRow` — A-1), and AS-C09 failed on two manager oracle defects (kafka's `deps list` footnote correctly disappears with the trigger; vcmi's control diff must normalize the per-index repo_uid — OC-1); candidate preserved.
- INPUT-2 (2026-09-21): A-1 (candidate path added: `trust-parity-fixtures/report__diagnostics-with-calls/expected.json`) + OC-1 (AS-C09 corrected; AS-C08/C10 assert the footnote stays; §0 and P-AS-02 state the deps-footnote movement); ledger docs/assurance/ALIAS-SUSPICION-1/oracle-corrections.md (pinned).
