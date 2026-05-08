# ACR-5: Boundary Contract Proof Case

Status: NOT STARTED
Depends: `acr-4-impact-propagation.md`
Follow-on: `acr-6-query-degradation-and-freshness.md`
Track: Core Infrastructure — Artifact Contract Registry

## Objective

Solve the `BoundaryContracts` and `BoundaryInteractionLinks` refresh problem using the new artifact contract model. This is the first proof case demonstrating the architecture works.

## Background

### Current Problem

The gRPC link detection (GR-3A) joins:
- `boundary_interaction_surfaces` (Layer 0, copied forward with new UIDs)
- `contract_elements` (Layer 0, re-indexed with new UIDs)
- `boundary_contracts` (Layer 2, mapping surface → element)

During refresh:
1. Surfaces are copied forward → new UIDs
2. Elements are re-indexed → new UIDs
3. `boundary_contracts` points to parent snapshot UIDs → broken FKs

This was documented in `refresh-integrity-parity.md` as a blocker.

### Solution Under New Model

Under the artifact contract model:
- `BoundaryInteractionSurfaces` = ExtractedFact, ReextractChangedInputs
- `ContractElements` = ExtractedFact, ReextractChangedInputs
- `BoundaryContracts` = DeterministicRelationship, RecomputeFromCurrentSnapshot
- `BoundaryInteractionLinks` = DeterministicRelationship, RecomputeFromCurrentSnapshot

Both `BoundaryContracts` and `BoundaryInteractionLinks` must be recomputed from scratch using current snapshot's surface and element UIDs. No copy-forward. No FK remapping.

## Scope

### In Scope

- Relocate GR chain to run after Layer 0-1 copy-forward
- Implement recompute-from-current-snapshot for BoundaryContracts
- Implement recompute-from-current-snapshot for BoundaryInteractionLinks
- Populate provenance for both families
- Integration tests proving no FK leakage

### Out of Scope

- Query-layer freshness filtering (ACR-6)
- Other deterministic relationship families

## Implementation

### Step 1: Relocate GR Chain

Currently, GR-1A/GR-2A/GR-3A runs in `orchestrator::refresh_repo`.

Move to `compose.rs`, executing AFTER `copy_forward_derived_artifacts()` completes.

**Before:**
```rust
// In orchestrator.rs
pub fn refresh_repo(...) {
    // ...
    run_gr_chain(...);  // Runs too early, uses stale UIDs
    // ...
}
```

**After:**
```rust
// In compose.rs
pub fn refresh_into_storage(...) {
    // Phase 1: Layer 0-1
    copy_forward_derived_artifacts(...);
    reextract_changed_files(...);
    
    // Phase 2: Deterministic Relationships
    // GR chain now runs here, using current snapshot UIDs
    regenerate_boundary_contracts(&storage, &snapshot_uid)?;
    regenerate_boundary_interaction_links(&storage, &snapshot_uid)?;
}
```

### Step 2: Regenerate BoundaryContracts

```rust
fn regenerate_boundary_contracts(
    storage: &Storage,
    snapshot_uid: &str,
) -> Result<u64, Error> {
    // 1. Clear any stale boundary_contracts for this snapshot
    storage.delete_boundary_contracts(snapshot_uid)?;
    
    // 2. Query current snapshot's surfaces and elements
    let surfaces = storage.get_all_boundary_surfaces(snapshot_uid)?;
    let elements = storage.get_all_contract_elements(snapshot_uid)?;
    
    // 3. Run GR-2A matching logic
    let contracts = match_surfaces_to_elements(&surfaces, &elements)?;
    
    // 4. Insert with provenance
    let mut count = 0u64;
    for contract in contracts {
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("BoundaryInteractionSurfaces", &contract.surface_stable_key),
            ProvenanceAnchor::new("ContractElements", &contract.element_stable_key),
        ]);
        
        storage.insert_boundary_contract(&contract, &provenance)?;
        count += 1;
    }
    
    Ok(count)
}
```

### Step 3: Regenerate BoundaryInteractionLinks

```rust
fn regenerate_boundary_interaction_links(
    storage: &Storage,
    snapshot_uid: &str,
) -> Result<u64, Error> {
    // 1. Clear any stale links for this snapshot
    storage.delete_boundary_interaction_links(snapshot_uid)?;
    
    // 2. Query current snapshot's surfaces and contracts
    let impl_surfaces = storage.get_boundary_surfaces_by_role(snapshot_uid, "impl")?;
    let client_surfaces = storage.get_boundary_surfaces_by_role(snapshot_uid, "client")?;
    let contracts = storage.get_all_boundary_contracts(snapshot_uid)?;
    
    // 3. Run GR-3A linking logic
    let links = compute_interaction_links(&impl_surfaces, &client_surfaces, &contracts)?;
    
    // 4. Insert with provenance
    let mut count = 0u64;
    for link in links {
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("BoundaryInteractionSurfaces", &link.impl_surface_stable_key),
            ProvenanceAnchor::new("BoundaryInteractionSurfaces", &link.client_surface_stable_key),
            ProvenanceAnchor::new("BoundaryContracts", &link.contract_uid),
        ]);
        
        storage.insert_boundary_interaction_link(&link, &provenance)?;
        count += 1;
    }
    
    Ok(count)
}
```

### Step 4: Update Refresh Diagnostics

```rust
// In IndexResult
pub struct IndexResult {
    // ... existing fields ...
    pub boundary_contracts_regenerated: u64,
    pub boundary_interaction_links_regenerated: u64,
}
```

## Test Cases

### Test 1: No Changes — Links Preserved

```rust
#[test]
fn refresh_boundary_links_no_changes() {
    // Setup: index repo with gRPC surfaces + proto contracts
    let r1 = index_into_storage(...);
    let links1 = storage.get_boundary_interaction_links(&r1.snapshot_uid)?;
    assert!(!links1.is_empty());
    
    // Refresh with no changes
    let r2 = refresh_into_storage(...);
    let links2 = storage.get_boundary_interaction_links(&r2.snapshot_uid)?;
    
    // Links should be semantically equivalent (different UIDs, same relationships)
    assert_eq!(links1.len(), links2.len());
    // Compare by stable semantic identity, not by UID
}
```

### Test 2: Surface Changed — Links Regenerated

```rust
#[test]
fn refresh_boundary_links_surface_changed() {
    // Setup
    let r1 = index_into_storage(...);
    
    // Modify a file containing a gRPC surface
    fs::write(path, modified_content)?;
    
    // Refresh
    let r2 = refresh_into_storage(...);
    let links2 = storage.get_boundary_interaction_links(&r2.snapshot_uid)?;
    
    // Links should use current snapshot UIDs
    for link in &links2 {
        // Verify surface_uid points to current snapshot
        let surface = storage.get_boundary_surface(&link.surface_uid)?;
        assert_eq!(surface.snapshot_uid, r2.snapshot_uid);
    }
}
```

### Test 3: No FK Leakage to Parent Snapshot

```rust
#[test]
fn refresh_boundary_links_no_fk_leakage() {
    let r1 = index_into_storage(...);
    let snap1_uid = r1.snapshot_uid.clone();
    
    // Modify any file to trigger refresh (not full rebuild)
    fs::write(unrelated_file, "changed")?;
    
    let r2 = refresh_into_storage(...);
    
    // Query all boundary_contracts and boundary_interaction_links
    let contracts = storage.get_all_boundary_contracts(&r2.snapshot_uid)?;
    let links = storage.get_all_boundary_interaction_links(&r2.snapshot_uid)?;
    
    // None should reference parent snapshot UIDs
    for contract in &contracts {
        assert_ne!(contract.surface_uid, /* any UID from snap1 */);
        assert_ne!(contract.element_uid, /* any UID from snap1 */);
    }
    
    for link in &links {
        let surface = storage.get_boundary_surface(&link.surface_uid)?;
        assert_eq!(surface.snapshot_uid, r2.snapshot_uid);
    }
}
```

### Test 4: Provenance Populated

```rust
#[test]
fn refresh_boundary_links_have_provenance() {
    let r1 = index_into_storage(...);
    
    let links = storage.get_all_boundary_interaction_links(&r1.snapshot_uid)?;
    
    for link in &links {
        let provenance = storage.get_provenance(
            ArtifactFamily::BoundaryInteractionLinks,
            &link.link_uid,
        )?;
        
        assert!(provenance.is_some());
        let prov = provenance.unwrap();
        assert!(!prov.depends_on.is_empty());
        // Should reference surfaces and/or contracts
    }
}
```

## Definition of Done

- [ ] GR chain relocated to compose.rs Phase 2
- [ ] BoundaryContracts regenerated from current snapshot
- [ ] BoundaryInteractionLinks regenerated from current snapshot
- [ ] Provenance populated for both families
- [ ] No FK leakage to parent snapshot UIDs
- [ ] Refresh diagnostics show regeneration counts
- [ ] All four test cases pass
- [ ] Existing boundary refresh tests still pass

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo test -p repo-graph-repo-index --test refresh

# Manual validation with a repo that has gRPC
rmap index /path/to/grpc-repo
rmap boundaries list /path/to/db repo-uid
rmap contracts usages /path/to/db repo-uid
# Modify a file
rmap refresh /path/to/grpc-repo
rmap boundaries list /path/to/db repo-uid  # Should show same count
```

## Notes

- This slice proves the artifact contract model works for the known problem case
- The GR chain relocation is the key architectural change
- Regeneration is simpler than copy-forward-with-remap for this case
- Provenance enables future impact tracking for these families
- Performance: regeneration is O(surfaces * elements) but gRPC matching is already this complexity
