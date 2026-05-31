# XPART-PROVE-1B: Export-Surface Reconciliation (Stage B, ST3 second half)

Slice ID: XPART-PROVE-1B
Status: **EXECUTED (2026-05-31) — XPART-PROVE-1B PASS (conditional).**
ST3 narrowed: declaration-map-backed named TypeScript package-boundary traversal is proven. ST3
remains open for anonymous structural members and packages without declaration maps / complex
exports. Decisions: D1 provider-source anchor → `CanonicalKey` (no neutral keys); D2
declaration-map + strict name-exact + unresolved (Basis 2 deferred); D3 alias-with-provenance,
never silent. Verdict + residuals below.
Depends: XPART-PROVE-1A (answer-class contract + the `xpart-probe` it extends), INGEST-CORE-1
(`repo-graph-ir` `CanonicalKey` + `ingest_partition`).
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`).
Addresses: **ST3, the cross-partition identity half left open by 1A.** 1A proved answer-class
semantics under an artificial source-aligned capture; real package consumption still fails
because the consumer references `dist/index.d.ts` symbols while the provider partition defines
`src/index.ts` symbols. **ST3 is not retired by either half alone; it narrows as each half
closes — and 1B shows a residual that keeps ST3 open even after both (anonymous structural
members).**

## The one risk this slice narrows

> Can a consumer-side published-interface SCIP symbol be reconciled to the provider-side source
> SCIP identity **without lying, broad heuristics, or silent misses**?

The deliverable is a reconciliation **contract**: every published-interface reference either maps
to the provider source canonical identity **with recorded provenance and basis**, or is left
explicitly unreconciled and **degrades the answer class**. No guessing; no silent attach.

## Concrete target

- `api-dist.scip` references `@fraktag/engine 0.1.0 dist/`index.d.ts`/Fraktag#`
  (and `…/Fraktag#listKnowledgeBases().`, `…/Fraktag#discoverKnowledgeBases().`, …).
- `engine.scip` defines `@fraktag/engine 0.1.0 src/index.ts/Fraktag#` (to be confirmed at probe
  runtime against the provider defs — not asserted here).
- Expected: reconcile the dist declaration symbol to the source canonical key for `Fraktag`.

`api-src.scip` is **not** the production answer. It remains a control fixture (proves the
answer-class machinery in isolation). 1B must work on the real consumer view (`api-dist.scip`).

## Measured ground truth (OBSERVED/EXECUTED, 2026-05-31)

These facts decide which bases are available and how Basis 1 is realized:

- `@fraktag/engine` `package.json`: legacy `main: dist/index.js`, `types: dist/index.d.ts`,
  **no `exports` map**, version `0.1.0`.
- `tsconfig`: `declaration: true`, `declarationMap: true`. `dist/index.d.ts.map` **exists**
  (also `dist/core/...d.ts.map`, `dist/nuggets/index.d.ts.map`). **Declaration-map basis is
  available.**
- `Fraktag` is declared **directly in the entry file on both sides** — `src/index.ts:57`
  (`export class Fraktag`) and `dist/index.d.ts:13` (`export declare class Fraktag`). The SCIP
  **descriptors are identical** (`Fraktag#`, `Fraktag#method().`); only the **file component**
  differs (`dist/index.d.ts` ↔ `src/index.ts`).
- `dist/index.d.ts` re-exports types via `export * from './core/types.js'` and
  `export type { … } from './core/...'`. Those symbols' declaring file is `dist/core/types.d.ts`,
  not `index.d.ts` → the **re-export sub-case**: same mechanism, different file, possibly an
  extra hop. This is where partial degradation will concentrate.

## Architecture direction — the export alias layer

The xref must stop treating **raw SCIP symbol equality** as cross-partition identity. Raw SCIP
symbols are *substrate* identities (per-capture, per-view). Repo-graph needs a **canonical
cross-partition identity** plus **provenance-bearing aliases**:

```text
canonical repo-graph key                       (the cross-partition identity)
  ├─ source SCIP symbol(s)                      (provider, indexed from src)
  ├─ published declaration SCIP symbol(s)       (consumer view, dist/*.d.ts)
  └─ alias provenance
       ├─ package name + version
       ├─ public export path
       ├─ package.json types/exports/main/module
       ├─ declaration file path (+ its .d.ts.map)
       └─ reconciliation basis (which rule below fired)
```

The probe builds an `ExportAliasIndex` mapping `published_symbol → canonical/source_symbol`
**with the provenance record above**. Whether the canonical anchor is the provider source SCIP
symbol or a freshly minted neutral key is **D1 below**.

## Reconciliation bases — strict order (stop at first that resolves unambiguously)

1. **`DeclarationMapExact` — declaration-map basis (strongest; available here).**
   For a published symbol `<pkg> <distFile>/<descriptor>`: read `<distFile>.map`'s `sources`
   array → the originating source file(s). If **exactly one** source file and the **same
   descriptor** exists among the provider's defs at that source file → reconcile. This uses the
   declaration map at **file-correspondence granularity** (`sources`) plus **descriptor-exact**
   matching — deterministic, no token decode. *Token-level fallback:* if descriptors diverge
   (renamed/aliased exports), decode the `.d.ts.map` VLQ mappings (reuse Sentry's `sourcemap`
   crate — do not hand-roll source-map parsing) to bridge the dist token position → src position,
   then match the provider occurrence at that position. The re-export sub-case (`export *`) is
   handled by following to the declaring dist file and applying the same rule there.

2. **Package export-surface basis — DEFERRED (not implemented in 1B).**
   `package.json` `types`/`exports`/`main`/`module` + source entrypoint exports establish
   entry-file correspondence (`dist/index.d.ts` ↔ `src/index.ts`). Subsumed by the declaration
   map for FRAKTAG (legacy `types` only); it adds **no measured value here** and opens TypeScript
   module-resolution complexity prematurely. **Deferred to a separate risk slice** for packages
   without declaration maps / with complex `exports`. See "Verdict / Residuals" below.

3. **`NameExactUnique` — exported-name exact basis (strict predicate; see ratified decisions).**
   Match only when: same package name+version, same public export path, same exported terminal
   name, **exactly one** source candidate, **exactly one** declaration candidate. The weakest
   admissible basis; cannot disambiguate overloads/same-name. Fallback when (1) and (2) cannot
   bridge.

4. **`Ambiguous` / `Unresolved` — no reconciliation.**
   `Ambiguous` = multiple candidates on either side (overload / re-export / path ambiguity).
   `Unresolved` = missing map or descriptor not found among provider defs. Either way: leave the
   alias unattached and **degrade the answer class**. No guessing.

**Strict-default discipline (inherited):** a single confirmed candidate attaches; anything
uncertain stays unresolved. Strict-default beats broad-but-uncertain.

## Acceptance bar (all must hold)

- `api-dist.scip` no longer produces `95 divergent / 0 source-aligned` **at the semantic xref
  layer** (raw layer still does — see last bullet).
- The 95 published-interface references each either map to the provider source canonical identity
  **or** explicitly degrade **with a recorded reason and basis**.
- Ambiguous aliases are **not** silently attached (zero silent misses).
- The 1A answer-class cases run against the **dist** capture:
  - `Exact` where alias reconciliation is complete,
  - `Partial`/`Unavailable`/`Stale` where it is not.
- **Raw SCIP equality remains documented as insufficient** — 1B adds a layer above it, it does
  not "fix" the substrate. The raw `95 divergent` measurement is preserved as the before-state.

Pass condition is **not the reconciled count alone**. For FRAKTAG the useful result is likely
`reconciled: 95`, but the bar is that **every non-reconciled case is explicitly classified**.

## The probe (extends `rust/tools/xpart-probe`, NOT production runtime)

Add:

```text
ExportAliasIndex
  provider package metadata        (name, version, types/exports/main, dist root)
  source SCIP definitions          (engine.scip defs, by file + descriptor)
  published declaration symbols    (api-dist references into the package)
  declaration maps                 (dist/*.d.ts.map: sources [+ VLQ for fallback])
  alias records: published_symbol -> { canonical/source_symbol, basis, provenance }
```

Required probe output (the before/after the user specified):

```text
api-dist:
  raw references into engine: 95
  raw source-aligned: 0
  raw divergent: 95

after export alias reconciliation:
  reconciled: N          (by basis: decl-map M1, name-exact M3, …)
  ambiguous: N           (multiple candidates — explicitly unresolved)
  unreconciled: N        (no map / descriptor absent — explicitly unresolved)
  silent misses: 0       (invariant; asserted)
```

Then re-run the 1A answer-class cases keyed by the **reconciled canonical identity** over the
**dist** capture and show Exact where complete, Partial/Unavailable/Stale where not.

## Ratified decisions (2026-05-31)

**D1 — Canonical anchor: provider source symbol.** A published symbol aliases to the provider
**source** SCIP symbol; that source symbol resolves through INGEST-CORE-1's existing
canonicalization to the repo-graph `CanonicalKey`. **No neutral keys minted in 1B.**

**D2 — Bases: declaration-map + strict name-exact + unresolved.** Implement `DeclarationMapExact`
(Basis 1), `NameExactUnique` (Basis 3), and the `Ambiguous`/`Unresolved` outcomes (Basis 4).
**Skip Basis 2** (package export-surface): FRAKTAG has declaration maps, so it adds no measured
value and opens TS module-resolution complexity prematurely. Basis 2 is **deferred** for packages
without declaration maps / with complex `exports` (separate slice).

**D3 — Alias with provenance, never silent replacement.** A `dist → src` reconciliation is **not
Layer-0 extracted truth**; it is a bounded identity reconciliation and must carry basis +
evidence. The xref records an alias; it never silently rewrites the published key.

### AliasRecord (the unit the probe produces)

```text
published SCIP symbol
  -> AliasRecord {
       published_symbol,
       provider_source_symbol,
       canonical_key,
       basis,                 // one of the accepted bases below
       package_name,
       package_version,
       declaration_file,
       declaration_map,
       source_file,
       confidence_class       // mirrors basis: exact vs degraded
     }
```

### Accepted bases (closed set)

```text
DeclarationMapExact   // sources-array file correspondence + descriptor-exact (VLQ token bridge as fallback)
NameExactUnique       // strict predicate below
Ambiguous             // multiple candidates either side — NOT attached
Unresolved            // no map / descriptor absent — NOT attached
```

### `NameExactUnique` admissibility — ALL must hold, else `Ambiguous`/`Unresolved`

- same package name
- same package version
- same exported terminal name
- exactly one published candidate
- exactly one source candidate
- no overload ambiguity
- no re-export ambiguity
- no path ambiguity

If any condition fails, the alias is `Ambiguous` or `Unresolved` and is **not attached**. Strict
default: a single confirmed candidate attaches; uncertainty stays unattached.

## Verdict (EXECUTED 2026-05-31) — required wording; do NOT write "ST3 retired"

**XPART-PROVE-1B PASS.**
ST3 narrowed: declaration-map-backed named TypeScript package-boundary traversal is proven. ST3
remains open for anonymous structural members and packages without declaration maps / complex
exports.

```text
GO for declaration-map-backed named TypeScript package boundaries.

FRAKTAG api-dist:
- raw: 95 refs, 0 source-aligned, 95 divergent
- alias reconciliation: 78 DeclarationMapExact, 0 NameExactUnique, 0 Ambiguous, 17 Unresolved
- named public API: 78/78 reconciled
- anonymous inline type-literal members: 0/17 reconciled, all explicitly Unresolved
- silent misses: 0
- answer-class contract: PASS on reconciled named target
```

### Residuals (ST3 stays open for these)

**Residual 1 — anonymous structural members.** `typeLiteralNN` descriptors are
compilation-unit-relative and unstable **even in source-path captures** (`api-src` is measured
95/78/17 — the same 17). They must NOT be treated as stable cross-partition identities. They
remain `Unresolved` unless a later positional/VLQ declaration-map slice proves a safe bridge — or
they are accepted as explicitly non-cross-partition-addressable anonymous structure.

**Residual 2 — package export-surface without declaration maps.** Basis 2 remains deferred.
Packages without declaration maps or with complex `exports` are NOT proven by XPART-PROVE-1B.

### Answer-class precision rule (prevents the passing named case from overgeneralizing)

`Exact` applies ONLY to symbols whose alias reconciliation basis is complete
(`DeclarationMapExact` or `NameExactUnique`). If a query target — or any answer member — depends
on an `Unresolved`/`Ambiguous` alias, the result class MUST be `Partial` or `Unavailable`, never
`Exact`. The probe satisfies this **by construction**: an `Unresolved` published symbol never
enters the canonical xref as an engine-defined target, so it cannot yield an `Exact`
cross-partition answer; the passing `BaseNode#id` case is a fully-reconciled named target and does
not generalize to the anonymous-member residual.

Broader ST3 closure (Residual 1 or 2) is a **separate risk slice**, not 1B.

## Out of scope (hard guardrails)

NO production runtime / LiveGraph / query migration; NO trust-model implementation (1B only
establishes that a reconciled alias is Layer-2 with provenance — it does not build the trust
layer); NO >2 partitions; NO npm registry resolution or multi-version packages; NO bundler
(rollup/esbuild) declaration handling. Two partitions, one `ExportAliasIndex`, the strict basis
cascade, the before/after measurement.

## References
- `docs/slices/xpart-prove-1.md` (1A — answer-class contract; the divergence finding)
- `docs/audits/xpart-prove-1/findings.md` (the `95 divergent / 0 aligned` before-state, local)
- `docs/architecture/scip-migration-plan.md` (Stage B)
- Sentry `sourcemap` crate (declaration-map VLQ decode — reuse, do not hand-roll)
