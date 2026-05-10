# ACR-4: Impact Propagation

Status: **DONE**
Depends: `acr-3-provenance-and-freshness-schema.md`
Follow-on: `acr-5-boundary-contract-proof-case.md`
Track: Core Infrastructure — Artifact Contract Registry

## Implementation Progress

### Completed

- [x] **Step 1: Fix JSON Provenance Matching (ACR-3 Tech Debt)**
  - Upgraded `mark_impacted_by_stable_keys()` to use `json_each()`/`json_extract()`
  - Added regression test `mark_impacted_does_not_false_match_prefix`
  - TECH-DEBT.md entry marked as FIXED

- [x] **Step 2: Impact Propagation Module**
  - Created `impact_propagation.rs` in repo-index crate
  - `ImpactReport` type for tracking impacted counts per family
  - `propagate_impact()` function iterates families and calls storage port

- [x] **Step 3: Wire Impact Propagation into Refresh**
  - `propagate_impact()` called after copy-forward in `refresh_into_storage_with_progress()`

- [x] **Step 4: Populate Provenance During Inference Creation**
  - Spring liveness inferences now populate `provenance_json` with canonical structure
  - Provenance references the target Node stable key (the symbol being classified)
  - Uses `Provenance::from_layer0_items()` with `ProvenanceAnchor::new("Nodes", stable_key)`
  - Inferences with provenance get `freshness_state = 'current'` (not 'unknown')

- [x] **Step 5: Track Changed L0 Stable Keys**
  - Changed stable keys collected from nodes of changed files
  - Uses delimiter-safe matching: `{repo}:{path}#` for SYMBOL, exact match for FILE
  - Passes stable keys to `propagate_impact()` for impact marking

- [x] **Step 6: Refresh Respects Changed vs Unchanged Files**
  - `persist_spring_liveness_inferences()` accepts `changed_file_paths` parameter
  - Refresh mode: only processes nodes from changed files, preserves copy-forwarded inferences
  - Full index mode: processes all nodes, replaces all Spring inferences
  - `copy_forward_inferences()` preserves `provenance_json` and `freshness_state`

- [x] **Step 7: Delimiter-Safe Delete**
  - `delete_inferences_by_kind_and_files()` uses `substr()` for exact prefix matching
  - Prevents false-matching path prefixes (e.g., `src/A.java` won't match `src/A.javaX`)

- [x] **Step 8: Tests**
  - 451 storage unit tests pass (including ACR-4 specific tests)
  - 16 refresh integration tests pass
  - Key tests:
    - `impact_propagation_marks_cross_file_dependency_impacted` (storage unit test)
    - `refresh_marks_cross_file_inference_impacted` (integration test - **canonical proof**)
    - `refresh_preserves_unchanged_spring_inferences`
    - `delete_by_file_does_not_false_match_path_prefix`

### Follow-on Work (Not Blocking ACR-4 Closure)

- [ ] Provenance for other inference producers (framework entrypoints, Lambda detection)
- [ ] Impact report included in refresh result diagnostics
- [ ] Cross-file provenance for true `MarkImpactedDeferRecompute` behavior

## Objective

Implement impact propagation from Layer 0 changes to upper-layer artifacts. When a Layer 0 item changes during refresh, mark dependent upper-layer rows as `impacted` based on their provenance.

## Scope

### In Scope

- Impact propagation algorithm
- Provenance population during extraction/computation
- Integration with refresh pipeline
- Diagnostic reporting of impact counts

### Out of Scope

- Query-layer freshness filtering (ACR-6)
- Eager recomputation of all impacted rows
- Background recomputation scheduling

## Core Algorithm

### During Refresh

1. **Identify Changed Layer 0 Items**
   - Files that were re-extracted (content hash changed)
   - Files that were config-widened (re-extracted due to config change)
   - Files that were deleted

2. **Collect Changed Stable Keys**
   - For each changed file, collect all Layer 0 stable keys that were affected:
     - File node stable key
     - Symbol node stable keys
     - Edge stable keys
     - Measurement stable keys
     - Surface stable keys

3. **Propagate Impact**
   - For each artifact family with `ImpactPolicy::MarkImpactedOnRelevantLayer0Change`:
     - Query rows where `provenance_json` references any changed stable key
     - Update `freshness_state = 'impacted'`
     - Update `freshness_updated_at`

4. **Report Impact**
   - Log count of impacted rows per family
   - Include in refresh diagnostics

### Provenance Population

When creating upper-layer artifact rows, populate `provenance_json` with their Layer 0 dependencies:

**BoundaryContracts:**
```rust
let provenance = Provenance::from_layer0_items(vec![
    ProvenanceAnchor {
        family: "BoundaryInteractionSurfaces".to_string(),
        stable_key: surface.stable_key.clone(),
    },
    ProvenanceAnchor {
        family: "ContractElements".to_string(),
        stable_key: element.stable_key.clone(),
    },
]);
```

**BoundaryInteractionLinks:**
```rust
let provenance = Provenance::from_layer0_items(vec![
    ProvenanceAnchor {
        family: "BoundaryInteractionSurfaces".to_string(),
        stable_key: impl_surface.stable_key.clone(),
    },
    ProvenanceAnchor {
        family: "BoundaryInteractionSurfaces".to_string(),
        stable_key: client_surface.stable_key.clone(),
    },
    ProvenanceAnchor {
        family: "ContractSchemas".to_string(),
        stable_key: schema.stable_key.clone(),
    },
]);
```

**Inferences:**
```rust
// inferences already have basis_json; ensure it follows provenance format
let provenance = Provenance::from_layer0_items(basis_anchors)
    .with_extractor("grpc_impl_detector");
```

## Implementation

### Impact Propagation Function

```rust
/// Propagate impact from Layer 0 changes to upper-layer artifacts.
pub fn propagate_impact(
    storage: &mut Storage,
    snapshot_uid: &str,
    changed_l0_stable_keys: &[String],
) -> Result<ImpactReport, StorageError> {
    use artifact_contracts::{registry, ImpactPolicy};
    
    let mut report = ImpactReport::new();
    let now = utc_now_iso8601();
    
    for family in registry::families_with_provenance() {
        let contract = registry::get_contract(family);
        
        match contract.impact_policy {
            ImpactPolicy::MarkImpactedOnRelevantLayer0Change => {
                let count = storage.mark_impacted_by_provenance(
                    snapshot_uid,
                    family,
                    changed_l0_stable_keys,
                    &now,
                )?;
                report.add(family, count);
            }
            ImpactPolicy::MarkImpactedOnAnyLayer0Change => {
                if !changed_l0_stable_keys.is_empty() {
                    let count = storage.mark_all_impacted(
                        snapshot_uid,
                        family,
                        &now,
                    )?;
                    report.add(family, count);
                }
            }
            ImpactPolicy::RecomputeOnRelevantLayer0Change => {
                // Handled by refresh pipeline Phase 2
            }
            ImpactPolicy::UnaffectedByLayer0Refresh => {
                // Skip
            }
        }
    }
    
    Ok(report)
}
```

### Storage Implementation

```rust
impl StorageConnection {
    /// Mark rows as impacted based on provenance matching changed stable keys.
    pub fn mark_impacted_by_provenance(
        &self,
        snapshot_uid: &str,
        family: ArtifactFamily,
        changed_stable_keys: &[String],
        now: &str,
    ) -> Result<u64, StorageError> {
        let table = family.table_name();
        let conn = self.connection();
        
        // Build JSON path query for provenance matching
        // This is SQLite-specific; uses JSON1 extension
        let mut total = 0u64;
        
        for key in changed_stable_keys {
            // Query rows where provenance_json contains this stable_key
            let sql = format!(
                "UPDATE {} SET freshness_state = 'impacted', freshness_updated_at = ?1
                 WHERE snapshot_uid = ?2
                   AND freshness_state = 'current'
                   AND provenance_json LIKE ?3",
                table
            );
            
            let pattern = format!("%\"stable_key\":\"{}\"%", key);
            let count = conn.execute(&sql, rusqlite::params![now, snapshot_uid, pattern])?;
            total += count as u64;
        }
        
        Ok(total)
    }
    
    /// Mark all rows as impacted (for MarkImpactedOnAnyLayer0Change policy).
    pub fn mark_all_impacted(
        &self,
        snapshot_uid: &str,
        family: ArtifactFamily,
        now: &str,
    ) -> Result<u64, StorageError> {
        let table = family.table_name();
        let sql = format!(
            "UPDATE {} SET freshness_state = 'impacted', freshness_updated_at = ?1
             WHERE snapshot_uid = ?2 AND freshness_state = 'current'",
            table
        );
        
        let count = self.connection().execute(&sql, rusqlite::params![now, snapshot_uid])?;
        Ok(count as u64)
    }
}
```

### Refresh Pipeline Integration

In `compose.rs`, after Layer 0-1 refresh completes:

```rust
// After copy-forward and re-extraction of Layer 0-1 artifacts
let changed_stable_keys = collect_changed_stable_keys(
    &extraction_result,
    &copy_forward_result,
    changed_files,
);

// Propagate impact to upper layers
let impact_report = propagate_impact(
    storage,
    &snapshot_uid,
    &changed_stable_keys,
)?;

tracing::info!(
    impacted_boundary_contracts = impact_report.get(ArtifactFamily::BoundaryContracts),
    impacted_inferences = impact_report.get(ArtifactFamily::Inferences),
    "impact propagation complete"
);

// Then proceed to Phase 2: recompute deterministic relationships
// ...
```

### Collecting Changed Stable Keys

```rust
fn collect_changed_stable_keys(
    extraction_result: &ExtractionResult,
    copy_forward_result: &CopyForwardResult,
    changed_files: &[PathBuf],
) -> Vec<String> {
    let mut keys = Vec::new();
    
    // Add FILE node stable keys for changed files
    for file in changed_files {
        let file_key = format!("{}:{}:FILE", repo_uid, file.display());
        keys.push(file_key);
    }
    
    // Add extracted node stable keys from changed files
    for node in &extraction_result.nodes {
        if changed_files.iter().any(|f| node.source_file == *f) {
            keys.push(node.stable_key.clone());
        }
    }
    
    // Add extracted surface stable keys from changed files
    for surface in &extraction_result.surfaces {
        if changed_files.iter().any(|f| surface.source_file == *f) {
            keys.push(surface.stable_key.clone());
        }
    }
    
    // ... similar for edges, measurements, etc.
    
    keys
}
```

## Impact Report Structure

```rust
pub struct ImpactReport {
    counts: HashMap<ArtifactFamily, u64>,
}

impl ImpactReport {
    pub fn add(&mut self, family: ArtifactFamily, count: u64) {
        *self.counts.entry(family).or_insert(0) += count;
    }
    
    pub fn get(&self, family: ArtifactFamily) -> u64 {
        *self.counts.get(&family).unwrap_or(&0)
    }
    
    pub fn total_impacted(&self) -> u64 {
        self.counts.values().sum()
    }
}
```

## Test Matrix

| Scenario | Expected Impact |
|----------|-----------------|
| File changed, inference depends on it | Inference marked impacted |
| File unchanged, inference depends on it | Inference remains current |
| Surface changed, boundary_contract depends on it | boundary_contract marked impacted |
| Schema re-indexed, link depends on it | Link handled by recompute (Phase 2) |
| No Layer 0 changes | No impact propagation |

## Definition of Done

- [x] Impact propagation algorithm implemented (`propagate_impact()` in impact_propagation.rs)
- [x] Provenance populated during inference creation (Spring liveness as first adopter)
- [x] Refresh pipeline calls propagate_impact after copy-forward
- [x] Changed L0 stable keys collected from nodes of changed files
- [x] Tests verify impacted rows are marked correctly (4 unit tests)
- [x] Tests verify current rows are not affected when their provenance is unchanged
- [x] Integration test: Spring inferences have provenance and 'current' freshness
- [ ] Impact report included in refresh diagnostics (follow-on)

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo test -p repo-graph-repo-index --test refresh

# Manual validation
rmap index /path/to/test/repo
# Modify a file
rmap refresh /path/to/test/repo
# Check freshness states
sqlite3 ~/.local/share/repo-graph/repo-graph.db \
  "SELECT freshness_state, COUNT(*) FROM inferences GROUP BY freshness_state"
```

## Performance Considerations

- Provenance LIKE queries may be slow on large tables
- Consider adding a normalized provenance table for efficient joins:
  ```sql
  CREATE TABLE artifact_provenance (
      artifact_family TEXT,
      artifact_uid TEXT,
      depends_on_family TEXT,
      depends_on_stable_key TEXT,
      PRIMARY KEY (artifact_family, artifact_uid, depends_on_stable_key)
  );
  CREATE INDEX idx_provenance_depends ON artifact_provenance(depends_on_stable_key);
  ```
- This is optional optimization; JSON LIKE queries work for moderate scale
- Profile before optimizing

## Notes

- Impact propagation runs once per refresh, after Layer 0-1 is complete
- Rows that are recomputed in Phase 2 (deterministic relationships) don't need impact marking — they're regenerated
- Impact marking is for rows that are NOT recomputed: inferences, surfaces, etc.
- The `MarkImpactedOnAnyLayer0Change` policy is coarse but safe; use for global aggregates
