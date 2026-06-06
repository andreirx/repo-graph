# IMPORTS-ASSET-AND-LITERAL-EXT-1: asset imports benign + literal-source-extension resolution

Slice ID: IMPORTS-ASSET-AND-LITERAL-EXT-1
Status: **IMPLEMENTED + LIVE-VALIDATED (2026-06-06), D1–D3 ratified.** A relative NON-CODE ASSET import
(`.css`/`.svg`/...) is benign (`has_asset_nonrelevant`); a literal-source-extension import (`./App.tsx`) resolves
to the exact FILE. Live: amodx `has_unresolved_after_overlay` = FALSE -> **workspace-local (RED) is now amodx's
SOLE remaining module-cycle blocker** (import classification COMPLETE). xpart Complete. See **Completion**. NO
workspace package edge, NO default migration, NO decommission, NO package resolver, NO broad asset graph.
Depends: IMPORTS-RELATIVE-RESOLUTION-COMPLETE-1 (the resolver + the snapshot StaticUnresolved arm), the cert
ObservationClassSummary. Track: Stage D, import completeness.

## Goal
```text
After RELATIVE-RESOLUTION-COMPLETE-1, amodx has exactly 3 unresolved relatives keeping has_unresolved_after_
overlay true: 2 CSS asset imports + 1 literal-`.tsx`. Neither is a real code-import gap: a `.css` import is not a
TS module (cannot be in a repo-local module cycle); `./App.tsx` IS a real source file the resolver simply does
not try as-is. Classify assets benign + resolve the literal source extension -> amodx's relative axis clears.
```

## Grounding (EXECUTED 2026-06-06)
```text
amodx assets: ONLY 2 -- `import './globals.css'`, `import './index.css'` (side-effect CSS). ZERO
  .scss/.sass/.less/.svg/images/fonts in amodx (the allowlist is for GENERALITY, not amodx-specific).
amodx literal-source-ext: ONLY 1 -- `from './App.tsx'` (App.tsx is a real indexed FILE).
current: a `.css` relative import -> StaticUnresolved (no TS FILE) -> has_unresolved_after_overlay. `./App.tsx`
  -> base `.../App.tsx` -> candidate_paths APPENDS (App.tsx.ts ...) and never tries the base AS-IS -> unresolved.
=> two small fixes close the exact 3.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — asset imports (the user's #1)
```text
A CLOSED allowlist of NON-CODE asset extensions -> a relative import ending in one is NON-CYCLE-RELEVANT
(benign, reported, NOT an edge, NOT blocking). Allowlist (explicit; UNKNOWN extensions are NEVER benign):
  styles: css scss sass less styl
  images: svg png jpg jpeg gif webp avif ico bmp
  fonts : woff woff2 ttf eot otf
(`.json` is DATA, NOT in the allowlist -- a separate decision.) WHERE: the livegraph snapshot classifies a
RELATIVE specifier ending in an allowlisted asset extension -> a NEW `has_asset_nonrelevant` benign flag, BEFORE
the relative-resolution attempt (so it never reaches has_unresolved_after_overlay). Covers static + literal
dynamic relative asset imports. RECOMMENDATION: as written. CONSERVATIVE: only the closed allowlist; an unknown
relative extension stays a normal (blocking-if-unresolved) code import.
```

### D2 — literal source-extension resolution (the user's #2)
```text
In the resolver: if the normalized base ends in a SOURCE extension (`.ts` `.tsx` `.mts` `.cts` `.d.ts`) AND the
base is EXACTLY a FILE node in the inventory -> resolve to it DIRECTLY (exclusive). The exact source-extension
match WINS and is NOT subject to Ambiguity with the appended candidates (`App.tsx.ts` etc.) -- "exact FILE node
wins only if exactly present". If the literal base is NOT a FILE node -> fall through to the normal candidate_
paths matching (which may still resolve/Ambiguous/NotFound). RECOMMENDATION: as written -- a pre-check in
resolve_imports before the candidate set; emits the normal AstImportFileInventoryResolved edge.
```

### D3 — cert policy (the user's #3)
```text
ObservationClassSummary + `has_asset_nonrelevant` (BENIGN, reported, does NOT block -- like has_external_
nonlocal). A literal-source-extension import that resolves -> a captured edge (the existing overlay path) ->
stops blocking (no new flag). An UNRESOLVED CODE import (not asset, not resolved) STILL blocks
(has_unresolved_after_overlay). Policy version 5 -> 6 (the import-completeness policy gained the asset class).
RECOMMENDATION: as written.
```

## Validation (EXECUTED later)
```text
- amodx: the 2 CSS imports -> has_asset_nonrelevant (benign); `./App.tsx` -> resolved edge. ->
  has_unresolved_after_overlay = FALSE. amodx's SOLE remaining module-cycle blocker becomes workspace-local
  (RED). (has_external_nonlocal/has_workspace_local_unedgeable/has_asset_nonrelevant true; the rest false.)
- unit: `./x.css`/`.svg`/`.woff2` -> has_asset_nonrelevant (no edge, no block); `./App.tsx` with App.tsx present
  -> resolved exact; `./App.tsx` with NO App.tsx -> unresolved (blocks); an unknown ext `./x.weird` -> NOT
  benign (a normal unresolved code import).
- xpart fixture: unchanged (extensionless relatives). repo-graph: unchanged.
- full gate; warm-cache: no IR shape change (has_asset_nonrelevant is snapshot-derived, not persisted) -> the
  policy version bumps but NO SCHEMA bump.
```

## Out of scope (hard guardrails)
```text
NO workspace package edge, NO default migration, NO decommission, NO package resolver. NO broad ASSET GRAPH
(assets are completeness EVIDENCE, never nodes/edges). NO `.json`/data-module handling. Only the CLOSED asset
allowlist + the exact source-extension match; unknown extensions are NEVER benign.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. import-resolver: an `is_asset_specifier(spec)` helper (the closed allowlist, by extension) + the literal
   source-extension exact pre-check in resolve_imports. Pure; unit-tested.
2. livegraph snapshot: a relative specifier (static StaticUnresolved OR literal-relative dynamic) ending in an
   asset extension -> has_asset_nonrelevant (BEFORE the overlay-resolved check); else the existing path.
3. cert: ObservationClassSummary + has_asset_nonrelevant (benign); fingerprint; audit reports it; policy 5 -> 6.
4. live: amodx has_unresolved_after_overlay -> false; gate; completion doc.
Stop if a `.json` (or other data) import turns out to be load-bearing for some repo's cycles -> present a matrix
(v1 EXCLUDES .json from the asset allowlist; it stays a normal import).
```

## Completion (implemented + live-validated 2026-06-06, EXECUTED)

Commits: `cc71951` (spec) + the impl/docs commits below. Ratified D1–D3.

### What landed
```text
import-resolver: `is_asset_specifier` (the CLOSED allowlist -- styles/images/fonts; NOT .json; unknown ext ->
  never asset) + a literal-source-extension EXACT pre-check in resolve_imports (base ends in .ts/.tsx/.mts/.cts
  AND is exactly a FILE node -> resolve directly, EXCLUSIVE of candidate-append/Ambiguity; else fall through).
livegraph snapshot: a relative specifier (static StaticUnresolved OR literal-relative dynamic) ending in an
  asset extension -> has_asset_nonrelevant (BENIGN), BEFORE the unresolved-relative check.
cert: ObservationClassSummary + has_asset_nonrelevant (benign, NOT in the blocking set); fingerprint; audit
  reports it; policy 5 -> 6. NO SCHEMA bump (snapshot-derived; no persisted IR shape change).
```

### Live validation (EXECUTED 2026-06-06)
```text
amodx (8/8) -> IncompleteImportClasses, policy_version=6:
    has_asset_nonrelevant_benign   = TRUE   (the 2 CSS imports -> benign)
    has_unresolved_after_overlay   = FALSE  <- CLEARED: 2 CSS benign + ./App.tsx resolved exact
    has_workspace_local_unedgeable = TRUE   <- the SOLE remaining blocker (RED)
    has_external_nonlocal_benign   = true ; has_unresolved_package=false ; has_alias_unresolved=false ;
    has_dynamic_unresolved=false
  => amodx now blocks ONLY on workspace-local. Every other import class is false or benign. Import
     CLASSIFICATION is COMPLETE for amodx.
xpart fixture -> CompleteForModuleImportCycles (regression intact).
```

### Acceptance (D1–D4) — PASS
```text
1. amodx has_unresolved_after_overlay = false                                                    PASS.
2. xpart remains Complete                                                                          PASS.
3. amodx blocked ONLY by workspace-local-unedgeable                                                PASS.
4. unit: asset allowlist (.css/.svg/.woff2 benign; .ts/.tsx/.json/unknown NOT); literal-source exact match;
   literal-source no-node falls through; asset benign + code still blocks (livegraph)              PASS.
Gate: workspace tests ok / 0 failures; clippy -D warnings clean; fmt clean. 3 resolver + 1 livegraph new tests.
```

### Significance — the import-classification thread is DONE
```text
Across PACKAGE-RESOLUTION-1 / TSCONFIG-PATHS-1 / PACKAGE-EXTERNAL-EVIDENCE-1 / DYNAMIC-CLASSIFICATION-1 /
RELATIVE-RESOLUTION-COMPLETE-1 / ASSET-AND-LITERAL-EXT-1, EVERY import class is now classified precisely. amodx's
SOLE remaining module-cycle blocker is has_workspace_local_unedgeable -- the RED IMPORTS-WORKSPACE-PACKAGE-EDGE
case (src-vs-dist moniker chasm). The CYCLES default-migration readiness is now a CLEAN single-variable
question: workspace-local is the only thing between amodx and a Complete certificate.
```

## Follow-up
```text
- IMPORTS-WORKSPACE-PACKAGE-EDGE (research): workspace-local (RED) is now amodx's SOLE remaining module-cycle
  blocker -> CYCLES default-migration readiness becomes a clean measurement.
- `.json`/data-module import policy, if a repo needs it.
```

## References
- `rust/crates/repo-graph-import-resolver/src/lib.rs` (`resolve_imports`/`candidate_paths` -- the exact source-ext pre-check + `is_asset_specifier`)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`module_cycle_live_state` StaticUnresolved arm -- the asset classification)
- `rust/crates/repo-graph-livegraph/src/module_cycle_cert.rs` (`ObservationClassSummary` -- + has_asset_nonrelevant)
- `docs/slices/imports-relative-resolution-complete-1.md` (the 3 remaining relatives this closes)
