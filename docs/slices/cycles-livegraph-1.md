# CYCLES-LIVEGRAPH-1: Headless FILE import-cycle detection over `EdgeType::Imports` (Stage D)

Slice ID: CYCLES-LIVEGRAPH-1
Status: **DESIGN — framing ratified (2026-06-02). Implementation NOT started.**
Depends: IMPORTS-MODULE-INGEST-1 (the `EdgeType::Imports` graph in `PartitionIr`/LiveGraph), the trust
model (`AnswerEnvelope`; never exact-empty on a partial graph), `repo-graph-algorithms` (`find_sccs`).
Track: Stage D. NOT a default migration. NOT raw decommission. NOT MODULE aggregation.

## Framing (hard constraints)
```text
- NOT default migration: `rmap cycles` remains the SQLite MODULE-import-cycle authority, unchanged.
- NOT raw decommission: no nodes/edges retirement credit.
- NOT MODULE aggregation: no MODULE identity / MODULE->MODULE edges unless separately ratified.
- FIRST target = FILE import-cycle detection over EdgeType::Imports (the captured FILE import graph).
- This is a DIFFERENT question from `rmap cycles` (FILE-import vs MODULE-import); NEVER presented as a
  migration of it, and NOT comparable to it.
```

## Purpose
```text
Add a headless, honestly-scoped FILE import-cycle detector over the resident EdgeType::Imports graph,
returning a trust-labelled answer whose "no cycles" claim is scoped to the captured import graph — never
a claim about complete TS import semantics.
```

## Grounding (EXECUTED 2026-06-02, evidence-cited)
```text
Q1 LiveGraph sees Imports edges: livegraph-feed `feed_partition` calls `lg.load_partition(id, outcome.ir,
   language)` with the WHOLE PartitionIr (no edge filtering). So Slot.ir.edges includes the AstImport
   edges from IMPORTS-MODULE-INGEST-1. (repo-graph-livegraph-feed/src/lib.rs:74)
Q2 Adjacency lost on unload: `unload_partition` sets `Slot.ir = None`; `contribution()` retains only
   `defines` (nodes) + incoming `ref_counts`, NOT outgoing adjacency. So a NON-resident partition
   contributes NO outgoing Imports edges. SCC over Imports REQUIRES every contributing partition resident.
   (repo-graph-livegraph/src/lib.rs:194, :313)
Q3 SQLite cycles: `find_cycles(snapshot,"module")` = SCC over IMPORTS edges between MODULE nodes, output
   CycleResult{cycle_id,length,nodes:[{node_id,name,file:null}]}; CLI `rmap cycles [--json]`. A DIFFERENT
   graph (MODULE-import) than this slice's FILE-import graph — comparison is invalid. (queries.rs:942)
Q4 Completeness limits inherited from IMPORTS-MODULE-INGEST-1: the captured import graph is
   node-resolved + intra-partition + relative + resolved ONLY. Non-relative/unresolved/dynamic/re-export
   and cross-partition imports are NOT CAPTURED (producer + resolution limits). So a fully-resident
   captured graph is STILL an incomplete view of TS imports.
Note: `find_sccs(&[DirectedEdge{source,target}]) -> SccResult` (repo-graph-algorithms) is reusable.
```

## Ratified decisions (2026-06-02)

### D1 — Surface: A (headless only this slice)
```text
A  Headless LiveGraph `file_import_cycles()` + a Test-API surface (RATIFIED for this slice).
B  `rmap cycles --engine livegraph --kind file-import`  -> DEFERRED to a later slice (CLI exposure).
C  Compare to existing `rmap cycles`                    -> INVALID/REJECTED: different graphs (FILE-import
   vs MODULE-import); a compare report would assert a false equivalence.
```
No CLI, no `--engine`, no `--kind`, no default change in this slice. `rmap cycles` is untouched.

### D2 — Completeness: the `CapturedImportGraphScope` label
```text
Exact cycles / Exact no-cycles ONLY when, for the scope analyzed, EVERY contributing partition is:
  resident AND Fresh AND TS-primary,
AND the answer is explicitly scoped to the captured import graph (resolved-relative, intra-partition,
node-resolved FILE imports — the CapturedImportGraphScope).
A "no cycles" result is Exact ONLY WITHIN that scope. It is NEVER "no import cycles" in the TS sense:
the producer drops non-relative/unresolved/dynamic imports and cross-partition edges are not captured,
so an unseen import could close a cycle. Any non-resident / non-Fresh / non-TS partition in scope, or any
reliance on NOT-CAPTURED classes, degrades the answer to Partial/Unavailable. Use the label
`CapturedImportGraphScope`, NOT "all import cycles".
```

### D3 — Algorithm: SCC over resident `Imports` edges ONLY
```text
Tarjan SCC via repo-graph-algorithms `find_sccs`, over DirectedEdges built from RESIDENT partitions'
Slot.ir.edges WHERE edge_type == Imports (basis == AstImport). SCCs of size > 1 are cycles (plus
self-loops if a file imports itself, if that ever occurs). NO CALLS edges. NO REFERENCES edges. NO MODULE
edges. Endpoints are FILE node keys (the import graph is FILE -> FILE).
```

### D4 — Trust
```text
- A FOUND cycle is REAL within the captured graph (positive evidence; the edges exist + are resident).
- A NO-CYCLE result is Exact ONLY within the CapturedImportGraphScope, never complete TS import semantics.
- Missing producer classes (non-relative/unresolved/dynamic/re-export) and cross-partition gaps DEGRADE or
  SCOPE the answer — they must be reflected in the envelope (scope label + degradation reasons), never
  silently treated as "absent cycles".
- Single-partition synthetic fixtures are fully resident, so they yield Exact-within-scope answers; the
  multi-partition / producer-incompleteness honesty rides on D2/D4.
```

### D5 — Cache: no new persistence
```text
Warm-cache SCHEMA_VERSION is already v2 (IMPORTS-MODULE-INGEST-1) and carries Imports edges. This slice
reads the resident IR; it needs NO new persistence. (If evidence later contradicts, stop.)
```

## `CapturedImportGraphScope` — precise definition
```text
The set of import edges this substrate can currently see:
  edge_type == Imports, basis == AstImport, both endpoints node-resolved to AstFileScope nodes in the
  SAME resident partition, derived from a relative + producer-resolved import.
Excludes (NOT CAPTURED, hence outside any Exact claim): non-relative/package imports, unresolved relative
imports, dynamic import(), re-export-only edges, and cross-partition imports.
An answer labelled within this scope makes NO claim about edges outside it.
```

## Out of scope (hard guardrails)
```text
No `rmap cycles` change. No CLI surface (B deferred). No compare to rmap cycles (C invalid). No MODULE
nodes / MODULE->MODULE edges / module-boundary inference. No CALLS/REFERENCES cycle detection. No raw
nodes/edges decommission credit. No default migration. No new persistence.
```

## Acceptance (headless, EXECUTED later)
```text
1. LiveGraph `file_import_cycles()` exists, returning a trust-labelled AnswerEnvelope over a FILE-import
   cycle result type, scoped to CapturedImportGraphScope.
2. SCC runs over resident Imports edges ONLY (no Calls/References/Module edges contribute).
3. A FOUND file-import cycle on a synthetic multi-file fixture is reported (positive evidence) — needs a
   fixture with a real relative import cycle (A imports B, B imports A); add a minimal fixture if the
   current synthetic has no cycle.
4. A no-cycle result is Exact ONLY within CapturedImportGraphScope (labelled), never a bare "no cycles".
5. A non-resident / non-Fresh / non-TS contributing partition degrades the answer to Partial/Unavailable
   (unit-tested; live single-partition is always resident).
6. `rmap cycles` (SQLite MODULE-import) is unchanged; no CLI exposure of the new query this slice.
```

## Commit structure (proposed)
```text
1. support: LiveGraph `file_import_cycles()` (SCC over resident Imports edges) + CapturedImportGraphScope
   semantics + unit tests (found-cycle, no-cycle-within-scope, non-resident degrade). Possibly a minimal
   multi-file import-cycle fixture.
```

## Follow-up slices
```text
- CYCLES-LIVEGRAPH-CLI-1 (was D1/B): expose via `rmap cycles --engine livegraph --kind file-import`,
  labelled distinctly from SQLite MODULE-import cycles. Only after the headless API is proven.
- IMPORTS-EXTRACT-COMPLETENESS-1 / IMPORTS-XPART-RESOLUTION-1: close the producer + cross-partition gaps;
  only then can the scope widen toward complete TS import-cycle semantics.
- MODULE-AGGREGATION-1: MODULE-import cycles in LiveGraph (the actual `rmap cycles` parity path).
```

## References
- `docs/slices/imports-module-ingest-1.md` (the captured import graph; D2/D3/D4 completeness limits)
- `docs/slices/path-cycles-livegraph-2.md` (why `rmap cycles` could not migrate; the semantic boundary)
- `rust/crates/repo-graph-livegraph-feed/src/lib.rs:74` (feed loads the whole IR incl. Imports edges)
- `rust/crates/repo-graph-livegraph/src/lib.rs:194,:313` (contribution / unload — adjacency lost on unload)
- `rust/crates/graph-algorithms/src/scc.rs:108` (`find_sccs` — reused SCC)
- `rust/crates/storage/src/queries.rs:942` (SQLite MODULE-import cycles — the DIFFERENT, untouched query)
