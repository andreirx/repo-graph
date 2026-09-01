# CHURN-SHALLOW-1 — history commands state what the history actually is

Status: SPECIFIED (2026-09-02) · Track: diverse-verification + v0.13.0 queue (merges
CHURN-DIAGNOSE-1). CODE slice. Maturity: MATURE (churn/hotspots).

## 1. Problem (measured — two shapes)

1. **Degenerate history MISLEADS** (VCMI, depth-1 clone): churn asserts "File Churn (last 90
   days) / 2072 files changed" from ONE commit — the entire tree counted as churn; hotspots
   ranks on it. Confidently framed, wrong.
2. **Determinable cause HEDGED** (django/leveldb/FRAKTAG, stale clones): "0 files changed /
   hint: no files changed in the 90-day window, or no git history available" — the tool has
   the repo open and can say WHICH; the hedge cascades into hotspots and risk.

## 2. Contract

1. **Diagnose the history before rendering counts** (cheap deterministic git facts at query
   time: commit count in window, total commit count reachable, shallow marker
   (`.git/shallow` / `git rev-parse --is-shallow-repository`), HEAD commit date):
   - **Shallow or single-commit history**: churn renders the honest state — "history is
     shallow (N commit(s) available; clone depth limits churn) — counts below reflect only
     that history" — and hotspots carries the same caveat on its ranking; NEVER frame a
     whole-tree initial commit as 90-day churn (either exclude root-commit file counts from
     the window claim or label them as the initial import; pick the honest smaller change
     and record it).
   - **Zero in window, real history**: "no files changed in the last 90 days (HEAD commit:
     <date>) — try --since <suggested>" with the suggestion derived from the HEAD date. The
     either/or hedge dies.
   - **No git history at all**: say exactly that.
2. **Cascades state their input**: hotspots/risk zero/degenerate states name the churn
   diagnosis they inherit (one sentence, not a re-derivation — consume the same fact).
3. **Git read failures**: unknown-with-reason (never a guessed state).
4. JSON additive (history diagnosis block); exit codes unchanged; churn computation frozen
   (what is counted; only the FRAMING and the degenerate-state handling change — if
   excluding the root commit from window counts requires computation change, that is the
   recorded honest-smaller-change decision, additive and test-pinned).

## 3. Stop conditions

Frozen: exit codes, storage schema, hotspot scoring formula, the commits-first sort
(QUANT-MECH-1). STANDING HONESTY RULES. New public APIs beyond additive DTO fields →
DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: the four history cells (shallow/single-commit, zero-in-window-with-history,
  no-history, healthy) × churn framing + hotspots/risk inheritance; git-failure
  unknown-with-reason; --since suggestion derivation.
- Live proof (isolated state root, registry sha unchanged): VCMI (shallow) — churn states
  the shallow history and stops claiming 90-day churn for the initial import; django
  (stale) — HEAD date + --since suggestion; repo-graph (healthy) — unchanged framing.
  Captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Churn never presents an initial import as recent change; determinable causes are stated, not
hedged; cascading surfaces name their inherited diagnosis; healthy repos unchanged; gates
green.
