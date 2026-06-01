# DATAFLOW-HOTPATH-MAP-1: Data Shapes & Hot Paths as Architecture Evidence

Slice ID: DATAFLOW-HOTPATH-MAP-1
Status: **CHARTER — DEFERRED. Do NOT run yet** (run before any warm-cache/decommission work, or
earlier if LIVEGRAPH-INTEGRATION-1B/1C surfaces data-shape confusion).
Track: Stage D, **inserted before PARTITIONED-WARM-CACHE-ARCH-1**.

## Purpose

Map the actual **data shapes and hot paths** end to end: source code → producer facts (AST / SCIP /
build maps) → canonical identity → `PartitionIr` → value facts → LiveGraph → `AnswerEnvelope` →
optional persistence. **This is architecture evidence, not a perf concern** — warm-cache format,
daemon integration, query migration, and raw decommission all depend on it. Without it, cache-format
decisions drift into "serialize whatever exists," recreating the SQLite mistake in binary form.

## Why before warm cache

Warm-cache design must decide WHAT to serialize: `PartitionIr`? xref? alias index? value facts? epoch
metadata? provenance? all of it? That decision requires the authority/rebuildability/epoch map below.

## What it must map (pipeline)

```text
source files
  -> producer substrate: tree-sitter AST | SCIP index | build/declaration maps
  -> extracted facts: symbols, refs, calls, boundaries, metrics, aliases
  -> canonical identity: CanonicalKey, IdentityBasis, alias provenance
  -> PartitionIr: nodes, edges, partition metadata
  -> value facts: complexity (now); boundaries/resources/contracts (future); RawAnchor vs Symbol ownership
  -> LiveGraph: resident partitions, xref summaries, epochs, value-fact side channel
  -> trust-labelled answers: AnswerEnvelope, AnswerClass, FreshnessState, contributing_languages
  -> persistence / warm cache: authoritative vs rebuildable vs epoch-bound
```

## Concrete questions it must answer

1. **Authoritative source of each fact** — source code / build config / SCIP / AST extractor /
   declaration map / user authority (A1).
2. **What is rebuildable** — nodes/edges, value facts, aliases, xref, answer cache.
3. **What must be persisted durably** — A1 user authority, waivers/declarations, maybe
   manifests/provenance; NOT raw derived facts unless warm-cache.
4. **What must be kept in memory** — current partition IR, xref, epochs, residency state, value-fact
   batches.
5. **Recompute cadence** — per refresh: ingest, aliases, xref, value facts. Per query: answer
   classification, residency/freshness labelling. NEVER per query: parse/index.
6. **Hot paths** — SCIP indexing, scip-ingest IR construction, xref construction, alias reconciliation,
   LiveGraph lookup, SQLite old-path comparison, future warm-cache load.
7. **Copy/allocation points** — SCIP decode, IR node/edge allocation, value-fact conversion, xref
   summary, answer-payload allocation.
8. **Data shapes at each boundary** — table-driven.

## Deliverable shape

```text
docs/architecture/dataflow-hotpath-map.md          # tracked architecture artifact
docs/audits/dataflow-hotpath-map-1/findings.md     # local if audits remain gitignored
```

The tracked architecture doc must include: pipeline diagram; table of data shapes per boundary; table
of authority/rebuildability/epoch-binding; hot-path timing evidence from prior probes (REFRESH-PROBE-1,
CJOIN/XPART/RUST-INGEST audits, hot-path-analysis.md); known copy/allocation risks; **implications for
warm-cache design** (what to serialize, what to rebuild, what is epoch-bound).

## Ordering (ratified)

```text
LIVEGRAPH-INTEGRATION-1B → LIVEGRAPH-INTEGRATION-1C → DATAFLOW-HOTPATH-MAP-1
→ PARTITIONED-WARM-CACHE-ARCH-1 → WARM-CACHE-1 → RAW-DECOMMISSION
```

Pull earlier if 1B/1C surfaces data-shape confusion.

## References
- `docs/hot-path-analysis.md`, `docs/slices/refresh-probe-1.md` (prior hot-path/timing evidence)
- `docs/slices/ingest-core-1.md` (`PartitionIr` / `IngestOutcome` shapes)
- `docs/slices/{trust-model-rebase-1,value-join-1}.md` (`AnswerEnvelope` / `ValueFact` shapes)
- `docs/architecture/scip-migration-plan.md` (Stage D ordering)
