# IMPORTS-MODULE-INGEST-1: Module-import extraction authority → AST facts into `PartitionIr` (Stage D)

Slice ID: IMPORTS-MODULE-INGEST-1
Status: **DESIGN — extraction authority + D1 ratified (2026-06-02). Implementation NOT started.**
Depends: PATH-CYCLES-LIVEGRAPH-2 (the cycles blocker that motivated this), `repo-graph-ir` (EdgeType/EdgeBasis),
`repo-graph-scip-ingest` (the SCIP+AST join), `repo-graph-warm-cache` (PartitionIr serialization + schema_version).
Track: Stage D. This is a **data-shape / extraction-authority** slice. NOT a cycles slice; NO daemon/CLI work.

## Framing
```text
This decides WHERE module-import facts come from in the new substrate. It does NOT implement cycle
detection (that is the later CYCLES-LIVEGRAPH-1, gated on this). PATH-CYCLES-LIVEGRAPH-2 proved LiveGraph
cannot answer `rmap cycles` because the substrate has no IMPORTS edges and no module graph. This slice
ratifies the authority + IR shape for import facts so a faithful import graph can later exist in LiveGraph.
```

## Purpose
```text
Ratify that module-import facts are EXTRACTED (AST/tree-sitter), not inferred from SCIP references, and
define the FILE->FILE import-edge shape that enters PartitionIr.
```

## Grounding (EXECUTED 2026-06-02, evidence-cited)
```text
Q1 SQLite IMPORTS authority = tree-sitter AST extractor, NOT SCIP:
   TS  src/adapters/extractors/typescript/ts-extractor.ts:159 (import_statement)
   Rust rust/crates/ts-extractor/src/extractor.rs:1313 (extract_import)
   Emits FILE->FILE edges ({repo}:{resolved_path}:FILE); MODULE->MODULE is a DERIVED enrichment in the
   indexer (repo-indexer.ts:2239). Edge fields: source/target, type=IMPORTS, resolution(static/dynamic),
   extractor, location, metadata{rawPath, resolvedPath}. Unresolved -> unresolved_edges (IMPORTS_FILE_NOT_FOUND).
Q3 Forms: named/namespace/default/side-effect/re-export/type-only -> IMPORTS edges (if relative). Dynamic
   import('x') NOT handled (parsed as a call). Side-effect imports -> edge, no binding.
Q2+Q4 SCIP fidelity (CRUX): ingest reads only symbol_roles & 0x1 (Definition). scip-typescript emits ZERO
   Import/Read/Write roles -- docs/audits/scip-ts-parity-spike-1/findings.md:29 (spike M2). FileScopeReference
   is classified by AST file-scope origin, not roles, and is a SUPERSET (imports + any module-level ref).
   At the SCIP level an import is INDISTINGUISHABLE from an ordinary cross-file reference; no form, no
   re-export/side-effect/dynamic, no module->module identity.
Q5 Identity: IR has Partition{id,kind,root} (ir:129) + file-scope nodes (AstFileScope, ir:65). The extractor
   natively emits FILE identity; SQLite primary is FILE->FILE, MODULE derived.
```

## Ratified decisions (2026-06-02)

### Authority = C — AST/tree-sitter import extraction → `PartitionIr`
Import facts are **Layer 0–1 EXTRACTED AST facts**, language-scoped TS-primary. The Rust `ts-extractor`
(`extract_import`) already produces FILE→FILE import facts; this slice routes them into `PartitionIr`. The
SCIP ingest already joins `ts-extractor` AST facts (`call_sites`) — imports are another explicit AST-derived
fact class. **No SCIP role inference.**

### B (reconstruct from SCIP/FileScopeReference) — REJECTED (recorded)
```text
REFUTED ON EVIDENCE. scip-typescript emits zero import roles (spike M2); FileScopeReference is a superset
of imports (imports + module-level refs) and cannot separate them. Deriving "import facts" from references
would present a Layer-2 inference as a Layer-0 extracted fact — the false-trust failure the fact-certainty
model forbids. Do NOT call file-scope references import facts.
```

### D1 = FILE→FILE identity (MODULE deferred)
Import edges are **FILE→FILE**, reusing the existing `AstFileScope`/FILE identity. Rationale: the
extractor's native fact is FILE→FILE; SQLite's stored primary is FILE→FILE; it avoids inventing a
module-boundary authority before a ratified module model; it keeps imports Layer 0–1 (MODULE aggregation is
Layer 2 derived). **No `MODULE→MODULE` edges, no package/directory/tsconfig module-boundary inference in
this slice.** MODULE identity is deferred to **MODULE-AGGREGATION-1** (or `CYCLES-LIVEGRAPH-1`).

### Form handling
```text
- side-effect imports (import 'x'): VALID FILE->FILE edges even without bindings.
- type-only imports / re-exports: classified EXPLICITLY iff the extractor exposes enough metadata; else
  record the limit honestly (do not silently merge).
- dynamic import('x'): UNSUPPORTED/DEGRADED unless separately extracted. Must surface as an explicit
  degraded/absent class — NEVER a silently missing edge.
```

### Trust
Imports are AST-derived, TS-primary. Unresolved and dynamic imports MUST NOT become silently missing
edges — they carry an explicit resolution class (below). A missing import is honest degradation, surfaced.

## Build contract (the forced follow-on decisions — to confirm at implementation sign-off)

```text
1. IR shape (repo-graph-ir):
   - new EdgeType::Imports          (update the ir:72 "Imports intentionally absent" comment)
   - new EdgeBasis::AstImport       (proposed; the import edge's basis — distinct from Call/Reference)
   - source/target = file-scope FILE identities (existing AstFileScope nodes)
2. Import metadata (carried on the import edge or a sibling fact):
   - raw specifier (rawPath)
   - resolved path (resolvedPath)
   - import kind: named | default | namespace | side-effect | re-export | type-only  (when available)
   - resolution class: static-resolved | unresolved | unsupported-dynamic
3. Integration (repo-graph-scip-ingest):
   - extend ingest to JOIN ts-extractor import facts into PartitionIr (alongside the existing call_sites join)
   - NO SCIP role inference; imports come from the AST extractor only
4. Trust:
   - imports tagged AST-derived, language TS-primary
   - unresolved/dynamic imports => explicit resolution class, never a missing edge
5. Cache (repo-graph-warm-cache):
   - the PartitionIr serialized shape GROWS => warm-cache schema_version BUMP expected (SchemaMismatch ->
     graceful re-extract; existing mechanism, NOT new persistence)
   - support (IR + cache DTO) and consumer (ingest) MUST land together (persistence-completeness rule):
     write path + read path + cache schema in the same implementation slice, or explicitly co-committed.
```

## Out of scope (hard guardrails)
```text
No daemon/CLI cycles work. No LiveGraph cycle query. No MODULE->MODULE edges / module-boundary inference.
No SCIP role inference. No raw nodes/edges decommission credit (cycles still SQLite until CYCLES-LIVEGRAPH-1).
No dynamic-import extraction (degraded class only). No new persistence store (warm-cache schema bump only).
```

## Acceptance (for the eventual implementation, EXECUTED later)
```text
1. repo-graph-ir carries EdgeType::Imports + EdgeBasis::AstImport + import metadata; unit-tested.
2. repo-graph-scip-ingest joins ts-extractor import facts into PartitionIr (no SCIP role inference).
3. warm-cache DTO/schema carries import edges; schema_version bumped; round-trip + SchemaMismatch tested.
4. synthetic fixture proves: a FILE->FILE named/default import edge; a side-effect import edge (no binding);
   an unresolved import as an explicit resolution class; a dynamic import as unsupported-dynamic (NOT a
   missing edge).
5. NO LiveGraph cycle query in this slice; NO default migration.
```

## Commit structure (proposed)
```text
1. support: repo-graph-ir import edge class + metadata + warm-cache DTO/schema_version bump (co-committed) + tests
2. ingest:  repo-graph-scip-ingest joins ts-extractor import facts into PartitionIr + synthetic-fixture tests
```

## Follow-up slices
```text
- MODULE-AGGREGATION-1  : derive MODULE identity / MODULE->MODULE edges from the FILE import graph (Layer 2).
- CYCLES-LIVEGRAPH-1    : LiveGraph-backed module-import cycles, gated on the import graph existing in IR/LiveGraph.
- CALL-CYCLES-LIVEGRAPH-1 (optional): call/recursion cycles — a NEW query, never a migration of rmap cycles.
```

## References
- `docs/slices/path-cycles-livegraph-2.md` (the cycles blocker; why imports are the prerequisite)
- `docs/audits/scip-ts-parity-spike-1/findings.md:29` (spike M2 — scip-typescript emits zero import roles)
- `rust/crates/ts-extractor/src/extractor.rs:1313` (`extract_import` — the existing AST import authority)
- `rust/crates/repo-graph-ir/src/lib.rs:72` (EdgeType {Calls, References}; "Imports intentionally absent")
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` (the SCIP + ts-extractor `call_sites` join to extend)
- `src/adapters/indexer/repo-indexer.ts:2239` (SQLite MODULE->MODULE derivation — the deferred aggregation)
