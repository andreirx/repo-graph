# CALL-BINDING-RECEIVER-1 — a C++ call binds to the receiver's type or stays unresolved; never to the caller's own class by default

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q1 (CRITICAL; REGRESSION from CPP-DECLARATORS-1 5c3ec2d). CODE slice: cpp-extractor + indexer resolver + two rewritten tests. Maturity: MATURE surfaces (`callers`, `callees`, `explain`, `trust`, `dead`). Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra (overhaul-track assignment).

## 0. Requirements allocation (catalog `docs/requirements/`, revision under human approval)

**Implements:** RG-REQ-005-L01 (indirect receiver never binds to the enclosing class without receiver-type evidence), RG-REQ-005-L02 (binding only on evidence; enclosing-type preference receiverless/explicit-`this` only), RG-REQ-002-L01 (no surface asserts a relationship the store does not hold — the CALLS half), RG-REQ-009-L03 (resolution % counts only evidence-bound edges), RG-REQ-001-L03 (unresolved rows preserved — restore the rows the regression consumed).

**Changes (evidence moves, pre-authorised):** RG-REQ-009-L03/L04 — trust's "calls resolved %" FALLS on every C++ repo (leveldb 34.7% → ≈33%); RG-REQ-013-L07 — `dead`'s substrate reports MORE candidates (155 leveldb symbols lose their fabricated fan-in); RG-REQ-005-L08 — callers/callees rows gain the real caller (`DB::Open`) and lose the self-row. Each movement is reported before/after with attribution; none is hidden or compensated.

**Preserves (the regression watch, §3):** RG-REQ-001-L02/L04/L05, RG-REQ-005-L05/L06/L07/L09, RG-REQ-006-L01/L02/L04 (the shared resolver's other stages), RG-REQ-007-L03, RG-REQ-012-L02, RG-REQ-004-L07 (cycles), RG-REQ-011-L06 (isolation), and every non-C/C++ extractor byte-stable.

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-08-root-causes-v0.18.0.md` RC-1, store-verified)

Outward: `callers leveldb::DBImpl::Recover` answers "itself"; the real caller `DB::Open` (db_impl.cc:1511) is missing; `callees` lists itself; `explain` shows the same. Receiver-bearing CALLS self-loops on the retained stores: 0 before → 155 leveldb / 497 OpenXcom / 836 vcmi / 1,098 poco / 2,522 duckdb / 321 gstreamer after; codegraph 0. Each loop gives its symbol fan-in ≥ 1 (shielding it from `dead`) and inflates trust's calls-resolved %.

Cause: `cpp-extractor/src/extractor.rs:1648 extract_call` emits the BARE field name as `target_key` and stores the receiver only as metadata (`{"calleeName","receiver"}`, comment :1657 "NOT consumed here"); `indexer/src/resolver.rs:951-962` (CPP-DECLARATORS-1 §2.3 amended) applies `enclosing_class_preference` (:1113) to every ambiguous C-family pool — picking the candidate in the CALLER's class without reading the receiver — so `versions_->Recover(...)` inside `DBImpl::Recover` binds to itself, and `impl->Recover(...)` inside `DB::Open` (container `leveldb::DB`, no candidate) is dropped as unresolved. The two tests that pinned the feature (`resolver.rs:1723` — a fictional `leveldb::DBImpl::Open`; `:1763` — encodes the dropped outside caller as intended) are wrong premises.

## 2. Contract

1. **Receiver types become extractor facts (C++).** In `cpp-extractor`, the `field_declaration` arm (:763) records `(enclosing class, member name) → declared type` (strip `*`, `&`, `const`, template arguments; keep the qualified name when written); the per-function local-variable map generalises `ctx.local_stream_types` (CPP-SB-1 D3, cleared at :1621) to any declared local type. A `field_expression` call carries `metadata_json.receiverType` (the resolved declared type, qualified where known) beside the existing `receiver`; `this->m()` / `(*this).m()` / an unqualified `m()` carry `receiver: "this"` or no receiver. Metadata keys are additive; no new column.
2. **A receiver-type binding stage in the shared resolver.** In `resolve_call_target`, BEFORE the bare-name singleton at :942 and gated on the edge carrying `receiverType`: look up `<receiverType>::<target_key>` in the qualified-name/stable-key index; on a unique hit bind; on a miss with IMPLEMENTS edges available, walk the receiver type's bases (BFS, first depth with exactly one hit); otherwise fall through to the existing stages.
3. **The invariant (as corrected by the adjudicator):** an INDIRECT receiver (any receiver other than `this`/explicit self) never binds to the caller's enclosing class without receiver-type evidence. `enclosing_class_preference` runs ONLY for receiverless and explicit-`this` calls. The C-family gate stays; C emits no receiver and is untouched.
4. **Honest remainder.** A receiver-bearing call whose receiver type is unknown or whose target is ambiguous stays unresolved with category `calls_obj_method_needs_type_info` (or a new `calls_receiver_type_unknown` if the distinction is needed — additive, mapped in `attribution.rs`), and is COUNTED. Recovering rows that RC-1 had consumed is the point, not a loss.
5. **Tests rewritten at the cause.** `resolver.rs:1723` and `:1763` are replaced by: (a) class `A { void run(); }`, class `B { A* a_; void run() { a_->run(); } }` → `callers A::run` = `B::run`, `callees B::run` = `A::run`, no self-loop; (b) `B::run() { run(); }` and `this->run()` → `B::run` (explicit self binds to the enclosing class); (c) `B { X* x_; void run() { x_->run(); } }` with `X` not indexed → unresolved, counted, not bound to `B::run`; (d) a receiverless ambiguous call from outside any class stays unresolved. A corpus assertion test over an indexed fixture: `SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%receiver%' AND metadata_json NOT LIKE '%"receiver":"this"%'` = 0.
6. **Outward proof on leveldb (isolated index, `RMAP_TRANSPORT=stdio`):** `callers leveldb::DBImpl::Recover` = `leveldb::DB::Open` (anchor the call site db_impl.cc:1511 where RG-REQ-005-L08 allows; at minimum the declaration anchor 1503 with the current renderer); `callees leveldb::DBImpl::Recover` includes `leveldb::VersionSet::Recover` and excludes itself; the self-loop count = 0; resolved/unresolved CALLS reported before/after (before 3,261/6,152); trust's calls-resolved % before/after with the decrease attributed; `dead`'s substrate count before/after.

## 3. Regression watch — what this slice must NOT change, and the test that proves it

| Preserved L | What would regress | Proof (existing test / write-first / corpus) |
|---|---|---|
| RG-REQ-005-L02, RG-REQ-001-L02 | unique-name binding and decl/def filtering for every language | `resolver.rs::ambiguous_name_stays_unresolved`, `one_definition_plus_n_forward_decls_resolves_to_the_definition`, `method_call_prototype_plus_definition_resolves_to_definition`; `indexer/tests/parity.rs::parity_against_shared_indexer_fixture_corpus` |
| RG-REQ-006-L01/L02/L04 | the Rust crate, Java suffix, TS/Python import stages of the same resolver | `rust_crate_import_*`, `java_suffix_*`, `aliased_named_import_uses_imported_name_for_lookup`, `namespace_import_member_resolves_to_target_module`, `file_resolution_*` — all green, untouched |
| RG-REQ-005-L07 | forward-decl flag read and `(decl)` ranking | `forward_decl_classification_reads_the_stamped_key`, `…malformed_metadata_is_unreadable_not_a_definition`, `find_facts/rank.rs::definition_beats_forward_decl` |
| RG-REQ-005-L05 | symbol resolution by key/qualified name/suffix (SYMBOL-IDENTITY-1) — this slice touches call BINDING, not lookup | `storage/src/queries.rs` resolve_symbol suffix tests; `agent/tests/explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver` |
| RG-REQ-001-L03, RG-REQ-005-L09 | unresolved classification and reader mapping | `categorize_*` tests; `agent/src/attribution.rs::every_basis_code_maps_to_its_expected_reader_class` (a new category needs a mapping) |
| RG-REQ-001-L02 non-C/C++ | every other extractor byte-stable | CALLS/IMPORTS edge counts and `unresolved_edges` counts identical before/after on isolated indexes of FRAKTAG (TS) and repo-graph's `rust/crates/gate` fixture or kafka's `clients` subtree (Java) — store SQL, copies only |
| C extractor | C emits no receiver; unchanged | nginx or sqlite fixture edge counts identical; `include_resolver.rs` tests untouched |
| RG-REQ-004-L07 | cycles exclude size-1 self-loops anyway | `graph-algorithms/src/scc.rs::self_loop_not_counted_as_cycle_by_size` stays; `rmap cycles` on leveldb byte-identical |
| LiveGraph callgraph certificate / union serve | the cert admits `EdgeType::Calls`; fewer, truer edges must not turn it RED spuriously | `daemon-runtime/src/callgraph_cert` tests green; if the cert compares edge SETS across engines, both engines see the same new set (the resolver is shared) — state the observation |
| RG-REQ-012-L02, RG-REQ-013-L07 | exit codes; `dead` refusal shape unchanged (its substrate count moves — a CHANGE, above) | `exit_code_contract.rs`; `dead_command.rs::dead_command_is_disabled` |
| RG-REQ-011-L06 | isolation | every `rmap` call under `RMAP_STATE_ROOT`/`RMAP_SOCKET_PATH` in a throwaway root; registry sha256 of the real root identical before/after (cite both) |

A preserved row whose proof is missing blocks the DoD; a preserved row whose proof fails is a STOP, not a fix-forward.

## 4. Stop conditions

Frozen: wire protocol (metadata keys additive), storage schema (no new column; a new `UnresolvedEdgeCategory` variant is additive and must be mapped), exit codes, `find`'s semantics, the LiveGraph↔SQLite parity certificate, `resolve_symbol_name`/`resolve_symbol` routing (ruling B), tree-sitter grammar version, every non-C/C++ extractor. If the receiver-type stage needs IMPLEMENTS edges anchored on the CLASS node (today they are anchored on the FILE — RC-3), implement the direct `<receiverType>::<name>` lookup only and record the base-walk as dependent on EXPLAIN-TYPE-SECTIONS-1 step 2 — do NOT re-anchor IMPLEMENTS here. If the LiveGraph callgraph cert turns RED for a reason other than the intended edge change, STOP + DECISION_REQUIRED. STANDING HONESTY RULES (no `unwrap_or(0)`/`.ok()`/`unwrap_or_default` on any fallible read; a malformed `receiverType` is unreadable-with-reason, not absent). Unmet DoD → STOP + DECISION_REQUIRED. Do NOT commit.

## 5. Validation (SYNCHRONOUS; ORDERED; `build-progress.md` written after EACH step — binding for a Codex builder)

1. Failing tests FIRST (§2.5 a–d + the corpus assertion on a two-class fixture); then the extractor facts; then the resolver stage; then the invariant guard.
2. Chunked gates: `cargo test -p repo-graph-cpp-extractor`, `-p repo-graph-indexer`, `-p repo-graph-classification`, `-p repo-graph-agent` (attribution), `-p repo-graph-storage --lib resolve_symbol`; `cargo fmt`/`clippy` per crate. NEVER `cargo test --workspace` (the operator's suite runs it after approval).
3. Live proof on the SMALLEST corpus: isolated index of leveldb (`RMAP_TRANSPORT=stdio`; a Codex sandbox cannot bind sockets); before-binary from a `git worktree` of HEAD on the SAME isolated index for the before numbers; the §2.6 outputs verbatim; the self-loop SQL on the isolated store copy.
4. Byte-stability proofs (§3 rows for FRAKTAG/Java/C) from isolated indexes of the named small fixtures — counts, not full diffs.
5. Cleanup of isolated roots; `build-N.md` with EXECUTED/OBSERVED/NOT RUN labels per step.

OPERATOR-RUN after approval: the self-loop count on the five other C++ corpora from fresh isolated indexes (poco/duckdb/gstreamer are too large for the builder window) and the full gate suite (`agent-manager/scripts/repo-graph-gates.sh`).

## 6. Definition of done

Every implemented L holds (§2.5 tests green; leveldb §2.6 outputs as stated; self-loops 0 on the fixture and leveldb); every preserved row in §3 has its proof EXECUTED and green; the three pre-authorised movements are reported with before/after numbers; no non-C/C++ edge count moved; gates green. Reviewer checks the §3 table row by row — a positive verdict without it is incomplete.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; kafka at ../legacy-codebases/kafka; nginx/sqlite at ../legacy-codebases/<name>; repo-graph is THIS repo.
