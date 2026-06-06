# IMPORTS-RELATIVE-RESOLUTION-COMPLETE-1: ESM extension substitution for relative imports

Slice ID: IMPORTS-RELATIVE-RESOLUTION-COMPLETE-1
Status: **SPEC — awaiting ratification (2026-06-06). NOT started.** Add SAFE extension-substitution candidates
(`.js`->`.ts`/`.tsx`, `.jsx`->`.tsx`, `.mjs`->`.mts`, `.cjs`->`.cts`) so a TypeScript-ESM relative import that
writes the OUTPUT extension (`./x.js`) resolves to its SOURCE FILE (`x.ts`). Relative resolution ONLY. NO
workspace package edge, NO external package resolver, NO default migration, NO decommission, NO heuristic unsafe
remapping (inventory-only, exactly-one-match).
Depends: the import-resolver `candidate_paths` + the livegraph overlay (`resolve_imports`). Track: Stage D,
import completeness.

## Goal
```text
amodx's only remaining non-RED module-cycle blocker is has_unresolved_after_overlay (unresolved RELATIVE
imports). The dominant cause: TypeScript ESM source imports the OUTPUT extension (`import {} from '../lib/db.js'`)
which TS resolves to the SOURCE `../lib/db.ts`. The resolver's candidate set tries `../lib/db.js.ts` (the
literal base + `.ts`) and NEVER `../lib/db.ts` -> the import is unresolved. Add the standard TS extension
substitutions so these resolve to a real FILE node.
```

## Grounding (EXECUTED 2026-06-06)
```text
amodx: 454 RELATIVE imports with a `.js` extension (`../lib/db.js`, `../auth/context.js`, `../auth/policy.js`,
  ...) + 1 `.tsx`. These are the dominant has_unresolved_after_overlay cause. ZERO `.jsx`/`.mjs`/`.cjs` in amodx
  (but they are standard TS-ESM forms -> include for correctness).
current candidate_paths (import-resolver:217) -> [base.ts, base.tsx, base.d.ts, base.mts, base.cts,
  base/index.ts, base/index.tsx]. For base `.../db.js` it tries `.../db.js.ts` (no such file) -> NO match ->
  StaticUnresolved survives the overlay -> has_unresolved_after_overlay.
the fix is LOCALIZED to candidate_paths (the overlay's resolve_imports consumes it); resolve_imports already
  does exactly-one-match (>1 -> Ambiguous), so ambiguity handling is unchanged. The ingest's intra-partition
  StaticResolved path uses the producer's resolved_path (not candidate_paths); a `.js` import that the producer
  did not normalize is StaticUnresolved -> the overlay resolves it (an AstImportFileInventoryResolved edge),
  functionally equivalent for cycles -> NO ingest change needed.
```

## Forced decisions (to ratify at sign-off) — every cell filled

### D1 — the substitution set (the user's expected policy)
```text
When the normalized base ends in a JS-family OUTPUT extension, ADD the SOURCE candidates (the standard TS
moduleResolution mapping):
  `.js`  -> `<stem>.ts`, `<stem>.tsx`   (a .js import is a .ts OR .tsx source)
  `.jsx` -> `<stem>.tsx`
  `.mjs` -> `<stem>.mts`
  `.cjs` -> `<stem>.cts`
(stem = base with the `.js`-family extension stripped). KEEP the existing 7 candidates (they cover the
extensionless + index forms). RECOMMENDATION: as written. (`.js`->`.d.ts` -- a type-only output -- is a possible
follow-up; the brief lists `.ts`/`.tsx`, so v1 stops there.)
```

### D2 — ambiguity (the user's #5)
```text
The resolver collects ALL inventory matches across the FULL candidate set and emits an edge ONLY when EXACTLY
ONE FILE matches; >1 distinct FILE -> `Ambiguous` -> NO edge (blocks). This is the EXISTING resolve_imports
behaviour -- the substitution candidates just join the set. So `./x.js` with both `x.ts` AND `x.tsx` present ->
Ambiguous -> blocks (never silently picked). RECOMMENDATION: reuse the existing handling unchanged.
```

### D3 — where + scope
```text
candidate_paths (import-resolver) ONLY -- consumed by the overlay's resolve_imports (relative StaticUnresolved
+ literal-relative dynamics). NO ingest change (the overlay covers it). INVENTORY-ONLY: no filesystem probing,
no package.json/exports, no node_modules. Relative specifiers ONLY (the substitution applies to a normalized
relative base; bare/package specifiers are untouched). RECOMMENDATION: as written.
```

## Validation (EXECUTED later)
```text
- amodx: the 454 `.js` relative imports resolve to their `.ts` sources -> has_unresolved_after_overlay DROPS
  (toward false, modulo any genuinely-missing/cross-partition-unloaded relatives). amodx's remaining blocker
  collapses toward just workspace-local (RED).
- a unit fixture: `./x.js` with `x.ts` present -> resolved edge; `./x.jsx`->`x.tsx`; `./x.mjs`->`x.mts`;
  `./x.cjs`->`x.cts`. `./x.js` with BOTH `x.ts` and `x.tsx` -> Ambiguous (no edge). `./x.js` with no source ->
  unresolved (blocks).
- xpart fixture: unchanged (its relatives are extensionless -> already resolve). repo-graph: unchanged.
- full gate. (No IR/cert change -> no warm-cache bump, no policy version bump.)
```

## Out of scope (hard guardrails)
```text
NO workspace package edge, NO external package resolver, NO default migration, NO decommission. NO filesystem
probing beyond the FILE inventory. NO package/exports. NO unsafe heuristic remapping -- only the standard,
deterministic TS extension substitutions, exactly-one-match. Bare/package specifiers untouched.
```

## Build contract (PROPOSED — gated on ratification)
```text
1. import-resolver: candidate_paths returns the existing 7 PLUS the substitution candidates when the base ends
   in `.js`/`.jsx`/`.mjs`/`.cjs` (return a Vec, or a sized superset). Pure; unit-tested (each substitution +
   ambiguity + no-source).
2. NO other crate changes (the overlay's resolve_imports already calls candidate_paths; the ambiguity handling
   is reused). NO IR/cert/audit/warm-cache change -> NO policy version bump.
3. live: amodx has_unresolved_after_overlay drops; gate; completion doc.
Stop if a `.js` base also legitimately names a real `x.js.ts` file in the inventory (a literal `.js.ts` source)
-> both the literal and the substitution match -> Ambiguous (the existing safe behaviour); record if observed.
```

## Follow-up
```text
- `.js`->`.d.ts` substitution (a type-only output import), if a repo needs it.
- IMPORTS-WORKSPACE-PACKAGE-EDGE (research): once has_unresolved_after_overlay clears, workspace-local (RED) is
  amodx's SOLE remaining blocker -> the default-migration readiness is a clean measurement.
```

## References
- `rust/crates/repo-graph-import-resolver/src/lib.rs:217` (`candidate_paths` -- the extension/index list to extend)
- `rust/crates/repo-graph-import-resolver/src/lib.rs:260` (`resolve_imports` -- exactly-one-match / Ambiguous, reused)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`rebuild_xpart_overlay` -- the consumer; no change needed)
- `docs/slices/imports-dynamic-classification-1.md` (the prior slice; amodx's remaining blockers = workspace-local + unresolved-relative)
