# ACR-2: Refresh Policy Integration

Status: NOT STARTED
Depends: `acr-1-artifact-contracts-crate.md`
Follow-on: `acr-3-provenance-and-freshness-schema.md`
Track: Core Infrastructure — Artifact Contract Registry

## Objective

Make the refresh pipeline consume the artifact contract registry. Dispatch refresh behavior by artifact truth class and refresh policy, not by ad-hoc table-specific code.

## Scope

### In Scope

- Refactor refresh pipeline to dispatch by contract
- Separate refresh phases by truth class
- Wire existing copy-forward logic under registry policies
- Add registry dependency to repo-index/indexer crates
- Ensure deterministic relationships are handled differently from extracted facts
- Add diagnostic logging showing which policy is applied

### Out of Scope

- Per-row provenance storage (ACR-3)
- Impact propagation (ACR-4)
- Freshness state columns (ACR-3)
- New schema changes

## Current State

The refresh pipeline in `compose.rs` currently:
1. Identifies changed/unchanged files
2. Re-extracts changed files
3. Calls `copy_forward_derived_artifacts()` for unchanged files
4. Re-runs proto indexer
5. Re-runs gRPC detection chain (GR-1A/GR-2A/GR-3A)

This is ad-hoc. The pipeline does not consult artifact contracts.

## Target State

The refresh pipeline should:
1. Identify changed/unchanged files
2. For each artifact family, consult its contract
3. Dispatch behavior based on `refresh_policy`
4. Execute in truth-class order: Layer 0-1 → Layer 2 → Layer 3+

## Implementation

### Phase 1: Add Registry Dependency

Add `artifact-contracts` as a dependency to:
- `repo-graph-indexer`
- `repo-graph-repo-index`

### Phase 2: Refactor Refresh Pipeline

Replace ad-hoc dispatch with contract-driven dispatch.

**Before:**
```rust
// Ad-hoc in compose.rs
copy_forward_measurements(...);
copy_forward_inferences(...);
copy_forward_boundary_surfaces(...);
copy_forward_contract_schemas(...);
// ... then re-run gRPC detection
```

**After:**
```rust
// Contract-driven in compose.rs
use artifact_contracts::{registry, TruthKind, RefreshPolicy};

// Phase 1: Extracted Facts
for family in registry::families_by_truth_kind(TruthKind::ExtractedFact) {
    let contract = registry::get_contract(family);
    match contract.refresh_policy {
        RefreshPolicy::ReextractChangedInputs => {
            reextract_for_changed_files(family, changed_files);
            copy_forward_for_unchanged_files(family, unchanged_files);
        }
        RefreshPolicy::CopyForwardWithFkRemap => {
            copy_forward_with_remap(family, unchanged_files, uid_mapping);
        }
        // ...
    }
}

// Phase 2: Deterministic Relationships
for family in registry::families_by_truth_kind(TruthKind::DeterministicRelationship) {
    let contract = registry::get_contract(family);
    match contract.refresh_policy {
        RefreshPolicy::RecomputeFromCurrentSnapshot => {
            recompute_from_current(family, snapshot_uid);
        }
        // ...
    }
}

// Phase 3: Projections and Inferences
// ...
```

### Phase 3: Centralize Copy-Forward Dispatch

Create a central dispatch function that routes to family-specific implementations:

```rust
fn refresh_family(
    storage: &mut Storage,
    family: ArtifactFamily,
    context: &RefreshContext,
) -> Result<RefreshFamilyResult, RefreshError> {
    let contract = registry::get_contract(family);
    
    match contract.refresh_policy {
        RefreshPolicy::ReextractChangedInputs => {
            refresh_extracted_fact(storage, family, context)
        }
        RefreshPolicy::RecomputeFromCurrentSnapshot => {
            recompute_deterministic_relationship(storage, family, context)
        }
        RefreshPolicy::CopyForwardWithFkRemap => {
            copy_forward_with_fk_remap(storage, family, context)
        }
        RefreshPolicy::MarkImpactedDeferRecompute => {
            mark_impacted_defer_recompute(storage, family, context)
        }
        RefreshPolicy::SnapshotIndependent => {
            Ok(RefreshFamilyResult::skipped("snapshot-independent"))
        }
        RefreshPolicy::NeverCopyForward => {
            // Regenerate or compute on read
            regenerate_projection(storage, family, context)
        }
    }
}
```

### Phase 4: Add Diagnostic Logging

Log which policy is applied to which family:

```rust
tracing::info!(
    family = %family,
    policy = %contract.refresh_policy,
    "applying refresh policy"
);
```

This makes the refresh pipeline auditable.

## Key Behavior Changes

### BoundaryContracts and BoundaryInteractionLinks

Currently treated inconsistently. After this slice:
- Both are `DeterministicRelationship`
- Both have `RecomputeFromCurrentSnapshot` policy
- GR-1A/GR-2A/GR-3A chain runs AFTER Layer 0-1 copy-forward completes
- Uses current snapshot's surface UIDs and element UIDs

### Inferences

Currently copied forward like extracted facts. After this slice:
- `Inference` truth kind
- `MarkImpactedDeferRecompute` policy (or `RecomputeFromCurrentSnapshot` depending on cost)
- Explicit handling separate from measurements

## Test Matrix

| Scenario | Expected Behavior |
|----------|-------------------|
| Unchanged file, extracted fact family | Copy forward |
| Changed file, extracted fact family | Re-extract |
| Any change, deterministic relationship | Recompute from current snapshot |
| Layer 0 unchanged, inference family | Copy forward or mark impacted |
| Layer 0 changed, inference family | Mark impacted or recompute |

## Definition of Done

- [ ] artifact-contracts crate is a dependency of indexer/repo-index
- [ ] Refresh pipeline dispatches by contract
- [ ] Extracted facts handled in Phase 1
- [ ] Deterministic relationships handled in Phase 2
- [ ] GR chain runs after Layer 0-1 complete
- [ ] Diagnostic logging shows policy application
- [ ] Existing refresh tests still pass
- [ ] No new schema changes

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo build -p repo-graph-repo-index
cargo test -p repo-graph-repo-index
cargo test -p repo-graph-repo-index --test refresh
```

## Notes

- This slice does not add per-row provenance/freshness — that's ACR-3
- The goal is to make refresh behavior explicit and auditable
- Family-specific implementations (e.g., `copy_forward_boundary_surfaces`) remain; they're just called via dispatch
- The GR chain relocation (from orchestrator to post-copy-forward) may be the most complex change
