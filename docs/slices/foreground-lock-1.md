# FOREGROUND-LOCK-1 — a transient lock is a moment's patience, not an internal error

Status: SPECIFIED (2026-09-01) · Track: v0.13.0 audit queue (pulled forward — root-caused as
the test-flake family's product side, 5535092). CODE slice. Maturity: MATURE (dispatch).

## 1. Problem (measured — twice, independently)

1. Audit smoke: foreground `assess` during zap-engine's background pass → "error:
   InternalError: failed to open storage connection: database is locked" (capture pair shows
   the postpass retry succeeds cleanly).
2. Test flakes (root-caused 2026-09-01): the chained retention pass held the DB while a
   foreground `find` open failed fast — same message, 1-in-5 in find_facts_seam.
SQLite returns SQLITE_BUSY immediately (no busy-handler) on lock-upgrade conflicts, so the
foreground open path (`RepoState::storage()`) has ZERO patience where the background passes
(ENRICH-ROOT-1's `open_existing_with_busy_retry`) already wait. The message compounds it:
"InternalError" mislabels a routine transient; no retry guidance, no store path, no holder.

## 2. Contract

1. **Short bounded patience on foreground opens**: `RepoState::storage()` (the dispatch
   handlers' open) retries lock/busy open failures with a SHORT budget (e.g. 3×150ms —
   sub-half-second total, responsiveness preserved; parameterize the existing
   `open_existing_with_busy_retry` rather than a third loop; background passes keep their
   longer budget). Non-lock errors surface immediately, unchanged.
2. **The exhausted-patience message tells the truth and the next move**: name the holder
   CLASS from the daemon's own activity registry (it knows what op runs on that db — "a
   background <enrich|retention|index> pass holds the store"), state safe-to-retry, include
   the db path. Never "InternalError" for a lock transient — VERIFY the error-code taxonomy:
   if a more honest EXISTING code fits (e.g. a busy/unavailable class), use it; adding a NEW
   protocol error code is DECISION_REQUIRED (CI/client-facing).
3. **No behavior change when the DB is genuinely absent/corrupt** (DatabaseMissing and true
   I/O faults keep their paths and messages).
4. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: protocol error-code SET (message text free; new code = DECISION_REQUIRED), exit
codes, storage schema, background-pass budgets/semantics. STANDING HONESTY RULES. Unmet DoD
→ STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: a held write transaction + a foreground dispatch → pre-fix
  immediate lock error (FAILS post-assert); post-fix the dispatch succeeds within patience;
  a HELD-BEYOND-patience case renders the honest holder-naming message.
- Unit: patience budget bounds; holder-class naming from the activity registry (and the
  honest unknown when no op is registered); non-lock errors immediate.
- Live proof (isolated state root, registry sha unchanged): index a repo, race `assess`/
  `find` against the background pass window (the zap-engine shape) → no lock error across
  20 attempts; captures.
- Re-enable check: this closes the flake family's product side — run the previously-flaky
  binaries 10× each as part of validation.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No foreground command fails on a transient lock a half-second of patience would clear; an
exhausted wait names the holder and the next move; no new protocol codes without a decision;
the flaky-binary family is quiet 10×; gates green.
