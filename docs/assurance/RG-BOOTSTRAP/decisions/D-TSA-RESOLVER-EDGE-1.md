# D-TSA-RESOLVER-EDGE-1 — TypeScript alias binding in the indexer reuses `repo-graph-import-resolver` (new dependency edge `indexer → repo-graph-import-resolver`)

Raised: 2026-10-03 by the audit round eight root-cause addendum (`docs/audits/2026-10-03-root-causes-v0.20.0.md`, RC-2 resolver side; commit `c98ed1ed`).

**What the seam read established.**
- The indexer's import ladder never consults the tsconfig aliases discovered at index time: `resolve_edges(edges, &ResolverIndex, …)` (`rust/crates/indexer/src/resolver.rs:510`) has no alias input and `ResolverIndex` (`:359-413`) no alias field; the aliases reach only the classifier (`orchestrator.rs:1134/:1524`), so every aliased TS import is `ImportsFileNotFound` (`resolver.rs:2574`) labelled `SpecifierMatchesProjectAlias`. ALIAS-SUSPICION-1 could only report, never bind.
- A complete alias resolver exists at `rust/crates/repo-graph-import-resolver/src/lib.rs:425 resolve_tsconfig_alias` (pattern capture, `baseUrl`, `candidate_paths` `:246-267`, `NotAnAlias | Resolved | Unresolved | Ambiguous`), consumed only by `repo-graph-livegraph/src/lib.rs:1280/:1311`; `indexer/Cargo.toml` does not depend on it. Every round-eight capture was served by SQLite (`fallback: LiveGraphUnavailable`).
- Field cost: amodx `admin` 333 / `renderer` 83, FRAKTAG `ui` 55, glamCRM `frontend` 231, hexmanos `frontend` 84 `@/` imports unbound — the intra-package import graph of the human's own TypeScript products is empty.

**Options presented (self-contained, reward and risk).**
- A — add the crate edge `indexer → repo-graph-import-resolver` and reuse `resolve_tsconfig_alias` / `candidate_paths`: one matcher, one extension list, both engines bind identically; one new graph edge, and the function's inputs must stay raw (specifier, alias config, inventory callback) so the indexer takes no LiveGraph shape.
- B — re-implement the matcher inside `indexer`: no new edge; a third extension list beside `file_resolution` (`resolver.rs:2678`) and `candidate_paths`, drifting the moment one is edited.
- C — move `resolve_tsconfig_alias` into a crate both already depend on: no duplication, no indexer→import-resolver edge; a relocation (permanent new placement) where a port suffices, touching LiveGraph's import path.

Resolved: 2026-10-03 by the HUMAN: **"Ok a"** — option A.

## What this record authorizes
- The slice TS-ALIAS-RESOLUTION-1 may add the dependency edge `indexer → repo-graph-import-resolver` and call `resolve_tsconfig_alias` from a TS-only ladder stage placed before the workspace stage (`resolver.rs:959`), binding `Inferred` under a new basis `tsconfig_paths_alias` with candidates recorded and ambiguity handled as the workspace stage does.
- Conditions carried into the packet: the function's inputs stay raw DTOs (no `repo-graph-livegraph` type crosses into `indexer`); the stored alias signal (`classification/src/types.rs:718-729 TsconfigAliases`) gains its anchor directory and `baseUrl` so `./src/*` can be resolved from `file_signals`; the `references` reader (RC-6, `repo-index/src/config.rs:185`) ships in the same slice; a full reindex is required and the packet states it; the component graph stays a DAG (the edge is checked against `repo-graph-import-resolver`'s own dependencies before admission).
- It does NOT decide RC-12 (`exports` subpaths read as a veto; D-TWR-REVIEW-1 revisit) nor the queue order; those remain the human's.

## Correction (appended 2026-10-03, operator) — the certainty named here was the operator's encoding, not the human's ruling
The sentence "binding `Inferred` under a new basis `tsconfig_paths_alias`" in "What this record authorizes" was the operator's wording when the human ruled only the crate edge ("Ok a"). It conflicted with the operator's later D-TSA-CERTAINTY-1 (static) and was caught by the TS-ALIAS-RESOLUTION-1 INPUT-1 review (F-TSA-03). The human has since ruled order and certainty together in D-TSA-RECORD-CONFLICT-1 (2026-10-03): before the workspace stage; STATIC where TypeScript's selection yields one indexed file, asserting the language's resolution; the basis recorded on the edge is `tsconfig_paths`, and the row may say `static (resolved through tsconfig paths)`. This record's authorization of the crate edge and of raw inputs stands; its certainty sentence is superseded.
