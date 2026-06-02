# CYCLES-FILE-IMPORT-RENDER-1: FILE-import cycle human render (no "module-level" mislabel)

Slice ID: CYCLES-FILE-IMPORT-RENDER-1
Status: **RATIFIED (2026-06-02). Implementation in progress.** A shipped human-output correctness defect
(O2 from IMPORTS-XPART-ENUMERATION-1): `rmap cycles --engine livegraph --kind file-import` prints its
NON-empty result through the generic module renderer, so it says "module-level cycle", "(N modules)", and
"Run: rmap modules deps <module>" — a FALSE semantic label for FILE-import cycles (the project explicitly
distinguishes FILE-import from MODULE-import cycles, which are different graphs).
Depends: CYCLES-LIVEGRAPH-CLI-1 (the `--engine livegraph --kind file-import` surface), the daemon
file-import cycles response. Track: Stage D, RENDERING ONLY.

## Scope (hard guardrails)
```text
RENDERING ONLY. No graph/query/trust changes. No module aggregation. No default migration. No raw
decommission. The daemon response (JSON) is unchanged (it carries no human-only strings — scope is the
structured D5 object, cycles are node lists). SQLite `rmap cycles` (the MODULE renderer) is UNCHANGED.
```

## The defect (OBSERVED 2026-06-02, live)
```text
$ rmap cycles --engine livegraph --kind file-import      # two-package fixture, one cross-partition cycle
Scope: captured resolved-relative FILE import cycles [cross-partition(2)] (...)   # correct
Cycles: repo-graph
Snapshot: ...
1 module-level cycle found        # WRONG (file-import, not module)
Cycle 1 (2 modules):              # WRONG ("modules" -> files)
  packages/a/src/main.ts -> packages/b/src/foo.ts -> packages/a/src/main.ts
Run: rmap modules deps <module> to see specific import edges   # WRONG (no such relation here)
```
Root cause: the NON-empty LiveGraph branch in `run_cycles` falls through to
`CyclesResponse::render_human` (the generic MODULE renderer). The EMPTY case was already special-cased
("No FILE import cycles found within the captured scope.").

## Ratified decision — split the renderer by kind
```text
Add `CyclesResponse::render_human_file_import(&self)` (FILE vocabulary; NO "rmap modules deps" hint), and
route the LiveGraph `--kind file-import` path to it; the SQLite path keeps `render_human` (MODULE) verbatim.
The split key is the existing `livegraph` flag in `run_cycles` (engine==livegraph & kind==file-import).
The file-import renderer owns BOTH empty + non-empty (moved out of `run_cycles`, so both are unit-testable
in the presentation layer); the `run_cycles` early-return for the empty case is removed.
```

## Requirements (from the slice brief)
```text
1. FILE-import human output says "FILE import cycle(s)", not "module-level cycle(s)".
2. Member-count label says "file(s)", not "module(s)".
3. No "rmap modules deps" follow-up hint on the FILE-import path.
4. SQLite `rmap cycles` output UNCHANGED (render_human untouched).
5. JSON UNCHANGED (it carries no human-only strings).
6. Tests: empty file-import render; non-empty file-import render; SQLite module render unchanged.
```

## Build contract (PROPOSED)
```text
1. presentation/cycles.rs: render_human_file_import (header + FILE-vocabulary body, correct plurals, no
   module-deps hint) reusing render_cycle_chain; + 3 tests (empty/non-empty file-import, SQLite unchanged).
2. commands/graph.rs run_cycles: route the livegraph branch to render_human_file_import; remove the empty
   early-return (the renderer handles it); keep the structured Scope line; SQLite path unchanged.
3. live: dev-install; re-run the fixture cycles -> FILE vocabulary, no "module", no "rmap modules deps".
4. docs: completion + evidence.
```

## Out of scope
```text
The Scope line itself (already correct, structured, from D6). The nested-fixture display_name (O1 /
XPART-FIXTURE-STANDALONE-1). Any daemon/JSON change. SQLite MODULE rendering.
```

## References
- `rust/crates/rgr/src/presentation/cycles.rs` (`CyclesResponse::render_human` — the generic MODULE renderer)
- `rust/crates/rgr/src/commands/graph.rs` (`run_cycles` — the `livegraph` branch + the empty special-case)
- `docs/slices/imports-xpart-enumeration-1.md` (O2 — the defect this fixes)
- `docs/slices/cycles-livegraph-cli-1.md` (the `--engine livegraph --kind file-import` surface)
