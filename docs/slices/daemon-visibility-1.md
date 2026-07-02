# DAEMON-VISIBILITY-1 — Long operations are visible, and never reported as dead

Status: SPECIFIED (2026-07-02) · Track: Product-surface honesty (daemon ops)
Origin: real first-run on a 160k-file repo, operator's second Mac (2026-07-02)
Prior art: `rmapd-perf-1-timeout.md`, `daemon-socket-health-1.md`,
`dev-install-doctor-wait-1.md`; the in-code TODO at
`rust/crates/rgr/src/daemon_client/mod.rs` ("the proper fix is to have the
daemon …") already names this gap.

## 1. Problems (all observed in one session)

**C — A running index is reported as a timeout failure.**
`rmap index` on a 160k-file repo blocked the CLI for 5 minutes, then printed
"timed out after 300s" — while `rmapd` kept indexing (visible in Activity
Monitor) and completed ~10–15 minutes later. The client's fixed 300s read
timeout presents *operation still running* as *operation failed*. The user
had no way to know the index survived, no completion signal, and no honest
next action. This violates the honesty rules (VISION): the surface reported
something false about the reader's world.

**D — No visibility into what the daemon is doing.**
`rmap doctor` during that window did not report that an index was in
progress — no current-operation line, no progress, no "enrichment ran / did
not run" after completion. The daemon's coordinator already knows it is
Writing (op kind, repo); none of that state is exposed on any surface.

**E — `doctor` reports bogus errors under normal concurrency.**
While the daemon held the database (indexing), `doctor` reported "error
opening database" — lock contention / busy presented as a health failure.
A diagnostics surface that cries wolf during normal operation trains the
reader to ignore it.

## 2. Contract

**C — Honest long-op client behavior.**
1. On client read-timeout for a mutating long op (index/refresh), the CLI
   MUST NOT print a bare failure. It probes the daemon: if the daemon is
   alive and the operation is still running (see D's status surface), it
   reports that truthfully — repo, operation, elapsed, and how to follow
   (`rmap doctor` / re-run semantics) — and exits with a distinct
   "still running" status (not the failure exit code).
2. While attached, the CLI surfaces the daemon's progress (see D) at least
   coarsely (phase + files processed) rather than blocking silently for
   minutes. Mechanism (streamed events vs periodic poll) is the builder's
   proposal — bounded to an ADDITIVE daemon-protocol change.
3. Completion remains observable: a follow-up `rmap doctor` (or the index
   command re-attached) states the last completed snapshot for the repo.

**D — An operations/status surface.**
1. The daemon exposes its current activity: active operation(s) — kind,
   repo, phase, started-at, and cheap progress counters where the pipeline
   already counts (files extracted / total inventory; postpass phase names).
   No new bookkeeping beyond what phases already know — this is exposure,
   not instrumentation buildout.
2. `rmap doctor` renders it: "indexing <repo>: extraction 42k/160k files,
   started 6m ago" or "idle; last snapshot <repo> @ <time>".
3. Enrichment honesty (pairs ENRICH-LIFECYCLE-1, does not implement it):
   after an index completes, `doctor` (and the index completion report)
   state enrichment's status for that snapshot in D5 next-action form —
   e.g. "enrichment: not run (run `rmap enrich …` for resolved call
   types)" — so "did it enrich?" is never a mystery. When
   ENRICH-LIFECYCLE-1 lands auto-enrichment, this same line reports
   running/completed.

**E — Doctor never reports normal contention as failure.**
`doctor`'s DB probe distinguishes: (a) database absent/corrupt → error;
(b) database locked/busy by the daemon → healthy, reported in reader frame
("in use by daemon — indexing <repo>", cross-referencing D); (c) open OK.
No "error opening database" while a live daemon holds it.

**Out of scope:** the installer (INSTALL-ROBUSTNESS-2); auto-enrichment
(ENRICH-LIFECYCLE-1); cancellation semantics (shipped, DAEMON-CANCEL);
progress persistence across daemon restarts; fancy TTY progress bars
(coarse text lines suffice — this is an honesty slice, not a UI slice).

## 3. Stop conditions

- Daemon protocol changes must be ADDITIVE (existing requests/replies
  unchanged); if honest behavior seems to require a breaking change →
  STOP + DECISION_REQUIRED.
- If exposing progress requires touching the W-B epoch / coordinator
  invariants (`daemon-w-b-epoch-1.md`) beyond read-only observation →
  STOP + DECISION_REQUIRED.
- Do NOT change indexing behavior/performance paths; exposure only.

## 4. Validation (end-of-slice, synchronous; TEST REPORT)

- `cargo build` + full `cargo test` + `cargo fmt --check` +
  `cargo clippy -- -D warnings` green (from `rust/`), report inlined.
- **Still-running proof (named test):** a slow in-flight write op + a client
  with a short timeout → client output states "still running" (not failure)
  and exit code is the distinct status.
- **Status proof (named test):** during an in-flight index, the status
  surface reports op kind/repo/phase; when idle, reports idle + last
  snapshot.
- **Doctor-contention proof (named test):** doctor against a daemon-held
  database reports healthy-in-use, not an error.
- **Enrichment-line proof (named test):** post-index report/doctor includes
  the enrichment status line with the D5-style next action.
- `./scripts/dogfood-isolated.sh` green; self-dogfood: index repo-graph in
  isolated state, observe doctor mid-index and post-index output (transcript
  inlined).

## 5. Definition of done

On a large repo: `rmap index` never reports a live operation as a failure,
progress is observable while attached and via `rmap doctor`, doctor reports
in-use databases as healthy, and every completed index states whether
enrichment ran — all proven by the named tests + an executed self-dogfood
transcript.
