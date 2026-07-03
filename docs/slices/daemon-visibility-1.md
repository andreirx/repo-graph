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

**F — A non-READY snapshot is invisible, and `orient` gaslights about it
(day-2 finding, same machine, 2026-07-03).** The next day `doctor` reported
"storage db 4 GB, 1 snapshot" — but `rmap orient` said "no READY snapshot
for repo id … index the repo first." So a 4 GB snapshot exists in some
non-READY state (the previous day's index was evidently interrupted before
finalize — daemon restart / machine sleep — or finalize failed), and:
- no surface shows the snapshot's STATE or the last index's OUTCOME;
- `orient`'s error says "index the repo first" to a user who indexed for
  15 minutes yesterday — it does not mention that a snapshot exists, what
  state it is in, or that 4 GB of disk is held by it.

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

**F — Snapshot state and last-index outcome are first-class facts.**
1. `doctor` (and `rmap repo info`) report, per repo: each snapshot's state
   (READY / in-progress / interrupted-partial), its size on disk, and the
   LAST INDEX OUTCOME (completed <time> / interrupted at <phase> /
   failed: <reason>). "1 snapshot, 4 GB" without its state is a non-answer.
2. `orient` (and any READY-snapshot-requiring surface) on a repo with only
   non-READY snapshots states the truth in reader frame: "a snapshot from
   <time> exists but was interrupted before completion (state: <X>, 4.0 GB)
   — re-run `rmap index`; reclaim the partial with `rmap maintenance
   prune`." NEVER a bare "index the repo first" while a snapshot exists.
3. Non-READY snapshots are visible to `rmap maintenance prune` (prunable,
   with size shown) so interrupted indexes do not silently hold disk.

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
- **Snapshot-state proof (named test):** with an interrupted (non-READY)
  snapshot fixture: doctor/repo-info name its state + size + last-index
  outcome; orient's error names the existing snapshot and both next actions
  (re-index / prune); `maintenance prune` lists it as prunable.
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
