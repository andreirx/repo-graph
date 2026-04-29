//! Policy-facts extraction support module for repo-graph.
//!
//! Extracts cross-layer policy propagation patterns from source code.
//!
//! ## PF-1: STATUS_MAPPING
//!
//! Status/error translation functions from C:
//! - Switch on any qualifying parameter (including pointer dereference)
//! - Enum-to-enum and numeric-to-enum mappings
//! - Fallthrough case grouping
//! - Expression returns preserved as normalized text
//!
//! ## PF-2: BEHAVIORAL_MARKER
//!
//! Behavioral markers from C indicating policy-relevant control flow:
//! - RETRY_LOOP: loops with sleep/delay that retry operations
//! - RESUME_OFFSET: curl CURLOPT_RESUME_FROM* patterns
//!
//! ## Design
//!
//! See `docs/design/policy-facts-support-module.md` for architecture.
//! See `docs/slices/pf-1-status-mapping.md` for STATUS_MAPPING spec.
//! See `docs/slices/pf-2-behavioral-marker.md` for BEHAVIORAL_MARKER spec.
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
pub use types::{BehavioralMarker, CaseMapping, MarkerEvidence, MarkerKind, StatusMapping};
