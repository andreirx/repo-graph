//! Quality policy assessment orchestration.
//!
//! This crate provides the use-case layer for quality policy evaluation.
//! It orchestrates the flow from loading policies and measurements through
//! evaluation and persistence.
//!
//! # Architecture
//!
//! This is an APPLICATION LAYER crate that depends on:
//! - `repo-graph-quality-policy`: pure domain assessment engine
//! - `repo-graph-storage`: types and storage port trait
//!
//! The storage crate defines `QualityPolicyStoragePort` trait and implements
//! it for `StorageConnection`. The runner is generic over that trait.
//!
//! # Usage
//!
//! ```ignore
//! let runner = QualityPolicyRunner::new(storage);
//! let result = runner.assess_snapshot("repo-uid", "snap-123", Some("snap-baseline"))?;
//! ```
//!
//! # Error Handling
//!
//! - Invalid policy payloads fail at use-case boundary (not silently skipped)
//! - Storage errors propagate through [`RunnerError`]
//! - Assessment results include counts by verdict

mod error;
mod runner;

pub use error::RunnerError;
pub use runner::{AssessmentResult, QualityPolicyRunner};

// Re-export the port trait and types from storage for convenience.
pub use repo_graph_storage::quality_policy_port::{
    EnrichedMeasurement, LoadedPolicy, QualityPolicyStoragePort,
};
