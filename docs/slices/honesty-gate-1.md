# HONESTY-GATE-1 — deps list never calls a live dependency unused

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #1 (human-ratified). CODE
slice. Maturity: MATURE (deps list is a top-level surface; its "unused" claim induces
destructive edits).

## 1. Problem (VERIFIED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

`deps list` emits the one output class that makes an agent BREAK RUNNING SOFTWARE:
- django `unused: asgiref` — 31 verified import sites (`django/tasks/base.py:7`,
  `core/paginator.py:6`, …), declared at `pyproject.toml:10`; NO caveat printed.
- zvec-grep `unused: @huggingface/transformers` — loaded by DYNAMIC import at
  `src/engine/models/backends/transformers-js.ts:99` (`await import("…")`); `@eslint/js`
  imported by a ROOT CONFIG file (`eslint.config.js:1`) outside scan scope. ~8 of 14
  "unused" false.
- storybook `used 13 · declared-unused 111` against a root manifest holding 0 deps + 13
  devDeps (+21 resolutions) — arithmetically impossible.
- hadoop/langchain4j: NO Maven parser exists (119 pom.xml; `72016 of 72016 external
  references not attributed`), disclosed only as "resolution downgraded on this index" —
  reads transient, is architectural. The Java path's `(imports not resolved on this
  index)` caveat fires there and is SILENT on the npm/py paths that are actually wrong.

## 2. Contract — the invariant, applied to deps

**INVARIANT (HONESTY-GATE, all slices in this family): no row is emitted whose evidence
the printed caveat excludes.** For deps list:
1. **"unused" is a claim with a stated basis.** A dependency may be labeled unused ONLY
   when the index's import evidence for that ecosystem is COMPLETE enough to support
   absence: static imports resolved for that language AND dynamic-import literals
   (`await import("x")`, `require("x")`, `importlib.import_module("x")` string-literal
   forms) extracted AND root-level config files (`*.config.{js,ts,mjs,cjs}`,
   `eslint.config.*`, `vite/vitest/jest/tailwind/postcss` configs, `setup.py`/`conftest.py`)
   in scope. Where any of these is not evidenced for the ecosystem, the column is NOT
   "unused" but **"no static import found"** with the caveat naming what was not
   checked. The word "unused" never renders without the basis being met.
2. **Arithmetic honesty.** Declared/used/unused/undeclared counts must reconcile against
   the manifest actually parsed; a count exceeding the manifest's declarations is a
   defect — surface the reconciliation (declared N from <manifest path>) and never emit
   impossible figures. `resolutions`/`overrides`/workspace-hoisted entries are named
   separately, never folded into "declared".
3. **Missing parsers are named as capability limits.** Maven (`pom.xml`) has no parser:
   render "Maven manifests are not parsed on this build (119 pom.xml present) — Java
   attribution unavailable" (the trust ceiling-vs-gap framing), never "downgraded on this
   index". Same shape for any other manifest kind the build cannot read.
4. **Caveat parity across ecosystems.** The Java `(imports not resolved on this index)`
   caveat mechanism applies to npm/python identically — one caveat model, one render
   path, no per-ecosystem silence.
5. Node builtins (`node:test`, `node:async_hooks`) bucket as builtins, never undeclared
   (the carried NODE-BUILTIN-AS-UNDECLARED minor — same surface, fold it in).
6. JSON additive (basis field, caveat field, manifest source); exit codes unchanged.

## 3. Stop conditions

Frozen: manifest parsers' output shapes (additive fields only), storage schema (additive
if a dynamic-import fact needs storing — additive migration RATIFIED), exit codes.
Building a Maven parser is OUT OF SCOPE (name the absence; a parser is its own slice).
STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's
real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing tests FIRST: (a) dynamic-import-only dependency → never "unused";
  (b) config-file-only dependency → never "unused"; (c) declared count vs manifest
  reconciliation (storybook shape) → impossible figures unrepresentable; (d) Maven-only
  repo → capability-limit sentence; (e) node: builtins bucketed.
- Live proof (isolated state root, registry sha unchanged): django (asgiref no longer
  unused, or labeled "no static import found" with the basis), zvec-grep
  (@huggingface/transformers, @eslint/js), storybook (counts reconcile to 13), hadoop
  (Maven sentence). Before/after captured verbatim.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No "unused" claim renders without its basis met; counts reconcile to the parsed manifest;
missing parsers are named as capability limits; caveats have ecosystem parity; the four
verified false claims are gone from the live proofs; gates green.

CORPUS PATHS: django at ../legacy-codebases/django; zvec-grep at
../legacy-codebases/zvec-grep; storybook at ../legacy-codebases/storybook; hadoop at
../legacy-codebases/hadoop; glamCRM at ../glamCRM; repo-graph is THIS repo.
