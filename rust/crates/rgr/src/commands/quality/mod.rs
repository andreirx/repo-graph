//! Quality command family.
//!
//! Quality measurement and risk analysis commands:
//! - `churn` — per-file git churn (RS-MS-2)
//! - `hotspots` — churn x complexity analysis (RS-MS-3b)
//! - `metrics` — measurement display
//! - `coverage` — Istanbul/c8 coverage import (RS-MS-4-prereq)
//! - `risk` — hotspot x coverage gap analysis (RS-MS-4)
//!
//! # Boundary rules
//!
//! This module owns quality command-family behavior:
//! - command handlers
//! - family-local DTOs
//! - family-local argument parsing
//! - family-local output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - coverage matching orchestration (lives in `crate::coverage`)
//! - scoring algorithms (belong in support crates)

mod churn;
mod coverage_cmd;
mod hotspots;
mod metrics;
mod risk;

pub use churn::run_churn;
pub use coverage_cmd::run_coverage;
pub use hotspots::run_hotspots;
pub use metrics::run_metrics;
pub use risk::run_risk;
