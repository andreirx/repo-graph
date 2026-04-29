//! Policy-facts extraction support module for repo-graph.
//!
//! Extracts cross-layer policy propagation patterns from source code.
//!
//! ## PF-1 Scope
//!
//! STATUS_MAPPING extraction from C:
//! - Status/error translation functions (switch on any qualifying parameter)
//! - Supports pointer dereference discriminants (`*param`)
//! - Enum-to-enum and numeric-to-enum mappings
//! - Fallthrough case grouping
//! - Expression returns preserved as normalized text
//!
//! ## Design
//!
//! See `docs/design/policy-facts-support-module.md` for architecture.
//! See `docs/slices/pf-1-status-mapping.md` for PF-1 implementation spec.
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_policy_facts::extractors::status_mapping::extract_status_mappings;
//!
//! let tree = parser.parse(source, None).unwrap();
//! let mappings = extract_status_mappings(&tree, source.as_bytes(), file_path, repo_uid);
//! ```

mod types;
pub mod extractors;
pub mod storage_port;

pub use storage_port::{PolicyFactsStorageError, PolicyFactsStorageRead, PolicyFactsStorageWrite};
pub use types::{CaseMapping, StatusMapping};
