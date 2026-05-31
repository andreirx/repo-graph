# TRUST-MODEL-REBASE-1: `repo-graph-trust-model` Vocabulary Support Crate (Stage C, slice 1)

Slice ID: TRUST-MODEL-REBASE-1
Status: **BUILT (2026-05-31) — crate `repo-graph-trust-model`, 13 invariant tests green; maturity
PROTOTYPE.** D1 optional `serde` feature; **D2 completeness is QUERY-CONTEXTUAL (no global basis
`is_complete`)**; D3 enforcement at the `AnswerEnvelope` / `QueryCompleteness` layer. `IdentityBasis`
labels are **descriptive only**. Not PRODUCTION until LiveGraph/query surfaces consume it.
Depends: STAGE-C-ENTRY-DECISION (`docs/architecture/stage-c-entry-decision.md`) and the Stage B
evidence it synthesizes (CJOIN-2, XPART-1A/1B, XPART-ST3, REFRESH-PROBE-1, RUST-INGEST-PROVE-1).
Track: Extraction Substrate Pivot — **Stage C, slice 1** (before LIVEGRAPH-RUNTIME-1).

## Purpose / the risk this slice retires

Fix the Stage B trust/freshness/identity vocabulary as a **typed, tested, reusable support crate**
BEFORE any runtime or query code reads or emits it. The risk is **semantic drift**: if LiveGraph
or query migration encodes trust semantics ad hoc, the non-binary certainty states Stage B
established will be re-invented inconsistently and a costly rebase will follow. Definition of done
is **the crate + invariant tests** — NOT runtime wiring.

## The crate — `repo-graph-trust-model`

A **pure-domain** crate, the most stable/abstract layer (the Dependency Rule points inward to it).
Mirrors `repo-graph-ir`'s discipline:

```text
#![forbid(unsafe_code)]
zero deps on: scip, sqlite/rusqlite, tree-sitter, tokio/daemon, repo-graph-ir, repo-graph-scip-ingest
```

It is the vocabulary everything trust-related will depend on; it depends on nothing volatile.
Enforced structurally (a dependency that reached SCIP/SQLite/tree-sitter would defeat the slice).

**Name (ratified):** `repo-graph-trust-model` at `rust/crates/repo-graph-trust-model`. The existing
**`repo-graph-trust`** crate (`rust/crates/trust`) is the **shipped v1 trust-reporting service** for
the OUTGOING SQLite/raw-graph substrate (storage-backed; depends on classification/serde/serde_json)
— **not pure-domain, and untouched by this slice** (hard guardrail: no shipped daemon/CLI/storage
dependency changes; no changes to `rust/crates/trust`). The two coexist during migration. **Stage D
possibility:** after raw-graph decommission, `repo-graph-trust-model` may become the canonical trust
crate, or the v1 `repo-graph-trust` name may be retired — a decommission decision, not a Stage C one.

**Maturity:** `repo-graph-trust-model` starts **PROTOTYPE → MATURE-support candidate** once the
invariant tests pass. It is **NOT PRODUCTION** until LiveGraph/query surfaces consume it.

## Types (the vocabulary — closed enums; extend only via a later slice)

1. **`AnswerClass`** — `Exact` · `Partial` · `Unavailable` · `Stale`. Plus `Granularity`
   (`PartitionSummary` · `CallerDetail`).
2. **`FreshnessState`** — `Fresh` · `Stale` · `PrecisionPending` · `RefreshFailed` · `Unavailable`.
3. **`IdentityBasis`** (generic; language attaches separately) — `AstAdopted` · `ScipSynthesized` ·
   `AstFileScope` · `DeclarationMapExact` · `NameExactUnique` · `RangeNameConfirmed` · `RawAnchored`.
4. **`DegradationReason`** (orthogonal to basis) — `AnonymousStructuralMember` ·
   `UnreconciledExportSurface` · `AmbiguousAlias` · `UnresolvedAlias` · `RawAnchoredByFailedNameGuard`
   · `ScipFallbackIdentity` · `UnsupportedWorkspaceMode` · `DefinitionOutsideDocument` ·
   `DuplicateCanonicalized`.
5. **`LanguageSupport`** — `TypeScriptPrimary` · `CppGuarded` · `RustPartialBeta`.
6. **`ProvenanceBasis`** — alias/reconciliation provenance: package name + version, public export
   path, declaration file, declaration map, source file, the `IdentityBasis` that produced it.
   (A raw DTO — simple owned fields, no framework types.)
7. **`QueryCompleteness`** — a derived verdict (`Complete` / `Degraded` / `Unknown`) computed
   **query-contextually** by `classify_answer(CompletenessInput)`. Never read from the basis alone.

**Completeness is query-contextual, NOT a global basis property (D2 ratified).** `IdentityBasis`
labels are **descriptive only** — the SAME basis is complete for one query granularity and degraded
for another. A pure policy computes the verdict:

```text
classify_answer(input: CompletenessInput) -> (AnswerClass, QueryCompleteness)
  CompletenessInput = {
    granularity,            // the requested answer granularity (file-ref vs call-graph vs symbol-owner)
    bases: Vec<IdentityBasis>,
    freshness: FreshnessState,
    degradation_reasons: Vec<DegradationReason>,
    language: LanguageSupport,
  }
```

Examples (why it cannot be global): `AstFileScope` is Complete for "which file references X?" but
Degraded for "which function calls X?"; `RawAnchored` is Complete for "where is this raw fact
observed?" but Degraded for "which symbol owns it?"; `ScipSynthesized` is Complete for a
compiler-derived reference identity but Degraded for governance/A1-stable identity with no canonical
value key.

## Invariants (encoded + tested — the core deliverable)

1. **`Exact` requires `QueryCompleteness::Complete`** — computed query-contextually by
   `classify_answer` (granularity + bases + freshness + reasons + language), NOT from the basis
   alone; otherwise the class is `Partial`/`Unavailable`, never `Exact`.
2. **`Partial` must list missing / degraded reasons** — a `Partial` with an empty
   missing-partitions AND empty `DegradationReason` set is illegal (would be indistinguishable from
   `Exact`).
3. **`Unavailable` is not empty** — `Unavailable` carries an explicit reason and is
   consumer-distinguishable from an `Exact` empty result. (`null` ≠ empty, applied to the class.)
4. **`Stale` is not `Fresh`** — the two `FreshnessState`s are distinct; a `Stale` answer can never
   be labelled `Fresh`/current.
5. **`null` ≠ empty** — unknown/unaddressable serializes as `Unavailable`/`null`, never as an empty
   collection that reads as known-zero. (architecture.md degradation primitive, type-enforced.)
6. **`PrecisionPending` cannot be `Exact` without an explicit fresh SCIP basis** — if
   `FreshnessState::PrecisionPending` (SCIP refresh lagging behind the AST fast delta), the class is
   `Exact` only when the answer carries a fresh SCIP-backed `IdentityBasis`; otherwise `Partial`.

Enforced at the **`AnswerEnvelope<T>`** layer (smart constructors; illegal states unrepresentable)
— basis alone never decides exactness:

```text
AnswerEnvelope<T> {
  class: AnswerClass,
  freshness: FreshnessState,
  completeness: QueryCompleteness,
  data: Option<T>,
  degradation_reasons: Vec<DegradationReason>,
  provenance: Vec<ProvenanceBasis>,
}
```

Constructors (a downstream runtime CANNOT mint an unjustified `Exact`):
- `exact(data, completeness, freshness, provenance)` — requires `data` Some, `freshness == Fresh`,
  `completeness == Complete`, `degradation_reasons` empty.
- `partial(data, reasons, …)` — requires `reasons` non-empty.
- `unavailable(reason, …)` — requires `data` None, `reason` non-empty.
- `stale(last_good_data, …)` — `class == Stale`, `freshness != Fresh`.
- `PrecisionPending` cannot be `Exact` unless the answer is explicitly NOT dependent on SCIP-backed
  state.

## Ratified decisions (2026-05-31)

**D1 — serialization: optional `serde` feature, `default = []`.** Trust vocabulary is domain;
serialization is a delivery concern. Query/API layers will need stable DTOs later → feature-gated
serde (pure by default; query-migration enables it).

**D2 — completeness is QUERY-CONTEXTUAL, not a global basis property.** `IdentityBasis` has **no**
global `is_complete()`; labels are **descriptive only**. Completeness is computed by
`classify_answer(CompletenessInput) -> (AnswerClass, QueryCompleteness)` (a pure `CompletenessPolicy`)
from requested answer **granularity** + bases + freshness + degradation reasons + language. The SAME
basis can be Complete in one context and Degraded in another (examples in Types). **No global
complete/incomplete basis list; no false semantics baked into the enum.**

**D3 — enforce at the `AnswerEnvelope` / `QueryCompleteness` layer** (smart constructors; illegal
states unrepresentable) — basis alone never decides exactness. The `AnswerEnvelope<T>` shape +
constructor rules (above) are normative.

## Definition of done

- `repo-graph-trust-model` crate compiles with the forbidden-dep set absent (structurally verified).
- All 7 types defined as closed enums / DTOs with `Debug, Clone, PartialEq, Eq` (+ `Hash` where
  sensible).
- All 6 invariants encoded (`AnswerEnvelope` smart constructors per D3) AND covered by unit tests,
  including negative cases (cannot build `Exact` when `classify_answer` returns Degraded for the
  given context; `PrecisionPending` + `Exact` only when the answer is not SCIP-dependent).
- `classify_answer(CompletenessInput)` (D2) tested across **query-contextual** cases — the SAME
  basis Complete in one context and Degraded in another (e.g. `AstFileScope`: file-ref Complete,
  call-graph Degraded).
- No runtime, no cache, no SQLite, no query migration, no LiveGraph wiring.

## Guardrails (hard)

```text
No runtime code.
No cache format.
No SQLite cleanup.
No query migration yet.
No LiveGraph wiring.
```

One reusable trust support module: types + invariants + tests. Nothing volatile may be reachable
from it.

## Exit criterion

The crate is the single source of truth for the Stage B vocabulary; the six invariants are
type-enforced and tested (positive + negative); the forbidden-dependency boundary holds. A later
slice (LIVEGRAPH-RUNTIME-1) consumes these types; it does not redefine them.

## References
- `docs/architecture/stage-c-entry-decision.md` (the vocabulary + the mandate this slice fulfils)
- `docs/slices/ingest-core-1.md` (`repo-graph-ir` pure-domain discipline to mirror)
- `agent_docs/architecture.md` (Layer model; `null` = unknown / empty = known-zero)
- Stage B slices (each `IdentityBasis` / `DegradationReason`'s origin)
