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
