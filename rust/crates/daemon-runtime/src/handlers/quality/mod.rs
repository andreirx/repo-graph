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
// `pub(crate)`: crate code outside the quality handlers reuses `is_vendored_path`
// (orientation docs, `crate::repo_root`). The working-tree root is NOT resolved here any
// more — it is the registry root (`crate::repo_root`, STATE-ROOT-RELATIVE-REPO-ROOT-1).
// The only callers are intra-crate, so `pub(crate)` (not `pub`) is the minimum
// visibility; the module stays crate-private.
pub(crate) mod support;

#[cfg(test)]
mod tests;

pub use churn::handle_churn;
pub use coverage::handle_coverage;
pub use dead_causes::handle_dead_causes;
pub use hotspots::handle_hotspots;
pub use risk::handle_risk;
