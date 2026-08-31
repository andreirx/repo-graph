# ORIENT-FACT-COHERENCE-1 — one snapshot, one reliability story, at every budget

Status: SPECIFIED (2026-08-31) · Track: Usefulness audit v0.11.0 fix queue, item #1 (standalone
reviewer's top priority). CODE slice. Maturity: MATURE surfaces (orient/check contracts).

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z, FRAKTAG)

Same snapshot (9bea3a5), seconds apart:
- `orient --budget small|medium|large`: "your code's calls **28%** resolved … 904 of these
  **1609** … run `rmap enrich` — resolves TypeScript"
- `orient --full` and `check --full`: "**31%** resolved … 904 of these **1685** …
  ENRICHMENT_STATE: Enrichment phase executed."

Budget changes facts — the cardinal presentation rule — and the stale side carries an
agent-wasting instruction (run an enrichment that already ran). SUSPECTED mechanism (verify
first, do not assume): budgeted orient serves leaves from the resident LiveGraph when the
per-leaf serve witness is GREEN (`handle_orient`, dispatch.rs ~4150-4230), while `--full`/check
read SQLite; the enrichment pass promotes resolutions into SQLite, so a resident graph built
BEFORE the pass serves pre-enrichment reliability counts. The witness certifies its leaves'
no-loss equality — reliability/enrichment-state evidently is not among the facts it certifies
against enrichment epochs.

## 2. Contract — AMENDED (operator ruling OFC1-MECHANISM-REFUTED = D-then-B, 2026-08-31)

**§1's suspected serving-route mechanism is REFUTED** (builder investigation cycle-1,
reviewer-corroborated: `OrientServeDecorator::get_trust_summary` delegates to SQLite on every
path; budget cuts only module/complexity depth) **and the measurement is re-confirmed as a
TEMPORAL race** (operator, capture mtimes: budgets at 23:55:07-11 pre-pass, --full/check at
23:55:13-14 post-pass — the enrichment pass completed in between). Budgets never changed facts.
The re-scoped contract:

1. **In-flight enrichment renders as in-flight.** While an enrichment pass for this repo is
   QUEUED or RUNNING, every surface that consumes the shared enrichment-state accessor
   (CONTRADICTION-SWEEP-1: orient/check/trust) renders that truth — e.g. "enrichment pass in
   progress — resolution figures may rise; re-run when it completes" — and the per-language
   enrich CTA is SUPPRESSED for the duration (never "run `rmap enrich`" while it runs; never
   "Enrichment phase did not run" while it is queued/running).
2. **The fix lives in the EXISTING shared accessor** (`check::enrichment_state_summary`) and
   the daemon fact it reads: extend the enrichment-state fact with the queued/running states
   the daemon already tracks (doctor's activity line proves the knowledge exists). No new
   accessor, no serving-route change, no new surface.
3. **check's ENRICHMENT_STATE follows**: queued/running is its own honest non-failing form
   (parallel to leveldb's "No eligible edges" pattern), not "did not run".
4. JSON additive only; exit codes unchanged.
## 3. Stop conditions

Frozen: the no-loss certificate computation and witness semantics (consuming an existing
epoch/fingerprint fact in a per-leaf DECISION is in scope; changing what the certificate
PROVES is not — if §2.2(a) requires extending certification, take (b) instead; if neither
works without touching frozen surfaces, STOP + DECISION_REQUIRED). Frozen: storage schema,
exit codes, trust computation, enrichment pass semantics. STANDING HONESTY RULES. Unmet DoD →
STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- A reproducing test FIRST: index a fixture, run enrichment, force the divergence the audit
  measured (budgeted vs full reliability figures) — must FAIL before the fix, PASS after.
- Unit: budget tiers render byte-identical reliability/enrichment-state/CTA blocks for the
  same snapshot; post-enrichment budgeted orient never emits the enrich CTA for resolved
  families; leaf-decision fallback path labeled per its existing contract.
- Live proof (isolated state root, registry sha unchanged): reproduce the FRAKTAG shape —
  index, let enrichment complete, capture `orient --budget medium` vs `orient --full` vs
  `check --full`: identical percentages, totals, enrichment state, and CTA. Captures in the
  report. (LM Studio is live on this machine — disable auto-seed in any test harness that
  indexes; see the standing lock-flake guard.)
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No rmap surface tells two factual stories about one snapshot: reliability, enrichment state,
and the CTA are identical at every budget and on check; the reproducing test pins it; the
serving-route decision that caused the split is documented in the fix; gates green.
