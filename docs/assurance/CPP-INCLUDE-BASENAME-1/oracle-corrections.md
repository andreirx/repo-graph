# CPP-INCLUDE-BASENAME-1 — oracle corrections and rebase records (append-only ledger)

Rules for this file: append only. An entry is never edited or deleted. A later understanding is a new, dated entry that references the earlier one. Each entry names the baseline it enters, its authority, the historical text it supersedes (kept), the correction, and the evidence for the correction.

Subject: the allocation block and the prose of `docs/slices/cpp-include-basename-1.md`. Admitted baseline before these entries: `docs/requirements/baselines/CPP-INCLUDE-BASENAME-1-INPUT-1.json` (accepted at 9755ba47, 2026-09-24). Baseline these entries enter: `docs/requirements/baselines/CPP-INCLUDE-BASENAME-1-INPUT-2.json` (RG-BOOTSTRAP-INPUT-11; the manager added D-STALE-SIGNAL-1, D-PSI-R1-VOCAB and D-TESB-READERS-1 to its governance and `requiredDecisionIds`).

Trigger: the rebase packet `.agent-manager/slices/CPP-INCLUDE-BASENAME-1-PREP-4/selection.md` (HEAD cea4e2ee). Since 9755ba47 the bootstrap baseline moved to RG-BOOTSTRAP-INPUT-11 and nine slices shipped (TOOLCHAIN-STALENESS-1 23d12a9b, CPP-ATTRIBUTE-MACRO-1A ddee0369, PYTHON-SUBMODULE-IMPORT-1 5a1b603f, DOCS-UNREADABLE-DECODE-1 833e67b2, PYTHON-RECEIVER-BINDING-1 dac37a98, PORTABLE-TMP-1 b8551d45, DEPS-GRADLE-CATALOG-1A 24ca42ac, TEST-EDGE-SCOPE-1A df98b655, TEST-EDGE-SCOPE-1B f0693e28); `git diff --stat 9755ba47 HEAD -- rust` = 214 files, +40,333/−2,686. Author of every entry below: requirements author of document item CPP-INCLUDE-BASENAME-1-PREP-4 (claude-opus-5-5). Approver of every entry: PENDING — the operator (in-place-manager) confirms or rejects with the review of this item. Independent review of these entries: pending (the PREP-4 reviewer).

Unchanged by every entry below: the implements / changes / preserves sets, P-CIB-01…P-CIB-05, the nine check ids and each check's `obligationIds`, every requirement text. The candidate paths change once (OC-4: eleven → ten).

Slice document digests: before these entries `sha256:ecad1149cba9be24563566895e5d8e459247af0c38b5b6a7b5a3fee22c3d777e` (the INPUT-2 placeholder pin); after: the value INPUT-2 pins for `docs/slices/cpp-include-basename-1.md`.

---

## OC-1 — 2026-10-02 — every isolated rmap invocation sets `RMAP_AUTO_REINDEX=off`

- **Authority.** D-STALE-SIGNAL-1, operator consequence: "once TOOLCHAIN-STALENESS-1 ships, every isolated rmap in a slice proof sets `RMAP_AUTO_REINDEX=off` beside `RMAP_AUTO_ENRICH=off`". The PREP-4 packet, task 1.
- **Historical text, kept.** CIB-C05 ran each `rmap index` with `RMAP_STATE_ROOT=… RMAP_SOCKET_PATH=… RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`; CIB-C09 ran each capture with the same five variables. Neither set `RMAP_AUTO_REINDEX`.
- **Rule (closes the class).** Each check that runs rmap does so through ONE helper that carries the full set — `RMAP_STATE_ROOT`, `RMAP_SOCKET_PATH`, `RMAP_TRANSPORT=stdio`, `RMAP_AUTO_ENRICH=off`, `RMAP_AUTO_REINDEX=off`, `RMAP_AUTO_RETENTION=off` — and calls the binary by its path: `idx()` in CIB-C05, `q()` in CIB-C09. No check invokes rmap outside its helper (EXECUTED: a scan of the nine commands' shell text finds no other `rmap` call).
- **Why it matters here.** This slice bumps `INDEXER_VERSION`. CIB-C09 now serves a copy of the nginx before-root (stamped `indexer:1.4.0`) with the candidate binary (OC-6). Under RG-REQ-001-L06 a daemon whose running version differs from the stamp schedules a background full re-index on the first request, which would turn the before-capture into an after-capture.
- **Precision (OBSERVED, `daemon-runtime/src/auto_reindex.rs:248-262` at cea4e2ee).** A stdio daemon already schedules no re-index (`mark_stdio_transport`; reason "stdio transport — a background re-index cannot outlive the request"). So under `RMAP_TRANSPORT=stdio` the opt-out is a second, independent guard. It stays required: it is the operator's standing rule, and it holds if a check is ever run over the socket transport. CIB-C09 asserts the outcome, whatever the mechanism: after the before-captures, the served copy still holds exactly its one captured ready snapshot (the uid in the manager's `registry.json`).
- **Sites.** CIB-C05 `command`, `environment`; CIB-C09 `command`, `environment`; §2.3; §4.
- **Probe (EXECUTED 2026-10-02).** HEAD's release binary (`rmap 0.19.0`, `rust/target/release`) served a builder-owned copy of the nginx before-root (`/private/tmp/CPP-INCLUDE-BASENAME-1-PREP-4-nginx-basecopy`, registry redirected) with the full set for eight commands (`imports` ×3, `modules deps` ×3, `modules list --json`, `trust --json`), each exit 0. Afterwards the copy held `[('repo_01m3yvfx2xfnabjbbk5642crp1/2026-10-02T17:44:46.634Z/6c6d5ed8', 'ready')]`, equal to the manager registry's `last_snapshot_uid`. Limit: the binary is HEAD's (same version as the stamp), so this probe shows the assertion's form and data, not the opt-out's effect under a version difference.

## OC-2 — 2026-10-02 — the before side is the manager's CIB-owned roots, served only as builder-owned copies

- **Authority.** The PREP-4 packet, task 2: "Repoint every root reference to the CIB-owned roots above. A root's digest must be asserted unchanged after each check that serves a copy of it." D-TESB-READERS-1, F-TESB-FIELD: "a stored-fact identity oracle compares against a baseline indexed by the SAME producer. The operator re-captures each before-root with HEAD's binary after any index-time slice lands". CLAUDE.md, Relay Builder Rules: "Never index a large repository twice for a before/after."
- **Historical text, kept.**
  - CIB-C05 built a before-binary from `git worktree add --detach /private/tmp/CPP-INCLUDE-BASENAME-1-headbase-wt HEAD` (target `/private/tmp/CPP-INCLUDE-BASENAME-1-headbase-target`) and indexed nginx, leveldb and poco twice — into `/private/tmp/CPP-INCLUDE-BASENAME-1-{nginx,leveldb,poco}-headbase` and `-after`. Its stated reason: the manager's v0.19.0 stores predated queued slices that move other facts of these corpora.
  - CIB-C06 removed that worktree and asserted `test -d` on `/private/tmp/CPP-INCLUDE-BASENAME-1-nginx-before`, `/private/tmp/CPP-ATTRIBUTE-MACRO-1-leveldb-before` and `/private/tmp/DOCS-UNREADABLE-DECODE-1-poco-before`.
  - §7 named those three roots and the trust-movement stores `/private/tmp/DEPS-GRADLE-CATALOG-1-{kafka,grpc-java,petclinic}-before`, `/private/tmp/TS-WORKSPACE-RESOLUTION-1-glamcrm-before`.
- **Evidence (OBSERVED 2026-10-02).**
  - The two borrowed roots and the trust-movement stores no longer exist (`ls /private/tmp/DEPS-GRADLE-CATALOG-1-*`: no match). The old CIB-C06 could not pass.
  - The manager's roots exist: `/private/tmp/CPP-INCLUDE-BASENAME-1-{nginx,leveldb,poco}-before`. Each holds `databases/<hex>.db` (0-byte `-wal`), `registry.json` (one repo, `db_path` inside the root), `index.out`, and `source.txt` whose first line is `nginx b0a4d0fb823b dirty=3 captured=2026-10-02T17:44:45Z producer=rmap 0.19.0`, `leveldb 7ee830d02b62 dirty=0 captured=2026-10-02T17:44:50Z producer=rmap 0.19.0`, `poco 49af4000f99f dirty=0 captured=2026-10-02T17:44:51Z producer=rmap 0.19.0`.
  - Each store has ONE ready `full` snapshot stamped `{"extractors":["ts-core:0.2.0","c-core:0.1.0","cpp-core:0.2.0","java-core:0.1.0","python-core:0.2.0","rust-core:0.2.0"],"indexer":"indexer:1.4.0"}` — HEAD's `INDEXER_VERSION` (`orchestrator.rs:66`). `git diff f0693e28 cea4e2ee -- rust` is empty.
  - The checkouts are at the recorded commits; leveldb and poco are clean; nginx has 3 porcelain entries.
  - Digest of the three roots (`find … -type f | sort | xargs shasum -a 256 | shasum -a 256`): `ef3d2b96fa2a5d1c3b905ea61c0ad1a382d4cf6064a2b46d42882dccc2093f59` at the start of this item and again after every probe below.
- **Correction.**
  - CIB-C05: the base is three builder-owned COPIES `/private/tmp/CPP-INCLUDE-BASENAME-1-{nginx,leveldb,poco}-basecopy` (the `.db` copied, the one-entry registry's `db_path` redirected and asserted — the TEST-EDGE-SCOPE-1B `copy()` form). The worktree, the before-binary and the three base indexes are removed: each checkout is indexed ONCE, by the candidate, into `-after`.
  - CIB-C05 preconditions, fail-closed: the three roots exist; their `source.txt` states match with the documented suffix (the TEST-EDGE-SCOPE-1B OC-5 form); the roots' digest equals the `roots-before` line of build-progress.md; the checkouts are at their commits, leveldb and poco clean; `git diff --quiet cea4e2ee HEAD -- rust` (the producer's rust tree is HEAD's); and, in the query, each base store's stamp names HEAD's `INDEXER_VERSION`. Any failure is a STOP (§4): the operator re-captures; the builder never indexes a before-store.
  - The roots' digest is asserted after CIB-C05 (copies made), after CIB-C09 (a copy served) and in CIB-C06.
  - §5 step 0 records `roots-before <sha256>` beside `registry-before`.
  - CIB-C06 removes the copies and the after-roots, asserts the three CIB roots exist and the digest, and no longer removes a worktree (none is created); its Python asserts that no worktree of this slice remains.
  - §7 names the CIB roots, the durable copy `~/repo-graph-retained/CPP-INCLUDE-BASENAME-1-before/`, and the copies.
- **Why the base is no longer a HEAD build.** The historical reason (v0.19.0 stores predating queued slices) no longer holds: the roots were produced by HEAD's binary from the same checkouts. Probe evidence that a copy is an identity baseline: `rg-store-diff.py` of the leveldb copy against itself — `files`, `nodes`, `edges`, `unresolved` all `total_differing=0`, `RG-STORE-DIFF PASS`.
- **Probes (EXECUTED 2026-10-02; the guard text extracted verbatim from the allocation, `BP` pointed at a probe file).**

  | Probe | Expected | Actual |
  |---|---|---|
  | CIB-C05 precondition chain, real roots and checkouts, `roots-before` = the real digest | exit 0 | exit 0 |
  | the same, `roots-before` = 64 zeros | exit ≠ 0 | exit 1 |
  | the same, no `roots-before` line | exit ≠ 0 | exit 1 |
  | `source.txt` grep: real nginx line / no suffix / commit `…823c` / `dirty=30` / suffix ` garbage` / `dirty=2` | match / match / no / no / no / no | rc 0 / 0 / 1 / 1 / 1 / 1 |
  | `git diff --quiet cea4e2ee HEAD -- rust` / `git diff --quiet 9755ba47 HEAD -- rust` | 0 / ≠ 0 | 0 / 1 |
  | `copy()` (destination renamed to a `CPP-INCLUDE-BASENAME-1-PREP-4-*` probe path) for leveldb and nginx | copies made, `db_path` redirected | done; redirected `db_path` OBSERVED |
  | `copy()` again into an existing destination | exit ≠ 0 | exit 1 |
  | CIB-C05 query, leveldb copy against itself (`want {}`) | pass | `leveldb moved rows by basis {} C/C++ unresolved IMPORTS 280 -> 280`, exit 0 |
  | CIB-C05 query, nginx copy against itself (no candidate) | fail | `AssertionError: ('nginx', 'moved rows by basis', {}, {'unique_basename': 791, 'ambiguous_basename': 13})` |

## OC-3 — 2026-10-02 — the staleness narrative is one statement

- **Authority.** D-STALE-SIGNAL-1 (the human, 2026-09-26). The accepted model: CPP-ATTRIBUTE-MACRO-1A OC-2/OC-3 and PYTHON-RECEIVER-BINDING-1 OC-2. The PREP-4 packet, task 3. Reviewer scope: staleness precision is out of scope by the human's ruling.
- **Historical text, kept.**
  - §0 Reported consequences: "a store indexed before this slice reports the `indexer` family stale on `orient`/`check` until `rmap repo rebuild <path>`; a REFRESH by the new binary already re-resolves every include of the snapshot (§3), yet the line stays after it because the refresh's `carried_forward` retains the older indexer version whenever a file is copied (RG-REQ-001-L06, conservative by ratification) — it clears on rebuild."
  - §2.1 item 3: "What the refresh does NOT clear is the staleness line (§0)."
  - §2.1 item 7: "The staleness line it triggers is TOOLCHAIN-STALENESS-1's (§0)."
  - §6 ship block: "existing stores report the `indexer` family stale on `orient`/`check` until `rmap repo rebuild <path>` (TOOLCHAIN-STALENESS-1), even after a refresh that has already re-resolved their includes."
  - CIB-C01 `inputs`: "D-CIB-L06-1 = B: a reported consequence, TOOLCHAIN-STALENESS-1 owns RG-REQ-001-L06".
- **New.** One statement, wherever the bump is mentioned: this slice owns the `INDEXER_VERSION` bump; what an existing store then shows and does is TOOLCHAIN-STALENESS-1's under RG-REQ-001-L06 and is not restated (§0 Reported consequences, §2.1 item 7, §6, CIB-C01 `inputs`). The §2.1 item 3 sentence is removed. §0's decision records name D-STALE-SIGNAL-1.
- **Superseded-signal sweep (EXECUTED, python over the nine check commands and the prose before §9).** No check asserts `facts from … to refresh`, `TOOLCHAIN_STALENESS`, an exit 2, a `families` key, "stale until rebuild" or `carried_forward`. After the edit the prose holds none of them; "rebuild" remains only as the re-index remedy in the reason-missing row (§2.1 item 6) and in §3's withdrawn-sentence record.

## OC-4 — 2026-10-02 — CIB-C01's version oracle measured at cea4e2ee; `compose.rs` leaves the candidate paths

- **Authority.** The PREP-4 packet, task 4. The accepted form: PYTHON-RECEIVER-BINDING-1 OC-3 (PSI-C01/PRB-C01) and DEPS-GRADLE-CATALOG-1A OC-3.
- **Historical text, kept.**
  - CIB-C01 matched `^const INDEXER_VERSION: &str = "…";$` (exactly one).
  - It required orchestrator.rs to differ from HEAD by exactly its version line, and compose.rs by exactly its `let extractor = "<v>"; // Match INDEXER_VERSION in orchestrator.rs` line.
  - It required the old literal to leave "exactly the two declared sites".
  - `candidatePaths` listed `rust/crates/repo-index/src/compose.rs`; CIB-C06 required it in the tree's exact change set (eleven paths).
- **Evidence (OBSERVED at cea4e2ee).**
  - `rust/crates/indexer/src/orchestrator.rs:66` `pub const INDEXER_VERSION: &str = "indexer:1.4.0";`.
  - `:4383`, `:4386` and `:4391` pin that literal inside `build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version` (added by TOOLCHAIN-STALENESS-1, 23d12a9b).
  - `rust/crates/repo-index/src/compose.rs:1648` `let extractor = orchestrator::INDEXER_VERSION;` (no literal copy since 23d12a9b).
  - `git grep -F 'indexer:1.4.0' HEAD -- '*.rs'` finds only those four orchestrator lines; `indexer:1.5.0` is found nowhere.
  - The old oracle could not pass on any correct candidate (no `^const` declaration; no compose.rs line to move; the pinning test must change).
- **New.**
  - CIB-C01 requires `^pub const INDEXER_VERSION` (exactly one), one minor above HEAD's.
  - orchestrator.rs equals HEAD's text with the old literal replaced by the new one on every line that holds it, and nothing else (`oc == oh.replace(H, C)`; the TEST-EDGE-SCOPE-1A TESA-C01 form; stricter than PRB-C01, because this slice changes nothing else in the orchestrator).
  - compose.rs is unchanged and reads the constant by name.
  - The old literal moves nowhere else; the new literal is added nowhere outside the orchestrator.
  - `build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version` is bound by name; its body changes with the literal, so it is not in the unchanged-body list.
  - `rust/crates/repo-index/src/compose.rs` leaves `candidatePaths` (eleven → ten); CIB-C06's exact list, P-CIB-03 and §4 say ten. repo-index stays a touched crate (`tests/cpp_include_roots.rs`), so CIB-C03 and CIB-C04 are unchanged.
- **Probes (EXECUTED 2026-10-02; scratch worktree `/private/tmp/CPP-INCLUDE-BASENAME-1-PREP-4-wt` at HEAD, removed; the version part of the CIB-C01 Python run verbatim from `rust/`).**

  | Probe | Expected | Actual |
  |---|---|---|
  | a. HEAD unchanged | fail | `("INDEXER_VERSION moves up exactly one minor from HEAD's", 'indexer:1.4.0', 'indexer:1.4.0')` |
  | b. `indexer:1.4.0` → `indexer:1.5.0` on every line of orchestrator.rs | pass | `CIB-C01 source assertions ok: indexer:1.4.0 -> indexer:1.5.0` |
  | c. the declaration only (the pinning test unchanged) | fail | `('orchestrator.rs differs from HEAD by exactly the version literal, …` |
  | d. b + compose.rs back to a literal copy | fail | `('compose.rs reads INDEXER_VERSION by name and is unchanged', …` |
  | e. b + `// indexer:1.5.0` appended to resolver.rs | fail | `('the new literal is added nowhere outside the orchestrator', {}, {…'crates/indexer/src/resolver.rs': 1})` |
  | f. `indexer:1.6.0` everywhere | fail | `("INDEXER_VERSION moves up exactly one minor from HEAD's", 'indexer:1.4.0', 'indexer:1.6.0')` |
  | g. b + one more orchestrator line | fail | `('orchestrator.rs differs from HEAD by exactly the version literal, …` |
  | CIB-C06 Python, ten allocated paths modified in the scratch worktree | path assertion passes | passed it; the next assertion failed because the probe ran inside the scratch worktree `CPP-INCLUDE-BASENAME-1-PREP-4-wt` ("no worktree of this slice remains") — the correct verdict for that environment |
  | the same + compose.rs modified | fail | `('the tree holds exactly the ten candidate paths', …` |

## OC-5 — 2026-10-02 — literals, floors and line citations re-measured at cea4e2ee

- **Authority.** The PREP-4 packet, task 4. Method: python sqlite3 `immutable=1` on the manager's roots (empty WALs; never served); `cargo test -p <crate> [--lib] -- --list` at HEAD (EXECUTED; no other cargo); source reads at cea4e2ee. The rule is recomputed from each store's own file list with the CIB-C05 `rule()` function, a candidate binding only when its one file has a FILE node (§2.1 item 2).
- **Unchanged (re-verified).**
  - nginx: 1,090 C/C++ unresolved IMPORTS, all `imports_file_not_found`; 791 unique basename, 13 ambiguous basename, 286 no match (156 single-segment + 130 multi-segment); 355 static C/C++ edges, all with a non-NULL carrier; 0 inferred; 17 MODULE nodes, 0 MODULE→MODULE edges; 6 module candidates.
  - nginx module pairs under `--include-inferred`: the eleven of §1, 667 imports (no nginx file is `is_test`).
  - nginx trust 299 = 286 + 13; the witnesses `src/core/ngx_config.h:22,26,30,34,38,42` (unique), `:12` (none), `src/core/ngx_core.h:52` (ambiguous, two candidates), `src/event/ngx_event.c:8,9` (unique) and `:10` (static).
  - leveldb: 280 C/C++ unresolved, all no match; 498 static, 0 inferred; identity.
  - poco: 69 unique suffix; 16 new MODULE→MODULE pairs including the `cpptrace/src/platform ↔ utils` cycle; 492 existing pairs, all `is_type_only` 0; 368 `imports_ambiguous_match`; 0 inferred C/C++ edges; 0 NULL carriers; no module-candidate pair.
  - kafka +84 and grpc-java +8 (re-measured on `~/repo-graph-retained/TEST-EDGE-SCOPE-1B-before/{kafka,grpc-java}-before`, `indexer:1.4.0`, read `immutable=1`, never served).
- **Corrected.**

  | Item | Old | New | Evidence |
  |---|---|---|---|
  | poco unique basename (moved, inferred) | 65 | 64 | the 65th, `dependencies/utf8proc/src/utf8proc.c:53` `#include "utf8proc_data.c"`, names `dependencies/utf8proc/src/utf8proc_data.c`, which is in `files` but has no FILE node (likewise `dependencies/sqlite3/src/sqlite3.c`); §2.1 item 2 keeps such a row unresolved |
  | poco `rg-store-diff` | `--expect edges:150 --expect unresolved:134` | `--expect edges:149 --expect unresolved:133` | 69 + 64 + 16; 69 + 64 |
  | CIB-C05 `want` for poco | `{"unique_suffix": 69, "unique_basename": 65}` | `{"unique_suffix": 69, "unique_basename": 64}`, plus an assertion that the utf8proc row and the dot-dot row `dependencies/pcre2/src/pcre2_jit_compile.c:81` `#include "../deps/sljit/sljit_src/sljitLir.c"` are unchanged | — |
  | poco C/C++ unresolved after | 3,667 | 3,668 | 3,801 − 133 |
  | poco trust figure after | 3,676 | 3,677 | 3,442 − 133 + 368 |
  | poco static rows | "vendored `dependencies/cpptrace/src/**`" | 64 under `dependencies/cpptrace/src/**`, 5 under `Data/SQLParser/{test,benchmark}` | — |
  | nginx file count | 397 | 397 (396 with a FILE node; `compile_commands.json` is inventoried without one) | `files` ⋈ `file_versions` vs FILE nodes |
  | floors | indexer 457, classification 269, repo-index 413, trust 120, storage 748, rgr `--lib` 1277, daemon-runtime `--lib` 786 | 504 = 489 + 15, 315, 461 = 457 + 4, 129 = 128 + 1, 812 = 805 + 7, 1440 = 1431 + 9, 877 | HEAD `-- --list`: 489, 315, 457, 128, 805, 1431, 877 |

- **Bound existing tests (EXECUTED).** Every existing test name the checks bind is in HEAD's `--list` output. Every unchanged-body helper finds exactly one body at HEAD: include_resolver 33, resolver 10, cpp_include_roots 2; every test of trust `rules.rs` (51), `service.rs` (38), storage `queries.rs` (84), rgr `imports.rs` (17) and `livegraph_feed.rs` (41). No new test name collides with an existing one. `sum_unresolved_imports_picks_imports_family` exists at HEAD; `service.rs` has one `#[cfg(test)]\nmod tests {`.
- **Line citations moved (old → new, cea4e2ee).**
  - resolver.rs: `is_c_family_extractor` :61-63 → :95-97 (gate block :49-63 → :83-97). Imports arm :514-577 → `resolve_target` :657-748 (arm :663-677) + `resolve_import_ladder` :755-821. Ambiguous early return :531-534 → :775-778. `is_system` :520 → :765. Stages 1-4 :661-724 → `resolve_import_target` :833-896. `JavaSuffixIndex` :271-289 → type :375 + `build_java_suffix_index` :1241-1251. Overlap arm :422-433 → :491-502.
  - Stage 6 placement: in `resolve_import_ladder` at its final `None` arm (:819), after the Java arm (:813-818). PYTHON-SUBMODULE-IMPORT-1 extracted the ladder unchanged; its Python stage runs before the ladder for Python edges only.
  - include_resolver.rs is unchanged since 9755ba47 (`resolve` :100-187, `derive_include_roots` :245, `build_include_resolution_map` :274-299, `no_suffix_guessing` :738-748).
  - orchestrator.rs: `INDEXER_VERSION` :59 → :66; refresh stream :1008-1039 → :1033-1063 (batch :1035, `resolve_edges` :1063); include map :883 → :892-893; `all_file_paths` :1965-1967 → :1977-1979; breakdown key :1091-1095 → :1114-1122.
  - compose.rs: the version copy at :1653 → none (reads the constant at :1648).
  - trust: `sum_unresolved_imports` :390-399 → :395-404; its test :994-1006 → :1001-1013; `compute_import_graph_reliability` :228-253 → :228-256; `compute_change_impact_reliability` :329-364 → :337-369; `sum_unresolved_calls` :371-387 → :373-393. service.rs: sum feed :370-374 → :343-347 and :375-379; change-impact feed :386-390 → :397-401; caveats :240-253 → :245-257; `minimal_input` :1035-1052 → :1063-1081; `.unwrap_or(0)` :333-342 → :337-347. overlay.rs :179-180 → :182-186, :312 → :318-320. stats.rs :442 → :460-466.
  - storage: `find_imports` :1726-1771 → :1776-1833; `ImportResult` :119-133 → :137-152; evidence :131-132 → :149-150; extractor select :1734 → :1785; evidence mapping :1752-1763 → :1803-1814; the shared-set test doc `trust_impl.rs` :1361-1368 → :1473-1480 (callers :1371/:1430 → :1483/:1542). The connectivity read is no longer `e.resolution = 'static'` at :810-811: it is TEST-EDGE-SCOPE-1B's default view (`compute_module_stats` :780-912, `ImportView::DEFAULT` :901; `ImportClass::from_resolution`, `classification/src/import_partition.rs:48-56`).
  - daemon-runtime: imports SQLite path :1759-1777 → :1792-1810; LiveGraph fallback :1760-1763 → `livegraph_feed.rs:1773-1777`; TS-only gate :1734-1742 → :1704-1718; test literal :4983-4995 → :5110-5122; modules-list headline :9383-9403 → :9608-9631 (stale comment :9392-9393 → :9614-9616; filter :9397 → :9622).
  - rgr: `imports --json` :848-858 → `commands/graph.rs:1002-1023`; default rows :92-99 → `imports.rs:96-107`; compare rows :323-333 → :347-359; the file is 975 lines (was 841).
  - classification: `unresolved_classifier.rs:102-107` → :100-117. types.rs unchanged at the cited lines.
- **New adjacent defect found (OBSERVED, §8).** `livegraph_feed.rs:2004-2009`: the `auto` imports path reads `repo_state.storage().ok().and_then(|conn| conn.find_imports(…).ok()).unwrap_or_default()`, so a failed read renders as `0 imports`. Pre-existing; outside this allocation.

## OC-6 — 2026-10-02 — `imports <file>` lists inferred rows only under `--include-inferred` (TEST-EDGE-SCOPE-1B); CIB-C09 follows, and gains a before-capture

- **Authority.** RG-REQ-002-L11 ("every surface that aggregates edges … shows the certain facts by default and states the inferred remainder as a count with the flag that includes it"). D-TESB-READERS-1 §1: every IMPORTS reader, `imports <file>` included, "reads static/dynamic by default, admits inferred rows only under `--include-inferred`". The PREP-4 packet, task 4.
- **Evidence (OBSERVED at cea4e2ee).**
  - `daemon-runtime/src/import_partition_view.rs:329-372` `partition_import_rows` drops the inferred rows unless `include_inferred`, counts them in `import_remainder.inferred.imports`, and keeps every kept row object unchanged.
  - Both SQLite paths call it (`dispatch.rs:1785-1787`, :1806-1808). `--include-inferred` routes `auto` to the SQLite path (:1642-1646). `--engine compare` refuses the flag (:1633-1641) and serves the unpartitioned listing (:1701-1714).
  - `rgr/src/presentation/imports.rs:74-127` prints the kept rows, then `+N inferred imports, not shown — --include-inferred` (`push_partition_lines` :115-126; `import_partition.rs` `per_file_remainder` :262-267, `remainder_lines` :228-240).
  - With HEAD's binary on a copy of the nginx before-root: `imports src/core/ngx_config.h` printed exactly `Imports: src/core/ngx_config.h\n\n0 imports\n`; its `--include-inferred --json` had `"imports": []` and an `import_remainder` whose `inferred.imports` was 0. `modules deps --json` had `results: []` with `import_view`/`import_remainder`. `modules list --json` had `unresolved_import_count` 1090. `trust --json` had `unresolved_imports=1090`.
  - `modules_deps.rs` prints the partition lines on the empty branch too (:156-157).
- **Historical text, kept.** CIB-C09 asserted that the DEFAULT `imports src/core/ngx_config.h` prints `6 imports` and the six inferred rows, and read the reasons from the default `--json`. It asserted only `line in ev` for `src/event/ngx_event.c`. §4 said: "if [1B] changed `imports <file>` so inferred rows are not printed inline, STOP and report the actual output". At HEAD that condition holds by design. These literals could not pass on any correct candidate.
- **New (CIB-C09).**
  - Default `imports src/core/ngx_config.h` == `Imports: src/core/ngx_config.h\n\n0 imports\n+6 inferred imports, not shown — --include-inferred\n`; its JSON has no row and `import_remainder.inferred.imports` 6.
  - `--include-inferred` prints exactly the six rows `  <path>  depth=1  inferred: unique basename → <path>`; its JSON carries the reason on each.
  - Default `imports src/event/ngx_event.c` == `…\n\n1 import\n\n  src/event/ngx_event.h  depth=1  static\n+2 inferred imports, not shown — --include-inferred\n`. `--include-inferred` prints the three rows exactly, ordered by name. The static JSON row has no `reason`.
  - BEFORE: the nginx base copy, served by the candidate, prints exactly `Imports: src/core/ngx_config.h\n\n0 imports\n`. Its `--include-inferred --json` has no row and an inferred remainder of 0. The copy still holds its one captured snapshot (OC-1).
  - `modules deps`, `trust` and `modules list` assertions are unchanged; their JSON keys were re-verified with HEAD's binary.
  - §1, §2.1 item 6 (the daemon path through `partition_import_rows`; the formatter's rows are read under `--include-inferred`), §2.2, §2.3, §4 and §6 restate it. §4's TEST-EDGE-SCOPE-1B and PYTHON-SUBMODULE-IMPORT-1 conditions are restated as verified at HEAD, with a STOP kept for a reader that differs at implementation time.
  - PYTHON-SUBMODULE-IMPORT-1's `find_file_importers` (`storage/src/agent_impl.rs:1691-1800`) accepts an inferred row with basis `unique_basename` and no alternate (it refuses a missing alternate only for `python_submodule`, :1784-1789). `explain_sections.rs:322-331` renders that row as `<file>  (inferred)`. No reason is rendered for inferred rows on `imports <file>` at HEAD.
- **Why this is a correction, not a decision.** RG-REQ-002-L11 and D-TESB-READERS-1 fix the default view. The slice's row form and reason are unchanged. Only where the agent reads them (under `--include-inferred`) and the check's literals move.
- **Limit.** The after literals cannot be executed before the candidate exists. The before literal, the JSON keys and the no-re-index assertion's data were OBSERVED with HEAD's binary.

## OC-7 — 2026-10-02 — D-PSI-R1-VOCAB: the bound suites' fixtures (record; no check changes)

- **Question (packet task 5).** Are the IMPORTS/CALLS fixtures of the bound suites canonical at HEAD? The bound suites are indexer, classification, repo-index, trust and storage (whole runs), and rgr and daemon-runtime (`--lib`).
- **Evidence (OBSERVED, grep at cea4e2ee).**
  - The only `resolution: "resolved"` fixtures in those crates are these:
    - Five are OWNS edges: `storage/tests/agent_impl.rs:1750/:1762`, `explain_serve_tests/fanin_fixture.rs:216`, `callgraph_cert/test_fixture.rs:365`, `callgraph_cert/ledger_tests.rs:767`, `focus_resolution_cert/test_fixture.rs:410`. A class check reads no OWNS edge.
    - Two are deliberate out-of-vocabulary counterexamples that assert refusal: `orient_lg_decisions/served_e2e.rs:1408` (IMPORTS, "insert out-of-vocabulary import") and `call_certainty.rs:893` (CALLS, `an_unreadable_stored_resolution_refuses_every_route_s_answer`).
  - The edges this slice writes are `static` or `inferred`.
  - The new storage read adds no resolution check: `find_imports`' existing `checked_import_class` (:1820-1831) stays. The renderer compares only `inferred`/`static`.
- **Conclusion.** Not applicable as a correction: no fixture needs a change, and no check changes.

## OC-8 — 2026-10-02 — the gate predicates of CIB-C01 and CIB-C05 (record; no check changes)

- **Question (packet task 6).** Do the gate predicates read the committed records and fail closed on anything but `accepted`?
- **Evidence.**
  - The prefix is `git cat-file -e HEAD:docs/assurance/<SLICE>/implementation-review.json && git show HEAD:… | python3 -c "… sys.exit(0 if (d.get('workItemId'), d.get('result')) == ('<SLICE>', 'accepted') else 1)"`, for TOOLCHAIN-STALENESS-1 and for TEST-EDGE-SCOPE-1B, in both checks. It reads the committed blob at HEAD, never the working tree.
  - OBSERVED at cea4e2ee: TOOLCHAIN-STALENESS-1 `kind implementation-review`, `result accepted`, completed 2026-09-26T18:48:04.621Z. TEST-EDGE-SCOPE-1B `result accepted`, completed 2026-10-02T17:21:34.609Z, last touched by f0693e28.
- **Probes (EXECUTED 2026-10-02).**

  | Probe | Expected | Actual |
  |---|---|---|
  | the CIB-C01 and CIB-C05 prefixes, extracted verbatim, at HEAD | exit 0 | 0, 0 |
  | the predicate on `result: accepted` | 0 | 0 |
  | `rejected` | ≠ 0 | 1 |
  | `refinement-required` | ≠ 0 | 1 |
  | another `workItemId` | ≠ 0 | 1 |
  | no `result` key | ≠ 0 | 1 |
  | malformed JSON | ≠ 0 | 1 |
  | `git cat-file -e` of a missing record | ≠ 0 | 128 |

- **Conclusion.** Verified as written; no change.

## OC-9 — 2026-10-02 (INPUT-2 iteration 1) — CIB-C05's `copy()` refuses a source store whose WAL holds bytes

- **Authority.** PREP-4 review-0 (`.agent-manager/slices/CPP-INCLUDE-BASENAME-1-PREP-4/review-0.json`, refinement-required), finding CIB-C05 (RG-REQ-011, -L06, -L07): "`copy()` copies each source `.db` but not its WAL. The document requires empty source WALs in step 0, yet CIB-C05 does not check that condition before copying. If a future capture has committed data in a non-empty WAL, its freshly recorded `roots-before` digest can still pass; the copy omits those transactions, and the later WAL check examines only the copy." Required action: fail before each copy unless the source WAL is absent or zero bytes; a negative probe; this entry.
- **Historical text, kept (OC-2's form).** `copy() { local src=$T-$1-before dst=$T-$1-basecopy; test ! -e "$dst" && mkdir -p "$dst/databases" && cp "$src"/databases/*.db "$dst/databases/" && …; }`.
- **Class and rule.** Class: a store read through a path that sees only its main file, with no check that the main file is the whole store. Rule: a manager root is used only through `copy()`, and `copy()` refuses — before creating anything — unless the root's `databases/` holds exactly one `.db` and every `*.db-wal` beside it is absent or empty (`test ! -s`). Class sweep (OBSERVED, the nine commands): `copy()` is the only reader of a manager root's database. `rg-store-diff.py` and the CIB-C05 query read only the copies and the after-roots; both already refuse a non-empty `-wal` on what they read. CIB-C09 reads the manager root's `registry.json` only. CIB-C06 reads every manager-root file only to digest it.
- **New (verbatim in CIB-C05).** `copy() { local src=$T-$1-before dst=$T-$1-basecopy w; test "$(ls "$src"/databases/*.db | wc -l | tr -d ' ')" = 1 || return 1; for w in "$src"/databases/*.db-wal; do test ! -s "$w" || { echo "REFUSED: $w holds uncheckpointed bytes - the copy would omit them"; return 1; }; done; test ! -e "$dst" && … }`. CIB-C05's `environment` and §2.3 state the precondition.
- **Probes (EXECUTED 2026-10-02; the `copy()` definition extracted verbatim from the allocation, `T` pointed at `/private/tmp/CPP-INCLUDE-BASENAME-1-PREP-4-probe`, source roots built with one sqlite `a.db` each; removed afterwards).**

  | Probe | Expected | Actual |
  |---|---|---|
  | `a.db-wal` holds 4 bytes | refused, nothing copied | rc 1, `REFUSED: …/a.db-wal holds uncheckpointed bytes …`, no copy |
  | `a.db-wal` empty | copied | rc 0, `a.db` copied |
  | no `a.db-wal` | copied | rc 0, `a.db` copied |
  | two `.db` files | refused | rc 1, no copy |
  | `test ! -s` on the real `-wal` of the three manager roots | pass | rc 0 ×3 |
  | `bash -n` on all nine commands | rc 0 | rc 0 ×9 |

- **Unchanged.** Every other check, the sets, the candidate paths and every expected value. **Author:** requirements author of CPP-INCLUDE-BASENAME-1-PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2 (allocation digest re-pinned).

## Operator confirmation (2026-10-02, in-place-manager) — OC-1 … OC-9 (INPUT-2)

The INPUT-2 corrections OC-1 through OC-9 authored by PREP-4 are approved; any "Approver: PENDING" line on them is resolved by this entry. Accepted by PREP-4 review-1 (codex gpt-6-sol). Carried as INPUT-2. Noted for the next revision, not corrected here (the accepted document's digest is pinned): the opening Status line enumerates OC-1…OC-8 while §9 and this ledger carry OC-9.
