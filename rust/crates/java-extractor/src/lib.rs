//! repo-graph-java-extractor — Java tree-sitter extractor.
//!
//! Concrete `ExtractorPort` adapter for Java files. Uses native
//! tree-sitter with compiled-in grammar.
//!
//! ── Architecture ─────────────────────────────────────────────
//!
//! This crate is an outer-layer adapter. It depends on:
//!   - `repo-graph-indexer` for `ExtractorPort` trait + DTOs
//!   - `repo-graph-classification` for `ImportBinding`, `RuntimeBuiltinsSet`
//!   - `tree-sitter` + `tree-sitter-java` for parsing
//!
//! It does NOT depend on storage, classification logic, or trust.
//!
//! ── Language surface ─────────────────────────────────────────
//!
//! Advertises `["java"]`. All `.java` files route to this extractor.
//!
//! ── Extraction scope (v1) ────────────────────────────────────
//!
//! Includes:
//!   - FILE node (one per file)
//!   - SYMBOL nodes: classes, interfaces, enums, records, methods,
//!     constructors, fields
//!   - IMPORTS edges from import statements
//!   - CALLS edges from method invocations (syntactic only)
//!   - INSTANTIATES edges from constructor calls (new ClassName())
//!   - IMPLEMENTS edges from implements/extends clauses
//!   - Annotations captured as metadata_json (raw, no interpretation)
//!
//! Excludes (design doc deferred items):
//!   - Overload call resolution (requires type solver)
//!   - Inheritance dispatch (requires class hierarchy)
//!   - Spring annotation semantics (separate detector layer)
//!   - Maven/Gradle dependency resolution
//!
//! ── Locked contract divergence: sync extraction ──────────────
//!
//! Same as TS extractor: sync `initialize()` and `extract()` per
//! the R5-A ExtractorPort lock.
//!
//! ── Error behavior ───────────────────────────────────────────
//!
//! - `initialize()`: returns `Err(ExtractorError)` on parser/grammar
//!   setup failure.
//! - `extract()`: returns `Err(ExtractorError)` only for true
//!   adapter/setup failures (null parser, unset grammar). Syntax
//!   errors in source produce partial trees with ERROR nodes —
//!   extraction proceeds and emits whatever the visitor finds.
//!
//! Design doc: `docs/design/java-rust-extractor-v1.md`

mod builtins;
mod extractor;
mod metrics;

pub use extractor::JavaExtractor;
