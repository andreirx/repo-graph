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
//! The retired TS prototype's `RustExtractor` used web-tree-sitter
//! (WASM, async grammar loading); this adapter was locked to native
//! tree-sitter (compiled C grammar, sync). The prototype is gone
//! (TS-PROTOTYPE-RETIREMENT-1; archived in git), but the contract
//! stands: `initialize()` and `extract()` are synchronous per the
//! ExtractorPort contract.
//!
//! -- Behavioral contract ------------------------------------------
//!
//! Behavior ported from the retired TS prototype's RustExtractor
//! (`src/adapters/extractors/rust/rust-extractor.ts`, removed by
//! TS-PROTOTYPE-RETIREMENT-1; archived in git history, last release
//! containing it: v0.4.0). This crate now owns the contract.
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
//! Complexity metrics: emits `cyclomatic_complexity`, `parameter_count`, and
//! `max_nesting_depth` per function/method/default-trait-method, using the same
//! decision-point counting rules as the C/TS extractors so values are
//! comparable (METRIC-LANG-COVERAGE-1). See `metrics.rs`. `function_length` and
//! `cognitive_complexity` remain unmeasured for Rust (mirrors c-extractor).
//!
//! -- Dedup contract -----------------------------------------------
//!
//! `#[cfg(...)]` conditional compilation can cause tree-sitter to see
//! duplicate definitions (both variants in source text). The extractor
//! deduplicates by stable key: first emission wins. This matches the
//! TS RustExtractor behavior.

mod builtins;
mod extractor;
mod metrics;

pub use extractor::RustExtractor;
