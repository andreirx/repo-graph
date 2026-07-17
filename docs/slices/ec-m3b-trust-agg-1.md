# EC-M3B-TRUST-AGG-1 — persist the g1 resolved-call aggregate; trust core reads it (EC-1 milestone M-3b)

Status: SPECIFIED (2026-07-17) · Track: Consolidation milestones (EC-1 §5.2 M-3b; predicate C-4's last eager FC2a scan)
Depends: M-0 (done), M-1 (done — the consolidation witness is live in CI; this slice pays its
integration there explicitly). HARD ordering: M-3b ≺ M-6's first drop.

## 1. Problem

Five surfaces (trust, check, orient, explain, stats) consume the snapshot resolved-call
count through ONE trust core, which today derives it by an eager read-time
`count_edges_by_type(CALLS)` scan [service.rs:875]. After any per-language CALLS-row drop
(M-6) that COUNT silently undercounts. The persisted full-stream aggregate is the only
honest source.

## 2. Contract (EC-1 §5.2 M-3b row, as ratified + amended)

1. **WRITE (g1):** index AND refresh persist the snapshot-level resolved-call count,
   computed from the FULL resolution stream (all languages — FC0 input, D-EC-2-A),
   including delta-refresh copy-forward. Full Persistence Completeness checklist: write
   path / read path / refresh behavior / trust impact / CLI visibility / validation.
   **Interim rule (ratified, D-EC-1/D-EC-7 supersession (c)):** the persisted value is
   PIPELINE-derived (one coherent accounting, matching the trust denominator) with an
   explicit provenance label; this accounting is EXPLICITLY TEMPORARY until the
   reconciliation layer ships (recon-design-1).
2. **READ:** the ONE shared trust core swaps `count_edges_by_type(CALLS)`
   [service.rs:875] for the persisted aggregate — trust, check, orient, explain, stats
   inherit through `assemble_trust_report`/`get_trust_summary` [agent_impl.rs:326-344];
   zero per-surface work. FC2b-derived module stats/cycles stay read-time owner reads
   (D-EC-5-A — NO migration here).
3. **Trust posture unchanged:** the two-source hybrid (Half-A/Half-B) untouched — only
   the v1 report's `resolved_calls` INPUT changes source. Honesty rules: a snapshot
   without the persisted aggregate (pre-migration snapshot) must be handled explicitly —
   fall back to the live COUNT while CALLS rows exist, labeled; never fabricate, never
   collapse unknown to zero.
4. **Scoping note (recorded deviation from the M-3b row's parenthetical):** the g2
   per-function degree family write MOVES to M-3a, where its consumers swap in the same
   slice — per the ratified deep-vertical directive (no dormant capability; a written
   family nothing reads is the field-bug factory). M-3a's own row already names its
   producer for that family; no ratified decision changes.
5. **The M-1 witness stays green** — any manifest change this slice needs (fact-class
   declarations for touched arms) is made explicitly and reviewed; that is the ONE
   integration by construction.

## 3. Stop conditions

- Frozen areas: W-B epoch/coordinator invariants, activity-registry semantics,
  enrich_pass semantics, postpass/extractor walks.
- If persisted-vs-live parity FAILS on any fixture (the self-validation), that is a
  FINDING (evidence + DECISION_REQUIRED) — do not paper over it.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- **Parity window (self-validating):** while CALLS rows exist, the persisted count MUST
  equal the live COUNT — asserted on fresh index AND refresh (delta copy-forward path
  exercised, not just fresh).
- Byte-compare trust/check/orient/explain/stats on fixtures (before/after — outputs
  identical while parity holds).
- Pre-migration-snapshot fallback path tested (old snapshot, no aggregate → labeled live
  COUNT, not a fabricated 0).
- Cargo gates chunked (standing pattern); consolidation witness 15/15; smoke via
  `dogfood-isolated.sh` if runnable in-sandbox, else recorded NOT RUN with reason.

## 5. Definition of done

The trust core's `resolved_calls` is served from the persisted, provenance-labeled,
full-stream g1 aggregate on both fresh and refreshed snapshots; parity green; the five
surfaces byte-identical; the eager read-time CALLS COUNT no longer on the serving path
(fallback only for pre-migration snapshots); witness green; gates green.
