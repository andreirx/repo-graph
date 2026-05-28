# CURRENT_SLICE.md

## Current Priority

**PERF-OBS-1B:** Timing instrumentation (phase breakdown, global vs sandbox).

See `docs/slices/perf-obs-1.md` for specification.

---

## Recently Completed

**STATE-ROOT-SEPARATION-1:** Authority vs Sandbox-Local State Boundaries — COMPLETE (2026-05-28)

See `docs/slices/state-root-separation-1.md`.

Architecture refinement: Split original "Tier A" into two sub-classes:

| Class | Sandbox Behavior | Examples |
|-------|------------------|----------|
| **A1: User Authority** | Blocked | aliases, baselines, declarations |
| **A2: Operational Local** | Allowed | repo registration, snapshot metadata |
| **B: Derived Cache** | Allowed | nodes, edges, measurements |

Implementation:
- `StateRootMode` enum with `Global` and `SandboxLocal` variants
- `DaemonState::state_root_mode()` detects sandbox via `/private/tmp/` prefix
- `require_global_mode_for_authority_write()` guard helper
- Guards applied to `mark_baseline`, `unmark_baseline`, `repo_alias`
- `rmap doctor` reports `authority_policy` probe
- Stdio daemon startup warning in sandbox mode

Validation (all EXECUTED):
- A1 blocking: 3 unit tests (mark_baseline, unmark_baseline blocked; global mode allowed)
- A2/B allowed: 1 integration test (sandbox index with real repo, verifies registration + cache)

**RETENTION-POLICY-1:** Automatic Pruning on Refresh — COMPLETE (2026-05-27)

See `docs/slices/retention-policy-1.md`.

- `enforce_retention_lifecycle()` helper: classify → prune → stats (sequenced, not single-tx)
- Wired into `handle_index` and `handle_refresh` in dispatch.rs
- Response includes `retention.pruned_count` and stats
- Daemon logs "retention: pruned N snapshot(s) for repo X"
- 19 retention tests + 4 migration tests verify lifecycle invariants
- Field-validated on `/tmp/test-retention`: 4 snapshots → 2 after auto-prune
- `classify_repo_retention()` is atomic (single transaction)
- `prune_prunable_snapshots()` is atomic (single transaction)
- Lifecycle is sequenced: if prune fails after classify commits, next run completes it
- Migration 029 repairs orphan FK references from pre-fix repos
- `retention/` module split by concern (all files under 500-line guardrail)

**CACHE-SEMANTICS-1:** Extracted Facts as Rebuildable Cache — COMPLETE (2026-05-27)

See `docs/slices/cache-semantics-1.md`.

Semantic contract for cache/authority separation:
- Migration 028 adds `derived_cache_epoch` and `retention_class` to snapshots
- Storage crate exposes `classify_repo_retention()`, `prune_prunable_snapshots()`, etc.
- **Whole-snapshot invalidation enforced**: stale-epoch snapshots excluded from protected roles (current/parent/baseline_auto), always marked prunable
- Valid epoch: `CURRENT_CACHE_EPOCH` or `NULL` (legacy)
- Daemon auto-classifies retention after index/refresh
- `rmap perf` shows retention stats
- `rmap doctor` reports prunable count
- User baseline marking via daemon methods only (`mark_baseline`/`unmark_baseline`); no CLI surface
- 15 storage tests verify retention behavior including stale epoch exclusion and baseline marking invariants

Design decisions made:
- Snapshot-level epoch (simpler than per-table versioning)
- Whole-snapshot invalidation: stale epochs cannot become current/parent/baseline_auto
- Hybrid baseline selection (automatic + user-explicit via daemon methods; CLI deferred)

**PERF-OBS-1A:** Storage Performance Observability (Volume Baseline) — COMPLETE (2026-05-27)

See `docs/slices/perf-obs-1.md`.

Volume baselines captured. Key finding: authority tiny, cache dominates. Sufficient for CACHE-SEMANTICS-1.

**PERF-OBS-1B** (timing instrumentation) deferred — can return to it later if needed.

---

**STDIO-STATE-ROOT-1:** Sandbox-Writable State Root for Stdio Transport — COMPLETE (2026-05-26)

See `docs/slices/stdio-state-root-1.md`.

When stdio transport is activated due to EPERM/EACCES sandbox denial:
- Injects `RMAP_STATE_ROOT=/private/tmp/repo-graph-agent/<uid>` into subprocess
- Creates sandbox root directory with mode 0700
- `rmap doctor` reports active state root and mode

Validated in glamCRM Codex shell:
- `rmap index .` — succeeded
- `rmap check` — succeeded  
- `rmap orient --focus backend` — succeeded
- `rmap modules list` — succeeded

**STDIO-TRANSPORT-1:** Agent-Safe Stdio Subprocess Transport — COMPLETE (2026-05-26)

See `docs/slices/stdio-transport-1.md`.

Transport abstraction with bounded auto-fallback on EPERM/EACCES.
Removed socket-only preflight gates from all command handlers.

---

**DAEMON-SOCKET-HEALTH-1:** Daemon Socket Health Diagnostics — COMPLETE (2026-05-26)

See `docs/slices/daemon-socket-health-1.md`.

Granular socket probes (socket_file, socket_connect, socket_ping).
Actionable error messages with recovery commands.
Root cause identified: Codex sandbox denies Unix socket connect (EPERM).

**SOCKET-RENDEZVOUS-1:** Canonical Daemon Socket Path Resolution — COMPLETE (2026-05-26)

See `docs/slices/socket-rendezvous-1.md`.

Platform-paths crate with canonical home lookup via `getpwuid_r(geteuid())`.
Files split into 4 modules (home.rs, dirs.rs, socket.rs, lib.rs) per 500-line guardrail.

**TS-IMPORT-RESOLUTION-1:** TypeScript aliased and namespace import resolution — COMPLETE (2026-05-23)

See `docs/slices/ts-import-resolution-1.md`.

**LEGACY-CONTRACT-MIGRATION-1:** Full slice — COMPLETE (2026-05-23)

All 7 legacy commands migrated to REG-1 daemon contract.

---

## Queued

Candidates (see ROADMAP.md):
- **PERF-OBS-1B:** Timing instrumentation (phase breakdown, global/sandbox comparison)
- **CURSOR-1:** Cursor MCP/rules integration

---

## Output Program Wave Model

| Wave | Slice | Commands | Status |
|------|-------|----------|--------|
| 1 | CLI-OUT-2B | orient, trust, cycles, check | VALIDATED |
| 1b | CLI-OUT-2C | stats | IMPLEMENTED |
| 2 | CLI-OUT-3 | callers, callees, path, imports | IMPLEMENTED |
| 3 | CLI-OUT-4 | modules (6), surfaces (2), boundaries (3) | COMPLETE |
| 4 | CLI-OUT-5 | docs (2), resource (3), policy (1) | COMPLETE |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | COMPLETE |
| 6 | CLI-OUT-7 | violations, gate, assess | COMPLETE |
