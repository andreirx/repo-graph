# ECONOMY-2 — signal per byte on the surfaces that spend the most

Status: SPECIFIED (2026-09-05) · Track: v0.16.0 audit queue #8 (human-ratified). CODE
slice, presentation. Maturity: MATURE.

## 1. Problem (MEASURED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

1. **Seed cursors are the cost center**: on seed-bearing `find` output, cursor
   boilerplate (full absolute path + 26-char repo uid per row) measures 30–53% of bytes;
   FIND-EVIDENCE-1's diet reached only the fact tier (2.2–2.8% reclaimed). `--text` is
   the one route with zero cursor overhead.
2. **`orient --full` is uncapped and mislabels truncation**: gstreamer 314,730 B (98.6%
   package-group/complexity rows) closing with `[budget not reached — output complete]`;
   zvec-grep `--full` is byte-identical to `--budget large` with no marker; `--budget
   large` shows 51 of 701 groups with no marker at all; the marker appears on 5/11 repos
   and on django while eliding 673 of 685 groups.
3. **`map --dry-run` is unusable at scale**: median 1.17 MB, max 32.2 MB (hadoop),
   gstreamer 254,685 lines; no cap, no count header, no `--limit` hint.

## 2. Contract

1. **Seed-row cursor diet, same discipline as the fact tier**: repo uid once in the
   seeds header; per-row cursor is the runnable short form (CURSOR-ROUNDTRIP-1's helper
   accepts it everywhere) — `explain <path#symbol:KIND>`; the absolute `cd … &&` prefix
   is dropped when the row's path is inside the current repo root (the common case) and
   kept only for out-of-root rows. JSON keeps the full canonical cursor (`cursor_raw`).
   Measured target: cursor bytes ≤ 15% of a seed-bearing `find` output on the audit
   probes; report before/after per probe.
2. **`--full` means complete or says what it elided** — one truthful ladder: every
   list that can be elided carries either all rows or an explicit `… and N more <kind> —
   <where to get them>` line; `[budget not reached — output complete]` renders ONLY when
   nothing was elided (assert by test on the seg2 `budget_saturated` signal); `--full` on
   a repo where `large` already showed everything renders a one-line "identical to
   --budget large (nothing further to show)" instead of silently repeating.
3. **`--full` caps the long tails honestly**: package-group and complexity-center rows
   cap at a stated bound (e.g. 200 rows) with the elision line naming the exact command
   for the rest (`stats --json` / `hotspots`); no unbounded 300 KB dumps.
4. **`map --dry-run` gets a count header and a cap**: header "N files would be mapped,
   M sidecar files, K bytes"; body capped at a stated line bound with `--limit <n>` /
   `--full` documented in the elision line; bytes-per-file summary retained. Byte
   measurements before/after on hadoop/gstreamer/zvec-grep.
5. No fact changes; JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: facts, ranking, seeds computation, cycle/orient semantics, exit codes. STANDING
HONESTY RULES (an elision line always states N and where). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: seed cursor short form + out-of-root prefix retained; `budget_saturated` ↔ marker
  equivalence; elision lines carry N + destination; map header/cap.
- Live proof (isolated state root, registry sha unchanged): `find` seed probes (leveldb
  obsolete-files, repo-graph retention) — cursor bytes before/after and the ≤15% target;
  `orient --full` on gstreamer (bytes before/after; no false "complete"), zvec-grep
  (`--full` vs `large` identical → the one-line notice); `map --dry-run` on hadoop +
  gstreamer (header, cap, bytes). Byte table per surface.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Seed-bearing output spends ≤15% of bytes on cursors while every cursor still runs; no
output claims completeness while eliding; long tails are capped with a truthful line
naming the rest; map dry-run is bounded and counted; gates green.

CORPUS PATHS: gstreamer at ../legacy-codebases/gstreamer; hadoop at
../legacy-codebases/hadoop; zvec-grep at ../legacy-codebases/zvec-grep; leveldb at
../legacy-codebases/leveldb; repo-graph is THIS repo.
