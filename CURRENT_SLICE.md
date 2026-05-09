# CURRENT_SLICE.md

## Current Priority

ACR program COMPLETE. Next priority: Module truth-model unification.

## Active Slice

NONE — ACR-6 delivered, awaiting next slice assignment.

## Branch Intent

ACR-6 complete. Query/read surfaces now report degradation semantics.
Next work per ROADMAP: `docs/slices/rust-module-parity.md`

## Definition of Done (ACR-6)

- [x] `FreshnessStateDto`, `FreshnessInfo` typed DTOs in signal.rs (8 tests)
- [x] `DegradationStatus`, `DegradationInfo` typed DTOs in limit.rs (6 tests)
- [x] `Signal.freshness: Option<FreshnessInfo>` — scaffolded, ready for L2+ signals
- [x] `Limit.degradation: Option<DegradationInfo>` — wired
- [x] `orient` MODULE_DATA_UNAVAILABLE carries structured degradation info
- [x] `surfaces list` reports degradation when empty (Rust indexer path)
- [x] Tests prove unsupported degradation is distinct from plain absence
- [ ] Per-signal freshness on orient (deferred: current signals don't consume L2+ families)
- [ ] Freshness on check (N/A: check is verdicts, not artifact queries)

## Status

ACR-6 COMPLETE. ACR program finished.

**Delivered:**
- `FreshnessStateDto`, `FreshnessInfo` in signal.rs (8 tests)
- `DegradationStatus`, `DegradationInfo` in limit.rs (6 tests)
- `Signal.freshness: Option<FreshnessInfo>` — scaffolded for future L2+ signals
- `Limit.degradation: Option<DegradationInfo>` — wired
- `orient`: MODULE_DATA_UNAVAILABLE now carries degradation info
- `surfaces list`: reports degradation when empty (Rust-only indexer path)

**Deferred (by design):**
- Per-signal freshness on orient: current signals consume L0/L1 facts, not L2+ families
- Freshness on check: check produces verdicts, not artifact queries

## Program Overview

| Slice | Scope | Status |
|-------|-------|--------|
| ACR-1 | Create artifact-contracts crate | DONE |
| ACR-2 | Refresh pipeline consumes registry | DONE |
| ACR-3 | Per-row freshness and provenance schema | DONE |
| ACR-4 | Impact propagation from L0 changes | DONE |
| ACR-5 | Boundary contract proof case | DONE |
| ACR-6 | Query degradation and freshness | DONE |

## Not the Priority

- New IPC families
- New language extractors
- CLI ergonomic polish
- Layer 3 hint expansion
- Module discovery expansion
- Feature work before ACR foundation is complete

## After ACR

- Module truth-model unification (`docs/slices/rust-module-parity.md`)

## Key Architecture References

- `docs/architecture/artifact-contract-model.md` — full specification
- `docs/architecture/adr/adr-artifact-contract-registry.md` — decision record
- `docs/VISION.md` §Product Layer Model — doctrine

## Approved DB Path

`./test-artifacts/repo-graph.db`

Do not create databases elsewhere.

## Known Drift to Avoid

- Treating tables as the unit of architecture (families are the unit)
- Ad-hoc refresh behavior without consulting contracts
- Provisional classifications without explicit maturity markers
- Prose-only documentation without code-level registry
- Treating JSON as product essence (JSON is CLI transport contract)
- Collapsing unsupported/unknown/impacted into single "missing" state
