# IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1: node_modules / lock-closure external evidence

Slice ID: IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-06), D1–D4 ratified.** A non-relative specifier resolving to a
REAL node_modules/@types install (realpath outside the repo source) is now benign `ExternalPackageNonLocal`,
captured at ingest (`external_node_modules`), with the WORKSPACE MAP taking precedence (the trust hinge). Live:
amodx `has_unresolved_package` -> false; `@amodx/shared` still workspace-local; xpart Complete. See
**Completion**. NO package workspace edge, NO default migration, NO raw decommission.
Depends: IMPORTS-PACKAGE-RESOLUTION-1 (the classifier; the declared-dep external rule this extends). Track:
Stage D, import completeness.

## Goal
```text
PACKAGE-RESOLUTION-1 marks a bare specifier ExternalPackageNonLocal (benign) ONLY when its package name is a
DIRECT dependency in the importing partition's package.json. That is too strict for TRANSITIVELY-pulled imports
(e.g. admin imports `@tiptap/core`, but declares `@tiptap/react` which pulls it) and TYPE-ONLY imports (backend
imports `aws-lambda`, whose types are `@types/aws-lambda`). Those fall to `PackageUnresolved` and BLOCK. Add
positive external evidence from node_modules/@types presence (or the lock closure) so they become benign.
```

## Grounding (EXECUTED 2026-06-06) — the measurement that justifies + bounds this
```text
Read-only classification of every non-relative amodx import (scripts/measure-amodx-import-residual.py),
replicating the daemon's rule + adding a node_modules/@types check:
  declared_external (benign)               588 occ / 69 distinct
  tsconfig_alias (resolved, PATHS-1)       386 occ / 91 distinct
  TRANSITIVE_external (node_modules/@types) 110 occ /  4 distinct  <- aws-lambda 97, lucide-react 11,
                                                                      @tiptap/core 1, domhandler 1
  workspace_local (RED probe, blocks)       63 occ /  5 distinct
  dynamic (blocks)                           6 occ
  node_builtin (benign)                      4 occ
  TRUE_unknown                               0 occ /  0 distinct   <- ZERO
=> the ENTIRE PackageUnresolved residual is node_modules/@types-resolvable externals (110/110); there are NO
   true unknowns. After this slice amodx's has_unresolved_package -> FALSE; the only remaining blockers are
   workspace-local (RED, can't edge) + dynamic. Strongly justified, tightly bounded.
```

## THE TRUST HINGE (load-bearing)
```text
node_modules presence is POSITIVE external evidence ONLY when it is a REAL external package, NOT a WORKSPACE
package that happens to be symlinked into node_modules:
- A workspace package (@amodx/shared) is symlinked node_modules/@amodx/shared -> packages/shared (INSIDE the
  repo). It MUST stay workspace-local (blocking), NEVER benign-external.
- RULE: external evidence = node_modules/<pkg> (or @types/<pkg>) exists AND its REALPATH is OUTSIDE the repo
  root (a real install or a pnpm-store symlink), AND the WORKSPACE MAP takes precedence (workspace-local first).
- STILL: do not infer external from ABSENCE in the workspace map; an unknown specifier NOT in node_modules
  stays PackageUnresolved (blocks). Workspace-local stays blocking unless edged (RED). Under-classify is safe.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — the external evidence source
```text
A. node_modules dir resolution (per partition): `<pkg>` or `@types/<pkg>` exists under the partition's (or
   root) node_modules AND realpaths OUTSIDE the repo. GROUND TRUTH of what resolves; simple; handles @types.
   [RECOMMENDED]
B. lockfile dependency closure (package-lock.json / pnpm-lock.yaml at the repo root): the canonical external
   dependency name set. More precise (excludes junk; marks workspace links), but FORMAT-SPECIFIC parsing.
C. both (A primary, B to corroborate / when node_modules absent).
RECOMMENDATION: A (with the realpath-outside-repo safety + the @types fallback). B DEFERRED -- A is sufficient
for the measured residual (node_modules present in the dev/validation environment), and lock parsing is
npm/pnpm/yarn-specific. If a CI/headless run lacks node_modules, fall back to B (a follow-up).
```

### D2 — where the evidence is captured (the boundary)
```text
A. per-OBSERVATION at INGEST: for each non-relative observation, the ingest checks node_modules/@types
   (realpath-outside-repo) -> a captured flag. Targeted (only imported specifiers, not all of node_modules);
   the IR carries the flag; the livegraph classifier stays IO-free. [RECOMMENDED]
B. capture the FULL node_modules package set at ingest -> IR. Heavy (thousands of names).
C. check in the livegraph snapshot. NO -- the snapshot is IO-free; node_modules is FS.
RECOMMENDATION: A -- the ingest already reads the partition root (read_package_manifest); add a node_modules
resolution check per non-relative observation, recording `external_node_modules: bool` (a new ImportObservation
field, or a refined ingest classification). The livegraph classifier consumes the flag; precedence: workspace
map -> workspace-local; else (declared dep OR external_node_modules) -> ExternalPackageNonLocal; else blocks.
```

### D3 — classifier precedence (extends PACKAGE-RESOLUTION-1)
```text
node:/builtin -> external. THEN workspace map -> WorkspaceLocalUnedgeable (blocks; a workspace symlink in
node_modules NEVER overrides this). THEN tsconfig alias (PATHS-1). THEN (declared dep OR node_modules-external)
-> ExternalPackageNonLocal (benign). ELSE -> PackageUnresolved (blocks). Dynamic stays separate (blocks).
RECOMMENDATION: as written -- the node_modules signal is added as a SECOND external evidence alongside declared
deps, AFTER the workspace-map precedence (the trust hinge).
```

### D4 — cert policy (no change)
```text
NO cert change: ExternalPackageNonLocal is ALREADY benign (PACKAGE-RESOLUTION-1 D5). This slice only moves MORE
specifiers into that benign class -> has_unresolved_package shrinks (-> false for amodx). The import-completeness
policy version bumps (3 -> 4): the external-evidence policy changed -> prior certificates re-evaluate.
RECOMMENDATION: bump the policy version; no ObservationClassSummary change.
```

## Validation (EXECUTED later)
```text
- amodx: the 110 node_modules/@types externals (aws-lambda/lucide-react/@tiptap/core/domhandler) -> benign ->
  has_unresolved_package = FALSE. Remaining blockers = workspace_local (@amodx/*, RED) + dynamic. Cert stays
  IncompleteImportClasses (honest -- workspace-local + dynamic).
- a workspace package symlinked in node_modules (@amodx/shared) -> STAYS WorkspaceLocalUnedgeable (the trust
  hinge: realpath inside repo / workspace-map precedence) -- NEVER flipped to benign.
- an unknown bare specifier NOT in node_modules -> stays PackageUnresolved (blocks).
- xpart fixture: unchanged (no package imports). repo-graph: unchanged (non-TS precedence).
- full gate; warm-cache round-trip for any new IR field.
```

## Out of scope (hard guardrails)
```text
NO package workspace edge (workspace-local stays blocking; that is the RED IMPORTS-WORKSPACE-PACKAGE-EDGE-1).
NO default migration, NO raw decommission. NO inferring external from absence in the workspace map. Dynamic
stays blocking. Lock-closure parsing (D1-B) deferred unless node_modules is unavailable.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. ingest: per non-relative observation, a node_modules/@types resolution check (realpath OUTSIDE repo) ->
   record external_node_modules (a new ImportObservation field; warm-cache DTO round-trip, serde(default)).
2. import-resolver: classify_package_import gains the node_modules-external signal (D3 precedence). Unit-tested
   (declared, node_modules-external, @types, workspace-symlink-not-external, unknown-not-in-nm).
3. livegraph snapshot: pass the per-observation external_node_modules flag into the classification.
4. audit: policy version 3 -> 4; the report already shows the obs classes.
5. live: amodx has_unresolved_package -> false; gate; completion doc.
Stop if distinguishing a workspace symlink from a real external requires more than realpath-outside-repo (e.g.
a pnpm store layout that aliases workspace packages) -> present a matrix.
```

## Completion (implemented + live-validated 2026-06-06, EXECUTED)

Commits: `afae08f` (spec + measurement) + the impl/docs commits below. Ratified D1–D4.

### What landed
```text
IR: ImportObservation + external_node_modules: bool (captured at ingest). Warm-cache CacheImportObservationDto
  round-trip + SCHEMA_VERSION 5 -> 6 (forces a clean re-ingest for the new evidence).
scip-ingest: resolves_external_node_modules(root, repo_root, pkg) -- checks the partition + repo-root
  node_modules (and @types/<pkg>) and CANONICALIZES: external iff the realpath has a `node_modules` segment OR
  is outside the canonical repo root (a workspace symlink resolving INTO the repo source is NOT external);
  conservative on any canonicalize failure (-> false, blocks). Memoized per package; runs at the ingest
  boundary, NOT the classifier. repo_root = partition root minus the repo-relative prefix.
import-resolver (PURE): classify_package_import gains external_node_modules. Precedence (the trust hinge):
  node:/builtin -> external; WORKSPACE map -> WorkspaceLocalUnedgeable (BEFORE node_modules); declared dep OR
  external_node_modules -> ExternalPackageNonLocal; else PackageUnresolved.
livegraph snapshot: passes obs.external_node_modules into the classifier. cert: NO change; audit policy 3 -> 4.
```

### Live validation (EXECUTED 2026-06-06)
```text
amodx (8/8 loaded) -> IncompleteImportClasses, policy_version=4:
    has_external_nonlocal_benign   = true
    has_workspace_local_unedgeable = true   <- @amodx/shared STILL blocks despite its node_modules symlink
                                              (the trust hinge: workspace precedence held)
    has_unresolved_package         = FALSE  <- the 110 node_modules/@types externals (aws-lambda/lucide-react/
                                              @tiptap/core/domhandler) are now BENIGN; was true
    has_alias_unresolved           = false
    has_dynamic                    = true   (blocks)
    has_unresolved_after_overlay   = true   (relative StaticUnresolved -- a separate axis, pre-existing)
  => the package-external residual is GONE; the remaining blockers are workspace-local (RED) + dynamic +
     unresolved-relative. The cert is honestly IncompleteImportClasses (not Complete) on those.
xpart fixture -> CompleteForModuleImportCycles (permits_default=true) -- REGRESSION INTACT.
```

### Acceptance (D4) — PASS
```text
1. amodx has_unresolved_package = false                                                          PASS.
2. @amodx/shared still blocks as workspace-local despite the node_modules symlink (trust hinge)   PASS
   (has_workspace_local_unedgeable=true; the unit test workspace_local_takes_precedence...node_modules covers it).
3. dynamic still blocks                                                                            PASS.
4. xpart remains Complete                                                                           PASS.
Gate: workspace tests ok / 0 failures; clippy -D warnings clean; fmt clean. Resolver classify tests (incl.
node_modules-external-benign + workspace-precedence-over-node_modules) + warm-cache v6 round-trip green.
```

### Provisioning note (environment, unchanged from PATHS-1)
```text
Live re-ingest uses the DURABLE scip-typescript producer under ~/.local/share/repo-graph-tools (the /private/tmp
install stays cleaned by macOS); the daemon points at it via RMAP_SCIP_TYPESCRIPT. The dev-install launcher
still races on validation in this environment; rmapd was restarted manually (exact `pkill -x rmapd`). No
production daemon behaviour changed.
```

## Follow-up
```text
- lockfile dependency-closure evidence (D1-B) for headless/CI runs without node_modules.
- IMPORTS-WORKSPACE-PACKAGE-EDGE (research): the remaining workspace-local block (RED probe).
- once amodx's blocking set is only dynamic (+ the RED workspace-local), revisit dynamic-import literals + the
  default migration readiness.
```

## References
- `rust/crates/repo-graph-import-resolver/src/lib.rs` (`classify_package_import` -- the external rule this extends)
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` (`read_package_manifest` -- the ingest-boundary FS read precedent)
- `rust/crates/repo-graph-ir/src/lib.rs` (`ImportObservation` -- the new `external_node_modules` field)
- `scripts/measure-amodx-import-residual.py` (the decisive measurement: 110/110 node_modules externals, 0 unknowns)
- `docs/slices/imports-package-resolution-1.md` (the declared-dep external rule + the trust hinge)
