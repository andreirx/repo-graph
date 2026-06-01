# LIVEGRAPH-INTEGRATION-1A: Real-Data Wiring (Stage D, integration — sub-slice 1A)

Slice ID: LIVEGRAPH-INTEGRATION-1A
Status: **DESIGN RATIFIED (D1=(a) committed real-index fixture, single partition) — building.**
Depends: VALUE-JOIN-1 + QUERY-MIGRATION-1 (`repo-graph-livegraph`), INGEST-CORE-1
(`repo-graph-scip-ingest`), `repo-graph-ir`, `repo-graph-trust-model`.
Track: Extraction Substrate Pivot — **Stage D integration**, sub-slice **1A** (real-data wiring;
1B = shipped query-surface wiring, separate, after 1A).

## Purpose / the one risk this slice retires

Prove the **non-hand-built** ingestion→runtime path in CI:

```text
committed index.scip (real scip-typescript output) -> repo-graph-scip-ingest -> PartitionIr + complexity map
  -> [repo-graph-livegraph-feed adapter] -> LiveGraph.load_partition + load_value_facts
  -> headless callers / callees / value_facts -> trust-labelled answers
```

## Claim discipline (exact, ratified)

```text
hand-built fixtures        = NOT enough (Stage C proved the runtime this way only)
committed real SCIP fixture = ENOUGH for 1A (real producer output, ingested, not a hand-built PartitionIr)
real multi-partition repo  = LATER (not proven by 1A)
```

- CLAIM: "1A proves real SCIP-ingest output flows into LiveGraph using a committed real-index fixture;
  it proves the non-hand-built ingestion→runtime path in CI."
- DO NOT CLAIM: "proves the runtime against an actual indexed repo." The committed synthetic index is
  real producer output but still a single-partition fixture.

## Dependency-direction invariant (non-negotiable)

```text
The adapter is a NEW outer crate depending on BOTH repo-graph-scip-ingest AND repo-graph-livegraph.
repo-graph-livegraph MUST NOT depend on repo-graph-scip-ingest (LIVEGRAPH-RUNTIME-1 D4).
repo-graph-scip-ingest MUST NOT depend on the runtime.
```

## Grounding (verified — the adapter is thin, no producer change)

- `repo-graph-scip-ingest::ingest_partition(...) -> IngestOutcome { ir: PartitionIr, complexity:
  BTreeMap<canonical_key, u32>, .. }`. The complexity map is already `pub` → **no scip-ingest change.**
- Committed self-contained real index: `repo-graph-scip-ingest/tests/fixtures/synthetic/`
  (`index.scip` + `src/` + `tsconfig`). INGEST-CORE-1's harness already ingests it and asserts
  cross-file edges + complexity attach → callers/callees + `value_facts` are exercisable in CI.

## Ratified decisions

**D1 — cross-partition real-data provenance: (a).** Use the committed `synthetic/index.scip`. Single
partition is acceptable. **Cross-partition real-data integration is DEFERRED** (see Residual).

**D2 — adapter crate.** New library crate **`repo-graph-livegraph-feed`** (path
`rust/crates/repo-graph-livegraph-feed`), deps `repo-graph-scip-ingest` + `repo-graph-livegraph` +
`repo-graph-ir` (+ `repo-graph-trust-model` for `IdentityBasis`/`LanguageSupport`). The seam 1B's
daemon wiring will call.

**D3 — adapter mechanism.**

```text
feed_partition(lg: &mut LiveGraph, id, outcome: IngestOutcome, language):
  1. value_facts = for (key, cx) in outcome.complexity:
       node = outcome.ir.nodes.find(|n| n.key == key)            // join by canonical key
       ValueFact { subject: Symbol(key), kind: CyclomaticComplexity, value: cx,
                   basis: basis_from_source(node.identity_source), // mapping owned by the adapter
                   source_range: node.range, provenance: node.provenance }
  2. lg.load_partition(id, outcome.ir, language)   // build value_facts BEFORE moving ir
  3. lg.load_value_facts(id, value_facts)          // epoch-stamped (D7)
```

The `IdentitySource -> IdentityBasis` mapping (`AstAdopted→AstAdopted`,
`ScipSynthesizedFallback→ScipSynthesized`, `AstFileScope→AstFileScope`) is **replicated privately in
the adapter** — it is the ingestion→trust-vocabulary translation, which belongs in this integration
layer. `repo-graph-livegraph` is **NOT touched** (its internal `basis_of` is its own xref concern; the
closed 3-variant enum makes any divergence a compile error at both sites). `AstAdopted` complexity →
owned (`value_facts` Exact); `ScipSynthesizedFallback` → not owned (Partial) — trust labels fall out
of VALUE-JOIN-1 unchanged.

## Acceptance (ratified)

```text
- ingest committed synthetic/index.scip through repo-graph-scip-ingest
- feed resulting PartitionIr into LiveGraph (committed real SCIP fixture partition, NOT hand-built)
- feed complexity map as ValueFact
- value_facts(symbol) returns trust-labelled complexity for a real ingested symbol
- callers/callees over real ingested edges return trust-labelled answers IF the fixture has suitable edges
- contributing_languages populated ({TypeScriptPrimary})
- value facts epoch-bound (load → swap-without-reload → Stale)
- no hand-built PartitionIr in the acceptance tests
- no production CLI/daemon behavior changed (headless crate only)
```

## Residual (named)

```text
Residual: real MULTI-PARTITION integration is NOT proven by 1A (single committed partition; callers/
callees are intra-partition; cross-partition resolution stays fixture-tested).
Upgrade: LIVEGRAPH-INTEGRATION-XPART-1 — committed two-partition SCIP fixtures, OR daemon-held real
repo partitions (may be absorbed by 1B).
```

## Commit scope

```text
New: repo-graph-livegraph-feed (adapter + integration test on the committed real index.scip) + docs.
NOT: repo-graph-livegraph, repo-graph-ir, repo-graph-scip-ingest, daemon, CLI, SQLite, warm cache.
Each commit builds.
```

## Definition of done

- `repo-graph-livegraph-feed::feed_partition` loads a real ingested partition + its complexity value
  facts into `LiveGraph`; an integration test ingests the committed `synthetic/index.scip` and asserts
  the acceptance on REAL (ingested, non-hand-built) data; clippy/fmt clean; workspace builds.
- The Residual is recorded; no claim of multi-partition/real-repo proof.

## Exit criterion

Real SCIP-ingest output flows into LiveGraph and produces trust-labelled headless answers via a
committed real-index fixture — the non-hand-built path is proven in CI. 1B routes the shipped `rmap`
surfaces onto the same `feed_partition` seam; LIVEGRAPH-INTEGRATION-XPART-1 adds real multi-partition.

## References
- `docs/slices/value-join-1.md` (value facts + the dep-direction finding this realizes)
- `docs/slices/query-migration-1.md` (callers/callees + the 1B CLI step)
- `docs/slices/ingest-core-1.md` (`ingest_partition` / `IngestOutcome` / the synthetic fixture)
- `docs/architecture/scip-migration-plan.md` (Stage D)
