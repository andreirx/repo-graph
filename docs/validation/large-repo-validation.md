# Large-Repo Validation Notes

Validation runs against real-world open-source repositories.
Purpose: stress-test indexing, measurement quality, and CLI ergonomics at scale.

## Validation Sequence (planned)

1. `react` — JS/TS-heavy, modern ecosystem
2. `sqlite` — compact systems code (C)
3. `nginx` — systems code (C)
4. `kafka` — Java, architecture-heavy
5. `swupdate` — embedded/safety-critical (C)
6. `hadoop` — very large, multi-package (Java)

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
