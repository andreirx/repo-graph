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
