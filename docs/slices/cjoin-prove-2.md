# CJOIN-PROVE-2: Macro/Preprocessor-Heavy C/C++ Join — Misattachment Test (Stage B, ST1)

Slice ID: CJOIN-PROVE-2
Status: EXECUTED 2026-05-31 — verdict **GO with macro-degraded envelope; ST1 RETIRED**
conditionally (production join must require range containment + name correspondence).
Evidence: `docs/audits/cjoin-prove-2/findings.md`; probe `rust/tools/cjoin-probe`.
Depends: CJOIN-PROVE-1 (fixture/probe setup; the join mechanism + probe `rust/tools/cjoin-probe`
exist — its original strong-attach verdict is superseded by CJOIN-PROVE-2), SCIP-CLANG-SPIKE-1
(scip-clang viability GO)
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`)
Closes: **ST1 / RK1 macro sub-risk** — the part CJOIN-PROVE-1 left open.

## The one risk this slice retires (the rest of ST1)

CJOIN-PROVE-1 originally reported 92.3% **range-contained** attachment on leveldb — amended
by this slice to **77.1% name-confirmed**, with 15.1% rejected as range-only misattachment.
It never checked that a range-join was the *right* symbol. The open sub-risk:
when macros distort the source/semantic ranges (function-generating macros, X-macros,
macro-wrapped declarations), do AST-derived value facts:

1. **attach strongly** (range still source-faithful), or
2. **degrade to raw-source-anchored** (honest, labeled), or
3. **MISATTACH** — bind to the *wrong* symbol because a distorted SCIP range overlaps an
   unrelated cpp-extractor function span?

**The no-go condition is misattachment, NOT a low attach rate.** A low attach rate that
degrades raw-anchored is acceptable; a value fact silently bound to the wrong callable is
not. This slice exists to find out which happens under heavy macros.

## What the probe must add (extends `rust/tools/cjoin-probe`)

CJOIN-PROVE-1's probe joins a SCIP `Method` def to a cpp-extractor body-function span by
**range containment alone** — it never checked that the joined function is *the same
symbol*. That is exactly the blind spot macros exploit. CJOIN-PROVE-2 adds a
**name-correspondence check on every join**:

- **Confirmed attach:** SCIP def range lands in a body-function span AND the SCIP def's
  terminal name corresponds to that function's name. The value fact binds correctly.
- **MISATTACHMENT:** range lands in a span but the names do NOT correspond — the SCIP def
  is a different symbol; binding cyclomatic here would be wrong. **This is the no-go signal.**
- (Unjoined classification — macro / declaration / coverage / bug — carries over from
  CJOIN-PROVE-1.)

Re-run the extended probe on leveldb3 as well — which proved to be **not** a clean baseline
but the **C++ annotation-macro counterexample** that exposed the range-only flaw (15.1%
rejected). nginx (macro-heavy C) is the clean case (95.9%). The unit-tested name comparison
is tri-state: only `Confirmed` attaches; `Mismatch` and `Uncomparable` raw-anchor.

## Measures (exact)

1. **Misattachment count + rate (PRIMARY / verdict):** confirmed-attach vs range-joined-but-
   name-mismatched. The verdict turns on this.
2. **Strong-attach rate:** confirmed attaches / body-bearing callables.
3. **Raw-anchored-degrade rate:** unjoined body-bearing callables that degrade honestly
   (macro/coverage), i.e. *not* attached at all (acceptable) vs misattached (not).
4. **Macro-locus split:** of misattachments and raw-anchored degrades, how many sit at
   macro-expansion / generated loci (the distortion signal) vs ordinary source.
5. **leveldb3 — annotation-macro counterexample** (NOT a clean baseline): validates that
   range-only joining misattaches and that the name guard rejects those attachments.

## Target selection (BY EVIDENCE — pending the tooling decision below)

Surveyed `legacy-codebases` (function-like-macro density, build system, compile_commands
feasibility with present tooling: `cmake` yes; `bear`/`meson`/`pkg-config` MISSING):

- **nginx — primary candidate.** 394 function-like macros (most macro-heavy present),
  product-source macro use (ngx_* module/string/config macros), ample body-bearing
  callables. Build: custom configure/make → compile_commands needs `bear` (or a `make -n`
  parse). **The user's #1 candidate.**
- **sqlite — secondary.** 266 function-like macros; autoconf/make → needs `bear`.
- **duckdb — rejected as primary.** Only 83 macros (modern C++, macro-light); cmake-buildable
  now, but too weak to stress the risk — would risk another inconclusive "macro-light" result.
- **Linux kernel — deferred** (config+build too heavy for this gate).
- **Generated mini fixture — supplemental only**, never primary evidence (per the criteria).

## Go/no-go (judged on misattachment)

- **GO with macro-degraded envelope (retires ST1):** value facts attach strongly where
  ranges are source-faithful; macro-expanded/generated constructs degrade to
  raw-source-anchored + labeled; **zero terminal-name-mismatch attachments** (same-name
  overload/signature ambiguity remains a documented residual). This is the acceptable
  outcome — strong where it can be, honest where it cannot, never wrong.
- **NO-GO:** any non-trivial rate of silent misattachment (value facts bound to the wrong
  callable). Retreat: C/C++ value layer is gated to source-faithful regions only, or
  graph-only under heavy macros — documented narrowing. The SCIP graph still ships.

## Verdict (EXECUTED 2026-05-31)

**GO with macro-degraded envelope — ST1 RETIRED, conditional on the name-correspondence
guard.** Evidence (`docs/audits/cjoin-prove-2/findings.md`):

- **nginx (macro-heavy C, 394 macros):** 95.9% name-confirmed; **0 mismatch + 1 uncomparable**
  (a `*` name-extraction quirk → raw-anchored, never attached). **Macro-heavy C does not
  misattach.**
- **leveldb (C++ thread-annotation macros):** 77.1% name-confirmed; **135 rejected (15.1%) =
  135 mismatch + 0 uncomparable** — `SCOPED_LOCKABLE`/`EXCLUSIVE_LOCK_FUNCTION` collapse
  cpp-extractor's class parse into one oversized node, so siblings range-overlap it.
- Misattachment is **detectable** (name mismatch) → **preventable** (raw-anchored, never
  bound). **Zero detected terminal-name-mismatch attachment** under the guard; uncomparable
  names raw-anchor.

**Residual (NOT retired):** the guard checks range containment + *terminal-name* correspondence.
It does **not** retire same-name **overload / signature / template-instantiation** ambiguity (a
wrong attach whose terminal name happens to match). The proven claim is *zero detected
terminal-name-mismatch attachment*, not zero misattachment. Stronger C/C++ value-join hardening
needs signature / arity / scope correspondence — deferred.

**Hard architectural rule (the ST1 closure condition):** a C/C++ value fact may attach to a
SCIP identity only when **range containment AND name correspondence agree**; otherwise it
remains raw-source-anchored. **Range-only joining is forbidden in production** — it silently
misattaches 15.1% on leveldb-class code.

## Definition of Done

- Extended `rust/tools/cjoin-probe` (name-correspondence / misattachment check), run on the
  macro-heavy target + leveldb3 annotation-macro counterexample.
- Report at `docs/audits/cjoin-prove-2/findings.md` (local) with Evidence-Law labels.
- A **GO-with-macro-degraded-envelope / NO-GO** verdict. On GO, the range-only /
  terminal-name-mismatch sub-risk is retired and Stage B proceeds to XPART-PROVE-1;
  same-name overload/signature ambiguity remains deferred hardening.

## Decisions (ratified)

1. **Target + tooling:** nginx via `bear` (installed). `bear`/`meson`/`pkg-config` installed;
   nginx (`auto/configure` + `bear -- make`) produced a 118-TU compile_commands; scip-clang
   indexed it (190 docs). duckdb rejected as too macro-light.
2. **Misattachment threshold:** NO-GO on any *systemic* misattachment class; isolated one-offs
   investigated, not auto-fail. Outcome: no systemic terminal-name-mismatch attach survives the
   guard; same-name overload/signature ambiguity is recorded as residual (see Verdict).

## References
- `docs/slices/cjoin-prove-1.md` (fixture/probe setup; original strong-attach verdict superseded)
- `docs/architecture/scip-migration-plan.md` (Stage B, ST1)
- `docs/audits/cjoin-prove-1/findings.md` (leveldb evidence, local)
