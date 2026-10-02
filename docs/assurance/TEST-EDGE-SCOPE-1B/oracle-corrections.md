# TEST-EDGE-SCOPE-1B — allocation amendments and oracle corrections (append-only ledger)

Rules for this file: append only. An entry is never edited or deleted. A later understanding is a new, dated entry that references the earlier one. Each entry names the baseline it enters, its authority, the historical result it supersedes (kept verbatim), the correction, and the evidence for the correction.

Subject: the allocation block of `docs/slices/test-edge-scope-1b.md`. Admitted baseline before these entries: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-1.json` (committed at f1a09ec3). Baseline these entries enter: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-2.json`.

Trigger: the implementation review-0 of the admission-1 candidate (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/review-0.json`, verdict `decision-required`; candidate saved at `.agent-manager/slices/TEST-EDGE-SCOPE-1B/candidate-admission-1.patch`, 85 paths). The operator took both recommended options (TESB-PROVENANCE-ALLOCATION → amend; TESB-C03-ORACLE → correct the check) and ordered the review's finding 2 closed as a class. Author: document item TEST-EDGE-SCOPE-1B-PREP-3 (builder role, claude-opus-5-5). Independent review of these entries: pending (the PREP-3 reviewer).

---

## A-1 — 2026-09-30 — allocation amendment: cycle provenance follows the served view

- **Authority.** Implementation review-0, DECISION_REQUIRED `TESB-PROVENANCE-ALLOCATION`. Recommended option: "Amend the allocation and test both label paths." The operator took it.
- **Finding (review-0, blocking 1).** "`orient_serve/storage_port_impl.rs` correctly declines LiveGraph cycle values when the default view excludes a directory import. But unchanged `orient_lg_decisions.rs::orient_cycles_outcome` checks only the unpartitioned cycle certificate; `orient_coherence.rs` uses that result to label `IMPORT_CYCLES`. `explain_lg_serve.rs::cycles_leaf_label` similarly can give the wrong fallback reason." Violated: RG-REQ-002-L02; P-TESB-05's honesty obligation.
- **Verified at HEAD (f1a09ec3; `rust/` identical to a85f6239).**
  - `orient_coherence.rs:119-124`: `decisions.import_cycles = Some(map_outcome(orient_cycles_outcome(repo_state, &snapshot_uid)))`. This only maps; the decision is `orient_lg_decisions.rs:328-376`, gated on `lg.module_import_cycles()` and the `cycles_cert` verdict.
  - `explain_lg_serve.rs:151-159`: `cycles_leaf_label` maps `OrientLgOutcome::Livegraph` to `LiveGraphRenderUnsupported` and `Fallback` through `map_outcome`.
  - The admission-1 fastpath gives `FallbackReason::LiveGraphCycleDivergence` when the view excludes an import (candidate livegraph_feed.rs, D-TESB-06).
- **Class and rule.** Class: a provenance label decided from a certificate over a different value than the one served. Rule (D-TESB-16): one predicate, inside `orient_cycles_outcome`, which both cycle leaves read. When D-TESB-06's excluded directory-import count for the DEFAULT view is not 0, the outcome is `Fallback { LiveGraphCycleDivergence }`, before any certificate is consulted. A failed read of the count is never taken as "excludes nothing". Otherwise the shipped behaviour stands. No new coherence reason, because repo-graph-coherence is outside the ten crates (TESB-C09).
- **Allocation change.** Candidate paths 85 → 88:
  - `rust/crates/daemon-runtime/src/orient_lg_decisions.rs` (the predicate);
  - `rust/crates/daemon-runtime/src/orient_lg_decisions/served_e2e.rs` (the tests, on its resident-LiveGraph GREEN-certificate fixture);
  - `rust/crates/daemon-runtime/src/explain_lg_serve.rs` (the `cycles_leaf_label` doc contract; the check itself if the builder places it there, one definition either way).

  Each was verified present at HEAD. `orient_coherence.rs` is NOT added: it only maps the outcome. `callgraph_cert/test_fixture.rs` keeps its one-token limit. §2.3 gains rows R17b and R17c. The daemon call-site inventory gains the new storage call.
- **Bound tests (TESB-C06; floor 853 → 863 with A-2's six).**
  - Orient, nothing excluded: `orient_cycles_outcome_serves_livegraph_with_green_cert`, `build_orient_envelope_repo_focus_cycles_leaf_is_livegraph` (existing).
  - Orient, an import excluded: `orient_cycles_outcome_falls_back_when_the_default_view_excludes_an_import_despite_a_green_cert`, `build_orient_envelope_repo_focus_cycles_leaf_is_sqlite_when_the_default_view_excludes_an_import`.
  - Explain, nothing excluded: `explain_cycles_leaf_label_is_render_unsupported_with_a_green_cert_when_nothing_is_excluded`.
  - Explain, an import excluded: `explain_cycles_leaf_label_is_cycle_divergence_when_the_default_view_excludes_an_import`.
  - Served-level, existing and unchanged: `explain_path_focus_cycles_delegated_carry_walk_and_sqlite_label`.
- **Limit.** No field check asserts the label (leveldb and poco are not LiveGraph-served; TESB-C13 does not read FRAKTAG's provenance).

- **Addendum 2026-09-30 (INPUT-2 cycle 2) — ruling TESB-PARTITION-FALLBACK-REASON.** The text above is kept unchanged as written at cycle 1. Where it conflicts with this addendum, this addendum governs.
  - **Authority.** PREP-3 review-0 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B-PREP-3/review-0.json`, decision-required) raised DECISION_REQUIRED `TESB-PARTITION-FALLBACK-REASON` with the recommendation "Add the distinct partition-view reason". The operator (in-place-manager) took it; the human may override.
  - **Finding.** Cycle 1's rule assigned `LiveGraphCycleDivergence` when the view excludes an import under a GREEN certificate. At HEAD that reason means "the repo module-cycle no-loss certificate is NOT GREEN" (`livegraph_feed.rs:219-222`; `repo-graph-coherence/src/lib.rs:94-95`). An excluded but redundant import need not change any SCC, so the label would assert a divergence the evidence does not establish (RG-REQ-002-L02/L04).
  - **Superseded.** Cycle 1's choice of `LiveGraphCycleDivergence`, its statement "no new coherence vocabulary", and its statement that `orient_coherence.rs` is not needed.
  - **Ruling as recorded.** A SQLite cycle answer declined solely because the served view excludes imports carries a new fallback reason. Final name: `LiveGraphPartitionedViewUnsupported` (wire string identical). It keeps the `LiveGraph*` prefix every variant uses, and it states the cause: the LiveGraph IR carries neither the resolution class nor the importer's test status, so it cannot answer an import view that excludes imports. It asserts nothing about the certificate or the SCCs. It is added to `livegraph_feed.rs::FallbackReason` and to its mirror `repo_graph_coherence::CoherenceFallbackReason`, and rendered in JSON (orient's and explain's leaf `provenance.fallback_reason`; `cycles`' `fallback_reason`) and in human output wherever a reason renders today (orient `--full`'s `fallback:` line; explain's human output renders no provenance reason for any leaf, and `cycles` human strips `fallback_reason`, both unchanged). It is ADDITIVE: a new value of existing fields; nothing removed or renamed; `LiveGraphCycleDivergence` keeps its meaning. A client built before the variant cannot decode the new value strictly; rmap and rmapd ship as one version (RG-REQ-014-L06).
  - **Sites (a grep at HEAD f1a09ec3 of every `FallbackReason::` and `CoherenceFallbackReason::` use in `rust/crates`; each exhaustive match and relevant construction read).**
    - Exhaustive matches: `livegraph_feed.rs:252-271` (`FallbackReason::as_str`); `repo-graph-coherence/src/lib.rs:117-140` (`CoherenceFallbackReason::as_str`); `orient_coherence.rs:243-278` (`map_fallback`).
    - Non-exhaustive: `agent/src/dto/coherent.rs:464-469` (`_` arm; unchanged — the new reason, like the divergence reasons, adds no envelope limit); `livegraph_feed.rs:1749` (a match over `Option`).
    - Constructions changed: the `cycles` auto fastpath's excluded-view refusal (`livegraph_feed.rs::cycles_auto_response`, :2845; admission-1 patch line 5421); the new predicate in `orient_lg_decisions.rs::orient_cycles_outcome`.
    - Constructions unchanged: `livegraph_feed.rs:2762` and `orient_lg_decisions.rs:371` (`LiveGraphCycleDivergence`, certificate not GREEN); `explain_lg_serve.rs:155` (`LiveGraphRenderUnsupported`).
    - Renderers: `rgr/src/presentation/orient.rs:288-289` (generic `as_str`, unchanged); `rgr/src/commands/graph.rs:536/659/762/932` (strip the reason from human, unchanged).
    - Every other use constructs or asserts another variant (`explain_coherence.rs`, `orient_lg_decisions/complexity_cert.rs`, `union_serve/mod.rs`, `explain_lg_identity.rs`, agent `dto/coherent.rs`, test files).
  - **Allocation change.** Paths 88 → 90: `rust/crates/daemon-runtime/src/orient_coherence.rs` and `rust/crates/repo-graph-coherence/src/lib.rs`. repo-graph-coherence becomes the eleventh touched crate. TESB-C09 excludes it from the byte-identity check, pins its diff to `src/lib.rs`, keeps its `Cargo.toml` unchanged, and runs its suite (floor 24, EXECUTED `cargo test -p repo-graph-coherence -- --list` at HEAD, + 1). TESB-C10 formats and lints it.
  - **Tests.** Each excluded-import case holds a GREEN certificate and comes in two forms: an exclusion that changes the SCCs and one that leaves them unchanged. Both carry the new reason, never a divergence.
    - Orient: `orient_cycles_outcome_names_the_partitioned_view_when_an_excluded_import_changes_the_sccs_despite_a_green_cert`, `orient_cycles_outcome_names_the_partitioned_view_when_an_excluded_import_leaves_the_sccs_unchanged_despite_a_green_cert`, `build_orient_envelope_cycles_leaf_serializes_the_partitioned_view_reason_never_a_divergence`.
    - Explain: `explain_cycles_leaf_label_names_the_partitioned_view_when_an_excluded_import_changes_the_sccs`, `explain_cycles_leaf_label_names_the_partitioned_view_when_an_excluded_import_leaves_the_sccs_unchanged`, `build_explain_envelope_cycles_leaf_serializes_the_partitioned_view_reason_never_a_divergence`.
    - The divergence meaning kept: `orient_cycles_outcome_with_a_non_green_cert_and_nothing_excluded_stays_cycle_divergence`, existing `cycles_fastpath_not_eligible_falls_back_to_sqlite`.
    - The fastpath: `cycles_fastpath_refused_for_an_excluded_import_names_the_partitioned_view_not_a_divergence`.
    - Human: `orient_full_serving_line_names_the_partitioned_view_fallback` (TESB-C07).
    - Mirror: `partitioned_view_reason_as_str_and_serde_name_match_and_differ_from_cycle_divergence` (TESB-C09).
    - Withdrawn cycle-1 identities: `orient_cycles_outcome_falls_back_when_the_default_view_excludes_an_import_despite_a_green_cert`, `build_orient_envelope_repo_focus_cycles_leaf_is_sqlite_when_the_default_view_excludes_an_import`, `explain_cycles_leaf_label_is_cycle_divergence_when_the_default_view_excludes_an_import`.

## OC-1 — 2026-09-30 — oracle correction: TESB-C03's type-only source guard

- **Authority.** Implementation review-0, DECISION_REQUIRED `TESB-C03-ORACLE`. Recommended option: "Correct the check, retain the failure record, and rerun." The operator took it.
- **Historical result, kept.** The approved INPUT-1 TESB-C03 command ended with this clause:

  ```
  … && grep -q 'repo_graph_classification::import_partition' crates/storage/src/import_partition_reads.rs && grep -q 'type_only_disposition_of' crates/storage/src/directory_module_edges.rs && git -C .. diff --quiet HEAD -- rust/crates/storage/src/migrations rust/crates/storage/Cargo.toml
  ```

  On the admission-1 candidate the builder recorded (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/build-progress.md`): "TESB-C03 EXECUTED exit 1 — FAIL on ONE clause only … all 54 named tests `... ok` …, `--list` 803 ≥ 802 … The failing clause: `grep -q 'type_only_disposition_of' crates/storage/src/directory_module_edges.rs` (exit 1)." Review-0 confirmed that the check, as written, contradicted its own specified placement, and that the code must not be moved to satisfy it. **TESB-C03 on admission-1 is FAIL, and that record stands.**
- **Why the oracle was wrong.** D-TESB-02/03 and C03's own inputs place the per-import PARSE (`type_only_disposition_of`) in the row read `import_partition_reads.rs`, whose rows the directory graph consumes, and the conjunctive AGGREGATE (`aggregate_module_edge_type_only`) in `directory_module_edges.rs`. The grep looked for the parse in the aggregate's file.
- **Corrected clause (text only; the rest of the command is unchanged).**

  ```
  … && grep -q 'repo_graph_classification::import_partition' crates/storage/src/import_partition_reads.rs && grep -q 'type_only_disposition_of(' crates/storage/src/import_partition_reads.rs && grep -q 'aggregate_module_edge_type_only(' crates/storage/src/directory_module_edges.rs && test "$(grep -rhoE --include='*.rs' 'fn (type_only_disposition_of|aggregate_module_edge_type_only)[^A-Za-z0-9_]' crates | sort | uniq -c | awk '{print $1}' | paste -sd' ' -)" = '1 1' && git -C .. diff --quiet HEAD -- rust/crates/storage/src/migrations rust/crates/storage/Cargo.toml
  ```

  The clause checks three things: the parse is called where it lives, the aggregate is called where it lives, and each function is defined exactly once across every crate's `*.rs` sources. That one definition is the indexer's `type_only.rs`, which TESB-C02 pins. Limit: a copy of either body under another name is not caught by the grep and is left to reviewer inspection.
- **Probe (EXECUTED 2026-09-30).** The admission-1 patch was applied with `git apply` to a scratch worktree `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-3-probe` (detached at HEAD f1a09ec3; `git status` showed 85 paths). The probe ran the command's source-guard tail only; the cargo part is out of scope for a document item.

  | Probe | Expected | Actual |
  |---|---|---|
  | corrected guard on the admission-1 candidate | exit 0 | exit 0; definition counts `1 fn aggregate_module_edge_type_only(`, `1 fn type_only_disposition_of(` |
  | the original clause `grep -q 'type_only_disposition_of' crates/storage/src/directory_module_edges.rs` | exit 1 (reproduces the historical FAIL) | exit 1 |
  | a second `fn type_only_disposition_of(…)` appended to storage `import_partition_reads.rs` | exit 1 | exit 1 |
  | a second `pub(crate) fn aggregate_module_edge_type_only<I>(…)` appended to storage `directory_module_edges.rs` | exit 1 | exit 1 |
  | the aggregate call in `directory_module_edges.rs` replaced by a local function | exit 1 | exit 1 |
  | files restored | exit 0 | exit 0 |

  Each mutation was restored from a copy before the next one. The worktree and the copies were removed after the probe.
- **Rerun.** The corrected check must be rerun on the next candidate (the builder's duty under INPUT-2). This entry does not convert the admission-1 FAIL into a pass.

## A-2 — 2026-09-30 — allocation amendment: unknown is never zero at any reader of partition evidence

- **Authority.** Implementation review-0, blocking finding 2, and the operator's PREP-3 packet: "Add ONE table … every place in the allocation that reads partition evidence … Bind every row to a named test at the reader and at the rendered surface (human and JSON). A partial payload and an unmatched member must each be tested." Obligation: RG-REQ-002-L04 (already preserved by the allocation; no new ID).
- **Finding (review-0).** Four sites where missing partition evidence becomes a measured zero, at admission-1 candidate lines:
  - rgr `presentation/import_partition.rs:83-95`;
  - daemon `import_partition_view.rs:124-145`;
  - classification `module_edges.rs:145-155`;
  - module-queries `facts.rs:179-189`.
- **Class and rule.** Class: a reader that turns absent, partial, unmatched or malformed partition evidence into a measured zero. The rule (D-TESB-17) has four parts:
  - In-process APIs return `Option` or a named error.
  - Producers emit every partition key on every answer, zeros included.
  - rgr decodes every key as `Option`. No `import_view` → the unavailable line. `import_view` with any required key missing or malformed → one unreadable line. Zero → nothing.
  - Fixtures that model a current daemon carry the complete payload.
- **Sweep (EXECUTED on the scratch worktree above).** The sweep found seven more instances of the class besides the review's four:
  - `ModuleGraphFacts::importer_rows` skips missing evidence;
  - `DirectoryModuleGraph::member_partitions` ignores a member outside the graph;
  - trust's `excluded_connectivity` carries `serde(default)` with skip-at-zero;
  - agent cycle items omit `partitions` when unmatched;
  - `partition_import_rows` keeps an out-of-vocabulary resolution as certain;
  - `excluded_cycles`/the importer block absent beside a present `import_view` render as "none";
  - gate, `violations` and `map` omit their keys at zero.

  Every other `serde(default)` the candidate adds decorates an `Option` field. Scope and method are stated in D-TESB-17.
- **Allocation change.** D-TESB-17's table (rows U1–U11) enters §2.1. No new path: every reader and test file is already allocated. New bound tests and floors: TESB-C01 +1 (314), C03 +1 (803), C04 +2/+1/+1 (95/46/127), C05 +2 (516), C06 +6 (with A-1's four: 863), C07 +12 (1394). RG-REQ-002-L04 joins TESB-C04's and C05's obligation lists. The acceptance boundary, P-TESB-02, §3 and §4 name the rule.

- **Addendum 2026-09-30 (INPUT-2 cycle 2) — finding TESB-MALFORMED-EVIDENCE-ORACLE.** The text above is kept unchanged as written at cycle 1.
  - **Authority.** PREP-3 review-0, finding TESB-MALFORMED-EVIDENCE-ORACLE (RG-REQ-002-L04, refinement-required); the operator's cycle-2 packet: "add a wrong-typed or non-numeric payload test for each serialized reader class … Each asserts the human AND JSON unavailable/unreadable outcome. Keep the partial-payload and unmatched-member tests."
  - **Class and rule.** Class: an oracle that injects only missing keys, so a decoder that turns a present malformed value into zero passes it. Rule: D-TESB-17's malformed-value table, rows W1–W9. Each serialized reader class gets a present wrong-typed value and a present non-numeric count, and each test asserts the unreadable line or a named error, never a zero, "nothing excluded" or silence. The classes are the remainder, the excluded-cycle list, the importer block, the not-judged counts, excluded connectivity, the per-file inferred count, per-item `partitions`, a row's resolution, and the stored importer flag.
  - **JSON outcome.** rgr's `--json` re-emits the daemon value verbatim on every surface concerned (inspection: `commands/modules/list.rs:110`, `modules/deps.rs:95`, `modules/violations.rs:173`, `orient.rs:228/378/566`, `trust.rs:92`, `gate.rs:133`, `graph.rs:876/1084`), so no rgr default can reach a JSON consumer. That pass-through is an inspection claim, because a test of it would be tautological. The producers' typed serialization and their malformed-input refusals are tested in TESB-C04/C05/C06. `map`'s JSON carries its rendered text, so its test asserts both outcomes at once.
  - **Allocation change.** No new path. New bound tests: TESB-C03 +1 (804), C04 +1 (trust 128), C06 +3 (within 871), C07 +15 (within 1410; the sixteenth cycle-2 rgr test is A-1's human-line test).

- **Addendum 2026-09-30 (INPUT-2 cycle 3) — PREP-3 review-1, finding F-RG-REQ-002-L04.** The two A-2 texts above are kept unchanged as written. Where they conflict with this addendum, this addendum governs.
  - **Finding.** The cycle-2 addendum's "JSON outcome" named rgr's verbatim `--json` pass-through, with producer tests as the guarantee. The review showed the pass-through re-emits an injected wrong-typed value (e.g. `rgr/src/commands/modules/list.rs:110-111`) without identifying it as unreadable, and that a correctly typed producer test cannot detect that path. The cycle-2 JSON claim is WITHDRAWN.
  - **Rule.** D-TESB-17's JSON consumer boundary. `presentation/import_partition.rs::mark_partition_evidence` runs on the `--json` print path of the ten partition surfaces, using the human renderers' decoders:
    - `modules list`, `modules deps`, `cycles`, `imports`, `orient`, `explain`, `trust`, `gate`, `violations`, `modules violations`;
    - not `check`, which carries no partition key, and not `map`, whose malformed payload fails closed.

    A malformed or partial key is replaced by `null`, with an `unreadable` entry (RFC 6901 pointer, reason, received value) in the additive root key `partition_evidence_status`. A payload without partition evidence reads `unavailable`. A well-formed document is unchanged apart from `{"state": "stated", "entries": []}`.
  - **Allocation change.** Paths 90 → 94: `rust/crates/rgr/src/commands/orient.rs`, `commands/trust.rs`, `commands/gate.rs`, `commands/modules/violations.rs`, each verified present at HEAD and each gaining only the call. TESB-C07 +10 tests (floor 1420):
    - per class: `json_boundary_marks_a_wrong_typed_or_non_numeric_remainder_unreadable`, `json_boundary_marks_a_wrong_typed_excluded_cycle_list_unreadable`, `json_boundary_marks_a_wrong_typed_or_non_numeric_importer_block_unreadable`, `json_boundary_marks_a_wrong_typed_or_non_numeric_not_judged_value_unreadable`, `json_boundary_marks_a_wrong_typed_or_non_numeric_excluded_connectivity_unreadable`, `json_boundary_marks_a_wrong_typed_or_non_numeric_item_partitions_unreadable`;
    - partial and older payloads: `json_boundary_marks_a_partial_payload_unreadable`, `json_boundary_states_unavailable_for_a_payload_without_partition_evidence`;
    - well formed: `json_boundary_leaves_well_formed_evidence_unchanged_and_states_it`;
    - wiring guard: `every_partition_surface_json_path_marks_partition_evidence`.

    TESB-C15 strips `partition_evidence_status` and `partitions_unavailable` as additive keys and asserts `stated` on its six JSON captures.

## Operator confirmation (2026-09-30, in-place-manager) — A-1 and OC-1, with their revisions

A-1 (including TESB-PARTITION-FALLBACK-REASON, the reviewer's recommended option) and OC-1 are approved; any "Approver: PENDING" line above is resolved by this entry. They were accepted by PREP-3 review-2 (codex gpt-6-sol) and are carried as INPUT-2. The admission-1 TESB-C03 failure stays on record. The human may override the fallback-reason ruling.

---

# INPUT-3 entries (2026-09-30)

Baseline these entries enter: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-3.json`. Admitted baseline before them: `TEST-EDGE-SCOPE-1B-INPUT-2.json` (committed at c81cdea4). Trigger: the implementation review-0 of the admission-2 candidate (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/review-0.json`, verdict `decision-required`; candidate saved at `.agent-manager/slices/TEST-EDGE-SCOPE-1B/candidate-admission-2.patch`, 94 paths). The operator's four rulings are given by the PREP-4 packet (in-place-manager; the human may override). Author: document item TEST-EDGE-SCOPE-1B-PREP-4 (builder role, claude-opus-5-5). Independent review of these entries: pending (the PREP-4 reviewer). Every entry above is unchanged.

## A-3 — 2026-09-30 — allocation amendment: a failed partition-evidence read names its own reason

- **Authority.** Review-0 DECISION_REQUIRED `TESB-PARTITION-READ-FAILURE`, recommended option A, "add a precise fallback reason". The operator ruled: A. The packet's instructions: a distinct additive value in `FallbackReason` and its coherence mirror, beside D-TESB-16's partition-view reason; the SQLite fallback stays; the read error's reason is carried, not discarded; every exhaustive match and literal construction of both enums allocated; tests force a storage-open failure and a graph-read failure and assert the served backend, the exact reason and the human and JSON provenance.
- **Finding (review-0, blocking 1).** "`default_view_partition_fallback` turns a failed storage open or `directory_module_graph` read into `FallbackReason::LiveGraphError`. The enum's documented contract in `livegraph_feed.rs` says that reason means *the LiveGraph engine errored*. … `.ok()` discards the actual read failure." Verified on the admission-2 candidate at `orient_lg_decisions.rs:398-417`.
- **Class and rule.** Class: a fallback reason that names a mechanism other than the one that failed, with the failure's text discarded. Rule: D-TESB-16's INPUT-3 addendum.
  - The read becomes `read_default_view_excludes_nothing -> Result<bool, String>`, whose `Err` names the failed read and keeps the underlying error.
  - `Err` maps to `Fallback { PartitionEvidenceUnreadable }` plus one daemon-log warning.
  - The final name is `PartitionEvidenceUnreadable`, with no `LiveGraph` prefix, because the unreadable thing is the SQLite store's partition evidence.
  - The error text is not added to the wire: a free-text `Provenance` key is a boundary shape the ruling did not authorize (§8 FALLBACK-READ-FAILURE-1).
- **Sites.** The table in D-TESB-16's INPUT-3 addendum lists three exhaustive matches, one `_` match, the defect, two HEAD instances of the class on other surfaces (surfaced in §8, not fixed), and two propagations that are not instances. The grep behind it covered every `FallbackReason::` and `CoherenceFallbackReason::` use in the admission-2 candidate (a scratch worktree), and every `LiveGraphBoundedServeDeclined =>` arm, which appears only in exhaustive matches.
- **Allocation change.** No new path. Checks:
  - TESB-C06 gains `read_default_view_excludes_nothing_carries_the_read_error`, `orient_cycles_outcome_names_partition_evidence_unreadable_when_the_store_cannot_be_opened`, `orient_cycles_outcome_names_partition_evidence_unreadable_when_the_directory_graph_read_fails`, `build_orient_envelope_cycles_leaf_serializes_partition_evidence_unreadable_never_livegraph_error` and `build_explain_envelope_cycles_leaf_serializes_partition_evidence_unreadable_never_livegraph_error`. Floor 871 → 877: the admission-2 builder's recorded `--list` of 872, plus 5.
  - TESB-C07 gains `orient_full_serving_line_names_the_partition_evidence_unreadable_fallback`.
  - TESB-C09 gains `partition_evidence_unreadable_reason_as_str_and_serde_name_match_and_differ_from_livegraph_error`. Floor 25 → 26.
  - The acceptance boundary, P-TESB-02, §2.3 R17b/R17c, §3 and §4 name the rule.

## A-4 — 2026-09-30 — allocation amendment: `partition_evidence_status` is derived from positive validation

- **Authority.** Review-0 blocking finding 2. The operator ruled: "Invert it. The status is `stated` ONLY when every required carrier and item was positively validated. It is `unavailable` only for a genuinely older payload (the documented absent-key shape). Anything else is `unreadable`, with a reason and a JSON pointer." The packet also asked for one named test per sibling guard, plus any others found, and for D-TESB-17's table to show the derivation rule.
- **Finding.** "`rgr/src/presentation/import_partition.rs:629–955` defaults to `state: "stated"` when traversal records neither `stated_any` nor `unavailable_any`." Verified on the admission-2 candidate. The four sibling guards the review lists were confirmed. The sweep found four more:
  - a non-object `value` read as the older shape;
  - a skipped daemon-stated `unavailable` module-edges block;
  - carrier containers never looked for;
  - a mixed document marked `unreadable` with no entry.
- **Class and rule.** Class: a status that defaults to its positive value when the traversal records nothing. This is its third appearance, after admission-1 review F-2 and PREP-3 review F-2. Rule: D-TESB-17's INPUT-3 status derivation.
  - Every carrier records exactly one outcome, and containers are validated first.
  - The derivation has eight rows, including the fail-closed row for a carrier without an outcome.
  - A per-surface table gives each surface's containers, carriers and older-daemon shape.
  - A non-object answer is a named error: the function returns `Result`, and a discarded result fails clippy and the wiring guard.
- **Allocation change.** No new path. TESB-C07 gains ten tests (the list is in D-TESB-17 INPUT-3 and in C07's command). Floor 1420 → 1431: the admission-2 builder's recorded `--list` of 1420, plus A-3's one test, plus these ten.

## OC-2 — 2026-09-30 — oracle correction: TESB-C11's usage probe

- **Authority.** Review-0 finding 4: "`/tmp/tesb-c11-a-usage.txt.err` says `--engine livegraph requires --kind file-import or module-import`. It does **not** demonstrate refusal of `--include-tests` with an explicit LiveGraph engine … This is an evidence correction, not permission to rewrite the historical C11 record." The operator ruled: add `--kind module-import`, assert the nonzero exit and the partition-specific diagnostic, verified in the source, and keep the historical result on record.
- **Historical result, kept.** The INPUT-2 TESB-C11 command held `q $C "$CAND" "$LD" $T-a-usage.txt cycles --include-tests --engine livegraph` and asserted only `rc(T + "a-usage.txt") == 1`. On the admission-2 candidate the builder recorded TESB-C11 EXECUTED exit 0 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/build-progress.md`). Its own note reads: "the usage probe `cycles --include-tests --engine livegraph` exits 1 on the pre-existing `--engine livegraph requires --kind` rule, which fires first." The capture `/tmp/tesb-c11-a-usage.txt.err` (71 bytes, still present 2026-09-30) holds that message. **That C11 pass stands as recorded. Its usage clause passed for the wrong reason and is not evidence of the partition refusal.**
- **Diagnostic, verified in the source.** Admission-2 `rgr/src/commands/graph.rs:265-277`, `refuse_partition_flags_with_explicit_engine`: `error: --include-tests / --include-inferred are not supported with --engine {engine} (it serves the unpartitioned import graph); omit --engine to use them`. It is reached only after the engine/kind validation (`parse_cycles_args`, :1149-1186). `run_cycles` prints it to stderr with `EXIT_USAGE_ERROR` (1).
- **Corrected clause (text only).** The invocation gains `--kind module-import`. The oracle gains four assertions: the partition-specific line is on stderr; no line mentions `requires --kind`; stdout is empty; the exit code is 1, as before.
- **Probe (EXECUTED 2026-09-30).** Both invocations ran with the admission-2 release build (`rust/target/release/rmap`, `rmap 0.19.0`; its binaries contain the admission-2 strings, so the identity is INFERRED), fully isolated (`RMAP_STATE_ROOT`/`RMAP_SOCKET_PATH` = `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-4-c11`, stdio, auto passes off), from the leveldb checkout. The corrected assertion lines, extracted from the new command text, were run on each output.

  | Invocation | exit | stderr | corrected oracle |
  |---|---|---|---|
  | `cycles --include-tests --engine livegraph` (INPUT-2) | 1 | `error: --engine livegraph requires --kind file-import or module-import` | exit 1 (AssertionError on the partition line) |
  | `cycles --include-tests --engine livegraph --kind module-import` (INPUT-3) | 1 | `error: --include-tests / --include-inferred are not supported with --engine livegraph (it serves the unpartitioned import graph); omit --engine to use them` | exit 0 |

  The parse fails before any daemon or state access, and the state root held only the probe's captures.
- **Rerun.** The corrected C11 must be rerun on the next candidate. This entry does not change the admission-2 record.

## A-5 — 2026-09-30 — allocation amendment: the `path` field witness, TESB-C16; one contradiction surfaced

- **Authority.** Review-0 finding 3: "The slice's definition of done requires a real `rmap path` fixture whose sole route uses an inferred import … The builder explicitly reports only a unit test for this case. RG-REQ-002-L11 remains **UNVERIFIED at that acceptance boundary**." The operator ruled: make the check explicit and named — an isolated `rmap path` over a real fixture whose sole route uses an inferred import, with no route by default and the route under `--include-inferred`, quoting the fixture's actual import source line, with full isolation and builder-owned roots only.
- **Contradiction, verified.** No `rmap path` route can contain an IMPORTS hop, at HEAD or in the admission-2 candidate:
  - `path` resolves both endpoints through `StorageConnection::resolve_symbol`, whose exact-stable-key tier and name tiers select `kind = 'SYMBOL'` only (storage `queries.rs:3301-3308`).
  - Its walk follows edges whose source is on the frontier (`queries.rs:1949-1951`).
  - Every stored IMPORTS edge is FILE→FILE or MODULE→MODULE. EXECUTED 2026-09-30 by sqlite `?immutable=1` on the manager's before-roots: leveldb `FILE→FILE static 498`, `MODULE→MODULE static 21`; kafka `FILE→FILE inferred 3 / static 42479`, `MODULE→MODULE static 5602`; FRAKTAG and grpc-java FILE→FILE and MODULE→MODULE static only.
  - A FILE stable key given to `path` is refused (probe below).

  The operator's literal witness therefore cannot be produced, which contradicts a ruling and §6's text. It is surfaced as DECISION_REQUIRED `TESB-PATH-WITNESS` (§9 of the slice document), recommended option A.
- **As authored (option A, pending the operator's confirmation).** TESB-C16 is an isolated `rmap path` over a builder-made Python fixture whose only route is the INFERRED call `app.py:2` `    return worker.process_batch()`: an untyped receiver and a unique method name, stored `inferred` by PYTHON-RECEIVER-BINDING-1. It asserts:
  - the store row;
  - the default no-route line with its runnable command;
  - the `--include-inferred` route, in human and JSON form;
  - the printed command, run as printed, answering byte-identically;
  - the stated limit: FILE keys exit 2 with `symbol not found`.

  The inferred-import hop stays proven at the daemon function (TESB-C06 `path_include_inferred_walks_inferred_hops_and_counts_them`). §6 says "an inferred edge" and cites this entry. TESB-C14 also checks `/private/tmp/TEST-EDGE-SCOPE-1B-path`. §5 runs C16 after C15.
- **Probes (EXECUTED 2026-09-30, admission-2 release build, PREP-4-owned roots, each removed).**

  | Probe | Expected | Actual |
  |---|---|---|
  | TESB-C16 as written, with only its root and capture names renamed to `TEST-EDGE-SCOPE-1B-PREP-4-path` / `/tmp/tesb4-c16` | exit 0 | exit 0, `TESB-C16 ok`; the EXIT trap removed the root |
  | the same, `rmap` wrapped to strip `--include-inferred` | exit ≠ 0 | exit 1: `AssertionError: ('missing line', '1 hop', …)` |
  | the same, `rmap` wrapped to add `--include-inferred` to every `path` | exit ≠ 0 | exit 1: the default shows the route, so no command is printed and `test "$CMD" = …` fails |
  | `rmap path app.py lib/worker.py` / FILE stable keys on the fixture | refused | exit 2, `error: InvalidRequest: symbol not found: …` |

## Guard probes for INPUT-3 (EXECUTED 2026-09-30)

- `bash -n` on the sixteen check commands, each written to a file: exit 0 for all sixteen.
- The named-test loops of TESB-C06, C07 and C09 were run on copies of the admission-2 builder's cargo logs (`/tmp/tesb-c06.txt`, `/tmp/tesb-c07.txt`, `/tmp/tesb-c09b.txt`, read-only):
  - each exits 1 on its first INPUT-3 name (`MISSING read_default_view_excludes_nothing_carries_the_read_error`, `MISSING orient_full_serving_line_names_the_partition_evidence_unreadable_fallback`, `MISSING partition_evidence_unreadable_reason_as_str_and_serde_name_match_and_differ_from_livegraph_error`);
  - the same loops without the 17 INPUT-3 names exit 0;
  - the floors (877, 1431, 26) exceed the admission-2 builder's recorded `--list` counts (872, 1420, 25). That comparison is INFERRED from the recorded counts; no cargo was run.
- The TESB-C14 leftover loop, renamed to the PREP-4 prefix: exit 0 with no root present; exit 1 `LEFT path` with a `…-path` directory present (removed after).
- Every probe ran in a scratch worktree `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-4-probe` (detached at c81cdea4 with the admission-2 patch applied, 94 paths), in PREP-4-named roots, or in `/tmp/tesb4-*` captures. All were removed at the end of the item.

## A-5 addendum — 2026-09-30 (INPUT-3 cycle 2) — ruling TESB-PATH-WITNESS → A

- **Authority.** PREP-4 review-0 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B-PREP-4/review-0.json`, decision-required) raised DECISION_REQUIRED `TESB-PATH-WITNESS` with the recommendation A. The operator (in-place-manager) took A in the cycle-2 packet; the human may override. The packet: "`rmap path` resolves SYMBOL endpoints only (`storage/src/queries.rs:3280–3308`). The definition of done's 'sole route uses an inferred import' therefore cannot be shown at the CLI on the current product; that wording was the manager's error."
- **Ruling as recorded.**
  - §6 and TESB-C16: the CLI witness is an isolated `rmap path` whose sole route uses an inferred CALL (`app.py:2` `    return worker.process_batch()`). By default it gives no route; with `--include-inferred`, the route.
  - The inferred-IMPORT walk stays proven by TESB-C06's unit test `path_include_inferred_walks_inferred_hops_and_counts_them`, now named in §6.
  - FILE endpoints for `path` are out of scope, recorded as the follow-up §8 PATH-FILE-ENDPOINTS.
- **Superseded.** Only the "pending the operator's confirmation" status of A-5 above; its text is kept as written. TESB-C16's command is unchanged, and its cycle-1 probes stand.
- **Also in cycle 2 (review-0's fixable finding).** A blank line at the end of the slice document made `git diff --check` exit 2; it is removed. This is a document hygiene fix, not an oracle change.

## Operator confirmation (2026-09-30, in-place-manager) — the INPUT-3 corrections

The INPUT-3 corrections authored by PREP-4 are approved; any "Approver: PENDING" line on them is resolved by this entry. They are:
- TESB-PARTITION-READ-FAILURE → A (a precise fallback reason);
- the positive-validation derivation of partition-evidence status;
- TESB-C11's oracle correction (the admission-2 result retained);
- TESB-PATH-WITNESS → A (a CLI inferred-call witness; the inferred-import walk proven by the C06 unit test; `path` file endpoints recorded as a follow-up).

Accepted by PREP-4 review-1 (codex gpt-6-sol). Carried as INPUT-3. The human may override either operator ruling.

# INPUT-4 entries (2026-10-02)

Baseline these entries enter: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-4.json`. Admitted baseline before them: `TEST-EDGE-SCOPE-1B-INPUT-3.json` (committed at cabf3b86). Trigger: the operator's standing gate suite on the admission-3 candidate (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/candidate-admission-3.patch`, 94 paths; all sixteen checks passed; ruled shippable by the HUMAN with three follow-ups, D-TESB-SHIP-1 draft) failed 4 of 4 runs, at low load, in `rust/crates/daemon-runtime/tests/concurrency_dispatch.rs`. Author: document item TEST-EDGE-SCOPE-1B-PREP-5 (builder role, claude-opus-5-5). Independent review of this entry: pending (the PREP-5 reviewer). Every entry above is unchanged.

## A-6 — 2026-10-02 — allocation amendment: the cancellation fixture stores the file-level shape, and the whole daemon-runtime suite is bound

- **Authority.** The HUMAN, 2026-10-02: "Fix the fixture first". The PREP-5 packet's task: add the fixture file to the candidate paths with the ring in the shape 1B reads; close the class; bind the whole daemon-runtime suite fail-closed with the three test names. The packet names this amendment "A-3". That ID is INPUT-3's entry above, and an ID is never reused, so this entry is A-6.
- **Finding (operator gate, recorded by the manager).**
  - `dispatched_cycles_cancels_mid_tarjan_when_peer_disconnects` (:636): "cycles: ran to completion instead of cancelling mid-flight".
  - `dispatched_default_cycles_cancels_via_sqlite_fallback` (:797) and `dispatched_default_cycles_cancels_during_cert_build` (:925): `left: Some(0)`, `right: Some(1)`.

  No allocation check bound the file. TESB-C06 ran `--lib` and asserted `tests/` unchanged.
- **Cause, verified (OBSERVED at HEAD cabf3b86 and in the admission-3 patch).**
  - `inject_module_ring` (:488–517) stores the ring only as MODULE nodes `cm{i}` and MODULE→MODULE `IMPORTS` rows.
  - In the candidate, `cycles --engine sqlite` (dispatch.rs), the default route's `serve_cycles_sqlite`, and orient's and explain's cycle reads (storage `agent_impl.rs`) all derive the directory graph through `StorageConnection::directory_module_graph`. That function reads `file_imports_with_partition` (FILE-bearing endpoints, the importer's `files.is_test`) × `get_file_ownership_from_owns_edges` (MODULE→FILE `OWNS`).
  - The fixture has neither FILE nodes nor OWNS, so every view of its ring is empty.
  - The cycles certificate build (`livegraph_feed.rs::module_cycle_compare_data_cancellable` → `find_cycles_cancellable`) is unchanged by the candidate and still reads the persisted MODULE graph (P-TESB-02).
  - `find_sccs_cancellable` consults its checkpoint once per 256 DFS steps (graph-algorithms `scc.rs`), so an empty or tiny graph emits none.
- **Class and rule.**
  - Class 1: a fixture that stands for an index but stores a shape the reader under test does not read, so the proof runs over an empty view.
  - Class 2, its enabler: a touched crate's integration suite that no check binds.
  - Rule 1: a fixture stores what an index stores for the reader under test.
  - Rule 2: the whole suite of every touched daemon crate is bound.
  - For class 1 the shape is a `files` row, a FILE node, an `OWNS` edge (`resolution: "static"`, as `indexer/src/orchestrator.rs:1407-1421` writes it) and a production file→file `static` IMPORTS edge per ring step. The persisted MODULE ring stays, because the certificate build reads it.
  - For class 2: daemon-runtime's integration targets were the gap. rgr's integration targets other than the trust seam are also unbound, but none injects IMPORTS rows. They stay the operator gate's (CLAUDE.md "Gates in a relay run").
- **Class sweep (2026-10-02).** Integration `tests/` directories of every crate were searched for `"IMPORTS"`, `EdgeType::Imports`, `insert_edges` and `INSERT INTO edges`, plus the storage parity fixture corpus.

  | Hit | Disposition |
  |---|---|
  | `daemon-runtime/tests/concurrency_dispatch.rs::inject_module_ring` (6 callers) | INSTANCE — fixed by this amendment. Callers: the three gate failures, plus `dispatched_orient_cancels_mid_cycle_tarjan_when_peer_disconnects`, `dispatched_explain_cancels_mid_cycle_tarjan_when_peer_disconnects` and `live_peer_orient_and_explain_complete_on_large_fixture`. Those three were not among the gate's failures, but over the admission-3 fixture their named cycle Tarjan had no ring to traverse. Where they cancelled is not established (INFERRED; §8 CANCEL-LOOP-IDENTITY). All six are bound by name in TESB-C17. |
  | same file, `inject_stats_fixture` | not an instance: `stats` keeps the persisted MODULE fans (P-TESB-02/03, D-TESB-12), the shape it stores; its cancellation is the SQL interrupt. Unchanged. |
  | same file, `module_ring_livegraph` | not an instance: LiveGraph IR, not stored rows. `dispatched_default_cycles_cancels_via_livegraph_module_tarjan` is bound by name in TESB-C17, because it runs through 1B's changed `cycles_auto_response`. |
  | `storage/tests/agent_impl.rs`, `call_aggregate_families.rs`, `gate_impl.rs` | not instances: every endpoint of an injected IMPORTS row is a FILE or SYMBOL node carrying a `file_uid`, the shape `file_imports_with_partition` reads. Bound by TESB-C03 (the whole storage suite), which passed at admission 3. |
  | `storage/tests/parity.rs`, `resolved_call_aggregate.rs`, `storage-parity-fixtures/` | no IMPORTS rows. |
  | `agent/tests/*`, `rgr/tests/*`, `repo-index/tests/*`, `repo-graph-scip-ingest/tests/harness.rs` | assertions on output, comments, or a real fixture indexed through the indexer (the stored shape is the indexer's). |
- **Allocation change.**
  - Paths 94 → 95: `rust/crates/daemon-runtime/tests/concurrency_dispatch.rs`, fixture only. The fixture rule, the uid constraints and the kept MODULE ring are stated in TESB-C17's inputs.
  - New check TESB-C17: `cargo test -p repo-graph-daemon-runtime` (unit suite, every integration target, doc-tests), fail-closed in TESB-C06's form, with seven names: the three failures, the LiveGraph sibling, and the three other fixture users. Further guards:
    - `concurrency_dispatch` lists exactly 18 tests;
    - the integration targets list at least 168;
    - `tests/` is otherwise unchanged, with no untracked file;
    - the fixture file differs, its diff removes no line other than a `//` comment, and it adds an `"OWNS"` edge and an `upsert_files` call.
  - TESB-C06's `tests/`-unchanged guard excludes that one file.
  - §0's P-TESB-02/05 proofs, §3's RG-REQ-011 row, §5 (C17 after C06; the regression baseline recorded on the candidate before the fixture changes), §6 (seventeen checks) and §8 (CANCEL-LOOP-IDENTITY) follow.
  - Obligation IDs unchanged.
- **Counts (EXECUTED 2026-10-02 at HEAD cabf3b86, `cargo test -p repo-graph-daemon-runtime -- --list`; the candidate adds no integration test).** `concurrency_dispatch` 18. The 24 integration targets: 168. Unit suite: 829 (TESB-C06's floor 877 applies to the candidate). Doc-tests: 2. `--test '*' -- --list` gives 168; `--test concurrency_dispatch -- --list` gives 18.
- **Probes (EXECUTED 2026-10-02).** Every scratch path was PREP-5-owned and removed: the worktree `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-5-probe` (detached at cabf3b86) and the captures in `/tmp/tesb5`.

  | Probe | Expected | Actual |
  |---|---|---|
  | `bash -n` on the seventeen check commands | 0 | 0 for all seventeen |
  | C17 name loop and negative greps, synthetic log with the seven names `ok` | 0 | 0 |
  | the same, `dispatched_default_cycles_cancels_during_cert_build` missing | 1 | 1, `MISSING dispatched_default_cycles_cancels_during_cert_build` |
  | the same, `…via_sqlite_fallback ... FAILED` | 1 | 1 |
  | the same, a `thread '…' panicked at` line | 1 | 1 |
  | C17 diff guards, fixture file unchanged | 1 | 1 |
  | an additive fixture (`upsert_files`, `"OWNS"`) plus one rewritten `//` comment | 0 | 0 |
  | the same plus a removed assertion-message line | 1 | 1 |
  | additive with `upsert_files` but no `"OWNS"` | 1 | 1 |
  | an untracked file in `tests/` | 1 | 1 |
  | another `tests/` file changed | 1 | 1 |
  | TESB-C06's narrowed guard, only the fixture file changed | 0 | 0 |
  | TESB-C06's narrowed guard, another `tests/` file changed | 1 | 1 |

- **Limits.** The fixture itself and the whole-suite run are the implementation's, and are not run here (no cargo beyond `-- --list`). The fixed tests can pass only if the default partitioned view contains the ring (the connected count is 1, served by SQLite) and the Tarjan runs long enough to checkpoint. That is a prediction from the source, INFERRED. The `panicked at` negative grep now also scans the integration targets' output. If a passing integration test printed such a line, C17 would fail closed, never pass falsely; no such line was observed, because none was run.

## A-6 addendum — 2026-10-02 (INPUT-4 cycle 2) — what TESB-C17 proves about cancellation

- **Authority.** PREP-5 review-0 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B-PREP-5/review-0.json`, refinement-required), finding C17, verifiability, under RG-REQ-004-L07: "`TESB-C17.expected`, the allocation's `acceptanceBoundary`, and §3 call the orient and explain results proof of cancellation inside their cycle Tarjan loops. But `assert_cancelled_in_flight` checks only `Cancelled` and a message containing 'during.' … The new fixture makes the ring available; it does not identify which loop caused a cancellation." Required action: keep the names and assertions; state that C17 verifies the full suite, the connected default cycle result and cancellation responses over a fixture visible to the partitioned reader; mark cancellation in the named loop unverified; keep CANCEL-LOOP-IDENTITY; propose a later assertion or rename.
- **Verified.** `assert_cancelled_in_flight` (concurrency_dispatch.rs:447-466 at HEAD) asserts the code and the substring "during". `cancel::loop_checkpoint` (daemon-runtime `cancel.rs:192-200`) returns `Break` on a failed heartbeat emit. The phase label travels in the heartbeat and not in the error, so no current assertion can name the loop.
- **Class and rule.** Class: an evidence claim taken from a test's name rather than from its assertions. The same wording covered the `cycles` routes and the certificate build too, not only orient and explain. Rule: a check's `expected`, and every proof cell that cites the check, state only what the bound assertions establish. Anything the names add is marked UNVERIFIED and has a follow-up.
- **Superseded wording (kept above as written).** A-6's text needs no change: it already calls the orient and explain cancellation sites "not established (INFERRED)". The slice document's wording is corrected; its §9 entry for INPUT-4 cycle 2 lists the sites. The superseded `expected` of TESB-C17 read: "… `cycles --engine sqlite`, the default `cycles` route's SQLite fallback and its certificate build each cancel DURING their Tarjan when the peer disconnects, … orient's and explain's cycle Tarjans cancel mid-flight and run to an identical, complete answer for a live peer …".
- **Follow-up.** §8 CANCEL-LOOP-IDENTITY offers three options: A, assert the failing heartbeat's phase; B, rename the tests to what they assert; C, leave them. A is recommended. The names and assertions are unchanged in INPUT-4, by the human's ruling "Fix the fixture first".
- **No allocation change beyond wording.** Paths stay at 95 and checks at 17. Commands, obligation IDs and counts are unchanged.

## A-6 addendum 2 — 2026-10-02 (INPUT-4 cycle 3) — C17 states only what its assertions show

- **Authority.** PREP-5 review-1 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B-PREP-5/review-1.json`, refinement-required, finding TESB-C17, verifiability). The review says the acceptance boundary claims a disconnected peer "receives" `Cancelled`, while `FailAfter` only simulates a failed emitter write and the assertion checks the dispatch result. It also says `expected` and the P-TESB-02 cell claim a "divergent certificate", while the test asserts only `count == 1` and `backend_used == "sqlite"`. Required action, wording only: dispatch returns `Cancelled` after a simulated disconnect; the connected call serves one cycle through SQLite; certificate behaviour stays with the bound certificate checks.
- **Verified.** `FailAfter::emit` (concurrency_dispatch.rs:431-439) returns `EmitError("simulated peer disconnect")` after `ok_for` emits. `assert_is_coherence_envelope` (:1613-1620) checks that four keys are present. The connected assertions of the seven tests are listed in TESB-C17's inputs ("THE ASSERTIONS"). No C17 test reads the cycles certificate.
- **Class and rule.** The class is the same as in the first addendum, found again one level down: a claim taken from a test's name or doc comment ("in-flight", "delivery", "divergent cert") rather than from its assertions. The closing rule is now mechanical: C17's inputs carry the assertion list, and every C17 claim — its `expected`, the acceptance boundary, the §0 and §3 proof cells, the Status line and the §9 history — is written only from that list.
- **Superseded wording (quoted).** TESB-C17 `expected`, cycle 2: "… for a connected peer, the default `cycles` answers the 1000-module ring as one cycle served by SQLite (`count` 1, `backend_used` "sqlite": the default partitioned view contains it), with the certificate built over the persisted MODULE ring and divergent, and `orient`/`explain` return the same complete envelope twice …". P-TESB-02 cell, cycle 2: "C17 (connected, the certificate built over the persisted MODULE ring diverges and SQLite serves the ring as one cycle; INPUT-4 A-6)". Acceptance boundary, cycle 2: "a disconnected peer receives `Cancelled` with "during", never a completed answer".
- **Allocation change.** `P-TESB-02` is removed from TESB-C17's `obligationIds`; it stays covered by C03, C06, C11 and C13. Wording changes are as listed in the slice document's §9 entry for INPUT-4 cycle 3. Commands, paths, counts, test names and assertions are unchanged.

## Operator confirmation (2026-10-02, in-place-manager) — A-3 and its revisions

A-3 is approved under the HUMAN's ruling of 2026-10-02, "Fix the fixture first". It allocates `daemon-runtime/tests/concurrency_dispatch.rs`, rebuilds the cancellation ring in the file-level shape the partitioned `cycles` reads, and binds the whole daemon-runtime suite as TESB-C17. Any "Approver: PENDING" line on A-3 is resolved by this entry. Accepted by PREP-5 review-2 (codex gpt-6-sol). Carried as INPUT-4.

# INPUT-5 entries (2026-10-02)

Baseline these entries enter: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-5.json`. Admitted baseline before them: `TEST-EDGE-SCOPE-1B-INPUT-4.json` (committed at 7a948ff5). Trigger: implementation review-0 of the admission-4 candidate (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/review-0.json`, decision-required; candidate saved at `.agent-manager/slices/TEST-EDGE-SCOPE-1B/candidate-admission-4.patch`, 95 paths). Author: document item TEST-EDGE-SCOPE-1B-PREP-6 (builder role, claude-opus-5-5). Independent review of these entries: pending (the PREP-6 reviewer). Every entry above is unchanged.

## Record — 2026-10-02 — the manager's before-roots were lost and re-captured (no oracle change)

- **Authority.** Review-0, "Blocking evidence gap": "All five manager-owned `*-before` roots required by TESB-C11–C14 are absent. The current run therefore has C11, C12, and C13 at **execution-failed**, not pass." DECISION_REQUIRED TESB-FIELD-EVIDENCE-4, option A: "restore the exact HEAD-produced roots, then rerun C11–C14". The operator (in-place-manager) took A: lost infrastructure is restored, not waived. The human may override.
- **What the operator did (from the PREP-6 packet).** The roots captured 2026-09-29 had been purged from `/private/tmp`. All five were re-captured with the same producer: HEAD's release binary, and `git diff a85f6239 HEAD -- rust` is empty. The checkouts were at the recorded states: poco 49af4000f99f clean, leveldb 7ee830d02b62 clean, kafka 0dad6a7c9c0a clean, grpc-java f43013161b3c dirty=2997, FRAKTAG 9bea3a5e4d32 dirty=14. A durable copy is at `~/repo-graph-retained/TEST-EDGE-SCOPE-1B-before/`.
- **Verified by this item (OBSERVED, read-only).** The five directories exist. The first line of each `source.txt` names the recorded commit and dirty count and `producer=rmap 0.19.0`, for example `grpc-java f43013161b3c dirty=2997 captured=2026-10-02T14:25:01Z producer=rmap 0.19.0`. The file counts are poco 6, leveldb 6, kafka 6, grpc-java 6, FRAKTAG 5. The durable copy holds five directories (2.9 GB). This item did not verify that the re-captured stores equal the lost ones: no digest of the lost roots survives (the admission-4 `build-progress.md` has a `registry-before` line and no `roots-before` line).
- **Consequence.** C11–C13 must run on the next admission against these roots. The admission-3 field results stay historical evidence; they are not evidence for this admission. No command or expected value changes.

## OC-3 — 2026-10-02 — oracle correction: TESB-C14 is fail-closed

- **Authority.** Review-0: "TESB-C14 is a false-positive oracle. Its command separates the digest comparisons from later checks with `;`, so a failed roots comparison can still yield exit 0. `/tmp/tesb-run-TESB-C14.out` reports missing roots and then `TESB-C14 ok`. Correct the check in the allocation; do not record this run as a pass." The PREP-6 packet adds a second requirement: the `roots-before` line that the builder records must also fail closed when a root is absent.
- **Historical result, kept.** The INPUT-4 command, quoted from 7a948ff5: `… && test "$(grep -E '^roots-before [0-9a-f]{64}$' … | tail -1 | cut -d' ' -f2)" = "$(find /private/tmp/TEST-EDGE-SCOPE-1B-poco-before … -type f | sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1)" && PIDS=$(pgrep -x rmapd | paste -sd, -); { [ -z "$PIDS" ] || ! ps -o command= -p "$PIDS" | grep -v '/\.local/bin/rmapd' | grep -q .; } && for d in …`. The admission-4 capture `/tmp/tesb-run-TESB-C14.out` (OBSERVED, 442 bytes) holds five `find: …-before: No such file or directory` lines and then `TESB-C14 ok: 95 changed paths, all allocated`. The admission-4 record has no `roots-before` line, so the roots comparison compared an empty string with a digest and failed. Only the `;` turned that failure into exit 0. **The admission-4 C14 result is not a pass, and it is not evidence for P-TESB-06 or for the RG-REQ-011 IDs that C14 carries.**
- **Class and rule.** Class: an assertion in a check command whose failure does not set the command's exit status. Two forms occur: a top-level separator breaks the `&&` chain, or an expansion (`$(find …)`) hides a failed listing and produces a digest of a shorter list. Rule: TESB-C14 runs under `set -euo pipefail`. Every precondition and comparison is its own statement that ends the command on failure (`|| fail "<reason>"`, or `|| { echo …; exit 1; }`). The roots digest is one snippet, from `R=(` to the digest's `exit 1; }`, and the snippet text is identical in TESB-C14 and in the §5 step-0 recording command. The snippet requires each root to exist and to hold at least one file before it digests the roots. The rmapd check fails if `pgrep` (status other than 0 or 1), `ps` or `grep` fails. The leftover-worktree check uses `case` on the captured `git worktree list`, not `! … | grep -q`, so a pipe status cannot hide a match.
- **Kept assertions:** the registry digest is unchanged; the roots digest is unchanged; no `rmapd` other than `~/.local/bin/rmapd` runs; no builder root `/private/tmp/TEST-EDGE-SCOPE-1B-{before,leveldb-copy,leveldb-fresh,poco-copy,kafka-copy,grpc-java-copy,FRAKTAG-copy,plain,path}` remains; no `TEST-EDGE-SCOPE-1B-before` worktree remains; the Python allocation/paths check passes. The Python text is unchanged.
- **Class sweep.** The other sixteen commands were split at top level. Their top-level `;` separate only assignments, function definitions or loop syntax, and every loop body ends in `|| exit 1` or `|| { …; exit 1; }`. No other command takes a digest or a listing of roots. Result: no other instance.
- **Probes (EXECUTED 2026-10-02).** The new C14 command and the step-0 recording command ran exactly as extracted from the slice document (`bash -n` exit 0 for both, and for all seventeen check commands). Most probes ran in a scratch worktree, `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-probe`, detached at 7a948ff5, with its own `.agent-manager/slices/TEST-EDGE-SCOPE-1B/build-progress.md`. "OLD" is the INPUT-4 command from 7a948ff5. The manager roots were read only. The single rename was of the directory itself, and a trap restored it.

  | Probe | Expected | Actual |
  |---|---|---|
  | P1 step-0 recording, all five roots present | exit 0, two lines appended | exit 0, `registry-before 4762fd94…` and `roots-before 7d4b12c7…` |
  | P2 C14, all present | exit 0, `ok` | exit 0, `TESB-C14 ok: 0 changed paths, all allocated` |
  | P3a recording, `…-leveldb-before` renamed | exit ≠ 0, nothing appended | exit 1, `FAIL: before-root missing or empty: /private/tmp/TEST-EDGE-SCOPE-1B-leveldb-before`; the file is byte-identical |
  | P3b C14, the same root renamed | exit ≠ 0, no `ok` | exit 1, the same `FAIL:` line, no `ok` |
  | P3c OLD C14, the same root renamed | (the defect) | exit 0, `find: …: No such file or directory` and then `TESB-C14 ok` |
  | P3d C14, the name restored | exit 0, `ok` | exit 0, `ok` |
  | P4 C14, recorded registry digest altered | exit ≠ 0, no `ok` | exit 1, `TESB-C14 FAIL: operator registry changed` |
  | P4 OLD, the same | (the defect) | exit 0, `TESB-C14 ok` |
  | P5 C14, recorded roots digest altered | exit ≠ 0, no `ok` | exit 1, `TESB-C14 FAIL: manager before-roots changed` |
  | P6 C14, no `build-progress.md` | exit ≠ 0 | exit 1, `TESB-C14 FAIL: no .agent-manager/…/build-progress.md` |
  | P6b C14, an empty `build-progress.md` | exit ≠ 0 | exit 1, `TESB-C14 FAIL: no registry-before line in …` |
  | P12 C14, the admission-4 record shape (a registry line, no roots line), in `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-p2` | exit ≠ 0 | exit 1, `TESB-C14 FAIL: no roots-before line in …` |
  | P7 C14, `/private/tmp/TEST-EDGE-SCOPE-1B-plain` present (empty directory made and removed by this item) | exit ≠ 0 | exit 1, `TESB-C14 FAIL: LEFT plain` |
  | P8 C14, worktree `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-wt-TEST-EDGE-SCOPE-1B-before` present | exit ≠ 0 | exit 1, `TESB-C14 FAIL: worktree TEST-EDGE-SCOPE-1B-before remains` |
  | P9 C14's rmapd lines (verbatim), a foreign `rmapd` running (a symlink to `/bin/sleep` under `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-fake`) | exit ≠ 0 | exit 1, `TESB-C14 FAIL: rmapd other than the operator's ~/.local/bin/rmapd: /private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-fake/rmapd 30` |
  | P9b the same lines, only the operator's `~/.local/bin/rmapd` running | exit 0 | exit 0 |
  | P10 C14, an untracked unallocated file in the worktree | exit ≠ 0, no `ok` | exit 1, the Python `AssertionError` and then `TESB-C14 FAIL: allocation/paths check failed` |
  | P11 C14, everything restored | exit 0, `ok` | exit 0, `ok` |

  The digest of the five roots was `7d4b12c73d44a41e9aef3d3638bf130177bde9cf8c9e3401752190a57e9170b0` before and after the probes. A first P9 attempt, with a copied `sleep` binary, tested nothing, because macOS killed the copy before the check ran. It was repeated with a symlink (the P9 row). Everything this item created was removed: both worktrees, the fake directory, `…-plain`, `…-PREP-6-p2` and the `/tmp/tesb6` captures.
- **Limits.** P2 and P11 ran C14 in a worktree at HEAD with no candidate applied, so the Python check saw 0 changed paths. The candidate case (95 allocated paths) is the implementation's. P9 ran the rmapd lines on their own. The full command reaches those lines only after the roots digest has passed, and that path was proven by P2. The next builder records a fresh `registry-before`/`roots-before` pair at step 0, so no admission-4 record is reused.

## OC-3 addendum — 2026-10-02 (INPUT-5 cycle 2) — the rmapd check compares the executable path exactly

- **Authority.** PREP-6 review-0 (`.agent-manager/slices/TEST-EDGE-SCOPE-1B-PREP-6/review-0.json`, refinement-required, finding TESB-C14, RG-REQ-011-L06): "It excludes any command containing `/.local/bin/rmapd`, not just `$HOME/.local/bin/rmapd`. For example, `/private/tmp/fake/.local/bin/rmapd 30` passes that filter … Compare the executable path with the operator's exact installed path, then probe a foreign `rmapd` under a different `.local/bin` directory. Keep the current probe that accepts the operator process."
- **Superseded text (cycle 1, kept as written in OC-3 above).** `if [ -n "$PIDS" ]; then CMDS=$(ps -o command= -p "$(printf '%s\n' "$PIDS" | paste -sd, -)") || fail "ps failed for rmapd pids $PIDS"; OTHER=$(printf '%s\n' "$CMDS" | grep -v '/\.local/bin/rmapd') || [ $? -eq 1 ] || fail "grep failed"; [ -z "$OTHER" ] || fail "rmapd other than the operator's ~/.local/bin/rmapd: $OTHER"; fi`. The INPUT-4 command had the same substring filter. OC-3's P9 row ("a foreign `rmapd` … exit 1") is correct for its fake, but it did not test a fake under another `.local/bin`.
- **Further fact (EXECUTED).** `ps -o comm=` and `ps -o command=` print argv[0] on macOS. With `(exec -a "$HOME/.local/bin/rmapd" <fake>/.local/bin/rmapd 900)`, ps printed comm `/Users/apple/.lo…` and command `/Users/apple/.local/bin/rmapd 900`, while the executable was `/bin/sleep` (lsof txt and `proc_pidpath`). So an exact comparison of `comm` would still be a proxy.
- **Class and rule.** Class: an identity assertion that accepts something that looks like the identity instead of the identity itself. Rule: for each pid that `pgrep -x rmapd` lists, read the kernel's executable path with `proc_pidpath` (macOS libproc, `python3 -c`). Require it to equal exactly `$HOME/.local/bin/rmapd`. If the path cannot be read, fail. `$HOME/.local/bin/rmapd` is a regular file, not a symlink (OBSERVED, `ls -l`), so the resolved executable path of the operator's daemon is that path (EXECUTED: `proc_pidpath(8278)` = `/Users/apple/.local/bin/rmapd`).
- **New lines (verbatim in the allocation).** `PIDS=$(pgrep -x rmapd) || [ $? -eq 1 ] || fail "pgrep failed"` and `for p in $PIDS; do EXE=$(python3 -c '…proc_pidpath…' "$p") || fail "executable path of rmapd pid $p unreadable"; [ "$EXE" = "$HOME/.local/bin/rmapd" ] || fail "rmapd other than the operator's $HOME/.local/bin/rmapd: pid $p runs $EXE"; done`.
- **Probes (EXECUTED 2026-10-02).** The scratch worktree `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-probe` was detached at 7a948ff5, and the fakes ran under `/private/tmp/TEST-EDGE-SCOPE-1B-PREP-6-fake`; both were removed by a trap. "rmapd lines" means the two lines run verbatim under `set -euo pipefail` with `fail` defined. "CYCLE1" means the superseded cycle-1 line. The full C14 is the command as it now stands in the allocation.

  | Probe | Expected | Actual |
  |---|---|---|
  | R0 step-0 recording | exit 0 | exit 0 |
  | R1 full C14, only the operator's `~/.local/bin/rmapd` (pid 8278) running | exit 0, `ok` | exit 0, `TESB-C14 ok` |
  | R5 rmapd lines, operator only | exit 0 | exit 0 |
  | R2 rmapd lines, a fake at `…-PREP-6-fake/.local/bin/rmapd 900` (a symlink to `/bin/sleep`) | exit 1 | exit 1, `TESB-C14 FAIL: rmapd other than the operator's /Users/apple/.local/bin/rmapd: pid 87342 runs /bin/sleep` |
  | R2 CYCLE1 line, the same fake | (the finding) | exit 0: passes |
  | R4 full C14, the same fake | exit 1, no `ok` | exit 1, the same FAIL line, no `ok` |
  | R3 rmapd lines, the fake with argv[0] = `/Users/apple/.local/bin/rmapd` | exit 1 | exit 1, `… pid 87427 runs /bin/sleep` |
  | R3 CYCLE1 line, the same spoof | (proxy defect) | exit 0: passes |
  | R5b rmapd lines, the fakes stopped | exit 0 | exit 0 |
  | R6 full C14, `…-leveldb-before` renamed (the cycle-1 property is kept) | exit 1, no `ok` | exit 1, `FAIL: before-root missing or empty: …` |
  | R7 full C14, everything restored | exit 0, `ok` | exit 0, `ok` |

  The digest of the five roots was `7d4b12c73d44a41e9aef3d3638bf130177bde9cf8c9e3401752190a57e9170b0` before and after. After cleanup, only the operator's pid 8278 matched `pgrep -x rmapd`.
- **Limits.** The check is macOS-only (libproc); the allocation already names macOS paths (`$HOME/Library/Application Support`). A process that exits between `pgrep` and `proc_pidpath` makes the check fail; the remedy is to rerun it. A second process that runs the operator's installed binary itself would pass, as it did under every earlier form of this check. The assertion is about which executable runs, not how many instances run.

## Operator confirmation (2026-10-02, in-place-manager) — the INPUT-5 correction and the restored roots

The TESB-C14 correction authored by PREP-6 is approved; any "Approver: PENDING" line on it is resolved by this entry. TESB-FIELD-EVIDENCE-4 (admission-4 review-0) is resolved as option A by the operator: the five before-roots were re-captured on 2026-10-02 with the same producer (HEAD's release binary; `git diff a85f6239 HEAD -- rust` empty) on checkouts at exactly their recorded commits and dirty states; `source.txt` in each root records this; a durable copy is kept at `~/repo-graph-retained/TEST-EDGE-SCOPE-1B-before/`. Accepted by PREP-6 review-1 (codex gpt-6-sol). Carried as INPUT-5.

---

# INPUT-6 entries (2026-10-02)

Baseline these entries enter: `docs/requirements/baselines/TEST-EDGE-SCOPE-1B-INPUT-6.json`. Admitted baseline before them: `TEST-EDGE-SCOPE-1B-INPUT-5.json` (committed at 774cecca). Trigger: implementation review-0 of the admission-5 candidate (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/review-0.json`, verdict `decision-required`; the candidate is `.agent-manager/slices/TEST-EDGE-SCOPE-1B/candidate-admission-5.patch`, 95 paths, byte-identical to `candidate-admission-4.patch`, sha256 `55e43da90a04b605dc2ed74fe7beecee64380ed85adaf74433be2c628c9995a2`, EXECUTED `cmp`). On the re-captured before-roots, 15 of 17 checks passed, including C12 and the fail-closed C14. TESB-C11 and TESB-C13 failed on the oracle text, not on the product. The reviewer's decision TESB-FIELD-ORACLES, option A: "amend only C11/C13 oracle text … Check the C11 SCC member set and each displayed walk edge; allow documented metadata after C13's exact source-state fields." The operator (in-place-manager) took A; the human may override. Author: document item TEST-EDGE-SCOPE-1B-PREP-7 (builder role, claude-opus-5-5). Independent review of these entries: pending (the PREP-7 reviewer). Every entry above is unchanged.

## OC-4 — 2026-10-02 — oracle correction: TESB-C11 does not pin a cycle walk

- **Authority.** Review-0: "C11 pins a particular *before* cycle walk. … `cycle_walk.rs` sorts neighbours by node UID; the indexer creates directory-module UIDs with UUID v4. The re-captured store prints `db → util → helpers/memenv → db`, not the pinned `db → table → db`. I checked that the actual walk's three arrows are edges in the captured SCC, whose four-member set is unchanged." Option A, as the operator worded it in the PREP-7 packet: assert (a) the excluded cycle's member set is exactly {db, table, util, helpers/memenv}, (b) every displayed arrow is an edge of that SCC, checked against the store, (c) `table → db` is present under `--include-tests` and absent by default, as before; pin no walk order anywhere in the document.
- **Historical result, kept.** The INPUT-5 C11 command (774cecca) held, verbatim:
  - `has(rd(T + "b-orient.txt"), "1 import cycle (db -> table -> db). Docs: README.md, CONTRIBUTING.md.")`
  - `be = rd(T + "b-ex.txt"); has(be, "Import cycles (1)"); has(be, "  - Cycle 1 (4 modules): db -> table -> db")`

  Admission 5 ran it: exit 1, `AssertionError: ('missing line', '1 import cycle (db -> table -> db). Docs: README.md, CONTRIBUTING.md.')` (`/tmp/tesb-a5-run-TESB-C11.out`, OBSERVED). HEAD printed, on a copy of the re-captured leveldb root, `1 import cycle (db -> util -> helpers/memenv -> db). Docs: README.md, CONTRIBUTING.md.` (`/tmp/tesb-c11-b-orient.txt` line 4) and `  - Cycle 1 (4 modules): db -> util -> helpers/memenv -> db` / `    (+ 1 more member in this cycle)` (`/tmp/tesb-c11-b-ex.txt` lines 27–28), both OBSERVED. The builder's diagnostic, with only those two literals made soft, printed `TESB-C11 ok` (`.agent-manager/slices/TEST-EDGE-SCOPE-1B/build-progress.md`). **The admission-5 C11 result is a failure and stays one. The diagnostic is not a pass of C11.**
- **Store facts (EXECUTED, read-only, python `sqlite3 ?immutable=1` over `/private/tmp/TEST-EDGE-SCOPE-1B-leveldb-before`, with C11's own `facts`/`dir_graph`/`sccs` functions).** All 498 file→file IMPORTS rows are `static`. Test-inclusive directory graph, edges among the four members: `db→table`, `db→util`, `helpers/memenv→db`, `helpers/memenv→util`, `table→db`, `table→util`, `util→helpers/memenv`; its SCCs: exactly `[(db, helpers/memenv, table, util)]`. Default (production) graph, the same members: `db→table`, `db→util`, `helpers/memenv→util`, `table→util`; no SCC. So both walks, `db→table→db` and `db→util→helpers/memenv→db`, are closed walks over edges of the same SCC, and `table → db` exists only through test imports.
- **Class and rule.** Class: an oracle that pins a rendering whose order follows an identifier that the source does not determine (here a UUID v4 node UID). Rule: no rendered cycle walk is pinned as text anywhere in the slice document. A walk is asserted as exactly one walk line, closed (first member = last), every arrow an edge of the asserted SCC in the store; the SCC is asserted by its member set; an off-walk count line, where rendered, together with the walk accounts for the member set.
- **New lines (verbatim in the allocation; they replace the two lines above).**
  ```
  # orient and explain: a rendered cycle walk is never pinned as text (OC-4; the walk order follows node UIDs, §8 CYCLE-WALK-DETERMINISM-1)
  SCC = ("db", "helpers/memenv", "table", "util")
  ADJ = dir_graph(Fx, True, False)[0]
  assert sccs(ADJ) == [SCC] and members(b) == [SCC], ("the before view's one SCC is exactly the four members", sccs(ADJ), members(b))
  assert "db" in ADJ["table"] and "db" not in dir_graph(Fx, False, False)[0].get("table", set()), "table -> db exists only through imports from test files"
  def ring(text, pattern):
      hits = [m.group(1) for m in (re.fullmatch(pattern, l) for l in text.splitlines()) if m]
      assert len(hits) == 1, ("one rendered cycle walk", pattern, hits)
      w = hits[0].split(" -> ")
      assert len(w) >= 3 and w[0] == w[-1], ("a closed walk", w)
      for x, y in zip(w, w[1:]):
          assert x in SCC and y in SCC and y in ADJ.get(x, set()), ("a displayed arrow that is not an edge of the SCC in the store", x, y)
      return w
  ring(rd(T + "b-orient.txt"), r"1 import cycle \((.+)\)\. Docs: README\.md, CONTRIBUTING\.md\.")
  ...
  be = rd(T + "b-ex.txt"); has(be, "Import cycles (1)"); we = ring(be, r"  - Cycle 1 \(4 modules\): (.+)")
  k = len(SCC) - len(set(we))
  assert [l for l in be.splitlines() if re.fullmatch(r"\s*\(\+ \d+ more members? in this cycle\)", l)] == ([] if k == 0 else ["    (+ %d more member%s in this cycle)" % (k, "" if k == 1 else "s")]), ("the walk and the off-walk count account for the member set", k)
  ```
  Kept, unchanged: `members(b) == [("db", "helpers/memenv", "table", "util")]`, the candidate's `excluded_cycles` = that set under `include_tests`, `members(at) == members(b)`, the store recomputation `ex`, and requirement (c) "as before": `("table", "db", 3) in edges(amt) and not any(e[:2] == ("table", "db") for e in edges(am))` and the `modules deps table` rows. The new store assertion on `table → db` adds the directory level; it does not replace the module level.
- **Class sweep (EXECUTED, python over the whole slice document).** Patterns: `\w+ (->|→) \w+ (->|→)`, `Cycle \d+ \(\d+ modules\):`, `import cycle \(`, and the same over every check's `command`, `inputs`, `environment` and `expected`. Hits: C11's command (the two lines above, changed); §1's leveldb grounding (it quoted HEAD's walk from the 2026-09-29 root; now an observation of each root, no pin); §2.2's field-witness row (`leveldb's db → table → db leaves cycles`; now the SCC by its members, citing RG-REQ-004-L12's wording). Not walks: `table -> db` as a `modules deps` row (§2.4, C11); the check order `C01 → C02 → …` in §5. C12, C13 and C15 pin no walk: they compare before and after binaries on one store, where the UIDs are the same. After the edit, no check command contains a `x -> y -> z` string (EXECUTED).
- **Probes (EXECUTED 2026-10-02).** "Captures" are the admission-5 run's `/tmp/tesb-c11-*` files (all written 2026-10-02 18:14 local by one run, OBSERVED `ls -T`); the store is the manager's leveldb root, read immutably. No rmap, no cargo.

  | Probe | Expected | Actual |
  |---|---|---|
  | N0 the new assertion lines alone, real captures | pass | pass, walk `['db', 'util', 'helpers/memenv', 'db']` |
  | N1 the same, the oracle's member set without `util` (`SCC = ("db", "helpers/memenv", "table")`) | fail | `AssertionError: ("the before view's one SCC is exactly the four members", …)` |
  | N2 captures edited to the 2026-09-29 walk (`db -> table -> db`, `(+ 2 more members in this cycle)`) | pass (walk order is not pinned) | pass, walk `['db', 'table', 'db']` |
  | N3 orient capture edited to `db -> helpers/memenv -> db` (`db→helpers/memenv` is not an edge) | fail | `AssertionError: ('a displayed arrow that is not an edge of the SCC in the store', 'db', 'helpers/memenv')` |
  | N4 explain capture's off-walk line edited to `(+ 2 more members …)` beside a three-member walk | fail | `AssertionError: ('the walk and the off-walk count account for the member set', 1)` |
  | N5 the WHOLE corrected C11 Python, extracted from the allocation, real captures | `TESB-C11 ok` | `TESB-C11 ok`, exit 0 |
  | N6 the WHOLE INPUT-5 C11 Python, real captures | the admission-5 failure | `AssertionError: ('missing line', '1 import cycle (db -> table -> db). …')` |
- **Limits.** N5 runs C11's Python over captures from the admission-5 run. It is not a rerun of C11: the shell part (worktree, builds, serving, `rg-store-diff`) did not run here. The corrected C11 must run on the next admission. The walk check proves that each displayed arrow is a store edge; it does not prove that the walk is the one `cycle_walk.rs` would choose, which is the subject of CYCLE-WALK-DETERMINISM-1.

## OC-5 — 2026-10-02 — oracle correction: TESB-C13's `source.txt` preconditions accept the documented suffix

- **Authority.** Review-0: "C13 pins the end of each `source.txt` line. The re-captured grpc-java and FRAKTAG lines have the required commit and dirty counts, followed by `captured=… producer=…`. The approved `$`-anchored greps reject those lines." Option A: require the exact commit and `dirty=<n>` fields and accept the documented trailing metadata.
- **Historical result, kept.** The INPUT-5 C13 command (774cecca) held `grep -q '^grpc-java f43013161b3c dirty=2997$' $R-grpc-java-before/source.txt && grep -q '^FRAKTAG 9bea3a5e4d32 dirty=14$' $R-FRAKTAG-before/source.txt`. Admission 5: exit 1 with empty output (`/tmp/tesb-a5-run-TESB-C13.out`, 0 bytes, OBSERVED), before any product assertion; its trap was not yet set, so the worktree survived. The builder's diagnostic with `dirty=2997( |$)` / `dirty=14( |$)` passed (`/tmp/tesb-a5-run-C13-diag.out`, `TESB-C13 ok`). **The admission-5 C13 result is a failure and stays one. The diagnostic is not a pass of C13.**
- **The real lines (OBSERVED, first line of each root's `source.txt`).** `grpc-java f43013161b3c dirty=2997 captured=2026-10-02T14:25:01Z producer=rmap 0.19.0` and `FRAKTAG 9bea3a5e4d32 dirty=14 captured=2026-10-02T14:25:19Z producer=rmap 0.19.0`. The format `<repo> <commit> dirty=<n> captured=<iso> producer=<rmap version>` is the one the INPUT-5 record above documents.
- **Class and rule.** Class: a source-state precondition that anchors the end of a record whose documented format carries trailing fields. Rule: require the repository name, the 12-character commit and the `dirty=<n>` field exactly; accept only the documented suffix ` captured=<YYYY-MM-DDThh:mm:ssZ> producer=rmap <x.y.z>`, or none; anything else after `dirty=<n>` fails. The diagnostic's `( |$)` would also have accepted an undocumented suffix, so it is not used.
- **New lines (verbatim in the allocation).** `grep -Eq '^grpc-java f43013161b3c dirty=2997( captured=[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z producer=rmap [0-9]+\.[0-9]+\.[0-9]+)?$' $R-grpc-java-before/source.txt && grep -Eq '^FRAKTAG 9bea3a5e4d32 dirty=14( captured=[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z producer=rmap [0-9]+\.[0-9]+\.[0-9]+)?$' $R-FRAKTAG-before/source.txt`.
- **Class sweep.** No other check command reads a `source.txt` (EXECUTED, python over all seventeen commands). C11 and C12 assert their checkouts with `git rev-parse --short=12 HEAD` and `git status --porcelain`; C14 digests the roots and does not parse `source.txt`.
- **Probes (EXECUTED 2026-10-02, grep -E on macOS).**

  | Probe | Expected | Actual |
  |---|---|---|
  | Q1 grpc-java root's real `source.txt` | match | match |
  | Q2 FRAKTAG root's real `source.txt` | match | match |
  | Q3 grpc-java line, commit `f43013161b3d` | no match | no match |
  | Q4 FRAKTAG line, commit `9bea3a5e4d33` | no match | no match |
  | Q5 grpc-java line, `dirty=29970` | no match | no match |
  | Q6 grpc-java line in the INPUT-5 form (no suffix) | match | match |
  | Q7 grpc-java line with an undocumented suffix ` garbage` | no match | no match |
  | Q8 FRAKTAG line, `dirty=1` | no match | no match |
  | Q9 the two greps extracted verbatim from the allocation, real roots | exit 0 | exit 0 |
  | Q10 the same, the oracle's grpc-java commit changed to `f43013161b3d` | exit ≠ 0 | exit 1 |
  | Q11 the INPUT-5 grep, real grpc-java root | (the finding) | no match |
- **Limits.** The probes test the two preconditions only. The corrected C13 must run on the next admission.

## Operator confirmation (2026-10-02, in-place-manager) — the INPUT-6 corrections

The C11 and C13 corrections authored by PREP-7 are approved (admission-5 review-0's option A; text-only); any "Approver: PENDING" line on them is resolved by this entry. The follow-up CYCLE-WALK-DETERMINISM-1 is recorded: a cycle's rendered walk order follows random node UIDs, so two indexes of one commit can print different walks of the same cycle (RG-REQ-001-L09); outside this allocation. Accepted by PREP-7 review-0 (codex gpt-6-sol). Carried as INPUT-6.
