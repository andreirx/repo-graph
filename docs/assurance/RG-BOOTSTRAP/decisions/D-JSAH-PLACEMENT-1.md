# D-JSAH-PLACEMENT-1 — the ambiguity payload builder and its human renderer live in sibling modules of the over-guardrail files

Raised: 2026-10-05 by the operator while allocating JAVA-SYMBOL-AMBIGUITY-HINT-1.

**The problem in plain words.** The two files that own today's behaviour are far over the 500-line guardrail (repo-graph CLAUDE.md:87 — no new responsibility in a file over 500 lines): `daemon-runtime/src/dispatch.rs` (10,544 lines; `parse_ambiguous_matches` at :228, four call sites: `handle_callers`, `handle_callees`, `handle_path` from/to) and `rgr/src/commands/graph.rs` (1,633 lines; `handle_daemon_error` renders the `AmbiguousSymbol` error). The fix needs a builder that looks each key up in the store (file, line, signature — `query_symbol_by_field`) and a human renderer with value tests.

**Options (reward / risk).**
- A — a sibling module `daemon-runtime/src/dispatch_ambiguous_matches.rs` (`ambiguous_matches(storage, snapshot_uid, keys) -> Value`, replacing `parse_ambiguous_matches`, which is deleted — the precedent is `dispatch_explain_alias.rs`) and a sibling module `rgr/src/commands/ambiguous_matches.rs` (`render_ambiguous_matches(command, data) -> String`, called from the existing `AmbiguousSymbol` branch of `handle_daemon_error`). Both have exactly one production caller family and a test seam. Reward: dispatch.rs loses a function; graph.rs's branch shrinks to one call; the renderer is unit-tested on JSON values. Risk: two new files for ~80 lines of logic.
- B — edit in place. Reward: no new file. Risk: adds a store lookup to dispatch.rs and a multi-line renderer to graph.rs — new responsibilities in over-guardrail files; the renderer is testable only through the CLI.

**Resolved: 2026-10-05 by the OPERATOR (in-place manager) as A**; the human may override. `agent/src/explain/mod.rs` (1,502 lines) changes only the body of the existing `classify_type_constructor_collision` and the value of `ConstructorHint.cursor` — no new responsibility.
