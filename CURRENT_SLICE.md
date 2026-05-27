# CURRENT_SLICE.md

## Current Priority

**PERF-OBS-1:** Storage Performance Observability — PARTIAL

See `docs/slices/perf-obs-1.md`.

### Completed (PERF-OBS-1A)

- `rmap perf` with per-table/tier/layer metrics and classification coverage
- `rmap doctor` shows DB size and snapshot count (transport-correct)
- Daemon startup timing logged (cold vs warm)
- Volume baselines: repo-graph (1.4GB), glamCRM, django
- State root lifecycle audit

**Key finding:** Authority rows are tiny (44 in repo-graph), extracted cache dominates (1.6M rows).

### Not Completed (PERF-OBS-1B)

- `rmap perf --timing` with phase breakdown — removed because implementation was wall-clock only
- Global vs sandbox comparison artifact — requires running in both modes
- hadoop baseline — timeout boundary documented, not actual capture

### Decision Point

Options:
1. **Start CACHE-SEMANTICS-1** — PERF-OBS-1A provides sufficient baseline for tier semantics work
2. **Complete PERF-OBS-1B** — finish timing instrumentation before storage architecture work
3. **Split slice** — close PERF-OBS-1A, create PERF-OBS-1B as separate slice

---

## Recently Completed

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
- **CACHE-SEMANTICS-1:** Tier B refresh/invalidation semantics (PERF-OBS-1A volume baseline sufficient)
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
