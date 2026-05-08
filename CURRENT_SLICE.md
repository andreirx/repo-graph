# CURRENT_SLICE.md

## Current Priority

Artifact Contract Registry (ACR) — foundational architecture for artifact semantics.

## Active Slice

`docs/slices/acr-3-provenance-and-freshness-schema.md`

## Branch Intent

Codify artifact truth classes, refresh policies, and provenance requirements in code
before continuing refresh bug fixes. The registry becomes the authority for how each
artifact family behaves during refresh and query.

## Definition of Done (ACR-3)

- [ ] Per-row freshness columns added to derived artifact tables
- [ ] Per-row provenance columns/tables for Layer 2+ families
- [ ] Freshness state enum (`current`, `impacted`, `stale`, `unknown`) in schema
- [ ] Provenance anchor storage for derived rows
- [ ] Migration path for existing data

## Carry-over from ACR-2

These items require ACR-3 scaffolding before they can be completed:

- **Per-row freshness/provenance** required to honor `MarkImpactedDeferRecompute` policy
- **Proto reindex drift** remains until better scaffolding exists (ContractSchemas re-indexes all)
- **Inferences** use copy-forward instead of `MarkImpactedDeferRecompute` pending ACR-3/4
- **Boundary proof case** belongs to ACR-5

## Program Overview

| Slice | Scope | Status |
|-------|-------|--------|
| ACR-1 | Create artifact-contracts crate | DONE |
| ACR-2 | Refresh pipeline consumes registry | DONE (copy-forward + recompute dispatched; reindex drift documented) |
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

- Treating tables as the unit of architecture (families are the unit)
- Ad-hoc refresh behavior without consulting contracts
- Provisional classifications without explicit maturity markers
- Prose-only documentation without code-level registry
- Treating JSON as product essence (JSON is CLI transport contract)
