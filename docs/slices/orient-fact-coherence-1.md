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

## 2. Contract

1. **One computation per fact, per snapshot.** The reliability figures (resolved %, call
   totals, unclassified counts), the enrichment-state line, and the enrichment CTA render
   IDENTICALLY for a given snapshot across `orient` at every budget, `orient --full`, and
   `check`. Budgets may cut SECTIONS; they must never change a rendered NUMBER or claim.
2. **Fix rides the existing per-leaf serve-decision seam.** First VERIFY the mechanism (a
   probe reproducing the divergence in a test, naming the actual serving route per side).
   Then: the leaf(s) carrying reliability/enrichment-derived facts must either (a) prove
   enrichment-epoch coherence in their EXISTING per-leaf decision (serve only when the
   resident data reflects the same enrichment epoch SQLite would serve), or (b) always serve
   those facts from SQLite (they are cheap aggregate counts) with the existing SQLite leaf
   label. Choose the smaller change that satisfies §2.1; record which and why.
3. **Freshness labeling**: if any surface can still legitimately render an older epoch (it
   should not, after §2.2), it must say so explicitly — but the DEFAULT outcome of this slice
   is identical numbers, not labeled divergence.
4. **The CTA follows the coherent fact.** With enrichment executed, no budget tier may emit
   "run `rmap enrich`" for already-resolved families; the CTA logic (CONTRADICTION-SWEEP-1's
   per-language truthful line) must consume the SAME coherent enrichment state.

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
