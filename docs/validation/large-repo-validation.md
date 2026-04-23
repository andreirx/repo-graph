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
| `swupdate` | **validated** — operational (v1.2, conventional roots) | C |
| `nginx` | **validated** — operational (v1.2, requires explicit `--include-root`) | C |
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

### Hotspot Filtering Validation (2026-04-23)

Validates `--exclude-tests` and `--exclude-vendored` presentation filtering.

| Filter | Raw Count | Filtered Count | Excluded |
|--------|-----------|----------------|----------|
| (none) | 125 | 125 | — |
| `--exclude-tests` | 125 | 115 | 10 test files |
| `--exclude-vendored` | 125 | 125 | 0 (no standard vendored segments) |
| both | 125 | 115 | 10 |

**Key observation:** `mongoose/` is NOT excluded by `--exclude-vendored`.
This is correct — `mongoose` is not a standard vendored segment. swupdate
uses it as an embedded library with project-specific naming. Config-based
exclusion rules (deferred) would be needed to exclude it.

mongoose files remain in filtered output:
- `mongoose/mongoose.c`
- `mongoose/mongoose.h`
- `mongoose/mongoose_interface.c`
- `mongoose/mongoose_multipart.c`

Test files correctly excluded (e.g., `test/test_mongoose_upload.c`).

---

## nginx

**Date:** 2026-04-23
**Commit:** shallow clone (depth 1)
**Branch:** master
**Clone location:** `../legacy-codebases/nginx/`

### Metrics (from `rmap trust`)

| Metric | Value |
|--------|-------|
| Files indexed | 396 |
| Nodes total | 4,388 |
| Edges resolved | 9,533 |
| Edges unresolved | 13,012 |
| call_resolution_rate | 42.5% |
| call_graph reliability | LOW |
| import_graph reliability | LOW |
| suspicious_zero_connectivity modules | **13** |
| unresolved_imports | 1,079 |

### Validation Results

| Test | Result | Notes |
|------|--------|-------|
| Index completes | PASS | 396 files, 4388 nodes, no failures |
| Language coverage | PASS | C files indexed correctly |
| Topology | **FAIL** | 0 modules with any connectivity |
| Trust honest | PASS | Correctly reports LOW, flags `alias_resolution_suspicion` |
| Hotspots credible | PARTIAL | Core files surface, but shallow clone limits churn accuracy |
| Direct navigation | PASS | callers/callees produce real local structure |

### Root Cause: Angle-Bracket Local Includes

nginx uses **angle-bracket includes for local headers**:

```c
#include <ngx_config.h>   // local header, not system
#include <ngx_core.h>     // local header, not system
```

This is build-system style. nginx expects `-I src/core -I src/event` from
its configure script. The C extractor v1.1 only resolves **quoted includes**
(`"foo.h"`), explicitly skipping angle-bracket includes as system headers.

This is NOT a failure of the v1.1 conventional-roots approach. It is a
different include style entirely:

- **swupdate**: quoted includes (`"util.h"`) → v1.1 works
- **nginx**: angle-bracket includes (`<ngx_core.h>`) → v1.1 has no effect

### Hotspot Quality (limited evidence)

| Rank | File | Sum Complexity | Hotspot Score |
|------|------|----------------|---------------|
| 1 | src/http/ngx_http_upstream.c | 1,113 | 8,130,465 |
| 2 | src/http/ngx_http_core_module.c | 665 | 3,616,935 |
| 3 | src/http/v2/ngx_http_v2.c | 696 | 3,528,720 |
| 4 | src/http/modules/ngx_http_proxy_module.c | 621 | 3,344,706 |
| 5 | src/http/ngx_http_request.c | 632 | 2,629,120 |

These are core nginx files. The ranking is structurally suggestive.

**Caveat:** Shallow clone (depth 1) means `commit_count=1` for all files.
The hotspot scores are based on line counts, not real historical churn.
This is structural validation only, not hotspot-history validation.

### Caller/Callee Quality

`ngx_http_process_request` callers:
- `ngx_http_process_request_headers` (request.c)
- `ngx_http_process_request_line` (request.c)
- `ngx_http_v2_run_request` (v2/ngx_http_v2.c)
- `ngx_http_v3_process_request` (v3/ngx_http_v3_request.c)

`ngx_http_process_request` callees:
- `ngx_http_finalize_request` (4 calls)
- `ngx_http_handler` (core_module.c)

Direct function navigation produces useful local results despite topology collapse.

### Comparison to Prior C Repos

| Metric | sqlite | swupdate v1.1 | nginx |
|--------|--------|---------------|-------|
| call_resolution_rate | 53.2% | 38.2% | 42.5% |
| suspicious_zero_connectivity | 0 | 0 | **13** |
| Module topology | Works | Works | **Collapsed** |
| Hotspots | Credible | Credible | Structural only |
| callers/callees | Useful | Useful | Useful |

### Assessment

**Indexing:** PASS. 396 files, 4,388 nodes, no parse failures.

**Hotspot signal:** PARTIAL. Core nginx files surface, but shallow clone
means scores are line-count-based, not history-validated.

**Direct-call navigation:** PASS. Callers/callees return useful local results.

**Module topology:** FAIL. All 13 C modules have zero connectivity. The
include graph is completely collapsed.

**Trust honesty:** PASS. Trust report correctly identifies the problem:
`alias_resolution_suspicion` triggered with 13 suspicious modules.

### Verdict

C v1.1 does NOT generalize to nginx. The failure mode is specific:
**angle-bracket local includes are not resolved.**

The extractor remains useful for:
- File/function triage (hotspots surface core files)
- Direct navigation (callers/callees work)

The extractor is NOT useful for:
- Module topology (completely collapsed)
- Change impact (no import graph)
- Architectural analysis (no cross-module edges)

### v1.2 Results (angle-bracket resolution shipped)

v1.2 shipped: angle-bracket includes now attempt resolution via configured
and conventional roots. Same-dir remains quote-only.

**Without explicit roots:** no improvement (nginx has no conventional root dirs)

**With explicit roots (`--include-root src/core --include-root src/event ...`):**

| Metric | v1.1 | v1.2 (with roots) |
|--------|------|-------------------|
| Resolved edges | 9,533 | **10,344** |
| Unresolved imports | 1,079 | **294** |
| Zero-connectivity modules | 13 | **0** |
| `src/core` fan_in | 0 | **13** |
| `alias_resolution_suspicion` | triggered | **false** |

v1.2 works correctly. nginx requires explicit `--include-root` flags because
it doesn't follow conventional root patterns (`include/`, `inc/`). Each nginx
module directory (`src/core/`, `src/event/`, `src/http/`) must be declared.

### Practical Guidance for nginx-style repos

```bash
rmap index ./nginx ./nginx.db \
  --include-root src/core \
  --include-root src/event \
  --include-root src/http \
  --include-root src/os/unix \
  --include-root src/stream \
  --include-root src/mail
```

This is the expected UX for build-system-heavy C repos. The CLI provides the
same information that would come from `compile_commands.json` or configure
output.

See `docs/milestones/c-include-resolution-v1.2.md` for design.

### Hotspot Filtering Validation (2026-04-23)

Validates `--exclude-tests` and `--exclude-vendored` presentation filtering.

| Filter | Raw Count | Filtered Count | Excluded |
|--------|-----------|----------------|----------|
| (none) | 276 | 276 | — |
| `--exclude-tests` | 276 | 276 | 0 |
| `--exclude-vendored` | 276 | 276 | 0 |
| both | 276 | 276 | 0 |

nginx has no files matching either filter:
- **No standard test paths:** nginx tests live outside the main source tree
  or use non-standard naming (no `.test.`, `.spec.`, `tests/`, `__tests__/`)
- **No standard vendored segments:** no `vendor/`, `third_party/`, `deps/`

This is the expected neutral baseline. The `filtering` field is correctly
present when flags are active, with honest zero counts.

---

## Conclusions (2026-04-23, updated for v1.2 angle-bracket resolution)

Five repos validated: react, typescript (JS/TS), sqlite, swupdate, nginx (C).

### Validated Operating Envelope

**Rust `rmap` large-repo support for JS/TS is real:**
- react: 4,345 files, 18s
- typescript: 39,264 files, 75s
- Throughput: ~500 files/sec (release build)
- DB size: scales linearly (~17 KB/file average)

**Rust `rmap` C support (v1.1): two include styles, two outcomes**

Quoted-include layouts (swupdate, sqlite):
- v1.1 conventional roots resolver works
- Module topology operational
- Call graph partial but usable (macro patterns expected)

Angle-bracket-include layouts (nginx):
- v1.1 has NO effect (angle-brackets skipped as system)
- Module topology collapsed (13 zero-connectivity modules)
- Direct navigation (callers/callees) still works
- Hotspots still surface core files

| Repo | Include Style | v1.2 Topology | Config Required |
|------|---------------|---------------|-----------------|
| sqlite | quoted | Works | none |
| swupdate | quoted | Works | none (uses conventional `include/`) |
| nginx | angle-bracket | **Works** | `--include-root` per module dir |

**v1.2 is validated for both quoted and angle-bracket C layouts.**

nginx requires explicit `--include-root` flags because it doesn't use
conventional root directories. This is expected UX for build-system-heavy
C projects.

**Hotspot signal is useful across languages:**
- react: strong head quality (React compiler, RSC, reconciler)
- typescript: credible head quality (compiler utilities, core modules)
- sqlite: credible head quality (btree, vdbe, fts5 core)
- swupdate: credible after filtering vendored libs (hawkbit, diskpart, curl)
- nginx: structural only (shallow clone limits churn accuracy)

**Unresolved-edge rate patterns:**
- JS/TS: ~55-60% (type-only imports, dynamic patterns)
- C (sqlite): 47% internal calls unresolved
- C (swupdate): 62% internal calls unresolved
- C (nginx): 58% internal calls unresolved
- Call-graph degradation is fundamental C behavior: macro patterns (ERROR, TRACE, MG_*, ngx_*)

### Identified Limitations

1. **Test baseline pollution (JS/TS)** — generated test output files surface
   in raw hotspot rankings. View-policy filtering needed.

2. **Unresolved-edge rate ~55-60% (JS/TS)** — syntax-only extraction cannot
   resolve dynamic patterns, type-only imports, internal API factories.

3. **Conditional compilation (C)** — C v1 does not expand macros or evaluate
   `#ifdef` blocks. Functions in alternate code paths are not extracted.

4. **Macro call patterns (C)** — ERROR(), TRACE(), MG_*(), ngx_*() macros look
   like function calls but don't resolve. This is expected C behavior.

5. **Angle-bracket local includes (C)** — nginx uses `<ngx_core.h>` for local
   headers. v1.1 skips these as system includes. **Blocks nginx topology.**

6. **Java/Python/Rust extractors blocked** — kafka, hadoop, swupdate (Rust
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
- Ambiguity detection: `ImportsAmbiguousMatch` category

**DONE: nginx validation (C)**
- Initial: topology collapsed (13 zero-connectivity modules)
- Root cause: angle-bracket local includes (`<ngx_core.h>`)
- v1.2 fix: angle-bracket includes now resolve via configured/conventional roots
- With explicit `--include-root` flags: topology restored (0 zero-connectivity)
- `src/core` fan_in=13, all modules connected

**DONE: Angle-bracket local include resolution (v1.2)**
- Separate delimiter syntax from semantic origin
- Apply same resolution policy to both quoted and angle-bracket forms
- Same-dir remains quote-only
- True system headers stay unresolved (no local match)
- Ambiguity detection unchanged
- nginx requires explicit `--include-root` (no conventional root dirs)
- swupdate unchanged (uses quoted includes with conventional roots)

**P1: Hotspot presentation filtering**
- View-policy controls for vendored libs, test baselines, generated paths
- Improves top-of-list working set for AI agent consumption

**P3: Macro detection/filtering**
- Improves noise accounting in trust reports
- Does not restore graph connectivity (expected C limitation)

**P4: Java extractor (Rust)**
- Unblocks kafka, hadoop
- Lower priority — those repos are huge layered systems

### Validation Status (post-v1.2)

**v1.2 is validated for both quoted and angle-bracket C layouts.**

- swupdate: works with conventional roots (no config needed)
- nginx: works with explicit `--include-root` flags

The next slice is P1: hotspot presentation filtering.

Call-graph completeness remains LOW across all C repos (38-53%) due to macro
patterns — this is expected C behavior, not a resolver bug.

---

## linux (Kbuild Module Discovery — Layer 3A)

**Date:** 2026-04-23
**Commit:** shallow clone
**Branch:** master
**Clone location:** `../legacy-codebases/linux/`

### Purpose

Validates Layer 3A build-system-derived module discovery. Linux kernel is
the canonical Kbuild repo — modules are defined by `obj-y`/`obj-m`
assignments in Makefile/Kbuild files, not by package manager manifests.

This is NOT a graph extraction validation (no Rust C extractor yet). It
validates the Kbuild detector + scanner adapter + indexer integration.

### Metrics (from `discoverBuildSystemModules()`)

| Metric | Value |
|--------|-------|
| Files scanned | 2,977 |
| Files with modules | 159 |
| Modules discovered | 703 |
| Diagnostics | 20,250 |

### Diagnostic Breakdown

| Kind | Count | Meaning |
|------|-------|---------|
| `skipped_config_gated` | 17,708 | obj-$(CONFIG_...) assignments (D1: out of scope) |
| `skipped_conditional` | 2,382 | Conditional directive lines (ifeq, ifdef, endif, else) |
| `skipped_inside_conditional` | 83 | Assignments inside conditional blocks |
| `skipped_variable` | 77 | Unexpanded variable references |

The high `skipped_config_gated` count reflects Linux kernel reality: most
module inclusions are config-gated. The 703 discovered modules are the
unconditional baseline that always exists regardless of configuration.

### Sample Modules (top-level)

| Module Path | Display Name |
|-------------|--------------|
| `init` | init |
| `kernel` | kernel |
| `mm` | mm |
| `fs` | fs |
| `drivers` | drivers |
| `net` | net |
| `sound` | sound |
| `security` | security |
| `crypto` | crypto |
| `arch/alpha/kernel` | kernel |
| `arch/x86/kernel` | kernel |

### Assessment

**File discovery:** PASS. 2,977 Makefile/Kbuild files found. Deep tree
traversal works (kernel has directories 10+ levels deep).

**Module extraction:** PASS. 703 unconditional modules discovered. Covers
top-level subsystems (kernel/, fs/, net/, drivers/) and core architecture
modules. Only obj-y and obj-m assignments are extracted per D1 scope.

**Config-gated handling:** PASS. 17,708 obj-$(CONFIG_...) assignments
correctly skipped with `skipped_config_gated` diagnostic. These depend on
Kconfig evaluation which we do not perform. HIGH confidence is only
assigned to unconditional modules.

**Conditional handling:** PASS. 83 assignments inside conditional blocks
(ifeq/ifdef) were correctly skipped with `skipped_inside_conditional`.

**Variable handling:** PASS. 77 unexpanded variable references recorded
as diagnostics, not emitted as modules.

**Directory skipping:** PASS. Documentation/, scripts/, tools/ directories
skipped as configured. These contain Makefiles for build utilities, not
kernel module definitions.

**Scan time:** ~1 second. Acceptable for a repo this size.

### Neutral Validation (non-Kbuild repo)

| Repo | Files Scanned | Modules | Diagnostics |
|------|---------------|---------|-------------|
| repo-graph | 1 | 0 | 0 |

The test fixture Makefile (`test/fixtures/cli-boundary/Makefile`) was
scanned but contains build targets (`build:`, `deploy:`), not obj-y/obj-m
assignments. Zero modules discovered. No false positives.

### Verdict

Layer 3A Kbuild detection is validated on the canonical target repo.

**What works:**
- Deep tree traversal (2,977 files across deep directory structure)
- Module boundary detection (obj-y, obj-m, obj-$(CONFIG_...))
- Line continuation handling
- Conditional block tracking (ifdef/endif nesting)
- Diagnostic recording for skipped content

**What is explicitly out of scope (D1 locked):**
- Kconfig variable evaluation
- Conditional compilation analysis
- Variable expansion beyond trivial forms

**Integration status:**
- Detector: pure function, validated
- Scanner adapter: filesystem traversal, validated
- Indexer: wired, combines Layer 1 + Layer 3A roots
- Orchestrator: kind precedence (declared > operational > inferred)
- Persistence: candidates + evidence stored, diagnostics ephemeral

### Known Limitations

1. **No conditional evaluation** — Modules inside `ifdef CONFIG_FOO` blocks
   are not discovered. This is by design (D1 minimal scope).

2. **No variable expansion** — `obj-$(SUBDIR)` where SUBDIR is a Makefile
   variable is skipped, not resolved.

3. **Diagnostics not persisted** — A1 stores modules and evidence, but
   diagnostics are ephemeral. Persistence deferred until schema home exists.

4. **No duplicate detection across files** — If multiple Makefiles define
   the same module path, both evidence items are stored. The orchestrator
   deduplicates by root path, but evidence inflation is possible.
