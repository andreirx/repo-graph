# QUANT-MECH-1 — churn ranks by commits, quantity surfaces get budgets, stats says each thing once

Status: SPECIFIED (2026-08-30) · Track: Usefulness audit v0.9.0 fix queue, item #10 (final).
CODE slice, presentation/query-layer. Maturity: MATURE surfaces.

## 1. Problem (measured — audit §10; re-verified 2026-08-30)

1. **churn sorts by `lines_changed`** at BOTH layers (`git/src/churn.rs:215`,
   `rgr/src/presentation/churn.rs:68`). Bulk edits, renames, and generated files outweigh
   sustained change: on the audit corpus the true churn leader (most commits) ranked 15th.
   Churn's question is "what keeps changing", and commit count answers it; line volume is the
   tiebreaker, not the signal.
2. **churn and hotspots are unbudgeted** (452–561 rows in the audit runs; churn's renderer
   comment says "No truncation. Caller can pipe to head") — the exact silent-wall-of-rows
   shape the audit standard replaced with budget + explicit remainder everywhere else.
3. **stats prints the same ~50 directories five times** (audit-measured) — five sections each
   re-listing the directory population instead of one population + per-section deltas.

## 2. Contract

1. **churn ranking: `commit_count` DESC, then `lines_changed` DESC, then path ASC** — changed
   at the computation layer (`git/src/churn.rs`) and mirrored in the renderer; both columns
   remain rendered. This is a deliberate, audit-ratified BEHAVIOR change: record it in the
   contract doc; JSON array order follows the new sort (no field changes). If a consumer
   named in `docs/architecture/gate-contract.txt` depends on the old order, STOP +
   DECISION_REQUIRED.
2. **Budgets on churn + hotspots** per the house standard: default cap (25 rows), explicit
   `(+N more — --full)` remainder line (never silent), `--full` uncapped. Exit codes and JSON
   contracts unchanged (JSON stays complete — budgets are a HUMAN-render concern; if JSON is
   currently unbounded that is its contract, leave it).
3. **stats de-dup**: each fact rendered once. The directory population renders once; sections
   that today re-list it render only their per-section values keyed against that single list
   (or a compact table with one row per dir and one column per section — whichever is the
   smaller change). No metric is dropped; the information content is identical or better
   (measured: the same ~50 dirs must appear exactly once). If stats' structure cannot be
   de-duped without changing its JSON, the JSON stays as-is (human render only).

## 3. Stop conditions

Frozen: exit codes, JSON field shapes (order change in churn's array is the one ratified
exception), storage schema, trust, LiveGraph/witness, hotspot scoring formula, churn's git
computation semantics (what is counted — only the SORT changes). STANDING HONESTY RULES.
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: churn comparator (commit-leader outranks line-leader; determinism ties); budget
  remainder lines exact; --full uncapped; stats fixture where the old render repeated a dir
  N times → new render has it exactly once with all N sections' values present.
- Live proof (isolated state root, registry sha unchanged): a validation repo where the
  commit-leader ≠ line-leader (django or glamCRM — verify from git log and show the audit's
  ranked-15th case fixed if reproducible); churn/hotspots row counts at default ≤ budget with
  explicit remainder; stats before/after capture showing single directory listing.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Churn's leader is the file that keeps changing, not the file that once changed hugely; no
quantity surface renders an unbudgeted wall; stats states each directory once; contract docs
record the ratified sort change; gates green.
