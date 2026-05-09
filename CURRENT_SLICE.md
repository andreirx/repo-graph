# CURRENT_SLICE.md

## Current Priority

Artifact Contract Registry (ACR) — foundational architecture for artifact semantics.

## Active Slice

`docs/slices/acr-4-impact-propagation.md`

## Branch Intent

Codify artifact truth classes, refresh policies, and provenance requirements in code
before continuing refresh bug fixes. The registry becomes the authority for how each
artifact family behaves during refresh and query.

## Definition of Done (ACR-4)

- [x] `mark_impacted_by_stable_keys()` uses `json_each()`/`json_extract()` (fix ACR-3 scaffolding)
- [x] Unit tests for JSON-based provenance matching (449 storage tests pass)
- [x] TECH-DEBT.md ACR-3 scaffolding entry marked FIXED
- [x] Impact propagation module created (`impact_propagation.rs`)
- [x] Impact propagation wired into refresh pipeline
- [x] Populate provenance_json during inference creation (Spring liveness)
- [x] Changed L0 stable keys tracked from extraction results
- [x] Unit tests: provenance populated → freshness='current', mark_impacted works
- [x] Integration test: Spring inferences have provenance and 'current' freshness

## Status

ACR-4 implementation complete. Impact propagation now works end-to-end:

1. **Provenance populated**: Spring liveness inferences have `provenance_json` with canonical structure
2. **Freshness tracked**: Inferences with provenance get `freshness_state='current'`
3. **Copy-forward preserves state**: Unchanged file inferences keep their provenance and freshness
4. **Changed file detection**: Stable key matching uses proper delimiters (no prefix false-matches)
5. **Refresh respects changes**: Only changed file inferences are regenerated; unchanged are copied

**Integration tests**:
- `refresh_spring_inference_has_provenance_and_current_freshness` - proves provenance populated
- `refresh_preserves_unchanged_spring_inferences` - proves copy-forward + regenerate semantics
- `refresh_marks_cross_file_inference_impacted` - **canonical proof**: cross-file provenance causes `current → impacted` on surviving row

**Follow-on work** (not blocking ACR-4 closure):
- Provenance for other inference producers (framework entrypoints, etc.)
- Impact report included in refresh diagnostics
- Query-layer freshness filtering (ACR-6)

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
| ACR-3 | Per-row freshness and provenance schema | DONE (schema + storage port; parity blocked on TS) |
| ACR-4 | Impact propagation from L0 changes | DONE (provenance populated, copy-forward preserves state, delimiter-safe matching) |
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
