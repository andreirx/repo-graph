# DAEMON-RESIDUALS-3 — seed publish yields; LiveGraph under the coordinator; VACUUM guarded

Status: SPECIFIED (2026-09-05) · Track: split from DAEMON-RESIDUALS-1. CODE slice.
Maturity: MATURE.

## 1. Problem (MEASURED / SURVEYED 2026-09-04)

1. django `orient --full` → Busy during the chunk-seed publish (78,214 vectors):
   `publish_guarded` holds the DB write slot longer than the 450 ms foreground patience.
2. TECH-DEBT #2b: `handle_livegraph_preload`/`handle_livegraph_refresh` mutate the
   LiveGraph with NO repo coordinator guard — can swap the graph under live readers.
3. Retention's VACUUM `journal_mode=DELETE` window suspends WAL's reader-non-blocking
   guarantee unguarded.

## 2. Contract

1. Seed publish in bounded transactions (≤N rows / ≤T ms per write-slot hold; N/T stated
   against the 450 ms patience), slot released between chunks; the FORGET-vs-SEED and
   generation-supersede invariants hold across chunk boundaries (a superseded generation
   abandons its remaining chunks).
2. Preload/refresh acquire the coordinator's refresh guard; epoch-bound readers keep their
   graph until release (W-B invariants frozen).
3. The VACUUM window runs under the repo's writer guard; foreground opens receive the
   typed Busy naming "retention VACUUM"; bounded duration reported in doctor.
4. Frozen invariants as DAEMON-RESIDUALS-1 §2.5; wire protocol unchanged.

## 3. Stop conditions

Frozen invariants; storage schema; exit codes. STANDING HONESTY RULES. Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: chunked publish preserves supersede/forget races (extend seed_pass_tests);
  preload under coordinator excludes a concurrent reader swap; VACUUM window yields the
  named Busy.
- Live proof (isolated state root, registry sha unchanged): a django-scale seed publish
  with a concurrent foreground `orient` loop — zero Busy over ≥20 attempts; retention
  VACUUM with a concurrent read — named Busy or success, never a raw error. Gates first,
  proofs small; delete every isolated root.

## 5. Definition of done

Seed publishes never exceed the foreground patience; LiveGraph mutations are coordinated;
the VACUUM window is guarded and named; gates green.

CORPUS PATHS: django at ../legacy-codebases/django; repo-graph is THIS repo.
