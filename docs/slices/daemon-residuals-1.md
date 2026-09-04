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
   getPage` with a 344 MB WAL still growing — i.e. the PRUNE (row deletes across old
   snapshots) runs as ONE multi-hour transaction under the write lock, not a brief VACUUM
   window. Contract addendum for #4: prune work must be CHUNKED (bounded rows/time per
   transaction, write slot released between chunks, progress visible in `doctor` with an
   ETA basis) so foreground reads interleave; and a store this size on a single repo is
   itself a finding (which tables? snapshot count? seed_vectors per snapshot?) — the
   diagnosis must report the size composition.
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
4. **#4 guarded**: the VACUUM DELETE-mode window runs under the repo's writer guard and
   foreground opens during it receive the typed Busy with the holder named ("retention
   VACUUM"), never a raw SQLite error; bounded duration reported in doctor.
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
