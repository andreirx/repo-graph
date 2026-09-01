# ORIENT-SMALL-ENRICH-1 — no tier promises figures that cannot rise

Status: SPECIFIED (2026-09-01) · Track: v0.13.0 audit queue #1 (trust-state defect per the
standalone second opinion). CODE slice, small. Maturity: MATURE (orient/check contracts).

## 1. Problem (measured — audit run 2026-09-01T09-06-40Z, 6 repos)

`orient --budget small` renders "Enrichment pass in progress — resolution figures may rise;
re-run when it completes" on repos whose own MEDIUM tier says "no semantic-resolution path
exists for C++ on this build" (leveldb captures one second apart; also django, nginx, poco,
sqlite, swupdate). A false expectation at the tier agents read first: the figures CANNOT
rise. Mechanism (SUSPECTED — verify first): OFC-1's `enrich_state_override =
Some(EnrichmentState::InFlight)` is injected from the daemon-global/auto-pass flight state
without consulting THIS repo's per-language enrichability; the small tier renders the
accessor's in-flight line where medium renders the materiality-gated no-path next-action.

## 2. Contract

1. **The in-flight fact is per-repo applicable before it renders.** A pass in flight for a
   repo with NO materially-present enrichable language must not render the may-rise promise
   anywhere. Fix at the honest layer (verify, then choose the smaller):
   (a) the dispatch injection site consults the SAME per-language capability facts (CS-1
   materiality gate / CHECK-SIGNAL-1 CeilingFact source) and injects the override only when
   ≥1 materially-present language is enrichable; or (b) the shared accessor's rendering
   composes in-flight WITH the capability fact ("enrichment pass running for other repos —
   does not apply to this repo's <langs>"). Prefer (a) if the pass genuinely cannot affect
   the repo (don't render a true-but-irrelevant daemon fact as repo state); record why.
2. **Tier consistency is the test**: all budget tiers + check render the SAME enrichment
   posture for the same snapshot instant (the OFC-1 §2.1 rule extended to this state).
3. **Also fix the naming drift found with it**: orient (all tiers) + stats say "no
   semantic-resolution path exists for C" on C++ repos while check/dead say "C/C++" — one
   phrasing from one source (the display-name set the CTA gate already owns).
4. JSON additive only; exit codes unchanged.

## 3. Stop conditions

Frozen: enrichment pass semantics/scheduling, the EnrichmentState sum type's variants (a
GATE on injection/rendering, not a new state — if a new variant is genuinely needed, STOP +
DECISION_REQUIRED), storage schema, exit codes, trust computation. STANDING HONESTY RULES.
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: no-path-repo fixture with a held in-flight pass → small tier
  renders the may-rise line (FAILS post-assert), then the fix → all tiers render the no-path
  posture; enrichable-repo fixture keeps the in-flight line on every tier (regression).
- Unit: the injection/render gate across the four capability cells; naming drift (one
  source, C/C++ display names).
- Live proof (isolated state root, registry sha unchanged): leveldb — capture small+medium
  during a real in-flight window (index a TS repo first so a pass is running daemon-wide):
  both tiers show the C/C++ no-path posture, no may-rise line. A TS repo shows in-flight on
  BOTH tiers during its own pass.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No repo is ever promised figures that cannot rise; the in-flight line renders only where the
pass can apply; tiers agree; C/C++ naming is uniform from one source; gates green.
