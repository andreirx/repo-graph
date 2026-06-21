# WARM-CACHE-DAEMON-WIRING-1: Wire the Warm Cache into the Daemon Refresh Path (Stage D)

Slice ID: WARM-CACHE-DAEMON-WIRING-1
Status: **IMPLEMENTED + live-validated (2026-06-01).** Commit 1 support `feed_partition_ir` (`103f76b`);
commit 2 daemon wiring (`ef98a92`). All 6 acceptance checks pass on the synthetic fixture (see
Completion). Next: WARM-CACHE-VALUEFACTS-1.
Depends: WARM-CACHE-1 (`repo-graph-warm-cache`, incl. the `repo_graph_version` key fix `7b0eb4c`),
LIVEGRAPH-INTEGRATION-1C (daemon-owned synchronous SCIP refresh: `run_refresh`,
`compute_build_inputs_hash`, `discover_scip_typescript`, `feed_partition`), PARTITIONED-WARM-CACHE-ARCH-1
(the authority/validation invariants).
Track: Stage D, immediately after WARM-CACHE-1; gates WARM-CACHE-VALUEFACTS-1.

## Purpose

```text
Wire repo-graph-warm-cache into the daemon SCIP refresh path from LIVEGRAPH-INTEGRATION-1C.
```

On a refresh, consult a validated partition cache BEFORE running the multi-second SCIP producer; on a
hit, warm-load the graph and skip the producer; on a miss/mismatch/corruption, run the producer exactly
as today and write a fresh cache afterward. The cache is a non-authoritative accelerator (it only saves
the indexer-bound producer run — DATAFLOW §5); it never becomes the source of truth and is always safe
to delete.

## Ratified decisions (2026-06-01)

### D1 — cache location: repo-local `.rgr/warm-cache/`
```text
<repo>/.rgr/warm-cache/<partition_id>.cache     (partition cache; one file per partition)
```
Reason: the warm cache is explicitly disposable and repo-state-derived; repo-local makes deletion,
inspection, and the trust boundary obvious. **Requirement:** `.rgr/` must be gitignored. Verify the
existing ignore (the daemon already writes `.rgr/livegraph-compare/`); add a single entry only if
absent. **No broad new ignore churn.**

### D2 — ValueFacts sidecar: NOT in this slice
Partition cache only. No `ValueFact ↔ CacheValueFactDto` conversion, no sidecar read/write here.
```text
Recorded: after a graph-only warm load, value_facts are Unavailable/Stale until a refresh recomputes
them. Acceptable — graph queries (callers/callees) are this slice's target. (Matches ARCH D7: the
sidecar is OPTIONAL for serving graph queries.)
```
Next slice: **WARM-CACHE-VALUEFACTS-1** (the `ValueFact ↔ CacheValueFactDto` conversion + sidecar
read/write + the feed-API shape for value facts).

### D3 — warm-load trigger: lazy, on `livegraph_refresh` (no eager daemon-start scan)
```text
livegraph_refresh(partition):
  compute build_inputs_hash                       (1C compute_build_inputs_hash — no producer run)
  construct expected CacheKey
  try read_validated + decode_partition(.rgr/warm-cache/<partition_id>.cache, expected_key)
  if HIT:
    feed the decoded PartitionIr into LiveGraph   (graph-only; value_facts empty/Unavailable)
    skip the producer
    return { warmed_from_cache: true, ... }
  if MISS / MISMATCH / CORRUPTION:
    run the producer                              (existing 1C path, unchanged)
    decode_index + ingest_partition
    feed LiveGraph                                (existing feed_partition, with value facts)
    best-effort write the cache                   (after feed; outside the write lock; failure non-fatal)
    return { warmed_from_cache: false, ... }
```
Reason: the daemon discovers partitions per request; a boot-time warm load needs a partition inventory
that does not exist yet. Lazy lands the cache check inside the existing `run_refresh` entrypoint.

### Decide-and-record (ratified; mechanism settled during build)
- **CacheKey construction:** `repo_uid` (from `resolve_and_load_repo`), `partition_id`,
  `build_inputs_hash` (1C `compute_build_inputs_hash(project_dir, producer)`), `indexer_name` +
  `indexer_version` (the discovered producer), `repo_graph_version` (the runtime crate version).
  `created_at` = daemon clock at write time (manifest metadata, not identity).
- **Write timing:** after successful ingest/feed; outside the LiveGraph write lock; best-effort; a
  failure is logged/structured but does NOT fail the refresh (serving the fresh in-memory graph wins).
- **Feed decoded PartitionIr:** add `feed_partition_ir` (or equivalent) to `repo-graph-livegraph-feed`
  — builds the same resident graph + xref as `feed_partition` from a `PartitionIr`, with `value_facts`
  empty (Unavailable). No `IngestOutcome` required for a cache hit.
- **No eviction:** stale entries are inert (key mismatch → ignored); disk accumulation is recorded
  tech debt (see Tech debt).

## Architecture / dependency edges (must stay a DAG)

```text
NEW edge:  daemon-runtime → repo-graph-warm-cache → repo-graph-ir
```
DAG-safe: `repo-graph-warm-cache` depends only on `repo-graph-ir`; it does not depend on daemon-runtime,
livegraph, livegraph-feed, or scip-ingest, so no cycle. `feed_partition_ir` is a SUPPORT addition to
`repo-graph-livegraph-feed` (already a daemon-runtime dependency). Definition of done = SUPPORT
(`feed_partition_ir`) + IMPLEMENTATION (daemon refresh-path wiring) + validation.

## Spec requirements (must hold — explicit)

```text
- no value-facts sidecar wiring
- no daemon-start eager load
- no cache eviction
- no SQLite decommission
- cache miss must preserve the current producer path (1C) unchanged
- cache corruption / mismatch must NOT crash the daemon (treated as a miss → producer)
- cache hit must avoid producer execution
```

## Acceptance (the definition of done is these six, EXECUTED)

```text
1. first livegraph_refresh with no cache runs the producer and writes the cache
2. second livegraph_refresh with the same build_inputs_hash loads the cache and skips the producer
3. a corrupt cache file falls back to the producer (no crash)
4. a wrong build_inputs_hash / schema_version falls back to the producer (no crash)
5. `rmap ... --engine livegraph` (callers/callees) works after a cache hit
6. the default SQLite path is unchanged
```

## Out of scope (hard guardrails)

```text
No ValueFacts sidecar (WARM-CACHE-VALUEFACTS-1). No eager daemon-start warm load. No cache eviction /
GC. No SQLite decommission. No async/non-blocking refresh (DAEMON-ASYNC-REFRESH-1). No new query
semantics. No rgr (TypeScript) CLI changes. No changes to repo-graph-warm-cache (WARM-CACHE-1 is done).
The cache is NEVER authoritative and is always safe to delete.
```

## Files likely in scope (confirm during build)

```text
rust/crates/repo-graph-livegraph-feed/src/lib.rs   feed_partition_ir (support: graph-only feed)
rust/crates/daemon-runtime/Cargo.toml              + repo-graph-warm-cache dependency
rust/crates/daemon-runtime/src/livegraph_refresh.rs  cache try (before producer) + write (after feed) +
                                                     CacheKey construction + .rgr/warm-cache path
rust/crates/daemon-runtime/src/dispatch.rs         surface warmed_from_cache in the refresh response
.gitignore                                          verify .rgr/ ignored (single entry only if absent)
```
If `livegraph_refresh.rs` approaches the 500-line structural guardrail, extract a `warm_cache` helper
module (path + key construction + try/write) rather than appending — decide during build.

Out of scope files: repo-graph-warm-cache (done), scip-ingest, storage/SQLite, rgr.

## Tech debt recorded
```text
- No eviction: stale .rgr/warm-cache entries accumulate on disk (inert; ignored by key mismatch).
  Future: a GC/eviction slice or a bounded cache dir.
- Graph-only warm load leaves value_facts Unavailable/Stale until a refresh recomputes — addressed by
  WARM-CACHE-VALUEFACTS-1.
```

## Proposed commit structure (confirm)
```text
1. support: Add feed_partition_ir graph-only LiveGraph feed entrypoint   (repo-graph-livegraph-feed)
2. impl:    Wire warm cache into daemon SCIP refresh (lazy cache-before-producer)   (daemon-runtime [+ .gitignore])
```

## References
- `docs/slices/warm-cache-1.md` (the support crate; `CacheKey`/`CacheManifest`/`read_validated`/`decode_partition`/`atomic_write`)
- `docs/slices/partitioned-warm-cache-arch-1.md` (D3 key identity refinement; D5 atomic write; D6 refresh interaction; D7 sidecar independence)
- `docs/slices/livegraph-integration-1c.md` (`run_refresh`, `compute_build_inputs_hash`, `feed_partition`)
- `docs/architecture/dataflow-hotpath-map.md` (§5 indexer-bound; §9 warm-cache implications)

## Completion (live-validated 2026-06-01, EXECUTED)

Commits: `103f76b` (support `feed_partition_ir`) + `ef98a92` (daemon wiring). Validated against the
synthetic fixture (`rust/crates/repo-graph-scip-ingest/tests/fixtures/synthetic`) via the installed
daemon (v0.2.1) with the Node-18 producer wrapper:

```text
1. cleared .rgr/warm-cache
2. rmap dev livegraph-refresh         -> warmed_from_cache=false, producer ran, default.cache (9288B) created
3. rmap dev livegraph-refresh (again) -> warmed_from_cache=true, producer SKIPPED (0.177s vs ~2.4s), value_facts=0
4. rmap callers makeCircle --engine livegraph -> report (served from the cache-populated LiveGraph)
5. truncate default.cache to 7 bytes; refresh -> warmed_from_cache=false, producer fallback (2.436s), no crash
6. rmap callers makeCircle / callees report (default sqlite) -> unchanged (report; Circle.describe + makeCircle)
```

Tests: 71 daemon-runtime lib tests (incl. 3 new `livegraph_warm_cache`) + the warm-cache crate's 10 +
`feed_partition_ir` (live) green; `clippy -D warnings` clean; `cargo fmt --all --check` clean.

Recorded notes: the `PartitionIr` is cloned before the feed so the cache write is strictly after the
swap (cold producer path only). A cache HIT still runs producer DISCOVERY (the `build_inputs_hash`
embeds producer identity); only producer EXECUTION is skipped — serving purely from cache with an
absent producer is out of scope (would need hash/producer decoupling). No eviction: stale entries are
inert (KeyMismatch) but accumulate on disk (tech debt).

## Known limitation (producer-absent warm start)

```text
Known limitation:
Current warm-cache lookup still requires producer discovery because CacheKey includes indexer
identity/version. A valid cache cannot yet be used when the producer is absent. This preserves strict
coherence but limits warm-start usefulness in producer-unavailable environments.

Follow-up:
WARM-CACHE-PRODUCER-ABSENT-1 — decide whether a cache may be trusted from manifest + source hash when
the producer binary is absent, and what trust/freshness label it must carry.
```

This is NOT solved in this slice. "Warm cache works even when the producer is unavailable" is NOT
proven and must not be implied: strict key validation (incl. producer identity) is correct, but it
means a hit currently depends on producer discovery succeeding.
