# WARM-CACHE-VALUEFACTS-1: Value-Facts Sidecar Warm Start (Stage D)

Slice ID: WARM-CACHE-VALUEFACTS-1
Status: **DESIGN — decisions SURFACED, awaiting ratification. Implementation NOT started.**
Depends: WARM-CACHE-DAEMON-WIRING-1 (graph warm load; `feed_partition_ir`; `livegraph_warm_cache`
seam; `ef98a92`), WARM-CACHE-1 (`CacheValueFactDto`, `encode_value_facts`/`decode_value_facts`,
sidecar-independence rule), VALUE-JOIN-1 (`ValueFact` model; epoch-bound value facts D7).
Track: Stage D, immediately after WARM-CACHE-DAEMON-WIRING-1.

## Purpose

Restore VALUE FACTS on a warm start so a cache hit yields the FULL runtime (graph + value facts), not
graph-only. Today a graph-only warm load leaves `value_facts` Unavailable until a producer refresh
recomputes them (WARM-CACHE-DAEMON-WIRING-1 D2). This slice adds the ValueFacts SIDECAR: write it after
a producer refresh, read it on a cache hit, and feed value facts into the LiveGraph epoch-coherently.

## Invariants (must hold regardless of mechanism)

```text
- Partition-cache validity and ValueFacts-sidecar validity are INDEPENDENT (WARM-CACHE-1 / ARCH D7).
  A missing/invalid sidecar NEVER blocks the graph hit — value_facts simply stay Unavailable.
- The sidecar is keyed by the SAME CacheKey as the partition (same build_inputs_hash) — value facts and
  graph topology come from the same producer run.
- Value facts fed on warm load are epoch-stamped to the JUST-loaded partition epoch (D7). A later swap
  without reload makes them detectably Stale — never silently attached to a new epoch.
- The ValueFact <-> CacheValueFactDto conversion is TOTAL and semantic-round-trip-tested.
- Do NOT fake value facts: only feed facts actually decoded from a valid sidecar.
```

## Open decisions (RATIFY before implementation)

### D1 — where does the `ValueFact <-> CacheValueFactDto` conversion live? (dependency-edge boundary)

The conversion must name BOTH `repo_graph_livegraph::ValueFact` and
`repo_graph_warm_cache::CacheValueFactDto` (+ `repo_graph_trust_model::IdentityBasis`). The warm-cache
crate deliberately omits it (no LiveGraph/trust-model dep). So it lives in a layer that has both.

| Option | Location | Trade-off |
|---|---|---|
| (a) daemon-runtime module | extend `livegraph_warm_cache` (or a sibling) | simplest, no new crate; daemon-runtime already deps livegraph + warm-cache + trust-model; cost: conversion logic in the wiring layer (an adapter concern leaking into the daemon) |
| **(b) new `repo-graph-warm-cache-feed` adapter crate** | mirrors `repo-graph-livegraph-feed` (the "depends on BOTH runtime + X" adapter) | cleanest separation; owns ValueFact<->DTO + the warm-load feed; round-trip-testable in isolation; reusable; daemon stays thin (wires, does not convert); cost: one new crate |
| (c) extend `repo-graph-livegraph-feed` | it already builds `ValueFact`s (`value_facts_of`) | co-locates value-fact logic; cost: livegraph-feed's purpose is ingest→runtime; adding warm-cache mixes a second concern (CCP) + a new dep edge feed→warm-cache |

Lean: **(b)** — matches the established both-depending-adapter pattern (`livegraph-feed`), keeps the
conversion out of the daemon and out of the ingest adapter, and is independently round-trip-testable.
(a) is the pragmatic no-new-crate alternative if you prefer fewer crates.

### D2 — warm-load feed API for value facts

On a cache hit: `feed_partition_ir` loads the graph (epoch N); restoring facts = `load_value_facts(id,
facts)` which stamps the CURRENT epoch (N) → epoch-coherent by construction.

| Option | Shape | Trade-off |
|---|---|---|
| (a) two calls in the daemon | `feed_partition_ir(...)` then `load_value_facts(id, facts)` | reuses existing APIs; cost: two-step, daemon must order them + own the DTO→ValueFact conversion inline |
| **(b) combined feed entrypoint** | `feed_partition_ir_with_value_facts(lg, id, ir, facts, language)` in the D1 adapter (graph + facts, one atomic epoch) | one call, epoch-coherent + graph-only-fallback obvious; lives with the conversion (D1); cost: a new entrypoint |

Lean: **(b)**, in whichever crate D1 selects.

### D3 — sidecar path + key

| Field | Proposal |
|---|---|
| path | `<project_dir>/.rgr/warm-cache/<partition_id>.vf` (sibling of `<partition_id>.cache`) |
| key | the SAME `CacheKey` as the partition (same `build_inputs_hash`) — invariant above |
| codec | `encode_value_facts` / `decode_value_facts` (already in `repo-graph-warm-cache`) |

Lean: as proposed. (Alternative path suffix `.valuefacts` — cosmetic.)

### D4 — write / read timing + independence

```text
Write (after a producer refresh, value facts available from IngestOutcome.complexity -> ValueFact):
  best-effort write the partition cache AND the sidecar, each independent + non-fatal.
Read (on a partition-cache HIT, after feed_partition_ir):
  best-effort try the sidecar; valid -> convert + load_value_facts; absent/invalid -> value_facts stay
  Unavailable (graph-only, the WIRING-1 behavior). A sidecar failure NEVER blocks the graph hit.
```
Lean: as written (D7 independence already proven in WARM-CACHE-1).

### D5 — conversion completeness (implementation + tests)

Total field/variant mapping: `ValueSubject{Symbol,RawAnchor}` <-> `CacheValueSubjectDto`;
`ValueFactKind` <-> `CacheValueFactKindDto`; `IdentityBasis` (7) <-> `CacheIdentityBasisDto` (7);
`SourceRange`/`Provenance` <-> the cache DTOs (already mirrored). Required: a semantic round-trip test
`value_fact_roundtrip_preserves_semantics` (ValueFact -> DTO -> ValueFact equal) for all subject/basis
variants. Lean: as written.

## Acceptance (proposed — confirm at ratification)

```text
1. producer refresh writes BOTH <partition>.cache and <partition>.vf
2. warm refresh (cache hit) restores graph AND value facts: value_facts query returns the complexity
   fact (Exact/owned where applicable), NOT Unavailable
3. absent sidecar (delete .vf only) -> graph hit still works; value_facts Unavailable (independence)
4. corrupt sidecar (truncate .vf) -> graph hit still works; value_facts Unavailable; no crash
5. wrong-key sidecar (stale build_inputs_hash) -> rejected; value_facts Unavailable
6. value_fact_roundtrip_preserves_semantics (unit) green
```

## Out of scope
```text
No change to graph warm load (WIRING-1). No producer-absent cache use (WARM-CACHE-PRODUCER-ABSENT-1).
No eviction. No SQLite decommission. No new value-fact KINDS (complexity only, per VALUE-JOIN-1 D1).
```

## Commit structure (proposed — confirm)
```text
1. support: ValueFact <-> CacheValueFactDto conversion + feed_partition_ir_with_value_facts (D1 crate)
2. impl:    daemon sidecar write (after refresh) + read (on hit) in livegraph_warm_cache/run_refresh
```

## References
- `docs/slices/warm-cache-daemon-wiring-1.md` (graph warm load; the seam this extends)
- `docs/slices/warm-cache-1.md` (`CacheValueFactDto`, `encode_value_facts`/`decode_value_facts`, D7 independence)
- `docs/slices/value-join-1.md` (`ValueFact` model; epoch-bound value facts D7)
