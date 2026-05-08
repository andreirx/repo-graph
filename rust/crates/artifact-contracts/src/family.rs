//! Artifact family enumeration.
//!
//! The unit of architecture is the artifact family, not the database table.
//! A family may map to one table, multiple tables, a read model with no table,
//! or an inferred surface built from lower facts.

use serde::{Deserialize, Serialize};

/// All artifact families in the repo-graph system.
///
/// Each family has an explicit contract defining its truth class, refresh
/// policy, identity model, degradation policy, and provenance requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFamily {
    // ═══════════════════════════════════════════════════════════════════
    // Layer 0-1: Extracted Facts
    // Directly source-owned, deterministic, file/parse-unit anchored.
    // ═══════════════════════════════════════════════════════════════════

    /// File path + content hash tracking.
    /// Table: `file_versions`
    FileVersions,

    /// AST-derived symbols and file markers.
    /// Table: `nodes`
    Nodes,

    /// AST-derived relationships (calls, imports, contains).
    /// Table: `edges`
    Edges,

    /// File-local deterministic metrics (complexity, line counts).
    /// Table: `measurements`
    Measurements,

    /// IPC/network call sites detected in source files.
    /// Table: `boundary_interaction_surfaces`
    BoundaryInteractionSurfaces,

    /// Channel metadata anchored to boundary surfaces.
    /// Table: `boundary_channel_details`
    BoundaryChannelDetails,

    /// Proto/schema file parse output.
    /// Table: `contract_schemas`
    ContractSchemas,

    /// Schema-internal elements (messages, fields, services).
    /// Table: `contract_elements`
    ContractElements,

    /// Extracted governance annotations from source.
    /// Table: `policy_facts`
    PolicyFacts,

    // ═══════════════════════════════════════════════════════════════════
    // Layer 2: Deterministic Relationships
    // Computed from current lower-layer facts. Not source-owned by one file.
    // ═══════════════════════════════════════════════════════════════════

    /// Surface-to-contract-element mapping.
    /// Table: `boundary_contracts`
    BoundaryContracts,

    /// gRPC service-to-implementation links.
    /// Table: `boundary_interaction_links`
    BoundaryInteractionLinks,

    // ═══════════════════════════════════════════════════════════════════
    // Layer 3: Hints / Inferences
    // Evidence-backed but partial, heuristic, degradable.
    // ═══════════════════════════════════════════════════════════════════

    /// Derived facts with confidence (dead-code candidates, trust signals).
    /// Table: `inferences`
    Inferences,

    /// Discovered module boundaries (manifest-backed and inferred).
    /// Table: `module_candidates`
    ModuleCandidates,

    /// Runtime/build surface discovery.
    /// Table: `project_surfaces`
    ProjectSurfaces,

    /// Evidence supporting project surface discovery.
    /// Table: `project_surface_evidence`
    ProjectSurfaceEvidence,

    /// Entrypoint markers for surfaces.
    /// Table: `surface_entrypoints`
    SurfaceEntrypoints,

    /// Config root markers for surfaces.
    /// Table: `surface_config_roots`
    SurfaceConfigRoots,

    /// Environment dependencies for surfaces.
    /// Table: `surface_env_dependencies`
    SurfaceEnvDependencies,

    /// Evidence for environment dependencies.
    /// Table: `surface_env_evidence`
    SurfaceEnvEvidence,

    /// Filesystem mutation markers.
    /// Table: `surface_fs_mutations`
    SurfaceFsMutations,

    /// Evidence for filesystem mutations.
    /// Table: `surface_fs_mutation_evidence`
    SurfaceFsMutationEvidence,

    // ═══════════════════════════════════════════════════════════════════
    // Layer 4: Governance Overlays
    // Human-authored policy and declarations.
    // ═══════════════════════════════════════════════════════════════════

    /// Quality gate rules and boundary declarations.
    /// Table: `requirement_declarations` (filtered by kind)
    RequirementDeclarations,

    /// Temporary exemptions from gate failures.
    /// Table: `waivers`
    Waivers,
}

impl ArtifactFamily {
    /// Returns the primary database table name for this family.
    ///
    /// Note: Some families may span multiple tables or have no direct table.
    /// This returns the primary table for storage operations.
    pub fn table_name(&self) -> &'static str {
        match self {
            Self::FileVersions => "file_versions",
            Self::Nodes => "nodes",
            Self::Edges => "edges",
            Self::Measurements => "measurements",
            Self::BoundaryInteractionSurfaces => "boundary_interaction_surfaces",
            Self::BoundaryChannelDetails => "boundary_channel_details",
            Self::ContractSchemas => "contract_schemas",
            Self::ContractElements => "contract_elements",
            Self::PolicyFacts => "policy_facts",
            Self::BoundaryContracts => "boundary_contracts",
            Self::BoundaryInteractionLinks => "boundary_interaction_links",
            Self::Inferences => "inferences",
            Self::ModuleCandidates => "module_candidates",
            Self::ProjectSurfaces => "project_surfaces",
            Self::ProjectSurfaceEvidence => "project_surface_evidence",
            Self::SurfaceEntrypoints => "surface_entrypoints",
            Self::SurfaceConfigRoots => "surface_config_roots",
            Self::SurfaceEnvDependencies => "surface_env_dependencies",
            Self::SurfaceEnvEvidence => "surface_env_evidence",
            Self::SurfaceFsMutations => "surface_fs_mutations",
            Self::SurfaceFsMutationEvidence => "surface_fs_mutation_evidence",
            Self::RequirementDeclarations => "declarations",
            Self::Waivers => "declarations",
        }
    }

    /// Returns all artifact families.
    pub fn all() -> &'static [ArtifactFamily] {
        &[
            // Layer 0-1
            Self::FileVersions,
            Self::Nodes,
            Self::Edges,
            Self::Measurements,
            Self::BoundaryInteractionSurfaces,
            Self::BoundaryChannelDetails,
            Self::ContractSchemas,
            Self::ContractElements,
            Self::PolicyFacts,
            // Layer 2
            Self::BoundaryContracts,
            Self::BoundaryInteractionLinks,
            // Layer 3
            Self::Inferences,
            Self::ModuleCandidates,
            Self::ProjectSurfaces,
            Self::ProjectSurfaceEvidence,
            Self::SurfaceEntrypoints,
            Self::SurfaceConfigRoots,
            Self::SurfaceEnvDependencies,
            Self::SurfaceEnvEvidence,
            Self::SurfaceFsMutations,
            Self::SurfaceFsMutationEvidence,
            // Layer 4
            Self::RequirementDeclarations,
            Self::Waivers,
        ]
    }
}

impl std::fmt::Display for ArtifactFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
