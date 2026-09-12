# CPP-INCLUDE-ROOTS-1 — `#include` resolves through every `include/` directory the repository has

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q3 (HIGH; never worked — deferred by the C include-resolution milestone v1.1). CODE slice: `indexer/src/include_resolver.rs` only (increment 1). Increment 2 (unique multi-segment suffix fallback) is RG-REQ-006-L11, PROPOSED, not in this packet. Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-006-L03 (C/C++ includes resolve through per-module include roots), RG-REQ-004-L09 (the poco zero-state becomes a real edge list — the denominators move), RG-REQ-002-L06 (coverage stated — the unresolved count falls to the true system-header remainder).

**Changes (pre-authorised, measured in RC-10):** poco resolved IMPORTS 1,702 → 11,595, unresolved 13,703 → ~3,810 (3,442 system headers + 368 ambiguous), `modules list` gains ~50 cross-module pairs led by `Net → Foundation` (616); duckdb +2,277; OpenXcom +153; cycles, trust's unresolved-import count and the zero-state note move accordingly; poco's store grows (~6× IMPORTS) — index time and size measured and reported.

**Preserves (§3):** RG-REQ-006-L01/L02/L04 (other resolver stages), RG-REQ-005-L01/L02/L07/L09 (call binding untouched), RG-REQ-004-L02 (edge derivation unchanged), RG-REQ-001-L02, byte-stability of leveldb/vcmi/nginx/swupdate/sqlite (provable: zero `*/include` directories beyond the root), `--include-root` as escape hatch.

## 1. Problem (ROOT-CAUSED — RC-10)

`include_resolver.rs:96-180` tries same-directory (quoted), configured roots (`--include-root`, empty by default) and three REPO-ROOT-ANCHORED literals `include`, `inc`, `src/include` (:62). poco's headers live at `Foundation/include/Poco/*.h` — 35 `*/include` directories none of which is tried — so `Net/src/HTTPClientSession.cpp`'s `#include "Poco/Exception.h"` is unresolved and no module edge can be derived ("No cross-module dependencies detected … 13703 imports did not resolve"). Never worked: the literal list is from the resolver's introducing commit 55f1ac6; `docs/milestones/c-include-resolution-v1.1.md:290-302` deferred build-system detection. gstreamer is a SECOND cause (meson subprojects; headers never under `include/`) — L11, separate.

## 2. Contract

1. Conventional include roots are DERIVED from the indexed file list: every directory whose last component is `include` or `inc`, at any depth, is a candidate root (the three literals become a special case of this rule). `build_include_resolution_map` already receives `file_paths`.
2. Resolution order unchanged: same-directory (quoted) → configured roots → derived roots. Several roots matching the same header → `Ambiguous`, unresolved, counted (existing behaviour `ambiguous_when_multiple_roots_have_same_header`).
3. No guessing is introduced: `no_sibling_directory_magic` and `no_suffix_guessing` stay green; single-segment system headers (`<vector>`, `<string.h>`) stay unresolved.
4. Adding roots can only turn Unresolved into Resolved or Ambiguous — never flip an existing resolution (same-directory hits return first; no corpus repo with an existing root-based resolution gains a root).
5. Outward proof: poco `modules list` shows `Net → Foundation` (616 file-level imports) among ~50 pairs; unresolved 13,703 → ~3,810; `cycles` renders whatever SCCs now exist with their sizes; leveldb and vcmi `modules list`/`cycles` byte-identical.

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-006-L03 existing behaviour | same-dir precedence; configured roots win over conventional; ambiguity | `include_resolver.rs` tests `same_directory_wins_over_include_root`, `configured_root_wins_over_conventional`, `ambiguous_when_multiple_roots_have_same_header`, `no_sibling_directory_magic`, `no_suffix_guessing` |
| Byte-stability repos | leveldb, vcmi, nginx, swupdate, sqlite | resolved/unresolved IMPORTS counts identical on isolated indexes (leveldb + nginx at minimum in the builder window; others operator-run) |
| RG-REQ-006-L01/L02/L04 | non-C/C++ stages | their test families green; no file outside `include_resolver.rs` changes |
| RG-REQ-005-L01/L02/L07/L09 | call binding | `resolver.rs` call tests green (the file is not touched) |
| RG-REQ-004-L02 | edge derivation | `derive_module_dependency_edges` untouched; `two_crate_fixture_renders_a_to_b_edge_verbatim` |
| RG-REQ-004-L09 | zero-state wording rule | `list_render_empty_edges_is_zero_state` still green on a fixture with zero edges |
| RG-REQ-011-L03 | store growth stays within retention budget | poco store size and index time before/after reported; `retention_prune_benchmark_gate` green |
| RG-REQ-011-L06 | isolation | throwaway roots; registry sha unchanged |

## 4. Stop conditions

Frozen: resolution order, `--include-root`, the call resolver, schema, wire, exit codes. No CMake/meson parsing (strictly larger and recovers less). No suffix fallback (L11). If poco's index time or store size grows beyond the smoke's 300 s client window or the retention budget, STOP + report (a cost decision for the human). STANDING HONESTY RULES. Unmet DoD → STOP. Do NOT commit.

## 5. Validation (ORDERED; `build-progress.md` after each step)

1. Failing test FIRST: a fixture with `Foundation/include/Poco/Exception.h` and `Net/src/a.cpp` including `"Poco/Exception.h"` resolves; a duplicated header under two `include/` roots stays ambiguous; `no_suffix_guessing` unchanged.
2. `cargo test -p repo-graph-indexer`; fmt/clippy.
3. Live proof: isolated index of poco (`RMAP_TRANSPORT=stdio`; poco is ~0.6 GB store — within the window) with a worktree before-binary on a second isolated root for the before counts; `modules list`, `cycles`, the store SQL counts; index wall time and store size before/after.
4. Byte-stability: leveldb and nginx isolated indexes — counts identical.
5. Cleanup; `build-N.md`.

OPERATOR-RUN after approval: duckdb, OpenXcom, vcmi, gstreamer, swupdate, sqlite counts; full gate suite.

## 6. Definition of done

§2.5 holds on poco; §3 rows green; cost reported; gates green.

CORPUS PATHS: poco, leveldb, nginx at ../legacy-codebases/<name>.
