# IMPORTS-XPART-RESOLUTION-1: pure cross-partition import resolver (support only) (Stage D)

Slice ID: IMPORTS-XPART-RESOLUTION-1
Status: **IMPLEMENTED + validated (2026-06-02).** Pure `repo-graph-import-resolver` crate
(relative + extension/index; ambiguity reported, never picked) + `EdgeBasis::AstImportFileInventoryResolved`
(no warm-cache schema bump). In-memory edge candidates only. Wiring deferred to IMPORTS-XPART-WIRING-1.
See Completion.
Depends: IMPORTS-EXTRACT-COMPLETENESS-1 (the `StaticUnresolved` observations this upgrades), `repo-graph-ir`
(EdgeBasis / IrEdge), `repo-graph-warm-cache` (the DTO enum-variant compat touch).
Track: Stage D. A **pure support resolver** only. NO CLI/cycles/default migration. NO module aggregation.
NO raw decommission. NO daemon wiring (deferred to IMPORTS-XPART-WIRING-1).

## Framing (hard constraints)
```text
- Goal: a PURE resolver that upgrades `StaticUnresolved` imports into node-resolved FILE -> FILE edges
  when the target FILE exists in the repo (possibly in ANOTHER partition) under relative + extension/index
  TS resolution — from a global FILE inventory, deterministically.
- Resolver OUTPUT is in-memory edge CANDIDATES, NOT persisted PartitionIr edges (see the decisive
  coherence finding). Unresolved/package/dynamic stay observations.
- No daemon/CLI code. No module aggregation. No cycles. No warm-cache schema bump (only the EdgeBasis
  enum-variant DTO compat).
```

## Decisive grounding findings (2026-06-02, EXECUTED)
```text
F1 COHERENCE (forces in-memory edges): the warm cache is PER-PARTITION (CacheKey.partition_id +
   source_inputs_hash; warm-cache lib). A cross-partition edge in partition A's IR -> B's FILE would let
   A's cache stay valid while B changed -> A serves an edge to a stale/missing target. Therefore resolved
   cross-partition edges MUST be runtime/in-memory, NEVER written to a per-partition IR or cache.
   (Today the design is safe ONLY because cross-partition imports are observations, not edges, ir:116.)
F2 NO MULTI-PARTITION DAEMON LOADING: `livegraph_refresh` loads ONE partition at a time (named, or
   "default"); nothing enumerates a repo's packages at daemon time (only the CLI compose layer does, at
   index time). The LiveGraph `defines` map holds FILE nodes across all LOADED partitions (the global
   inventory exists once loaded), but no loop loads them all. (dispatch.rs:1950; livegraph lib defines.)
F3 OBSERVATION-CONTEXT GAP: the IR `ImportObservation` carries `{raw_specifier, resolution, modifiers}`
   ONLY — NOT the source file or resolved path. So it is INSUFFICIENT to resolve a target. The resolver
   therefore takes its OWN explicit input type (source file + specifier); HOW that input is populated
   (extending the observation, which is an intra-partition cacheable fact, vs re-deriving) is the WIRING
   slice's decision (IMPORTS-XPART-WIRING-1). THIS slice does not change the observation -> no schema bump.
F4 partition = one TS package; partition.root = package dir; global FILE inventory = repo-relative FILE
   node keys across partitions.
```

## Ratified decisions (2026-06-02)

### D1 — Scope: A (pure resolver support module THIS slice; wiring deferred)
A new PURE crate `repo-graph-import-resolver` (zero daemon/storage deps; depends only on `repo-graph-ir`).
The daemon multi-partition enumeration + the in-memory cross-partition edge layer + live multi-partition
validation are DEFERRED to IMPORTS-XPART-WIRING-1 (F1/F2 make a one-slice end-to-end too large).

### D2 — Resolution cases: relative + extension/index ONLY
```text
Resolve a RELATIVE specifier against the global inventory by trying, for the normalized repo-relative
target base T:
  T, T.ts, T.tsx, T.d.ts, T.mts, T.cts, T/index.ts, T/index.tsx
NO tsconfig path aliases. NO package `exports`/`types`. A non-relative (package) specifier is NOT
resolved — it stays its unsupported class (PackageExternal). Do NOT implement PARTIAL package resolution
that looks correct but misses conditional exports/types (deferred to a separate slice).
```

### D3 — Edge provenance: distinct `EdgeBasis::AstImportFileInventoryResolved`
A resolved cross-partition edge is NOT producer-emitted/node-resolved-inside-the-partition; it is an AST
observation + file-inventory resolution (a stronger inference than a raw observation, but NOT identical to
`AstImport`). It carries the distinct basis so trust/diagnostics can explain the resolution chain. Add the
variant to `repo-graph-ir::EdgeBasis` + the warm-cache `CacheEdgeBasisDto` (From-arm compat) — **no
SCHEMA_VERSION bump** (the variant is never serialized into a cache; cross-partition edges are not cached).

## Pure resolver design (the build contract)
```text
crate repo-graph-import-resolver (pure; deps: repo-graph-ir only):
  FileInventory          : repo-relative path -> FILE node key (built by the caller from global FILE keys).
  ImportCandidate        : { source_file: String (repo-relative), raw_specifier: String }.
  ResolvedImportEdge     : { src_file_key, dst_file_key, raw_specifier, resolved_repo_path }.
  ResolveOutcome         : { edges: Vec<ResolvedImportEdge>, unresolved: Vec<ImportCandidate> }.
  fn resolve_imports(inv: &FileInventory, cands: &[ImportCandidate]) -> ResolveOutcome
    - non-relative specifier -> unresolved (PackageExternal; out of scope).
    - relative: normalize dirname(source_file)+raw_specifier (repo-relative; handles `..` across roots),
      try the D2 candidate list against `inv`; first hit -> ResolvedImportEdge with basis
      AstImportFileInventoryResolved (the caller stamps the basis onto the IrEdge); else -> unresolved.
  PURE + deterministic from the inventory alone; no filesystem, no producer, no daemon.
The caller (wiring slice) builds `FileInventory` (global FILE keys) + `ImportCandidate`s, runs the
resolver, and inserts the resolved edges into the LiveGraph IN-MEMORY (never the per-partition IR/cache).
```

## Out of scope (hard guardrails)
```text
No daemon/CLI code. No multi-partition enumeration (IMPORTS-XPART-WIRING-1). No in-memory edge layer wiring
(IMPORTS-XPART-WIRING-1). No persisted cross-partition edges (coherence F1). No tsconfig/package resolution
(separate slice). No module aggregation. No cycles. No warm-cache SCHEMA_VERSION bump. No change to the IR
`ImportObservation` (the observation-context gap F3 is the wiring slice's decision).
```

## Acceptance (EXECUTED later — pure unit tests over a synthetic inventory)
```text
inventory = { repo:packages/a/src/main.ts:FILE, repo:packages/b/src/foo.ts:FILE,
              repo:packages/a/bar/index.ts:FILE, repo:packages/a/widget.tsx:FILE }
1. `../../b/src/foo` from packages/a/src/main.ts -> resolved to packages/b/src/foo.ts (cross-partition,
   .ts candidate)                                    [the ../pkg/foo -> foo.ts case]
2. `./bar` from packages/a/src/main.ts -> resolved to packages/a/bar/index.ts        [index.ts case]
3. `./widget` from packages/a/src/main.ts -> resolved to packages/a/widget.tsx       [.tsx candidate]
4. `./missing` from packages/a/src/main.ts -> UNRESOLVED (stays unresolved)
5. `react` from packages/a/src/main.ts -> UNRESOLVED, explicit PackageExternal (unsupported; not partially
   resolved)
6. resolved edges carry basis AstImportFileInventoryResolved; NONE are written to a PartitionIr/cache.
```

## Commit structure (proposed)
```text
1. support: repo-graph-ir EdgeBasis::AstImportFileInventoryResolved + warm-cache CacheEdgeBasisDto variant
   (From-arm compat, NO schema bump) + new crate repo-graph-import-resolver (types + resolve_imports) + unit
   tests (the 6 cases).
2. docs: status/evidence.
```

## Completion (implemented + validated 2026-06-02, EXECUTED)

Commits: `229a9dc` (1/2 support) + this doc (2/2). NO daemon/CLI; NO wiring; NO module aggregation; NO cycles.

- **New PURE crate `repo-graph-import-resolver`** (deps: `repo-graph-ir` only, for the output `EdgeBasis`;
  inputs are plain strings; no filesystem/producer/daemon). `FileInventory::from_file_keys`,
  `ImportCandidate{source_file_key, raw_specifier}`, `resolve_imports -> ImportResolutionReport{resolved,
  unresolved}`. D2 rules (relative + ext/index); `>1` match -> `Ambiguous` (never a silent pick);
  non-relative -> `PackageExternal`; malformed key -> `BadSourceKey`.
- **`repo-graph-ir`**: `EdgeBasis::AstImportFileInventoryResolved` (runtime-only; never persisted).
- **`repo-graph-warm-cache`**: `CacheEdgeBasisDto` variant + From-arm compat ONLY — **SCHEMA_VERSION stays
  v3** (the variant never appears in a cache payload; the stop condition "if the variant forces a schema
  bump, stop" was NOT triggered).

```text
Tests (EXECUTED): repo-graph-import-resolver 7 — cross-partition `../../b/src/foo` -> packages/b/src/foo.ts;
  `./bar` -> bar/index.ts; `./widget` -> widget.tsx; `./missing` -> NotFound; `react` -> PackageExternal;
  `./dup` (dup.ts + dup.tsx) -> Ambiguous; malformed key -> BadSourceKey. Full cargo test --workspace green;
  clippy --workspace -D warnings + fmt clean.
```

### Handoff to IMPORTS-XPART-WIRING-1 (the F3 gap, explicit)
```text
The IR `ImportObservation` carries `{raw_specifier, resolution, modifiers}` — NOT the source file. So the
wiring slice MUST PROVIDE the `ImportCandidate`s itself (source_file_key + raw_specifier); the resolver does
NOT read observations. The wiring decides how to obtain the source file per StaticUnresolved import — e.g.
extend the IR observation with source/resolved-path (intra-partition, cacheable) THEN, or carry candidates
out-of-band at ingest. The wiring also builds the global `FileInventory` (FILE keys across loaded
partitions) and inserts resolved edges into the LiveGraph IN-MEMORY (never a per-partition cache).
```

## Follow-up slices
```text
- IMPORTS-XPART-WIRING-1 : daemon multi-partition enumeration/loading; build FileInventory + ImportCandidates
  (resolves the F3 observation-context gap — may extend the IR observation with source/resolved-path as
  intra-partition cacheable fields THEN); run the resolver; insert resolved edges into the LiveGraph
  IN-MEMORY (NO per-partition cache persistence); live multi-partition validation.
- IMPORTS-PACKAGE-RESOLUTION-1 (optional): tsconfig path aliases + package exports/types (a real TS resolver
  policy; needs new metadata inputs).
- MODULE-AGGREGATION-1 / CYCLES-LIVEGRAPH-1 (module) / IMPORTS-LIVEGRAPH-1 / STATS-LIVEGRAPH-1 : gated on a
  complete-enough import graph (this + wiring).
```

## References
- `docs/slices/imports-extract-completeness-1.md` (the StaticUnresolved observations; the IR ImportObservation shape)
- `docs/slices/sqlite-raw-decommission-readiness-2.md` (why this gates the cycles + imports/stats threads)
- `rust/crates/repo-graph-warm-cache/src/lib.rs` (CacheKey per-partition; F1 coherence; CacheEdgeBasisDto)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`defines` global FILE inventory; one-partition load; F2)
- `rust/crates/daemon-runtime/src/dispatch.rs` (`livegraph_refresh` per-partition; F2)
- `rust/crates/repo-graph-ir/src/lib.rs` (`EdgeBasis`, `ImportObservation`, `ImportResolution`)
