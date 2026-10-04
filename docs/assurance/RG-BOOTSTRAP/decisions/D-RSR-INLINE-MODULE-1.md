# D-RSR-INLINE-MODULE-1 — a `use` inside an inline `mod x { }` resolves from THAT module; the extractor records the enclosing inline module names and the resolver appends them to the file's module path

Raised: 2026-10-04 by the RUST-SELF-RESOLUTION-1 document author (cycle 1, claude-opus-5-5): `DECISION_REQUIRED D-RSR-INLINE-MODULE-1`.

**The problem in plain words.** RG-REQ-006-L14 and the slice's rule 2.1 (2)(c) take the module path from the FILE (`src/a/b.rs` → `a::b`). Rust resolves `super`, `self` and a sibling path from the module that ENCLOSES the `use`; inside `mod tests { use super::X; }` written in `src/a/b.rs`, the enclosing module is `a::b::tests`, so `super` is `a::b` — the file itself — not `a`. Measured on the HEAD-built before-roots: the file-only rule binds 30 rows (repo-graph 29, codegraph 1) to a file Rust does not mean — e.g. `rgr/src/presentation/mod.rs:457` `use super::boundaries_summary::partition::Additive;` inside `mod http_count_coherence` would bind `rgr/src/lib.rs`. A static edge to the wrong file contradicts RG-REQ-001-L03 and RG-REQ-002-L01, and the slice's own replay (file-only) could not see it.

**Options (reward / risk, by usefulness to an agent net of misdirection).**
- A — the extractor records the enclosing inline `mod` names on the IMPORTS edge as an additive `metadata_json` key (present only when the `use` sits inside one or more inline module bodies; a `use` inside a function body is in the file's module and carries no key); the resolver appends those names to the file's module path before applying the head rule. Result on repo-graph: 15 correct bindings + 17 self-imports (bind nothing, D-RSR-SELF-IMPORT-1); codegraph's one row becomes a self-import. Risk: none measured; one key, one rule; L14 amended.
- B — the extractor flags a `use` inside an inline module and the resolver binds nothing for `super`/`self`/sibling heads there. Result: 30 rows stay unresolved. Risk: loses 15 true edges for the same key cost.
- C — accept and state as a limit. Risk: 30 wrong static edges that look certain — misdirection.

Resolved: 2026-10-04 by the OPERATOR (in-place manager) as **A** (the author's recommendation); the human may override. RG-REQ-006-L14 carries the amendment (operator, PROPOSED leaf).

## What this record rules
- Extractor: an IMPORTS edge from a `use` nested in inline `mod` bodies carries `inlineModulePath: ["outer", "inner"]` (outermost first); a top-level `use` carries no such key (every existing edge's metadata is byte-identical).
- Resolver stage 3.6: module path = the file's module path (2.1 (2)(c)) + the key's names, in order; then the head rule; a hit equal to the source file binds nothing.
- Oracle: RSR-C05's replay derives the inline nesting independently of the candidate — a brace-depth scan of the source file for `mod <name> {` bodies enclosing the `use` line — and predicts with it; the extractor's key is tested by RSR-C01 against the same fixtures.
