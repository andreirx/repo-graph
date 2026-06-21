# IMPORTS-DYNAMIC-CLASSIFICATION-1: classify dynamic `import()` (literal vs non-literal)

Slice ID: IMPORTS-DYNAMIC-CLASSIFICATION-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-06), D1–D4 ratified (D3 = model B).** A LITERAL dynamic
`import('...')` is classified by its TARGET CLASS (relative -> resolved edge `AstDynamicImportResolved` or the
relative-unresolved bucket; bare -> workspace/external/node_modules/alias/unknown); only NON-LITERAL
`import(expr)` -> `has_dynamic_unresolved`. Live: amodx's dynamic flag DISAPPEARS (all dynamics literal); blocking
stays workspace-local + unresolved-relative. See **Completion**. NO workspace package edge, NO default migration,
NO raw decommission, NO package-manager/network, NO heuristic target resolution.
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

### D3 — cert policy split (the user's #2) — RATIFIED MODEL B (corrected 2026-06-06)
```text
ObservationClassSummary: replace the blanket `has_dynamic` with `has_dynamic_unresolved` (BLOCKS) = a
NON-LITERAL dynamic `import(expr)` (empty specifier) ONLY -- the only genuinely dynamic-unresolvable case. A
LITERAL dynamic is classified by its TARGET CLASS (its static counterpart): resolved-relative -> captured edge;
UNRESOLVED-relative -> has_unresolved_after_overlay (the SAME relative bucket as static, NOT a dynamic signal);
bare -> workspace -> has_workspace_local_unedgeable; external -> has_external_nonlocal benign; alias ->
resolved/has_alias_unresolved; unknown bare -> has_unresolved_package. Policy version 4 -> 5.
(Original D3 also put unresolved-literal-relative in has_dynamic_unresolved; corrected to model B -- once a
dynamic has a literal specifier the remaining uncertainty is target-resolution class, not "dynamic". This makes
has_dynamic_unresolved a PRECISE non-literal signal + matches the validation intent.)
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

## Completion (implemented + live-validated 2026-06-06, EXECUTED)

Commits: `182bf38` (spec) + the impl/docs commits below. Ratified D1–D4 with **D3 = model B** (corrected).

### What landed
```text
IR: + EdgeBasis::AstDynamicImportResolved (runtime-only, never persisted; warm-cache enum arm).
livegraph overlay: a LITERAL relative dynamic resolves via the SAME relative machinery (re-stamped
  AstDynamicImportResolved); a literal bare dynamic matching a tsconfig alias -> a dynamic-resolved edge; a
  NON-LITERAL `import(expr)` is NEVER edged.
livegraph snapshot: the DynamicUnsupported arm splits -- empty specifier -> has_dynamic_unresolved; literal
  relative -> overlay edge OR has_unresolved_after_overlay (B: the relative bucket, not dynamic); literal bare ->
  the SHARED classify_bare_specifier helper (workspace/external/alias/unknown -- the SAME path as a static
  PackageExternal, so they cannot drift).
cert: ObservationClassSummary has_dynamic -> has_dynamic_unresolved (NON-LITERAL ONLY); evaluate + fingerprint;
  audit policy 4 -> 5.
```

### Live validation (EXECUTED 2026-06-06)
```text
amodx dynamics (comprehensive scan): ALL LITERAL -- @amodx/effects x6 (workspace-local) + ../src/auth/authorizer
  x4 + ../types.js x1 (relative); ZERO non-literal.
amodx audit -> IncompleteImportClasses, policy_version=5:
    has_dynamic_unresolved         = FALSE  <- the dynamic flag DISAPPEARED (all dynamics literal): the 6
                                              @amodx/effects -> has_workspace_local_unedgeable; the relative ones
                                              -> has_unresolved_after_overlay (e.g. ../types.js: the .js->.ts
                                              extension gap, a pre-existing relative-resolver limitation).
    has_workspace_local_unedgeable = true   (static @amodx/* + the dynamic @amodx/effects)
    has_unresolved_after_overlay   = true   (static + dynamic unresolved relatives)
    has_external_nonlocal_benign   = true ; has_unresolved_package = false ; has_alias_unresolved = false
  => amodx now blocks ONLY on workspace-local (RED) + unresolved-relative -- NOT dynamic. The dynamic category
     is gone from the report; the blocking set is maximally precise.
xpart fixture -> CompleteForModuleImportCycles (permits_default=true) -- REGRESSION INTACT.
unit (the paths amodx lacks): non-literal -> has_dynamic_unresolved; literal-relative-resolved ->
  AstDynamicImportResolved edge; literal-relative-UNRESOLVED -> has_unresolved_after_overlay; literal-external
  (node_modules) -> benign; literal-unknown -> has_unresolved_package.
```

### Acceptance — PASS
```text
1. amodx dynamic flag disappears; blocking -> workspace-local + unresolved-relative                PASS.
2. fixture: literal relative dynamic -> edge + cycle contribution (AstDynamicImportResolved)        PASS (unit).
3. fixture: literal external dynamic benign with positive external evidence                          PASS (unit).
4. fixture: non-literal dynamic blocks                                                               PASS (unit).
5. xpart remains Complete                                                                            PASS.
Gate: workspace tests ok / 0 failures; clippy -D warnings clean; fmt clean.
```

### Significance — amodx's import-class story is now COMPLETE
```text
Across PACKAGE-RESOLUTION-1 / TSCONFIG-PATHS-1 / PACKAGE-EXTERNAL-EVIDENCE-1 / DYNAMIC-CLASSIFICATION-1, every
import CLASS is now classified precisely. amodx's ONLY remaining module-cycle blockers are:
  - has_workspace_local_unedgeable (the RED IMPORTS-WORKSPACE-PACKAGE-EDGE probe -- src-vs-dist moniker chasm)
  - has_unresolved_after_overlay (unresolved RELATIVE imports -- a relative-resolver gap, e.g. .js->.ts)
Neither is an import-classification gap. The next levers are the workspace-edge research and relative-resolver
completeness -- both measurable now.
```

## Follow-up
```text
- relative-resolver completeness (e.g. `.js` import specifier -> `.ts` source; the remaining has_unresolved_
  after_overlay on amodx) -- the now-dominant non-RED blocker.
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
