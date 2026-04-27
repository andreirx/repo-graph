//! repo-graph-rust-extractor -- Rust source file tree-sitter extractor.
//!
//! Concrete `ExtractorPort` adapter for Rust source files. Uses native
//! tree-sitter with the compiled-in tree-sitter-rust grammar.
//!
//! Slice substep state (Rust extractor v1):
//!   - crate skeleton + parse ............ done
//!   - FILE node + imports ............... done
//!   - function/struct extraction ........ done
//!   - enum/trait/impl extraction ........ done
//!   - const/static/type alias ........... done
//!   - call extraction ................... done
//!   - runtime builtins .................. done
//!   - integration wiring ................ pending (compose.rs)
//!
//! -- Architecture -------------------------------------------------
//!
//! This crate is an outer-layer adapter. It depends on:
//!   - `repo-graph-indexer` for `ExtractorPort` trait + DTOs
//!   - `repo-graph-classification` for `ImportBinding`, `RuntimeBuiltinsSet`
//!   - `tree-sitter` + `tree-sitter-rust` for parsing
//!
//! It does NOT depend on storage, classification logic, or trust.
//!
//! -- Language surface ---------------------------------------------
//!
//! Advertises `["rust"]` as the language identifier.
//! Routes `.rs` files only.
//!
//! -- Locked contract divergence: sync extraction ------------------
//!
//! The TS `RustExtractor` uses web-tree-sitter (WASM, async grammar
//! loading). The Rust adapter uses native tree-sitter (compiled C
//! grammar, sync). `initialize()` and `extract()` are synchronous
//! per the ExtractorPort contract.
//!
//! -- Behavioral contract ------------------------------------------
//!
//! Mirrors the TS RustExtractor at:
//!   src/adapters/extractors/rust/rust-extractor.ts
//!
//! Extracts:
//!   - FILE nodes (one per file)
//!   - SYMBOL nodes for functions, structs, enums, traits, impl methods,
//!     constants, statics, and type aliases
//!   - IMPORTS edges from `use` declarations
//!   - CALLS edges from function/method call expressions
//!   - ImportBinding records for `use` items
//!
//! Visibility: items with `pub` (or `pub(crate)`, `pub(super)`, etc.)
//! are marked EXPORT; items without are PRIVATE.
//!
//! Does not compute complexity metrics in v1 (returns empty metrics map).
//!
//! -- Dedup contract -----------------------------------------------
//!
//! `#[cfg(...)]` conditional compilation can cause tree-sitter to see
//! duplicate definitions (both variants in source text). The extractor
//! deduplicates by stable key: first emission wins. This matches the
//! TS RustExtractor behavior.

mod builtins;
mod extractor;

pub use extractor::RustExtractor;
