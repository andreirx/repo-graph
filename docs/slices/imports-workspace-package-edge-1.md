# IMPORTS-WORKSPACE-PACKAGE-EDGE-1: resolve workspace-local package imports to module-cycle edges

Slice ID: IMPORTS-WORKSPACE-PACKAGE-EDGE-1
Status: **PROBE 1A EXECUTED -> RED (2026-06-05). 1B NOT BUILT (deferred).** Mechanism A is REFUTED by
measurement: in separate partition indexes, `packages/shared` exports under `@amodx/shared 1.0.0 src/...` while
`admin` references `@amodx/shared 1.0.0 dist/...` (it resolves the dep through the BUILT `node_modules/.../dist`).
Package@version match; the descriptor path differs (src vs dist) -> no cross-index symbol match without a
dist->src bridge (declaration maps absent, or a rejected heuristic). B refuted, C/D rejected, A RED ->
workspace-local package imports REMAIN `WorkspaceLocalUnedgeable` (honest block). See **Probe 1A**. NO default
migration, NO raw decommission, NO heuristic dist->src.
Depends: IMPORTS-PACKAGE-RESOLUTION-1 (the classifier + `WorkspaceLocalUnedgeable`). Track: Stage D, import edges.

## Goal
```text
Turn the detected-but-unedgeable workspace-local package imports into real module-cycle edges WITHOUT a
heuristic. After IMPORTS-PACKAGE-RESOLUTION-1, external npm is benign and the remaining high-value blocker is
workspace-local imports (@amodx/shared -> packages/shared, a loaded partition). These are genuine repo-local
cycle edges; the blocker is purely TARGET-MODULE RESOLUTION, which the grounding shows is not cleanly available.
```

## Grounding (EXECUTED 2026-06-05) — refutes B, gates A, blocks D
```text
1. PRODUCER (Q1): TsImportObservation (indexer/types.rs:406) carries for a package import ONLY raw_specifier +
   modifiers -- NO symbol id / moniker / resolved_path. The AST extractor does not resolve packages.
2. SCIP external symbols (Q2): scip-ingest (lib.rs:638-690) FILTERS occurrences to PARTITION-LOCAL symbols
   (symbol_to_key); external/cross-package references are SKIPPED; index.external_symbols is read only for
   counting, NEVER retained. -> no import-site -> external-moniker mapping exists today.
3. Moniker stability (Q3): UNVERIFIABLE from code -- need a scip-typescript SCIP DUMP to confirm @amodx/shared
   exports carry STABLE package monikers that admin's imports reference identically.
4. B metadata (declaration maps): amodx packages/shared has 0 *.d.ts.map; tsconfig declarationMap=false
   (declaration=true, outDir=dist, no rootDir/composite); types=dist/index.d.ts (UNINDEXED dist); no `source`,
   no `exports`. => NO dist->src bridge except a heuristic. B REFUTED for this repo.
5. Module-edge channel (Q4): module_import_cycles (livegraph:1169) derives MODULE edges EXCLUSIVELY by
   aggregating FILE edges (file_import_edges:1099 = intra AstImport + xpart_overlay). NO module-only channel ->
   a module edge requires a FILE edge (which pollutes file_import_cycles), unless a NEW module-only source is
   added. EdgeBasis (ir:88-113) has AstImportFileInventoryResolved (derived, runtime-only) as the labelling
   precedent; a new derived basis can be added.
=> STOP CONDITIONS FIRED: "entrypoint -> unindexed dist + no declarationMap" (B); "module-level only vs FILE->
   FILE graph" (D). A is the only NON-heuristic mechanism, and it is GATED on a probe (3) + new plumbing (2).
```

## Probe 1A (EXECUTED 2026-06-05) — VERDICT: RED (measurement-only, no production code)
```text
METHOD: generated per-partition SCIP exactly as the daemon does --
  scip-typescript-node18 index --cwd amodx/packages/shared --output /tmp/probe-shared.scip
  scip-typescript-node18 index --cwd amodx/admin            --output /tmp/probe-admin.scip
then compared the @amodx/shared SCIP symbol monikers across the two INDEPENDENT indexes (`strings` + grep;
2326 @amodx/shared symbol strings in shared, 520 references in admin).

THE FIVE FACTS:
1. shared exports carry stable monikers?           YES -- `scip-typescript npm @amodx/shared 1.0.0 src/...`
                                                    (package + version + descriptor; stable).
2. admin references the SAME moniker?              NO  -- admin references `@amodx/shared 1.0.0 dist/...`.
                                                    Package@version MATCH; the DESCRIPTOR PATH differs (dist
                                                    vs src) because admin resolves the dep through the BUILT
                                                    node_modules/@amodx/shared/dist, not shared's source.
3. map reference -> shared SOURCE file (no decl maps)? NO -- requires bridging dist/... -> src/... ; the
                                                    declaration maps that would do it are absent (B grounding).
4. cross-partition match survives separate indexes? PARTIAL -- only the PACKAGE@VERSION survives (which the
                                                    classifier already knows); the exact symbol/file does NOT.
5. ambiguity surfaced not picked?                  MOOT -- there is no exact match to be ambiguous about.

ROOT CAUSE (inherent, not a config miss): repo-graph indexes PER-PARTITION. Each partition resolves its
imports through its own node_modules -> a workspace dep resolves to the BUILT package (dist/*.d.ts), while the
exporting partition is indexed from SOURCE (src/*.ts). The SCIP descriptor encodes that path -> src vs dist
mismatch. The ONLY bridges are declaration maps (absent) or a heuristic (rejected). A whole-repo single SCIP
index WOULD resolve to source, but that contradicts the foundational partition model (out of scope).

VERDICT: RED. Mechanism A cannot resolve a workspace-local import to the exporting SOURCE module/file without
the rejected dist->src bridge. Per the ratified outcomes: record the blocker; @amodx/shared etc. REMAIN
WorkspaceLocalUnedgeable (the IMPORTS-PACKAGE-RESOLUTION-1 honest block stands). NO 1B build.

NEXT (higher value, now): IMPORTS-PACKAGE-RESOLUTION-1C (tsconfig path aliases @/lib -- the OTHER blocking
class, structurally cleaner: paths/baseUrl are repo-local source, no dist indirection). Workspace-package edges
would need either declaration-map emission in the target repos (a per-repo build-config change, not ours to
make) OR a whole-repo unified index mode (a foundational re-architecture) -> a research item, not a near slice.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — the resolution mechanism (the user's A/B/C/D)
```text
A. SCIP / declaration-map SYMBOL resolution: retain the external moniker at the import site, match it to the
   exporting partition's defining symbol (-> its source FILE -> its module). HIGHEST fidelity, NON-heuristic.
   PREREQUISITE: the probe (1A) -- verify scip-typescript emits stable cross-package monikers + wire external-
   symbol retention (currently discarded). [RECOMMENDED, gated on the probe]
B. package entry + declarationMap (types/.d.ts -> declarationMap -> src). REFUTED: no *.d.ts.map exist;
   declarationMap=false. Only viable for repos that SHIP declaration maps (amodx does not). [REJECTED for now]
C. convention src/index.ts. HEURISTIC -> rejected by the brief + the no-heuristic guardrail. [REJECTED]
D. module-only package-root edge (source module -> target package-root module), labelled DERIVED (not
   AstImport). Needs a NEW module-only edge channel (Q4) AND still needs a target-module identity (A or B or
   heuristic) -- so it does NOT escape the resolution problem; it only changes granularity. [DEGRADED fallback]
RECOMMENDATION: A, GATED on the 1A probe. B rejected (no decl maps), C rejected (heuristic), D only as a
labelled degraded module-level edge IF A's target resolves but file-level is undesired.
```

### D2 — the prerequisite PROBE (1A; what must be true before any edge)
```text
A SCIP DUMP probe over amodx admin + packages/shared: (i) does scip-typescript DEFINE @amodx/shared's exports
with a stable package moniker (scip-typescript npm @amodx/shared <ver> <descriptor>)? (ii) does admin's index
REFERENCE that SAME moniker at the `@amodx/shared` import site? (iii) is the descriptor stable across the two
partition indexes (so a cross-index match is exact, not heuristic)? GREEN (all yes) -> A is buildable. RED ->
workspace-package edges are NOT resolvable without heuristics -> DEFER (WorkspaceLocalUnedgeable stands honest).
RECOMMENDATION: run 1A as a measurement-only probe (no plumbing) FIRST; its verdict decides 1B.
```

### D3 — edge granularity + basis (the user's #6)
```text
If A resolves the target to a SOURCE FILE: emit a FILE->FILE cross-partition edge (it aggregates to the module
edge naturally) with a NEW derived basis (e.g. `AstImportWorkspacePackageResolved`) -- runtime-only, NEVER
persisted, NEVER labelled `AstImport`. This reuses the existing module aggregation (Q4) and is honest about
provenance. If A resolves only to a PACKAGE/MODULE (not a file): a module-only edge (D) requires the new
module-only channel + the derived basis, and must be EXCLUDED from file_import_cycles. RECOMMENDATION: FILE->
FILE with a new derived basis (preferred -- no new channel, honest label); module-only (D) only if A is
file-imprecise.
```

### D4 — target identity safety (the stop guard)
```text
NEVER pick a target by heuristic (no src/index.ts convention, no dist->src string-strip). The target must come
from a VERIFIED moniker match (A) or a real declarationMap (B, absent here). If neither yields an exact target,
the import STAYS WorkspaceLocalUnedgeable (blocking) -- the IMPORTS-PACKAGE-RESOLUTION-1 trust hinge holds.
RECOMMENDATION: as written.
```

## Validation (EXECUTED later, IF 1A is GREEN)
```text
- 1A probe: report the moniker evidence for @amodx/shared (admin import ref vs shared export def) with the exact
  monikers. GREEN/RED verdict recorded.
- (1B, if GREEN) amodx: @amodx/shared imports -> resolved cross-partition edges (derived basis); the audit's
  has_workspace_local_unedgeable DROPS for the resolved ones; remaining blockers = @/lib (PackageUnresolved) +
  dynamic + transitive. A pure-workspace TS repo with no @/ aliases + no dynamic -> closer to Complete.
- xpart fixture: unchanged (no package imports). repo-graph: unchanged (non-TS precedence).
- NO false edge: every workspace-package edge traces to a verified moniker (or the import stays unedgeable).
- full gate.
```

## Out of scope (hard guardrails)
```text
NO default migration, NO raw decommission, NO package-EXTERNAL resolver (external stays benign), NO tsconfig
path aliases (@/ stays PackageUnresolved -> IMPORTS-PACKAGE-RESOLUTION-1C), NO heuristic dist->src, NO
convention src/index.ts. A workspace-package edge is emitted ONLY from a verified target.
```

## Build contract (PROPOSED — gated on ratification; staged 1A probe -> 1B edge)
```text
1A (PROBE, measurement-only): dump + inspect scip-typescript monikers for amodx admin + packages/shared; verify
   the cross-partition moniker match (D2). Record GREEN/RED. NO production code.
1B (EDGE, only if 1A GREEN): retain external monikers at import sites in scip-ingest (the discarded layer, Q2);
   a cross-partition moniker->defining-file resolver (the symbol analogue of the relative overlay); emit a
   FILE->FILE edge with a NEW derived EdgeBasis (D3); the snapshot reclassifies resolved workspace-local imports
   from WorkspaceLocalUnedgeable -> captured; the cert's has_workspace_local_unedgeable drops accordingly.
Stop if 1A is RED (monikers absent/unstable) -> DEFER; do NOT build a heuristic edge.
```

## Follow-up
```text
- IMPORTS-PACKAGE-RESOLUTION-1C: tsconfig paths/baseUrl (@/lib) -> the other blocking class.
- the daemon RUNTIME wiring (cache the BaselineInput) + CYCLES-DEFAULT-MIGRATION-1 (un-deferred) -- once a real
  TS repo can reach Complete.
```

## References
- `rust/crates/indexer/src/types.rs:406` (`ImportObservation` -- no package symbol/moniker)
- `rust/crates/repo-graph-scip-ingest/src/lib.rs:638` (external SCIP symbols discarded; only partition-local)
- `rust/crates/repo-graph-livegraph/src/lib.rs:1099,1169` (module edges derive ONLY from file edges)
- `rust/crates/repo-graph-ir/src/lib.rs:88` (`EdgeBasis`; `AstImportFileInventoryResolved` derived precedent)
- `docs/slices/imports-package-resolution-1.md` (the classifier + WorkspaceLocalUnedgeable this resolves)
