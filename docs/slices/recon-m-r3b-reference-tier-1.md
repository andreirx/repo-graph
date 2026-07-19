# RECON-M-R3B-REFERENCE-TIER-1 — the reference tier on callers/callees/explain (reconciliation IMPL milestone M-R3b)

Status: SPECIFIED (2026-07-19) · Track: Reconciliation IMPL (recon-design-1 §6.1, ratified §8)
Depends: M-R2 (c202279 — union serving), M-R3a (109cf3b — the shared projection + §5.3.0 gate).

## 1. Contract — the recon-design-1 §6.1 **M-R3b row IS the binding contract**

The reference tier on callers/callees/explain — "reads / writes / type references" — from the
SCIP semantic overlay's non-Calls reference kinds, budget-disciplined per §5.2. The tier
renders ONLY in W-BOTH (it is semantic-overlay data; W-ONE/W-NONE have no S witness), through
the M-R3a shared projection/labeling discipline (§5.3.0 gate: accounting + complete coverage
basis or suppressed). Additive beside existing rows — never replacing, never inflating call
counts or the trust denominator.

## 2. Gate — the M-R3b row's gate column

Tier renders only in W-BOTH (named test); truncation named test (amodx max fan-in 456 is the
fixture-scale bound — budgets per §5.2); R-0 (zero-SCIP repos: tier absent, no phantom
sections) / R-1 (mixed repos: tier scoped to covered partitions only); S-4 informs budgets
(record the budget rationale). Canonical smoke + chunked cargo gates + consolidation witness
green.

## 3. Stop conditions

Frozen: union_serve row semantics (Calls union — consume its witness machinery, do not
change it), the M-R3a projection contracts, trust ratio, storage write paths, livegraph
feed/refresh + epoch/coordinator, extractors/postpass. Unknown never zero; the §5.3.0 gate
applies to every tier rendering. Do NOT commit. Witness green; manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate; chunked cargo gates (standing pattern); witness 15/15; canonical smoke with
provenance; isolated dogfood; live E2E on a covered fixture showing the tier rendered with
its coverage frame.

## 5. Definition of done

In W-BOTH, callers/callees/explain can show the reads/writes/type-references tier,
budget-truncated with named truncation, coverage-labeled via the shared gate; absent
everywhere else; no call-count or trust-input change anywhere; gates green.
