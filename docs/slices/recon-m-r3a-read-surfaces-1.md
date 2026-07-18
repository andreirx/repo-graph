# RECON-M-R3A-READ-SURFACES-1 — divergence posture + union accounting read surfaces (reconciliation IMPL milestone M-R3a)

Status: SPECIFIED (2026-07-18) · Track: Reconciliation IMPL (recon-design-1 §6.1, ratified §8)
Depends: M-R1 (c0e1dad), M-R2 (c202279). M-R2 ∥ M-R3a — both consume the M-R1 ledger.

## 1. Contract — the recon-design-1 §6.1 **M-R3a row IS the binding contract**, verbatim

Divergence posture + the union accounting's read surfaces: the trust `witnesses` block, the
doctor operational block, orient/stats g1u lines, g2u liveness/degree overlays, g3u sketch
pairs (§5.3.2-4, §5.4) — all through ONE shared projection. This INCLUDES the
escalate-deferred `identity_collision` rendering (trust block + doctor; the recorded M-R1
gate amendment bad69da moved it here). The union accounting NEVER touches the M-3a/M-3b
persisted pipeline accounting or its write path (§5.3 — no new coupling); the trust ratio's
denominator remains the pipeline-only floor.

## 2. Gate — the M-R3a row's gate column, verbatim

§5.3.1 invariance + accounting-label tests; zero-SCIP absence (R-0: the blocks/overlays
absent or explicitly n/a, never zeros) + mixed-repo scoping (R-1) tests; W-ONE
REASON-RENDERING tests (three reasons → three distinct posture lines + next actions; stale
≠ "available but not loaded"; the stale∧producer-absent compound renders its blocker);
doctor's ledger-ABSENT rendering (last capture outcome + build-failure reason; trust
renders unknown, never a stale number); deterministic ordering; RECORDS the measured g3u
pair delta (§5.3.4); smoke.

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, activity-registry, enrich_pass, postpass/
extractor walks, the M-3a/M-3b persisted families + their write paths, the trust
denominator, union serving's row semantics (M-R2 — consume, don't change). Honesty rules
absolute: unknown renders as unknown, never zero, never stale numbers. Any
baseline/invariance mismatch is a FINDING. Do NOT commit. Consolidation witness green;
manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate column in full; flag semantics consistent with M-R2 (surfaces appear per the
ratified visibility rules — the union accounting blocks are read surfaces of ledger state,
their rendering follows the design's §5.3/§5.4 visibility, recorded explicitly in the
report); chunked cargo gates; witness 15/15; canonical smoke per
docs/testing/end-of-slice-procedure.md with provenance; isolated dogfood.

## 5. Definition of done

trust/doctor/orient/stats render the witness-ledger accounting through one shared
projection with honest unknowns and deterministic ordering; collision rendering lands
(closing the M-R1 amendment); R-0 repos show no phantom zeros; pipeline accounting
untouched; all gates green.
