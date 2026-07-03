//! Utility functions for daemon runtime.

pub mod context;
pub mod measurement_coverage;
pub mod time;
pub mod trust;

pub use context::compute_storage_root_path;
pub use measurement_coverage::{measurement_coverage_json, measurement_coverage_unavailable_json};
pub use time::utc_now_iso8601;
pub use trust::compute_trust_overlay_for_snapshot;
