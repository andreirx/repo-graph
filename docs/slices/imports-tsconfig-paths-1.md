# IMPORTS-TSCONFIG-PATHS-1: resolve tsconfig path aliases (`@/lib/*`) to source FILE edges

Slice ID: IMPORTS-TSCONFIG-PATHS-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-06), D1–D4 ratified.** tsconfig `paths`/`baseUrl` aliases
(`@/lib/api`) now resolve to source FILE edges (basis `AstImportTsconfigPathResolved`); a resolved alias stops
blocking, an unresolved one -> `has_alias_unresolved`. Captured at the INGEST boundary (JSONC via `json5`),
resolved by the pure resolver, edged in the runtime overlay. Live: amodx `@/lib/*` resolve; xpart stays
Complete. See **Completion**. NO workspace package imports, NO external package resolver, NO default migration,
NO raw decommission, NO heuristic package-entry remapping.
Depends: IMPORTS-PACKAGE-RESOLUTION-1 (the classifier; `@/` was `PackageUnresolved`), the import-resolver
relative resolution. Track: Stage D, import completeness.

## Goal
```text
After IMPORTS-PACKAGE-RESOLUTION-1, external npm is benign and workspace-local is honestly blocking (RED probe).
The remaining ACTIONABLE blocking class is tsconfig path aliases: `@/lib/api` is neither relative nor a workspace
package nor a declared dep -> currently `PackageUnresolved` (blocks). But it resolves, via the partition's own
tsconfig `paths` + `baseUrl`, to a LOCAL SOURCE file (admin/src/lib/api.ts) -- a real intra-partition import edge.
Expand the alias, reuse the relative-resolution rules, emit the FILE edge, stop blocking.
```

## Grounding (EXECUTED 2026-06-05)
```text
amodx admin/tsconfig.json: extends=None, baseUrl=".", paths={"@/*": ["./src/*"]}. So `@/lib/api` -> baseUrl-rel
  `./src/lib/api` -> admin/src/lib/api.ts (CONFIRMED: admin/src/lib has api.ts, csv-headers.ts, upload.ts).
  INTRA-PARTITION, resolves to INDEXED SOURCE (no dist). Aliases are PER-PARTITION (no extends); backend has no
  paths; renderer/tsconfig.json is JSONC (comments/trailing commas -> a strict JSON parse FAILS).
ts-extractor: `@/lib/api` -> raw_specifier="@/lib/api", is_relative=false, resolved_path=None -> classified
  PackageExternal -> (PACKAGE-RESOLUTION-1) PackageUnresolved (not workspace, not declared) -> BLOCKS today.
resolver (REUSABLE): normalize_join(dir,spec) (line 229) + candidate_paths (211: base.ts/.tsx/.d.ts +
  /index.ts ...) + resolve_imports(inventory, candidates) (260). After alias expansion -> a partition-rel path
  -> the SAME extension/index match against the global FILE inventory. EdgeBasis (ir:88) precedent:
  AstImportFileInventoryResolved (derived, runtime-only).
=> alias resolution = (expand via paths/baseUrl + partition prefix) THEN the existing relative machinery. Clean.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — alias metadata source (the user's #1)
```text
A. parse tsconfig at the BOUNDARY (ingest) -> a TsconfigPaths DTO in PartitionIr; the pure resolver gets the DTO
   (stays IO-free).                                                                              [RECOMMENDED]
B. parse inside the resolver. NO -- the resolver is pure/IO-free (its contract); tsconfig is on disk.
C. parse in the daemon/audit per query. NO -- per-query IO; the alias config is partition-stable.
RECOMMENDATION: A. scip-ingest reads {root}/tsconfig.json (alongside read_package_manifest) with a JSONC-TOLERANT
parser (tsconfig is JSONC -- comments + trailing commas; REUSE a crate e.g. json5 / jsonc-parser, do NOT
reinvent), captures {baseUrl, paths} + the partition repo-relative PREFIX (needed for baseUrl="." resolution)
into the IR. The resolver expands + matches purely.
```

### D2 — scope (the user's #2)
```text
A. explicit `paths` + `baseUrl` in the partition's OWN tsconfig (wildcard `@/*` + prefix/exact).  [RECOMMENDED]
B. baseUrl bare-import resolution (a non-aliased bare specifier resolved against baseUrl).        [include: cheap]
C. extends / inherited configs.                                                                   [DEFER]
RECOMMENDATION: A + B for v1 (amodx admin: baseUrl="." + paths {"@/*": ["./src/*"]}, no extends -> fully covered).
Support wildcard (`@/*`) AND exact (`@foo` -> `./x`) path entries; first-match per tsconfig `paths` semantics
(try each mapping target, first that resolves wins; MULTIPLE targets resolving -> Ambiguous, surfaced not picked).
C (extends) DEFERRED -- amodx needs none; a follow-up flattens extends if a repo requires it.
```

### D3 — edge basis (the user's #3)
```text
A NEW `EdgeBasis::AstImportTsconfigPathResolved` -- DISTINCT from AstImport (intra, AST-direct) and
AstImportFileInventoryResolved (relative cross-partition). Maps to EdgeType::Imports; runtime-only, NEVER
persisted (same discipline as the inventory-resolved basis). Honest provenance: "resolved via tsconfig paths".
RECOMMENDATION: as written. The edge is a real FILE->FILE edge to an INDEXED source file (no dist heuristic).
```

### D4 — cert policy (the user's #4)
```text
A RESOLVED alias -> a captured edge -> NOT blocking (it leaves the PackageUnresolved bucket). An UNRESOLVED
alias (matches a `paths` prefix but no FILE matches, or Ambiguous) -> a NEW `has_alias_unresolved` blocking flag
(distinct from has_unresolved_package, so the report shows "alias that did not resolve" vs "unknown bare
specifier"). Workspace-local (WorkspaceLocalUnedgeable) + dynamic + relative-unresolved STILL block.
RECOMMENDATION: as written. Bumps IMPORT_COMPLETENESS_POLICY_VERSION 2 -> 3 (the policy gained alias resolution).
Classification PRECEDENCE: a specifier matching a tsconfig `paths` prefix is an ALIAS (resolve it) BEFORE the
package classification (workspace/external/unresolved) -- `@/` is unambiguously the alias, not a package.
```

## Validation (EXECUTED later)
```text
- amodx: `@/lib/*` (43 occurrences) -> AstImportTsconfigPathResolved edges to admin/src/lib/*; the audit's
  has_unresolved_package DROPS the alias contribution; remaining blockers = workspace-local-unedgeable
  (@amodx/shared) + dynamic + any genuinely-unresolved. The blocking set shrinks again.
- a partition with a `paths` entry whose target FILE is absent -> has_alias_unresolved (blocks, honest).
- xpart fixture: unchanged (no aliases). repo-graph: unchanged headline (non-TS precedence).
- NO false edge: an alias edge requires an actual inventory FILE match (extension/index); Ambiguous -> surfaced,
  never picked. JSONC parse failure on a partition -> no aliases for it (safe: its `@/` imports stay blocking).
- full gate.
```

## Out of scope (hard guardrails)
```text
NO workspace package imports (stay WorkspaceLocalUnedgeable), NO external package resolver, NO default migration,
NO raw decommission, NO heuristic package-entry remapping, NO dist->src. extends/inherited configs DEFERRED.
Dynamic imports stay blocking. Only paths/baseUrl in the partition's own (JSONC) tsconfig.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. IR: Partition gains tsconfig alias config (baseUrl + paths mappings + the repo-relative prefix). Captured at
   ingest via a JSONC-tolerant parse of {root}/tsconfig.json (REUSE a crate; missing/malformed -> no aliases, safe).
   Round-trips through the warm-cache DTO (serde(default), like package_name).
2. import-resolver (PURE): expand_tsconfig_alias(specifier, paths, baseUrl, prefix) -> Option<repo-relative base>
   (wildcard + exact; Ambiguous if >1 target resolves) THEN the existing candidate_paths/inventory match. + a new
   EdgeBasis. Unit-tested (wildcard, exact, miss, ambiguous, no-config).
3. livegraph snapshot/overlay: BEFORE the package classification, try alias expansion for a PackageExternal obs;
   resolved -> emit the FILE edge (new basis, into the overlay so it aggregates to file+module cycles) + count it
   captured; matched-prefix-but-unresolved/Ambiguous -> has_alias_unresolved; no prefix match -> fall through to
   the package classification.
4. cert (module_cycle_cert): ObservationClassSummary + has_alias_unresolved (blocks); evaluate + fingerprint;
   policy version 2 -> 3. audit reports the alias breakdown.
5. live: amodx alias edges resolve + the audit shows has_unresolved_package shrink; gate; completion doc.
Stop if: tsconfig `paths` semantics need full Node/TS module resolution (conditional exports, nested baseUrl) to
disambiguate -> present a matrix (v1 is prefix/wildcard + extension/index ONLY).
```

## Completion (implemented + live-validated 2026-06-06, EXECUTED)

Commits: `30c852e` (spec) + the impl/docs commits below. Ratified D1–D4.

### What landed
```text
dep: json5 = "0.4" (scip-ingest) -- the ratified JSONC parser (REUSE, not hand-rolled). Parse failure -> no
  aliases (SAFE).
IR: Partition + tsconfig_aliases: Option<TsconfigAliasConfig{base_url, paths, partition_prefix}> (captured at
  ingest) + a new EdgeBasis::AstImportTsconfigPathResolved (runtime-only, never persisted). Round-trips through
  the warm-cache DTO (serde(default)).
scip-ingest: read_tsconfig_aliases({root}/tsconfig.json via json5) -> the config; partition_prefix captured for
  baseUrl resolution.
import-resolver (PURE): resolve_tsconfig_alias(specifier, config, inventory) -> {NotAnAlias | Resolved(file) |
  Unresolved | Ambiguous}: match a `paths` pattern (wildcard + exact), substitute, join baseUrl+prefix, then the
  SAME candidate_paths/inventory match as a relative import; >1 distinct file -> Ambiguous (NEVER picked). +
  specifier_matches_any_alias (the snapshot's is-alias test).
livegraph: the overlay (rebuild_xpart_overlay) emits a FILE->FILE alias edge on Resolved (basis
  AstImportTsconfigPathResolved); the snapshot classifies a PackageExternal obs: overlay-resolved -> captured;
  matched-a-pattern-but-not-resolved -> has_alias_unresolved; else -> package classification (alias precedes
  package).
cert: ObservationClassSummary + has_alias_unresolved (BLOCKS); evaluate + fingerprint; audit reports it; policy
  version 2 -> 3.
```

### Live validation (EXECUTED 2026-06-06; durable producer reprovisioned -- see note)
```text
amodx (8/8 loaded) -> IncompleteImportClasses, policy_version=3:
    has_external_nonlocal_benign   = true   (react/@tiptap/node: -> benign)
    has_workspace_local_unedgeable = true   (@amodx/shared -> blocks; the RED-probe honest block)
    has_unresolved_package         = true   (transitive externals + @/ from an extends/app-config -> blocks)
    has_alias_unresolved           = false  (every @/ in a partition whose tsconfig.json HAS the paths resolved)
    has_dynamic                    = true   (blocks)
  PROOF the @/lib aliases RESOLVED (not silently fell through): admin's warm cache persists the alias config
  (`./src/*` present) AND has_alias_unresolved=false -- the snapshot clears that flag ONLY via overlay
  membership, and the overlay adds an alias edge ONLY on Resolved. So admin's `@/lib/*` -> AstImportTsconfigPath
  FILE edges to admin/src/lib/*. The blocking set shrank (the @/ that resolve left has_unresolved_package).
xpart fixture -> CompleteForModuleImportCycles (permits_default=true; all obs false) -- REGRESSION INTACT.
```

### Acceptance (D4) — PASS
```text
1. amodx `@/lib/*` resolve (config captured + has_alias_unresolved=false -> edges)            PASS.
2. external package noise remains benign                                                       PASS (has_external_nonlocal_benign).
3. workspace-local unedgeable remains blocking                                                 PASS.
4. dynamic remains blocking                                                                     PASS.
5. xpart fixture remains Complete                                                               PASS.
Gate: workspace tests ok / 0 failures; clippy -D warnings clean; fmt clean. 5 alias-resolver + 23 module_cycle
+ the warm-cache round-trip tests green.
```

### Honest caveat (recorded)
```text
A `@/` import resolves ONLY when the partition's OWN `tsconfig.json` carries the `paths` (extends/app-config is
DEFERRED). amodx backend's `@/` (its paths live elsewhere) therefore stay PackageUnresolved -> has_unresolved_
package=true is partly from that + transitive externals. That is honest (no extends-flattening in v1); the
follow-up handles extends.
```

### Provisioning note (environment, not the slice)
```text
The scip-typescript@0.4.0 producer was installed under the EPHEMERAL /private/tmp and macOS cleaned part of it
mid-session, breaking re-ingest. Reprovisioned DURABLY under ~/.local/share/repo-graph-tools/scip-typescript-
0.4.0 (+ a copied Node 18; node_modules NOT committed); the daemon points at it via RMAP_SCIP_TYPESCRIPT. No
production daemon behaviour changed. Verified with a minimal `scip-typescript index` before re-validating.
```

## Follow-up
```text
- extends/inherited tsconfig flattening (if a target repo needs it).
- the daemon RUNTIME wiring (cache the BaselineInput) + CYCLES-DEFAULT-MIGRATION-1 (un-deferred) -- once a real
  TS repo's blocking set is empty (no workspace-local, no alias, no dynamic) it can reach Complete.
- IMPORTS-WORKSPACE-PACKAGE-EDGE (research): declaration-map emission OR a unified-index mode (the RED blocker).
```

## References
- `rust/crates/repo-graph-import-resolver/src/lib.rs:211,229,260` (candidate_paths, normalize_join, resolve_imports — reuse)
- `rust/crates/repo-graph-ir/src/lib.rs:88` (`EdgeBasis` — add `AstImportTsconfigPathResolved`)
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` (`read_package_manifest` — the ingest-boundary parse precedent)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (`ObservationClassSummary` — the D4 amendment target)
- `docs/slices/imports-package-resolution-1.md` (`@/` currently PackageUnresolved; the classifier this extends)
- `docs/slices/imports-workspace-package-edge-1.md` (the RED probe — why aliases, not packages, are next)
