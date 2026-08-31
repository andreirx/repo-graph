# FIND-RANK-1 — find's cap slots go to the symbols people are looking for

Status: SPECIFIED (2026-08-31) · Track: Usefulness audit v0.11.0 fix queue, item #5 (final
top-tier). CODE slice, ranking/presentation. Maturity: find is fresh (FIND-FACTS-1).

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z)

- `django find formset`: all 8 displayed symbol hits are `tests/...` VARIABLES
  (`AbsoluteMaxFavoriteDrinksFormSet` first, alphabetical); `BaseFormSet` — the reason anyone
  types "formset" — is invisible behind `(+192+ more)`. Same shape on glamCRM ("offer" →
  `ABSENT_OFFER_ID` test constant leads). Alphabetical order under the per-class cap wastes
  every slot on test noise.
- The cap size is never named; `(+192+ more)` renders an unexplained lower bound ("+N+").
- FRAKTAG "woocommerce": after an honest all-class no-match, the seed tier serves 10
  irrelevant seeds at 0.50-0.54 with no floor or abstain — hearsay padding an empty answer.

## 2. Contract

1. **Deterministic rank within the symbol class** (facts tier stays deterministic — ordering
   is part of the contract, documented): (a) non-test before test (the stored is_test fact of
   the defining file — never path strings); (b) kind weight: type-defining and callable
   symbols (class/interface/enum/struct/trait/function/method) before
   variables/constants/properties; (c) match quality: exact name match, then prefix, then
   substring; (d) shorter qualified name before longer; (e) path ASC as the final
   deterministic tiebreak. Other classes keep their current order unless the same test-last
   rule trivially applies (files: non-test first, same basis).
2. **The cap is named and exact**: `showing 8 of 200 — --full for all` (real numbers; the
   `matched` count is already exact per FIND-FACTS-1's `matched_is_floor` handling — where a
   floor, say `at least N`, never `+N+`).
3. **Seed tier floor + abstain**: seeds below a fixed similarity floor (pick from the spike
   data — e.g. 0.60 — and RECORD the basis; a pinned constant, not adaptive) are not
   rendered; when ALL seeds fall below the floor, the tier renders the honest abstain:
   "no seeds above the similarity floor (best: 0.54) — the concept may not have a distinct
   home in this repo." Never 10 rows of 0.50 hearsay after a no-match.
4. **Unknown is_test on a hit**: ranks in the non-test partition (never demoted on unknown —
   the FIXTURE-POLLUTION-1 direction rule) and carries no marker in find (density; the
   is_test fact is a ranking input here, not a rendered claim).
5. JSON: additive rank metadata only if needed; existing fields unchanged; exit codes
   unchanged; `--exact` determinism preserved.

## 3. Stop conditions

Frozen: the facts-tier corpus (fact classes; FIND-FACTS-1 contract), seed pins/formula
(the FLOOR is presentation-side filtering, not a formula change), storage schema, exit
codes. STANDING HONESTY RULES. New public APIs beyond additive DTO fields →
DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: the rank comparator (each rule pair-tested + full determinism property); cap line
  wording with exact and floor counts; seed floor + abstain rendering; unknown-is_test
  partition placement.
- Live proof (isolated state root, registry sha unchanged): django `find formset` →
  `BaseFormSet` (or the true core symbols) in the visible 8, test variables demoted;
  glamCRM `find offer` → production symbols lead; FRAKTAG `find woocommerce` → abstain line,
  zero sub-floor seeds. Before/after captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

The visible slots answer the query: core production symbols first, test noise last, the cap
stated with real numbers, and the seed tier abstains instead of padding; determinism
documented and tested; gates green.
