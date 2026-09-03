# FIND-EVIDENCE-1 — a fact row you can act on without a second call

Status: SPECIFIED (2026-09-03) · Track: zg head-to-head fix queue #1 (human-ratified
direction 2026-09-03: imitate the output shape — presentation IS usefulness). CODE slice,
presentational. Maturity: MATURE.

## 1. Problem (MEASURED — docs/audits/2026-09-03-zg-vs-rmap-find.md)

find's EVIDENCE dimension graded D+ against zg's B on identical ground truth, entirely on
presentation of facts we already store:
- Fact rows name file but not line (`CompactRange — db/db_impl.cc`, no `:582`) — every row
  costs an `rmap explain` round-trip for the one datum the agent came for.
- No row shows a line of source; zg's one doc-comment line (`"Prune the READY snapshots
  marked as prunable"`) answered a whole concept task outright.
- Cursor boilerplate (full absolute path + 26-char repo uid restated per row) measured at
  39–52% of output bytes across four captures.
- Scaffolding outranks the answer (rg-t1: types.rs and a re-export above prune.rs, the
  file that deletes).

## 2. Contract

1. **Line anchors.** Every symbol fact row renders `path:line` from the stored span; seeds
   (file-granularity today) render the file's path unchanged — no invented lines. A fact
   whose span is absent in the DB renders WITHOUT a line and that absence is visible (no
   0, no 1, no guess).
2. **One evidence line per symbol fact row**: the doc-comment first line if stored, else
   the signature if stored, else nothing (absence visible, never a fabricated preview —
   the zg arbitrary-line defect is the anti-pattern). Read from stored facts only; no
   file I/O at render time.
3. **Cursor diet.** Repo uid printed ONCE in a header; per-row explain cursors become
   relative (`explain <stable_key_suffix>` or equivalent that the daemon accepts) — the
   full form must remain obtainable (a `--full-cursors` flag or the JSON), because
   copy-paste executability is the point of cursors: whatever short form we print MUST
   run as printed from the repo cwd. If a runnable short form is impossible without
   daemon-side changes, a daemon-side additive alias is IN SCOPE (precedent chain).
4. **Ranking nudge, minimal form:** within a fact class, rows WITH a doc-comment/signature
   do not rank below rows without one when lexical relevance ties. No new scoring model —
   tie-break only. (The rg-t1 scaffolding inversion; anything deeper is out of scope.)
5. JSON: additive fields only (`line`, `evidence`, header uid); existing fields unchanged;
   exit codes unchanged. Byte-size regression check: total output for the audit's capture
   set must SHRINK on average (economy is a goal, not a casualty).

## 3. Stop conditions

Frozen: find's match semantics, seeds computation, exit codes, storage schema (reads
only). STANDING HONESTY RULES (no invented lines/snippets; absent facts visibly absent).
New public APIs beyond the optional daemon-side cursor alias → DECISION_REQUIRED. Unmet
DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: row with span+doc → `path:line` + evidence line; row missing span → no line, no
  invention; cursor header + short cursors; tie-break test.
- Live proof (isolated state root, registry sha unchanged): re-run the audit probes
  `find CompactRange` (leveldb), `find ConversationManager` (FRAKTAG), `find
  witness_epoch` (repo-graph) — report before/after outputs and byte counts; a printed
  short cursor is EXECUTED verbatim and must resolve.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

An agent can open the right file at the right line from any symbol fact row without a
second tool call; evidence lines come only from stored facts; output bytes go down; the
short cursors run as printed; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; repo-graph
is THIS repo.
