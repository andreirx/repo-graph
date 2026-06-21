# PARTITIONED-WARM-CACHE-ARCH-1: Warm-Cache Architecture (Stage D)

Slice ID: PARTITIONED-WARM-CACHE-ARCH-1
Status: **DESIGN RATIFIED (D1–D8, 2026-06-01) — architecture only. WARM-CACHE-1 is the build slice.**
Depends: DATAFLOW-HOTPATH-MAP-1 (serialize-PartitionIr-only; indexer-bound), LIVEGRAPH-INTEGRATION-1C
(real `build_inputs_hash`; the feed/swap path), VALUE-JOIN-1 (value facts are a separate channel),
INGEST-CORE-1 (`PartitionIr`).
Track: Stage D, after 1C; gates WARM-CACHE-1 (impl).

## Framing (the invariants the architecture must enforce)

```text
Warm cache is a NON-AUTHORITATIVE acceleration layer.
It MUST NOT become the source of truth.
It MUST be safe to delete (rebuildable by re-indexing).
It MUST be epoch/hash/provenance VALIDATED before load — never partially trusted.
```

Its only purpose: skip the multi-second SCIP re-index on warm start (the pipeline is indexer-bound —
DATAFLOW §5). Everything else (xref, value-fact placement, aliases, answer envelopes) rebuilds in ms
from the cached `PartitionIr` + current metadata, so it is NOT cached (DATAFLOW §9).

## Default (from DATAFLOW, already decided)

```text
Persist PartitionIr (the only expensive-to-reproduce artifact).
Rebuild xref, value facts placement, aliases, AnswerEnvelopes from PartitionIr / current metadata.
```

## D1 — serialized artifact

| Option | Cached | Trade-off |
|---|---|---|
| **(a) PartitionIr only (lean)** | nodes + edges + partition meta | smallest; xref/answers rebuild in ms; the expensive SCIP work is what's saved |
| (b) PartitionIr + xref | + `defines`/`ref_counts` | redundant — xref is ~21ms to rebuild (DATAFLOW §5); caching it duplicates derivable state |
| (c) full LiveGraph snapshot | resident IR + xref + epochs + value facts | largest; bakes in runtime state that should be reconstructed; closest to the SQLite "serialize everything" mistake |

*Lean: (a) PartitionIr only.* (Value facts are NOT in PartitionIr — see D7.)

## D2 — format (decide from MEASURED load needs, not vibes)

| Option | Property |
|---|---|
| rkyv | zero-copy load; fastest deserialize; stricter layout/versioning discipline |
| **bincode / postcard** | full deserialize; simplest + correct; portable |
| Cap'n Proto / FlatBuffers | zero-copy + schema; extra schema toolchain |
| embedded KV values | a store wrapping the value bytes; orthogonal to the value encoding |

**Principle: the architecture defines the VALIDATION ENVELOPE (D4) + write protocol (D5)
INDEPENDENT of the value format.** The format is swappable behind that envelope.
**Ratified: bincode first** (correctness + simple validation beats zero-copy complexity); rkyv remains
a later optimization ONLY if measured deserialize cost dominates. **The format is subordinate to the
validation envelope.**

## D3 — cache key (ratified fields)

```text
repo_uid | partition_id | build_inputs_hash | indexer_name | indexer_version | schema_version | repo_graph_version
```

`build_inputs_hash` is produced by 1C (real sha256 over sources + tsconfig/package.json/lockfile +
producer identity). `schema_version` = the cache format/layout version; `repo_graph_version` = the
runtime's version. A change in ANY field invalidates the entry.

**WARM-CACHE-1 refinement (ratified 2026-06-01).** The seven properties are realized as TWO distinct
identity axes with distinct diagnostics, not one flat key:
- `repo_graph_version` is a **`CacheKey` field** (producer/runtime identity; only the caller knows the
  expected value, so it must travel in the key) → mismatch = `KeyMismatch`.
- `schema_version` is a **crate-owned constant** (`SCHEMA_VERSION`) self-validated by
  `repo-graph-warm-cache`, NOT a key field (cache-format identity) → mismatch = `SchemaMismatch`.
- `created_at` is manifest **metadata only**, never part of identity.
A change in any of the seven still invalidates the entry; the split preserves specific diagnostics.

## D4 — validation before load (manifest header; never partially trust)

```text
magic | schema_version | producer versions (indexer_name+version) | build_inputs_hash
content length / checksum | partition_id | created_at
```

On ANY mismatch (magic, schema_version, versions, hash, checksum, partition_id): **discard the entry
and treat the partition as needing refresh (re-index).** No partial trust, no best-effort load.

## D5 — write protocol (atomic; crash-safe)

```text
write to a temp file in the same dir → fsync temp → atomic rename over the target → fsync parent dir if feasible
```

A crash during write leaves EITHER the old cache OR no cache — **never a corrupt entry that validation
would accept.** (Validation + atomic-rename together guarantee: an accepted entry is complete + matches.)

## D6 — interaction with refresh

```text
On daemon start (per partition):
  if a valid cache entry exists AND build_inputs_hash still matches the current inputs → LOAD it (warm).
  else → no warm state; the partition needs a SCIP refresh (1C) / is ProducerUnavailable until then.

On a successful SCIP refresh (1C):
  write the cache entry AFTER the PartitionIr is produced.
  the LiveGraph may swap before or after the cache write, BUT a cache-write failure MUST NOT block
  serving the fresh in-memory state (cache is acceleration, not correctness).
```

## D7 — value facts (FORCED — value facts are NOT inside PartitionIr)

`IngestOutcome` is `{ ir: PartitionIr, complexity: BTreeMap<key,u32>, .. }` — the value facts
(complexity) are a SEPARATE channel (VALUE-JOIN-1 D6), NOT inside `PartitionIr`. So a PartitionIr-only
cache **loses value facts on warm start** unless they are recomputed (needs the producer/source) or
cached separately.

| Option | Warm start restores | Cost |
|---|---|---|
| **(a) PartitionIr cache + ValueFacts SIDECAR under the same `build_inputs_hash` (lean)** | graph + value facts (full runtime), no producer run | a small second artifact + its own validation; value facts kept OUT of PartitionIr |
| (b) PartitionIr only; value facts recomputed | graph now; value facts only after a refresh runs the producer | warm start serves `value_facts` = `Unavailable` until a refresh (defeats warm-start for value facts) |

**Ratified: (a) PartitionIr cache + a ValueFacts sidecar under the same `build_inputs_hash`.** Value
facts are separate from graph topology by design — do NOT put them into `PartitionIr`, do NOT cache the
full LiveGraph. **Sidecar rule (independence):** the sidecar is OPTIONAL for serving graph queries. If
the PartitionIr cache is valid but the ValueFacts sidecar is absent/invalid → **load the graph;
`value_facts` returns `Unavailable`/`Stale` until recomputed.** A value-sidecar failure MUST NOT
invalidate the graph cache.

## D8 — serialization boundary (SURFACED: IR purity vs serde)

`repo-graph-ir` is **dependency-free by invariant** (INGEST-CORE-1 group10 asserts its `[dependencies]`
is empty — pure domain), so `PartitionIr` is NOT serde-serializable today. Serializing it forces:

| Option | Mechanism | Trade-off |
|---|---|---|
| (a) Optional `serde` on `repo-graph-ir` (feature-gated) | `#[cfg_attr(feature="serde", derive(...))]` on the IR types; relax group10 to allow an OPTIONAL serde dep | one feature-gated dep on the pure crate; least code; mirrors `repo-graph-trust-model`'s optional-serde pattern |
| (b) Cache-side mirror DTO | the cache crate defines serializable mirror structs + `From`/`Into` conversions | keeps `repo-graph-ir` strictly zero-dep; cost: mirror types + conversions to maintain in lockstep with the IR |

**Ratified: (b) cache-side mirror DTO + conversions. Do NOT add serde to `repo-graph-ir`** — it is the
pure domain graph artifact; serialization is infrastructure, and even an optional serde dep leaks
delivery-mechanism pressure into the domain crate (Clean Architecture > convenience; group10 stays).
Model:

```text
repo-graph-warm-cache  (depends on repo-graph-ir)
  CachePartitionIrDto:  impl From<&PartitionIr> for CachePartitionIrDto + impl TryFrom<CachePartitionIrDto> for PartitionIr
  CacheValueFactsDto:   the same for the value facts
```

The DTOs carry the serde derives + the manifest; `repo-graph-ir` stays zero-dep.

## Out of scope (hard guardrails)

```text
No implementation (WARM-CACHE-1). No SQLite decommission. No LiveGraph default. No async refresh
(DAEMON-ASYNC-REFRESH-1). No multi-repo orchestration. No new query semantics. The cache is NEVER
authoritative and is always safe to delete.
```

## Ratified (2026-06-01) + WARM-CACHE-1 build scope

D1 PartitionIr only (no xref). D2 bincode first (format subordinate to the validation envelope).
D3–D6 confirmed. D7 PartitionIr cache + ValueFacts sidecar (independent — a sidecar failure never
invalidates the graph cache). D8 cache-side mirror DTOs (NO serde in `repo-graph-ir`).

**Required: semantic round-trip tests** — `PartitionIr → CachePartitionIrDto → PartitionIr` and
`ValueFacts → CacheValueFactsDto → ValueFacts` must be equal.

**WARM-CACHE-1 (next — the build slice):**

```text
repo-graph-warm-cache support crate (depends on repo-graph-ir; pure, no daemon)
manifest validation (D4)
atomic write (D5)
CachePartitionIrDto round-trip (D8)
CacheValueFactsDto sidecar round-trip (D7/D8)
NO daemon wiring yet (unless separately ratified)
```

## References
- `docs/architecture/dataflow-hotpath-map.md` (§3 authority/rebuildability, §9 warm-cache implications)
- `docs/slices/livegraph-integration-1c.md` (real `build_inputs_hash`; the feed/swap path)
- `docs/slices/value-join-1.md` (value facts as a separate channel; the D7 reason)
- `docs/slices/ingest-core-1.md` (`PartitionIr` / `IngestOutcome` shapes; group10 zero-dep invariant)
