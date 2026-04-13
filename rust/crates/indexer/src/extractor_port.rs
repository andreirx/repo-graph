//! Extractor port — the interface extractors must implement.
//!
//! Mirror of `ExtractorPort` from `src/core/ports/extractor.ts`.
//!
//! This trait is defined by the indexer (policy) and implemented
//! by concrete extractor adapters (e.g., a future tree-sitter
//! TypeScript extractor in Rust-6). The dependency direction is
//! adapter → policy.
//!
//! ── Sync vs async (locked contract divergence) ───────────────
//!
//! The TS `ExtractorPort` declares `extract()` and `initialize()`
//! as `Promise`-returning (async). The Rust trait is synchronous.
//! This is a deliberate divergence, not an accidental narrowing:
//!
//!   1. Tree-sitter parsing is CPU-bound in both runtimes. The TS
//!      `Promise` wrapper is a Node.js convention, not a signal
//!      that extraction has I/O.
//!
//!   2. The Rust indexer receives source text from the caller (no
//!      file I/O inside `extract()`). There is no blocking I/O
//!      that would benefit from async.
//!
//!   3. `async fn` in Rust traits is not object-safe. The indexer
//!      needs `&dyn ExtractorPort` for heterogeneous extractor
//!      collections. Making `extract()` async would require
//!      `async_trait` or `Box<dyn Future>` return types, adding
//!      allocation and complexity for zero architectural benefit.
//!
//!   4. If a future adapter needs async (e.g., LSP-based
//!      enrichment), that is a separate enrichment port, not the
//!      base extraction port. The TS codebase already separates
//!      extraction (tree-sitter, sync under the hood) from
//!      enrichment (TypeChecker/rust-analyzer, genuinely async).
//!
//! The `initialize()` method IS preserved from the TS contract
//! (synchronous form) to allow one-time setup (grammar loading,
//! WASM compilation) before first use. Dropping it would force
//! adapters to hide initialization behind global state or
//! constructor side effects.

use repo_graph_classification::types::RuntimeBuiltinsSet;

use crate::types::ExtractionResult;

/// Error type for extraction failures.
///
/// Extraction can fail due to parser errors, malformed source,
/// or internal extractor bugs. The error carries a human-readable
/// message. The indexer orchestrator catches extraction errors
/// per-file and records them as `ParseStatus::Failed` without
/// aborting the entire indexing run.
#[derive(Debug)]
pub struct ExtractorError {
	pub message: String,
}

impl std::fmt::Display for ExtractorError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "extraction failed: {}", self.message)
	}
}

impl std::error::Error for ExtractorError {}

/// The interface a language extractor must implement.
///
/// An extractor takes source text for a single file and produces
/// nodes (symbols), unresolved edges (symbolic references), metrics,
/// and import bindings. The indexer orchestrator routes files to
/// extractors by language/extension and collects results.
///
/// The trait is object-safe (`&dyn ExtractorPort`) so the indexer
/// can hold a heterogeneous collection of extractors.
pub trait ExtractorPort {
	/// Human-readable name and version. e.g., `"ts-base:1.0.0"`.
	fn name(&self) -> &str;

	/// Language identifiers this extractor handles.
	/// e.g., `["typescript", "tsx"]`.
	///
	/// The indexer uses `language_to_extensions()` to map these
	/// to file extensions for routing.
	fn languages(&self) -> &[String];

	/// Runtime builtins known to this extractor's language ecosystem.
	/// Used by the classifier to distinguish runtime globals from
	/// project symbols.
	fn runtime_builtins(&self) -> &RuntimeBuiltinsSet;

	/// One-time initialization before first use. Mirrors the TS
	/// contract's `initialize(): Promise<void>`.
	///
	/// Concrete adapters use this for grammar loading, WASM
	/// compilation, or other one-time setup that must complete
	/// before `extract()` can be called. The indexer orchestrator
	/// calls `initialize()` on each registered extractor before
	/// starting the extraction phase.
	///
	/// # Errors
	/// Returns `Err(ExtractorError)` if initialization fails
	/// (e.g., grammar file not found, WASM compilation error).
	fn initialize(&mut self) -> Result<(), ExtractorError>;

	/// Extract nodes and edges from a single file's source text.
	///
	/// # Arguments
	/// - `source`: UTF-8 source text (not a file path)
	/// - `file_path`: repo-relative path (forward slashes)
	/// - `file_uid`: e.g., `"my-repo:src/core/Foo.ts"`
	/// - `repo_uid`: repository identifier
	/// - `snapshot_uid`: snapshot being built
	///
	/// # Errors
	/// Returns `Err(ExtractorError)` only for true adapter/setup
	/// failures (null parser, wrong grammar, uninitialized state).
	/// Syntactically broken source still produces `Ok` with a
	/// partial extraction result — tree-sitter returns partial
	/// trees with ERROR nodes, and the extractor visits whatever
	/// it can find. The indexer records `ParseStatus::Failed` only
	/// when this method returns `Err`.
	fn extract(
		&self,
		source: &str,
		file_path: &str,
		file_uid: &str,
		repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<ExtractionResult, ExtractorError>;
}
