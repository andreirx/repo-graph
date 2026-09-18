<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "EXPLAIN-CYCLES-HONEST-1",
  "baselinePath": "docs/requirements/baselines/EXPLAIN-CYCLES-HONEST-1-INPUT-1.json",
  "parentRequirementIds": [
    "RG-REQ-002",
    "RG-REQ-003",
    "RG-REQ-004",
    "RG-REQ-005",
    "RG-REQ-010",
    "RG-REQ-011",
    "RG-REQ-012"
  ],
  "implements": [
    "RG-REQ-003-L01",
    "RG-REQ-002-L01",
    "RG-REQ-004-L07"
  ],
  "preserves": [
    "RG-REQ-003-L05",
    "RG-REQ-005-L06",
    "RG-REQ-010-L01",
    "RG-REQ-012-L07",
    "RG-REQ-011-L06"
  ],
  "preservationObligationIds": [
    "P-ECH-01",
    "P-ECH-02",
    "P-ECH-03",
    "P-ECH-04"
  ],
  "changes": [
    "RG-REQ-012-L06"
  ],
  "acceptanceBoundary": "The explain / cycles / orient human and JSON outputs of the freshly built rmap on fresh isolated indexes of leveldb and vcmi (both the recorded base revision's binary and the candidate), plus cargo test -p repo-graph-rgr (lib presentation incl. explain_cycles_*) -p repo-graph-storage (lib cycle) -p repo-graph-agent (lib cycle) -p repo-graph-daemon-runtime (explain_cycle_walk_route_consistency, cycle_honesty_route_consistency) as named per check.",
  "candidatePaths": [
    "rust/crates/storage/src/agent_impl.rs",
    "rust/crates/storage/src/agent_cycle_labeling.rs",
    "rust/crates/rgr/src/presentation/explain_sections.rs",
    "rust/crates/rgr/src/presentation/explain.rs",
    "rust/crates/rgr/src/presentation/orient_guidance.rs",
    "rust/crates/rgr/src/presentation/cycle_walk_display.rs",
    "rust/crates/rgr/src/presentation/mod.rs",
    "rust/crates/agent/src/explain/mod.rs",
    "rust/crates/daemon-runtime/tests/explain_cycle_walk_route_consistency.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/EXPLAIN-CYCLES-HONEST-1/verification.json",
    "docs/assurance/EXPLAIN-CYCLES-HONEST-1/implementation-review.json"
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
      "checkId": "ECH-C01",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-002-L01",
        "RG-REQ-004-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr --lib explain_cycles_ 2>&1 | tee /tmp/ech-c01.txt | grep -E 'test result: ok\\. ([7-9]|[1-9][0-9]+) passed; 0 failed' && for t in explain_cycles_absent_walk_renders_unordered_no_arrows explain_cycles_valid_walk_renders_ring_from_walk_not_member_order explain_cycles_offwalk_members_reported_as_plus_n_more explain_cycles_malformed_walk_renders_unreadable_not_ring explain_cycles_empty_walk_renders_unordered explain_cycles_non_string_module_renders_unreadable_not_dropped explain_cycles_unordered_list_caps_at_eight_with_plus_k_more; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c01.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the explain_cycles_* render tests in rust/crates/rgr/src/presentation/explain.rs (section 2.2's evidence taxonomy, one test per row) plus any further regression test the family gains"
      },
      "expected": "exit 0: at least seven explain_cycles_* tests pass with 0 failed AND each of the seven NAMED tests is listed as ok — the count is a lower bound; a missing named test or any failure fails the check."
    },
    {
      "checkId": "ECH-C02",
      "obligationIds": [
        "RG-REQ-003-L05",
        "RG-REQ-005-L06",
        "RG-REQ-010-L01",
        "RG-REQ-012-L07",
        "P-ECH-01",
        "P-ECH-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr --lib presentation 2>&1 | tee /tmp/ech-c02.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ech-c02.txt && for t in cycle_anchor_full_chain_for_small_cycle cycle_anchor_unordered_form_when_no_walk cycle_anchor_one_element_walk_is_not_a_fabricated_self_ring cycle_anchor_non_string_walk_element_is_unreadable_not_dropped cycle_anchor_truncates_large_cycle cross_surface_cycle_walk_agrees_ordered cross_surface_cycle_walk_agrees_unordered_without_edges no_edges_renders_unordered_with_no_arrows offwalk_members_reported_as_plus_n_more truncated_edges_render_unordered_no_arrows render_shows_cycle_count headline_counts_come_from_the_shared_partition no_high_confidence_beside_no_match render_shows_callers tier0_header_anchors_symbol_file_at_identity_line tier1_callers_anchor_file_line_when_both_present semantic_no_match_renders_labeled_candidates_in_human_mode; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c02.txt || { echo \"MISSING $t\"; exit 1; }; done && test \"$(grep -cE '^test .*fn tier[01]_|^test .*tier[01]_.* ok$' /tmp/ech-c02.txt)\" -ge 6",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the whole rgr presentation test module: orient's cycle_anchor_* and cross_surface_cycle_walk_agrees_* family (orient_guidance's chain formatting must be byte-identical after the shared validation is extracted), the cycles/walk.rs unordered/off-walk/truncated tests, cycles/tests.rs count tests, explain's confidence/callers/tier anchor tests, the semantic-candidates test"
      },
      "expected": "exit 0: the whole presentation suite is green and every named test is listed as ok — no-behavior-change for orient's cycle line (RG-REQ-003-L05), explain's confidence/seed/anchor rules (RG-REQ-005-L06, RG-REQ-010-L01, RG-REQ-012-L07) and for the cycles renderer (P-ECH-01) and explain's other sections (P-ECH-03)."
    },
    {
      "checkId": "ECH-C03",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-004-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-storage --lib cycle 2>&1 | tee /tmp/ech-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ech-c03.txt && for t in focus_module_cycle_read_carries_the_same_walk_as_the_repo_read focus_path_cycle_read_carries_the_same_walk_as_the_repo_read; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c03.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-agent --lib cycle 2>&1 | tee /tmp/ech-c03b.txt | grep -E '^test result: ok\\.' && grep -qE '^test .*walk_follows_real_edges_not_member_order .* ok$' /tmp/ech-c03b.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "storage's cycle tests incl. the two NEW tests (section 2.1.1): on a fixture with a real 3-module ring, find_cycles_involving_module / find_cycles_involving_path return the SAME walk (and type_only) find_module_cycles returns for that cycle — both now pass through label_module_cycles; the agent's cycle_walk kernel tests"
      },
      "expected": "exit 0: the focus reads carry the repo read's walk for the same cycle (one derivation, RG-REQ-004-L07) and the walk kernel's tests stay green."
    },
    {
      "checkId": "ECH-C04",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-002-L01",
        "RG-REQ-012-L06",
        "P-ECH-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --test explain_cycle_walk_route_consistency 2>&1 | tee /tmp/ech-c04.txt | grep -E 'test result: ok\\. ([2-9]|[1-9][0-9]+) passed; 0 failed' && for t in explain_symbol_focus_cycle_walk_equals_orient_cycle_walk explain_path_focus_cycle_walk_equals_orient_cycle_walk; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c04.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-daemon-runtime --test cycle_honesty_route_consistency 2>&1 | tee /tmp/ech-c04b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/ech-c04b.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the NEW integration test rust/crates/daemon-runtime/tests/explain_cycle_walk_route_consistency.rs: an isolated dispatcher indexes a temp repo with a 3-module import ring; `explain` on a symbol in one ring module and `explain` on that module's path both return an EXPLAIN_CYCLES item whose `walk` equals the walk `orient`'s cycle leaf carries for the same member set (both from label_module_cycles); the existing cycle_honesty_route_consistency suite"
      },
      "expected": "exit 0: explain's carried walk equals orient's for the same cycle on the SQLite route (one derivation; the JSON now carries `walk` on this route — the RG-REQ-012-L06 additive change, REPORTED), and the existing route-consistency suite remains green (P-ECH-02: no route gains a walk it cannot verify)."
    },
    {
      "checkId": "ECH-C05",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-002-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "B=$(python3 -c \"import json;print(json.load(open('.agent-manager/slices/EXPLAIN-CYCLES-HONEST-1/status.json'))['candidateTracking']['baseRevision'])\") && rm -rf /private/tmp/EXPLAIN-CYCLES-HONEST-1-before && git worktree add --detach /private/tmp/EXPLAIN-CYCLES-HONEST-1-before $B && (cd /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust && cargo build --release -p repo-graph-rgr -p rmapd > /tmp/ech-build-before.log 2>&1 && tail -n1 /tmp/ech-build-before.log && test -x target/release/rmap && test -x target/release/rmapd) && cargo build --release -p repo-graph-rgr -p rmapd --manifest-path rust/Cargo.toml > /tmp/ech-build-cand.log 2>&1 && tail -n1 /tmp/ech-build-cand.log && test -x rust/target/release/rmap && test -x rust/target/release/rmapd && for R in leveldb vcmi; do rm -rf /private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before /private/tmp/EXPLAIN-CYCLES-HONEST-1-$R; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap index ) >/dev/null 2>&1 && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/dev/null 2>&1 || exit 1; done && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain leveldb::DBImpl::Recover ) 2>/dev/null > /tmp/ech-ldb-explain-before.txt && grep -q '^  - Cycle 1: db -> helpers/memenv -> table -> util$' /tmp/ech-ldb-explain-before.txt",
        "cwd": ".",
        "environment": "a git worktree of the recorded base revision built once (before-binary and its rmapd); fresh isolated indexes of leveldb and vcmi with BOTH binaries, throwaway roots, stdio, auto passes off",
        "inputs": "the leveldb and vcmi checkouts; .agent-manager/slices/EXPLAIN-CYCLES-HONEST-1/status.json for the base revision"
      },
      "expected": "exit 0: the defect reproduces at the base revision on a fresh leveldb index — explain's Import-cycles block draws the alphabetical member set as arrows, `  - Cycle 1: db -> helpers/memenv -> table -> util` (verified literal, 2026-09-18) — and the four indexes (two corpora × two binaries) exist for the following checks; both cargo builds must exit 0 themselves (their output goes to a log, the last line is printed only after success) and both binaries must exist in each tree — a failed build can never index a stale binary."
    },
    {
      "checkId": "ECH-C06",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-002-L01",
        "RG-REQ-004-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl::Recover ) 2>/dev/null > /tmp/ech-ldb-explain-after.txt && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" cycles ) 2>/dev/null > /tmp/ech-ldb-cycles-after.txt && python3 -c \"import re,sys\nex=open('/tmp/ech-ldb-explain-after.txt').read(); cy=open('/tmp/ech-ldb-cycles-after.txt').read()\nassert 'db -> helpers/memenv -> table -> util' not in ex, 'fabricated member-order ring still rendered'\nm=re.search(r'^  - Cycle 1 \\((\\d+) modules\\): (.+)$', ex, re.M); assert m, ex\nring_ex=m.group(2).strip(); size_ex=int(m.group(1))\ncm=re.search(r'^Cycle 1 \\((\\d+) modules\\):\\n  (.+)$', cy, re.M); assert cm, cy\nring_cy=cm.group(2).strip(); size_cy=int(cm.group(1))\nassert ring_ex==ring_cy and size_ex==size_cy, (ring_ex, ring_cy, size_ex, size_cy)\nassert ' -> ' in ring_ex and ring_ex.split(' -> ')[0]==ring_ex.split(' -> ')[-1], ring_ex\nmore_ex=re.search(r'^    \\(\\+ (\\d+) more members? in this cycle\\)$', ex, re.M); more_cy=re.search(r'^  \\(\\+ (\\d+) more members? in this cycle\\)$', cy, re.M)\nassert (more_ex.group(1) if more_ex else None)==(more_cy.group(1) if more_cy else None), (more_ex, more_cy)\nprint('explain ring == cycles ring:', ring_ex, '| size', size_ex, '| off-walk', more_ex.group(1) if more_ex else 0)\"",
        "cwd": ".",
        "environment": "candidate on the ECH-C05 leveldb index",
        "inputs": "the candidate's explain and cycles human outputs on the same fresh leveldb index"
      },
      "expected": "exit 0: on leveldb (a 4-module SCC with a verified 2-member ring plus 2 off-walk members) explain's Import-cycles block renders `Cycle 1 (K modules): <ring>` with the SAME ring text and size `cycles` renders for Cycle 1 (today `util -> helpers/memenv -> util`, K=4, `(+ 2 more members in this cycle)` — REPORTED from the outputs, compared not hardcoded), the ring closes on its first member, and the fabricated member-order chain is gone."
    },
    {
      "checkId": "ECH-C07",
      "obligationIds": [
        "RG-REQ-003-L01",
        "RG-REQ-002-L01",
        "RG-REQ-004-L07",
        "P-ECH-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "T0=$(python3 -c 'import time;print(time.time())') && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain CGHeroInstance ) 2>/dev/null > /tmp/ech-vcmi-explain-after.txt && SECS=$(python3 -c \"import time;print(round(time.time()-$T0,2))\") && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" cycles ) 2>/dev/null > /tmp/ech-vcmi-cycles-after.txt && T1=$(python3 -c 'import time;print(time.time())') && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/vcmi' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-vcmi-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain CGHeroInstance ) 2>/dev/null > /tmp/ech-vcmi-explain-before.txt && SECSB=$(python3 -c \"import time;print(round(time.time()-$T1,2))\") && python3 -c \"import re\nex=open('/tmp/ech-vcmi-explain-after.txt').read(); cy=open('/tmp/ech-vcmi-cycles-after.txt').read(); bf=open('/tmp/ech-vcmi-explain-before.txt').read()\nblk=re.search(r'^Import cycles \\((\\d+)\\)\\n((?:  .*\\n|    .*\\n)+)', ex, re.M); assert blk, ex\nassert ' -> ' not in blk.group(2), blk.group(2)\nm=re.search(r'^  - Cycle 1 \\((\\d+) modules\\): (members \\(unordered\\): .+)$', ex, re.M); assert m, ex\ncm=re.search(r'^Cycle 1 \\((\\d+) modules\\):\\n  (members \\(unordered\\): .+)$', cy, re.M); assert cm, cy\nassert m.group(1)==cm.group(1) and m.group(2).strip()==cm.group(2).strip(), (m.groups(), cm.groups())\nassert re.search(r'^  - Cycle 1: AI/BattleAI -> AI/EmptyAI', bf, re.M), 'before-binary should still fabricate'\nprint('vcmi explain unordered == cycles unordered:', m.group(2)[:120])\" && echo \"COST vcmi explain secs before=$SECSB after=$SECS\" | tee /tmp/ech-cost.txt",
        "cwd": ".",
        "environment": "candidate on the ECH-C05 vcmi index; before-binary on the vcmi before index",
        "inputs": "the vcmi checkout; vcmi's 55-module Cycle 1 (edge set over CYCLE_EDGE_CAP ⇒ no walk on every route)"
      },
      "expected": "exit 0: on vcmi (whose largest cycle has a truncated edge set, so no route can verify a walk) explain's Import-cycles block contains NO `->` and its Cycle 1 line renders the SAME `members (unordered): …(+ N more)` text and size `cycles` renders (compared, not hardcoded); the before-binary still renders the fabricated `AI/BattleAI -> AI/EmptyAI …` chain; explain wall time before/after is REPORTED (today 1.38 s; the labeling adds two snapshot reads and per-cycle edge selection) — a cost the human reads, never a numeric acceptance bound (RG-REQ-011-L11 withdrew the latency floor)."
    },
    {
      "checkId": "ECH-C08",
      "obligationIds": [
        "RG-REQ-012-L06",
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain leveldb::DBImpl::Recover --json ) 2>/dev/null > /tmp/ech-ldb-explain-after.json && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain leveldb::DBImpl::Recover --json ) 2>/dev/null > /tmp/ech-ldb-explain-before.json && python3 -c \"import json\ndef item(p):\n    d=json.load(open(p)); s=[x for x in d['value']['signals'] if x['value'].get('code')=='EXPLAIN_CYCLES'][0]; return s['value']['evidence']['items'][0]\nb=item('/tmp/ech-ldb-explain-before.json'); a=item('/tmp/ech-ldb-explain-after.json')\nassert 'walk' not in b or b['walk'] is None, b\nw=a.get('walk'); assert isinstance(w,list) and len(w)>=2 and all(isinstance(x,str) and x for x in w), a\nassert set(w) <= set(a['modules']) and a['modules']==b['modules'] and a['length']==b['length'], (a,b)\nprint('JSON walk now carried on the SQLite explain serve:', w, '| type_only present:', 'type_only' in a)\"",
        "cwd": ".",
        "environment": "candidate and before-binary on the two leveldb indexes",
        "inputs": "the explain --json envelopes (value.signals[].value.code == EXPLAIN_CYCLES → evidence.items[0])"
      },
      "expected": "exit 0: before, the SQLite explain serve carried no `walk` (the key is absent — serde skips None); after, it carries a verified ring (≥ 2 non-empty strings, all members of `modules`) while `modules` and `length` are unchanged — the pre-authorised ADDITIVE JSON change (RG-REQ-012-L06: human and JSON answer the same question — the human ring is drawn from this very field), REPORTED with whether `type_only` now rides along."
    },
    {
      "checkId": "ECH-C09",
      "obligationIds": [
        "RG-REQ-003-L05",
        "RG-REQ-004-L07",
        "P-ECH-01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "for R in leveldb vcmi; do for C in cycles orient; do ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap $C ) > /tmp/ech-$R-$C-before.raw 2>/dev/null || exit 1; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" $C ) > /tmp/ech-$R-$C-after.raw 2>/dev/null || exit 1; test -s /tmp/ech-$R-$C-before.raw && test -s /tmp/ech-$R-$C-after.raw || exit 1; grep -vE '^Snapshot:|^index basis:' /tmp/ech-$R-$C-before.raw > /tmp/ech-$R-$C-before.txt; grep -vE '^Snapshot:|^index basis:' /tmp/ech-$R-$C-after.raw > /tmp/ech-$R-$C-after.txt; cmp /tmp/ech-$R-$C-before.txt /tmp/ech-$R-$C-after.txt && echo \"$R $C byte-identical ($(wc -l < /tmp/ech-$R-$C-after.txt) lines)\" || exit 1; done; done",
        "cwd": ".",
        "environment": "both binaries on the four ECH-C05 indexes",
        "inputs": "the human `cycles` and `orient` outputs — captured raw with exit status required, then normalised"
      },
      "expected": "exit 0: `cycles` and `orient` render byte-identically before and after on leveldb (an ordered ring) and vcmi (an unordered large cycle and ordered small ones) — no-behavior-change for the cycles renderer and orient's cycle line (RG-REQ-003-L05, RG-REQ-004-L07's cycles/orient half, P-ECH-01): extracting the walk validation into a shared function changed nothing they print."
    },
    {
      "checkId": "ECH-C10",
      "obligationIds": [
        "P-ECH-03",
        "RG-REQ-005-L06",
        "RG-REQ-010-L01",
        "RG-REQ-012-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "for R in leveldb vcmi; do case $R in leveldb) T=leveldb::DBImpl::Recover;; vcmi) T=CGHeroInstance;; esac; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain $T ) > /tmp/ech-c10-$R-before.txt 2>/dev/null || exit 1; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain $T ) > /tmp/ech-c10-$R-after.txt 2>/dev/null || exit 1; test -s /tmp/ech-c10-$R-before.txt && test -s /tmp/ech-c10-$R-after.txt || exit 1; done && python3 -c \"import re\ndef strip(p):\n    s=open(p).read(); s=re.sub(r'^index basis:.*\\n', '', s, flags=re.M)\n    return re.sub(r'^Import cycles \\(\\d+\\)\\n(?:(?:  |    ).*\\n)*', '', s, flags=re.M)\nfor R in ('leveldb','vcmi'):\n    b=strip(f'/tmp/ech-c10-{R}-before.txt'); a=strip(f'/tmp/ech-c10-{R}-after.txt')\n    assert a==b, f'{R}: explain differs outside the Import cycles block'\n    print(R, 'explain identical outside the Import cycles block')\"",
        "cwd": ".",
        "environment": "both binaries on the four ECH-C05 indexes; this check acquires its own four explain captures (exit status required, non-empty)",
        "inputs": "explain leveldb::DBImpl::Recover and explain CGHeroInstance, before-binary and candidate, captured by this command"
      },
      "expected": "exit 0: with the Import-cycles block and the per-index basis line removed, explain's human output is byte-identical before and after on both corpora — no-behavior-change for every other explain section (P-ECH-03; confidence, seeds, anchors: RG-REQ-005-L06, RG-REQ-010-L01, RG-REQ-012-L07)."
    },
    {
      "checkId": "ECH-C11",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-ECH-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "inspection",
        "subject": "isolation of every live proof and the cleanup of every throwaway root and the git worktree",
        "criterion": "every rmap invocation in ECH-C05..C10 ran under an RMAP_STATE_ROOT/RMAP_SOCKET_PATH inside /private/tmp/EXPLAIN-CYCLES-HONEST-1-* with RMAP_TRANSPORT=stdio and RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off; the operator's real registry '$HOME/Library/Application Support/repo-graph/registry.json' has the SAME sha256 before the first proof and after the last (shasum -a 256, both values quoted in build-progress.md); at hand-off `ls -d /private/tmp/EXPLAIN-CYCLES-HONEST-1-*` lists nothing, `git worktree list` shows no /private/tmp/EXPLAIN-CYCLES-HONEST-1-before entry, and no rmapd from that worktree or rust/target survives (pgrep -fl rmapd)",
        "inputs": "build-progress.md; the two registry digests; the final ls/worktree/pgrep outputs"
      },
      "expected": "the inspection finds every proof isolated, the real registry digest identical before/after, and no throwaway root, worktree or daemon left behind (no-behavior-change for RG-REQ-011-L06; P-ECH-04)."
    },
    {
      "checkId": "ECH-C12",
      "obligationIds": [
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "git diff --check && (cd rust && cargo fmt --check -p repo-graph-rgr -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-agent && cargo clippy -p repo-graph-rgr -p repo-graph-storage -p repo-graph-daemon-runtime --tests -- -D warnings > /tmp/ech-clippy.log 2>&1 && tail -n1 /tmp/ech-clippy.log) && diff <(git status --short -- rust | grep -vE '^ M rust/crates/storage/src/agent_cycle_labeling.rs$' | sort) <(printf ' M rust/crates/agent/src/explain/mod.rs\\n M rust/crates/rgr/src/presentation/explain.rs\\n M rust/crates/rgr/src/presentation/explain_sections.rs\\n M rust/crates/rgr/src/presentation/mod.rs\\n M rust/crates/rgr/src/presentation/orient_guidance.rs\\n M rust/crates/storage/src/agent_impl.rs\\n?? rust/crates/daemon-runtime/tests/explain_cycle_walk_route_consistency.rs\\n?? rust/crates/rgr/src/presentation/cycle_walk_display.rs\\n' | sort)",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "git status/diff of the candidate; rustfmt and clippy over the touched crates"
      },
      "expected": "exit 0: the diff is whitespace-clean, rustfmt-clean and clippy-clean with -D warnings (clippy's own exit status is required; its output is logged and its last line printed only after success), and — after filtering the optional ninth allowed path rust/crates/storage/src/agent_cycle_labeling.rs out of the actual status — the working tree under rust/ holds exactly the eight always-touched candidate paths (six modified, two new) and nothing else moved. The ninth path is filtered, not required: today `label_module_cycles` already accepts the pre-filtered `Vec<CycleResult>`, so the focus reads call it as-is and that file is not expected to change; the check is decidable and passes whether or not it is touched, and still fails on any OTHER moved path."
    }
  ]
}
-->

# EXPLAIN-CYCLES-HONEST-1 — `explain`'s Import-cycles block draws arrows only over a verified walk, the same walk `cycles` and `orient` draw

Status: ALLOCATED (2026-09-18; specified 2026-09-12) · Track: audit round six, Q4 (HIGH; never worked — the honest form shipped twice for `cycles` and `orient` and never reached `explain`); ratified order position 3 of the remaining queue (human 2026-09-14). CODE slice: the two SQLite focus-scoped cycle reads route through the existing labeling kernel (so a verified walk reaches explain), the explain cycle renderer draws only from that walk, the strict walk validation becomes one shared function with two callers. Maturity: MATURE (`explain`, `cycles`, `orient`). Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13). Every literal in the checks above was read from the product on 2026-09-18 (fresh isolated leveldb and vcmi indexes; current sources) — not from memory.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-003-L01 (`explain` draws no cycle arrow without a verified walk; with a walk it draws the ring), RG-REQ-002-L01 (no surface asserts a relationship the store does not hold — the cycle-arrow half), RG-REQ-004-L07 (the unordered/ring rule on every surface — explain was the surface where it was NOT MET).

**Changes (pre-authorised, REPORTED never predicted):** RG-REQ-012-L06 — `explain --json` on the SQLite route now carries `walk` (a verified ring) and `type_only` in its EXPLAIN_CYCLES items; additive (the LiveGraph serve and older daemons still carry neither); the human ring is drawn from this very field, so the two modes answer the same question (ECH-C08).

**Preserves:** RG-REQ-003-L05 (orient's cycle line — its chain formatter keeps its text; ECH-C02, ECH-C09), RG-REQ-005-L06 / RG-REQ-010-L01 / RG-REQ-012-L07 (explain's confidence, seed and anchor rules — ECH-C02, ECH-C10), RG-REQ-011-L06 (isolation — ECH-C11).

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-ECH-01 | `cycles` and `orient` render byte-identically before/after on leveldb and vcmi; their renderer tests stay green | ECH-C02; ECH-C09 |
| P-ECH-02 | No route gains a walk it cannot verify: the LiveGraph explain serve and any cycle whose edge set is truncated (vcmi's 55-module cycle) still render the unordered form; the route-consistency suite stays green | ECH-C01; ECH-C04; ECH-C07 |
| P-ECH-03 | Every explain section other than Import cycles is byte-identical before/after | ECH-C02; ECH-C10 |
| P-ECH-04 | Every proof isolated; the operator's registry digest unchanged; nothing left behind | ECH-C11 |

## 1. Problem (ROOT-CAUSED — RC-4, docs/audits/2026-09-08-root-causes-v0.18.0.md)

`rust/crates/rgr/src/presentation/explain_sections.rs:226–249 render_cycles` joins `item["modules"]` — the SCC member set, SORTED by `canonicalize_cycles` — with `" -> "`, asserting edges that do not exist. Observed today on fresh isolated indexes: leveldb `explain leveldb::DBImpl::Recover` renders `  - Cycle 1: db -> helpers/memenv -> table -> util` while `cycles` on the same index renders the verified ring `util -> helpers/memenv -> util` `(+ 2 more members in this cycle)` and `orient` says `1 import cycle (util -> helpers/memenv -> util)`; vcmi `explain CGHeroInstance` renders `Cycle 1: AI/BattleAI -> AI/EmptyAI -> AI/MMAI -> …` (55 modules) while `cycles` honestly renders `members (unordered): AI/BattleAI, AI/EmptyAI, … (+ 47 more)` because that cycle's edge set exceeds `CYCLE_EDGE_CAP`. Why: both focus reads (`storage/src/agent_impl.rs`, the `find_cycles_involving_module` / `_path` pairs — four `AgentCycle { walk: None, type_only: None, .. }` construction sites) hand-build their cycles instead of passing them through `agent_cycle_labeling::label_module_cycles`, the kernel `find_module_cycles` (orient's SQLite serve) uses to attach the verified walk (`intra_cycle_edges` + `find_cycle_walk` over `module_import_edges`) and the type-only verdict; the LiveGraph serve (`daemon-runtime/src/explain_lg_serve.rs::serve_cycles`) carries no walk either. Never worked: the render line exists since 82b6557; CYCLE-HONESTY-1 and COHERENCE-3 built the honest form for `cycles`/`orient` and never touched `explain_sections.rs`.

## 2. Contract

### 2.1 The fix, at the cause — a vertical slice, no dormant branch

1. **The focus reads carry the verified walk (storage).** `find_cycles_involving_module{,_cancellable}` and `find_cycles_involving_path{,_cancellable}` filter the raw cycles as today and then route the FILTERED cycles through `label_module_cycles` (the same call `find_module_cycles` makes), so the returned `AgentCycle`s carry `walk` (Some for a verified ring, None where the edge set is truncated) and `type_only`. One derivation for `cycles`, `orient` and `explain` (RG-REQ-004-L07). If `label_module_cycles` needs an entry that accepts the pre-filtered `CycleResult`s, it lives in `agent_cycle_labeling.rs` (the ninth allowed path). Cost: two snapshot reads (module names, tracked files) and per-cycle edge selection over the focus's few cycles — reported on vcmi (ECH-C07; a cost the human reads, not a bound).
2. **The renderer draws only from the walk (rgr).** `render_cycles`: `Cycle N (K modules): A -> B -> A` when `item["walk"]` is a valid ring (≥ 2 non-empty strings), followed by `(+ M more members in this cycle)` when the ring visits fewer members than `length` — the exact two-line form of `cycles/walk.rs`, under explain's bullet; `Cycle N (K modules): members (unordered): A, B, … (+ K more)` (eight shown, as `cycles`) when `walk` is absent, null or empty; `Cycle N (K modules): cycle walk unreadable on this snapshot — run rmap cycles` when `walk` is present but malformed (a non-string element, an empty string, fewer than two members — the orient rule, RG-REQ-003-L05); a `modules` entry that is not a string renders the same unreadable line (today's `filter_map(as_str)` silently drops it — a dishonesty removed). `K` is `length`, never `modules.len()`.
3. **One shared validation, two callers (rgr).** The strict walk validation inside `orient_guidance.rs::format_cycle_anchor` (non-string ⇒ None; empty string ⇒ None; < 2 ⇒ None) moves to a free function in the NEW `rgr/src/presentation/cycle_walk_display.rs` (registered in `presentation/mod.rs`); orient's `format_cycle_anchor` calls it and keeps its own chain text (`A -> B -> C -> A`, `first 3 -> ... -> last -> first`) byte-for-byte; explain formats the full ring. Abstraction one-liner: what — one validation function; users — orient's headline chain and explain's block; axis — the honesty rule "a walk is a ring of ≥ 2 non-empty names or it is drift"; rejected alternative — duplicating the 10-line rule in explain (a correctness rule copied is a correctness rule that drifts).
4. **The LiveGraph serve is unchanged** (`walk: None` ⇒ the unordered form): no route gains a walk it cannot verify (P-ECH-02). The stale comments in `agent/src/explain/mod.rs` ("None on this focus-scoped serve") are corrected.

### 2.2 Evidence taxonomy — `walk` and `modules` as explain receives them (one row = one bound test)

| Input shape (EXPLAIN_CYCLES item) | Outcome | Bound test |
|---|---|---|
| `walk` absent (LiveGraph serve; older daemon; today's JSON omits the key) | `members (unordered): …`, zero arrows | `explain_cycles_absent_walk_renders_unordered_no_arrows`; ECH-C07 (vcmi, truncated) |
| `walk` present, valid 3-member ring in an order that differs from `modules`' sort | ring drawn in WALK order, closing on its first member | `explain_cycles_valid_walk_renders_ring_from_walk_not_member_order`; ECH-C06 (leveldb) |
| valid walk shorter than `length` | `(+ M more members in this cycle)` after the ring | `explain_cycles_offwalk_members_reported_as_plus_n_more`; ECH-C06 |
| `walk` present but malformed (`["A", 42]`, `["A", ""]`, `["A"]`) | `cycle walk unreadable on this snapshot — run rmap cycles`, no ring | `explain_cycles_malformed_walk_renders_unreadable_not_ring` |
| `walk` present and empty `[]` | unordered form (legitimate absence, as orient) | `explain_cycles_empty_walk_renders_unordered` |
| a non-string element in `modules` | the unreadable line, never a silently shorter list | `explain_cycles_non_string_module_renders_unreadable_not_dropped` |
| more than eight members, no walk | eight shown `(+ K more)` | `explain_cycles_unordered_list_caps_at_eight_with_plus_k_more` |
| storage: focus read on a real 3-module ring | same `walk` as `find_module_cycles` | `focus_module_cycle_read_carries_the_same_walk_as_the_repo_read`, `focus_path_cycle_read_carries_the_same_walk_as_the_repo_read` |
| dispatcher: explain vs orient on the same ring | equal walks | `explain_symbol_focus_cycle_walk_equals_orient_cycle_walk`, `explain_path_focus_cycle_walk_equals_orient_cycle_walk` |

### 2.3 Outward proof (what a user of the product gains)

`explain leveldb::DBImpl::Recover` shows the same ring `cycles` shows (`util -> helpers/memenv -> util`, `+ 2 more members`) instead of a fabricated four-arrow chain; `explain CGHeroInstance` on vcmi lists the 55 members unordered with zero arrows instead of 54 invented edges; `cycles` and `orient` are untouched. Magnitudes are compared between the two surfaces' outputs, never hardcoded (ECH-C06/C07).

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-003-L05, P-ECH-01 | orient's cycle line, `cycles` render | ECH-C02 (cycle_anchor_*, cross_surface_*, cycles/walk tests); ECH-C09 byte-identity on two corpora |
| RG-REQ-005-L06, RG-REQ-010-L01, RG-REQ-012-L07, P-ECH-03 | other explain sections | ECH-C02; ECH-C10 |
| P-ECH-02 | a fabricated walk on a route without one | ECH-C01 (absent/malformed rows); ECH-C04 route suite; ECH-C07 |
| RG-REQ-011-L06, P-ECH-04 | isolation | ECH-C11 |

## 4. Stop conditions

Frozen: wire shapes other than the additive `walk`/`type_only` on the SQLite explain serve, storage schema, cycle computation, exit codes, the `cycles` renderer, orient's chain text. No new walk computation (the kernel is `agent::cycle_walk`); no walk on the LiveGraph serve. explain's wall time on vcmi is REPORTED before/after, never gated (RG-REQ-011-L11 withdrew the latency floor). STANDING HONESTY RULES. Unmet DoD → STOP. Do NOT commit. Nothing outside the candidate paths.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

1. Failing tests FIRST: the seven `explain_cycles_*` render tests (§2.2), the two storage tests, the two dispatcher tests; then the storage routing, the shared validation, the renderer, the comment fix.
2. Chunked per-crate gates ECH-C01 → C02 → C03 → C04 — NEVER `cargo test --workspace`. Every `/tmp/ech-*.txt` capture is written by THIS cycle's run of the exact allocated command.
3. Live proofs ECH-C05 → C06 → C07 → C08 → C09 → C10 (build `-p rmapd` with `-p repo-graph-rgr` in BOTH trees; each tree's target/release first in PATH; foreground only).
4. ECH-C11 cleanup, ECH-C12 hygiene, hand-off with the evidence object (each check's outcome NESTED under `outcome`).

## 6. Definition of done

All twelve checks pass; §2.3 holds on leveldb and vcmi; `cycles`/`orient` byte-identical; explain's other sections byte-identical; cost reported (no numeric bound).

## 7. Follow-ups (not this slice)

A walk on the LiveGraph explain serve would need the live graph's intra-SCC edges on that route — the same edge cap and kernel; a separate increment if ever wanted (today it renders the honest unordered form).

CORPUS PATHS: leveldb, vcmi at /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/<name>.
