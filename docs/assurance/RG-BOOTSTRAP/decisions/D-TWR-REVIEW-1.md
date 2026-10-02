# D-TWR-REVIEW-1 — The four decisions of TS-WORKSPACE-RESOLUTION-1's first implementation review

Raised: 2026-10-03 by the implementation review of TS-WORKSPACE-RESOLUTION-1 (INPUT-1, admission 1, cycle 1; reviewer codex gpt-6-sol; decisions D-TWR-NOTE, D-TWR-ENTRY-ORIGIN, D-TWR-INVENTORY, D-TWR-ORACLE in `.agent-manager/slices/TS-WORKSPACE-RESOLUTION-1/review-0.json`, local). Seven of nine checks passed; TWR-C03 and TWR-C07 failed; two contract conflicts were found by source inspection.

Resolved: 2026-10-03 by the OPERATOR (in-place-manager), each as the reviewer's recommended option A, each verified against the source or the tool. The human may override any of the four.

## 1. D-TWR-NOTE = A — the LiveGraph-route note states only what the rule does
The approved slice text made the human renderer say, on three renders (`rgr/src/presentation/imports.rs:166-178`, emitted at `:391`, `:538`, `:668`), that every `WorkspaceLocalUnedgeable` import is recorded as an inferred source-entry import by the default route. That is false in two verified cases:
- `repo-graph-import-resolver/src/lib.rs:70-114` gives that label to workspace **subpath** imports too (amodx `admin/src/components/editor/Toolbar.tsx:10` `import { getPluginList } from "@amodx/plugins/admin";`), which this slice deliberately leaves unresolved;
- a root import whose declared target IS indexed (storybook `code/frameworks/ember/template/cli/Button.stories.js:1` `import { linkTo } from '@storybook/addon-links';`) is left unresolved by the rule itself.

Ruling: the note says the default route *may* record an inferred source-entry import, only when the workspace-source rule applies (root specifier, no indexed declared target); otherwise it tells the agent to open the import and look (the human's standing rule, D-TEST-UNDETERMINED-1's wording family). Tests cover the subpath and the indexed-declared-target counterexamples on all three renders. Rejected: B (one shared rule on both backends now — a new cross-backend boundary, outside this slice); C (defer — FRAKTAG's relationship stays hidden).

## 2. D-TWR-ENTRY-ORIGIN = A — declared entries carry their origin; completions apply to `main` only
The accepted rule applies the Node completion forms (`.js`, `.json`, `.node`, `/index.js`) to `main` only; an `exports` target is a literal path. The candidate (`indexer/src/workspace_import.rs:113-117`) applies them to every declared target, because the boundary field `npm_declared_entries: Vec<String>` (compose → indexer) has lost whether an entry came from `main` or `exports`. Counterexample (the reviewer's): `exports` target `./lib/index`, an indexed `lib/index.js`, one indexed `src/index.ts` — the approved rule binds the source entry as inferred; the candidate declines it.

Ruling: the declared-entry carrier states the origin as a sum type (`Main(path)` | `Export(path)`), never a boolean beside a string; completion is a function of the variant. This is a data-shape change on an internal boundary between two allocated crates (repo-index compose → indexer), implementing the already-ratified rule, so it is the operator's; the shape is the smallest that encodes the rule. Rejected: B (ratify completions for `exports` too — silently declines valid inferred relationships and changes the approved rule); C (defer).

## 3. D-TWR-INVENTORY = A — the reader inventory test joins the allocation
TEST-EDGE-SCOPE-1B's `storage/tests/import_reader_inventory.rs` guards that every IMPORTS reader is listed with its partition disposition. This slice adds a reader (`storage/src/crud/module_edges_support.rs::checked_workspace_import_sites`, so `deps list`/`deps why` keep the import) and must list it; the test also carries a row that is now inaccurate (`get_external_imports_for_snapshot` "reads unresolved edges only"). Allocation amendment A-1 adds the test file; its disposition record is updated by amendment, not by rewriting. Rejected: B (remove the deps reader — FRAKTAG falsely reads "no static import", RG-REQ-006-L07); C (defer).

## 4. D-TWR-ORACLE = A — the operator's store-diff tool normalizes `unresolved.target_key`
`agent-manager/scripts/rg-store-diff.py` normalizes stable keys (repo uid stripped) for nodes and edge endpoints, but not `unresolved.target_key`; across a fresh repository uid an unchanged FRAKTAG `./index.css` import reads as removed + added (`/tmp/twr-c07-run.txt`: unresolved rows 3/1 against the expected 1/1). TWR-C07's own SQL has the same gap. Ruling: fix the tool (operator-owned; its other users — PRB1, DGC1A, CIB1, 1B — compared stores whose unresolved target keys carried no repo uid, so their verdicts stand; the fix is re-validated on their retained before/after pairs where available), correct C07's query by oracle correction, rerun C07 on all four corpora. The builder's supplementary one-off oracle is evidence, not the check. Rejected: B (accept the one-off oracle as the check); C (defer).

## Consequences
INPUT-2: A-1 (inventory test path, note tests), OC-1 (C07 query), OC-2 (the note's approved text); the origin sum type is a code change inside allocated paths, described in §2.1. The candidate is preserved and re-applied at admission 2.
