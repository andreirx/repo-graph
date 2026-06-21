# TS-EXTRACTOR-COVERAGE-1: TypeScript Declaration Coverage Gap

Slice ID: TS-EXTRACTOR-COVERAGE-1
Status: PLANNED (follow-on; recorded from INGEST-CORE-1 evidence)
Origin: `docs/slices/ingest-core-1.md` join probe
Track: Extraction Substrate Pivot — extractor coverage

## Why this exists

INGEST-CORE-1's AST↔SCIP definition-join probe (FRAKTAG `@fraktag/engine`) found the
join mechanism **clean** — `join_bug = 0`, `coordinate/path mismatch = 0` — but a
declaration-join ceiling of **81.9%** (979/1195 declaration-kind defs) *as first measured*.

> **Correction (2026-05-30, INGEST-CORE-1 edge derivation).** The original claim that the
> entire 214 gap is `ts-extractor` coverage — "no AST node at all" — was **wrong**. The
> 214 was a **mix of causes**, since separated by evidence:
> - **Constructors / getters / setters — name-reconciliation gaps, NOT coverage gaps.**
>   Both SCIP and `ts-extractor` model them; the join failed only because SCIP marks them
>   `<constructor>` / `<get>X` / `<set>X` while `ts-extractor` uses the bare identifier.
>   INGEST-CORE-1 added narrow name reconciliation and **recovered 31 of 33** such defs
>   (fallback 216 → 185; matched 979 → 1010). No longer a coverage concern.
> - **Abstract-class explicit constructors — a real `ts-extractor` coverage gap.** The 2
>   unrecovered (`BaseLLMAdapter` L31, `BaseNugget` L23) are explicit constructors of
>   `abstract class`es for which `ts-extractor` emits **no AST node** (it does emit
>   non-abstract constructors). This is a genuine extractor bug this slice owns.
> - **Interface / type-member field signatures — genuine SCIP-only granularity.** The
>   `Term` residual (`strictness`, `kbId`, `title`, … in `types.ts`) has no AST node by
>   design; `ts-extractor` does not emit interface field nodes.
> - **Object-literal / destructuring bindings — non-modeled granularity.**

This slice owns deciding which of those SCIP declarations *should* become repo-graph
identity nodes, and extending `ts-extractor` **only where the product model wants
those identities**. It does not belong in INGEST-CORE-1 (changing `ts-extractor`
semantics was explicitly out of that slice's scope).

## Evidence (INGEST-CORE-1 join probe, fraktag/engine)

- Declaration denominator: Namespace 52/52, Type 130/132, Method 329/381, Term 468/630.
- Residual unmatched (declaration kinds): 162 `Term` + 52 `Method`, all classified
  `ts_extractor_coverage_gap` (no AST node), 0 join/coordinate failures.
- The unmatched `Method`s were a **mix**, not a single cause: most were
  `<constructor>` / `<get>` / `<set>` members now recovered by name reconciliation (see
  Correction); the true residual is abstract-class explicit constructors (extractor gap)
  plus interface method signatures / object-literal methods. The earlier "not standard
  class methods" wording was an overclaim and is retracted.
- **`SymbolInformation.kind` is unusable for classification** — `scip-typescript`
  leaves it `UnspecifiedKind` for these symbols. Classification must use source/AST
  analysis, not the SCIP `Kind` field.

## Scope

Classify the unmatched SCIP declaration kinds into:
- **real `ts-extractor` coverage gaps** — **explicit constructors of `abstract` classes**
  (proven: `ts-extractor` emits no node for them) and any class fields/properties the
  extractor should emit but doesn't (extend the extractor);
- **interface / type-alias member signatures** — type-level members;
- **object-literal / destructuring bindings** — SCIP-only granularity;
- **other SCIP-only / by-design exclusions**.

For each category, decide: become a repo-graph identity node, or remain labeled
non-modeled granularity. Extend `ts-extractor` only for categories the product model
wants as identities.

## Non-goals
- Not INGEST-CORE-1. INGEST-CORE-1 reconciled constructors/getters (31 recovered) and
  ships the remaining **185** as labeled fallback nodes (cause = extractor coverage /
  non-modeled granularity), count surfaced; this slice owns the residual.
- No change to the SCIP↔AST join mechanism (it is clean).

## References
- `docs/slices/ingest-core-1.md`
- `docs/slices/scip-ingest-ir-1.md` (R3 AST↔SCIP correlation)
- Probe: `rust/crates/repo-graph-scip-ingest/examples/join_probe.rs`
