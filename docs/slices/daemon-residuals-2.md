# DAEMON-RESIDUALS-2 — retention: reproduce, measure, fix what was measured, then prevent

Status: SPECIFIED (2026-09-05) · Track: split from DAEMON-RESIDUALS-1 (human-ratified
(a)+(b) + prevention set; the FK-index premise RETRACTED — indexes exist since
001-initial.sql). CODE slice, diagnose-then-fix. Maturity: MATURE.

## 1. Problem (MEASURED on production, 2026-09-04)

A retention prune on a 4.8 GB / 29-snapshot repo-graph store ran 5h+ with ZERO committed
progress (per-snapshot transactions; cascades already index-seek; 2 MB page cache; the
daemon restarts each time killed it since May). A `repo remove` + `index` rebuild took 40 s
(253 MB, 1 snapshot). The prune's actual mechanism is UNDIAGNOSED.

## 2. Contract

1. **Reproduce on a representative multi-snapshot store** (6–8 isolated re-indexes of a
   mid-size repo, or a snapshot-heavy fixture) and MEASURE per phase: which statement,
   which table, rows/s, page-cache behaviour, WAL growth, FK RESTRICT checks on
   non-cascade children, the `parent_snapshot_uid` update. The diagnosis is a deliverable.
2. **Fix ONLY the measured mechanism** (chunked per-snapshot deletes with the write slot
   released between chunks are ratified regardless; `cache_size` sized to the store for
   maintenance connections is ratified regardless).
3. **(b) Rebuild-instead-of-delete** when the prunable share dominates (copy the kept
   snapshot(s) into a fresh file and swap; no VACUUM needed), exposed as
   `rmap maintenance rebuild`.
4. **Prevention set** (human directive "no future purge takes this long"): snapshot hard
   cap (current + parent) with prune-on-commit at index/refresh commit; a retention time
   budget that aborts at a chunk boundary and flips the repo to the rebuild path; `doctor`
   shows store size, snapshot count, prunable share, last-pass duration + progress with
   an ETA basis; a retention BENCHMARK GATE (N snapshots × M rows under a fixed bound) in
   the test suite.
5. Frozen invariants: single-writer per DB FIFO-fair; readers never see partial writes;
   W-B epoch/coordinator semantics; wire protocol; the 300 s client timeout value.

## 3. Stop conditions

Frozen as §2.5; storage schema additive only; exit codes. STANDING HONESTY RULES (Busy
named; progress stated with basis). If the measured mechanism needs more than chunking +
cache sizing + rebuild, STOP + DECISION_REQUIRED with options. Never touch the operator's
real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproduction FIRST with per-phase numbers; unit: chunk boundaries preserve per-snapshot
  atomicity; cap + prune-on-commit; budget → rebuild; rebuild swap atomic; benchmark gate.
- Live proof (isolated state root, registry sha unchanged): the reproduced store prunes
  (or rebuilds) under the bound with a concurrent foreground read loop staying under the
  patience; doctor fields verbatim. Gates first, proofs small; delete every isolated root.

## 5. Definition of done

Mechanism measured and named; the measured fix + rebuild path + prevention set shipped
under the frozen invariants; benchmark gate green; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; repo-graph is
THIS repo.
