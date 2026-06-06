# IMPORTS-DYNAMIC-CLASSIFICATION-1: classify dynamic `import()` (literal vs non-literal)

Slice ID: IMPORTS-DYNAMIC-CLASSIFICATION-1
Status: **SPEC — awaiting ratification (2026-06-06). NOT started.** Split the blanket `DynamicUnsupported` so a
LITERAL dynamic `import('...')` is classified like its static counterpart (relative -> resolved edge;
bare -> workspace/external/node_modules), and only NON-LITERAL `import(expr)` stays blocking. NO workspace
package edge, NO default migration, NO raw decommission, NO package-manager/network, NO heuristic target
resolution.
Depends: IMPORTS-PACKAGE-RESOLUTION-1 / -TSCONFIG-PATHS-1 / -PACKAGE-EXTERNAL-EVIDENCE-1 (the static
classification this reuses), the import-resolver relative + alias resolution. Track: Stage D, import completeness.

## Goal
```text
After EXTERNAL-EVIDENCE-1, amodx's only remaining import-class blockers are workspace-local (RED), dynamic, and
unresolved-relative. Dynamic is currently a BLANKET block: `if o.is_dynamic { DynamicUnsupported }` -> every
`import()` blocks, even a literal one whose target is perfectly resolvable. A literal dynamic import is, for
cycle purposes, the SAME edge as a static import -- the dynamic-ness does not change cycle-relevance. Classify
literal dynamics by their specifier (reusing the static rules) and keep only NON-LITERAL `import(expr)` blocking.
```

## Grounding (EXECUTED 2026-06-06)
```text
amodx dynamics: ALL 6 are LITERAL + WORKSPACE-LOCAL -- import("@amodx/effects"|"@amodx/effects/render"|
  "@amodx/effects/celebration"). ZERO relative, ZERO external, ZERO non-literal.
producer (ts-extractor collect_dynamic_imports, extractor.rs:1429): one observation per `import(...)` call;
  raw_specifier = the FIRST STRING-LITERAL argument when statically present, ELSE EMPTY; is_relative =
  raw.starts_with('.'); resolved_path ALWAYS None; is_dynamic = true.
=> LITERAL vs NON-LITERAL is decidable at the IR today: NON-EMPTY raw_specifier = literal; EMPTY = non-literal.
   A literal relative dynamic can be overlay-resolved (normalize_join(dirname, raw) + candidate_paths -- the
   overlay never needed resolved_path). A literal bare dynamic reuses the static package classification.
CAVEAT (recorded): a CONCATENATED `import('./p/' + x)` captures the FIRST string ('./p/') -> treated as a
  literal that will simply fail to resolve -> blocks (honest; errs toward attempting + failing, never a false
  edge). amodx has none.
```

## THE amodx COVERAGE (honest — must be on the table)
```text
amodx dynamics are ALL literal WORKSPACE-LOCAL (@amodx/effects). So this slice EXPLAINS amodx's dynamic blocker
(reclassifies the 6 -> WorkspaceLocalUnedgeable, the RED edge-blocked case) rather than REDUCING it:
has_dynamic -> FALSE, but the 6 fold into has_workspace_local_unedgeable (already true). amodx's distinct
blocking categories shrink {workspace-local, dynamic, unresolved-relative} -> {workspace-local, unresolved-
relative}. The literal-RELATIVE -> edge and literal-EXTERNAL -> benign paths have NO amodx case -> validated on a
FIXTURE. This matches the brief's "dynamic blocker reduced OR EXPLAINED" (explained, for amodx).
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — edge basis (the user's #1)
```text
A NEW `EdgeBasis::AstDynamicImportResolved` for a LITERAL RELATIVE dynamic import that overlay-resolves to a
FILE -- DISTINCT from AstImport (static) / AstImportFileInventoryResolved (static relative) / AstImportTsconfig
PathResolved (alias). Maps to EdgeType::Imports; runtime-only, never persisted (like the other resolved bases).
RECOMMENDATION: as written -- the provenance must say "dynamic import", not be conflated with a static edge.
```

### D2 — the classification (literal dynamics reuse static rules)
```text
For an `is_dynamic` observation:
  - raw_specifier EMPTY (non-literal `import(expr)`) -> `DynamicUnresolved` (BLOCKS). NO inferred edge.
  - raw_specifier literal + RELATIVE -> overlay-resolve (same rules as StaticUnresolved relative); RESOLVED ->
    a FILE edge (basis AstDynamicImportResolved); UNRESOLVED -> blocks (dynamic-relative-unresolved).
  - raw_specifier literal + BARE -> the SAME package classification as a static PackageExternal: tsconfig alias
    -> resolve; workspace map -> WorkspaceLocalUnedgeable; declared/node_modules external -> benign; else ->
    PackageUnresolved.
RECOMMENDATION: as written -- a literal dynamic is its static counterpart + the is_dynamic provenance. Only the
non-literal case is genuinely dynamic-specific (and unresolvable without runtime).
```

### D3 — cert policy split (the user's #2)
```text
ObservationClassSummary: replace the blanket `has_dynamic` with `has_dynamic_unresolved` (BLOCKS) = a
NON-LITERAL dynamic OR a literal-relative dynamic the overlay did not resolve. A literal dynamic that resolves /
classifies routes to the EXISTING flags (a resolved-relative edge -> captured; workspace -> has_workspace_local_
unedgeable; external -> has_external_nonlocal benign; alias -> resolved/has_alias_unresolved; unknown bare ->
has_unresolved_package). Blocking set = has_dynamic_unresolved + the existing blocking flags. Policy version 4 -> 5.
RECOMMENDATION: as written. (`has_external_nonlocal` already benign; `has_workspace_local_unedgeable` already
blocks; no NEW benign/blocking semantics beyond has_dynamic_unresolved.)
```

### D4 — module-cycle contribution (the user's #3)
```text
A RESOLVED literal-relative dynamic edge (AstDynamicImportResolved) CONTRIBUTES to file + module cycles (it is a
real import edge), via the overlay (so it aggregates like the other resolved edges). Workspace-local / external
/ non-literal dynamics produce NO edge -> no cycle contribution (same as their static equivalents). The basis is
DISTINCT so the provenance is honest ("a cycle edge via a dynamic import").
RECOMMENDATION: yes for literal-local (resolved) dynamics, basis-labelled. (The brief's expectation.)
```

## Validation (EXECUTED later)
```text
- amodx: the 6 literal @amodx/effects dynamics -> WorkspaceLocalUnedgeable; has_dynamic_unresolved = FALSE (no
  non-literal); has_dynamic(old) gone. The dynamic blocker is EXPLAINED (folded into workspace-local).
- a FIXTURE (the literal-relative + literal-external + non-literal paths amodx lacks): import('./foo') ->
  AstDynamicImportResolved edge + contributes to a cycle; import('react') (declared/node_modules) -> benign;
  import(expr) -> has_dynamic_unresolved (blocks). NO false edge for the non-literal.
- xpart fixture: unchanged (no dynamics). repo-graph: unchanged (non-TS precedence).
- full gate; warm-cache round-trip if any IR shape change (the new EdgeBasis is runtime-only -> no persist).
```

## Out of scope (hard guardrails)
```text
NO workspace package edge (a literal workspace-local dynamic stays WorkspaceLocalUnedgeable -- the RED case).
NO default migration, NO raw decommission, NO package-manager/network, NO heuristic target resolution (a
non-literal `import(expr)` is NEVER guessed -> always blocks). Concatenated literals are best-effort (first
string) and simply fail to resolve.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. IR: + EdgeBasis::AstDynamicImportResolved (runtime-only). No new ImportObservation field (literal-ness =
   raw_specifier non-empty; is_dynamic already exists). Warm-cache EdgeBasis DTO enum arm (never persisted).
2. ingest: NO change to classify_import_observations' is_dynamic -> DynamicUnsupported (the IR resolution class
   stays; the snapshot does the literal split, reusing the IR fields). [OR add a refined IR resolution -- decide
   in build; prefer snapshot-side to avoid an IR resolution-enum change.]
3. import-resolver: reuse resolve_imports (relative) + classify_package_import (bare) for literal dynamics; the
   snapshot routes is_dynamic obs through them + the non-literal -> has_dynamic_unresolved.
4. livegraph overlay: literal-relative dynamic -> a ResolvedImportEdgeCandidate (basis AstDynamicImportResolved).
   snapshot: the dynamic arm classifies (D2) into the flags + has_dynamic_unresolved.
5. cert: ObservationClassSummary has_dynamic -> has_dynamic_unresolved; evaluate + fingerprint; audit policy 4->5.
6. live (amodx: dynamic explained) + the fixture (relative/external/non-literal); gate; completion doc.
Stop if distinguishing literal-relative-resolvable from a concatenation needs AST re-derivation beyond the
producer's first-string capture -> present a matrix (v1 = first-string-literal, best-effort).
```

## Follow-up
```text
- IMPORTS-WORKSPACE-PACKAGE-EDGE (research): the workspace-local block now ALSO covers the dynamic @amodx/effects.
- unresolved-relative imports (amodx's other remaining axis: has_unresolved_after_overlay).
- once amodx's blocking set is only workspace-local (RED) + unresolved-relative, the default-migration readiness
  is a measurement question again.
```

## References
- `rust/crates/ts-extractor/src/extractor.rs:1429` (`collect_dynamic_imports` -- literal vs empty specifier)
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` (`classify_import_observations` -- is_dynamic -> DynamicUnsupported)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`rebuild_xpart_overlay` + `module_cycle_live_state` -- the split)
- `rust/crates/repo-graph-ir/src/lib.rs` (`EdgeBasis` -- + AstDynamicImportResolved)
- `docs/slices/imports-package-external-evidence-1.md` (the static classification this reuses)
