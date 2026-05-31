# XPART-ST3-BOUNDARY-DECISION: ST3 Residuals — Degraded Classes, Not Blockers

Slice ID: XPART-ST3-BOUNDARY-DECISION
Status: **DECISION RECORD (architecture boundary) — no code.** 2026-05-31.
Track: Extraction Substrate Pivot — Stage B (`docs/architecture/scip-migration-plan.md`).
Depends: XPART-PROVE-1A/1B (the answer-class contract + the two residuals).

## Decision

The two XPART-PROVE-1B residuals are **explicit degraded answer classes, not blockers.** ST3 is
**closed for the current LiveGraph stage** with those degraded classes documented. **Proceed to
REFRESH-PROBE-1.**

This is a boundary record, not a probe — no new code. It defines how the residuals are
classified; the LiveGraph runtime must honor that classification when built (forward obligation
below).

## Contradiction check (required before recording) — none; doctrine supports it

- VISION **"Orientation, Not Oracle"**: repo-graph narrows the search space; it does not guarantee
  completeness, and "over-promising completeness undermines trust when edge cases are missed."
- VISION Layer doctrine + **Dependency Rule 3** ("Outer layers must surface unknowns"): partial,
  source-anchored orientation with explicit degradation is the doctrine, not a workaround.
- `agent_docs/architecture.md` degradation primitive: **`null` = unknown, empty = known-zero —
  never conflate.** This is the exact mechanism that makes the degraded classes safe (below).

The decision is consistent with — and required by — existing doctrine. No contradiction found.

## The degraded classes (mapping onto the ratified answer-class contract)

Both map to existing answer classes with an explicit, machine-readable reason. **Critical safety
rule (architecture.md): the result is `null`/unknown (`Unavailable`), NEVER empty.** An empty
result means "known zero callers" (a positive fact); these residuals are "unknown / not
addressable" and must never be presented as known-zero.

**Residual 1 — anonymous structural members (`typeLiteralNN`).** Not a stable cross-partition
identity (compilation-unit-relative; unstable across indexes even in source-path — `api-src`
measured 95/78/17). A cross-partition query whose target, or any answer member, is an anonymous
structural member returns **`Unavailable` (reason: `AnonymousStructuralMember`)** — never `Exact`,
never empty. Reversible: a future positional/VLQ declaration-map slice may upgrade it to
addressable.

**Residual 2 — package export-surface without declaration maps / complex `exports`.** Export
surface unreconciled (Basis 2 deferred). A cross-partition query into such a package returns
**`Partial` (resident facts + reason: `UnreconciledExportSurface`)**, or **`Unavailable`** when
the target itself is unreconciled — never `Exact`. Reversible: a future Basis-2 slice may upgrade
it.

Both reuse the XPART-PROVE-1B **answer-class precision rule**: `Exact` only for complete-basis
(`DeclarationMapExact`/`NameExactUnique`) symbols; any `Unresolved`/`Ambiguous`-dependent answer
is `Partial`/`Unavailable`.

## Why these are not blockers

1. **Named public-API traversal is the product-critical path** (VISION value frontier: modules,
   boundaries, callers/callees). 1B proved it — named surface 78/78, 0 misattachment, 0 silent.
2. **Anonymous structural members are not stable identity anchors.** They cannot be a durable
   cross-partition target short of positional identity; degrading them is correct, not a shortcut.
3. **Complex export-surface support is breadth expansion**, not a core-runtime blocker.
4. **The answer-class contract + `null`≠empty already prevent silent correctness claims** — a
   degraded query is visibly degraded, never a false "zero callers."

## ST3 status after this record

ST3 is **CLOSED for the current LiveGraph stage**, scoped to declaration-map-backed named
TypeScript package-boundary traversal, **with two named, reversible degraded classes**. The
residuals are documented degradations with explicit upgrade slices, not open blockers:
- Residual 1 → future positional/VLQ declaration-map slice.
- Residual 2 → future Basis-2 export-surface slice.

## Forward obligation (lands with the LiveGraph runtime slice, not here)

The runtime cross-partition `callers`/`path` surfaces MUST implement the two degraded classes
above with their explicit reasons and the `null`≠empty rule. This record is the contract;
enforcement is a runtime-slice obligation. No probe or runtime code is in scope here.

## Next

**REFRESH-PROBE-1** (Stage B refresh-at-scale), then RUST-INGEST-PROVE-1. Stage C runtime stays
gated behind remaining Stage B evidence.

## References
- `docs/slices/xpart-prove-1b.md` — verdict, residuals, answer-class precision rule
- `docs/audits/xpart-prove-1/findings-1b.md` — evidence (local)
- `agent_docs/architecture.md` — `null`=unknown / empty=known-zero
- `docs/VISION.md` — Orientation-not-Oracle; Layer degradation doctrine
