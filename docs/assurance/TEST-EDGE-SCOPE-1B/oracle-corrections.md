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
