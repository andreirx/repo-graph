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
mod hotspots;
mod risk;
mod support;

#[cfg(test)]
mod tests;

pub use churn::handle_churn;
pub use coverage::handle_coverage;
pub use hotspots::handle_hotspots;
pub use risk::handle_risk;
