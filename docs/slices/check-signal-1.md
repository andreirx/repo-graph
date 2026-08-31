# CHECK-SIGNAL-1 — check's verdict discriminates again: unreachable ceilings are not failures

Status: SPECIFIED (2026-08-31) · Track: Usefulness audit v0.11.0 fix queue, item #4. CODE
slice. Maturity: MATURE (check is CI-facing; verdict→exit-code MAPPING frozen).

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z)

All six graded repos land FAIL or INCOMPLETE at Fresh because `CALL_GRAPH_RELIABILITY` is
always LOW — exit-2 is ambient, not signal; an agent (or CI) learns nothing from check's
verdict because it is the same everywhere. Two distinct situations are conflated:
- **Enrichable-but-unenriched** (TS repo, enrichment available/not run): LOW is actionable —
  a failing condition with the CTA is CORRECT.
- **No-path ceiling** (C/C++/Python on this build: no resolver exists): LOW is PERMANENT —
  nothing the reader does changes it; "Incomplete" falsely implies actionability, forever.
Compounding: django's `ENRICHMENT_STATE: Enrichment phase did not run.` renders as a FAILING
condition where no Python path exists, while leveldb already gets the honest passing form
("No eligible edges for enrichment").

## 2. Contract

1. **Split the conflation by the resolver-path fact** (the SAME per-language capability facts
   the CTA logic and dead-causes use — one source of truth, never re-derived):
   - Materially-present language(s) WITH a resolution path, enrichment not yet
     applied/complete → `CALL_GRAPH_RELIABILITY` keeps its CURRENT failing classification
     (Fail today — ruling actionable-verdict 2026-08-31: current behavior is authoritative;
     this slice does not reclassify the actionable cell) with the per-language CTA.
   - ALL materially-present languages WITHOUT a resolution path on this build → the condition
     renders as a PASSING stated limitation: "call-graph resolution has reached this build's
     ceiling for <langs> (no resolver exists) — <N>% resolved is the deterministic-extraction
     figure; verify call/dead claims against source." The reliability FIGURES render
     unchanged (Layer honesty); only the verdict contribution changes.
   - Mixed repos: the enrichable side governs (actionability exists → the current failing
     classification + CTA naming
     only the enrichable languages, per the CS-1 materiality gate).
2. **ENRICHMENT_STATE follows the same fact**: no-path languages get leveldb's honest
   non-failing form (never "did not run" as a failure); in-flight keeps OFC-1's form;
   enrichable-and-not-run keeps the failing form WITH the CTA.
3. **Verdict→exit-code mapping untouched** (frozen). Repos whose verdict legitimately moves
   (e.g. leveldb Incomplete→Pass when its only conditions were no-path ceilings) are the
   POINT — record the expected movement for the six audit repos in the report (predicted vs
   measured). check's JSON: condition entries additive (`ceiling: true` marker); STALE-era
   deprecations unaffected.
4. Contract doc updated in-slice (check's CI contract: what Pass-with-limitation means).

## 3. Stop conditions

Frozen: verdict→exit-code mapping, exit codes, storage schema, trust computation, the
reliability FIGURES themselves (only the condition classification changes), LiveGraph/
witness. STANDING HONESTY RULES. If the per-language capability facts are not reachable at
check's evaluation site without a new public API, STOP + DECISION_REQUIRED. Unmet DoD →
STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: the four cells (no-path only / enrichable only / mixed / in-flight) × verdict
  contribution + wording; figures unchanged; JSON additive marker; TD-flake note:
  this slice touches the same next-action inputs as TD's d5 flake — run
  honest_degradation_impl2 5× as part of validation.
- Live proof (isolated state root, registry sha unchanged): leveldb (pure C++ →
  ceiling Pass form), django (pure Python → ceiling Pass + honest ENRICHMENT_STATE),
  glamCRM (mixed → current failing classification with TS-only CTA), FRAKTAG post-enrichment (whatever its
  true state now yields). Predicted-vs-measured verdict table for all four.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

check's verdict varies with reality: permanent ceilings render as stated-limitation passes,
actionable gaps keep their current failing classification with their CTA, and the same capability fact drives the
condition, the CTA, and ENRICHMENT_STATE; figures untouched; mapping frozen; gates green.
