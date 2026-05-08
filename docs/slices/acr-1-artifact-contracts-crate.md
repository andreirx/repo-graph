# ACR-1: Artifact Contracts Crate

Status: DONE
Depends: None (foundational)
Follow-on: `acr-2-refresh-policy-integration.md`
Track: Core Infrastructure — Artifact Contract Registry

## Objective

Create the `artifact-contracts` support crate with all type definitions and the canonical artifact family registry.

This is the foundation for all subsequent ACR slices. No refresh behavior changes in this slice.

## Scope

### In Scope

- Create `rust/crates/artifact-contracts` crate
- Define all enums: `ArtifactFamily`, `TruthKind`, `RefreshPolicy`, `IdentityPolicy`, `DegradationPolicy`, `ProvenancePolicy`, `ImpactPolicy`, `FreshnessTracking`
- Define `ArtifactContract` struct
- Define `ClassificationMaturity` enum (allows provisional classifications)
- Implement registry accessor functions
- Register ALL currently relevant artifact families
- Add completeness and coherence tests
- Add crate documentation

### Out of Scope

- Refresh pipeline changes
- Schema changes
- Query behavior changes
- Per-row provenance storage
- Impact propagation logic

## Deliverables

### 1. Crate Structure

```
rust/crates/artifact-contracts/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API
│   ├── family.rs        # ArtifactFamily enum
│   ├── truth_kind.rs    # TruthKind enum
│   ├── refresh.rs       # RefreshPolicy enum
│   ├── identity.rs      # IdentityPolicy enum
│   ├── degradation.rs   # DegradationPolicy enum
│   ├── provenance.rs    # ProvenancePolicy enum
│   ├── impact.rs        # ImpactPolicy enum
│   ├── freshness.rs     # FreshnessTracking, FreshnessState enums
│   ├── contract.rs      # ArtifactContract struct
│   ├── maturity.rs      # ClassificationMaturity enum
│   └── registry.rs      # get_contract(), all_families(), etc.
└── tests/
    ├── completeness.rs  # Every family has a contract
    └── coherence.rs     # Policy combinations are valid
```

### 2. Type Definitions

```rust
// family.rs
pub enum ArtifactFamily {
    // Layer 0-1: Extracted Facts
    FileVersions,
    Nodes,
    Edges,
    Measurements,
    BoundaryInteractionSurfaces,
    BoundaryChannelDetails,
    ContractSchemas,
    ContractElements,
    PolicyFacts,
    
    // Layer 2: Deterministic Relationships
    BoundaryContracts,
    BoundaryInteractionLinks,
    
    // Layer 3: Hints / Inferences
    Inferences,
    ModuleCandidates,
    ProjectSurfaces,
    ProjectSurfaceEvidence,
    SurfaceEntrypoints,
    SurfaceConfigRoots,
    SurfaceEnvDependencies,
    SurfaceEnvEvidence,
    SurfaceFsMutations,
    SurfaceFsMutationEvidence,
    
    // Layer 4: Governance Overlays
    RequirementDeclarations,
    Waivers,
}

// truth_kind.rs
pub enum TruthKind {
    ExtractedFact,
    DeterministicRelationship,
    Projection,
    Inference,
    GovernanceOverlay,
}

// refresh.rs
pub enum RefreshPolicy {
    ReextractChangedInputs,
    RecomputeFromCurrentSnapshot,
    CopyForwardWithFkRemap,
    MarkImpactedDeferRecompute,
    NeverCopyForward,
    SnapshotIndependent,
}

// identity.rs
pub enum IdentityPolicy {
    StableLogicalKey,
    SnapshotRowId,
    DerivedFromCurrentSnapshot,
    SnapshotIndependent,
}

// degradation.rs
pub enum DegradationPolicy {
    MustBePresent,
    MayBeOmittedWithExplicitUnknown,
    MustTriggerWarning,
    MustTriggerRebuildRecommendation,
    UnsupportedOnEmbodiment,
}

// provenance.rs
pub enum ProvenancePolicy {
    DirectFromSourceFile,
    DerivedFromLayer0Items,
    DerivedFromArtifactFamilies,
    HumanAuthored,
}

// impact.rs
pub enum ImpactPolicy {
    RecomputeOnRelevantLayer0Change,
    MarkImpactedOnRelevantLayer0Change,
    MarkImpactedOnAnyLayer0Change,
    UnaffectedByLayer0Refresh,
}

// freshness.rs
pub enum FreshnessTracking {
    PerRow,
    ImplicitFromSource,
    None,
}

pub enum FreshnessState {
    Current,
    Impacted,
    Stale,
    Unknown,
}

// maturity.rs
pub enum ClassificationMaturity {
    Stable,
    Provisional,
    Experimental,
}

// contract.rs
pub struct ArtifactContract {
    pub family: ArtifactFamily,
    pub truth_kind: TruthKind,
    pub refresh_policy: RefreshPolicy,
    pub identity_policy: IdentityPolicy,
    pub degradation_policy: DegradationPolicy,
    pub provenance_policy: ProvenancePolicy,
    pub impact_policy: ImpactPolicy,
    pub freshness_tracking: FreshnessTracking,
    pub classification_maturity: ClassificationMaturity,
    pub layer_dependencies: Vec<ArtifactFamily>,
    pub description: &'static str,
}
```

### 3. Registry API

```rust
// registry.rs

/// Get the contract for a specific artifact family.
pub fn get_contract(family: ArtifactFamily) -> &'static ArtifactContract;

/// Get all registered artifact families.
pub fn all_families() -> &'static [ArtifactFamily];

/// Get families by truth kind.
pub fn families_by_truth_kind(kind: TruthKind) -> Vec<ArtifactFamily>;

/// Get families that require provenance tracking.
pub fn families_with_provenance() -> Vec<ArtifactFamily>;

/// Get families that participate in refresh.
pub fn families_for_refresh() -> Vec<ArtifactFamily>;

/// Get families that require per-row freshness tracking.
pub fn families_with_freshness_tracking() -> Vec<ArtifactFamily>;
```

### 4. Tests

**Completeness Tests:**
- Every `ArtifactFamily` variant has a registered contract
- Every contract has all required fields populated
- No `ArtifactFamily::Unknown` or placeholder variants

**Coherence Tests:**
- `ExtractedFact` families have `DirectFromSourceFile` provenance
- `DeterministicRelationship` families have `RecomputeFromCurrentSnapshot` or `CopyForwardWithFkRemap` refresh policy
- `SnapshotIndependent` identity implies `SnapshotIndependent` refresh
- `UnsupportedOnEmbodiment` degradation is only on known unsupported families
- Families with `DerivedFromLayer0Items` provenance have non-empty `layer_dependencies`

## Family Classification Reference

Refer to `docs/architecture/artifact-contract-model.md` for the canonical classification of each family.

Key classifications for this slice:

| Family | Truth Kind | Refresh Policy | Maturity |
|--------|------------|----------------|----------|
| Nodes | ExtractedFact | ReextractChangedInputs | Stable |
| Edges | ExtractedFact | ReextractChangedInputs | Stable |
| BoundaryInteractionSurfaces | ExtractedFact | ReextractChangedInputs | Stable |
| BoundaryContracts | DeterministicRelationship | RecomputeFromCurrentSnapshot | Stable |
| BoundaryInteractionLinks | DeterministicRelationship | RecomputeFromCurrentSnapshot | Stable |
| ModuleCandidates | Inference | RecomputeFromCurrentSnapshot | Provisional |
| ProjectSurfaces | Inference | RecomputeFromCurrentSnapshot | Provisional |
| Inferences | Inference | MarkImpactedDeferRecompute | Provisional |

## Definition of Done

- [ ] Crate compiles with no warnings
- [ ] All artifact families registered
- [ ] All completeness tests pass
- [ ] All coherence tests pass
- [ ] Crate documentation exists
- [ ] No refresh behavior changed
- [ ] No schema changes
- [ ] Cargo workspace includes the new crate

## Validation Commands

```bash
cd /Users/apple/Documents/APLICATII\ BIJUTERIE/repo-graph/rust
cargo build -p artifact-contracts
cargo test -p artifact-contracts
cargo doc -p artifact-contracts --no-deps
```

## Notes

- Use `&'static` references where possible to avoid allocation in registry lookups
- Consider using `phf` or similar for compile-time registry if performance matters
- The `Provisional` maturity level allows honest classification of families whose semantics are still being determined
- This slice is foundational — do not rush classifications that are uncertain
