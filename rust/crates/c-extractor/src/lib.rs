//! C language extractor for repo-graph.
//!
//! Extracts structural information from C source files using tree-sitter-c.
//!
//! ## Scope (v1)
//!
//! - FILE nodes (one per file)
//! - SYMBOL nodes: functions (definitions only), structs, enums, typedefs
//! - IMPORTS edges from `#include` directives
//! - CALLS edges from direct function calls (plain identifiers only)
//! - Cyclomatic complexity metrics
//!
//! ## Explicit Exclusions
//!
//! - Function declarations/prototypes (definitions only)
//! - C++ constructs (separate extractor)
//! - Macro expansion (source-truth semantics)
//! - Function pointer calls (marked unresolved)
//!
//! See `docs/milestones/c-extractor-v1.md` for full design decisions.

pub mod boundary_detector;
mod extractor;
mod metrics;

pub use boundary_detector::{
    extract_boundary_calls, is_boundary_function, MmapFlags, RawBoundaryCall, SocketFamily,
    SocketType,
};
pub use extractor::CExtractor;
