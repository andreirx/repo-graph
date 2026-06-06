# IMPORTS-TSCONFIG-EXTENDS-1: resolve tsconfig `extends` for alias metadata

Slice ID: IMPORTS-TSCONFIG-EXTENDS-1
Status: **DEFERRED / NOT STARTED (2026-06-06). Premise refuted by measurement; no validation case in the repo
set.** Merge `compilerOptions.baseUrl` + `paths` across `extends` chains to feed the existing alias resolver.

## Why deferred (EXECUTED grounding 2026-06-06)
```text
The motivating case -- "amodx backend `@/` blocked by an extends/app-config" -- DOES NOT EXIST:
- backend imports `@/` ZERO times (grep backend/src -> 0); it has no aliases at all.
- NO amodx tsconfig uses `extends` anywhere (admin/backend/renderer/infra/packages/tools -- none).
- admin + renderer define `paths` DIRECTLY in their own tsconfig.json; renderer's JSONC parsed via json5
  (its `./src/*` config is captured in the warm cache) -> already resolved by IMPORTS-TSCONFIG-PATHS-1.
=> amodx's `@/` alias story is COMPLETE without extends. Building EXTENDS-1 now would be building on a refuted
   premise (no live validation case). The residual amodx blocker is NOT aliases -- it is transitive externals
   (-> IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1, measured decisive).
```

## When to revisit
```text
Pick this up only when a target repo in the validation set actually uses `extends` to inherit `paths`/`baseUrl`
(and that inheritance causes real `@/` (or other alias) imports to block). Then ground that repo's extends
forms (relative / package / omitted `.json`), match TS merge semantics (child overrides; `paths` is REPLACED
not deep-merged; `baseUrl`/`paths` are relative to the config that DEFINES them), add cycle detection on the
extends chain, and feed the merged effective {baseUrl, paths} into the existing `resolve_tsconfig_alias`. Until
then it is speculative.
```

## References
- `docs/slices/imports-tsconfig-paths-1.md` (the alias resolver this would feed; the honest "extends deferred" caveat)
- `scripts/measure-amodx-import-residual.py` (the measurement that refuted the premise + redirected to external-evidence)
