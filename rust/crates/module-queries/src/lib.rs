//! Module graph query orchestration for repo-graph.
//!
//! This crate provides a unified read model for module graph data that:
//! - Loads module context, imports, and ownership once
//! - Derives module edges once
//! - Exposes preloaded facts for downstream consumers
//!
//! # Architecture
//!
//! This is a **support module** that sits between storage and CLI:
//! - Depends on `repo-graph-storage` for data access
//! - Depends on `repo-graph-classification` for edge derivation and evaluation
//! - Is consumed by `rgr` CLI commands
//!
//! The module graph loading is the single source of truth for:
//! - Module context with TS/Rust fallback handling
//! - Resolved import loading
//! - Module edge derivation
//! - Violation evaluation from preloaded facts
//!
//! # Boundary Rules
//!
//! This crate owns:
//! - `ModuleGraphFacts` — preloaded module graph data
//! - `load_module_graph_facts` — single-load orchestration
//! - `evaluate_violations_from_facts` — evaluation from preloaded facts
//! - `ModuleQueryContext` — unified module read model (moved from rgr)
//!
//! This crate does NOT own:
//! - Storage queries (belongs in `repo-graph-storage`)
//! - Classification algorithms (belongs in `repo-graph-classification`)
//! - CLI rendering/output (belongs in `repo-graph-rgr`)

mod context;
mod facts;
mod violations;

pub use context::ModuleQueryContext;
pub use facts::{load_module_graph_facts, ModuleGraphFacts, ModuleQueryError};
pub use violations::{
    evaluate_violations_from_facts, DiscoveredModuleViolationsResult,
};
