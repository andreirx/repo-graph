//! Quality handlers for daemon requests.
//!
//! LEGACY-CONTRACT-MIGRATION-1B: Quality family handlers.
//!
//! Module structure:
//! - `churn` — file churn metrics from git history
//! - `hotspots` — hotspot analysis (churn × complexity)
//! - `risk` — risk scoring (hotspot × coverage gap)
//! - `coverage` — coverage import (write operation)
//! - `support` — shared utilities

mod churn;
mod coverage;
mod dead_causes;
mod hotspots;
mod risk;
// INDEX-BASIS-1: `pub(crate)` so the orient/check/explain drift helper (in
// `crate::index_drift`) reuses the SAME `resolve_root_path` (db-relative root_path
// → on-disk git root) the quality handlers use to reach git — one definition, not a
// duplicated path join. The only caller is intra-crate, so `pub(crate)` (not `pub`)
// is the minimum visibility; the module stays crate-private.
pub(crate) mod support;

#[cfg(test)]
mod tests;

pub use churn::handle_churn;
pub use coverage::handle_coverage;
pub use dead_causes::handle_dead_causes;
pub use hotspots::handle_hotspots;
pub use risk::handle_risk;
