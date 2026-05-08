//! Artifact Contract Registry
//!
//! This crate defines the canonical artifact ontology for repo-graph.
//! Every persisted artifact family has an explicit contract defining:
//!
//! - **Truth kind**: What kind of data is this? (extracted fact, relationship, inference, overlay)
//! - **Refresh policy**: How does it behave during incremental refresh?
//! - **Identity policy**: How is row identity tracked?
//! - **Degradation policy**: How are missing/incomplete states reported?
//! - **Provenance policy**: How is provenance to Layer 0 tracked?
//! - **Impact policy**: What happens when upstream Layer 0 changes?
//! - **Freshness tracking**: How is per-row freshness state tracked?
//!
//! # Architecture
//!
//! The unit of architecture is the **artifact family**, not the database table.
//! A family may map to one table, multiple tables, or a read model with no table.
//!
//! # Layer Model
//!
//! - **Layer 0-1**: Extracted facts (nodes, edges, measurements, surfaces, schemas)
//! - **Layer 2**: Deterministic relationships (boundary_contracts, boundary_interaction_links)
//! - **Layer 3**: Hints/inferences (inferences, module_candidates, project_surfaces)
//! - **Layer 4**: Governance overlays (requirement_declarations, waivers)
//!
//! # Usage
//!
//! ```rust
//! use artifact_contracts::{ArtifactFamily, registry, TruthKind};
//!
//! // Get the contract for a specific family
//! let contract = registry::get_contract(ArtifactFamily::BoundaryInteractionLinks);
//! assert_eq!(contract.truth_kind, TruthKind::DeterministicRelationship);
//!
//! // Get all families by truth kind
//! let relationships = registry::families_by_truth_kind(TruthKind::DeterministicRelationship);
//! assert!(relationships.contains(&ArtifactFamily::BoundaryContracts));
//!
//! // Get families that need per-row freshness tracking
//! let freshness_families = registry::families_with_freshness_tracking();
//! ```
//!
//! # References
//!
//! - `docs/architecture/artifact-contract-model.md` — full specification
//! - `docs/architecture/adr/adr-artifact-contract-registry.md` — decision record

// ═══════════════════════════════════════════════════════════════════════════
// Module declarations
// ═══════════════════════════════════════════════════════════════════════════

mod contract;
mod degradation;
mod family;
mod freshness;
mod identity;
mod impact;
mod maturity;
mod provenance;
mod refresh;
mod truth_kind;

pub mod registry;

// ═══════════════════════════════════════════════════════════════════════════
// Public re-exports
// ═══════════════════════════════════════════════════════════════════════════

pub use contract::ArtifactContract;
pub use degradation::DegradationPolicy;
pub use family::ArtifactFamily;
pub use freshness::{FreshnessFilter, FreshnessState, FreshnessTracking};
pub use identity::IdentityPolicy;
pub use impact::ImpactPolicy;
pub use maturity::ClassificationMaturity;
pub use provenance::{Provenance, ProvenanceAnchor, ProvenancePolicy};
pub use refresh::RefreshPolicy;
pub use truth_kind::TruthKind;

// ═══════════════════════════════════════════════════════════════════════════
// Convenience re-exports from registry
// ═══════════════════════════════════════════════════════════════════════════

pub use registry::{
    all_families, families_by_layer, families_by_truth_kind, families_for_refresh,
    families_with_freshness_tracking, families_with_provenance, get_contract,
    unsupported_families,
};
