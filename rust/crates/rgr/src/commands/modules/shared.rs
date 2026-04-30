//! Shared types and re-exports for modules command family.
//!
//! # Boundary rules
//!
//! This module provides thin re-exports from the `repo-graph-module-queries`
//! support crate. CLI commands should import through this module for
//! consistent access patterns within the CLI family.
//!
//! This module does **not** own:
//! - Module graph loading (belongs in `repo-graph-module-queries`)
//! - Violation evaluation (belongs in `repo-graph-module-queries`)

// Re-export from support crate for CLI convenience
pub use repo_graph_module_queries::{
    evaluate_violations_from_facts, load_module_graph_facts, ModuleQueryContext,
};
