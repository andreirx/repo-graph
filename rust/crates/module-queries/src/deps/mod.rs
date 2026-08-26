//! Dependency reconciliation surface (DEP-1).
//!
//! Reconciles declared dependencies (from manifests) with observed
//! external references (from imports) to produce a module-level
//! dependency summary.
//!
//! # Architecture
//!
//! This module is part of `repo-graph-module-queries`, the support
//! layer between storage and CLI. It owns:
//!
//! - `types.rs` — DTOs for dependency summaries
//! - `normalize.rs` — specifier → package normalization (npm, Cargo)
//! - `reconcile.rs` — join declared + observed + builtins
//! - `compose.rs` — orchestration layer that loads data from storage
//!
//! This module does NOT own:
//! - Storage queries (belongs in `repo-graph-storage`)
//! - CLI rendering/output (belongs in `repo-graph-rgr`)
//!
//! # Future extraction seam
//!
//! If DEP-2/DEP-3 need to reuse this logic, or a second non-CLI
//! consumer appears, extract to a dedicated `deps-reconcile` crate.

mod builtins;
mod classify;
mod compose;
mod normalize;
mod reconcile;
mod resolve;
mod types;

pub use builtins::deps_runtime_builtins;

pub use compose::{
    cargo_runtime_builtins, compose_dependency_summaries, npm_runtime_builtins,
    ComposeDependenciesInput, ComposeDependenciesResult,
};
pub use normalize::{normalize_cargo_specifier, normalize_npm_specifier};
pub use reconcile::{reconcile_module_dependencies, ReconcileInput};
pub use resolve::{build_identifier_resolution_map, resolve_import_specifier};
pub use types::{
    DependencyCategory, DependencyEntry, DriftEntry, DriftKind, ManifestContext,
    ManifestProvenance, ModuleDependencySummary, PackageUsage, ProvenanceRead,
};
