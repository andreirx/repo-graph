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
mod boundaries;
mod contracts;
mod dead;
mod declare;
mod deps;
mod docs;
mod doctor;
mod enrich;
mod gate;
mod graph;
mod hook;
mod index;
mod inferences;
mod integrate;
mod maintenance;
mod modules;
mod orient;
mod perf;
mod policy;
mod quality;
mod repo;
mod resource;
mod surfaces;
mod trust;
mod uninstall;

pub use assess::run_assess;
pub use boundaries::run_boundaries;
pub use contracts::run_contracts;
pub use dead::run_dead;
pub use declare::run_declare;
pub use deps::run_deps;
pub use docs::run_docs;
pub use doctor::run_doctor;
pub use enrich::run_enrich;
pub use gate::run_gate;
pub use graph::{run_callees, run_callers, run_cycles, run_dev, run_imports, run_path, run_stats};
pub use hook::run_hook;
pub use index::{run_index, run_refresh};
pub use inferences::run_inferences;
pub use integrate::run_integrate;
pub use maintenance::run_maintenance;
pub use modules::{run_modules, run_violations};
pub use orient::{run_check_cmd, run_explain_cmd, run_orient};
pub use perf::run_perf;
pub use policy::run_policy;
pub use quality::{run_churn, run_coverage, run_hotspots, run_metrics, run_risk};
pub use repo::run_repo;
pub use resource::run_resource;
pub use surfaces::run_surfaces;
pub use trust::run_trust;
pub use uninstall::run_uninstall;
