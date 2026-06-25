//! Graph algorithms for code analysis.
//!
//! Pure algorithmic implementations with no storage or presentation concerns.
//! Input is abstract node IDs and edges. Output is structural results.
//!
//! # Algorithms
//!
//! - **Tarjan SCC**: O(V+E) strongly connected component detection
//!
//! # Design Principles
//!
//! - No external dependencies beyond std
//! - Deterministic output ordering
//! - Node IDs are opaque strings (caller maps to/from domain types)
//! - No human-readable formatting (caller's responsibility)

mod scc;

pub use scc::{
    find_sccs, find_sccs_cancellable, CancelCheck, Cancelled, DirectedEdge, Scc, SccResult,
};
