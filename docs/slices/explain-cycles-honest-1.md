<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "EXPLAIN-CYCLES-HONEST-1",
  "baselinePath": "docs/requirements/baselines/EXPLAIN-CYCLES-HONEST-1-INPUT-4.json",
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
    "P-ECH-04",
    "P-ECH-05"
  ],
  "changes": [
    "RG-REQ-012-L06"
  ],
  "acceptanceBoundary": "The explain / cycles / orient human and JSON outputs of the freshly built rmap on fresh isolated indexes of leveldb and vcmi (both the recorded base revision's binary and the candidate), plus cargo test -p repo-graph-rgr (lib presentation incl. explain_cycles_*) -p repo-graph-storage (lib cycle) -p repo-graph-agent (lib cycle) -p repo-graph-daemon-runtime (explain_cycle_walk_route_consistency, cycle_honesty_route_consistency, and the WHOLE --lib unit suite including the five m2_parity_* certificates and the two A-1 delegation tests) as named per check.",
  "candidatePaths": [
    "rust/crates/storage/src/agent_impl.rs",
    "rust/crates/storage/src/agent_cycle_labeling.rs",
    "rust/crates/rgr/src/presentation/explain_sections.rs",
    "rust/crates/rgr/src/presentation/explain.rs",
    "rust/crates/rgr/src/presentation/orient_guidance.rs",
    "rust/crates/rgr/src/presentation/cycle_walk_display.rs",
    "rust/crates/rgr/src/presentation/mod.rs",
    "rust/crates/agent/src/explain/mod.rs",
    "rust/crates/daemon-runtime/tests/explain_cycle_walk_route_consistency.rs",
    "rust/crates/daemon-runtime/src/explain_lg_serve.rs",
    "rust/crates/daemon-runtime/src/explain_lg_serve_tests.rs",
    "rust/crates/daemon-runtime/src/explain_coherence.rs",
    "rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs",
    "rust/crates/daemon-runtime/src/orient_serve/mod.rs",
    "rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs",
    "rust/crates/daemon-runtime/src/orient_serve/tests.rs",
    "rust/crates/repo-graph-coherence/src/lib.rs"
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
        "command": "B=$(python3 -c \"import json;d=json.load(open('.agent-manager/slices/EXPLAIN-CYCLES-HONEST-1/status.json'));v=(d.get('candidateTracking') or {}).get('baseRevision');print(v or '')\"); [ -n \"$B\" ] || B=$(git rev-parse HEAD); test -n \"$B\" && rm -rf /private/tmp/EXPLAIN-CYCLES-HONEST-1-before && git worktree add --detach /private/tmp/EXPLAIN-CYCLES-HONEST-1-before \"$B\" && (cd /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust && cargo build --release -p repo-graph-rgr -p rmapd > /tmp/ech-build-before.log 2>&1 && tail -n1 /tmp/ech-build-before.log && test -x target/release/rmap && test -x target/release/rmapd) && cargo build --release -p repo-graph-rgr -p rmapd --manifest-path rust/Cargo.toml > /tmp/ech-build-cand.log 2>&1 && tail -n1 /tmp/ech-build-cand.log && test -x rust/target/release/rmap && test -x rust/target/release/rmapd && for R in leveldb vcmi; do rm -rf /private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before /private/tmp/EXPLAIN-CYCLES-HONEST-1-$R; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap index ) >/dev/null 2>&1 && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" index ) >/dev/null 2>&1 || exit 1; done && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/leveldb' && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-leveldb-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain leveldb::DBImpl::Recover ) 2>/dev/null > /tmp/ech-ldb-explain-before.txt && grep -q '^  - Cycle 1: db -> helpers/memenv -> table -> util$' /tmp/ech-ldb-explain-before.txt",
        "cwd": ".",
        "environment": "a git worktree of the base revision (status.json candidateTracking.baseRevision when the relay recorded it, else git rev-parse HEAD) built once (before-binary and its rmapd); fresh isolated indexes of leveldb and vcmi with BOTH binaries, throwaway roots, stdio, auto passes off",
        "inputs": "the leveldb and vcmi checkouts; the base revision from .agent-manager/slices/EXPLAIN-CYCLES-HONEST-1/status.json candidateTracking.baseRevision when the relay has recorded it, else git rev-parse HEAD (the committed tip the candidate's uncommitted diff sits on) — a durable, always-available input, tolerant of the missing/null status field (never a KeyError)"
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
        "command": "for R in leveldb vcmi; do for C in cycles orient; do ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap $C ) > /tmp/ech-$R-$C-basebin.raw 2>/dev/null || exit 1; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" $C ) > /tmp/ech-$R-$C-candbin.raw 2>/dev/null || exit 1; test -s /tmp/ech-$R-$C-basebin.raw && test -s /tmp/ech-$R-$C-candbin.raw || exit 1; grep -vE '^Snapshot:|^index basis:' /tmp/ech-$R-$C-basebin.raw > /tmp/ech-$R-$C-basebin.txt; grep -vE '^Snapshot:|^index basis:' /tmp/ech-$R-$C-candbin.raw > /tmp/ech-$R-$C-candbin.txt; cmp /tmp/ech-$R-$C-basebin.txt /tmp/ech-$R-$C-candbin.txt && echo \"$R $C byte-identical across binaries on one index ($(wc -l < /tmp/ech-$R-$C-candbin.txt) lines)\" || exit 1; done; done",
        "cwd": ".",
        "environment": "BOTH binaries served against the SAME index — the ECH-C05 before index of each corpus (leveldb, vcmi); the binary is the only variable",
        "inputs": "the human `cycles` and `orient` outputs of the base binary and the candidate on one index per corpus — captured raw with exit status required, then normalised (Snapshot/index-basis lines)"
      },
      "expected": "exit 0: `cycles` and `orient` render byte-identically between the base binary and the candidate on the SAME index, for leveldb (an ordered ring) and vcmi (an unordered large cycle and ordered small ones) — no-behavior-change for the cycles renderer and orient's cycle line (RG-REQ-003-L05, RG-REQ-004-L07's cycles/orient half, P-ECH-01): extracting the walk validation into a shared function changed nothing they print. (Two independently built indexes are NOT a byte-identity oracle: a walk's ring among equally valid 2-cycles depends on the index — OC-1; CYCLES-WALK-DETERMINISM-1.)"
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
        "command": "for R in leveldb vcmi; do case $R in leveldb) T=leveldb::DBImpl::Recover;; vcmi) T=CGHeroInstance;; esac; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release:$PATH /private/tmp/EXPLAIN-CYCLES-HONEST-1-before/rust/target/release/rmap explain $T ) > /tmp/ech-c10-$R-basebin.txt 2>/dev/null || exit 1; ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/'$R && export RMAP_STATE_ROOT=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before RMAP_SOCKET_PATH=/private/tmp/EXPLAIN-CYCLES-HONEST-1-$R-before/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" explain $T ) > /tmp/ech-c10-$R-candbin.txt 2>/dev/null || exit 1; test -s /tmp/ech-c10-$R-basebin.txt && test -s /tmp/ech-c10-$R-candbin.txt || exit 1; done && python3 -c \"import re\ndef strip(p):\n    s=open(p).read(); s=re.sub(r'^(index basis:|Repo:).*\\n', '', s, flags=re.M)\n    return re.sub(r'^Import cycles \\(\\d+\\)\\n(?:(?:  |    ).*\\n)*', '', s, flags=re.M)\nfor R in ('leveldb','vcmi'):\n    b=strip(f'/tmp/ech-c10-{R}-basebin.txt'); a=strip(f'/tmp/ech-c10-{R}-candbin.txt')\n    assert a==b, f'{R}: explain differs outside the Import cycles block'\n    print(R, 'explain identical outside the Import cycles block (same index, both binaries)')\"",
        "cwd": ".",
        "environment": "BOTH binaries served against the SAME before index of each corpus; this check acquires its own four explain captures (exit status required, non-empty)",
        "inputs": "explain leveldb::DBImpl::Recover and explain CGHeroInstance, base binary and candidate on one index per corpus, captured by this command"
      },
      "expected": "exit 0: with the Import-cycles block and the per-run `index basis:` and `Repo:` lines removed, explain's human output is byte-identical between the base binary and the candidate on the same index, on both corpora — no-behavior-change for every other explain section (P-ECH-03; confidence, seeds, anchors: RG-REQ-005-L06, RG-REQ-010-L01, RG-REQ-012-L07)."
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
        "command": "git diff --check && (cd rust && cargo fmt --check -p repo-graph-rgr -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-agent -p repo-graph-coherence && cargo clippy -p repo-graph-rgr -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-coherence --tests -- -D warnings > /tmp/ech-clippy.log 2>&1 && tail -n1 /tmp/ech-clippy.log) && diff <(git status --short -- rust | grep -vE '^ M rust/crates/storage/src/agent_cycle_labeling.rs$' | sort) <(printf ' M rust/crates/agent/src/explain/mod.rs\\n M rust/crates/daemon-runtime/src/explain_coherence.rs\\n M rust/crates/daemon-runtime/src/explain_lg_serve.rs\\n M rust/crates/daemon-runtime/src/explain_lg_serve_tests.rs\\n M rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs\\n M rust/crates/daemon-runtime/src/orient_serve/mod.rs\\n M rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs\\n M rust/crates/daemon-runtime/src/orient_serve/tests.rs\\n M rust/crates/repo-graph-coherence/src/lib.rs\\n M rust/crates/rgr/src/presentation/explain.rs\\n M rust/crates/rgr/src/presentation/explain_sections.rs\\n M rust/crates/rgr/src/presentation/mod.rs\\n M rust/crates/rgr/src/presentation/orient_guidance.rs\\n M rust/crates/storage/src/agent_impl.rs\\n?? rust/crates/daemon-runtime/tests/explain_cycle_walk_route_consistency.rs\\n?? rust/crates/rgr/src/presentation/cycle_walk_display.rs\\n' | sort)",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "git status/diff of the candidate; rustfmt and clippy over the touched crates (A-1 adds repo-graph-coherence and the eight daemon-runtime/coherence paths)"
      },
      "expected": "exit 0 (A-1: the status list is the 14 modified + 2 new paths of the amended allocation): the diff is whitespace-clean, rustfmt-clean and clippy-clean with -D warnings (clippy's own exit status is required; its output is logged and its last line printed only after success), and — after filtering the optional allowed path rust/crates/storage/src/agent_cycle_labeling.rs out of the actual status — the working tree under rust/ holds exactly the sixteen non-optional always-touched candidate paths (fourteen modified, two new — the printf list above) and nothing else moved. The optional agent_cycle_labeling.rs path (the seventeenth candidate path) is filtered, not required: today `label_module_cycles` already accepts the pre-filtered `Vec<CycleResult>`, so the focus reads call it as-is and that file is not expected to change; the check is decidable and passes whether or not it is touched, and still fails on any OTHER moved path."
    },
    {
      "checkId": "ECH-C13",
      "obligationIds": [
        "P-ECH-02",
        "P-ECH-05",
        "RG-REQ-002-L01",
        "RG-REQ-003-L05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib m2_parity_ 2>&1 | tee /tmp/ech-c13.txt | grep -E '^test result: ok\\. 5 passed; 0 failed' && for t in m2_parity_explain_file_focus_equals_sqlite m2_parity_explain_path_focus_equals_sqlite_with_nonempty_cycle m2_parity_full_serve_equals_sqlite_repo_focus m2_parity_full_serve_equals_sqlite_path_focus m2_parity_full_serve_equals_sqlite_file_focus; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c13.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the five EXISTING M-2 parity certificates in rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs and orient_serve/tests.rs — their assertions are UNCHANGED (only the two path-focus tests' doc comments are corrected: the cycles item is now SQLite-delegated and carries the walk); the two path-focus certificates failed on the third admission's accepted candidate (walk/type_only present on the SQLite serve, absent on the LiveGraph rebuild)"
      },
      "expected": "exit 0: all five parity certificates pass with their assertions unchanged — the two path-focus ones because the M-2 decorator now delegates the focus cycle reads to SQLite (A-1), the repo-focus and the two file-focus ones because the repo-level M-2 cycle serve and every other M-2 leaf remain unchanged (no behavior change; P-ECH-05); no route serves a focus-cycles value that differs from SQLite's (P-ECH-02)"
    },
    {
      "checkId": "ECH-C14",
      "obligationIds": [
        "P-ECH-02",
        "RG-REQ-003-L01",
        "RG-REQ-002-L01",
        "RG-REQ-012-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib focus_cycles_delegated 2>&1 | tee /tmp/ech-c14.txt | grep -E '^test result: ok\\. 2 passed; 0 failed' && for t in explain_path_focus_cycles_delegated_carry_walk_and_sqlite_label orient_path_focus_cycles_delegated_carry_walk; do grep -qE \"^test .*$t .* ok$\" /tmp/ech-c14.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "two NEW tests (A-1). rust/crates/daemon-runtime/src/explain_serve_tests/mod.rs::explain_path_focus_cycles_delegated_carry_walk_and_sqlite_label: on the green fixture (`w.m2.cycle_values` asserted true) the decorator's `run_explain(REPO, MODULE_DIR)` EXPLAIN_CYCLES item carries a NON-EMPTY `walk` equal to the bare SQLite serve's for the same item, and `explain_lg_serve::cycles_leaf_label(&f.state, &snapshot_uid)` is `OrientLeafLabel::SqliteFallback { reason: CoherenceFallbackReason::LiveGraphRenderUnsupported }` (asserted with `matches!`). rust/crates/daemon-runtime/src/orient_serve/tests.rs::orient_path_focus_cycles_delegated_carry_walk: the decorator's path-focus `orient(REPO, Some(MODULE_DIR))` IMPORT_CYCLES item carries the same NON-EMPTY `walk` as the bare SQLite serve. Both assert the walk is non-empty (non-vacuous: the fixture's real src <-> lib ring)"
      },
      "expected": "exit 0: with a resident LiveGraph and a GREEN cycle-values cert, explain's and orient's path-focus cycle items carry the verified walk (the SQLite value, delegated), explain's cycles leaf is labelled sqlite with reason LiveGraphRenderUnsupported (never a livegraph label over a SQLite value, never an unordered LiveGraph value replacing the ring), and `explain --json` carries `walk` on this route too (RG-REQ-012-L06, reported)"
    },
    {
      "checkId": "ECH-C15",
      "obligationIds": [
        "P-ECH-05",
        "RG-REQ-003-L05",
        "RG-REQ-005-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib 2>&1 | tee /tmp/ech-c15.txt | grep -E '^test result: ok\\. [0-9]+ passed; 0 failed' && ! grep -E '^test .* FAILED$' /tmp/ech-c15.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the WHOLE daemon-runtime unit suite (about 780 tests; the operator gate suite's unit for this crate, which the third admission's acceptance boundary omitted); run to completion in the foreground"
      },
      "expected": "exit 0: every daemon-runtime unit test passes — no behavior change anywhere else in the daemon (the (b) leaves, module-summary, callers/callees, imports, complexity serves remain as before; P-ECH-05)"
    },
    {
      "checkId": "ECH-C16",
      "obligationIds": [
        "P-ECH-02",
        "RG-REQ-003-L01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "! grep -n 'fn serve_cycles(' rust/crates/daemon-runtime/src/explain_lg_serve.rs && ! grep -n 'fn cycle_involves(' rust/crates/daemon-runtime/src/explain_lg_serve.rs && ! grep -n 'cycles_qualified_filtered' rust/crates/daemon-runtime/src/orient_serve/storage_port_impl.rs && grep -n 'fn cycles_leaf_label' rust/crates/daemon-runtime/src/explain_lg_serve.rs && grep -n 'LiveGraphRenderUnsupported' rust/crates/daemon-runtime/src/explain_lg_serve.rs && ! grep -n 'response shape (reserved)' rust/crates/repo-graph-coherence/src/lib.rs && ! grep -n 'find_cycles_involving_\\*' rust/crates/daemon-runtime/src/orient_serve/mod.rs",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "the candidate sources: explain_lg_serve.rs (the LiveGraph focus-cycle rebuild and `cycle_involves` removed with their only callers; the leaf-label function named `cycles_leaf_label`), explain_lg_serve_tests.rs (the `cycle_involves` unit tests removed), orient_serve/storage_port_impl.rs (`cycles_qualified_filtered` removed), orient_serve/mod.rs (`M2LeafServe::cycle_values` doc names the repo-level read only), repo-graph-coherence/src/lib.rs (`LiveGraphRenderUnsupported` documented for this use, no longer '(reserved)')"
      },
      "expected": "exit 0: no dead LiveGraph focus-cycle rebuild remains; every name matches what the code does (a function that only labels is not called serve; a witness field's comment does not claim reads it no longer gates; a reused reason is not marked reserved). The three removal greps are scoped to each function's OWN source file (explain_lg_serve.rs for serve_cycles/cycle_involves, storage_port_impl.rs for cycles_qualified_filtered) with a literal open-paren where needed, NOT a recursive rust/crates/daemon-runtime/src scan — otherwise the negated grep can never exit 0, because the substring 'fn serve_cycles' also matches the FROZEN fn serve_cycles_fastpath / fn serve_cycles_sqlite in livegraph_feed.rs (untouched by this slice) and the generated *_MAP.md sidecars still name the removed functions. So decidability holds: the negated greps pass only when the three functions are gone from their own files; that cycle_involves' unit tests were removed with it is enforced by compilation (ECH-C13/C14/C15 would fail to build otherwise)."
    }
  ]
}
-->

# EXPLAIN-CYCLES-HONEST-1 — `explain`'s Import-cycles block draws arrows only over a verified walk, the same walk `cycles` and `orient` draw

Status: ALLOCATED (2026-09-18; specified 2026-09-12) · Track: audit round six, Q4 (HIGH; never worked — the honest form shipped twice for `cycles` and `orient` and never reached `explain`); ratified order position 3 of the remaining queue (human 2026-09-14). CODE slice: the two SQLite focus-scoped cycle reads route through the existing labeling kernel (so a verified walk reaches explain), the explain cycle renderer draws only from that walk, the strict walk validation becomes one shared function with two callers. Maturity: MATURE (`explain`, `cycles`, `orient`). Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13). Every literal in the checks above was read from the product on 2026-09-18 (fresh isolated leveldb and vcmi indexes; current sources) — not from memory.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-003-L01 (`explain` draws no cycle arrow without a verified walk; with a walk it draws the ring), RG-REQ-002-L01 (no surface asserts a relationship the store does not hold — the cycle-arrow half), RG-REQ-004-L07 (the unordered/ring rule on every surface — explain was the surface where it was NOT MET).

**Changes (pre-authorised, REPORTED never predicted):** RG-REQ-012-L06 — `explain --json` on the SQLite route now carries `walk` (a verified ring) and `type_only` in its EXPLAIN_CYCLES items; additive (older daemons carry neither; the M-2 LiveGraph decorator delegates the focus cycle reads to SQLite — A-1 — so explain's and orient's path- and module-focus items carry the same fields on every route, ECH-C14); the human ring is drawn from this very field, so the two modes answer the same question (ECH-C08).

**Preserves:** RG-REQ-003-L05 (orient's cycle line — its chain formatter keeps its text; ECH-C02, ECH-C09), RG-REQ-005-L06 / RG-REQ-010-L01 / RG-REQ-012-L07 (explain's confidence, seed and anchor rules — ECH-C02, ECH-C10), RG-REQ-011-L06 (isolation — ECH-C11).

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-ECH-01 | `cycles` and `orient` render byte-identically before/after on leveldb and vcmi; their renderer tests stay green | ECH-C02; ECH-C09 |
| P-ECH-02 | No route gains a walk it cannot verify, and no route serves a focus-cycles value that differs from SQLite's: any cycle whose edge set is truncated (vcmi's 55-module cycle) renders the unordered form on every route; the M-2 LiveGraph decorator cannot compute the walk, so it delegates the focus cycle reads to SQLite and explain's cycles leaf is labelled `sqlite` + `LiveGraphRenderUnsupported` (A-1, D-ECH-002); the route-consistency suite and the five parity certificates stay green | ECH-C01; ECH-C04; ECH-C07; ECH-C13; ECH-C14 |
| P-ECH-03 | Every explain section other than Import cycles is byte-identical before/after | ECH-C02; ECH-C10 |
| P-ECH-04 | Every proof isolated; the operator's registry digest unchanged; nothing left behind | ECH-C11 |
| P-ECH-05 | The repo-level orient headline's M-2 LiveGraph serve (`find_module_cycles{,_cancellable}` via `m2_module_cycles`) and every other M-2 / (b) leaf are unchanged — the repo-level decoration asymmetry (no split / type_only / walk on the LiveGraph route) stands as ratified by ORIENT-CYCLES-DISAGREE-1 2(1)/2(5), TYPE-ONLY-IMPORTS-1 and COHERENCE-3 and is a recorded residual of IMPORTS-WITNESS-UNION-1, not this slice's; the daemon-runtime unit suite stays green | ECH-C13; ECH-C15 |

## 1. Problem (ROOT-CAUSED — RC-4, docs/audits/2026-09-08-root-causes-v0.18.0.md)

`rust/crates/rgr/src/presentation/explain_sections.rs:226–249 render_cycles` joins `item["modules"]` — the SCC member set, SORTED by `canonicalize_cycles` — with `" -> "`, asserting edges that do not exist. Observed today on fresh isolated indexes: leveldb `explain leveldb::DBImpl::Recover` renders `  - Cycle 1: db -> helpers/memenv -> table -> util` while `cycles` on the same index renders the verified ring `util -> helpers/memenv -> util` `(+ 2 more members in this cycle)` and `orient` says `1 import cycle (util -> helpers/memenv -> util)`; vcmi `explain CGHeroInstance` renders `Cycle 1: AI/BattleAI -> AI/EmptyAI -> AI/MMAI -> …` (55 modules) while `cycles` honestly renders `members (unordered): AI/BattleAI, AI/EmptyAI, … (+ 47 more)` because that cycle's edge set exceeds `CYCLE_EDGE_CAP`. Why: both focus reads (`storage/src/agent_impl.rs`, the `find_cycles_involving_module` / `_path` pairs — four `AgentCycle { walk: None, type_only: None, .. }` construction sites) hand-build their cycles instead of passing them through `agent_cycle_labeling::label_module_cycles`, the kernel `find_module_cycles` (orient's SQLite serve) uses to attach the verified walk (`intra_cycle_edges` + `find_cycle_walk` over `module_import_edges`) and the type-only verdict; the LiveGraph serve (`daemon-runtime/src/explain_lg_serve.rs::serve_cycles`) carries no walk either. Never worked: the render line exists since 82b6557; CYCLE-HONESTY-1 and COHERENCE-3 built the honest form for `cycles`/`orient` and never touched `explain_sections.rs`.

## 2. Contract

### 2.1 The fix, at the cause — a vertical slice, no dormant branch

1. **The focus reads carry the verified walk (storage).** `find_cycles_involving_module{,_cancellable}` and `find_cycles_involving_path{,_cancellable}` filter the raw cycles as today and then route the FILTERED cycles through `label_module_cycles` (the same call `find_module_cycles` makes), so the returned `AgentCycle`s carry `walk` (Some for a verified ring, None where the edge set is truncated) and `type_only`. One derivation for `cycles`, `orient` and `explain` (RG-REQ-004-L07). If `label_module_cycles` needs an entry that accepts the pre-filtered `CycleResult`s, it lives in `agent_cycle_labeling.rs` (the ninth allowed path). Cost: two snapshot reads (module names, tracked files) and per-cycle edge selection over the focus's few cycles — reported on vcmi (ECH-C07; a cost the human reads, not a bound).
2. **The renderer draws only from the walk (rgr).** `render_cycles`: `Cycle N (K modules): A -> B -> A` when `item["walk"]` is a valid ring (≥ 2 non-empty strings), followed by `(+ M more members in this cycle)` when the ring visits fewer members than `length` — the exact two-line form of `cycles/walk.rs`, under explain's bullet; `Cycle N (K modules): members (unordered): A, B, … (+ K more)` (eight shown, as `cycles`) when `walk` is absent, null or empty; `Cycle N (K modules): cycle walk unreadable on this snapshot — run rmap cycles` when `walk` is present but malformed (a non-string element, an empty string, fewer than two members — the orient rule, RG-REQ-003-L05); a `modules` entry that is not a string renders its own unreadable line naming what is unreadable — `cycle members unreadable on this snapshot — run rmap cycles` — never a silently shorter list (today's `filter_map(as_str)` drops it; OC-4: the first text said 'the same unreadable line' as a malformed walk, which would blame the walk for a corrupt member list). `K` is `length`, never `modules.len()`.
3. **One shared validation, two callers (rgr).** The strict walk validation inside `orient_guidance.rs::format_cycle_anchor` (non-string ⇒ None; empty string ⇒ None; < 2 ⇒ None) moves to a free function in the NEW `rgr/src/presentation/cycle_walk_display.rs` (registered in `presentation/mod.rs`); orient's `format_cycle_anchor` calls it and keeps its own chain text (`A -> B -> C -> A`, `first 3 -> ... -> last -> first`) byte-for-byte; explain formats the full ring. Abstraction one-liner: what — one validation function; users — orient's headline chain and explain's block; axis — the honesty rule "a walk is a ring of ≥ 2 non-empty names or it is drift"; rejected alternative — duplicating the 10-line rule in explain (a correctness rule copied is a correctness rule that drifts).
4. **The LiveGraph route serves the focus-cycles leaf only when it can reproduce the whole value — it cannot compute the walk, so it delegates to SQLite (A-1, D-ECH-002).** The verified walk is a per-edge fact of the SQLite module edge set; the LiveGraph's dirname-aggregated module edges are not certified equal to it (`livegraph_feed.rs:2517-2524`), so a LiveGraph walk would not be certifiable and an unordered LiveGraph value would replace the ring on TypeScript repos with a resident LiveGraph (`explain_coherence.rs` swaps the leaf unconditionally). Therefore: (a) `orient_serve/storage_port_impl.rs` — `find_cycles_involving_path{,_cancellable}` and `find_cycles_involving_module{,_cancellable}` delegate to `self.inner` unconditionally; `cycles_qualified_filtered` is removed; orient_serve/mod.rs's cycle-values leaf docs (BOTH the module-level `//!` leaf list and `M2LeafServe::cycle_values`'s doc comment) name the repo-level `find_module_cycles*` read only, so the literal `find_cycles_involving_*` no longer appears anywhere in mod.rs (ECH-C16). The repo-level `find_module_cycles{,_cancellable}` M-2 serve is UNCHANGED (P-ECH-05). (b) `explain_lg_serve.rs` — `serve_cycles` no longer rebuilds the leaf; it becomes `cycles_leaf_label(repo_state, snapshot_uid) -> OrientLeafLabel`: the existing `orient_cycles_outcome` gate's fallback label when the gate is not green, else `OrientLeafLabel::SqliteFallback { reason: CoherenceFallbackReason::LiveGraphRenderUnsupported }` (the LiveGraph answer cannot be rendered into the response shape — the shape now carries the walk); `explain_coherence.rs` sets `decisions.cycles = Some(label)` and adds no replacement; `cycle_involves` and its unit tests (explain_lg_serve_tests.rs) are removed with their only caller. (c) `repo-graph-coherence/src/lib.rs` — `LiveGraphRenderUnsupported`'s doc comment names this use instead of '(reserved)'. (d) Two delegation tests (§2.2, ECH-C14) and the five existing parity certificates (ECH-C13) bind it. The stale comments in `agent/src/explain/mod.rs` ("None on this focus-scoped serve") are corrected. No corpus proof exists for the LiveGraph route (a dev-only TypeScript path); the fixture tests are the seam.

### 2.2 Evidence taxonomy — `walk` and `modules` as explain receives them (one row = one bound test)

| Input shape (EXPLAIN_CYCLES item) | Outcome | Bound test |
|---|---|---|
| `walk` absent (older daemon; a truncated edge set; today's JSON omits the key) | `members (unordered): …`, zero arrows | `explain_cycles_absent_walk_renders_unordered_no_arrows`; ECH-C07 (vcmi, truncated) |
| A-1: resident LiveGraph, GREEN cycle-values cert, path focus (explain and orient) | the SQLite value with its non-empty `walk`; explain's cycles leaf labelled `sqlite` + `LiveGraphRenderUnsupported` | `explain_path_focus_cycles_delegated_carry_walk_and_sqlite_label`, `orient_path_focus_cycles_delegated_carry_walk`; the five `m2_parity_*` certificates unchanged (ECH-C13) |
| `walk` present, valid 3-member ring in an order that differs from `modules`' sort | ring drawn in WALK order, closing on its first member | `explain_cycles_valid_walk_renders_ring_from_walk_not_member_order`; ECH-C06 (leveldb) |
| valid walk shorter than `length` | `(+ M more members in this cycle)` after the ring | `explain_cycles_offwalk_members_reported_as_plus_n_more`; ECH-C06 |
| `walk` present but malformed (`["A", 42]`, `["A", ""]`, `["A"]`) | `cycle walk unreadable on this snapshot — run rmap cycles`, no ring | `explain_cycles_malformed_walk_renders_unreadable_not_ring` |
| `walk` present and empty `[]` | unordered form (legitimate absence, as orient) | `explain_cycles_empty_walk_renders_unordered` |
| a non-string element in `modules` | `cycle members unreadable on this snapshot — run rmap cycles`, never a silently shorter list | `explain_cycles_non_string_module_renders_unreadable_not_dropped` |
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
| P-ECH-02 | a fabricated walk on a route without one; a LiveGraph-served focus value without the walk (route-dependent output) | ECH-C01 (absent/malformed rows); ECH-C04 route suite; ECH-C07; ECH-C13 parity certificates; ECH-C14 delegation tests |
| P-ECH-05 | the repo-level M-2 cycle serve or any other M-2 / (b) leaf | ECH-C13 (repo- and file-focus certificates); ECH-C15 (whole daemon-runtime unit suite) |
| RG-REQ-011-L06, P-ECH-04 | isolation | ECH-C11 |

## 4. Stop conditions

Frozen: wire shapes other than the additive `walk`/`type_only` on the SQLite explain serve, storage schema, cycle computation, exit codes, the `cycles` renderer, orient's chain text. No new walk computation (the kernel is `agent::cycle_walk`); no walk computed on the LiveGraph route (it delegates — A-1); the repo-level `find_module_cycles` M-2 serve, `orient_cycles_outcome`, the cycles cert and the LiveGraph feed are untouched. explain's wall time on vcmi is REPORTED before/after, never gated (RG-REQ-011-L11 withdrew the latency floor). STANDING HONESTY RULES. Unmet DoD → STOP. Do NOT commit. Nothing outside the candidate paths.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

1. Apply the preserved accepted candidate (RESUME NOTE in the packet) — it already holds the seven `explain_cycles_*` render tests (§2.2), the two storage tests, the two dispatcher tests, the storage routing, the shared validation, the renderer and the comment fix; confirm with ECH-C01 → C04.
2. A-1 (§2.1 point 4): the two delegation tests FIRST (they fail on the applied candidate: the decorator's path-focus item carries no walk), then the port delegation, the explain leaf label, the removals and the comment corrections; ECH-C13 → C14 → C16.
3. Chunked per-crate gates ECH-C01 → C02 → C03 → C04, then ECH-C15 (the whole daemon-runtime unit suite, foreground) — NEVER `cargo test --workspace`. Every `/tmp/ech-*.txt` capture is written by THIS cycle's run of the exact allocated command.
4. Live proofs ECH-C05 → C06 → C07 → C08 → C09 → C10 (build `-p rmapd` with `-p repo-graph-rgr` in BOTH trees; each tree's target/release first in PATH; foreground only).
5. ECH-C11 cleanup, ECH-C12 hygiene, hand-off with the evidence object (each check's outcome NESTED under `outcome`).

## 6. Definition of done

All sixteen checks pass; §2.3 holds on leveldb and vcmi; the five M-2 parity certificates pass unchanged and the two delegation tests prove the ring reaches the LiveGraph route by delegation; `cycles`/`orient` byte-identical; explain's other sections byte-identical; cost reported (no numeric bound).

## 7. Follow-ups (not this slice)

IMPORTS-WITNESS-UNION-1 (docs/TECH-DEBT.md 2026-09-19; D-ECH-002 direction 1, deferred by the human): the two engines are two witnesses of the import graph — SQLite (every language, repo-graph's own resolution stages) and the LiveGraph (TypeScript via scip-typescript, AST imports plus an in-memory tsconfig-alias / literal-dynamic overlay); a walk on the LiveGraph route, per-edge witness provenance for imports/cycles and the repo-level orient headline's decoration asymmetry all belong to it. TS-ALIAS-RESOLUTION-1 (D-ECH-002 direction 2, now — awaits ordering): a tsconfig `paths` alias resolution stage in the SQLite indexer (RG-REQ-006-L04) so that `FRAKTAG/packages/ui/src/components/ui/separator.tsx:4 import { cn } from "@/lib/utils"` (today `unresolved_edges` basis `SpecifierMatchesProjectAlias`, no module edge) resolves to `packages/ui/src/lib/utils.ts` on every route. Catalog corrections CC-1/CC-2 queued in docs/assurance/RG-BOOTSTRAP/catalog-corrections.md.

CORPUS PATHS: leveldb, vcmi at /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/<name>.

## 8. Oracle corrections (ledger: docs/assurance/EXPLAIN-CYCLES-HONEST-1/oracle-corrections.md)

- OC-1 (2026-09-19; ECH-C09; carried as INPUT-2): the byte-identity oracle compared two independently built indexes; a walk's ring among equally valid 2-cycles depends on the index (vcmi: `client/renderSDL -> client/render/hdEdition -> client/renderSDL` on one build, `client/gui -> client/renderSDL -> client/gui` on another — pre-existing, CYCLES-WALK-DETERMINISM-1), so the oracle measured index non-determinism, not the renderer. Corrected to both binaries on the same index. Found by the builder at cycle 3.
- OC-2 (2026-09-19; ECH-C10; carried as INPUT-2): explain's per-index `Repo: repo_<uid>` line was not stripped; corrected (same-index comparison and the line stripped). Found by the builder at cycle 3.
- OC-3 (2026-09-19; ECH-C05; INPUT-2 refinement, finding ECH-PREP2-F01): the base-revision lookup indexed `status.json['candidateTracking']['baseRevision']` directly, which KeyErrors at document time because the relay has not yet recorded `candidateTracking` — the check was not decidable in isolation. Corrected to read the field tolerantly (missing/null → empty) and fall back to `git rev-parse HEAD` (a durable, always-available input; the committed tip the candidate's uncommitted diff sits on), requiring a non-empty revision before the worktree is created. No allocation/check-id/requirement/candidate-path change; a method correction only. Found by the INPUT-2 document review (codex gpt-5.6-terra).
- OC-4 (2026-09-19; §2.1.2 and the §2.2 taxonomy row for a non-string `modules` entry; carried as INPUT-3): the text said such an entry renders 'the same unreadable line' as a malformed walk; the candidate renders `cycle members unreadable on this snapshot — run rmap cycles`, which names what is unreadable. The text is corrected to the members line (a message blaming the walk for a corrupt member list would be a wrong name); the bound test name is unchanged. Found by the review of the second admission's cycle 2 (ECH-IR-002).

## 9. Amendments (allocation-changing — never an oracle correction; each carried as a new baseline with its own document review)

- A-1 (2026-09-19; D-ECH-002, human decision; carried as INPUT-4): the third admission's accepted candidate (13/13, 12/12) failed the operator gate suite on two M-2 parity certificates outside the allocation (`m2_parity_explain_path_focus_equals_sqlite_with_nonempty_cycle`, `m2_parity_full_serve_equals_sqlite_path_focus`): the SQLite focus reads now carry `walk`/`type_only`, the LiveGraph rebuild carries neither, and on a TypeScript repo with a resident LiveGraph the decorator would have replaced the ring with an unordered value (route-dependent output; the manager's packet said 'the LiveGraph serve is unchanged' without listing the certificates in the regression watch, and the acceptance boundary omitted `daemon-runtime --lib` — a manager oracle defect). Amendment: §2.1 point 4 (the LiveGraph route delegates the focus cycle reads to SQLite and labels explain's leaf `sqlite` + `LiveGraphRenderUnsupported`); P-ECH-02 rewritten; P-ECH-05 added; candidate paths +8 (explain_lg_serve.rs, explain_lg_serve_tests.rs, explain_coherence.rs, orient_serve/storage_port_impl.rs, orient_serve/mod.rs, explain_serve_tests/mod.rs, orient_serve/tests.rs, repo-graph-coherence/src/lib.rs); checks ECH-C13..C16 added; ECH-C12's status list and crate list extended; acceptance boundary includes the whole daemon-runtime unit suite; §0 Changes note, §2.2 row, §3, §4, §5, §6, §7 updated. The repo-level orient headline's M-2 serve is outside this amendment (ratified asymmetry; P-ECH-05).
