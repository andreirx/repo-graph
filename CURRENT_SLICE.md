# CURRENT_SLICE.md

## Current Priority

Artifact Contract Registry (ACR) — foundational architecture for artifact semantics.

## Active Slice

`docs/slices/acr-1-artifact-contracts-crate.md`

## Branch Intent

Codify artifact truth classes, refresh policies, and provenance requirements in code
before continuing refresh bug fixes. The registry becomes the authority for how each
artifact family behaves during refresh and query.

## Definition of Done (ACR-1)

- [ ] `rust/crates/artifact-contracts` crate created
- [ ] All enums defined: ArtifactFamily, TruthKind, RefreshPolicy, IdentityPolicy, DegradationPolicy, ProvenancePolicy, ImpactPolicy, FreshnessTracking
- [ ] ArtifactContract struct defined
- [ ] Registry accessor functions implemented
- [ ] All artifact families registered with contracts
- [ ] Completeness tests pass (every family has a contract)
- [ ] Coherence tests pass (policy combinations are valid)
- [ ] No refresh behavior changed yet (support module only)

## Program Overview

| Slice | Scope | Status |
|-------|-------|--------|
| ACR-1 | Create artifact-contracts crate | ACTIVE |
| ACR-2 | Refresh pipeline consumes registry | NOT STARTED |
| ACR-3 | Per-row freshness and provenance schema | NOT STARTED |
| ACR-4 | Impact propagation from L0 changes | NOT STARTED |
| ACR-5 | Boundary contract proof case | NOT STARTED |
| ACR-6 | Query degradation and freshness | NOT STARTED |

## Not the Priority

- New IPC families
- New language extractors
- CLI ergonomic polish
- Layer 3 hint expansion
- Module discovery expansion
- Feature work before ACR foundation is complete

## Key Architecture References

- `docs/architecture/artifact-contract-model.md` — full specification
- `docs/architecture/adr/adr-artifact-contract-registry.md` — decision record
- `docs/VISION.md` §Product Layer Model — doctrine

## Approved DB Path

`./test-artifacts/repo-graph.db`

Do not create databases elsewhere.

## Known Drift to Avoid

- Starting refresh fixes before ACR-1 is complete
- Treating tables as the unit of architecture (families are the unit)
- Ad-hoc refresh behavior without consulting contracts
- Provisional classifications without explicit maturity markers
- Prose-only documentation without code-level registry
