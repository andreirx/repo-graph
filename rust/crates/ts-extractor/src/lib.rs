//! repo-graph-ts-extractor — TypeScript/TSX tree-sitter extractor.
//!
//! Concrete `ExtractorPort` adapter for TypeScript, TSX, JavaScript,
//! and JSX files. Uses native tree-sitter with compiled-in grammars.
//!
//! Slice substep state (Rust-6):
//!   - R6-A crate skeleton + parse ........ done
//!   - R6-B FILE node + imports ........... done
//!   - R6-C function/variable extraction .. done
//!   - R6-D class extraction .............. done
//!   - R6-E interface/type/enum ........... done
//!   - R6-F call extraction ............... done
//!   - R6-G metrics extraction ............ done
//!   - R6-H runtime builtins .............. done (shipped in R6-A correction)
//!   - R6-I parity harness ................ done
//!   - R6-J script integration ............ done
//!   - R6-K final acceptance gate ......... done
//!
//! ── Architecture ─────────────────────────────────────────────
//!
//! This crate is an outer-layer adapter. It depends on:
//!   - `repo-graph-indexer` for `ExtractorPort` trait + DTOs
//!   - `repo-graph-classification` for `ImportBinding`, `RuntimeBuiltinsSet`
//!   - `tree-sitter` + `tree-sitter-typescript` for parsing
//!
//! It does NOT depend on storage, classification logic, or trust.
//!
//! ── Language surface ─────────────────────────────────────────
//!
//! Advertises `["typescript", "tsx", "javascript", "jsx"]`.
//! Grammar routing:
//!   - `.ts`, `.js` → TypeScript grammar
//!   - `.tsx`, `.jsx` → TSX grammar
//!
//! ── Locked contract divergence: sync extraction ──────────────
//!
//! The TS `TypeScriptExtractor` uses web-tree-sitter (WASM, async
//! grammar loading). The Rust adapter uses native tree-sitter
//! (compiled C grammars, sync). `initialize()` and `extract()` are
//! synchronous per the R5-A ExtractorPort lock.
//!
//! ── Error behavior (locked at R6-A) ──────────────────────────
//!
//! - `initialize()`: returns `Err(ExtractorError)` on parser/grammar
//!   setup failure.
//! - `extract()`: returns `Err(ExtractorError)` only for true
//!   adapter/setup failures (null parser, unset grammar). Syntax
//!   errors in source produce partial trees with ERROR nodes —
//!   extraction proceeds and emits whatever the visitor finds.
//!   This mirrors the TS extractor, which does not reject
//!   syntactically invalid files.

mod builtins;
mod extractor;
mod metrics;

pub use extractor::TsExtractor;
