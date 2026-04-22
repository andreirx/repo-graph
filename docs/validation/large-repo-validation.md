# Large-Repo Validation Notes

Validation runs against real-world open-source repositories.
Purpose: stress-test indexing, measurement quality, and CLI ergonomics at scale.

## Scope

**Track A: Rust `rmap` validation (JS/TS repos only)**

The Rust CLI currently has only the TS/JS extractor. Large-repo validation
for `rmap` applies only to JS/TS codebases.

Validation targets:
1. `react` — JSX-heavy, modern ecosystem (done)
2. `typescript` — large TS-first codebase, compiler architecture
3. `next.js` — framework, mixed patterns
4. `webpack` — bundler, plugin architecture
5. `angular` — framework, heavy decorators

**Track B: Extractor gaps (blocked repos)**

These repos cannot validate `rmap` until Rust extractors are ported.
They are roadmap evidence, not validation targets.

| Repo | Blocked by | Language |
|------|------------|----------|
| `sqlite` | missing Rust C extractor | C |
| `nginx` | missing Rust C extractor | C |
| `kafka` | missing Rust Java extractor | Java |
| `hadoop` | missing Rust Java extractor | Java |
| `swupdate` | missing Rust C extractor | C |

Do NOT use `rgr` (TS CLI) as a proxy for `rmap` validation. That answers
"is the TS extractor mature?" not "is the Rust product ready?"

---

## react

**Date:** 2026-04-22
**Commit:** `94643c3b8516928e4cc7fad99912272670a0a990`
**Branch:** `main`
**Clone:** full (with history for churn analysis)

### Metrics

| Metric | Value |
|--------|-------|
| Files indexed | 4,345 |
| Nodes total | 34,789 |
| Edges resolved | 23,504 |
| Edges unresolved | 29,836 |
| Unresolved rate | 55.9% |
| DB size | 137 MB |
| Indexing time | 17.8s (release build) |
| Complexity measurements | 11,072 |
| Files with hotspots | 394 |

### Hotspot Head Quality (top 10)

| Rank | File | Hotspot Score |
|------|------|---------------|
| 1 | compiler/.../HIR/BuildHIR.ts | 826,686 |
| 2 | compiler/.../ReactiveScopes/CodegenReactiveFunction.ts | 282,343 |
| 3 | packages/react-server/src/ReactFlightServer.js | 191,646 |
| 4 | compiler/.../ReactiveScopes/BuildReactiveFunction.ts | 129,792 |
| 5 | packages/react-reconciler/src/ReactFiberWorkLoop.js | 102,951 |
| 6 | compiler/.../Inference/InferMutationAliasingEffects.ts | 91,176 |
| 7 | packages/react-server/src/ReactFizzServer.js | 65,758 |
| 8 | compiler/packages/snap/src/minimize.ts | 63,560 |
| 9 | compiler/.../Entrypoint/Program.ts | 59,160 |
| 10 | packages/react-reconciler/src/ReactFiberCommitWork.js | 52,364 |

### Assessment

**Indexing scale:** Good. 4,345 files indexed in 18s (release).

**Runtime:** Good. No memory issues observed.

**DB size:** Reasonable. 137 MB for a repo this size.

**Hotspot head quality:** Strong. The top files are genuinely important:
- React compiler (actively developed, high complexity)
- React Server Components infrastructure
- Fiber reconciler core

An AI agent would get useful work-priority guidance from this ranking.

**Unresolved edge rate (55.9%):** Real limitation.

This is high but not unexpected for React:
- Heavy JSX usage creates unresolved targets
- Dynamic patterns, callbacks, higher-order components
- Syntax-only extractor (no type information)

The high unresolved rate does NOT invalidate hotspots — hotspots depend on
churn × complexity, not on call graph completeness. It DOES limit trust in
graph-heavy surfaces (callers, callees, path) on JSX-heavy codebases.

**Interpretation:**
- React is a valid measurement validation target (churn, hotspots work)
- React is only a partial graph-validation target (callers/callees less reliable)

### Defects Exposed

**Stable-key collision (fixed):** Initial indexing failed with 1,073 collisions.
Multiple same-name symbols in test files (e.g., multiple `function Component()`)
violated the `(name, subtype)` uniqueness assumption. Fixed with `:dupN`
disambiguation in the Rust TS/JS extractor.

### Deferred Observations

- Coverage import not tested (would require running React's test suite)
- Risk analysis not tested (depends on coverage)
- Module graph not explored
- No framework detector for React patterns (would require JSX-aware extraction)

---

## typescript

**Date:** 2026-04-22
**Commit:** `55423abe4d029017f19b6e4c32097591994836b4`
**Branch:** `main`
**Clone:** full (with history)

### Metrics

| Metric | Value |
|--------|-------|
| Files indexed | 39,264 |
| Nodes total | 326,469 |
| Edges resolved | 62,670 |
| Edges unresolved | 95,330 |
| Unresolved rate | 60.3% |
| DB size | 674 MB |
| Indexing time | 75s (release build) |
| Complexity measurements | 58,924 |
| Files with hotspots | 10,326 |

### Hotspot Head Quality

Raw hotspot ranking is polluted by test baseline files (`tests/baselines/reference/*.js`)
which are generated output with high line counts but low complexity. Filtering by
path prefix gives clean signal.

**Top hotspots (excluding test baselines):**

| Rank | File | Hotspot Score |
|------|------|---------------|
| 1 | src/compiler/utilities.ts | 339,624 |
| 2 | src/harness/fourslashImpl.ts | 81,528 |
| 3 | src/compiler/commandLineParser.ts | ~50K |
| 4 | src/compiler/moduleNameResolver.ts | high |
| 5 | src/compiler/core.ts | high |
| 6 | src/server/editorServices.ts | high |
| 7 | src/compiler/program.ts | high |
| 8 | src/compiler/binder.ts | high |

### Assessment

**Indexing scale:** Strong. 39K files in 75s (~530 files/sec).

**Runtime:** No memory issues at 674 MB DB.

**Hotspot signal:** Credible for core compiler code. Test baselines pollute raw
rankings. An agent filtering `src/compiler/` or `src/services/` gets clean signal.

**Unresolved rate (60.3%):** Slightly higher than React. TypeScript uses internal
API patterns (factory functions, type-only imports, namespace merging) that don't
resolve syntactically.

**Interpretation:**
- TypeScript confirms hotspot signal generalizes to large TS-first codebases
- Test baseline pollution is a data quality issue, not extractor failure
- Unresolved rate ~60% appears stable across large TS repos
