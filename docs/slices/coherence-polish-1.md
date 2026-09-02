# COHERENCE-POLISH-1 — three places where two surfaces say different words for one fact

Status: SPECIFIED (2026-09-02) · Track: v0.13.0 queue tail (bundled: each item is a
one-fact/two-wordings defect; none exceeds a sentence-level change). CODE slice, small.
Maturity: MATURE.

## 1. Problems (measured — audit 2026-09-01T09-06-40Z)

1. **FUNNEL-VOCAB**: doctor's promotion funnel gate lines and its rejection reasons use
   irreconcilable vocabularies (amodx: gate "receiver type is defined in this repo … 0
   filtered out here" beside top rejection "receiver type isn't a type defined in this
   repo: 758" — those 758 were actually filtered at the "maps to exactly one class" gate).
   An agent cannot tell which gate rejects.
2. **TRUST-CEILING echo**: trust frames "34% resolved (below 50% target)" with no ceiling
   statement on repos where check says the ceiling is reached — implying an unimprovable
   number can improve. trust must consume the SAME CeilingFact (CHECK-SIGNAL-1) and render
   the ceiling posture in its call-graph section.
3. **Governance wording**: (a) gate's not-armed line never states the arming division — one
   clause: boundaries are checked by `rmap violations`, requirements/quality-policies arm
   the gate; (b) `violations` says "0 discovered module violations" while `modules
   violations` says "0 violations" — align the noun phrases (one vocabulary).

## 2. Contract

1. Funnel: ONE vocabulary, gate-anchored — every rejection reason string IS its gate's name
   (or a documented 1:1 mapping rendered next to the gate); the sum invariant (reached =
   passed + filtered, per gate) asserted by test. Numbers/semantics unchanged — wording and
   attribution only. If reasons genuinely attach to a different gate than their text implies,
   fix the ATTRIBUTION to the true gate (that is the honesty fix, not a wording swap).
2. Trust consumes CeilingFact (existing injected fact — same route as check) and renders
   the ceiling sentence where applicable; "below N% target" is suppressed for at-ceiling
   languages (a target that cannot be approached is not a target).
3. Governance: the arming-division clause on gate's not-armed line; noun-phrase alignment
   across the two violations surfaces. Byte-changes to these lines only.
4. JSON additive only (funnel reason/gate mapping if needed); exit codes/verdicts frozen.

## 3. Stop conditions

Frozen: funnel computation/promotion semantics, trust ratio computation, verdict mapping,
exit codes, storage schema. STANDING HONESTY RULES. New public APIs beyond additive DTO
fields → DECISION_REQUIRED (injected-fact precedent citable for trust's CeilingFact).
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: funnel reason↔gate attribution + sum invariant; trust ceiling rendering per
  capability cell (at-ceiling / actionable / mixed / unknown); gate arming clause;
  violations noun alignment.
- Live proof (isolated state root, registry sha unchanged): amodx doctor — funnel reasons
  attributed to their true gates; leveldb trust — ceiling sentence present, no "below
  target"; repo-graph gate not-armed line carries the division clause. Captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

The funnel's reasons name their true gates and sum; trust never implies an unimprovable
number can improve; gate says who arms what; the violations surfaces share one vocabulary;
nothing else moves; gates green.
