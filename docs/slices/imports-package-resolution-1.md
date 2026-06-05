# IMPORTS-PACKAGE-RESOLUTION-1: classify TS package imports (workspace-local vs external vs unresolved)

Slice ID: IMPORTS-PACKAGE-RESOLUTION-1
Status: **SPEC — awaiting ratification (2026-06-05). NOT started. Policy only; no implementation.** Decide which
package/import classes RESOLVE (workspace-local -> graph edges) vs are SAFELY non-cycle-relevant (external) vs
still BLOCK (unresolved/dynamic), so the module-cycle certificate stops counting benign external npm imports as
completeness holes. TS package-import resolution/completeness slice; NOT a default migration, NO raw
decommission, NO module-cycle default flip, NO non-TS support.
Depends: CYCLES-COMPLETENESS-ENUMERATION-1 (amodx now reaches `IncompleteImportClasses`), the IR
`ImportObservation`/`ImportResolution`, the livegraph cross-partition overlay, the import-resolver crate.
Track: Stage D, import completeness.

## Goal
```text
ENUMERATION-1 made amodx (8/8 loaded) certify `IncompleteImportClasses` -- the remaining TS blocker is
package/dynamic imports, NOT missing partitions. But most of amodx's package imports are EXTERNAL npm
(react/@tiptap/@aws-sdk/node:) that can NEVER be in a REPO-LOCAL module cycle, plus a few WORKSPACE-LOCAL ones
(@amodx/shared -> packages/shared) that SHOULD be edges. Stop treating external imports as local graph holes;
resolve workspace-local imports to edges; keep genuinely-unknown imports blocking. Goal is the POLICY +
classification, not a general resolver.
```

## Grounding (EXECUTED 2026-06-05) — the amodx measurement + the code mechanics
```text
amodx root package.json: workspaces=[infra,backend,admin,renderer,tools/*,packages/*]; workspace packages
  @amodx/effects|plugins|shared. Top non-relative specifiers (occurrences):
    EXTERNAL npm / builtin: react 320, @tiptap/pm/model 352, @tiptap/* ~1000+, @aws-sdk/lib-dynamodb 207,
      lucide-react 162, aws-lambda 99, zod 70, node:buffer 263, node:fs 158, node:* hundreds.
    WORKSPACE-LOCAL: @amodx/shared 96  (-> packages/shared, a LOADED partition; a real dropped edge).
    TSCONFIG PATH ALIAS: @/lib/api 43, @/lib/* (baseUrl/paths; not relative, not a workspace pkg, not a dep).
CODE: classify_import_observations (scip-ingest/lib.rs:783) -> PackageExternal := `!is_relative` (ANY bare
  specifier; @amodx/shared and react are lumped IDENTICALLY). Overlay rebuild_xpart_overlay (livegraph/lib.rs:
  1034) processes ONLY StaticUnresolved (relative); resolve_imports (import-resolver/lib.rs:172) REJECTS every
  bare specifier as PackageExternal. Partition (ir/lib.rs:197) has id/kind/root/indexer -- NO package_name. NO
  package.json `name` map, NO tsconfig paths, NO exports reading ANYWHERE. Edges: only StaticResolved (intra) +
  overlay-resolved relative (cross) become edges; PackageExternal/Dynamic dropped (ir/livegraph Q5).
=> the lever is a 3-way RECLASSIFICATION of the current single PackageExternal bucket, using package.json
   metadata that is not read today.
```

## THE TRUST HINGE (load-bearing; must be on the table before ratifying)
```text
"External imports don't block completeness" is SAFE only if "external" carries POSITIVE external evidence.
  - Mark `ExternalPackageNonLocal` ONLY for: a `node:`/builtin specifier, OR a package-name DECLARED in a
    package.json dependencies/devDependencies, OR one resolving INTO node_modules. NOT merely "absent from the
    workspace map" -- an unknown bare specifier (e.g. @/lib path alias) is `PackageUnresolved` (BLOCKS), never
    silently "external".
  - `WorkspaceLocalResolved` is SAFE only if it ACTUALLY becomes an edge. A workspace-local import marked
    "resolved" but NOT edged is a FALSE COMPLETE (the cycle through it is missed). So: WorkspaceLocalResolved
    REQUIRES the edge; if the target package/file cannot be edged (target partition not loaded, no resolvable
    entry), it DEGRADES to `PackageUnresolved` (blocks), never to resolved-benign.
=> External-benign needs positive external proof; workspace-local needs a real edge; everything else BLOCKS.
   Under-classifying (-> blocks) is safe; over-classifying as benign/resolved risks a false trust claim.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — the classification scheme (1A; the user's expected direction)
```text
Re-classify each current `PackageExternal` observation into:
  WorkspaceLocalResolved  -- package-name ∈ the loaded WORKSPACE MAP (package.json `name` -> partition) AND an
                             edge to that package is produced. -> a MODULE/FILE edge (cycle-relevant).
  ExternalPackageNonLocal -- `node:`/builtin, OR declared dependency/devDependency, OR resolves into
                             node_modules. -> NON-cycle-relevant; does NOT block completeness.
  PackageUnresolved       -- neither of the above (unknown bare specifier, unloaded workspace target, tsconfig
                             path alias not yet handled). -> BLOCKS/degrades completeness (honest unknown).
DynamicUnsupported stays SEPARATE: blocks, unless a future literal + local-resolvable `import('...')` (stretch).
RECOMMENDATION: as written. Three classes + Dynamic; positive-evidence rule for External (the trust hinge).
```

### D2 — metadata source (what makes the classification decidable)
```text
A. per-partition package.json `name` -> the WORKSPACE MAP (package-name -> partition). [needed for WorkspaceLocal]
B. package.json dependencies/devDependencies (root + per-package) -> the DECLARED-EXTERNAL set. [for External]
C. `node:` prefix + the Node builtin list -> External. [cheap, certain]
D. tsconfig paths/baseUrl (the @/lib aliases). [DEFERRED -> those stay PackageUnresolved until a follow-up]
E. package exports/main/types (the exact entry FILE). [only if FILE-level edges are required; see D4]
RECOMMENDATION: A+B+C for v1 (decides all three classes). D deferred (a 1C follow-up reclassifies @/ aliases
from Unresolved -> local). E only if D4 needs file-level precision.
```

### D3 — package imports: edges or evidence? (the user's #4)
```text
WorkspaceLocalResolved -> a graph EDGE (cycle-relevant). ExternalPackageNonLocal -> completeness EVIDENCE only
(never an edge; it cannot be in a repo-local cycle). PackageUnresolved/Dynamic -> blocking EVIDENCE (no edge).
RECOMMENDATION: as written -- "only WorkspaceLocalResolved can produce graph edges" (ratified by the brief).
EDGE GRANULARITY: module cycles aggregate by dirname, so a workspace-local import needs an edge to the TARGET
PACKAGE's module (its dir), not necessarily the exact file -> the target partition's representative/entry node
suffices (avoids full package-exports resolution in v1; E only if file-precision is later required).
```

### D4 — which layer owns it (the user's #5)
```text
A. import-resolver crate: gains a WORKSPACE-AWARE mode (input: workspace map + external set) -- the pure
   classification/resolution mechanism. [RECOMMENDED home for the rule]
B. ingest producer (scip-ingest): reads the partition root package.json `name` -> Partition.package_name (a
   NEW IR field), so the name rides in the IR (keeps livegraph IO-free). [RECOMMENDED for the metadata capture]
C. daemon audit only: NO -- the audit is read-only + per-repo; classification belongs in the shared resolve path.
RECOMMENDATION: B captures package_name at ingest (into the IR); the livegraph OVERLAY builds the workspace map
from loaded partitions' package_names + the declared-external set, and calls A (the workspace-aware resolver)
to classify + edge. The cert's ObservationClassSummary splits accordingly (D5). NOT the audit (C).
```

### D5 — completeness-policy change (the cert consumes the new classes)
```text
Today: ObservationClassSummary.has_package_external -> IncompleteImportClasses (ANY bare import blocks).
NEW: split it. ExternalPackageNonLocal -> a BENIGN flag that does NOT block. Only PackageUnresolved (+ Dynamic +
unresolved-after-overlay relative) -> IncompleteImportClasses. WorkspaceLocalResolved -> captured (edged) -> not
a gap. This AMENDS module_cycle_cert.rs ObservationClassSummary (additive: e.g. has_unresolved_package replaces
the blocking role of has_package_external; has_external_nonlocal is reported-but-benign) + the audit snapshot.
RECOMMENDATION: as written. This is the change that lets amodx's react/@tiptap stop blocking while @amodx/shared
becomes an edge -- the cert advances toward Complete (or stays blocked HONESTLY on @/lib PackageUnresolved).
```

## Validation (EXECUTED later)
```text
- amodx: @amodx/shared (96) -> WorkspaceLocalResolved -> a cross-partition edge (admin/renderer -> packages/
  shared). react/@tiptap/@aws-sdk/node:* -> ExternalPackageNonLocal (declared deps / builtins) -> benign.
  @/lib/* -> PackageUnresolved (no tsconfig paths yet) -> still blocks. EXPECTED cert: still
  IncompleteImportClasses BUT now ONLY because of @/lib (PackageUnresolved) + dynamic -- the external-npm noise
  is gone, and the report shows the breakdown. (A clean pure-workspace-relative TS repo with no @/ aliases ->
  Complete.)
- the xpart fixture: unchanged (relative imports only -> already Complete).
- repo-graph: unchanged headline (IncompleteUnsupportedLanguage; non-TS precedes import classes).
- NO false Complete: an ExternalPackageNonLocal requires positive external evidence; a WorkspaceLocalResolved
  requires a real edge (else degrades to PackageUnresolved).
- full gate.
```

## Out of scope (hard guardrails)
```text
NO default migration, NO raw decommission, NO module-cycle default flip, NO non-TS support. NO general module
resolver (Node resolution algorithm, conditional exports) -- workspace-name + declared-dep + node: only. tsconfig
paths/baseUrl (@/ aliases) DEFERRED to a follow-up. package exports/main file-precision only if D3 escalates.
Dynamic import literals DEFERRED (stay blocking).
```

## Build contract (PROPOSED — gated on ratification; likely staged 1A -> 1B)
```text
1A (classify): Partition.package_name (IR) captured at ingest (B); a workspace-aware classifier in the
   import-resolver (A) that, given {workspace map, declared-external set, node-builtins}, maps a bare specifier
   -> {WorkspaceLocalResolved(target partition) | ExternalPackageNonLocal | PackageUnresolved}; the overlay
   builds the inputs from loaded partitions + repo package.json; pure + unit-tested. NO cert change yet
   (classification observable via the audit report only).
1B (consume): the overlay EDGES WorkspaceLocalResolved (to the target package's representative node); the cert's
   ObservationClassSummary splits (D5) so External stops blocking; the audit reports the 3-way breakdown.
Stop if capturing package_name requires reading package.json OUTSIDE the ingest boundary (e.g. per-query) ->
present a boundary matrix (the IR must carry it; livegraph stays IO-free).
```

## Follow-up
```text
- IMPORTS-PACKAGE-RESOLUTION-1C: tsconfig paths/baseUrl -> reclassify @/ aliases from PackageUnresolved -> local.
- dynamic-import literal resolution (literal + local-resolvable `import('...')`).
- the daemon RUNTIME wiring (cache the BaselineInput) + CYCLES-DEFAULT-MIGRATION-1 (un-deferred) -- once real
  TS repos can reach Complete, the migration's serve-path becomes worthwhile.
```

## References
- `rust/crates/repo-graph-scip-ingest/src/lib.rs:783` (`classify_import_observations`; PackageExternal := !is_relative)
- `rust/crates/repo-graph-ir/src/lib.rs:121,153,197` (`ImportResolution`, `ImportObservation`, `Partition` -- no package_name)
- `rust/crates/repo-graph-livegraph/src/lib.rs:1018` (`rebuild_xpart_overlay`; StaticUnresolved-only today)
- `rust/crates/repo-graph-import-resolver/src/lib.rs:172` (bare specifier rejected as PackageExternal)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (`ObservationClassSummary` -- the D5 amendment target)
- `docs/slices/cycles-completeness-enumeration-1.md` (amodx -> IncompleteImportClasses, the measurement that justifies this)
