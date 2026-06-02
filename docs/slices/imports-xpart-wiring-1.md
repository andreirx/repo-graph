# IMPORTS-XPART-WIRING-1: cross-partition import-edge overlay in the LiveGraph (Stage D)

Slice ID: IMPORTS-XPART-WIRING-1
Status: **BLOCKED on KEY-NAMESPACE-REPO-RELATIVE-1 (2026-06-02).** D1–D4 ratified, but a grounding probe found
that FILE/node keys are PARTITION-relative (`{repo}:{partition-relative}:FILE`), so multi-partition repos
COLLIDE (`packages/a/src/main.ts` == `packages/b/src/main.ts`) and the flat `defines` map is unsound. The
cross-partition overlay needs one collision-free repo-relative namespace across slots AND overlay. Fix the
key namespace first (KEY-NAMESPACE-REPO-RELATIVE-1), then resume. Implementation NOT started.
Depends: IMPORTS-XPART-RESOLUTION-1 (the pure `repo-graph-import-resolver` + `EdgeBasis::AstImportFileInventoryResolved`),
IMPORTS-EXTRACT-COMPLETENESS-1 (the `StaticUnresolved` observations), CYCLES-LIVEGRAPH-1 (`file_import_cycles`),
`repo-graph-warm-cache` (schema v3 -> v4).
Track: Stage D. The STATEFUL wiring for cross-partition resolved import edges. NO CLI/cycles migration. NO
module aggregation. NO raw decommission. NO persisted cross-partition edges.

## Framing
```text
Upgrade StaticUnresolved imports into node-resolved cross-partition FILE -> FILE edges that live ONLY in an
in-memory LiveGraph OVERLAY (never persisted; per-partition cache coherence F1). The existing
file_import_cycles() consumes the overlay so cross-partition import cycles become detectable. The pure
resolver (IMPORTS-XPART-RESOLUTION-1) does the resolution; this slice plumbs the source-file context and
maintains the overlay.
```

## Grounding (EXECUTED 2026-06-02)
```text
Q1/Q2 Daemon loads ONE partition at a time (livegraph_refresh; "default"); loaded PartitionIrs live in the
   LiveGraph `slots` (Slot.ir). No daemon-time auto-enumeration of a repo's packages (F2) — this slice
   maintains the overlay over WHATEVER partitions are loaded (manual multi-preload for validation;
   auto-enumeration stays a separate concern).
Q3 PartitionIr.import_observations LACKS source-file context (F3) -> D1 fixes it.
Q4 Global FileInventory = the FILE node keys across all loaded slots (LiveGraph `defines` retains FILE keys
   even on unload). MUST be a collision-free REPO-RELATIVE namespace: today a FILE key is
   `{repo}:{partition-relative-path}:FILE`, so two packages could collide on `src/index.ts`. The overlay
   builder MUST re-base to repo-relative using `partition.root` (a KEY design point, below).
Q5/Q6 Overlay lives in the LiveGraph (D2); recomputed eagerly on load/swap/unload (D3); coherent with the
   current loaded set; never serialized.
Precedent: the warm-cache value-facts SIDECAR shows independent per-partition artifacts; we choose D1=A
   (extend the observation) over a sidecar because source_file is intrinsic to an observation.
```

## Ratified decisions (2026-06-02)

### D1 — `ImportObservation` gains `source_file` (schema v3 -> v4)
`ImportObservation` (repo-graph-ir) gains `source_file` (the importing file's repo-relative path / FILE
identity). PRODUCER/INGEST-populated — never guessed later. Warm-cache `CacheImportObservationDto` +
`SCHEMA_VERSION 3 -> 4` (old caches -> SchemaMismatch -> re-extract through the existing manifest gate).
**Support + consumer co-committed** (the IR field + the cache DTO + the ingest population land together).

### D2 — overlay INSIDE the LiveGraph
The resolved cross-partition edges are a LiveGraph field (the overlay). `file_import_cycles()` reads ONE
surface (slot `Imports` edges UNION the overlay). The resolution LOGIC stays in `repo-graph-import-resolver`
(the LiveGraph is a CLIENT of it — like it is of `repo-graph-algorithms` for SCC); the LiveGraph stores the
resolver's OUTPUT. The overlay is NEVER serialized (the warm-cache DTO conversion excludes it).

### D3 — eager recompute on load / swap / unload
Every `load_partition` / `swap_partition` / `unload_partition` CLEARS + REBUILDS the overlay: build the
repo-relative `FileInventory` from all slots' FILE keys, build `ImportCandidate`s from all slots'
`import_observations` (resolution == `StaticUnresolved`, using `source_file`), run `resolve_imports`, store
the resolved edges. Cheap (hashmap lookups, no I/O); always coherent with the loaded set.

### D4 — trust / completeness
Overlay edges carry `EdgeBasis::AstImportFileInventoryResolved`. They feed `file_import_cycles()` like any
import edge; the EXISTING whole-graph completeness rule degrades the answer (Partial/Stale) when a
contributing partition is non-resident/stale (CYCLES-LIVEGRAPH-1 D2). The overlay is NEVER persisted.

## Key design point — repo-relative namespace
```text
FILE keys are `{repo}:{partition-relative-path}:FILE` (path relative to partition.root). For cross-partition
resolution the inventory + candidates MUST use a single REPO-RELATIVE namespace: prepend `partition.root` to
each partition-relative path before building the inventory / candidates (so `packages/a/src/index.ts` and
`packages/b/src/index.ts` do not collide). The resolved dst is then mapped back to the actual FILE key. The
overlay builder owns this re-basing; the pure resolver stays namespace-agnostic (it just matches strings).
```

## Build contract (the commit plan)
```text
1. source-file plumbing (support + consumer co-committed, D1):
   - repo-graph-ir: ImportObservation.source_file
   - repo-graph-warm-cache: CacheImportObservationDto.source_file + SCHEMA_VERSION 3 -> 4 + round-trip
   - repo-graph-scip-ingest: populate source_file (the importing file's repo-relative path) on each IR
     observation; update unit + synthetic-fixture tests
2. overlay (D2/D3):
   - repo-graph-livegraph: depend on repo-graph-import-resolver; an `xpart_import_edges` overlay field;
     rebuild_xpart_overlay() (repo-relative inventory + StaticUnresolved candidates -> resolve_imports ->
     overlay); call it from load/swap/unload; file_import_cycles() unions slot Imports + overlay; overlay
     EXCLUDED from any serialization. Unit tests: a hand-built 2-partition LiveGraph where pkg-a imports
     pkg-b cross-partition forms a file-import cycle; unload pkg-b -> overlay edge gone + answer degrades.
3. validation:
   - live: preload TWO synthetic partitions (manual, via `rmap dev livegraph-preload` twice) that
     cross-import, then `rmap cycles --engine livegraph --kind file-import` shows the cross-partition cycle.
     If a 2-package fixture is heavy, cover end-to-end with the hand-built LiveGraph unit test + document the
     live limitation.
4. docs.
```

## Out of scope (hard guardrails)
```text
Daemon multi-partition AUTO-ENUMERATION (F2 — discovering all a repo's packages) stays a separate concern;
this slice maintains the overlay over whatever partitions are loaded. NO persisted cross-partition edges. NO
CLI/cycles migration (the existing file-import cycle surface simply now sees the overlay). NO module
aggregation. NO package/tsconfig resolution (the resolver is relative+ext/index only).
```

## Acceptance (EXECUTED later)
```text
1. ImportObservation.source_file populated by ingest; warm-cache v4 round-trip preserves it; old v3 caches
   re-extract (SchemaMismatch).
2. LiveGraph overlay: a 2-partition hand-built graph where pkg-a/src/main.ts imports `../b/src/foo` (in
   pkg-b) yields a cross-partition Imports edge with basis AstImportFileInventoryResolved.
3. file_import_cycles() detects a cross-partition import CYCLE (pkg-a <-> pkg-b) via the overlay.
4. unload pkg-b -> overlay rebuilt without the edge; the answer degrades (Partial), never a stale edge.
5. overlay is never serialized (warm-cache round-trip of a partition with observations excludes overlay edges).
6. live (if a 2-package fixture is staged): two preloads + `rmap cycles --engine livegraph --kind file-import`
   shows the cross-partition cycle; else the unit test stands and the live gap is documented.
```

## Follow-up slices
```text
- IMPORTS-XPART-ENUMERATION-1 : daemon auto-discovery + loading of all a repo's packages (F2), so the
  overlay covers the whole repo without manual preloads.
- IMPORTS-PACKAGE-RESOLUTION-1 : tsconfig path aliases + package exports/types.
- MODULE-AGGREGATION-1 / CYCLES-LIVEGRAPH default migration : once the import graph is complete enough.
```

## References
- `docs/slices/imports-xpart-resolution-1.md` (the pure resolver; the F3 handoff this fulfils)
- `docs/slices/cycles-livegraph-1.md` (`file_import_cycles` — the overlay consumer)
- `docs/slices/imports-extract-completeness-1.md` (the StaticUnresolved observations)
- `rust/crates/repo-graph-warm-cache/src/lib.rs` (CacheImportObservationDto; SCHEMA_VERSION; value-facts sidecar precedent)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (LiveGraph slots/xref_epoch; load/swap/unload; file_import_cycles)
- `rust/crates/repo-graph-import-resolver/src/lib.rs` (resolve_imports; FileInventory; ImportCandidate)
