# DEV-INSTALL-DOCTOR-WAIT-1: Remove the ~80s `rmap doctor` Wait (validation-infrastructure / bug)

Slice ID: DEV-INSTALL-DOCTOR-WAIT-1
Status: **FIXED + validated (2026-06-01).**
Task type: bug / validation-infrastructure.
Track: interrupt before slice work — a fixed wait in `doctor` is a correctness smell (validation paying a
timeout/heavy query, not proving readiness).

## Cause (traced read-only first)

`rmap doctor` → `execute_doctor` → `storage_summary_probe` (rgr/commands/doctor.rs) sent the heavy
`perf` RPC. On the daemon, `handle_perf` calls `collect_database_metrics()` which runs `SELECT COUNT(*)
FROM "<table>"` for EVERY table + `dbstat` sizing — full scans of the large `nodes`/`edges` tables. For
a big repo (repo-graph itself) that is ~50-100s; the CLI sat at ~0% CPU waiting on the single-threaded
daemon.

- NOT a timeout (READ_TIMEOUT_SECS=300; the query completed under it), NOT a retry loop, NOT a stale
  socket / network probe / stdio fallback / repo lock. Measured + ruled out.
- Intermittent BY CWD: `doctor`/`perf` from the tiny fixture = 0.2-0.4s; from the repo-graph root =
  ~80-100s. dev-install runs `doctor` inside the repo-graph tree → slow. The cold cargo `--release`
  build (the original ~50-min run) had masked this steady ~80s sink.
- Mismatch: `doctor` uses only `db_size_bytes` + `total_snapshots` + `prunable`, but `perf` computes the
  full per-table breakdown regardless.

(Surfaced via the dev-install phase timing committed in `a4a8130`: validate was 82s of a 95s warm run.)

## Fix (Option A — cheap storage-health path; ratified)

New daemon method `storage_health` returning ONLY what the doctor probe needs, all cheap:

```json
{ "db_size_bytes": <fs metadata on the DB file>, "total_snapshots": <COUNT>, "prunable_snapshots": <COUNT> }
```

- `db_size_bytes`: `std::fs::metadata(db_path).len()` (filesystem metadata, NOT a per-table calc).
- `total_snapshots` + `prunable_snapshots`: `get_retention_stats` — cheap `COUNT(*)`s on the small
  `snapshots` table (confirmed cheap by trace; `prunable` did NOT need to be dropped/`null`).
- Does NOT call `collect_database_metrics`. **`rmap perf` is unchanged** (still the heavy diagnostic).
- doctor's storage probe still runs + reports the SAME health summary (db size + snapshots + prunable);
  no check removed; the "not indexed" contract preserved.

Files: `daemon-runtime/src/handlers/metrics.rs` (`handle_storage_health`),
`daemon-runtime/src/dispatch.rs` (route), `rgr/src/commands/doctor.rs` (`perf` → `storage_health`, flat
fields).

## Validation (EXECUTED)

```text
rmap doctor   (repo-graph root, daemon up)  -> 0.42s   (was ~80s; target <5s)            PASS
dev-install validate phase                  -> 2s      (was 82s)                          PASS
rmap perf     (repo-graph root)             -> ~50s    (still heavy, UNCHANGED)           PASS
daemon stopped -> rmap doctor               -> exit 1, 0.07s, reports daemon unreachable  PASS
daemon restarted -> rmap doctor             -> exit 0 (healthy)                           PASS
daemon-runtime 72 tests; clippy -D warnings (daemon + rgr); cargo fmt --all --check clean.
```

Acceptance met: warm `doctor` <5s; no fixed-timeout wait in validation; `doctor` still fails when the
daemon is down; the same health checks are preserved (none removed); `perf` untouched.

## Out of scope (honored)
```text
No timeout around the probe. No `perf` optimization. No release-profile change. No sccache. No daemon
architecture change. Doctor storage probe NOT removed.
```

## Follow-ups (recorded, not done)
- `rmap perf` itself is ~50-100s on large repos (the per-table `COUNT(*)` + `dbstat`). A separate
  optimization if/when `perf` performance matters (PERF-OBS follow-up).
- The phase-timing instrumentation (`a4a8130`) stays in `dev-install-local.sh` for future regressions.

## References
- `scripts/dev-install-local.sh` (phase timing, `a4a8130`)
- `rust/crates/daemon-runtime/src/handlers/metrics.rs` (`handle_perf` heavy; `handle_storage_health` cheap)
- `rust/crates/rgr/src/commands/doctor.rs` (`storage_summary_probe`)
