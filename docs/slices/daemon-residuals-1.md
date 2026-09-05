# DAEMON-RESIDUALS-1 — the measured contention symptoms, each with its mechanism proven

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #7 (human-ratified Option A,
2026-09-04). CODE slice, diagnose-then-fix. Maturity: MATURE (daemon-runtime; frozen
invariants below).

## 1. Problem (MEASURED under batch load, 2026-09-04 audit; SURVEYED same day)

The daemon is already concurrent (DAEMON-CONCURRENCY-1 shipped 10493e8, 2026-06-24:
thread-per-connection, 64-conn cap, typed Busy; single-writer per DB via
`DatabaseState::write_lock` + per-repo `RepoCoordinator`). Four residuals remain:
1. **django `orient --full` → Busy** during the chunk-seed publish (78,214 vectors):
   `publish_guarded` holds the DB write slot longer than the foreground open patience
   (`OpenPatience::Foreground` = 4×150ms = 450ms, `state.rs:437-461`). Mechanism known.
2. **repo-graph `assess` → 301s client timeout** while the linux index was in its
   resolve phase on a DIFFERENT DB. NOT write-mutex contention. Uncontended retest:
   instant ("not armed"). Candidates: CPU/IO starvation by the resolve phase; a lazily
   built certificate (`import_cert`/`cycles_cert`/`stats_cert`, `state.rs:230-280`)
   taking the write lock on first build; connection-cap interaction. UNDIAGNOSED.
3. **TECH-DEBT #2b**: `handle_livegraph_preload`/`handle_livegraph_refresh` mutate the
   in-memory LiveGraph with NO repo coordinator guard — can swap the graph under live
   readers (`dispatch.rs` ~764-880 per TECH-DEBT citation).
4. **Retention VACUUM window**: `prune.rs:143-158` flips `journal_mode=DELETE` for VACUUM
   — the one place WAL's reader-non-blocking guarantee is suspended, unguarded against
   concurrent foreground reads. FIELD-CONFIRMED on the operator's PRODUCTION daemon
   (2026-09-04): 24 consecutive read commands (orient/modules/cycles/stats/trust/find…)
   all returned `Busy: a background retention pass is writing this repo's store` within
   seconds — the Busy is honest and named (good), but a retention pass blanks EVERY read
   surface for its whole duration. Priority within this slice rises accordingly.
   ESCALATED SAME DAY: `doctor` showed `reclaiming repo-graph: started 2h12m ago`; the
   store is 4.8 GB, the daemon alive at 4% CPU inside `sqlite3_step → BtreeNext →
   getPage` with a 344 MB WAL still growing. CORRECTED (code read, 2026-09-04 evening):
   the prune is NOT one transaction — `delete_snapshots_cascade` (prune.rs:186) runs ONE
   TRANSACTION PER SNAPSHOT: explicit `DELETE … WHERE snapshot_uid=?` on six orphan
   tables, then `DELETE FROM snapshots` which fires `ON DELETE CASCADE` across 37 FK child
   tables. RETRACTED 2026-09-05 (builder diagnosis, operator-verified on the live DB via
   PRAGMA index_list + EXPLAIN QUERY PLAN): every child table HAS a `snapshot_uid`-leading
   index — most since `001-initial.sql`, which the operator's grep of `migrations/*.rs`
   never saw, plus `sqlite_autoindex` from UNIQUE/PK constraints; only `declarations`
   (small) scans. The cascade deletes already use index seeks. THE PRUNE'S MECHANISM IS
   UNDIAGNOSED (candidates: per-row secondary-index maintenance across ~100k rows per
   snapshot on a 4.8 GB file with a 2 MB page cache; FK RESTRICT checks on non-cascade
   children; the `parent_snapshot_uid` update; WAL growth) — REPRODUCE AND MEASURE on a
   representative multi-snapshot store before any causal fix ships; the daemon's RSS is
   ~32 MB (SQLite's default 2 MB page cache) against a 4.8 GB file → page-cache thrash.
   28 snapshots to delete (27 READY prunable + 1 failed; keep-set is current-state only)
   accumulated since 2026-05-28 because a pass this long NEVER COMPLETES before a daemon
   restart (reboot, install, launchd) kills it — and it restarts from scratch. Hours, on
   a 977-file repo whose full reindex takes minutes.
   REBUILD DONE (human-ratified, 2026-09-04 19:22): `rmap repo remove` + `rmap index` —
   the index took ~40 s (19:22:46 → 19:23:24); store 4,802 MB → 253 MB (19×); snapshots
   29 → 1; nodes 454k → 18k, edges 419k → 22k, extraction_edges 2.19M → 110k; orient
   serves. The prune it replaced had run 5h+ and committed nothing. This is the
   measured case for (b): O(current) ≈ 40 s vs O(history × unindexed children) = never. Contract addendum for #4 (HUMAN-RATIFIED 2026-09-04: (a)+(b) — (c) optional; plus the prevention set in §2.6): (a) the MEASURED mechanism(s) from a reproduction on a multi-snapshot store (the
   FK-index migration is RETRACTED — indexes exist) + chunk within a snapshot with the write slot released
   between chunks, progress in `doctor`; and/or (b) REBUILD instead of DELETE when the
   prunable share dominates the store (copy the kept snapshot(s) into a fresh file and
   swap; O(current) instead of O(history × children); no VACUUM needed); and/or (c) make
   the pass RESUMABLE across daemon restarts so it never restarts from zero. The diagnosis
   must report the size composition.
   MEASURED (read-only, same day): 4,579 MB; 29 snapshots retained; extraction_edges
   2,194,150 rows; unresolved_edges 1,791,621; nodes 454,261; edges 418,513;
   measurements 305,623; file_versions 33,122; seed_vectors only 16,812 (chunk vectors are
   NOT the bloat). The prune deletes old-snapshot rows from the two ~2M-row edge tables —
   the transaction-size problem is per-snapshot edge facts × 29 snapshots. Chunk the
   prune AND question why 29 snapshots are retained on one repo (retention policy
   window vs today's relay-driven re-index cadence).

## 2. Contract

1. **Diagnose #2 FIRST, reproduced**: on an isolated root with a large repo index in
   flight (vcmi or linux), run `assess` (and `orient`, `check`) against a different,
   already-indexed repo; capture daemon-side timing per phase and thread state at the
   hang. State the mechanism with evidence. If it is a first-build certificate taking the
   write lock, the fix is in scope (build certs off the write slot or under the read
   guard with an honest "building" state); if it is resource starvation, the fix is a
   priority/yield mechanism for background resolve phases — STOP + DECISION_REQUIRED
   with options if it exceeds a bounded yield; if it is the connection cap, STOP + report.
2. **#1 seed publish yields**: publish vectors in bounded transactions (e.g. ≤N rows or
   ≤T ms per write-slot hold) with the slot released between chunks so a foreground open
   fits inside its 450ms patience; the FORGET-vs-SEED and generation-supersede race
   invariants (seed_pass.rs review-5 #2 / review-10 #3) MUST hold across chunk
   boundaries — a superseded generation abandons its remaining chunks, never mixes.
3. **#3 under the coordinator**: preload/refresh acquire the repo coordinator's refresh
   (writer) guard; readers bound to an epoch keep their graph until release (the W-B
   epoch invariants FROZEN).
4. **#4 reproduced, then chunked and guarded** (D2 Option C, 2026-09-05): FIRST reproduce
   retention on a representative multi-snapshot store (6–8 isolated re-indexes of a
   mid-size repo, or a snapshot-heavy fixture) and measure per phase (statement, table,
   rows/s, cache behaviour) — the FK-index premise is RETRACTED; fix ONLY the measured
   mechanism. THEN: the retention PRUNE runs
   as bounded transactions (≤N rows or ≤T ms of write-slot hold each; N/T stated and
   justified against the 450ms foreground patience), releasing the write slot between
   chunks so foreground reads interleave; `doctor` shows prune progress (rows done/total,
   elapsed) rather than a bare "started Nh ago". The VACUUM DELETE-mode window (when the
   reclaim threshold IS met) runs under the repo's writer guard and foreground opens
   during it receive the typed Busy naming the holder ("retention VACUUM"), never a raw
   SQLite error. The diagnosis also reports WHY 29 snapshots were retained on one repo
   (policy window vs re-index cadence) — a policy finding, not necessarily a fix here.
6. **Prevention set (human directive 2026-09-04 — "no future purge takes this long"):**
   (i) HARD CAP on retained snapshots per repo: current + its parent only — prune the
   previous history at index/refresh COMMIT time (prune-on-commit), so history never
   accumulates between passes; (ii) a retention TIME BUDGET: any pass over T (e.g. 60 s
   per repo) aborts at the next chunk boundary, records the overrun in `doctor`, and
   flips that repo to the REBUILD path on the next pass instead of retrying the delete;
   (iii) the rebuild path exposed as `rmap maintenance rebuild` for operators; (iv) a
   maintenance-connection `PRAGMA cache_size` sized to the store (not the 2 MB default);
   (v) `doctor` reports store size, snapshot count, prunable share, and last pass
   duration, with a stated threshold; (vi) a retention BENCHMARK GATE in the test suite:
   N snapshots × M rows must prune (or rebuild) under a fixed bound, so a regression to
   hours cannot ship silently.
5. Frozen invariants (survey §7, code-stated): single-writer per DB FIFO-fair; readers
   never observe partial writes (READY-snapshot filter + WAL); no head-of-line blocking;
   bounded concurrency with honest Busy; prompt shutdown; per-connection ordering;
   W-B epoch/coordinator semantics; wire protocol unchanged.

## 3. Stop conditions

Frozen: everything in §2.5; storage schema; exit codes; the 300s client timeout value
(a symptom threshold, not a knob to turn). STANDING HONESTY RULES (Busy is named, never
silent). Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state
root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproduction FIRST for #2 (the diagnosis is a deliverable even if the fix splits).
- Unit: chunked publish preserves supersede/forget races (extend seed_pass_tests);
  preload under coordinator excludes a concurrent reader swap; VACUUM window yields typed
  Busy.
- Live proof (isolated state root, registry sha unchanged): django-scale seed publish
  with a concurrent foreground `orient --full` loop — zero Busy over ≥20 attempts; the #2
  reproduction before/after; retention VACUUM with a concurrent read — named Busy or
  success, never raw error.
- Chunked cargo gates; consolidation witness; dogfood-isolated green; the concurrency
  test suites (`daemon-transport/tests/concurrency.rs`, `concurrency_dispatch.rs`) green.

## 5. Definition of done

Each of the four symptoms has its mechanism proven; #1/#3/#4 fixed under the frozen
invariants; #2 fixed or split with evidence; gates green.

CORPUS PATHS: django at ../legacy-codebases/django; vcmi at ../legacy-codebases/vcmi;
linux at ../legacy-codebases/linux; repo-graph is THIS repo.
