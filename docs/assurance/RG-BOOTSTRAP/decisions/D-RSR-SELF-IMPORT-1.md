# D-RSR-SELF-IMPORT-1 — a Rust `use` whose ladder target is the source file itself binds nothing and keeps its unresolved row

Raised: 2026-10-04 by the manager packeting RUST-SELF-RESOLUTION-1, from the ladder replay over the v0.20.0 stores (`predict.py`).

**The problem in plain words.** Two shapes of `use` lead the module ladder back to the file that holds them. (1) A test module importing its own file: `rust/crates/daemon-runtime/src/livegraph_feed.rs:3599` `use crate::livegraph_feed::…` inside `#[cfg(test)] mod tests` — module path `livegraph_feed`, target `src/livegraph_feed.rs`, the source. (2) A sibling or `crate::` path whose module is written INLINE or re-exported, from the crate's entry file: vscode `cli/src/lib.rs` `use commands::…` where no `commands.rs`/`commands/mod.rs` exists — the ladder shortens to the entry point, which is `lib.rs`, the source. Counts (v0.20.0 stores): repo-graph 26 rows (18 relative-head, 8 sibling), vscode 125 (all sibling, almost all from `lib.rs`), zap-engine 0, codegraph 0.

**Options (reward / risk, by usefulness to an agent net of misdirection).**
- A — bind nothing when the target equals the source; the row stays unresolved with its classification (`this repository?`, basis `relative_import_target_unresolved` for the heads, the crate-internal heuristic for a sibling path). Reward: `rmap imports cli/src/lib.rs` never prints `cli/src/lib.rs  depth=1  static` 125 times; shape (1) stays an honest "not bound" row that costs the agent nothing (a file importing itself tells it nothing). Risk: shape (1) is a true fact the index could state; 18 rows on repo-graph are left unbound.
- B — bind both shapes as static self-edges. Reward: shape (1) is stated. Risk: shape (2) renders a file as its own import once per binding (125 rows on one vscode file), a no-information edge an agent must learn to skip; `modules deps` intra-module counts carry them.
- C — bind shape (1) (direct hit) and not shape (2) (reached by shortening). Reward: exact. Risk: a second rule and a second test family for 18 rows; the ladder must report HOW it hit.

Resolved: 2026-10-04 by the OPERATOR (in-place manager) as **A**; the human may override (C is the alternative if the self-reference is wanted).

## What this record rules
- Stage 3.6 returns no candidate when the probed FILE key equals the source file's key; the row is classified as every other unresolved Rust relative import is.
- The contract states the rule in one sentence under the `imports` section, with the repo-graph count.
