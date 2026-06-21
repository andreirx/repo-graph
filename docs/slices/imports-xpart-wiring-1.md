# IMPORTS-XPART-WIRING-1: cross-partition import-edge overlay in the LiveGraph (Stage D)

Slice ID: IMPORTS-XPART-WIRING-1
Status: **IMPLEMENTED (2026-06-02), headless.** D1–D4 landed + D5 (scope flag set, user-ratified). Overlay
edges are in-memory only; `file_import_cycles()` detects cross-partition cycles via the overlay. The
multi-partition DAEMON path stays gated behind IMPORTS-XPART-ENUMERATION-1 (F2), so live validation covers
single-partition regression + the v5 re-extraction only — the cross-partition path is covered headlessly.
See **Completion**. The key-collision blocker was CLOSED by KEY-NAMESPACE-REPO-RELATIVE-1 (`b72b075`).
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

## Completion (implemented 2026-06-02, EXECUTED)

Commits: `0aef2fe` (1/4 source_file plumbing) + `a334e56` (2/4 overlay) + this doc (3/4).

### What landed
```text
repo-graph-ir            : ImportObservation.source_file (importing file's repo-relative path = the
                           cross-partition edge src the flattened observation otherwise loses).
repo-graph-warm-cache    : CacheImportObservationDto.source_file + SCHEMA_VERSION 4 -> 5 (old caches ->
                           SchemaMismatch -> re-extract); round-trip carries source_file.
repo-graph-scip-ingest   : classify PER-DOC (not flattened), stamping each doc's repo-relative key_path
                           as source_file on every IR observation.
repo-graph-import-resolver: FileInventory::file_key_for made PUBLIC (the overlay resolves the importing
                           file's src key from the SAME inventory).
repo-graph-livegraph     : CLIENT of the resolver; `xpart_overlay` field; rebuild_xpart_overlay() on
                           load/swap/unload (D3); xpart_import_edges() read accessor; file_import_cycles()
                           unions intra AstImport edges WITH the overlay; ImportCycleScope = D5 flag set.
```

### D5 ratification — scope as a FLAG SET (user decision 2026-06-02)
```text
ImportCycleScope: single enum variant -> closed struct
  { captured_resolved_relative, intra_partition, cross_partition, xpart_edge_count }.
Rationale (user): a new single variant loses single-partition precision; an overlay bool beside a stale
enum name forces readers to compose fields; a flag set is honest + extensible without overclaiming "all
imports". cross_partition = xpart_edge_count > 0 (CONTRIBUTION semantics): false means no cross-partition
edge was in the SCC universe, NOT that resolution was skipped. Scope describes the UNIVERSE queried; the
answer class + `missing` carry completeness.
```

### Divergence (recorded)
```text
src key via inventory lookup, NOT string reconstruction: the ratified text wrote
  ImportCandidate.source_file_key = {repo}:{source_file}:FILE.
Implementation instead looks it up via FileInventory::file_key_for(source_file) — identical key, one
source of truth, and returns None (skip) when the importing file is not resident (no src node to anchor an
edge). No behavior difference for a resident importer. resolver: file_key_for made public; no logic change.
```

### TECH DEBT — must address WITH IMPORTS-XPART-ENUMERATION-1
```text
The daemon serializes the cycles scope as a HARD-CODED string (livegraph_feed.rs:934
"CapturedResolvedRelativeIntraPartition") and the CLI renders that string (graph.rs:830). This slice left
BOTH untouched (the "no CLI" guardrail). It stays ACCURATE only because the daemon loads ONE partition at a
time (F2): the overlay contributes zero edges in any daemon-served answer, so cross_partition is always
false there. WHEN IMPORTS-XPART-ENUMERATION-1 enables multi-partition daemon serving, the daemon MUST emit
the structured env.scope (the D5 flag set) and the CLI MUST render it — else the CLI under-reports
cross-partition coverage (a false trust claim). (Pre-existing latent: the daemon scope was ALREADY
hard-coded, never derived from the answer; this slice did not introduce that, but enumeration must fix it.)
```

### Acceptance outcomes (EXECUTED unless noted)
```text
1. source_file populated by ingest; warm-cache v5 round-trip preserves it; old caches re-extract. PASS.
2. 2-partition overlay: a/main imports ../../b/src/foo -> a cross-partition edge with basis
   AstImportFileInventoryResolved, and ONLY once b is resident.  PASS
   (xpart_overlay_resolves_cross_partition_edge_only_when_target_resident).
3. file_import_cycles() detects the a/main <-> b/foo cross-partition CYCLE via the overlay; the cycle is
   built ENTIRELY from the overlay (intra_partition == false, xpart_edge_count == 2).  PASS
   (xpart_overlay_forms_cross_partition_file_import_cycle).
4. unload b -> overlay rebuilt without the edge; answer degrades to Partial + missing=[b], never a stale
   edge.  PASS (unload_rebuilds_overlay_without_the_edge_and_degrades).
5. Overlay never serialized: PartitionIr has NO overlay field (compile-time guarantee); the cross-partition
   cycle came from the runtime overlay, not ir.edges (intra_partition == false in #3).  PASS (structural).
6. LIVE 2-package daemon path: NOT RUN — the daemon loads one partition at a time (F2), so the live path
   cannot stage a cross-import this slice. The cross-partition behavior is covered headlessly (#2-#5); the
   live gap is the IMPORTS-XPART-ENUMERATION-1 follow-up.  DOCUMENTED LIMITATION.
```

### Validation evidence
```text
EXECUTED: cargo test -p repo-graph-ir -p repo-graph-warm-cache -p repo-graph-scip-ingest (green);
  cargo test -p repo-graph-livegraph (49) -p repo-graph-import-resolver (7) (green);
  cargo test --workspace (220 binaries ok, 0 failures, exit 0);
  cargo clippy --workspace --all-targets -- -D warnings (clean); cargo fmt --all -- --check (clean).
LIVE (dev-install-local.sh, EXECUTED): release build + daemon restart -> doctor healthy, validation
  passed (rmap/rmapd 0.2.1, pid 92015). `rmap dev livegraph-refresh` of the synthetic partition ->
  "warmed_from_cache": false (the v4 cache was REJECTED under v5 -> fresh re-extract: 15 nodes, 12 edges,
  epoch 1) = live v5 re-extraction evidence. `rmap cycles --engine livegraph --kind file-import` (single
  partition) -> class=Exact, freshness=Fresh, no cycles, serves WITHOUT panic on the new overlay/scope
  code. The daemon still emits the HARD-CODED scope string (accurate single-partition: cross_partition is
  false under F2) -- confirming the TECH DEBT above (the string even keeps the retired variant NAME; it is
  a JSON literal, not the headless type). Cross-partition overlay NOT exercisable live under F2 (by design).
```

## Follow-up slices
```text
- IMPORTS-XPART-ENUMERATION-1 : daemon auto-discovery + loading of all a repo's packages (F2), so the
  overlay covers the whole repo without manual preloads. PREREQUISITE OF THAT SLICE (carried from the TECH
  DEBT above): the daemon cycles emission (`livegraph_feed.rs:934`) MUST switch from the hard-coded scope
  string to the structured `env.scope` (D5 flag set), and the CLI (`graph.rs:830`) MUST render it — else a
  multi-partition daemon answer would under-report cross-partition coverage (a false trust claim).
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
