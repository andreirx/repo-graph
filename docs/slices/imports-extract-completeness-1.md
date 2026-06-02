# IMPORTS-EXTRACT-COMPLETENESS-1: widen TS import extraction (observations, no inference) (Stage D)

Slice ID: IMPORTS-EXTRACT-COMPLETENESS-1
Status: **DESIGN — Decision A + direction ratified (2026-06-02). Implementation NOT started.**
Depends: IMPORTS-MODULE-INGEST-1 (the captured FILE import graph + the NOT-CAPTURED limits this closes;
the no-dangling-symbolic-edges rule D4), `repo-graph-ir` / `repo-graph-warm-cache` (PartitionIr shape +
schema_version), `repo-graph-ts-extractor` (the producer being widened).
Track: Stage D. A **producer-completeness** slice. NOT a cycles impl. NOT a CLI migration. NOT decommission.

## Framing (hard constraints)
```text
- Producer-completeness for TS imports ONLY. No LiveGraph cycle implementation. No CLI migration. No raw
  nodes/edges decommission.
- Goal: widen the Rust `ts-extractor` import output so the IR can DISTINGUISH captured, unresolved,
  package, dynamic, re-export, and type-only imports WITHOUT inference (each from a producer-observed fact,
  never a downstream guess).
- This is the producer-API widening that IMPORTS-MODULE-INGEST-1 D3 explicitly deferred (it stopped rather
  than widen the producer mid-slice).
```

## Purpose
```text
Emit explicit import OBSERVATIONS from `ts-extractor` (raw syntactic facts per import/export-from/dynamic
statement) so ingest can CLASSIFY each import's resolution without inference, and so non-node-resolved
imports become honest completeness evidence — never dangling graph edges.
```

## Grounding (EXECUTED 2026-06-02, evidence-cited)
```text
Q1 ExtractionResult: { nodes, edges, metrics, import_bindings, resolved_callsites } (extractor.rs:282) —
   public. No import-completeness/observation field today.
Q2 ImportBinding { identifier, specifier, is_relative, location, is_type_only, imported_name, kind } where
   ImportKind = { Named, Default, Namespace } (classification/types.rs). `extract_import` (extractor.rs:1313)
   emits a `Resolution::Static` IMPORTS edge ONLY for relative + resolved imports; bindings for every import
   clause; non-relative / unresolved -> NO edge; side-effect -> edge-if-resolved but no binding.
Q3 Legacy TS extractor (src/adapters/extractors/typescript/ts-extractor.ts): same forms; unresolved imports
   were tracked SEPARATELY in the SQLite `unresolved_edges` table (a side store), NOT in the edge output.
   The Rust extractor currently DROPS them entirely. (So "richer" = it persisted unresolved elsewhere.)
Q4 Detectability by the current parser:
   relative resolved   -> edge + binding (Static)                                    DETECTED
   unresolved relative -> binding (is_relative) but resolve_import_path None -> dropped  DETECTABLE (re-emit)
   package/non-relative-> binding (is_relative=false), no edge                        DETECTABLE
   side-effect         -> import_statement with no import_clause (no binding)         DETECTABLE (node)
   re-export           -> `export_statement` w/ source -> extract_import (NOT flagged) DETECTABLE (node.kind)
   dynamic import()    -> a call_expression; NOT parsed as an import today            DETECTABLE (NEW parse)
   type-only           -> is_type_only flag on binding                               DETECTED
Q5 Resolution ownership: SPLIT today. The extractor normalizes the relative path (extensionless); the
   INGEST does node-resolution (match against partition FILE nodes, IMPORTS-MODULE-INGEST-1 D4). This
   slice keeps that split: the extractor OBSERVES (syntax); ingest CLASSIFIES (incl. node-resolution).
```

## Ratified decision: A — observations live in `PartitionIr.import_observations`
```text
A  PartitionIr.import_observations: Vec<ImportObservation>            (RATIFIED)
   - completeness evidence is a first-class field of the partition artifact; the warm cache persists it
     (schema bump); no dangling graph edges.
B  edge metadata on dangling/symbolic import edges                    (REJECTED)
   - directly contradicts IMPORTS-MODULE-INGEST-1 D4 ("no symbolic dangling edge"). A non-node-resolved
     import must NEVER appear as a graph edge.
C  separate sidecar from ingest                                       (REJECTED)
   - a NEW persistence artifact; the warm cache already persists PartitionIr. Keep evidence in the artifact.
```

## Design direction (the build contract — confirm at implementation sign-off)
```text
1. ts-extractor (the producer widening):
   - emit ONE `ImportObservation` per import / export-from / dynamic-import statement into a NEW public
     `ExtractionResult.import_observations`. RAW SYNTACTIC FACTS only (no classification):
       { raw_specifier, resolved_path: Option<String> (relative-resolved only), is_relative, is_type_only,
         is_re_export (node.kind == export_statement), is_side_effect (no import clause), is_dynamic,
         location }
   - ADD dynamic `import()` detection (a call_expression whose callee is `import`) -> an observation with
     is_dynamic = true. (The only NEW parsing.)
   - KEEP the existing resolved IMPORTS edges (`extract_import`) for compatibility — observations are
     ADDITIVE; the edge path is unchanged.
2. repo-graph-ir + repo-graph-warm-cache:
   - PartitionIr gains `import_observations: Vec<ImportObservation>` (an IR DTO mirroring the producer
     observation, MINUS producer-only noise). Warm-cache DTO + SCHEMA_VERSION bump (v2 -> v3); round-trip test.
3. repo-graph-scip-ingest (classification, NO inference):
   - for each observation, CLASSIFY (each from an observed fact):
       StaticResolved    : relative + resolved_path + node-resolves in THIS partition -> EdgeType::Imports
                           (the ONLY class that becomes a graph edge; IMPORTS-MODULE-INGEST-1 D4 unchanged)
       StaticUnresolved  : relative + (no resolved_path OR target file not in partition) -> observation only
       PackageExternal   : non-relative specifier                                       -> observation only
       DynamicUnsupported: is_dynamic (no static target)                                -> observation only
       ReExport          : is_re_export (MODIFIER; may combine with a resolution class) -> observation flag
       TypeOnly          : is_type_only (MODIFIER; may combine)                         -> observation flag
   - Do NOT synthesize edges for unresolved/package/dynamic imports as if they point to local FILE nodes.
   - Non-node-resolved observations are COMPLETENESS EVIDENCE / degradation metadata, NOT graph edges.
```

## Classification taxonomy (record the axes)
```text
RESOLUTION axis (mutually exclusive): StaticResolved | StaticUnresolved | PackageExternal | DynamicUnsupported
MODIFIER flags (orthogonal, may combine with any resolution): ReExport, TypeOnly
=> e.g. a `export type { X } from './y'` that resolves is StaticResolved + ReExport + TypeOnly. Only the
   StaticResolved RESOLUTION (and only when node-resolved intra-partition) yields an EdgeType::Imports edge;
   the modifiers and the non-StaticResolved classes live in import_observations.
```

## Out of scope (hard guardrails)
```text
No LiveGraph cycle implementation. No CLI surface. No `rmap imports`/`stats` migration. No raw decommission.
No MODULE aggregation. No cross-partition node resolution (that is IMPORTS-XPART-RESOLUTION-1 — a
StaticResolved import whose target file is in ANOTHER partition stays StaticUnresolved-from-this-partition's
view here, recorded as an observation). No widening of NON-import extraction.
```

## Acceptance (EXECUTED later)
```text
1. ts-extractor ExtractionResult.import_observations exists; one observation per import/export-from/dynamic
   statement; dynamic import() detected (is_dynamic). Existing resolved IMPORTS edges unchanged. Unit-tested.
2. repo-graph-ir PartitionIr.import_observations + warm-cache DTO + SCHEMA_VERSION v2->v3 + round-trip test.
3. ingest classifies each observation; ONLY StaticResolved-node-resolved -> EdgeType::Imports; the rest ->
   import_observations (NO graph edges). Unit + synthetic-fixture: the fixture's relative import is
   StaticResolved (edge); add fixtures/observations for unresolved, package, dynamic, re-export, type-only.
4. NO dangling symbolic edges anywhere (re-assert IMPORTS-MODULE-INGEST-1 D4).
5. counts surfaced (observations by class) for honest completeness reporting.
```

## Commit structure (proposed)
```text
1. producer: ts-extractor ImportObservation + dynamic-import detection + ExtractionResult field + tests.
2. support:  repo-graph-ir PartitionIr.import_observations + warm-cache DTO/schema bump (co-committed) + tests.
3. ingest:   classify observations -> edges (StaticResolved only) + import_observations + fixture tests.
4. docs:     status/evidence; update IMPORTS-MODULE-INGEST-1's NOT-CAPTURED list (now CAPTURED-as-evidence).
```

## Follow-up slices
```text
- IMPORTS-XPART-RESOLUTION-1 : resolve StaticResolved targets ACROSS partitions (+ index files / package
  exports) so cross-partition imports become edges, not just observations.
- MODULE-AGGREGATION-1 / CYCLES-LIVEGRAPH-1 (module) : once the import graph + observations are complete
  enough, derive MODULE cycles with honest scope.
- IMPORTS-LIVEGRAPH-1 / STATS-LIVEGRAPH-1 : migrate `rmap imports`/`stats` (gated on this completeness).
```

## References
- `docs/slices/imports-module-ingest-1.md` (the captured graph; D3 deferral of producer widening; D4 no-dangling-edges)
- `docs/slices/sqlite-raw-decommission-readiness-2.md` (why this gates the cycles + imports/stats threads)
- `rust/crates/ts-extractor/src/extractor.rs:282` (ExtractionResult), `:1313` (`extract_import`), `:207` (export-from)
- `rust/crates/classification/src/types.rs` (ImportBinding / ImportKind)
- `rust/crates/repo-graph-ir/src/lib.rs` (PartitionIr / ImportEdgeMeta / ImportResolution), `repo-graph-warm-cache` (SCHEMA_VERSION)
