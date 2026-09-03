//! C++ language extractor for repo-graph.
//!
//! Extracts structural information from C++ source files using tree-sitter-cpp.
//!
//! ## Scope (v1)
//!
//! - FILE nodes (one per file)
//! - SYMBOL nodes: namespaces, classes, structs, enums, methods, constructors,
//!   destructors, functions, type aliases
//! - IMPORTS edges from `#include` directives
//! - CALLS edges from direct call sites (plain and qualified identifiers)
//! - IMPLEMENTS edges from base class clauses (inheritance)
//! - Cyclomatic complexity metrics
//! - C ABI boundary evidence (`extern "C"` linkage detection)
//!
//! ## Explicit Exclusions
//!
//! - Template instantiation tracking (syntax only, no semantic model)
//! - Overload resolution
//! - Macro expansion (source-truth semantics)
//! - compile_commands.json integration (Layer 2, separate slice)
//! - clangd/libclang enrichment (Layer 3, future)
//!
//! ## C ABI Boundary Evidence
//!
//! This extractor detects `extern "C"` linkage specifications and persists
//! them as symbol/file metadata. This enables downstream queries for:
//! - Files with C ABI boundaries
//! - Modules with mixed C/C++ interop surfaces
//! - Wrapper/shim file detection
//!
//! Macro-wrapped linkage (`BEGIN_EXTERN_C`, etc.) is NOT detected.
//!
//! See `docs/milestones/cpp-extractor-v1.md` for full design decisions.

mod extractor;
// IS-TEST-CPP-1: structural gtest/gmock test-marker detection over the parse tree.
// Crate-private — the only caller is this crate's extractor, which rides the result
// on the FILE node's metadata_json for the compose-side reclassify postpass to read.
mod gtest_marker;
mod linkage;
mod metrics;

pub use extractor::CppExtractor;
