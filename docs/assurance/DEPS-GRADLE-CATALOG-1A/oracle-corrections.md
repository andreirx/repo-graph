# DEPS-GRADLE-CATALOG-1A — oracle corrections ledger

Append-only (agent-manager docs/MANAGER.md § Oracle corrections). INPUT-1 was accepted at f6ffda2a (2026-09-24). Every entry below is carried as INPUT-2. The document item is DEPS-GRADLE-CATALOG-1A-PREP-4, and HEAD is 8b88f205c9b3c05b78f943a46f4fb7c60a7c3675. Its rust tree was clean against HEAD (`git diff --quiet HEAD -- rust`, EXECUTED).

## OC-1 (2026-09-27, PREP-4) — DGC-A04's isolated invocations lacked `RMAP_AUTO_REINDEX=off` (→ INPUT-2)

- **Old:** DGC-A04's `q()` exported `RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`. **New:** `RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off RMAP_AUTO_RETENTION=off`. DGC-A04 `environment` now states the reason.
- **Why it is load-bearing here:** DGC-A04 uses the candidate binary to serve builder-owned copies (`-stale`) of the v0.19.0 stores. The stamp of those copies is `{"extractors":["ts-core:0.2.0","c-core:0.1.0","cpp-core:0.1.0","java-core:0.1.0","python-core:0.1.0","rust-core:0.2.0"],"indexer":"indexer:1.0.0"}` (OBSERVED 2026-09-27 on all three manager roots, `immutable=1`). The candidate stamps `indexer:1.3.0`, `cpp-core:0.2.0` and `python-core:0.2.0`. Under RG-REQ-001-L06 as amended (D-STALE-SIGNAL-1), the first request for a store with a differing stamp schedules a background FULL index. The BEFORE side would then silently become the candidate's own facts.
- **Transport note (OBSERVED, source):** a stdio daemon already schedules nothing (`daemon-runtime/src/auto_reindex.rs:260-262` `note_first_use` returns when `stdio` is set or `auto_reindex_enabled()` is false). The explicit opt-out makes the proof independent of the transport. It follows the operator rule of D-STALE-SIGNAL-1 and the model (PYTHON-RECEIVER-BINDING-1 OC-1).
- **Scope:** `q()` is the only definition in the block, and every `rmap` call of every check goes through it (`"$RB/rmap"` occurs once, inside `q()`; OBSERVED). No other check invokes rmap or rmapd.
- **Probe (EXECUTED):** the `q()` definition was extracted from the new command and from the old one, then run with a stub `rmap` that prints its environment. New → `RMAP_AUTO_REINDEX=off` reaches the binary (PASS). Old → unset (FAIL). `bash -n` on the command: ok.
- **Author:** requirements author of DEPS-GRADLE-CATALOG-1A-PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

## OC-2 (2026-09-27, PREP-4) — the slice narrated the superseded staleness signal (→ INPUT-2)

- **Rule:** the slice states its `INDEXER_VERSION` bump as its own. What an existing store then shows and does is TOOLCHAIN-STALENESS-1's under RG-REQ-001-L06 as amended by RG-BOOTSTRAP-INPUT-10 (D-STALE-SIGNAL-1). The slice does not restate it. The models are CPP-ATTRIBUTE-MACRO-1A OC-2/OC-3, PYTHON-SUBMODULE-IMPORT-1 OC-2 and PYTHON-RECEIVER-BINDING-1 OC-2.
- **Checks:** none asserted the superseded shape. No command greps `… facts from … to refresh`, reads `families`, requires a `TOOLCHAIN_STALENESS` condition or requires exit 2 (OBSERVED: a scan of the six commands). The two remaining "exit 2" mentions are `deps list`'s own decode failure (`rgr/src/commands/deps.rs:133-146`, unchanged since f6ffda2a), not staleness. No staleness oracle is added.
- **Shipped line, read at HEAD for the record:** `index toolchain differs from running rmap (<component> <old> → <new>[, …]) — <suffix>`. With `reindex` disabled (stdio or the opt-out) the suffix is `run rmap repo rebuild <path>` (`rgr/src/presentation/toolchain_staleness.rs:34/:44-51`). It renders on `orient`/`check` only, never on `deps list`. So DGC-A04's human oracle over `deps list` output is unaffected (the `handle_deps_list` body is byte-identical to f6ffda2a's, EXECUTED `diff`).
- **Old → new text:**
  - DGC-A01 `inputs` VERSION clause: "… because TOOLCHAIN-STALENESS-1 and the queued resolver slices land first and may move it …" → the rewritten VERSION clause, ending with the one statement.
  - DGC-A01 `inputs` gate: "absent on f6ffda2a, so this slice cannot be implemented today" → present at 8b88f205. The gate stays in the command so the check cannot pass on a tree without the record.
  - DGC-A01 `expected`: "(its intended effect: … stores written by the previous indexer show TOOLCHAIN-STALENESS-1's rebuild line — RG-REQ-002-L07); apart from that stamp and its line" → the one statement.
  - DGC-A02 `expected`: "the version stamp and its staleness line are DGC-A01's intended effect" → "the version bump is DGC-A01's".
  - P-DGC-01: "stores written by the previous indexer show TOOLCHAIN-STALENESS-1's rebuild line on `orient`/`check` with `check` exit 2 until `rmap repo rebuild <path>` (RG-REQ-002-L07)" → the one statement.
  - Prose:
    - the Status line: "until `rmap repo rebuild <path>`" and "IMPLEMENTATION IS BLOCKED …" → gate satisfied at HEAD;
    - §0 decision records: D-STALE-SIGNAL-1 added (the manifest's new `requiredDecisionIds` entry);
    - §0 Changes (b): the `indexer facts from <old> (current <new>) — … to refresh` line with `check` exit 2 → the one statement;
    - §2.1: the untracked-provenance bullet;
    - §3: two "after/on a rebuild" → "once re-indexed";
    - §6: the ship block.
  - §9's history entries are unchanged.
- **Author:** requirements author of PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

## OC-3 (2026-09-27, PREP-4) — DGC-A01's version oracle measured f6ffda2a; compose.rs leaves the candidate paths (→ INPUT-2)

- **Old:**
  - DGC-A01 matched `^const INDEXER_VERSION` (exactly one).
  - It required orchestrator.rs to differ from HEAD by exactly its version line, and compose.rs by exactly its `let extractor = "<v>"; // Match INDEXER_VERSION in orchestrator.rs` line.
  - It required the old literal to leave "exactly the two declared sites".
  - DGC-A05 required compose.rs in the tree's exact change set.
  - `candidatePaths` listed `rust/crates/repo-index/src/compose.rs` for "the version line only".
- **Evidence (OBSERVED at 8b88f205):**
  - `rust/crates/indexer/src/orchestrator.rs:63` `pub const INDEXER_VERSION: &str = "indexer:1.2.0";`.
  - `:4409`, `:4412` and `:4417` pin that literal inside `build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version`.
  - `rust/crates/repo-index/src/compose.rs:1647` `let extractor = orchestrator::INDEXER_VERSION;`.
  - `git grep -F 'indexer:1.2.0' -- '*.rs'` finds only those four orchestrator lines, so HEAD holds no undeclared production copy (the §4 stop condition).
  - The old oracle cannot pass on any correct candidate. Probe: it fails on the correct bump with `('exactly one INDEXER_VERSION constant', [])`.
- **New:**
  - DGC-A01's Python block requires `^pub const INDEXER_VERSION` (exactly one) and exactly one minor above HEAD's.
  - orchestrator.rs's `-U0` diff must be the old literal replaced by the new one on every line that holds it, and nothing else. Every removed line holds the old literal, the added lines are those lines with the literal swapped, and the declaration is among them. So no other orchestrator line may change. This is tighter than PYTHON-SUBMODULE-IMPORT-1's PSI-C01 form, which filters the diff to the declaration line; the original DGC-A01 also allowed no other orchestrator change.
  - The old literal leaves orchestrator.rs entirely.
  - compose.rs is unchanged and reads the constant by name.
  - The old literal moves nowhere else, and the new one is added nowhere outside the orchestrator.
  - DGC-A02's name loop binds `build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version`, which is in the indexer crate, the suite DGC-A02 runs.
  - `rust/crates/repo-index/src/compose.rs` leaves `candidatePaths` (16 → 15) and DGC-A05's exact list. Its only reason to change was the version copy, which no longer exists. The carrier paragraph already required compose.rs to stay untouched. The "sixteen" statements (DGC-A05 `inputs`, §0 heading, §4) now read fifteen. The acceptance boundary, DGC-A01 `inputs`/`expected`, §0's path list and §2.1 (4) state the new rule. DGC-A05's list equals `candidatePaths` exactly (15 = 15, EXECUTED).
- **Allocation effect, stated:** one candidate path fewer. No obligation set, check id, requirement text or measured prediction changes. The operator may instead keep compose.rs as a candidate path; then DGC-A05's exact equality would have to admit its absence, which weakens that guard. This ledger does not recommend it.
- **Probes (EXECUTED; scratch worktree `/private/tmp/DEPS-GRADLE-CATALOG-1A-PREP-4-wt` at 8b88f205, removed).** The new Python block was run from `rust/`.
  - It passes on the correct bump (`indexer:1.3.0` on all four orchestrator lines), both unstaged and staged. It prints `orchestrator lines swapped: 4`.
  - It fails (rc 1, with the named assertion) on:
    - HEAD unmoved;
    - the declaration bumped with the pins unmoved;
    - one pinning line left on the old literal;
    - compose.rs turned back into a literal;
    - the new literal added in `agent/src/dto/toolchain_staleness.rs`;
    - the old literal moved there;
    - a major bump;
    - a two-minor bump;
    - the correct bump plus an unrelated orchestrator line.
  - DGC-A02's name loop over a synthetic `cargo test` log passes with all ten names. With the pinning test missing it fails with `MISSING build_toolchain_json_lists_extractor_names_in_order_then_the_indexer_version`.
  - DGC-A05's exact-list comparison, run in the worktree, passes on the fifteen paths. It fails on fifteen + compose.rs, on one path missing and on an extra path.
  - `bash -n` on all six commands: ok.
- **Author:** requirements author of PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

## OC-4 (2026-09-27, PREP-4) — literals, positions and floors re-verified at 8b88f205 (→ INPUT-2)

- **Floors** (EXECUTED `cargo test -p <crate> [--lib] -- --list | grep -c ': test$'` at 8b88f205, rust tree clean; floor = listing + the slice's new tests, whose number is unchanged):

  | Suite | Listed at f6ffda2a | Listed at 8b88f205 | Floor (was) |
  |---|---|---|---|
  | repo-index | 409 | 416 | 442 (435) = 416 + 26 |
  | indexer | 442 | 484 | 485 (443) = 484 + 1 |
  | module-queries | 85 | 85 | 86 (86) |
  | daemon-runtime `--lib` | 786 | 803 | 806 (789) = 803 + 3 |
  | rgr `--lib` | 1,268 | 1,313 | 1,317 (1,272) = 1,313 + 4 |

  Every existing test the checks bind by name exists at 8b88f205: the thirteen `config::tests::gradle_*`, `nearest_ancestor_gradle`, `gradle_leaf_without_deps_does_not_inherit_parent`, `unreadable_manifest_records_failed_not_parsed`, the eight named indexer tests, and `build_toolchain_json_…` (OC-3). The sampled new names are absent. With HEAD's counts, each floor fails, and it passes at listing + new (probe).
- **Positions.** `git diff --numstat f6ffda2a HEAD` shows every cited Rust file byte-identical to f6ffda2a except compose.rs, orchestrator.rs, dispatch.rs and daemon-runtime lib.rs. The byte-identical files are config.rs, manifest_deps.rs, invalidation.rs, settings_gradle.rs, module-queries `deps/{types,mod,compose}.rs` and `lib.rs`, deps_coverage.rs, deps_ecosystem_presence.rs, reader_context.rs, deps_headline.rs, rgr `deps_list.rs` and `commands/deps.rs`, and repo-index `Cargo.toml`. Their citations stand. Corrected, "at 8b88f205":
  - compose.rs:641 → :635 (`resolve_gradle_deps`);
  - compose.rs:1136 → :1130 (`parse_settings_gradle`);
  - compose.rs:1653 → :1647 (reads by name; OC-3);
  - orchestrator.rs:59 → :63;
  - dispatch.rs 10,252 → 10,270 lines, :6907-6930 → :6925-6948 and :6982-6996 → :7000-7014.

  The body of `handle_deps_list` (:6724) is byte-identical to f6ffda2a's. DGC-A06's Python anchors (`fn handle_deps_list(`, `fn handle_deps_why(`, `crate::deps_headline::build_deps_list_response(`) still resolve. Daemon-runtime lib.rs changed, but DGC-A06 asserts only its one-line diff, which is position-free.
- **Stored-fact literals.** EXECUTED, python sqlite3 `immutable=1` on the manager's re-created roots, each with one registry entry whose `db_path` is inside the root, one `full` `ready` snapshot equal to the registry's `last_snapshot_uid`, and no `-wal`. All match the allocation:
  - kafka: 5,877 Java files, all `{org.ajoberstar.grgit}`; provenance 1 record (`build.gradle`).
  - petclinic: 47 Java files, all the twelve groups; 1 record.
  - grpc-java:
    - 1,562 Java files;
    - `core/` 233 × `{com.google.guava}`;
    - `gae-interop-testing/gae-jdk8/` 2 × `{com.google.appengine, com.squareup.okhttp, javax.servlet}`;
    - `com.google.guava` and `com.squareup.okhttp` in no other file;
    - 57 records, none `build.gradle` or `bom/build.gradle`.
  - No record carries `undetermined_blocks`.
  - Also measured: no unresolved row with basis `specifier_matches_package_dependency` targets `org.ajoberstar*`, `com.google.guava*` or `com.squareup.okhttp*` in kafka or grpc-java. The three removed groups therefore back no classification basis. This supports DGC-A01's "no extracted fact changes other than the Gradle declared set" for those stores (INFERRED beyond the basis column).
  - Manager root digests recorded at the start of this item (`shasum -a 256` of `registry.json` and the `.db`). They are re-checked at the end (see the author's report).
- **Query-time literals:** the rows (61 / 1 / 37), counts and rendered lines were measured with the v0.19.0 binary (2026-09-24). They are NOT re-measured with 8b88f205's binary: building it is outside a document item (`cargo` beyond `-- --list` is excluded). DGC-A04 asserts the row-by-row equality of stale against after, both served by the candidate, and only declared-side absolutes. Those absolutes derive from the re-measured stored sets and from module rows. The deps query code (module-queries, `handle_deps_list`, rgr `deps_list.rs`) is byte-identical to f6ffda2a's. The observed-side absolutes stay REPORTED. The `inputs` wording "a queued predecessor may legitimately change the query side" → the query side has moved since the v0.19.0 binary.
- **Corpus sources** (OBSERVED at the pinned commits):
  - kafka 0dad6a7 (clean): :19, :23, :25-28, :167, :306, :503, :512, :518-519, :967, :1868, :1878-1884 and :1955 as cited; 62 top-level `project(':…')` heads; 65 `dependencies {` heads.
  - grpc-java f430131: `build.gradle` :14, :190, :220, :271 and :467; `bom/build.gradle:15`; `core/build.gradle:1-5`; `gae-jdk8/build.gradle:15-20`; 20 tracked `examples/**settings.gradle*`.
  - petclinic edf4db2: :33 / :59.
  - grpc-java's checkout carries 2,997 untracked files, all rmap `MAP.md` sidecars with the generated marker, dated 2026-09-05. They predate the v0.19.0 index, which holds none of them (the exhaust predicate, RG-REQ-011-L07). No change.
- **Other text:** §5 step 0's report base `f6ffda2a…` → `8b88f205…` (a report command, not a check; no check pins f6ffda2a). §7 states the re-created roots and their stamp. The Status line states the re-verification.
- **Author:** requirements author of PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

## OC-5 (2026-09-27, PREP-4) — determinations with no text change (→ INPUT-2)

- **The `"resolved"` fixture class (D-PSI-R1-VOCAB; PYTHON-RECEIVER-BINDING-1 A-1): not applicable.** The class is a test fixture that seeds a resolution value outside `static | dynamic | inferred` into a read that a slice makes certain-only. This slice makes no resolution read certain-only. It reads and writes `file_signals.package_dependencies_json` and the provenance blob, and adds query-time envelope keys. The suites its checks run (repo-index, indexer, module-queries, daemon-runtime `--lib`, rgr `--lib`) therefore meet HEAD's fixtures as HEAD has them. `git grep -F '"resolved"'` over module-queries, repo-index, `rgr/src/presentation/deps_list.rs` and `daemon-runtime/src/deps_*` finds nothing (EXECUTED).
- **PORTABLE-TMP-1 (D-PTMP-ROOT-1): the `/private/tmp` paths stay valid.** On macOS the sandbox base is unchanged (`platform-paths/src/sandbox.rs:40` `"macos" => Path::new("/private/tmp")`, OBSERVED). The isolated roots `/private/tmp/DEPS-GRADLE-CATALOG-1A-*` are therefore classified exactly as before. DGC-A04 is macOS-only by construction (`sed -i ''`, `shasum`), and the corpora and the operator registry path are macOS paths.
- **The TOOLCHAIN-STALENESS-1 gate: kept as is.** It is satisfied at HEAD: `git show HEAD:docs/assurance/TOOLCHAIN-STALENESS-1/implementation-review.json` → `workItemId` TOOLCHAIN-STALENESS-1, `result` accepted (EXECUTED). It stays the first link of DGC-A01, so the check cannot pass on a tree without the record.
- **Author:** requirements author of PREP-4 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-2.

- **Slice document digests (OC-1 … OC-5, the Status line and the §9 INPUT-2 entry, one pass):** before sha256:659ed4b78b760ea4f93d9226e711b45ec622b61e60f4eeef105ba768cb14d709 (the manager's INPUT-2 `baselinePath` edit, equal to the manifest's placeholder `allocation` digest); after sha256:07326d4fc1393b945490a91abfdb982d26d0f1b52f1f629f5912fc4a52968f38. The INPUT-2 manifest's `allocation` digest is re-pinned to the after value.

## OC-6 (2026-09-27, PREP-5) — DGC-A04 predicted rows that correctly disappear (→ INPUT-3)

- **Context:** INPUT-2 (1ea958bc) was admitted and built. DGC-A01, A02, A03, A05 and A06 passed. DGC-A04 failed at `assert strip(b) == strip(a), (n, "a header field moved", …)`: kafka `count` 61 → 60 (`.agent-manager/slices/DEPS-GRADLE-CATALOG-1A/evidence/DGC-A04.out`, rc 1). The implementation review (`review-1.json`, `RESULT: decision-required`) found no product defect and raised D-DGC-A04-EMPTY-RECORD. The operator resolved it as **A**: correct the oracle to the existing sparse-row contract. Keeping empty rows (B) would change a frozen output contract outside the allocation.
- **Old prediction (DGC-A04 `inputs`, PREDICTED AFTER):**
  - "kafka — every row loses exactly `org.ajoberstar.grgit` … and nothing else in any row or the header moves";
  - the command asserted `len(bm) == ROWS[n]` (61 / 1 / 37 rows after), `strip(b) == strip(a)` including `count`, the human header and rollup unchanged apart from the declared total, and `set(a) == set(b)` over the Java `file_signals` rows.
- **Sparse-emission sites, at HEAD 1ea958bc (OBSERVED, source), and not touched by the candidate** (`candidate-admission-1.patch`: its orchestrator.rs hunks are the `INDEXER_VERSION` literal only; its module-queries `deps/compose.rs` hunks add `undetermined_blocks: None` to two test helpers):
  - `rust/crates/module-queries/src/deps/compose.rs:469-480` `reconcilable_module_paths` = the keys of `module_imports` ∪ `module_declared` ∪ `module_rejected`, called at :316-317. A declared key is inserted only for a `file_signals` row that `get_package_dependencies_for_snapshot` returns, and that read filters `AND fs.package_dependencies_json IS NOT NULL` (`rust/crates/storage/src/crud/module_edges_support.rs:398-411`). Import and rejected keys come only from `external_library_candidate` references (`module_edges_support.rs:285-312`).
  - `rust/crates/repo-index/src/compose.rs:658` maps an empty declared set to `None`. `rust/crates/indexer/src/orchestrator.rs:701-707` writes a `file_signals` row only when `has_bindings || has_pkg_deps || has_aliases`.
- **New (rule, DGC-A04 only):** a row whose every fact is a coordinate 1A excludes is ABSENT after. The rule is computed from the BEFORE side, never from the candidate.
  - **Module row:** absent iff every entry is `declared_but_unobserved` of its predicted group and every observed-side count (`declared_and_used`, `type_only_import`, `observed_but_undeclared`, `first_party_self`, `runtime_builtins`, `unknown_external_like`, `rejected_non_specifier`) is 0. This is evaluated on the v0.19.0 rows, and it must agree with the same rule read from the v0.19.0 store (`store_vanished`: owned Java declared sets inside the predicted group, and no Java `external_library_candidate` reference from the module's files).
  - **Java file-signal row:** absent iff `import_bindings_json` and `tsconfig_aliases_json` are NULL and its declared set lies inside its predicted group (`bare`, read from the v0.19.0 store with `immutable=1`).
  - **Everything else unchanged from INPUT-2:**
    - every surviving row is compared with its v0.19.0 row minus exactly the predicted coordinates;
    - the JSON `count`, the human header (`deps · java · N modules`) and the rollup's module count move by exactly the absent rows (an absent row is predicted only inside the rollup, and this is asserted);
    - every other header field, the marking, the human transform and petclinic's `rg-store-diff.py` whole-store identity and byte-identical render are unchanged;
    - an absent file-signal row reads as the empty declared set.
- **Literals** (asserted beside the rule; EXECUTED, python sqlite3 `?immutable=1` on the manager's re-created roots):
  - Module rows: kafka `['group-coordinator/group-coordinator-api']` (61 → 60); petclinic none; grpc-java none. `core` keeps its row on `observed_but_undeclared` 1.
    - The kafka module is the 17th of the 61 rows, inside the rollup. Its row is `declared_but_unobserved` 1 (`org.ajoberstar.grgit`), with every other count 0.
    - It owns eleven Java files, six of them with import bindings. No Java `external_library_candidate` reference comes from them.
  - The two derivations agree on all three stores: the rule over the stale capture and the store read.
  - Java file-signal rows absent: kafka 535, petclinic 0, grpc-java 14 (all under `core/`, e.g. `core/src/main/java/io/grpc/internal/BackoffPolicy.java`). No non-Java `file_signals` row carries a declared set in any of the three stores (kafka: 149 Python rows; grpc-java: 3 C++ rows; none with `package_dependencies_json`).
  - Human render: kafka `deps · java · 60 modules` and `(+53 more modules: 0 declared deps — \`--json\` for all)`.
- **Agreement with the builder's diagnostic** (`evidence/DGC-A04-diagnostic.py`/`.out`, non-binding):
  - It reports kafka 61 → 60 with `group-coordinator/group-coordinator-api` vanished, and 535 kafka / 14 grpc-java Java files with no after-row, 0 petclinic.
  - That agrees on every count and on the module. Its file rule was weaker: absent ⊆ files with no import bindings, plus per-file equality reading an absent row as the empty set. With the per-file equality, every absent file's v0.19.0 set lies inside its predicted group. Its absent sets are therefore subsets of this rule's sets, and with equal counts (535 = 535, 14 = 14) they are equal (INFERRED from its assertions; the after-stores were deleted by DGC-A05).
- **Probes** (EXECUTED; scratch `/private/tmp/DEPS-GRADLE-CATALOG-1A-PREP-5-work` and `-probe`, removed). The extracted Python was run with its capture and after-root paths redirected. The inputs were:
  - the candidate's real captures (`evidence/dgc-{kafka,petclinic,grpc-java}-{stale,after}.{json,txt}`);
  - a MODELED after-store: the v0.19.0 Java `file_signals` minus the predicted group, under the unchanged writer rule. The real after-stores no longer exist.

  Results:
  - **Positive:** the corrected oracle prints `DGC-A04 OK` (kafka 60 rows, absent `['group-coordinator/group-coordinator-api']`; 5,877 files, 535 without a row; grpc-java 235 moved, 14 without a row; petclinic unchanged).
  - **Old behaviour (must fail) — all fail:**
    - the unfixed JSON (stale served as after) → `count moves by exactly the absent rows` (61, 61);
    - the unfixed writer (every v0.19.0 row kept) → `the Java file-signal rows after are the v0.19.0 rows minus exactly the fact-less ones`;
    - the human header left at 61, and the rollup left at +54 → the whole-text transform;
    - option B (the empty row kept, count 61) → the count assertion.
  - **A candidate that drops a row that still has a fact (must fail) — all fail:**
    - JSON without kafka `clients`, grpc-java `core` or petclinic `.` → the count assertion;
    - a swap (the empty row kept and `connect/api` dropped, count 60) → `module rows: the v0.19.0 rows minus exactly the absent ones`;
    - a store missing a kafka row, or a grpc-java row, that still has a fact → the file-signal assertion;
    - the human render without the kafka `clients` row → the whole-text transform.
  - **Control:** the INPUT-2 oracle fails on the same captures exactly as the implementation run did (`a header field moved`, `count` 61 vs 60).
  - `bash -n` passes on all six check commands.
- **Not changed:**
  - any other check, `expected` beyond DGC-A04, the obligation sets, `candidatePaths`, the acceptance boundary, §2.3 and §6. §6's definition of done names no row count. §2.3 says "every row and header field identical except the predicted removals" and "(kafka 5,877 files; grpc-java 233 + 2)"; both remain true under the rule, which counts an absent row as a predicted removal and reads an absent file row as an empty set. They were left untouched under the item's diff bound; see the author's report.
  - the product: no code, no emitter, and no frozen output contract.
- **Author:** requirements author of DEPS-GRADLE-CATALOG-1A-PREP-5 (claude-opus-5-5). **Approver:** in-place-manager (operator; the D-DGC-A04-EMPTY-RECORD = A ruling in the PREP-5 packet). **Carried as:** INPUT-3.
- **Slice document digests (OC-6, the Status line and the §9 INPUT-3 entry, one pass):**
  - before: sha256:fbc48b3bfb5ea8087244d741573cbb914d9a60c463f4881f9b7e2e73668d1702 (the manager's INPUT-3 `baselinePath` edit, equal to the manifest's placeholder `allocation` digest);
  - after: sha256:2da600706ce93673885dd402bad31b31edcfd23d428f5314a091a367a6bb4bef.

  The INPUT-3 manifest's `allocation` digest is re-pinned to the after value.

## A-1 (2026-09-27, PREP-6) — allocation amendment: two attribution cases answer honestly; DGC-A01 and DGC-A06 gain the tests (→ INPUT-4)

- **Context:** INPUT-3 (8b8dbc5b) was admitted and built (admission 2), and all six checks passed. Implementation review-0 (`.agent-manager/slices/DEPS-GRADLE-CATALOG-1A/review-0.json`, `RESULT: decision-required`, finding F-DGC-PROJECT-PROVENANCE, decision D-DGC-PROJECT-PROVENANCE) found that nearest-script attribution can report a CERTAIN declared set where the truth is unknown. The operator ruled D-DGC-ATTRIBUTION-1 (option D, under the human's standing ruling of 2026-09-26; the human may override). The manager added it to INPUT-4 as governance and to `requiredDecisionIds`, and pointed `baselinePath` at INPUT-4.
- **Sites** (OBSERVED, source; the candidate read from a scratch worktree of 8b8dbc5b with `candidate-admission-2.patch` applied, which is byte-identical to admission 1's patch by `cmp`):
  - Case 1 — no ancestor script. In the candidate's `manifest_deps.rs:88-89`, `let Some((script_dir, script)) = nearest else { return Vec::new(); };` returns before any settings file or other project script is read. `GradleBuild::declared_for` (:601-629) would accept another script's `project(':P')` block. At 8b8dbc5b the nearest-script walk falls through to `self.gradle_cache.insert(dir, empty.clone()); empty` (:133-134).
  - Case 2 — a failure hidden behind a parsed record. `ManifestProvenanceCollector::record` (candidate :817-827; 8b8dbc5b :300-309) and `record_failed` (candidate :832-842; 8b8dbc5b :314-323) both push only `if self.seen.insert(path.clone())`. The first record per path wins, so the candidate's `fail_gradle_record` (:160-177), reached through a root script already recorded as parsed, is dropped. Query time then pins the parsed record: module-queries `deps/compose.rs:496-568` `attach_manifest_context` returns `ManifestContext::Parsed` for an `error == None` record.
- **Rule (one class, three rules; §2.1 (6), DGC-A01's ATTRIBUTION HONESTY paragraph):** the class is an attribution outcome reported through nearest-script provenance, although the declared set can depend on another script of the build or on a settings failure.
  - (a) A Java file under an ancestor settings file that has no ancestor script goes through the build attribution. Its answer is one of three: the project's `declared_for` set (DECLARED); the empty set with the build's `undetermined_blocks` marking (UNDETERMINED); or, when the build's attribution is itself unknown, a FAILED `java` record naming the cause whose `dir` contains the file. The implementation takes the smaller of DECLARED and UNDETERMINED and states which.
  - (b) `record_failed` replaces an earlier parsed record of the same path in place. Parsed after FAILED stays FAILED, and a second failure keeps the first reason. A replaced record carries no marking; its build's marking moves to the build's smallest-path recorded non-FAILED record.
  - (c) A Java file in no Gradle build keeps the empty set with no record and no marking.
  - The third outcome of (a), an unknown build reached by no script, goes beyond the ruling's wording, which names only DECLARED and UNDETERMINED. It is the same class and follows this slice's existing rule for an unknown build (§2.1 (2): a FAILED record naming the cause). A FAILED build carries no marking (D-DGC-CONDITIONAL-1), so UNDETERMINED is not available there. It is carried here for the operator's confirmation.
- **Allocation:**
  - DGC-A01: the name loop gains `collector_failure_wins_over_parsed_for_the_same_path_in_either_order`, `gradle_project_reached_by_no_ancestor_script_is_declared_or_marked_never_a_certain_empty_set`, `gradle_unknown_build_reached_by_no_ancestor_script_is_a_failed_record_never_a_certain_empty_set`, `gradle_attribution_failure_wins_over_an_earlier_parsed_record_of_the_same_script` and `java_file_in_no_gradle_build_keeps_the_empty_set_unmarked` (NEW, all in `repo-index/src/manifest_deps.rs`'s test module). It also gains the existing `collector_dedupes_by_path` and `collector_record_failed_carries_reason`, bound because the collector rule changes. The floor moves 442 → 447 (416 + 31). `inputs` gains the ATTRIBUTION HONESTY paragraph (fixtures and PASS/FAIL oracles), and `expected` gains the rule.
  - DGC-A06: the name loop gains `deps_list_gradle_project_without_an_ancestor_script_is_declared_or_marked_in_json_and_human` and `deps_list_gradle_failure_behind_a_parsed_script_is_unknown_with_reason_in_json_and_human` (NEW, in `rgr/src/presentation/deps_list.rs`'s test module). They drive the real daemon in-process through the isolated harness of `daemon-runtime/tests/deps_attrib_nested_workspace.rs:36-86` and render with `DepsListResponse::render_human`, the `deps list` JSON and human boundary. The rgr `--lib` floor moves 1,317 → 1,319. `inputs` gains the BOUNDARY paragraph, and `expected` gains the rule.
  - Placement (local decision): the boundary tests live in an already-allocated path because D-DGC-ATTRIBUTION-1 expects no new candidate path. rgr is the one crate that holds both the daemon (a normal dependency) and the renderer, and its `tempfile` dev-dependency already exists (rgr/Cargo.toml:196). A daemon-runtime integration test could not render the human text.
  - The (c) guard is NEW because no existing test proves it: `nearest_ancestor_gradle` and `gradle_leaf_without_deps_does_not_inherit_parent` both hold a build script (OBSERVED).
- **Oracle independence:** every PASS condition is stated over the fixture's source (the declared coordinate `org.acme`, the unreadable `zz/settings.gradle`) and the existing wire and render vocabulary (`entries[].category`, `manifest_path`, `manifest_context`, `declared_undetermined_*`, the `[<manifest>]` row header, the note line). It is never stated over a candidate output. The case-1 checks accept exactly the ruling's two outcomes and fail on a certain empty set (at the boundary: `org.acme` declared for no row, with no note).
- **Floors** (OBSERVED from the admitted build's logs, `evidence2/dgc-01.txt` 442 tests ok and `dgc-06-rgr.txt` 1,317 ok): the new floors are the candidate's counts plus the added tests. The module-queries floor (86), the daemon-runtime floor (806) and the indexer floor (485) are unchanged.
- **Field:** the three corpora hold neither case. Each has a root `build.gradle` (§1). The candidate's captured `deps list` outputs (`evidence2/dgc-{kafka,grpc-java,petclinic}-after.{json,txt}`) hold 0 `present but not parsed` rows and 0 `gradle project attribution unknown` reasons (OBSERVED, grep). DGC-A04's predictions are therefore unchanged (INFERRED).
- **Probes** (EXECUTED; scratch `/private/tmp/DEPS-GRADLE-CATALOG-1A-PREP-6-work`, removed):
  - `bash -n` passes on all six check commands.
  - DGC-A01's loop (49 names), with its log path redirected: it passes on the admitted build's real log plus the seven new lines, and on a synthetic log holding every name. It fails on the real log alone (`MISSING collector_failure_wins_over_parsed_for_the_same_path_in_either_order`). It also fails when one name is missing — `java_file_in_no_gradle_build_keeps_the_empty_set_unmarked`, `collector_dedupes_by_path` or `gradle_nested_subprojects_block` — and when a new test reads `FAILED`.
  - DGC-A06's loop (10 names), with its three log paths redirected: it passes on the real logs plus the two new lines. It fails on the real logs alone, with either new name missing, with a new test `FAILED`, and with an existing render test `FAILED`.
- **Not changed:** the obligation sets, check ids, `candidatePaths`, `acceptanceBoundary`, DGC-A02–A05, requirement texts and the product.
- **Author:** requirements author of DEPS-GRADLE-CATALOG-1A-PREP-6 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-4.
- **Slice document digests (A-1 and the §9 INPUT-4 entry, one pass):**
  - before: sha256:a919552d7c0a6c3999f44a26b7d36b0508416a67c74c1a807bd45cb60029a5be (the manager's INPUT-4 `baselinePath` edit, equal to the manifest's placeholder `allocation` digest);
  - after: sha256:13fffa8d72ac5a15678c410953b31898cf8cd16bc2908ca9f0a4b67075f3b240.

  The INPUT-4 manifest's `allocation` digest is re-pinned to the after value.

## A-1 revision 1 (2026-09-27, PREP-6 iteration 1) — the case-1 oracles bind to project `a` and to the undetermined block (→ INPUT-4)

- **Raised:** document review-0 of PREP-6 (`.agent-manager/slices/DEPS-GRADLE-CATALOG-1A-PREP-6/review-0.json`, `RESULT: refinement-required`, F-1). DGC-A06's case-1 DECLARED branch accepted `org.acme` declared on SOME row, so it could pass with the declaration wrongly on sibling `b` and `a` unanswered. DGC-A01's UNDETERMINED branch accepted a marking whose `first` begins with `settings.gradle`, a location that identifies no dependency block although the rendered claim counts dependency blocks.
- **Class and rule:** the class is an oracle that checks evidence is present without binding it to the source fact and to the affected answer. The rule: every D-DGC-ATTRIBUTION-1 oracle binds its evidence to the affected file's own answer (the file's resolved set; the `results[]` row whose `module` is the file's module, and that row's human block) and to the exact source fact (`<script>:<line>` of the dependency block).
- **Applied (text only; names, floors, obligations, paths unchanged):**
  - DGC-A01, `gradle_project_reached_by_no_ancestor_script_is_declared_or_marked_never_a_certain_empty_set`: DECLARED = `A.java` → exactly `["org.acme"]`. UNDETERMINED = `A.java` → `[]`, plus exactly one marking with `first` = `b/build.gradle:2` (the `project(':a')` block's `dependencies {` head), plus a non-FAILED `java` record covering `a/src/main/java/app`. It fails on any other `first`, on a marking without such a record, and on any `B.java` answer but `[]`. Rule (a)'s UNDETERMINED text in the same paragraph and in §2.1 (6) now requires the marking to locate the block and reach the file's own module row.
  - DGC-A06, `deps_list_gradle_project_without_an_ancestor_script_is_declared_or_marked_in_json_and_human`: every assertion is on row `module == "a"` (compose.rs sets `module` to the module's canonical path, :377) and on its human block. DECLARED = `org.acme` in a declared category on row `a`, and in its human example groups. UNDETERMINED = row `a` in manifest scope with `org.acme` `observed_but_undeclared`, the note naming `b/build.gradle:2`, and `undeclared N (declared set incomplete)` on row `a`. In both, an `org.acme` entry or mention on any other row fails. The fixture import becomes `import org.acme.Lib;`, whose observed package is exactly the declared group (`normalize_java_specifier`, normalize.rs:92-102, OBSERVED at HEAD), so no 1B matching is involved. Stated consequence: UNDETERMINED needs a record covering `a/`; DECLARED needs none.
- **The other four D-DGC tests, checked against the rule:** `gradle_unknown_build_reached_by_no_ancestor_script_is_a_failed_record_never_a_certain_empty_set` binds to a FAILED record covering `a/src` whose reason names the form. Case 2 binds at the reader to the only `build.gradle` record (the nearest script of `Z.java`) and at the boundary to every row (no row may read `build.gradle` as parsed, Z's included), with the reason naming `zz/settings.gradle`. The guard binds to `P.java`'s set and an empty record list. All unchanged, as the review found.
- **Author:** requirements author of DEPS-GRADLE-CATALOG-1A-PREP-6 (claude-opus-5-5). **Approver:** PENDING — the operator (in-place-manager). **Carried as:** INPUT-4.
- **Slice document digests (this revision and its §9 line, one pass):** before: sha256:13fffa8d72ac5a15678c410953b31898cf8cd16bc2908ca9f0a4b67075f3b240 (A-1 as first submitted); after: sha256:a4686a37da0ef7d94db06f976188a14c36c84db202fe22e3c7fefa110fea8667. The INPUT-4 manifest's `allocation` digest is re-pinned to the after value.

## Operator confirmation (2026-09-28, in-place-manager) — A-1 and its third outcome

A-1 is approved; its "Approver: PENDING" line above is resolved by this entry. The third outcome of rule (a) is confirmed as within D-DGC-ATTRIBUTION-1: an unknown build reached by no script is a FAILED record naming the cause. That is the ruling's invariant (never a certain empty set; unknown with a reason) under this slice's existing unknown-build rule (§2.1 (2)). A FAILED build carries no `undetermined_blocks` marking (D-DGC-CONDITIONAL-1), so the ruling's UNDETERMINED branch cannot apply there. PREP-6 accepted at review-1 (codex gpt-6-sol). Carried as INPUT-4.
