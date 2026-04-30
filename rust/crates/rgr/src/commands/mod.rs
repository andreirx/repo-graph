//! Command family implementations.
//!
//! # Boundary rules
//!
//! This module owns **command-family behavior only**:
//! - `run_*` handler functions
//! - family-specific DTOs
//! - family-local argument parsing
//! - family-local rendering helpers
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - cross-family helpers (belong in `crate::cli` or support crates)
//! - domain/application logic (belongs in support crates)
//!
//! Each submodule changes for one actor/reason only.
//!
//! Command families use shared infrastructure from `crate::cli`.

mod assess;
mod dead;
mod declare;
mod docs;
mod gate;
mod graph;
mod index;
mod modules;
mod orient;
mod policy;
mod quality;
mod resource;
mod surfaces;
mod trust;

pub use assess::run_assess;
pub use dead::run_dead;
pub use declare::run_declare;
pub use docs::run_docs;
pub use gate::run_gate;
pub use graph::{run_callers, run_callees, run_cycles, run_imports, run_path, run_stats};
pub use index::{run_index, run_refresh};
pub use modules::{run_modules, run_violations};
pub use orient::{run_check_cmd, run_explain_cmd, run_orient};
pub use policy::run_policy;
pub use quality::{run_churn, run_coverage, run_hotspots, run_metrics, run_risk};
pub use resource::run_resource;
pub use surfaces::run_surfaces;
pub use trust::run_trust;
