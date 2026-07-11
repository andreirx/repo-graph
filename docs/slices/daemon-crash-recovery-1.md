# DAEMON-CRASH-RECOVERY-1 — A crash leaves nothing the next boot can't explain

Status: **DELIVERED (2026-07-11)** · Track: Daemon correctness

## 0. Delivery record (2026-07-11)

Shipped via target-owned relay (approved cycle 4; one escalation ratified:
crash reason persisted in the extraction-diagnostics blob — durable, zero
migration) + operator validation. Kill-9 e2e transcript (gstreamer, kill -9
at t+10s mid-extraction): the next boot logs the orphaned op's posthumous
OUTCOME line ("op index interrupted (daemon restart) …"), reconciliation
logs its work ("marked 1 interrupted snapshot(s)"), and `repo info` renders
the durable reader-frame reason ("interrupted — daemon restart, reconciled
<time>") — surviving log rotation. Crash orphans are classified/countable as
prunable (review-1 catch) and reclaimed by the retention machinery. F8 op
lifecycle lines cover index/refresh/enrich/retention including the
yield-after-start path (review-2 catch). Storage-probe lock-race renders
reader-frame. Gates: 4934/0 workspace, fmt/clippy clean, dogfood green,
named proofs 2/2. Cosmetic follow-up noted: "interrupted: interrupted —"
label/reason wording dupe on the repo-info line.
Origin: TECH-DEBT F7–F12 (second-machine v0.5.0 field incident, 87k-file repo)

## 1. Problem — a daemon that dies mid-write leaves invisible wreckage

The daemon died mid-postpass on an 87k-file index (peak 5.8 GB). Everything
downstream failed honestly-shipped expectations because every shipped
guarantee assumed a SURVIVING daemon: 3 crash-orphaned `building` snapshots
(11 GB) sit in the store with retention stats `total: 3, every class: 0`;
`maintenance prune` says "no prunable snapshots found"; `orient` says "index
the repo first"; the daemon log for the entire incident is startup + three
broken-pipe lines.

## 2. Contract

1. **F7/F11 — Startup reconciliation.** On boot (and on repo load), any
   `building` snapshot with no live operation (the activity registry is
   empty at boot by construction) is marked `interrupted` with reason
   `daemon restart`, logged, and becomes visible/prunable through the
   SNAPSHOT-RETENTION-1 machinery (classification, doctor, `maintenance
   prune`, auto-retention pass). The interrupted-detection must not require
   evidence only a surviving daemon writes.
2. **F8 — Operation lifecycle in the daemon LOG.** `index`/`refresh`/
   `enrich`/`retention` log: start (op, repo, snapshot_uid), phase
   transitions (coarse), and outcome (completed/interrupted/failed+reason)
   — to the daemon log via the existing logging mechanism. Forensics must
   never depend on doctor being reachable. Keep it additive and low-volume
   (no per-file spam).
3. **F10 — No READY-requiring surface may bypass F2.** Find the error path
   that produced bare `no READY snapshot for repo: <uid>. index the repo
   first.` (client transcript, v0.5.0) and route it through the
   snapshot-facts-aware message (names the partial: state, size, both next
   actions). Audit sibling paths for the same bypass (deterministic grep for
   the bare message constructors).
4. **F12 — Retention stats name what they exclude.** When total > sum of
   classes, the stats table lists the unclassified snapshots by state
   ("3 building (orphaned — daemon restart?)"), never implying an empty
   store.
5. **F9 (folded) — Storage probe lock-race case.** A failed open due to
   `database is locked` renders reader-frame ("held by another process —
   daemon restarting?"), not a raw FAIL, when the daemon cannot attribute
   the lock to its own activity.

## 3. Stop conditions

- Additive only; no schema migration (use the existing status vocabulary —
  `interrupted`/reason columns exist per INDEX-DISCONNECT-1; if reason
  storage is missing → STOP + DECISION_REQUIRED).
- Startup reconciliation must be cheap (a query + updates at boot; no scans
  of blob tables) and must not delay socket readiness materially.
- Do NOT touch postpass memory behavior (tracked separately under #8).
  Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/`, inlined.
- **Reconciliation proof (named test):** a db seeded with `building`
  snapshots + empty activity registry → boot/load marks them interrupted
  (reason recorded), retention classifies them prunable, prune reclaims,
  stats name them before reclaim.
- **Log lifecycle proof (named test):** an index run writes start/outcome
  lines to the log sink; an interrupted op's absence of outcome is repaired
  by the next boot's reconciliation line.
- **F2-path proof (named test):** the previously-bare error path now names
  the partial; grep-proof that no bare constructor remains on READY-requiring
  daemon surfaces.
- `./scripts/dogfood-isolated.sh` green; isolated self-dogfood: kill -9 the
  isolated daemon mid-index, restart it, observe reconciliation in log +
  doctor + prune reclaim (transcript inlined; /private/tmp only).

## 5. Definition of done

Kill -9 a daemon mid-index: the next boot logs and marks the orphan, doctor
and retention stats name it, prune (and the auto-pass) reclaim it, and every
READY-requiring surface tells the user the truth about what exists. Proven by
the named tests + the kill-9 transcript.
