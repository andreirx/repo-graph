# VALUE-JOIN-1: Attach Value-Layer Facts to LiveGraph Identities (Stage C, slice 4)

Slice ID: VALUE-JOIN-1
Status: **BUILT (2026-05-31) — `repo-graph-livegraph` value-fact support; 8 value-fact tests (25
livegraph total) green; clippy/fmt clean; workspace builds.** Headless `value_facts(symbol)`;
separate value-fact channel (D6); epoch-bound facts (D7). TS only (D4). Maturity **PROTOTYPE**. The
real complexity→`ValueFact` adapter is deferred to the wiring layer (see Commit scope).
Depends: QUERY-MIGRATION-1, TRUST-MODEL-REBASE-1, INGEST-CORE-1 (`repo-graph-scip-ingest` complexity),
STAGE-C-ENTRY-DECISION.
Track: Extraction Substrate Pivot — **Stage C, slice 4** (after QUERY-MIGRATION-1, before Stage D).

## Purpose / the one risk this slice retires

Attach Stage B value-layer facts to LiveGraph identities through the trust model **without
overclaiming certainty** — attach under proven basis rules, preserve raw-anchored facts when symbol
attachment is unsafe, expose trust-labelled, never present a raw-anchored fact as `Exact`
symbol-owned data.

## KEY SEMANTIC RULE (enforced)

```text
A value fact is not less true because it is raw-anchored.
Only the OWNERSHIP claim is degraded — not the measured value.
```

The measured value and the ownership/attachment claim are **separate axes**. A non-owned complexity
reading is still returned with its real value; the trust label degrades only the attachment
(`Partial` / `RawAnchored` / `DegradationReason`). Unsupported/missing → `Unavailable` (null ≠ empty).

## Ratified decisions (2026-05-31)

- **D1 — cyclomatic complexity only.** Already produced by INGEST-CORE-1 (`scip-ingest` `complexity`
  map), low-risk.
- **D2 — attach symbol-owned only when the basis is `SymbolOwnership`-complete**
  (`AstAdopted` / `DeclarationMapExact` / `NameExactUnique` / `RangeNameConfirmed`). Otherwise
  raw-anchored, never silently owned.
- **D3 — preserve unsafe/unowned facts as `RawAnchor`** (first-class `ValueFact`, not dropped).
- **D4 — TypeScript only.** Record: **C/C++ guarded value facts → VALUE-JOIN-CXX-1 (deferred);** Rust
  value facts deferred until a Rust value extractor / AST join exists.
- **D5 — headless `value_facts(symbol)` API only.** No value-join into callers/callees.
- **D6 — separate value-fact channel into LiveGraph.** `load_value_facts(partition_id,
  Vec<ValueFact>)` — value facts loaded SEPARATELY from `PartitionIr`. Rationale (Common Closure):
  `PartitionIr` is the structural graph artifact; value facts are a different volatility axis
  (complexity, boundaries, resources, contracts, quality grow independently). **`repo-graph-ir` is
  NOT touched.**
- **D7 — value-fact epoch coherence: Option A (explicit epoch binding).** Facts are partition-scoped
  AND epoch-bound; a fact is valid only for the partition epoch that produced it. On partition swap,
  the prior epoch's value facts are STALE/invalid until reloaded for the new epoch. (Explicit binding
  makes stale facts detectable + testable; not implicit replacement.)

## Model (LiveGraph-local; grounded in `repo-graph-ir` types)

```text
ValueFactKind { CyclomaticComplexity }

ValueSubject {
  Symbol(CanonicalKey),
  RawAnchor(SourceRange),          // SourceRange already carries `file` — no separate field
}

ValueFact {
  subject: ValueSubject,
  kind: ValueFactKind,
  value: u32,
  basis: IdentityBasis,            // from repo-graph-trust-model
  source_range: Option<SourceRange>,
  provenance: Provenance,          // from repo-graph-ir
}
```

`RawAnchor` is NOT a `CanonicalKey` (do not overload canonical identity for raw anchors). The `Slot`
gains `value_facts: Vec<ValueFact>` + `value_facts_epoch: Option<PartitionEpoch>` (the partition
epoch the batch was loaded for — D7). `load_value_facts` stamps the current partition epoch; retained
on unload like the xref summary.

## `value_facts(symbol)` trust labelling (SymbolOwnership granularity)

1. Resolve the symbol's defining partition (via `defines`). **Unknown symbol → `Unavailable`** (null
   ≠ empty).
2. Find a `ValueFact` with `subject == Symbol(symbol)` in that partition. **No fact → `Unavailable`.**
3. **Epoch coherence (D7):** `value_facts_epoch != Some(current partition epoch)` → facts are from a
   superseded epoch → **`Stale`** (swap-without-reload).
4. **Residency:** defining partition non-resident → **`Partial` + `missing=[partition]`** (retained
   facts served as last-good; never silent-empty).
5. **Ownership (D2):** owned iff the fact's basis is `SymbolOwnership`-complete (derived via
   `classify_answer(SymbolOwnership, [basis], Fresh, [])` == `Exact`).
   - **Owned + Fresh → `Exact`** (symbol-owned; value + basis + provenance).
   - **Owned + `PrecisionPending`** and basis is AST-derived (not SCIP-backed): **`Exact` via
     `exact_precision_pending` + `NotScipDependent` proof** — complexity is AST-local, so the
     invariant-6 path `callers`/`callees` avoid is admissible here. (SCIP-backed owned basis →
     `Partial` + `PrecisionPending`.)
   - **Owned + `Stale`/`RefreshFailed`/epoch-mismatch → `Stale`** (last-good).
   - **NOT owned (basis not `SymbolOwnership`-complete, e.g. `ScipSynthesized`) → `Partial`** + the
     ownership degraded to raw-anchored + `DegradationReason` (`ScipFallbackIdentity` for
     `ScipSynthesized`). **The value is preserved; NEVER `Exact`-owned.**
6. **Multi-language:** `contributing_languages` preserved (the defining partition's language; a union
   when future facts span partitions).

Micro-decisions recorded: (i) non-resident defining partition → `Partial`+`missing` (serve retained
last-good), not `Unavailable`; (ii) the non-owned reason is `ScipFallbackIdentity` for
`ScipSynthesized` (the only non-complete basis reachable for TS complexity; `AstFileScope` carries no
function-complexity fact); (iii) the value-fact batch shares one load epoch (per-batch, not per-fact)
since `load_value_facts` is atomic for the current epoch.

## Acceptance tests (ratified) — EXECUTED, all green

```text
symbol_owned_complexity_exact_for_ast_adopted_ts
raw_anchored_complexity_partial_not_exact_for_symbol_ownership
missing_value_fact_unavailable_not_empty
value_fact_epoch_mismatch_stale_or_precision_pending
partition_swap_without_value_reload_marks_value_facts_stale
nonresident_partition_value_facts_partial_or_unavailable
contributing_languages_preserved
```
Plus the criteria: attached fact carries `IdentityBasis` + provenance; raw-anchored cannot be `Exact`
symbol-owned; missing → `Unavailable` not empty; stale/refresh propagates; AST-only fact `Exact`
under `PrecisionPending`; multi-language `contributing_languages` preserved.

## Commit scope (as built)

```text
Touched: repo-graph-livegraph (value-fact support) + docs. NOT repo-graph-scip-ingest, NOT repo-graph-ir.
repo-graph-trust-model UNCHANGED this slice → no non-building-commit risk.
```
**Dep-direction finding (why scip-ingest was NOT touched):** the real complexity→`ValueFact` adapter
cannot live in `repo-graph-livegraph` (deps are `repo-graph-ir` + `repo-graph-trust-model` only — no
scip-ingest, LIVEGRAPH-RUNTIME-1 D4) nor in `repo-graph-scip-ingest` (it would couple ingest to the
runtime — wrong dependency direction). It belongs in the wiring/Main layer that holds BOTH deps (Stage
D integration). `scip-ingest` already exposes `complexity` as a `pub` field, so nothing there needs
changing. The headless support (types + `load_value_facts` + `value_facts` + epoch coherence) is
proven with hand-built `ValueFact` fixtures; the real-data adapter is the Stage-D wiring step.

## Out of scope (hard guardrails)

```text
No CLI wiring. No warm-cache / persistence. No value-join into callers/callees.
No new extractor expansion unless a proven join defect requires it.
No path/cycles. No C/C++ (VALUE-JOIN-CXX-1). No Rust value join. No repo-graph-ir change.
value_facts retrieves Symbol-subject facts; RawAnchor-subject facts are stored (D3) but range-keyed
retrieval (value_facts_at(file,range)) is a follow-up.
```

## Definition of done

- `load_value_facts` + `value_facts(symbol)` on `repo-graph-livegraph`, trust-labelled via
  `repo-graph-trust-model`, epoch-bound (D7).
- All 7 acceptance tests green; clippy `-D warnings` clean; fmt clean; workspace builds.
- Headless only; `repo-graph-ir` untouched; no CLI/storage/warm-cache.

## Exit criterion

Value facts attach to canonical identities where the basis proves ownership, degrade to honest
raw-anchored facts otherwise, are epoch-bound, and are exposed trust-labelled through the headless
API — the key semantic rule enforced. Stage D can build on a runtime carrying graph + value facts.

## References
- `docs/slices/ingest-core-1.md` (the complexity value fact + canonical-key attachment)
- `docs/slices/trust-model-rebase-1.md` (`IdentityBasis`, `SymbolOwnership` completeness, `NotScipDependent`)
- `docs/slices/query-migration-1.md` (headless API + `contributing_languages`)
- `docs/slices/cjoin-prove-2.md` (C/C++ guard → `RangeNameConfirmed`/`RawAnchored`; VALUE-JOIN-CXX-1)
- `docs/slices/rust-ingest-prove-1.md` (Rust ScipSynthesized-dominant → value facts absent/RawAnchored)
