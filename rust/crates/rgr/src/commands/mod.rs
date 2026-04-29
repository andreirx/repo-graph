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

mod graph;
mod index;
mod orient;
mod policy;
mod resource;
mod trust;

pub use graph::{run_callers, run_callees, run_cycles, run_imports, run_path, run_stats};
pub use index::{run_index, run_refresh};
pub use orient::{run_check_cmd, run_explain_cmd, run_orient};
pub use policy::run_policy;
pub use resource::run_resource;
pub use trust::run_trust;
