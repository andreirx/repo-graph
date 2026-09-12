# EXPLAIN-CYCLES-HONEST-1 — `explain`'s Import-cycles block draws arrows only over a verified walk

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q4 (HIGH; never worked — the honest form shipped twice for `cycles` and `orient` and never reached `explain`). CODE slice: `rgr/src/presentation/explain_sections.rs::render_cycles` (~15 lines) + tests. Maturity: MATURE (`explain`). Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-003-L01 (`explain` draws no cycle arrow without a verified walk), RG-REQ-002-L01 (no invented relationship — the cycle-arrow half), RG-REQ-004-L07 (the unordered form on every surface).

**Changes:** `explain <symbol|file|path>` human output replaces `Cycle N: A -> B -> C …` with `Cycle N (K modules): members (unordered): A, B, C, … (+ M more)` whenever `walk` is absent — today every explain serve. JSON unchanged (`modules` + `walk: null` already).

**Preserves (§3):** `rmap cycles` and `orient` byte-identical; RG-REQ-005-L06 (confidence), RG-REQ-010-L01 (seed block), RG-REQ-012-L07 (anchors), the explain output-contract tests other than the cycle block, both focus kinds and both serving engines.

## 1. Problem (ROOT-CAUSED — RC-4)

`explain_sections.rs:226-249 render_cycles` joins `item["modules"]` — the SCC member set, SORTED by `canonicalize_cycles` — with `" -> "`, asserting edges that do not exist (vcmi `AI/BattleAI -> AI/EmptyAI …`: `grep EmptyAI AI/BattleAI/` → 0). Both focus reads (`agent/src/explain/mod.rs:562-588`, `:882-906`) set `walk: None`; the LiveGraph route (`explain_lg_serve.rs:133`) carries no walk either. CYCLE-HONESTY-1 (`cycles/walk.rs::render_unordered`) and COHERENCE-3 (`orient_sections.rs` + `format_cycle_anchor`) built the rule; `git show --stat 55ab942` touches `explain_sections.rs` zero times.

## 2. Contract

1. `render_cycles` draws arrows only from `item["walk"]` via the shared `format_cycle_anchor` (strict validation; `None` on drift); with `walk` absent/null/empty/malformed it renders the unordered form mirroring `cycles/walk.rs::render_unordered`, with size and `(+ N more)` collapse.
2. One function covers symbol-focus, path-focus, SQLite and LiveGraph serves (all route through it).
3. Optional follow-up recorded, not built: populate `walk` on the focus-scoped reads so explain can draw the real ring.
4. Outward proof: vcmi `explain CGHeroInstance` and django `explain BaseHandler.get_response` contain zero `->` in the Import-cycles block and read `members (unordered)`; `rmap cycles` and `rmap orient` on repo-graph byte-identical.

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-004-L07, RG-REQ-003-L05 | `cycles`/`orient` rings and unordered forms | `cycles/walk.rs` tests; `orient_tests.rs::cycle_anchor_*` and `cross_surface_cycle_walk_agrees_*` green; repo-graph `cycles`/`orient` byte-identical |
| RG-REQ-005-L06 | confidence lines | `explain.rs::no_high_confidence_beside_no_match` |
| RG-REQ-010-L01 | semantic candidates block | `explain.rs::semantic_no_match_renders_labeled_candidates_in_human_mode` |
| RG-REQ-012-L07 | anchors in explain tiers | `explain.rs::tier0_*`, `tier1_*` |
| RG-REQ-012-L06 | JSON unchanged | `cli_output_mode.rs::explain_json_mode_returns_valid_envelope`; `walk: null` still emitted |
| explain output contract | every other section byte-identical | the `presentation/explain.rs` test family green; a before/after diff of `explain` on leveldb shows only the cycles block changed |

## 4. Stop conditions

Frozen: wire, storage, cycle computation, exit codes, `cycles`/`orient` renderers. If `format_cycle_anchor` cannot be reused without a signature change affecting `orient`, render the unordered form unconditionally for explain and record the follow-up — do not change the shared formatter. STANDING HONESTY RULES. Do NOT commit.

## 5. Validation (ORDERED)

1. Failing tests FIRST: `render_cycles` with `walk: null` → no arrow + unordered form; with a valid 3-member walk → ring; malformed walk → unordered.
2. `cargo test -p repo-graph-rgr --lib presentation`; fmt/clippy.
3. Live proof on isolated leveldb (small; its 4-module cycle): `explain leveldb::DBImpl::Recover` block before/after; `cycles`/`orient` byte-identical.
4. `build-N.md`.

## 6. Definition of done

§2.4 holds; §3 green; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb (vcmi/django proofs OPERATOR-RUN on retained copies).
