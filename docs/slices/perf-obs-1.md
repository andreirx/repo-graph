# PERF-OBS-1: Storage Performance Observability

## Status

PARTIAL (2026-05-27) — split into PERF-OBS-1A (complete) and PERF-OBS-1B (pending).

### PERF-OBS-1A: Volume Baseline — COMPLETE

1. `rmap perf` command with per-table/tier/layer metrics and classification coverage
2. `rmap doctor` shows DB size and snapshot count (transport-correct via DaemonClient)
3. Daemon startup timing logged at INFO level (cold vs warm detection)
4. Volume baselines captured for repo-graph, glamCRM, django (see `docs/perf-baselines/`)
5. State root lifecycle audit (`docs/architecture/state-root-lifecycle.md`)

**Key finding:** Authority rows are tiny (44 in repo-graph), extracted cache dominates (1.6M rows).

### PERF-OBS-1B: Timing Instrumentation — PENDING

1. Phase-level timing breakdown (handler, query, refresh stages)
2. Global vs sandbox comparison artifact for same repo
3. hadoop baseline (currently times out at >300s)
4. Memory metrics (RSS tracking)

**Why split:** The original DoD item "phase breakdown" requires handler-level timing annotations in daemon and query timing in storage layer. A simple wall-clock stopwatch is not phase breakdown. This is a separate instrumentation effort.

### Discovered Limitations

- hadoop (9.5GB) times out on perf query (>300s)
- `COUNT(*)` across many tables is O(n) in row count
- Large database metrics need sampling or caching strategy

## Problem Statement

Before migrating to the three-tier storage architecture (STORAGE-ARCH-1), we need baseline metrics to:
1. Identify actual bottlenecks vs assumed ones
2. Measure improvement after changes
3. Prioritize which commands to migrate first
4. Determine whether retention volume, query shape, or memory working set is the primary constraint

Currently, performance understanding is anecdotal (e.g., "refresh takes 6 minutes on large repos").

## Scope

### In Scope (Full Slice)

One-shot measurement commands and structured timing sufficient to answer storage-architecture questions:

**Volume metrics (PERF-OBS-1A):**
- Per-table row counts and disk size estimates
- Per-fact-layer row counts (L0-1 extracted, L2 derived, L3 hints)
- Historical snapshot retention count
- Classification coverage reporting

**Timing metrics (PERF-OBS-1B):**
- Refresh timing by phase (detect changes, extract, copy-forward, commit)
- Cold index timing
- Query latency for: `check`, `orient`, `trust`, `callers`, `path`, `cycles`

**Memory metrics (PERF-OBS-1B):**
- Daemon RSS after repo load
- Memory per loaded repo (if measurable)

**Comparison dimensions (PERF-OBS-1B):**
- Global launchd state root vs sandbox-local stdio state root

### Out of Scope

- Always-on daemon metrics logging
- Rolling metrics files or telemetry history
- Persistent metrics subsystem
- Automated alerting/dashboards
- External monitoring integration

## Definition of Done

### PERF-OBS-1A (Volume Baseline) — COMPLETE

1. `rmap perf` command outputs structured JSON with:
   - Per-table row counts and size estimates
   - Per-layer row counts
   - Snapshot count and retention breakdown
   - DB file size
   - Classification coverage report
2. `rmap doctor` reports DB file size and snapshot count
3. Daemon logs startup timing at INFO level (cold vs warm)
4. Volume baselines captured for: repo-graph, glamCRM, django

### PERF-OBS-1B (Timing Instrumentation) — PENDING

1. Phase-level timing breakdown in daemon handlers
2. `rmap perf --timing <command>` with handler/query phase breakdown (not just wall clock)
3. Global vs sandbox comparison artifact for glamCRM
4. hadoop baseline (requires timeout increase or sampling strategy)

## Reference Corpus

| Repo | Size Class | PERF-OBS-1A Status | PERF-OBS-1B Status |
|------|------------|--------------------|--------------------|
| repo-graph | small-medium | Captured | Pending |
| glamCRM | medium | Captured (global only) | Pending (needs sandbox comparison) |
| django | medium-large | Captured | Pending |
| hadoop | monorepo (9.5GB) | Timeout boundary documented | Pending (needs sampling) |

## Measurement Categories

### A. Volume by Tier (PERF-OBS-1A)

| Tier | Tables | What to measure |
|------|--------|-----------------|
| A (Authority) | repos, declarations, schema_migrations, snapshots (metadata) | Row count, size — should be small |
| B (Derived Cache) | nodes, edges, files, measurements, inferences, module_*, boundary_*, etc. | Row count, size — bulk of storage |

### B. Volume by Fact Layer (PERF-OBS-1A)

| Layer | Description | Expected tables |
|-------|-------------|-----------------|
| 0-1 | Extracted facts | nodes, edges, files, file_versions, measurements |
| 2 | Derived/inferred | inferences, module_candidates, boundary_*, semantic_facts |
| 3 | Orientation hints | project_surfaces, surface_* |

### C. Retention Analysis (PERF-OBS-1A)

- Total snapshot count per repo
- Age of oldest snapshot

### D. Command Latency (PERF-OBS-1B)

| Command | Expected Tier | Notes |
|---------|---------------|-------|
| `check` | A + B | Safety check, aggregation |
| `orient` | A + B | Multi-source summary |
| `trust` | B | Statistics aggregation |
| `callers` | B (candidate for C) | Graph traversal |
| `path` | B (candidate for C) | Path finding |
| `cycles` | B (candidate for C) | SCC analysis |

### E. Daemon Lifecycle (PERF-OBS-1A partial, PERF-OBS-1B full)

- Cold start: time from process start to ready — PERF-OBS-1A (startup log)
- Per-repo load time: time to activate a repo after index — PERF-OBS-1B

## Validation Plan

### PERF-OBS-1A — EXECUTED

1. Run `rmap perf` on repo-graph, verify JSON output is parseable — PASSED
2. Run `rmap doctor`, verify storage summary appears — PASSED
3. Capture baseline metrics to `docs/perf-baselines/` — DONE (3 repos)

### PERF-OBS-1B — PENDING

1. Run `rmap perf` on hadoop, verify it completes — BLOCKED (timeout)
2. Compare `rmap perf` output between global root and sandbox-local root for glamCRM
3. Run phase-level timing command and verify handler breakdown appears

## Implementation Approach

### Phase 1: Storage metrics command — COMPLETE (PERF-OBS-1A)

- `rmap perf` command created
- Query SQLite for table row counts (`SELECT COUNT(*) FROM <table>`)
- Query SQLite page count for size estimates (`PRAGMA page_count`, `PRAGMA page_size`)
- Group by tier (A/B) and layer (0-1/2/3)
- Output structured JSON with classification coverage

### Phase 2: Daemon lifecycle metrics — COMPLETE (PERF-OBS-1A)

- Startup timing logged at INFO level
- Cold vs warm detection based on registry.json existence
- Storage summary in `rmap doctor`

### Phase 3: Timing instrumentation — PENDING (PERF-OBS-1B)

- Instrument key phases in daemon handlers
- Add timing annotations to storage queries
- Report phase breakdown in response payload

### Phase 4: Baseline capture — PARTIAL

- Volume baselines archived to `docs/perf-baselines/` — DONE
- hadoop baseline — BLOCKED (timeout boundary documented)
- Global vs sandbox comparison — PENDING

## Files in Scope

### PERF-OBS-1A (Created/Modified)

- `rust/crates/rgr/src/commands/perf.rs` — perf command
- `rust/crates/rgr/src/commands/doctor.rs` — storage probe
- `rust/crates/storage/src/metrics.rs` — metrics queries
- `rust/crates/daemon-runtime/src/handlers/metrics.rs` — perf handler
- `rust/crates/daemon-runtime/src/lib.rs` — startup timing
- `docs/perf-baselines/` — baseline archive

### PERF-OBS-1B (Pending)

- `rust/crates/daemon-runtime/src/handlers/` — timing instrumentation
- Handler-level timing annotations
- Response payload timing breakdown

## Dependencies

- STORAGE-ARCH-1 (defines tier classification) — SPEC COMPLETE

## Notes

The volume baseline (PERF-OBS-1A) is sufficient to proceed with CACHE-SEMANTICS-1. The key finding — authority rows tiny, cache dominates — validates the tier separation model. Timing instrumentation (PERF-OBS-1B) can be done in parallel or deferred until tracing infrastructure is added.
