# CJOIN-PROVE-1: C/C++ AST↔SCIP Join Reliability (Stage B, ST1 / RK1)

Slice ID: CJOIN-PROVE-1
Status: EXECUTED 2026-05-31 — verdict GO for clean-C++ value envelope; ST1 macro sub-risk
OPEN (deferred to CJOIN-PROVE-2). Evidence: `docs/audits/cjoin-prove-1/findings.md`; probe:
`rust/tools/cjoin-probe`.
Depends: SCIP-CLANG-SPIKE-1 (GO — scip-clang real on leveldb), INGEST-CORE-1 (IMPLEMENTED
— the `(file,range)` join mechanism + IR exist and are clean for TS)
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`)
Addresses: **ST1 / RK1 — AST↔SCIP join reliability for C/C++.** Highest-severity strategic
trigger; **first** in Stage B. **Partially retires ST1:** the clean-C++ / leveldb-like
value-join is GO; the macro/preprocessor-heavy sub-risk remains OPEN (→ CJOIN-PROVE-2).

## The one risk this slice retires

Whether the `(file, range)` AST↔SCIP join — proven **clean** for TypeScript in
INGEST-CORE-1 (`join_bug=0`, `coordinate=0`; the only gap was extractor coverage) —
survives C/C++, where the preprocessor, macros, and header inclusion move, expand, or
synthesize the ranges SCIP reports relative to the source the tree-sitter AST sees. If the
join degrades badly **and** raw-anchored fallback is insufficient, C/C++ ships graph-only
and the "living working code" envelope narrows. This is the fault line, retired here on the
thin foundation, before any runtime is built on top.

**Research / probe tooling, not production code.** No production C/C++ ingestion path, no
adapter-trait extraction, no generalization of `repo-graph-scip-ingest`. The probe
(`rust/tools/cjoin-probe`, kept as reproducible evidence and reused by CJOIN-PROVE-2) reuses
the existing `scip` decode and `cpp-extractor` (tree-sitter-cpp), and measures.

## Identity direction under D3 — the fundamental difference from TS

This is NOT the TS join re-run on a new language. The identity direction **inverts**:

- **TS (INGEST-CORE-1):** AST identity is *primary* — the ts-extractor stable key IS the
  canonical identity; SCIP contributes linkage attached onto it.
- **C/C++ (D3 mixed-mode):** SCIP identity is *primary* — the scip-clang symbol + semantic
  graph are authoritative; the AST join exists *only* to attach value facts to that SCIP
  identity where the range join is strong, else the fact is raw-source-anchored + labeled.

The C/C++ semantic graph (defs/refs/calls) needs no AST join — it ships from SCIP
regardless. So the probe measures one thing: **can AST-derived value facts bind to
SCIP-native symbol identity by `(file,range)` when the preprocessor has moved / expanded /
synthesized the ranges?** That is why the retreat is "value-layer deferred, graph-only,"
not "no C/C++."

## Already established (NOT re-proven here)

- **SCIP-CLANG-SPIKE-1:** scip-clang produces compiler-grade C/C++ facts on leveldb
  (39 TUs → 90 docs, 16,643 occ, 0 errors; later CJOIN-PROVE-1 showed a stable join envelope
  but **not byte-identical SCIP output** — ±2 method defs / ±17 occ across captures).
  scip-clang viability = GO. CJOIN-PROVE-1 does not re-measure indexing.
- **INGEST-CORE-1:** the join mechanism (range containment + narrow name reconciliation),
  the IR, strict edge derivation — clean for TS. CJOIN-PROVE-1 reuses the mechanism and
  asks only whether it holds for C/C++.

## Target

`leveldb` (C++, CMake) — the SCIP-CLANG-SPIKE-1 primary, already captured:
- SCIP: **`/tmp/scip-spike/leveldb3.scip`** (the valid 90-doc capture) + **`leveldb-cap2.scip`**
  (freshly regenerated for determinism). **`leveldb.scip` / `leveldb2.scip` are broken
  externalized captures (0 docs, 1705 external) — INVALID; never use as evidence.**
- Source: `legacy-codebases/leveldb`.
- Build: `/tmp/scip-spike/leveldb-build/` (`compile_commands.json`).
- AST producer: `cpp-extractor` (tree-sitter-cpp 0.23), reused as-is — not modified.

## Approach (probe)

1. Decode `leveldb3.scip` (reuse `decode_index` / `scip_definitions` / `descriptors_info`).
2. Run `cpp-extractor` per source file → AST declaration ranges + exactly one value fact
   (cyclomatic complexity per function — mirrors INGEST-CORE-1's single-value-fact
   discipline).
3. Attempt the `(file, range)` join under **D3 mixed-mode**: the C/C++ *semantic graph*
   (defs/refs/calls) is authoritative from SCIP; *value facts* join to symbol identity where
   the range join is strong, else are raw-source-anchored + labeled.
4. Measure. No IR assembled into a runtime, no edges persisted — measurement only.

## Measures (exact — the go/no-go evidence)

**Primary denominator (the verdict basis): body-bearing product-source callables eligible
for cyclomatic attachment.** The chosen value fact is cyclomatic complexity, which only
exists for code with a body. So the denominator is the set of SCIP definitions that are
**function / method / constructor / destructor definitions WITH bodies, in product source** —
NOT declarations without bodies, NOT types / classes / namespaces / fields, NOT system-header
symbols, NOT external-only symbols. Using all declaration-kind defs as the denominator would
pollute the rate with symbols that *cannot* carry a cyclomatic metric and could manufacture a
false NARROW/NO-GO. The verdict is judged on this metric alone.

1. **Body-bearing callable join rate (PRIMARY / verdict):** % of body-bearing product-source
   callable SCIP definitions whose `(file,range)` joins a `cpp-extractor` AST function node,
   so the cyclomatic value fact can bind to the SCIP-native identity.
2. **Value-fact strong-join rate:** of those, % where cyclomatic actually attaches to the
   canonical SCIP identity vs degrades to raw-source-anchored + labeled (the D3 envelope).
3. **Macro / preprocessor range-mismatch rate:** of unjoined callables, how many fail because
   the SCIP range lands in macro-expanded / preprocessed territory the tree-sitter AST does
   not see as that symbol. The C/C++-specific failure mode TS lacks — isolated and counted,
   never lumped into "coverage gap."
4. **Source-file-kind split (required):** every measured symbol bucketed by file kind —
   `.cc`/`.cpp` implementation, `.h`/`.hpp` project header, system header / external include.
   System/external are reported and **EXCLUDED** from the product-source verdict; project
   headers count (C++ inline/template bodies legitimately live in headers).
5. **Residual cause split (honest diagnosis):** every unjoined callable classified as exactly
   one of — `macro_preprocessor_mismatch` / `cpp_extractor_coverage_gap` /
   `declaration_without_body` (non-value-bearing; must NOT count against the proof) /
   `external_or_system` / `genuine_join_bug`. Macro failures are NOT coverage gaps (the
   INGEST-CORE-1 overclaim lesson).

**Secondary diagnostics (reported, NOT the verdict basis):** all in-project declaration join
rate; type/class join rate; field/member join rate; system/external excluded count;
determinism — `leveldb3.scip` vs regenerated `leveldb-cap2.scip` (rate + classification
stable; raw counts vary ±2 methods / ±17 occ).

## Go/no-go (judged on the PRIMARY metric only)

- **GO:** body-bearing product-source callables join AST value facts with high reliability;
  residual failures are a clean, enumerable set under the five residual classes, with
  `declaration_without_body` / `external_or_system` correctly excluded (not counted against
  the proof); and unjoined value facts degrade to raw-source-anchored + labeled — never
  silently dropped or mis-attached. C/C++ then carries SCIP-native graph truth PLUS
  AST-attached value facts where the strong join exists. The mixed-mode boundary (which
  callable classes symbol-attach vs raw-anchor) is written down.
- **NARROW / NO-GO (ST1 retreat):** if body-bearing-callable join is unreliable AND
  raw-anchored fallback is insufficient → C/C++ ships **graph-only** (references/calls from
  SCIP, no value layer). "Living working code" narrows to TS/Rust + C-graph. Documented
  narrowing; the SCIP graph (already proven) still ships.

Retired by a *measured body-bearing-callable join rate + a written mixed-mode boundary* — not
by an inflated all-declarations rate, and not by "it joined once."

## Verdict (EXECUTED 2026-05-31)

**GO for leveldb-like clean-C++ AST↔SCIP value attachment.** Body-bearing callable join rate
is stable at **92.3%**; residual misses are cpp-extractor coverage gaps (anonymous-namespace
`.cc` class methods, template member functions, `operator()`); **no join bugs or coordinate
defects** (0/0). **ST1 is NOT fully retired** — macro/preprocessor-heavy behavior was not
exercised (leveldb is macro-light; `macro_mismatch=0`). **Open CJOIN-PROVE-2** for a
macro-heavy C/C++ target before declaring broad C/C++ value-layer readiness. Also recorded:
scip-clang is not byte-deterministic (±2 methods / ±17 occ across captures; join envelope
stable). Full evidence: `docs/audits/cjoin-prove-1/findings.md`.

## Definition of Done

- Probe at `rust/tools/cjoin-probe` (research tooling, non-production; reuses `decode_index`
  + `cpp-extractor`), runnable on `leveldb3.scip` + `legacy-codebases/leveldb`. Kept out of
  `rust/crates` so production crate deps are not polluted.
- Report at `docs/audits/cjoin-prove-1/findings.md` (local; `docs/audits/` gitignored) with
  Evidence-Law labels (EXECUTED / OBSERVED / INFERRED) on every measure.
- A **go / narrow / no-go** verdict with the mixed-mode boundary table.
- If narrow/no-go: the scope narrowing (C/C++ graph-only) recorded in the ADR + migration plan.

## Out of scope (hard guardrails)

- NO production C/C++ ingestion path; NO adapter-trait extraction / generalization of
  `repo-graph-scip-ingest` (premature until the join is proven — INGEST-CORE-1 anti-platform
  guardrail: the adapter trait is extracted when the second language is *committed*, not while
  probing it).
- NO C/C++ edge (call/reference) derivation into the IR — this probe measures the JOIN, not
  graph construction. C/C++ call-graph quality is downstream, only if GO.
- NO second config-heavy repo this pass (leveldb only); the Linux kernel is explicitly excluded.
- NO modification to `cpp-extractor` / `c-extractor`.

## Decisions — ratified 2026-05-31

1. **Value fact: cyclomatic-only** (first pass). Mirrors INGEST-CORE-1; isolates the join
   problem; avoids turning the probe into a C++ value-layer audit.
2. **C++ only** via `cpp-extractor`; `c-extractor` deferred (leveldb is C++; C-specific macro
   behavior becomes a separate probe only if C++ succeeds or that behavior is a distinct risk).
3. **Determinism: regenerated `leveldb-cap2.scip`** — `leveldb2.scip` is a broken externalized
   capture, so a fresh scip-clang capture rooted at the leveldb source was used. Finding: join
   rate + classification stable across captures; scip-clang counts vary ±2 methods / ±17 occ
   (NOT byte-deterministic, unlike scip-typescript).
4. **Primary denominator: body-bearing product-source callable definitions eligible for
   cyclomatic attachment** — NOT all declaration-kind defs (corrected at sign-off; an
   all-declarations rate is polluted by fields/types/bodiless declarations that cannot carry
   cyclomatic and would risk a false NARROW/NO-GO). All-in-project declaration join rate is a
   secondary diagnostic only. Source files split implementation / project-header / system /
   external so header/system artifacts do not pollute the verdict.

## References
- `docs/architecture/scip-migration-plan.md` (Stage B — CJOIN-PROVE-1)
- `docs/slices/scip-clang-spike-1.md` + `docs/audits/scip-clang-spike-1/` (scip-clang GO)
- `docs/slices/ingest-core-1.md` (the join mechanism + the honest-diagnosis discipline)
- `docs/architecture/adr/adr-extraction-substrate-scip-first.md` (the C/Linux ambition this gates)
