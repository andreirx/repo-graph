# PYTHON-RECEIVER-BINDING-1 — oracle corrections ledger

Append-only (agent-manager docs/MANAGER.md § Oracle corrections).

## OC-1 (2026-09-27) — isolated rmap invocations lacked `RMAP_AUTO_REINDEX=off` (→ INPUT-2)

- **Old:** `RMAP_AUTO_ENRICH=off`. **New:** `RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off`. **Evidence:** TOOLCHAIN-STALENESS-1 re-indexes a store whose stamp differs on the first request (D-STALE-SIGNAL-1). **Also:** before-roots re-captured 2026-09-27 with the release binary of HEAD 9df4a6ee under this slice's names (`/private/tmp/PYTHON-RECEIVER-BINDING-1-{django,leveldb}-before`). **Approver:** in-place-manager. **Carried as:** INPUT-2.
- **Slice document digests:** before sha256:ed9188013eb35ec4e03d13c0ced9197fa6f234e66a1173e1155028240fffdff3; after OC-1 sha256:0ca8e4cca6f3db3b2cebdd06b0646f813b261820bbe7a590ec0214f9bbde87cd.

## OC-2 (2026-09-27, PYTHON-RECEIVER-BINDING-1-PREP-7) — the slice narrated the superseded staleness signal (→ INPUT-2)

- **Rule:** the slice states its two version bumps (`python-core` 0.1.0 → 0.2.0, `INDEXER_VERSION` 1.1.0 → 1.2.0) as its own. What an existing store then shows and does is TOOLCHAIN-STALENESS-1's under RG-REQ-001-L06 as amended by RG-BOOTSTRAP-INPUT-10 (D-STALE-SIGNAL-1) and is not restated. Model: CPP-ATTRIBUTE-MACRO-1A OC-2/OC-3 and PYTHON-SUBMODULE-IMPORT-1 OC-2.
- **Checks:** none asserted the superseded shape. No command or `expected` of the twelve checks greps `… facts from python-core:0.1.0 … to refresh`, reads `families`, requires a `TOOLCHAIN_STALENESS` condition or requires exit 2 (OBSERVED: `grep` of the block). No staleness oracle is added; the one shipped case is proven by TOOLCHAIN-STALENESS-1 and CPP-ATTRIBUTE-MACRO-1A's CAM-A04. The shipped line, read at HEAD for the record, is `index toolchain differs from running rmap (<component> <old> → <new>[, …]) — run rmap repo rebuild <path>` (`rgr/src/presentation/toolchain_staleness.rs:34/:44`).
- **Old → new text:**
  - PRB-C12 `inputs` VERSION: "so every pre-slice Python snapshot … is reported stale until `rmap repo rebuild <path>`" → the one statement.
  - PRB-C01 `inputs` state (iii-a): "the fact is stale and TOOLCHAIN-STALENESS-1's Python line (`Python facts from python-core:0.1.0 (current python-core:0.2.0) — run rmap repo rebuild <path> to refresh`) states it until a full index (RG-REQ-001-L06; D-REFRESH-STALE-1/2)" → the one statement.
  - PRB-C08 `inputs`: "the ratified refresh of RG-REQ-011-L04 that the staleness line already prints" → "RG-REQ-011-L04's ratified refresh".
  - Prose: Status (the TOOLCHAIN-STALENESS-1 gate satisfied at HEAD, 23d12a9b); the §0 D-REFRESH-STALE-1 and D-PRB-CARRIER-1 bullets; D-STALE-SIGNAL-1 and D-PSI-R1-VOCAB added to §0's decision records (the manifest's new `requiredDecisionIds`); the §0 D-PRB-ENRICH-1 correction and legacy-label bullets; §2.1 R1 (iii-a) and step 1a; §2.2; §3 (two residuals); §6 ship block; §8.
- **Author:** requirements author of PYTHON-RECEIVER-BINDING-1-PREP-7 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

## OC-3 (2026-09-27, PREP-7) — PRB-C01's version oracle measured ef1d5300 (→ INPUT-2)

- **Old:** `^const INDEXER_VERSION` (one match required); compose.rs to differ from HEAD by exactly its `let extractor = "<v>"; // Match INDEXER_VERSION …` line; "the old version literal leaves exactly the two declared sites"; floor 466. **New:** PYTHON-SUBMODULE-IMPORT-1's accepted PSI-C01 form:
  - `^pub const INDEXER_VERSION`;
  - compose.rs unchanged and reading `orchestrator::INDEXER_VERSION` by name;
  - the old literal leaves the orchestrator and moves nowhere else;
  - the new literal is added nowhere outside the orchestrator;
  - `build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version` bound by name (fifteen existing tests);
  - floor 484 (OC-5).
- **Evidence (OBSERVED at 9df4a6ee):**
  - `orchestrator.rs:63` `pub const INDEXER_VERSION: &str = "indexer:1.1.0";`
  - `compose.rs:1647` `let extractor = orchestrator::INDEXER_VERSION;`
  - `orchestrator.rs:4388/:4391/:4396` pin `indexer:1.1.0`.

  The old oracle could not pass on any correct candidate. `PYTHON_EXTRACTOR_PREFIX` exists at `resolver.rs:56`, so the "or by this slice" clause is removed.
- **Probes** (scratch worktree `/private/tmp/PYTHON-RECEIVER-BINDING-1-PREP-7-wt` at 9df4a6ee, removed; the resolver literals stubbed in as one comment line). The oracle passes on the correct bump (declaration + pinning test). It fails on:
  - HEAD unmoved;
  - the declaration bumped with the pinning test unmoved;
  - compose.rs turned back into a literal;
  - the new literal added in `agent/src/dto/toolchain_staleness.rs`;
  - a major bump;
  - the correct bump without the resolver literals.
- **Unchanged:** compose.rs stays a candidate path the bump no longer touches (PYTHON-SUBMODULE-IMPORT-1's precedent). **Approver:** PENDING — the operator. **Carried as:** INPUT-2.

## OC-4 (2026-09-27, PREP-7) — before-roots: the removed django root, and a manager root under a builder-owned name (→ INPUT-2)

- **Old:**
  - PRB-C09 read `/private/tmp/PYTHON-SUBMODULE-IMPORT-1-django-before`, which that slice removed at its closeout.
  - PRB-C09 asserted `test ! -e /private/tmp/PYTHON-RECEIVER-BINDING-1-leveldb-before`, built a base binary from a detached worktree of HEAD and indexed leveldb into that name.
  - PRB-C10 `rm -rf`'d it together with `-before-bin` and the worktree.
- **Evidence (OBSERVED 2026-09-27):** the manager's OC-1 re-capture occupies `/private/tmp/PYTHON-RECEIVER-BINDING-1-leveldb-before` (133 files, 3,488 CALLS). PRB-C09 would stop at its first link, and PRB-C10 would delete a root the builder did not create (CLAUDE.md: "Never delete a root you did not create").
- **Rule:** a manager root is read, never created, served, written or deleted by a check.
- **New:**
  - PRB-C09's first link is `git diff --quiet 9df4a6ee08f578259ae8fb100a7e6bd429aaed2b HEAD -- rust`: HEAD's rust tree is still the one whose release binary captured both roots. A moved tree is a STOP (§4), and the manager re-captures.
  - Both manager roots must hold `registry.json` and an empty WAL. Both are read with `immutable=1` only: the python block reads django, and `rg-store-diff.py` reads django and leveldb.
  - The base-binary worktree, build, copy and leveldb index are removed.
  - Every `PYTHON-SUBMODULE-IMPORT-1-django-before` becomes `PYTHON-RECEIVER-BINDING-1-django-before` (command ×2, environment, inputs, PRB-C10, §1, §7).
  - PRB-C10 removes only the two builder-owned after-roots and asserts both manager roots present.
  - §5 and §7 follow.
- **No copy:** no check serves a manager root, so the DOCS-UNREADABLE-DECODE-1 OC-3 copy rule has no site here.
- **Probes (EXECUTED):**
  - PRB-C09's links before the build pass at HEAD. They fail with the base set to ef1d5300 (rc 1), and with a missing manager root (rc 1).
  - PRB-C10's two presence links pass.
  - `rg-store-diff.py` reads the manager's leveldb root (self-compare PASS, no write).
  - PRB-C09's before-store assertions (64,354 all `static`; 87,574; `test_base.py:190` `unknown`/`no_supporting_signal`) pass on the re-captured django root.
- **Approver:** PENDING — the operator. The alternative, keeping the base-binary index under a new builder-owned name and leaving the manager's leveldb root unused, is named in the author's report. **Carried as:** INPUT-2.

## OC-5 (2026-09-27, PREP-7) — literals re-measured on the re-captured roots; floors re-listed at 9df4a6ee (→ INPUT-2)

- **Method (EXECUTED, python sqlite3 `immutable=1`, WAL 0 bytes).** The django re-capture was diffed against the v0.19.0 witness store `~/repo-graph-retained/audit-v0.19.0/databases/0e968f3fb351d2f9.db`. The four sets were CALLS edges, unresolved CALLS rows, CALLS extraction edges and nodes, keyed by stable key without the repo uid, plus span, resolution or basis and classification, and metadata. Each set has 0 new-only and 0 old-only rows: 64,354 / 87,574 / 151,928 / 81,934.
- **Store counts re-taken directly, all unchanged:**
  - 64,354 CALLS, all `static`;
  - 87,574 unresolved CALLS;
  - 14,134 valid carriers `no_supporting_signal`/`unknown`, and 4 `self_call_ambiguous_mro`;
  - 36,307 carrier edges; every carrier was written by `python-core:0.1.0` (36,307 + 14,138);
  - 140,705 = 64,354 + 76,351, with 11,223 external;
  - `resolved_call_count` 64,354;
  - 271 `ListMixin.extend` edges (carriers at mutable_list.py:123/:141 and helpers.py:561/563/565; 266 not);
  - 16 carrier-less `self` calls bound (14 `WidgetTest.check_html.assertEqual`, 2 `SimpleTestCase.assertRaisesMessage`) and 12 unresolved, exact by `edge_uid` join;
  - 10,828 resolved dotted Python calls other than a two-part `self`/`cls`, none a carrier;
  - 31 to `hashed_file_path`;
  - 18,555 CALLS to `WidgetTest.check_html.assertEqual`;
  - 3,019 files.
- **Derived figures:** the hierarchy replica's 17,554 / 18,753, the pools (53,027; 5,487; 85,942 rows; 1,583,903 candidates; 117 at autodetector.py:1735) and the 50,444 / 1 receiver split are functions of those identical facts and the unchanged checkouts (django 4d455ae, leveldb 7ee830d, OBSERVED). They are therefore unchanged. The expectations of `callers ListMixin.extend` (2 + 266 + 3) and `callers WidgetTest.check_html.assertEqual` (2 + 14 + 18,539), the inferred band 10,327..10,828 and the rate band 24.7–25.1% stand.
- **MOVED:**
  - the store's byte size, 554,500,096 → 554,717,184 (§0 Changes, §3; still +8.8%);
  - the stamp (`cpp-core:0.2.0`, `indexer:1.1.0`; stated in no check).

  leveldb, re-measured: 133 files and 3,488 CALLS, all static (§7). It differs from its v0.19.0 witness only by CPP-ATTRIBUTE-MACRO-1A's shipped node and receiver-type facts, and no leveldb literal of the slice depends on them.
- **Floors** (EXECUTED `cargo test -p <crate> -- --list` at 9df4a6ee; floor = listing + new tests):

  | Crate | Listed | Floor (was) |
  |---|---|---|
  | indexer | 460 | 484 (466) |
  | repo-index | 413 | 416 (412) |
  | storage | 745 | 769 (765) |
  | agent | 475 | 489 (474; the `inputs` said 473) |
  | daemon-runtime | 946 | 959 (934) |
  | rgr | 1,955 | 1,974 (1,953) |

  Unchanged: python-extractor 63 / 89, classification 269 / 277, trust 119 / 125, enrichment 66 / 66. Every existing test bound by name in the ten crate checks exists at 9df4a6ee. The new/existing splits match the prose (for example PRB-C01 14 existing + 25 absent = 24 new + 1 rewritten).
- **Positions re-verified at 9df4a6ee:** dispatch.rs:2800 (the depth-8 literal; 10,267 lines), agent_impl.rs 2,470 lines, `build_toolchain_json` :1475. The Status line states that other line numbers are ef1d5300's.
- **Approver:** PENDING — the operator. **Carried as:** INPUT-2.

## A-1 (2026-09-27, PREP-7) — the strict certain CALLS read meets fixtures seeded outside the edge vocabulary (→ INPUT-2)

- **Raised:** the PREP-7 packet (item 4) and D-PSI-R1-VOCAB's note ("Any later strict read of the resolution column (PYTHON-RECEIVER-BINDING-1's CALLS reads in particular) will meet them; that slice's rebase allocates the corrections it needs").
- **Rule:**
  - A certain CALLS read counts `resolution` `static`/`dynamic` only. No other value is ever read as certain.
  - Every test fixture that seeds a value outside `static | dynamic | inferred` into a read this slice makes certain-only becomes `"static"`, one token per site, diff-guarded.
  - The slice states the rule once (§2.1 step 4, §2.2, PRB-C04 `inputs`). PRB-C04's parenthetical, which kept `"enriched"` fixtures "certain, as it is not `inferred`", contradicted D-PSI-R1-VOCAB and is corrected.
  - Measured: the 29 v0.19.0 witness stores hold only `static` (880,571) and `inferred` (439) CALLS, and both before-roots only `static` (EXECUTED).
- **Determination** (INFERRED from the source; a document item runs no test):
  - Nine daemon-runtime CALLS sites reach `find_symbol_callers`/`find_symbol_callees`. That is the SQLite side of the callgraph no-loss certificate (`callgraph_cert/mod.rs`), the witness ledger (`ledger.rs:946/:962`), the union serve (it consumes the ledger), the explain high-fan-in parity proof, and the explain/orient LiveGraph proofs, whose SQLite "mirrors BOTH key sets so the per-symbol no-loss key compare is GENUINELY GREEN". The ledger fixture asserts `sqlite_total == 2`. Under the certain read the SQLite side answers no row, so the compare turns red. The sites are:
    - `callgraph_cert/test_fixture.rs` :339 / :575 / :715 / :876 / :1122;
    - `callgraph_cert/ledger_tests.rs:781`;
    - `explain_serve_tests/fanin_fixture.rs:320`;
    - `explain_coherence_tests.rs:133`;
    - `orient_lg_decisions/served_e2e.rs:152`.
  - Not allocated: the OWNS sites (`test_fixture.rs:365`, `ledger_tests.rs:767`, `fanin_fixture.rs:216`, `focus_resolution_cert/test_fixture.rs:410`) and the IMPORTS site (`test_fixture.rs:382`). No read this slice changes consumes them, and no daemon-runtime test walks `path`. `focus_resolution_cert/test_fixture.rs` is not needed.
  - Same class, beyond the packet's daemon-runtime list: `storage/tests/call_aggregate_families.rs:402` seeds `"enriched"` into the live-derived fallbacks of `find_dead_nodes`/`map_resolved_dep_edges_in_path`, which `promotion_never_seeds_families_on_a_pre_migration_snapshot` asserts. PRB-C04 makes those fallbacks certain-only.
  - Kept: `call_aggregate_families.rs:337` and `enrichment_impl.rs:1203/:1452`. They read only persisted families or the generic `count_edges_by_type`.
- **Allocation:**
  - `candidatePaths` grows by four, 87 → 91: `rust/crates/daemon-runtime/src/callgraph_cert/test_fixture.rs`, `rust/crates/daemon-runtime/src/callgraph_cert/ledger_tests.rs`, `rust/crates/daemon-runtime/src/explain_serve_tests/fanin_fixture.rs`, `rust/crates/storage/tests/call_aggregate_families.rs`.
  - PRB-C07 gains a guard. The three new files may differ from HEAD by exactly their `-U0` token hunks. `explain_coherence_tests.rs` and `served_e2e.rs`, already allocated for literal updates, must hold no `"resolved"` seed and a `"static"` one.
  - PRB-C07 binds six consumers by name: `callgraph_cert_green_on_faithful_mirror`, the three ledger tests (spike, suspect, pipeline-only) and the two union-serve tests. It runs the whole daemon-runtime package, its library suite included.
  - PRB-C04 gains a guard over `@@ -402 +402 @@` and binds its consumer.
  - PRB-C10's allow-list gains the four paths.
- **Same class, found by the allow-list cross-check:** PRB-C10's allow-list lacked `rust/crates/enrichment/src/promotion.rs` and `rust/crates/storage/src/enrichment_impl.rs`, both candidate paths since revision 3. PRB-C10 would have failed on any candidate implementing PRB-C11. Rule: the allow-list equals `candidatePaths` exactly, asserted when the block was edited (91 = 91).
- **Probes** (scratch worktree at 9df4a6ee, removed; 16 cases):
  - PRB-C07's guard passes on the nine tokens, unstaged and staged. It fails on: no change; an extra OWNS token (`test_fixture.rs:365`, `ledger_tests.rs:767`); the IMPORTS token (`:382`); one site left uncorrected (`:1122`); a different value (`"dynamic"`); an extra line in `fanin_fixture.rs`; either already-allocated file left uncorrected.
  - PRB-C04's guard passes on the one token, unstaged and staged. It fails on: no change; `:337` changed too; a different value.
  - PRB-C10's regex admits all 91 paths and flags an untracked `daemon-runtime/src/lib2.rs` and `focus_resolution_cert/test_fixture.rs`.
- **Approver:** PENDING — the operator (in-place-manager). The packet authorized the daemon-runtime corrections; the storage site and the allow-list completion are the same class, carried here for confirmation. **Carried as:** INPUT-2.

- **Slice document digests (OC-2..OC-5 and A-1, one pass):** before sha256:0ca8e4cca6f3db3b2cebdd06b0646f813b261820bbe7a590ec0214f9bbde87cd; after sha256:ee41f9d0595aa1fa399d7728cb6366d04f6dd4254ac58a746662514a7404a1f6. The manifest's `allocation` digest has been re-pinned to the after value. The validator reports `ALLOCATION VALID: 12 checks`.

## OC-6 (2026-09-27, PYTHON-RECEIVER-BINDING-1-PREP-8) — PRB-C02's alias oracle predicted a result HEAD never produced (→ INPUT-3)

- **Id note:** the PREP-8 packet calls this item "OC-4" and the next "OC-5"; those ids are PREP-7's (above), so this ledger records them as OC-6 and OC-7. Any amendment would be A-2; none is made.
- **Raised by:** implementation review-0 of INPUT-2, finding F-2 (decision D-1; the operator, in-place-manager, chose option A: amend the oracle).
- **Old:** PRB-C02 `inputs` — "`Hit.f → Hit.g`, `static` (HEAD bound it through the hierarchy after the namespace stage found no `g` in `helper.py`); the alias case keeps HEAD's namespace-first result — `Aliased.caller → helper.run`, `static`, not `Aliased.run`"; the parent's `Aliased.caller → Aliased.run` called "(state i — the discriminator)". PRB-C02 `expected` — "keeps every hierarchy hit and HEAD's namespace-first result".
- **New:** HEAD's real result, `Aliased.caller → Aliased.run`, `static`, not `helper.run`; the `Hit.f` parenthetical says the namespace stage met the binding `self` and bound nothing; the parent's state-i edge is no longer called a discriminator; `expected` keeps every hierarchy hit, the alias fixture's included. The namespace-first ORDER for a legacy carrier whose alias the stage can resolve stays proven at resolver level by the bound PRB-C01 test `python_carrier_without_a_receiver_binding_from_python_core_0_1_0_meets_the_namespace_stage_before_the_hierarchy_as_at_head`.
- **Evidence (OBSERVED at 7a950321):**
  - `rust/crates/indexer/src/resolver.rs:1591` — `if !specifier.starts_with('.') { return None; }` in `resolve_import_specifier_to_file`, which the namespace stage (`resolve_call_target`, :1183-1231) needs to bind anything.
  - `rust/crates/python-extractor/src/extractor.rs` `extract_import_statement` records `import X as Y` as `ImportKind::Namespace` with the dotted module name as specifier (a Python `import` statement cannot be relative); `emit_from_import_binding` records `from … import` as `ImportKind::Named`, which the namespace stage does not match.
  - The INPUT-2 candidate's resolver fixture (`alias_fixture`, `.agent-manager/slices/PYTHON-RECEIVER-BINDING-1/candidate-admission-1.patch`) hand-builds the specifier `./helper` — which is why the unit test binds `helper.run` and the pipeline test cannot.
- **Rule (closes the class):** through the real pipeline the namespace stage binds no Python call. Every statement of a namespace-stage binding of a Python call is either a resolver-level statement about a hand-built `.`-relative binding, stated as such, or false and corrected. Grep of the whole slice document (`helper.run`, `helper as self`, `alias`, `namespace-first`, `binds first`) found and corrected, beyond PRB-C02: PRB-C01 `inputs` (its alias fixture described as a hand-built `.`-relative binding proving the order; "today's namespace-stage result on the hand-built binding"); §2.2's legacy-carrier bullet ("the namespace stage binds it `static` as at HEAD") and carrier-less bullet ("A file-level alias of that name binds it `static` there"); §3's stale-carrier residual ("a file-level alias named `self`/`cls` still binds first"; "would still bind through the alias there until that full index"). Kept, as true: the stage-order rule text of §2.1 R1(iii-a), the §2.1 precedence paragraph, P-PRB-02, PRB-C01 `expected`. Not edited: §9's revision-7 history entry ("keeps HEAD's alias-first result"), superseded by §9's INPUT-3 entry.
- **Unchanged:** check ids, obligation sets, requirement text, candidate paths, every command (PRB-C02's command binds test names only; the refresh test's name stays true).
- **Author:** requirements author of PYTHON-RECEIVER-BINDING-1-PREP-8 (claude-opus-5-5). **Approver:** in-place-manager (D-1 → A, recorded in the PREP-8 packet); the text is PENDING the operator's confirmation. **Carried as:** INPUT-3.

## OC-7 (2026-09-27, PREP-8) — PRB-C07 names the proof of review-0 F-1 (inferred rows counted twice under union serving) (→ INPUT-3)

- **Raised by:** implementation review-0 of INPUT-2, finding F-1: with `RMAP_RECON_UNION=1`, `--engine auto`, W-BOTH activation and `include_inferred`, the dispatch closures hand inferred SQLite rows to the union, `union_serve` serves them with `backend_used: "union"`, and `call_certainty::attach_remainder` appends them again because the backend is not `"sqlite"`; the witness basis also receives rows outside the certain set. A code defect the builder fixes at re-admission; the packet requires its tests to be named first.
- **Old:** PRB-C07's name loop without these tests; `-- --list` floor 959 (= 946 + 13); `expected` silent on union serving.
- **New:**
  - four names in the loop, qualified by their host: `call_certainty::tests::callers_include_inferred_under_union_serving_lists_each_inferred_row_once`, `call_certainty::tests::callees_include_inferred_under_union_serving_lists_each_inferred_row_once`, `call_certainty::tests::callers_include_inferred_on_union_fallback_lists_each_inferred_row_once`, `call_certainty::tests::callees_include_inferred_on_union_fallback_lists_each_inferred_row_once`;
  - floor 963 = 946 + 17 (EXECUTED `cargo test -p repo-graph-daemon-runtime -- --list` at the clean rust tree of 7a950321: 946, equal to 9df4a6ee's; 959 was the floor, not HEAD's count);
  - `expected` gains: under `RMAP_RECON_UNION=1` union serving and its fallback `--include-inferred` lists each inferred row exactly once, `count` equals the rows listed, and the union witness basis holds only certain rows;
  - `inputs` gains UNION SERVING: the ONE RULE (the union and its fallback receive the certain rows, so P rows and the witness ledger read the certain set; inferred rows are added once, after, whatever served), the host, the fixture approach (inferred rows added to the fixture's SQLite store in the test body; the A-1-guarded builders untouched; no process environment set), the requirement that the tests drive the `call_certainty` entry the dispatch union arm calls rather than re-assembling it, and the assertions (identities once each; `count` = rows listed; `witness_counts` sums to the union rows and no inferred row carries `witness`); §2.1 step 6 gains the rule.
- **Host decision (no amendment):** W-BOTH activation needs a resident LiveGraph and a classified witness ledger at the request fingerprint (`callgraph_union_eligibility`, callgraph_cert/mod.rs:488-499). Only `callgraph_cert::test_fixture` builds that, and it is `#[cfg(test)] pub(crate)` (callgraph_cert/mod.rs:81-82) — reachable from any unit-test module in the crate, unreachable from `tests/callers_inferred.rs` (an integration test over the public API; the real LiveGraph is fed by a SCIP preload, `livegraph_feed::preload_partition`). `union_serve/tests.rs`, where the existing union tests live, is not a candidate path. `call_certainty.rs` is a candidate path (a new file) and holds the composition (`attach_remainder`), so it hosts a `#[cfg(test)] mod tests`. OBSERVED at 7a950321; feasibility of the four tests INFERRED from the source, not executed.
- **Probes (EXECUTED):** `bash -n` on the changed command: ok. The name loop, extracted from the command, run against a synthetic `cargo test` output: all 31 names present → rc 0; `…callees_include_inferred_on_union_fallback…` missing → rc 1 (`MISSING …`); the four new names under `union_serve::tests::` instead of `call_certainty::tests::` → rc 1.
- **Noted for the reviewer:** union serving's §5.2 contract states `witness_counts` 1:1 with the answer's row multiset. With `include_inferred` the answer's rows exceed the union rows by the appended inferred rows, which carry no witness and are not counted `unmeasured`; the oracle states this ("the union witness basis holds only certain rows", per the packet). Flag-gated and non-default.
- **Unchanged:** check ids, obligation sets, requirement text, candidate paths (91), every other name and guard of PRB-C07.
- **Author:** requirements author of PREP-8 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-3.

- **Slice document digests (OC-6, OC-7, Status line and §9 entry, one pass):** before sha256:d84b9a2b32678a48ca8ed0a17a89a4310b7ade144b9dcc332ef8eed0d6bd8cba (the manager's INPUT-3 re-pin); after sha256:246ee43ca6bf2de7abbdc31c7cb93fc45aa7929121e30983d269d1ff00fb53ef. The INPUT-3 manifest's `allocation` digest is re-pinned to the after value.

## OC-7 revision 2 (2026-09-27, PREP-8 cycle 2) — PRB-C07's union proof rewritten under D-PRB-UNION-ROUTE-1 (→ INPUT-3)

- **Raised by:** document review-0 of PREP-8 (`.agent-manager/slices/PYTHON-RECEIVER-BINDING-1-PREP-8/review-0.json`). OC-6 was supported. OC-7 was decision-required (D-PRB-UNION-COUNT): its oracle appended inferred rows to a union answer while `witness_counts` covered only the certain rows, contradicting `docs/slices/recon-design-1.md` §5.2 (`witness_counts` one-to-one with the answer's rows, `count = rows.len()`). Finding F-ROUTING: the cycle-1 tests entered below `dispatch.rs:1295–1303`, where `RMAP_RECON_UNION` is read, so they could not prove flag-on routing.
- **Resolved by:** the operator, option D, recorded as `docs/assurance/RG-BOOTSTRAP/decisions/D-PRB-UNION-ROUTE-1.md` (the human may override); the manager added it to the INPUT-3 manifest (governance dependency, `requiredDecisionIds`).
- **Supersedes:** OC-7 above (its four test names, its floor 963, its `expected` clause, its UNION SERVING `inputs` paragraph and the "Noted for the reviewer" item, which described the contradiction the decision removes). The published OC-7 text is not edited. None of the four cycle-1 tests was written.
- **New (PRB-C07):**
  - **Routing.** The union arm is selected only when the flag is on, the engine is `Auto` and `include_inferred` is false. Each arm reads `include_inferred` first and computes `union_serving` as `call_certainty::union_arm_selected(union_serve::union_serving_enabled(), engine, include_inferred)`. The new pure predicate lives in `call_certainty.rs` rather than dispatch.rs, so dispatch.rs keeps its 60-line cap: the candidate used 42, and this adds about 10 (INFERRED). An inferred-inclusive request with the flag on is served by the existing `Auto` route.
  - **Certain rows into every comparison.** Every fetch closure handed to the engine or union responses returns certain rows only. The union's P rows, its witness ledger and `Compare`'s `compare_keys` (livegraph_feed.rs:611 at 7a950321) therefore see no inferred row. `call_certainty` adds the inferred rows once, after, whatever backend served the answer, and restates `count`. A union answer keeps §5.2 unchanged.
  - **Seven tests, all `call_certainty::tests::`:**
    - `union_arm_is_selected_only_with_the_flag_on_the_auto_engine_and_without_include_inferred` — the predicate's truth table: flag on/off × four engines × `include_inferred` false/true, true in exactly one cell;
    - `callers_/callees_include_inferred_on_the_livegraph_served_auto_route_lists_each_inferred_row_once`;
    - `callers_/callees_compare_engine_compares_only_certain_rows_with_include_inferred`;
    - `callers_/callees_union_answer_holds_no_inferred_row_and_its_witness_counts_match_its_rows`.

    The SQLite-served `Auto` route stays with the existing new `callers_include_inferred_lists_each_inferred_row_with_its_basis` (tests/callers_inferred.rs), whose oracle now also asserts each inferred row listed once and `count` equal to the rows listed.
  - **Floor:** 966 = 946 + 20.
  - **Union-route guard, in the command's Python block.** Each of `handle_callers` and `handle_callees` reads `include_inferred` and then computes `union_serving` through exactly one `union_arm_selected(union_serving_enabled(), engine, include_inferred)` call; `crate::` prefixes are optional and whitespace is ignored. dispatch.rs calls `union_serving_enabled()` exactly twice (2 at 7a950321, OBSERVED at :1301/:1480). So the flag reaches the arms, the epoch capture (:1311/:1489) and the served arm (:1388/:1563) only through the tested predicate.
  - **`expected`:** the clause follows D-PRB-UNION-ROUTE-1.
  - **Prose:** §2.1 step 6's bullet is rewritten, and §0 lists the decision.
- **Host (unchanged reasoning):** `callgraph_cert::test_fixture` is `#[cfg(test)] pub(crate)` (callgraph_cert/mod.rs:81-82). It is reachable from a unit-test module in `call_certainty.rs`, an allocated new file, and unreachable from `tests/callers_inferred.rs`. `union_serve/tests.rs` is not allocated. No amendment.
- **Verification limit, stated in `inputs`:** no test drives a live flag-on request through `ServiceDispatcher`. The flag is a process-wide environment read, and setting it in a parallel test binary would change every concurrent test's `Auto` arm. The source guard binds both arms to the tested predicate instead.
- **Probes (EXECUTED, scratch `/private/tmp/PYTHON-RECEIVER-BINDING-1-PREP-8-probe`, removed):**
  - `bash -n` on the command: ok.
  - Name loop over 34 names: all present → rc 0. Missing `callees_union_answer…` → rc 1. Missing the predicate test → rc 1.
  - The union-route guard, run on dispatch.rs variants, passes on:
    - the correct shape, both arms;
    - the imported short form.

    It fails on:
    - HEAD 7a950321;
    - the predicate called with `false` for `include_inferred`;
    - `include_inferred` read after the selection;
    - only `handle_callers` changed;
    - a third `union_serving_enabled()` call.
- **Unchanged:** check ids, obligation sets, requirement text, candidate paths (91), every other PRB-C07 name and guard.
- **Author:** requirements author of PREP-8 cycle 2 (claude-opus-5-5). **Approver:** the operator's decision D-PRB-UNION-ROUTE-1; this text is PENDING the operator's confirmation. **Carried as:** INPUT-3.
- **Slice document digests (cycle 2):** before sha256:246ee43ca6bf2de7abbdc31c7cb93fc45aa7929121e30983d269d1ff00fb53ef; after sha256:9400933652495542a270b4ec92c798bf28b1ae02f4be954f369ae0839ce24343. The INPUT-3 manifest's `allocation` digest is re-pinned to the after value.

## OC-7 revision 3 (2026-09-27, PREP-8 cycle 3) — PRB-C07's union-route guard follows the predicate into both branches (→ INPUT-3)

- **Raised by:** document review-1 of PREP-8 (`.agent-manager/slices/PYTHON-RECEIVER-BINDING-1-PREP-8/review-1.json`), finding F-ROUTING. The revision-2 guard checked only that each handler assigns `union_serving` from `union_arm_selected`. It did not inspect the epoch-capture branch or the serving branch. A candidate with the correct predicate and `if !union_serving` branches would have passed while routing real requests wrongly.
- **Old:** the revision-2 union-route guard in PRB-C07's command (flag read twice in dispatch.rs; each handler reads `include_inferred`, then binds `union_serving` through the predicate).
- **New (guard extended; everything in revision 2 kept):** in each of `handle_callers` and `handle_callees`, line comments stripped and whitespace removed:
  - `union_serving` is bound exactly once (`union_serving=` occurs once) and never negated (`!union_serving` is absent);
  - exactly two `if union_serving { … } else { … }` branches exist, braces matched, as at HEAD (:1311/:1388 and :1489/:1563 at 7a950321, OBSERVED);
  - the first is the epoch capture: its union branch calls `callgraph_union_eligibility(` and not `callgraph_cert_eligibility(`, and its other branch the reverse;
  - the second is the serving call: its union branch calls `<side>_union_response(` or `<side>_union_answer(`, and not `<side>_engine_response(`/`<side>_engine_answer(`; its other branch the reverse. `<side>` is `callers` or `callees`.

  PRB-C07 `inputs` states this. The union-or-Auto choice therefore stays in each arm, and any `call_certainty` entries the behaviour tests drive are per route (`<side>_union_answer`, `<side>_engine_answer`).
- **Seam choice:** the guard, not a runtime seam. Reaching the arms at run time needs the process-wide `RMAP_RECON_UNION`; the guard needs no process environment.
- **Probes (EXECUTED; scratch `/private/tmp/PYTHON-RECEIVER-BINDING-1-PREP-8-probe`, removed).** The guard block was extracted from the command and run on dispatch.rs variants derived from 7a950321.
  - It passes on:
    - the predicate edit with HEAD's branches;
    - the same with `call_certainty::<side>_union_answer`/`_engine_answer` entries.
  - It fails on each of these:
    - HEAD;
    - `handle_callees`' epoch capture as `if !union_serving`;
    - `handle_callers`' serving call as `if !union_serving`;
    - `handle_callers`' epoch branches swapped;
    - `handle_callees`' serving branches swapped;
    - `handle_callers` rebinding `union_serving`;
    - `handle_callees`' serving branch replaced by `if true`.
  - `bash -n` on the command: ok.
- **Unchanged:** the seven test names, floor 966, `expected`, check ids, obligation sets, requirement text, candidate paths (91).
- **Author:** requirements author of PREP-8 cycle 3 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-3.
- **Slice document digests (cycle 3):** before sha256:9400933652495542a270b4ec92c798bf28b1ae02f4be954f369ae0839ce24343; after sha256:6dbb7dd12d845b28bf9fe37264ab52a095c3bae069f2049bae5aa69184967f86. The INPUT-3 manifest's `allocation` digest is re-pinned to the after value.

## Operator confirmation (2026-09-27, in-place-manager) — OC-6 and OC-7 (revision 3)

OC-6 and OC-7 revision 3 are approved; their "Approver: PENDING" lines above are resolved by this entry. OC-6: the alias-case expectation was the manager's wrong prediction. `resolver.rs:1591` at 7a950321 refuses non-relative specifiers; the builder, implementation review-0 and PREP-8 review-0 found this independently. OC-7: carried under D-PRB-UNION-ROUTE-1 (operator; the human may override). Accepted by PREP-8 review-2 (codex gpt-6-sol). Carried as INPUT-3; records in `docs/assurance/PYTHON-RECEIVER-BINDING-1-INPUT-3/`.

## Carry (2026-09-27, PYTHON-RECEIVER-BINDING-1-PREP-9) — OC-6 and OC-7 (revision 3) carried as INPUT-4; no oracle changes

- **Reason:** a record defect. INPUT-3's committed approval (`docs/assurance/PYTHON-RECEIVER-BINDING-1-INPUT-3/baseline-approval.json`, commit 7dbdecaf) resolves only `D-PRB-UNION-ROUTE-1`. The INPUT-3 manifest requires nine decisions: the eight carried from INPUT-2 and that one. Admission therefore refuses INPUT-3. Published records are append-only, so INPUT-3's review and approval stay as published and cannot be used. They are not edited.
- **Carried:** OC-6 and OC-7 revision 3, as the operator confirmation above approved them, are carried unchanged as `docs/requirements/baselines/PYTHON-RECEIVER-BINDING-1-INPUT-4.json`. The entries above that say "→ INPUT-3" or "Carried as: INPUT-3" are history. This entry supersedes them for the carry and does not edit them.
- **Changed:** nothing in any oracle, check, obligation, candidate path or decision. INPUT-4 differs from INPUT-3 only in `baselineId` and the `allocation` digest. The slice document differs from 7dbdecaf only in the block's `baselinePath`, the Status line (revision 10) and its §9 INPUT-4 entry.
- **Author:** requirements author of PREP-9 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager).
- **Slice document digests (PREP-9):** before (7dbdecaf) sha256:6dbb7dd12d845b28bf9fe37264ab52a095c3bae069f2049bae5aa69184967f86; after sha256:43c42ad3e0d4f023defc01f8dc775b8b465970b6346f866ef78ccdeda4c1abd4. The INPUT-4 manifest's `allocation` digest is re-pinned to the after value.

## Closeout (2026-09-27, in-place-manager)

Shipped as `dac37a98` under INPUT-4. The implementation review's acceptance (admission 3, review-2) covers OC-1 … OC-7 as carried by INPUT-2 … INPUT-4. No correction changed behaviour.
