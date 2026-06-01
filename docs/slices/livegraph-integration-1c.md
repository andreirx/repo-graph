# LIVEGRAPH-INTEGRATION-1C: Daemon-Owned Async SCIP Ingestion / Refresh (Stage D)

Slice ID: LIVEGRAPH-INTEGRATION-1C
Status: **DESIGN RATIFIED (D0–D6). Building per the order. HIGH blast radius (daemon subprocess +
background thread + refresh).** **Completion is GATED on the success path running** — which requires a
provisioned `scip-typescript` (absent here). Until then the slice can only be **PARTIAL**
(absent-producer path validated). Guided by `docs/architecture/dataflow-hotpath-map.md`.
Depends: LIVEGRAPH-INTEGRATION-1B, DATAFLOW-HOTPATH-MAP-1, REFRESH-PROBE-1.
Track: Stage D integration, **1C** (after 1B; before warm-cache).

## Framing (hard boundary)

```text
1C builds daemon SCIP ingestion / refresh orchestration.
It does NOT design warm cache. It does NOT decommission SQLite. It does NOT make LiveGraph default.
```

## Core objective

Replace the dev-only preload (ingest a SUPPLIED `index.scip`) with a **daemon-owned, async SCIP
production path** for supported TS partitions: the daemon runs the SCIP producer, ingests, feeds, and
swaps the partition — outside the query path, serving last-good meanwhile. The dev-preload command
**remains a dev tool** (NOT the production fallback).

## Grounding from the dataflow map (must obey)

- **Pipeline is INDEXER-BOUND** (SCIP ~1.9–3.0s/TS partition; xref ~21ms). → producer async/background,
  never block a query; the real work is partition discovery, not the feed.
- **repo_uid + a real `build_inputs_hash`** thread through (1B used `"preload"` placeholders).
- 1C produces only in-memory state (warm cache is later).

## D0 — producer provisioning (ratified)

```text
1. configured binary path first:  RMAP_SCIP_TYPESCRIPT=/absolute/path/to/scip-typescript
2. PATH lookup second
3. absent producer => graceful ProducerUnavailable
No npx. No install from the daemon. No network. No package-manager execution.
```

**Absent behavior (must hold):** the refresh request returns a structured `ProducerUnavailable`; the
partition is unchanged; LiveGraph last-good remains; the default SQLite path is unaffected;
`--engine livegraph` returns fallback/Unavailable as in 1B. The daemon NEVER crashes on absence.

**VERIFIED (2026-06-01): scip-typescript is absent in this environment** (not PATH / fixture / global).
So the success path cannot be validated here until provisioned; see Validation.

## Ratified decisions

**D1 — partition discovery:** `package.json` + `tsconfig.json` boundaries only, TS-first. No broad
workspace resolver in 1C.

**D2 — trigger:** explicit `rmap dev livegraph-refresh --repo <repo> [--partition <id>]`. Do NOT
piggyback `rmap index`.

**D3 — process execution:** daemon runs `scip-typescript` **binary-direct** from the configured/
discovered path via Rust `std::process::Command` (NOT a shell string; no shell interpolation). Required:
a timeout, captured stdout/stderr, surfaced exit status, output `index.scip` under a temp/work dir.

**D4 — `build_inputs_hash`:** a fast non-cryptographic digest (coherence, not security) over: source
files in the partition + `tsconfig.json` + `package.json` + nearest lockfile (if present) + indexer
binary path/version + relevant env/config values + `scip-typescript --version` output if available.
If the version command is unavailable, hash the binary path + mtime/size, and mark the version
`unknown` in provenance. Replaces the `"preload"` placeholder.

**D5 — concurrency:** ONE partition refresh at a time (1C). A background worker thread/task runs the
producer; `RepoState` refresh state is lock-protected; **the query path never waits on the producer
process.** Do NOT hold the LiveGraph write lock while the producer runs — hold it ONLY to mark state
and to swap the result in.

**D6 — failure visibility:** a structured daemon response from the refresh command + trust state for
queries. Failure classes:

```text
ProducerUnavailable | ProducerFailed | Timeout | IngestFailed | HashFailed | UnsupportedPartition
```

## Async execution mechanism (ratified)

Use the simplest safe primitive: `std::thread::spawn` (or the daemon's existing runtime if present).
**Do NOT introduce Tokio for this slice.** Per-partition state machine:

```text
Idle/Fresh
Refreshing            → query serves last-good if present, FreshnessState::PrecisionPending
RefreshFailed(reason) → last-good remains
success: ingest → feed_partition + load value facts (same epoch) → atomic swap → state Fresh
failure: last-good remains → state RefreshFailed(reason)
```

The query path must remain lock-free during indexing (the producer runs outside any LiveGraph lock).

## Feed path (daemon-driven; unchanged shape from 1A/1B)

```text
scip-typescript → index.scip → decode_index → repo-graph-scip-ingest::ingest_partition
  → IngestOutcome { ir, complexity } → repo-graph-livegraph-feed::feed_partition
  → LiveGraph swap + value-fact reload (same partition epoch)
```

## Build order (ratified)

```text
1. Producer discovery + failure DTO (the 6 classes).
2. Explicit refresh command (rmap dev livegraph-refresh).
3. Background worker with the ProducerUnavailable path.
4. Success path behind a provisioned producer (Command spawn → decode → ingest → feed → swap).
5. Validate absent-producer behavior (ProducerUnavailable) — runnable NOW.
6. If possible, provision scip-typescript and validate the success path.
7. Do NOT mark complete unless the success path ran.
```

## Acceptance

```text
- daemon can run SCIP indexing for the synthetic TS partition WITHOUT dev preload (needs provisioned producer)
- produced PartitionIr enters LiveGraph; value facts loaded for the same epoch
- build_inputs_hash is real (not a placeholder)
- rmap --engine livegraph works after a daemon SCIP refresh (not after dev preload)
- a failed refresh leaves the last-good epoch and reports RefreshFailed (structured)
- default SQLite path unchanged
- scip-typescript absent => structured ProducerUnavailable, partition unchanged, no crash
```

## Validation (corrected — prevents a false "done")

```text
Implementation can proceed with ProducerUnavailable validation NOW (producer absent in this env).
Full success-path validation REQUIRES provisioning scip-typescript.
If the producer is absent after implementation, the slice is PARTIAL, not complete.
```

## Out of scope (hard guardrails)

```text
No warm cache / persistence. No SQLite decommission. No LiveGraph default. No Rust/C++ partitions.
No worker pool (D5). No piggyback on rmap index (D2). No npx / install / network (D0). No Tokio.
No broad multi-workspace resolver (D1). dev-preload stays a DEV tool, not the production fallback.
```

## References
- `docs/architecture/dataflow-hotpath-map.md` (indexer-bound; async producer; real hash)
- `docs/slices/livegraph-integration-1b.md` (`feed_partition` seam + `RepoState.livegraph` + dev preload)
- `docs/slices/refresh-probe-1.md` (two-speed: last-good / atomic swap / keep-on-failure)
- `docs/architecture/scip-migration-plan.md` (Stage D order)
