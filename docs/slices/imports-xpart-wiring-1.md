# IMPORTS-XPART-WIRING-1: cross-partition import-edge overlay in the LiveGraph (Stage D)

Slice ID: IMPORTS-XPART-WIRING-1
Status: **UNBLOCKED — amended (2026-06-02). Implementation NOT started.** The key-collision blocker is
CLOSED by KEY-NAMESPACE-REPO-RELATIVE-1 (`b72b075`): node/FILE keys are now repo-relative, so slots + overlay
share one collision-free namespace. D1–D4 stand. See **Amendment** for the re-evaluation (source_file is
still needed; cache is now v4 → v5 when source_file lands; no new boundary decision).
Depends: IMPORTS-XPART-RESOLUTION-1 (the pure `repo-graph-import-resolver` + `EdgeBasis::AstImportFileInventoryResolved`),
IMPORTS-EXTRACT-COMPLETENESS-1 (the `StaticUnresolved` observations), CYCLES-LIVEGRAPH-1 (`file_import_cycles`),
KEY-NAMESPACE-REPO-RELATIVE-1 (repo-relative keys; cache now v4), `repo-graph-warm-cache` (schema v4 -> v5).
Track: Stage D. The STATEFUL wiring for cross-partition resolved import edges. NO CLI/cycles migration. NO
module aggregation. NO raw decommission. NO persisted cross-partition edges.

## Amendment (2026-06-02, post KEY-NAMESPACE-REPO-RELATIVE-1)
```text
UNBLOCKED: keys are now repo-relative (b72b075). The F1 collision blocker is CLOSED — slots + overlay share
one namespace; a cross-partition target FILE key (e.g. repo:packages/b/src/foo.ts:FILE) is unambiguous.

RE-EVALUATION (is source_file still needed?): YES.
  - A cross-partition edge is src_file -> dst_file; the file-import SCC needs BOTH endpoints.
  - dst is now derivable: the producer's resolved_path is REPO-RELATIVE and correct cross-partition
    (resolve_import_path("../b/src/foo", "packages/a/src/main.ts") = "packages/b/src/foo"), so the resolver's
    normalize_join(dirname(source_file), raw_specifier) lands the right target — exactly because source_file
    is repo-relative now.
  - src is NOT recoverable from the IR ImportObservation ({raw_specifier, resolution, modifiers}): the
    observations are flattened per-partition, losing WHICH file imported. So the importing file's identity
    must be carried -> ImportObservation.source_file (the importing file's repo-relative path = the doc's
    ingest key_path). This is the F3 gap, now closed by extending the observation.

DECISIONS UNCHANGED: D1 ImportObservation.source_file (ingest-populated, repo-relative); D2 overlay inside
LiveGraph; D3 eager rebuild on load/swap/unload; D4 EdgeBasis::AstImportFileInventoryResolved + degrade on
non-resident, never persist.
CACHE: KEY-NAMESPACE took SCHEMA_VERSION to v4; source_file lands -> v5 (support+consumer co-committed).
NO PERSISTED OVERLAY (confirmed). DAEMON STILL LACKS ENUMERATION (F2) -> validation is manual multi-preload
OR unit-level (hand-built 2-partition LiveGraph). NO NEW BOUNDARY DECISION -> proceed to implement.
```

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

### D1 — `ImportObservation` gains `source_file` (schema v4 -> v5)
`ImportObservation` (repo-graph-ir) gains `source_file` (the importing file's repo-relative path / FILE
identity). PRODUCER/INGEST-populated — never guessed later. Warm-cache `CacheImportObservationDto` +
`SCHEMA_VERSION 4 -> 5` (old caches -> SchemaMismatch -> re-extract through the existing manifest gate).
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

## Key design point — repo-relative namespace (now RESOLVED upstream)
```text
KEY-NAMESPACE-REPO-RELATIVE-1 made FILE/node keys repo-relative (`{repo}:packages/a/src/main.ts:FILE`), so
the overlay builder needs NO re-basing: it builds `FileInventory::from_file_keys` DIRECTLY from the slots'
(already repo-relative) FILE keys, and `ImportObservation.source_file` is already the repo-relative importing
path. `ImportCandidate{ source_file_key = {repo}:{source_file}:FILE, raw_specifier }`; the resolver derives
the repo-relative target (correct cross-partition, because source_file is repo-relative). The pure resolver
stays namespace-agnostic (string matching).
```

## Build contract (the commit plan)
```text
1. source-file plumbing (support + consumer co-committed, D1):
   - repo-graph-ir: ImportObservation.source_file
   - repo-graph-warm-cache: CacheImportObservationDto.source_file + SCHEMA_VERSION 4 -> 5 + round-trip
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
