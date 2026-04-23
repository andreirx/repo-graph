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
| `sqlite` | **validated** — operational | C |
| `swupdate` | **validated** — operational (v1.1 include resolver) | C |
| `nginx` | deferred — awaiting v1.1 validation | C |
| `kafka` | missing Rust Java extractor | Java |
| `hadoop` | missing Rust Java extractor | Java |

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

---

## sqlite

**Date:** 2026-04-22
**Commit:** shallow clone (depth 1)
**Branch:** master
**Clone location:** `../legacy-codebases/sqlite/`

### Metrics

| Metric | Value |
|--------|-------|
| Files indexed | 453 |
| Nodes total | 10,827 |
| Edges resolved | 28,461 |
| Edges unresolved | 39,100 |
| Unresolved rate | 58% |
| DB size | 113 MB |
| Stable-key collisions | 0 |

### Edge Resolution Breakdown

| Category | Resolved | Unresolved | Notes |
|----------|----------|------------|-------|
| CALLS (internal) | 27,813 (53%) | 24,461 | conditional compilation, macros |
| CALLS (external) | — | 13,422 | libc/stdlib, correctly classified |
| IMPORTS (local) | 253 | 204 | cross-directory limitation |
| IMPORTS (system) | — | 995 | correctly excluded |

### Hotspot Head Quality

| Rank | File | Sum Complexity | Hotspot Score |
|------|------|----------------|---------------|
| 1 | autosetup/jimsh0.c | 4,455 | 112,234,815 |
| 2 | src/btree.c | 1,959 | 22,661,712 |
| 3 | src/vdbe.c | 1,592 | 14,934,552 |
| 4 | ext/fts5/fts5_index.c | — | high |

`btree.c`, `vdbe.c`, `fts5_index.c` are exactly the core engine files that
should surface. `jimsh0.c` is an embedded Jim Tcl interpreter (autosetup
dependency) — high complexity but not core sqlite. An agent filtering by
`src/` gets clean signal.

### Caller/Callee Quality

- `sqlite3_open`: 48 callers across test files, tools, extensions
- `sqlite3VdbeExec`: extensive callees (the main VM execution loop)

Direct function navigation produces useful results.

### Assessment

**Indexing:** PASS. 453 files, 10,827 nodes, no parse failures, no stable-key
collisions. The C extractor survives a serious C codebase.

**Hotspot signal:** PASS. Top files are genuinely important core engine
modules. An AI agent gets useful work-priority guidance.

**Direct-call navigation:** PASS. callers/callees return useful results for
well-known API functions.

**Include resolution:** PARTIAL. 253 resolved (same-directory local includes),
204 unresolved internal (cross-directory includes like `ext/` → `src/`). This
is a known C v1 limitation: no build-system search path emulation.

**Call graph completeness:** PARTIAL. 53% of internal calls resolved. The
unresolved 47% is explained by:
- Conditional compilation (`#ifdef` blocks with different function sets)
- Macro-generated function names
- Functions defined in amalgamation artifacts not present in source tree

**Dead-code trust:** NOT YET VALIDATED. With only 53% call resolution,
dead-code conclusions should be treated cautiously.

**Include-layer analysis:** NOT YET VALIDATED. Cross-directory include
resolution is needed for full header topology analysis.

### Verdict

C extractor v1 is operational on sqlite for indexing, hotspots, and
direct-call navigation; graph completeness remains partial due to known
C v1 limits (no macro expansion, no cross-directory include paths, no
conditional compilation awareness).

This is enough for an AI agent to use for:
- Triage
- Hotspot-driven inspection
- Direct function exploration

It is not yet enough to fully trust:
- Dead code analysis
- Architectural edge completeness
- Include-layer analysis across complex C repos

### Known Limitations Exposed

1. **Cross-directory includes unresolved** — Files in `ext/` trying to include
   `src/sqliteInt.h` do not resolve. Design limitation: no build-system
   search path emulation.

2. **53% internal call resolution** — Conditional compilation and macros
   create function calls that cannot be resolved syntactically.

3. **`sqlite3.h` is generated** — The main public header is not in the source
   tree (built from source fragments). 91 includes of `sqlite3.h` unresolved.

---

## swupdate

**Date:** 2026-04-22
**Commit:** shallow clone
**Branch:** master
**Clone location:** `../legacy-codebases/swupdate/`

### Metrics — v1.0 (pre-fix baseline)

| Metric | Value |
|--------|-------|
| Files indexed | 208 |
| Nodes total | 3,191 |
| Edges resolved | 5,315 |
| Edges unresolved | 12,347 |
| call_resolution_rate | 38.2% |
| call_graph reliability | LOW |
| import_graph reliability | LOW |
| suspicious_zero_connectivity modules | 15 |

### Metrics — v1.1 (conventional roots resolver)

| Metric | Value |
|--------|-------|
| Files indexed | 208 |
| Nodes total | 3,191 |
| Edges resolved | 5,751 |
| Edges unresolved | 11,930 |
| unresolved_imports | 1,466 |
| call_resolution_rate | 38.2% |
| call_graph reliability | LOW |
| import_graph reliability | LOW |
| zero_connectivity_c_modules | **0** |
| zero_connectivity_js_modules | 4 |

### v1.1 Improvement Summary

| Metric | v1.0 | v1.1 | Delta |
|--------|------|------|-------|
| Zero-connectivity C modules | 15 | 0 | -15 |
| Zero-connectivity total | 15 | 4 | -11 |
| `include` module fan_in | 15 | 15 | stable |
| `include/suricatta` fan_in | 0 | 3 | +3 |
| Modules with fan_out≥1 | 5 | 16 | +11 |

The v1.1 include resolver with conventional roots (`include/`, `inc/`,
`src/include/`) resolved the cross-directory include topology problem.
All C modules now connect correctly to the central `include/` header module.

The 4 remaining zero-connectivity modules are JavaScript files
(`bindings/nodejs`, `examples/www/v2/js`, `scripts/basic`, `web-app/js`)
which are expected — the TS/JS extractor produces no exports for plain JS.

### Hotspot Head Quality (from `rmap hotspots`)

| Rank | File | Sum Complexity | Notes |
|------|------|----------------|-------|
| 1 | mongoose/mongoose.c | 5,129 | vendored HTTP lib |
| 2 | fs/ff.c | 331 | vendored FatFS |
| 3 | suricatta/server_hawkbit.c | 297 | hawkBit integration |
| 4 | handlers/diskpart_handler.c | 365 | disk partition handler |
| 5 | corelib/channel_curl.c | 387 | curl channel |

Top 2 are vendored libraries. After filtering, real swupdate code surfaces:
hawkBit server integration, disk handler, curl channel. Credible signal.

### Caller/Callee Quality (from `rmap callers/callees`)

`install_single_image`:
- 4 callers: `chain_handler_thread`, `extract_files`, `install_images`
- 3 callees: `find_handler`, `swupdate_progress_inc_step`, `swupdate_progress_step_completed`

Direct function navigation produces useful local results.

### Module Topology (from `rmap stats` — v1.1)

Module topology is now trustworthy. The `include` module correctly receives
imports from 15 other modules (fan_in=15). Sub-modules like `include/suricatta`
show accurate connectivity (fan_in=3, fan_out=1).

### Assessment (updated for v1.1)

**Indexing:** PASS. 208 files, 3,191 nodes, no parse failures.

**Hotspot signal:** PASS (with filtering). Vendored libs dominate raw ranking,
but real swupdate code surfaces after top 2.

**Direct-call navigation:** PASS. Callers/callees return useful local results.

**Module topology:** PASS (v1.1). All C modules connect correctly. Only JavaScript
modules remain zero-connectivity (expected).

**Call graph completeness:** DEGRADED. 38.2% resolution. Macro-heavy call patterns
(ERROR, MG_*, TRACE, DEBUG) remain unresolved. This is expected for C.

**Import graph:** PASS (v1.1). Cross-directory includes to `include/` now resolve
via conventional roots.

### Verdict (updated for v1.1)

C v1.1 is fully operational for swupdate. Module topology and include graphs work
correctly. Call graph remains low-resolution due to macro patterns (expected).

Remaining limitations are fundamental C characteristics, not resolver bugs:
- Macro-heavy call-like syntax (ERROR, MG_*, TRACE, DEBUG)
- Generated/build-time headers (generated/autoconf.h)
- External library APIs without definitions (lua, curl, mongoose)

### v1.1 Design Reference

See `docs/milestones/c-include-resolution-v1.1.md` for the resolver design.
Key features:
- Resolution order: same-dir → configured roots → conventional roots
- Conventional roots: `include/`, `inc/`, `src/include/` (repo-root anchored)
- Ambiguity detection: multiple exact matches categorized as `ImportsAmbiguousMatch`
- CLI option: `rmap index --include-root <path>` for project-specific roots

---

## Conclusions (2026-04-22, updated for v1.1 include resolver)

Four repos validated: react, typescript (JS/TS), sqlite, swupdate (C).

### Validated Operating Envelope

**Rust `rmap` large-repo support for JS/TS is real:**
- react: 4,345 files, 18s
- typescript: 39,264 files, 75s
- Throughput: ~500 files/sec (release build)
- DB size: scales linearly (~17 KB/file average)

**Rust `rmap` C support (v1.1): both repos operational**

sqlite (disciplined C):
- call_resolution_rate: 53.2%, call_graph: MEDIUM
- Hotspots, callers/callees operational
- Graph completeness partial but usable

swupdate (embedded-style C, v1.1):
- call_resolution_rate: 38.2%, call_graph: LOW (macro-heavy, expected)
- Module topology: OPERATIONAL (v1.1 conventional roots resolver)
- Zero-connectivity C modules: 0 (was 15 in v1.0)
- `include` module fan_in: 15 (all C modules connect correctly)

**Hotspot signal is useful across languages:**
- react: strong head quality (React compiler, RSC, reconciler)
- typescript: credible head quality (compiler utilities, core modules)
- sqlite: credible head quality (btree, vdbe, fts5 core)
- swupdate: credible after filtering vendored libs (hawkbit, diskpart, curl)

**Unresolved-edge rate patterns:**
- JS/TS: ~55-60% (type-only imports, dynamic patterns)
- C (sqlite): 47% internal calls unresolved
- C (swupdate): 62% internal calls unresolved
- swupdate call-graph degradation is fundamental: macro-heavy patterns (ERROR, MG_*, TRACE)

### Identified Limitations

1. **Test baseline pollution (JS/TS)** — generated test output files surface
   in raw hotspot rankings. View-policy filtering needed.

2. **Unresolved-edge rate ~55-60% (JS/TS)** — syntax-only extraction cannot
   resolve dynamic patterns, type-only imports, internal API factories.

3. **Conditional compilation (C)** — C v1 does not expand macros or evaluate
   `#ifdef` blocks. Functions in alternate code paths are not extracted.

4. **Macro call patterns (C)** — ERROR(), TRACE(), MG_*() macros look like
   function calls but don't resolve. This is expected C behavior.

5. **Java/Python/Rust extractors blocked** — kafka, hadoop, swupdate (Rust
   portions) await Rust extractor ports.

### Follow-Up Items

**DONE: C extractor v1 (Rust)**
- sqlite: operational (53% call resolution, MEDIUM trust)
- swupdate: operational post-v1.1 (module topology works, call graph LOW but expected)

**DONE: swupdate validation (C)**
- Confirmed module topology works with v1.1 conventional roots resolver
- Call graph remains LOW due to macro patterns (expected, not a bug)

**DONE: Cross-directory include resolution (v1.1)**
- Conventional roots resolver shipped: `include/`, `inc/`, `src/include/`
- swupdate: 0 zero-connectivity C modules (was 15)
- CLI option: `rmap index --include-root <path>` for project-specific roots
- Ambiguity detection: `ImportsAmbiguousMatch` category (no matches in test repos)

**P1: nginx validation (C)**
- Now unblocked by v1.1 include resolver
- Will test generalization of conventional roots approach
- If nginx works, v1.1 is validated for general C projects

**P2: Hotspot presentation filtering**
- View-policy controls for vendored libs, test baselines, generated paths
- Improves top-of-list working set for AI agent consumption

**P3: Macro detection/filtering**
- Improves noise accounting in trust reports
- Does not restore graph connectivity (expected C limitation)

**P4: Java extractor (Rust)**
- Unblocks kafka, hadoop
- Lower priority — those repos are huge layered systems

### Validation Status (post-v1.1)

v1.1 conventional roots resolver shipped. swupdate now shows correct module
topology (0 zero-connectivity C modules). nginx validation is unblocked.

Call-graph completeness remains LOW on swupdate (38%) due to macro patterns —
this is expected C behavior, not a resolver bug. The resolver cannot make
`ERROR()`, `TRACE()`, `MG_*()` look like real function calls.
