# WARM-CACHE-PRODUCER-ABSENT-1: Trust a Warm Cache When the Producer Is Absent (Stage D)

Slice ID: WARM-CACHE-PRODUCER-ABSENT-1
Status: **RATIFIED (2026-06-01, D1–D5) — implementing. See "Ratified" below; the matrices are the
rationale.**

## Ratified (2026-06-01)

- **D1 — C/B hybrid.** Split `source_inputs_hash` from `producer_fingerprint`; the manifest carries
  `producer_fingerprint` from the prior successful run; producer-absent load only when
  `source_inputs_hash` matches.
- **D2 — key split, de-duplicated.** `producer_fingerprint = { name, version }` REPLACES the separate
  `indexer_name`/`indexer_version` (same fact — do not store twice); `build_inputs_hash` becomes
  `source_inputs_hash`:
  ```text
  ProducerFingerprint { name, version }   // optional impl fingerprint later
  CacheKey {
    repo_uid, partition_id,
    source_inputs_hash,                    // source files + tsconfig + package.json + lockfile + build config; NOT producer
    producer_fingerprint: ProducerFingerprint,
    repo_graph_version,
  }
  ```
- **D3 — `producer_fingerprint` = logical name + version** (`scip-typescript` + `0.4.0`); NOT binary
  path/mtime. Record: when the producer exposes a reliable `--version`, replace the hardcoded version
  source with the command output.
- **D4 — degraded label, no new `FreshnessState`.** Producer-absent cache answers are
  `FreshnessState::Stale` + `AnswerClass::Stale` + a NEW `DegradationReason::ProducerUnavailable`
  (added to `repo-graph-trust-model`). NEVER `Fresh`/`Exact`.
- **D5 — flow (confirmed) + ambiguity guard.** If, producer-absent, multiple cache candidates match
  `source_inputs_hash` but disagree on `producer_fingerprint`, reject all / report
  `AmbiguousProducerFingerprint` — never pick arbitrarily. (The single-file-per-partition layout yields
  ≤1 candidate today; the guard is a total function over the candidate set, tested directly.)
- **Required acceptance tests:** `producer_absent_cache_hit_loads_stale_with_ProducerUnavailable`,
  `producer_absent_source_hash_mismatch_rejects_cache`,
  `producer_absent_multiple_matching_fingerprints_rejected_or_reported_ambiguous`,
  `producer_present_cache_hit_still_fresh`,
  `producer_present_reinstall_same_logical_version_does_not_invalidate`.
- **Commit split (approved):** (1) key split / warm-cache DTO update, (2) trust label, (3) daemon
  producer-absent flow — but combine dependent changes to avoid a non-building intermediate (the key
  split must leave the daemon building + producer-present behavior intact).
Depends: WARM-CACHE-DAEMON-WIRING-1 (graph warm load; `build_cache_key`; `compute_build_inputs_hash`),
WARM-CACHE-VALUEFACTS-1 (sidecar), WARM-CACHE-1 (`CacheKey`/`CacheManifest`), LIVEGRAPH-INTEGRATION-1C
(`run_refresh`, `discover_scip_typescript`, the `ProducerUnavailable` failure), the trust model
(`FreshnessState`, `AnswerEnvelope`).
Track: Stage D. Blocks nothing; does NOT gate raw decommission (which stays after this).

## Purpose

```text
Decide and implement whether a valid partition cache can be trusted when the producer binary is absent.
```

Restart resilience is the core value of a warm cache: after a daemon restart on a machine where
`scip-typescript` is not installed, a valid cache SHOULD be able to serve. Today it cannot.

## The blocking finding (data-shape: the hash mixes source + producer)

`compute_build_inputs_hash(project_dir, producer)` (1C) folds BOTH classes of input into one digest:

```text
SOURCE inputs   : tsconfig.json, package.json, package-lock.json, yarn.lock, pnpm-lock.yaml, + all .ts sources
PRODUCER identity: producer binary PATH string, producer file len + mtime, the literal "scip-typescript@0.4.0"
```

So `build_inputs_hash` **cannot be recomputed without the producer binary** (its path + file metadata
are inputs). The current cache key therefore CANNOT be validated producer-absent — we must not pretend
otherwise. A producer-absent warm load requires separating the source portion from the producer portion.

Additional smell (surface): the producer portion currently mixes LOGICAL identity (name + version) with
INSTALL-SPECIFIC metadata (path + mtime). Binary path/mtime change on reinstall WITHOUT any semantic
change, so they over-invalidate the cache and couple validity to an install location. A logical
fingerprint (name + version) is the stable identity that actually matters for cache coherence.

## Decision 1 — the outcome (FORCED)

| Option | Behavior when producer absent | Trade-off |
|---|---|---|
| A — strict (status quo) | no warm-cache load; `ProducerUnavailable` | safe, zero new trust surface; defeats restart resilience (the core value) — record as explicit design if chosen |
| B — manifest-trust via source hash | split the hash; if `source_inputs_hash` matches + the manifest carries a `producer_fingerprint`, load with DEGRADED freshness | restores resilience; needs a degraded trust label + the hash split |
| C — producer-metadata sidecar | cache stores enough producer fingerprint to validate without the binary; source hash recomputed independently | same capability as B, framed as "the cache is self-describing"; B already achieves this via the manifest |
| **(lean) C/B hybrid** | split `source_inputs_hash` from `producer_fingerprint`; the manifest stores `producer_fingerprint` from the prior successful run; producer-absent load allowed ONLY if `source_inputs_hash` matches; the answer carries a degraded `ProducerUnavailable` warm-cache trust label | restores resilience with honest degraded trust; the manifest IS the producer sidecar (no separate file); requires the key split (D2) + the trust label (D4) |

Recommend the **C/B hybrid**. B and C collapse into the same mechanism once the manifest carries the
`producer_fingerprint` (the manifest already does, via the key). Force a choice — this is the
trust/coherence boundary.

## Decision 2 — cache key / hash split (data-shape; FORCED)

Split the single `build_inputs_hash` into two independent digests; recompose the key:

```text
source_inputs_hash : sha256 over configs + .ts sources only  (recomputable WITHOUT the producer)
producer_fingerprint: the producer LOGICAL identity (indexer_name + indexer_version; see D3)
cache_key = source_inputs_hash + producer_fingerprint + schema_version + repo_graph_version
            (+ repo_uid + partition_id)
```

Producer PRESENT → recompute both, validate the full key (today's strictness).
Producer ABSENT → recompute `source_inputs_hash` only; take `producer_fingerprint` from the manifest;
if `source_inputs_hash` matches → load (degraded, D4). If it does not match → no load (inputs changed).

This is a change to `repo-graph-warm-cache::CacheKey` (replace `build_inputs_hash` with
`source_inputs_hash` + `producer_fingerprint`) + `compute_build_inputs_hash` (split) +
`build_cache_key`. It invalidates existing on-disk caches — acceptable (the cache is disposable).

## Decision 3 — `producer_fingerprint` definition (FORCED)

| Option | Fingerprint = | Trade-off |
|---|---|---|
| **(lean) logical identity** | `indexer_name` + `indexer_version` (e.g. `scip-typescript` + `0.4.0`) | stable across reinstalls/path changes; the identity that actually governs coherence; producer-absent validation uses the manifest's value directly |
| binary metadata | path + len + mtime (today) | over-invalidates on reinstall; couples validity to install location; cannot be reproduced producer-absent anyway |
| binary content hash | sha256 of the producer binary | exact, but requires reading the binary (present-only) and changes on every rebuild of the producer |

Recommend **logical identity** (name + version). The version is currently a hardcoded `"0.4.0"`
(scip-typescript@0.4.0 has no machine `--version`); record that as the version source until a producer
exposes one (PRODUCER-COMPAT follow-up).

## Decision 4 — freshness / trust label for a producer-absent load (FORCED — trust boundary)

The facts are REAL (extracted by a prior successful producer run) and the SOURCE inputs still match;
only the producer is currently unverifiable. This is degraded freshness, NOT `Unavailable` (the graph
IS loaded) and NOT `Fresh` (no live producer confirmation).

| Option | Label | Trade-off |
|---|---|---|
| **(lean) Stale + `DegradationReason::ProducerUnavailable`** | reuse `FreshnessState::Stale` + a new degradation reason | minimal vocabulary growth; `Stale` already means "serving last-good"; the reason names WHY; every freshness consumer already handles `Stale` |
| new `FreshnessState::ProducerUnavailableWarmCache` | a dedicated state | explicit + greppable; cost: expands the freshness axis — every consumer/classifier must handle the new variant; larger blast radius |

Recommend **Stale + a `ProducerUnavailable` degradation reason**. It honors the fact-certainty model
(real extracted facts, degraded freshness, reason-labeled) without enlarging the freshness vocabulary.
The cache load is NEVER presented as `Fresh`/`Exact` when the producer was not re-verified.

## Decision 5 — operational flow (FORCED)

```text
run_refresh:
  source_inputs_hash = compute_source_inputs_hash(project_dir)        # no producer needed
  match discover_scip_typescript():
    Ok(producer):
      producer_fingerprint = logical_fingerprint()                   # name + version
      key = source_inputs_hash + producer_fingerprint + schema + repo_graph_version
      try cache (key) -> hit: warm load Fresh (today's behavior); miss: producer -> ingest -> write cache
    Err(ProducerUnavailable):
      # NEW: do not fail immediately — try the cache producer-absent
      load the manifest for this partition (if any); key = source_inputs_hash + manifest.producer_fingerprint + schema + repo_graph_version
      if cache validates under that key -> warm load with Stale + ProducerUnavailable degradation
      else -> ProducerUnavailable (no usable cache)
```

The producer-absent load reads the manifest's `producer_fingerprint` (it cannot recompute it) and gates
ONLY on `source_inputs_hash` matching + schema/repo_graph_version. The served answers are degraded
(D4). Value-facts sidecar: same source-hash gate; same degraded label.

## Acceptance (proposed — confirm at ratification)

```text
1. producer present, source unchanged -> Fresh warm load (unchanged from VALUEFACTS-1)
2. producer ABSENT, valid cache, source unchanged -> warm load succeeds, freshness Stale +
   ProducerUnavailable reason (graph + value facts served, NOT Fresh)
3. producer absent, source CHANGED -> no load (source_inputs_hash mismatch) -> ProducerUnavailable
4. producer absent, no cache -> ProducerUnavailable (today's behavior)
5. producer returns -> next refresh recomputes the full key + runs the producer -> Fresh
6. unit: source_inputs_hash is independent of producer identity (changing only the producer path/version
   does NOT change source_inputs_hash); cache_key still changes when producer_fingerprint changes
```

## Out of scope (hard guardrails)

```text
No raw decommission (stays AFTER this slice). No eviction. No new producer (PRODUCER-COMPAT). No
producer auto-install / network. No multi-producer. The producer-absent load is ALWAYS degraded-trust,
never Fresh.
```

## Proposed commit structure (confirm)

```text
1. support: split CacheKey into source_inputs_hash + producer_fingerprint (repo-graph-warm-cache) +
            split compute_build_inputs_hash (compute_source_inputs_hash + logical_fingerprint)
2. trust:   add DegradationReason::ProducerUnavailable (or the ratified label) to the trust model
3. impl:    daemon producer-absent warm-load path in run_refresh + degraded answer labeling
```

## References
- `docs/slices/warm-cache-daemon-wiring-1.md` (the inherited limitation + Known limitation section)
- `docs/slices/warm-cache-1.md` (`CacheKey`/`CacheManifest` shapes)
- `docs/slices/livegraph-integration-1c.md` (`compute_build_inputs_hash`; `discover_scip_typescript`; `ProducerUnavailable`)
- `agent_docs/architecture.md` (fact-certainty model — freshness must not over-claim)
