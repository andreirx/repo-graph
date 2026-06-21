# Stage C Entry Decision — Trust/Freshness Vocabulary Before Runtime

Status: **DECISION RECORD (architecture) — no code.** 2026-05-31.
Track: Extraction Substrate Pivot (`docs/architecture/scip-migration-plan.md`). Stage B complete;
this record gates Stage C entry.
Depends: all Stage B evidence — CJOIN-PROVE-1/2, XPART-PROVE-1A/1B, XPART-ST3-BOUNDARY-DECISION,
REFRESH-PROBE-1, RUST-INGEST-PROVE-1.

## Purpose

Translate Stage B evidence into (a) the **certainty / freshness / maturity vocabulary** the runtime
must speak, and (b) the **Stage C slice order**. Stage B produced **multiple non-binary certainty
states**. If LiveGraph or query migration starts before this vocabulary is fixed, the runtime will
encode trust semantics **ad hoc** and they will drift. **Stage C starts by preventing semantic
drift.**

## The decision

**TRUST-MODEL-REBASE-1 comes before LIVEGRAPH-RUNTIME-1.** Fix the vocabulary first; the runtime and
query surfaces then *consume* a fixed vocabulary instead of inventing one.

**Stage C order:** `1. TRUST-MODEL-REBASE-1 → 2. LIVEGRAPH-RUNTIME-1 → 3. QUERY-MIGRATION-1 →
4. VALUE-JOIN-1`.

---

## Q1 — Certainty classes after Stage B

Stage B replaced "resolved / unresolved" with a graded **identity-basis** taxonomy. Each class is
traceable to the slice that established it.

### `IdentityBasis` — how a node/edge's identity was obtained (GENERIC; language attaches separately)
| basis | meaning | proven on | origin |
|---|---|---|---|
| `AstAdopted` | value-level `(file,range)` AST join | TS | INGEST-CORE-1 |
| `ScipSynthesized` | SCIP-descriptor synthesized; no AST join | TS, Rust | INGEST-CORE-1 |
| `AstFileScope` | file/module-scope structural node | TS | INGEST-CORE-1 |
| `DeclarationMapExact` | published→source via declaration-map sources + descriptor-exact | TS | XPART-PROVE-1B |
| `NameExactUnique` | unique code-descriptor match (strict predicate) | TS | XPART-PROVE-1B |
| `RangeNameConfirmed` | value fact attached on range **and** terminal-name correspondence | C/C++ | CJOIN-PROVE-2 |
| `RawAnchored` | identity kept at the raw SCIP anchor; value fact NOT attached | C/C++ | CJOIN-PROVE-2 |

The basis is **generic** — `RangeNameConfirmed`/`RawAnchored` are currently proven on C/C++ but the
concept is language-independent. **Language attaches separately** via `LanguageSupport` + provenance,
never baked into the basis name. Rust per-crate identity is **not** a distinct basis: it is
`ScipSynthesized` + `LanguageSupport::RustPartialBeta` + `DegradationReason::ScipFallbackIdentity`.

### Edge bases (the derivation basis of an edge)
`SyntaxConfirmedCall` (→ `Calls`) · `DerivedReference` (→ `References`) · `FileScopeReference`
(module-init, never `Calls`). Origin: INGEST-CORE-1.

### `DegradationReason` — WHY a fact is degraded (a SEPARATE axis from `IdentityBasis`)
A fact carries an `IdentityBasis` (how identity was obtained) AND, when degraded, one or more
`DegradationReason`s. They are **orthogonal** — never conflate basis with reason.
- `AnonymousStructuralMember` — TS `typeLiteralNN`, compilation-unit-relative, not cross-partition-addressable (XPART-1B / ST3 Residual 1)
- `UnreconciledExportSurface` — package without declaration maps / complex `exports` (XPART-ST3 Residual 2)
- `AmbiguousAlias` — alias reconciliation found multiple candidates (XPART-1B)
- `UnresolvedAlias` — alias reconciliation found zero candidates (XPART-1B)
- `RawAnchoredByFailedNameGuard` — C/C++ value fact withheld; range matched but name correspondence failed (CJOIN-PROVE-2)
- `ScipFallbackIdentity` — identity is `ScipSynthesized`, no value-level join (TS fallback; Rust dominant)
- `UnsupportedWorkspaceMode` — whole-workspace indexing unsupported (Rust; RUST-INGEST-PROVE-1)
- `DefinitionOutsideDocument` — producer emitted a definition not in its document (Rust def-not-in-document; tolerated if bounded)
- `DuplicateCanonicalized` — duplicate symbol canonicalized by the deterministic dedup rule; provenance alias kept (Rust)

**Certainty is now TWO labelled axes — `IdentityBasis` (how) and `DegradationReason` (why-degraded) —
not a boolean.** Language is a THIRD axis (`LanguageSupport`, Q3).

## Q2 — Freshness states

| state | meaning | origin |
|---|---|---|
| `Fresh` | resident epoch == latest; no refresh in flight | — |
| `Stale` | refresh in flight; **last-good epoch served** meanwhile | REFRESH-PROBE-1 (D4) |
| `PrecisionPending` | two-speed gap: AST fast delta applied, SCIP slow refresh lagging | REFRESH-PROBE-1 (B); RUST (B-very-slow-async) |
| `RefreshFailed` | refresh errored; **last-good epoch kept** | REFRESH-PROBE-1 (D4) |
| `Unavailable` | no xref entry / partition not indexed | XPART-PROVE-1A |

Epoch contract (ratified, REFRESH-PROBE-1): serve last-good epoch during refresh; **atomic swap**
on success; keep last-good on failure; coalesce bursts; **never `Exact`-empty for stale/missing
state**.

## Q3 — Language support maturity (`LanguageSupport`, a SEPARATE axis)

| `LanguageSupport` | language | boundary |
|---|---|---|
| `TypeScriptPrimary` | TypeScript | declaration-map-backed **named** boundaries proven (XPART-1B 78/78); anonymous + no-decl-map degrade explicitly |
| `CppGuarded` | C / C++ | value join only on **range + terminal-name correspondence**; range-only forbidden; overload/signature/template residual deferred (CJOIN-PROVE-2) |
| `RustPartialBeta` | Rust | per-crate only (whole-workspace unsupported); B-very-slow-async refresh; ~94–96% `ScipSynthesized` identity; **no TS/C parity** (RUST-INGEST-PROVE-1) |

`LanguageSupport` is a query-visible label, **separate** from `IdentityBasis` and
`DegradationReason`. A fact's language lives here (+ provenance), never inside the basis name. Each
language carries a scoped support contract; maturity is not a global "supported" claim.

## Q4 — Degraded answer classes every query surface must preserve

The cross-partition answer-class contract (XPART-PROVE-1A) generalizes to **all** query surfaces:

- **Answer classes:** `Exact` / `Partial` / `Unavailable` / `Stale` (+ freshness `PrecisionPending`
  / `RefreshFailed`), each with a granularity (`PartitionSummary` / `CallerDetail`).
- **Degraded reasons (machine-readable `DegradationReason`):** the Q1 set —
  `AnonymousStructuralMember`, `UnreconciledExportSurface`, `AmbiguousAlias`, `UnresolvedAlias`,
  `RawAnchoredByFailedNameGuard`, `ScipFallbackIdentity`, `UnsupportedWorkspaceMode`,
  `DefinitionOutsideDocument`, `DuplicateCanonicalized` (+ freshness `Refreshing` / `RefreshFailed`).
- **`null` ≠ empty** (architecture.md primitive): unknown/unaddressable → `Unavailable`/`null`,
  **never** an empty result that reads as known-zero. (XPART-ST3-BOUNDARY-DECISION.)
- **No `Exact` unless all required bases are complete** (XPART-1B precision rule, generalized): if a
  target or any answer member depends on an `Unresolved`/`Ambiguous`/degraded basis, the class is
  `Partial`/`Unavailable`, never `Exact`.

These are **invariants the runtime must enforce**, not per-query conventions.

## Q5 — Which Stage C slice comes first, and why

**TRUST-MODEL-REBASE-1 first.** Stage B created the non-binary vocabulary above; the runtime and
query surfaces must consume a **fixed** vocabulary. Building LiveGraph or migrating queries first
would bake trust/freshness semantics into runtime code ad hoc, then force a costly rebase. Fix the
vocabulary as a typed, tested contract **before** any code reads or emits it.

## What TRUST-MODEL-REBASE-1 must produce (its mandate, set here)

A pure-domain **support crate `repo-graph-trust`** (no SCIP / SQLite / tree-sitter / daemon deps)
defining the typed, tested trust vocabulary — **no runtime wiring**. Types:
- `AnswerClass` — `Exact` / `Partial` / `Unavailable` / `Stale` (+ granularity).
- `FreshnessState` — `Fresh` / `Stale` / `PrecisionPending` / `RefreshFailed` / `Unavailable`.
- `IdentityBasis` — the generic Q1 taxonomy (`AstAdopted` … `RawAnchored`).
- `DegradationReason` — the Q1 reasons (orthogonal to basis).
- `LanguageSupport` — `TypeScriptPrimary` / `CppGuarded` / `RustPartialBeta`.
- `ProvenanceBasis` — alias / reconciliation provenance (package, version, export path, declaration
  file, `.d.ts.map`, basis).
- `QueryCompleteness` — the derived completeness verdict binding the above.

Invariants (tested):
- `Exact` requires a complete basis,
- `Partial` must list missing / degraded reasons,
- `Unavailable` is not empty,
- `Stale` is not `Fresh`,
- `null` ≠ empty,
- `PrecisionPending` cannot be `Exact` without an explicit fresh SCIP basis.

TRUST-MODEL-REBASE-1 is a **reusable trust support module**: types + invariants + tests only. It does
NOT build the runtime, cache, query migration, or wire into LiveGraph.

## Fact Certainty Model alignment

This vocabulary refines the existing Layer model (`agent_docs/architecture.md`), it does not replace
it: extracted exact identities are Layer 0–1; reconciled aliases (dist↔src) are **Layer 2 bounded
inference** carrying provenance; degraded/unresolved classes are the **surfaced unknowns** required
by Dependency Rule 3 ("outer layers must surface unknowns"); `null` ≠ empty is the architecture
degradation primitive made query-visible.

## Guardrails (this record and TRUST-MODEL-REBASE-1)

```text
No runtime code.
No cache format.
No SQLite cleanup.
No query migration yet.
```

## References
- `docs/slices/cjoin-prove-2.md` (C/C++ range+name guard; raw-anchored)
- `docs/slices/xpart-prove-1.md` + `xpart-prove-1b.md` (answer classes; identity bases; precision rule)
- `docs/slices/xpart-st3-boundary-decision.md` (degraded classes; `null` ≠ empty)
- `docs/slices/refresh-probe-1.md` (B two-speed; freshness states; epoch contract)
- `docs/slices/rust-ingest-prove-1.md` (Rust PARTIAL/BETA; B-very-slow-async; fallback identity)
- `docs/slices/ingest-core-1.md` (IdentitySource; EdgeBasis)
- `agent_docs/architecture.md` (Layer model; `null` = unknown / empty = known-zero)
