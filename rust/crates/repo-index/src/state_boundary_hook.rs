//! Concrete `ExtractionResultHook` implementation for
//! state-boundary emission.
//!
//! This module lives in `repo-index` (the composition root), not
//! in `repo-graph-indexer` (the orchestration policy crate). The
//! indexer depends only on the `ExtractionResultHook` trait and
//! the shared DTOs. The concrete wiring to `state-extractor` and
//! `state-bindings` is composed here.
//!
//! SB-4-pre locks:
//! - 4-pre.1 = A: compose owns concrete wiring.
//! - 4-pre.2 = Shape B: hook-owned buffer, drain at snapshot
//!   close, structured diagnostics.
//! - 4-pre.4 = C: diagnostic + continue.
//! - 4-pre.5 = C2: compose-layer diagnostic on invalid repo_uid,
//!   continue without state-boundary emission.
//!
//! SB-7A: Uses `AdapterRegistry` for language dispatch. The hook
//! gets the adapter from the registry, calls `adapt_callsites()`
//! to get DTOs, and feeds them to the language-specific emitter.
//!
//! SB-7A multi-language fix: One emitter per language per snapshot.
//! Each language's files are matched against their own language-
//! specific bindings. This ensures `match_form_a(..., language)`
//! receives the correct language for binding-table dispatch.
//!
//! Diagnostic policy (hybrid):
//! - Unsupported language (no adapter expected yet) → silent skip
//! - Expected adapter missing from registry → diagnostic emitted

use std::collections::HashMap;

use repo_graph_indexer::hook::{
	ExtractionExtras, ExtractionHookDiagnostic, ExtractionResultHook,
};
use repo_graph_indexer::routing;
use repo_graph_indexer::types::ExtractionResult;
use repo_graph_state_bindings::{BindingTable, Language, RepoUid};
use repo_graph_state_extractor::{
	default_registry, AdapterContext, AdapterRegistry, EmitterContext, StateBoundaryEmitter,
};

/// State-boundary extraction-result hook.
///
/// Constructed by `compose.rs` before each indexing run. Holds
/// the binding table reference, adapter registry, per-language
/// emitters (lazily initialized), and accumulated diagnostics.
///
/// Lifecycle:
/// 1. `compose.rs` validates `repo_uid` via `RepoUid::new`. If
///    invalid, a diagnostic is recorded and the hook returns
///    empty extras on every call (no emission, no abort).
/// 2. Per file, `on_extraction_result`:
///    - classifies file language
///    - gets adapter from registry (SB-7A)
///    - calls `adapter.adapt_callsites()` to get DTOs
///    - feeds DTOs to the language-specific emitter
/// 3. At snapshot close, `drain_snapshot_extras` aggregates all
///    emitters and returns nodes + edges + diagnostics.
///
/// SB-7A multi-language: One emitter per language ensures
/// `match_form_a(..., language)` receives the correct language
/// for binding-table dispatch.
pub struct StateBoundaryHook {
	/// Validated repo_uid for emitter construction.
	/// `None` if `RepoUid::new` failed at hook-construction time
	/// (the hook degrades gracefully: no emission, diagnostic
	/// recorded).
	repo_uid: Option<RepoUid>,
	/// Reference to the embedded binding table.
	table: &'static BindingTable,
	/// Language adapter registry (SB-7A).
	registry: AdapterRegistry,
	/// Per-language emitters. Lazily initialized on first callsite
	/// for each language. Each emitter receives only callsites for
	/// its language, ensuring correct binding-table dispatch.
	emitters: HashMap<Language, StateBoundaryEmitter<'static>>,
	/// Snapshot UID, captured on first `on_extraction_result` call.
	/// Needed for lazy emitter construction.
	snapshot_uid: Option<String>,
	/// Accumulated diagnostics.
	diagnostics: Vec<ExtractionHookDiagnostic>,
}

/// Extractor name stamped on every emitted state-boundary edge.
const STATE_EXTRACTOR_NAME: &str = "state-extractor:0.1.0";

/// Language classification result for diagnostic policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageClassification {
	/// Language is supported and expected to have an adapter.
	/// Missing adapter = configuration fault → diagnostic.
	Supported(Language),
	/// Language is recognized but not yet supported by state-boundary
	/// substrate (SB-7B/SB-7C pending). Missing adapter = expected →
	/// silent skip.
	Unsupported(Language),
	/// Language is unknown to the routing layer. Silent skip.
	Unknown,
}

/// Convert routing language string to classification.
///
/// Supported languages (adapters expected):
/// - TypeScript/JavaScript (SB-7A)
/// - Python (SB-7C)
///
/// Unsupported languages (adapters not yet implemented):
/// - Java, Rust, C++ (SB-7B pending)
///
/// Unknown languages:
/// - Everything else
fn classify_language(lang_str: Option<&str>) -> LanguageClassification {
	match lang_str {
		// SB-7A: TypeScript adapter is shipped. Missing = fault.
		Some("typescript" | "tsx" | "javascript" | "jsx") => {
			LanguageClassification::Supported(Language::Typescript)
		}
		// SB-7C: Python adapter is shipped. Missing = fault.
		Some("python") => LanguageClassification::Supported(Language::Python),
		// SB-7B pending: adapters not yet implemented.
		Some("java") => LanguageClassification::Unsupported(Language::Java),
		Some("rust") => LanguageClassification::Unsupported(Language::Rust),
		Some("cpp" | "c") => LanguageClassification::Unsupported(Language::Cpp),
		// Unknown language.
		_ => LanguageClassification::Unknown,
	}
}

impl StateBoundaryHook {
	/// Construct a new hook. If `repo_uid` fails `RepoUid::new`
	/// validation, a diagnostic is recorded and the hook will
	/// produce no state-boundary output.
	pub fn new(repo_uid: &str) -> Self {
		let table = BindingTable::load_embedded();
		let registry = default_registry();
		let (validated, diagnostics) = match RepoUid::new(repo_uid) {
			Ok(uid) => (Some(uid), vec![]),
			Err(e) => (
				None,
				vec![ExtractionHookDiagnostic {
					code: "state_boundary_invalid_repo_uid".into(),
					message: format!(
						"repo_uid {:?} failed validation: {}. \
						 State-boundary emission disabled for this run.",
						repo_uid, e
					),
					file_uid: None,
					file_path: None,
				}],
			),
		};
		Self {
			repo_uid: validated,
			table,
			registry,
			emitters: HashMap::new(),
			snapshot_uid: None,
			diagnostics,
		}
	}

	/// Get or create the emitter for a specific language.
	///
	/// Returns `None` if repo_uid validation failed at construction.
	/// Each language gets its own emitter to ensure correct binding-
	/// table dispatch via `match_form_a(..., language)`.
	fn get_or_create_emitter(
		&mut self,
		snapshot_uid: &str,
		language: Language,
	) -> Option<&mut StateBoundaryEmitter<'static>> {
		let repo_uid = self.repo_uid.as_ref()?;

		// Capture snapshot_uid on first call.
		if self.snapshot_uid.is_none() {
			self.snapshot_uid = Some(snapshot_uid.to_string());
		}

		// Get or insert emitter for this language.
		if !self.emitters.contains_key(&language) {
			let emitter = StateBoundaryEmitter::new(
				self.table,
				EmitterContext {
					repo_uid: repo_uid.clone(),
					snapshot_uid: snapshot_uid.to_string(),
					language,
					extractor_name: STATE_EXTRACTOR_NAME.to_string(),
				},
			);
			self.emitters.insert(language, emitter);
		}
		self.emitters.get_mut(&language)
	}
}

impl ExtractionResultHook for StateBoundaryHook {
	fn on_extraction_result(
		&mut self,
		_repo_uid: &str,
		snapshot_uid: &str,
		file_uid: &str,
		file_path: &str,
		result: &ExtractionResult,
	) {
		if result.resolved_callsites.is_empty() {
			return;
		}

		// SB-7A: Classify language for hybrid diagnostic policy.
		let lang_str = routing::detect_language(file_path);
		let language = match classify_language(lang_str) {
			LanguageClassification::Supported(lang) => lang,
			LanguageClassification::Unsupported(_) => {
				// SB-7B/SB-7C pending. Silent skip — not a fault.
				return;
			}
			LanguageClassification::Unknown => {
				// Unknown language. Silent skip.
				return;
			}
		};

		// SB-7A: Adapter returns DTOs. Call adapt_callsites in a
		// separate scope so the immutable borrow of self.registry
		// is dropped before we mutably borrow self for emitter access.
		let callsites = {
			let Some(adapter) = self.registry.get(language) else {
				// Hybrid diagnostic policy: expected adapter missing
				// from registry = configuration fault. Emit diagnostic.
				self.diagnostics.push(ExtractionHookDiagnostic {
					code: "state_boundary_missing_adapter".into(),
					message: format!(
						"State-boundary adapter for {:?} is expected but not \
						 registered. This is a substrate configuration fault.",
						language
					),
					file_uid: Some(file_uid.to_string()),
					file_path: Some(file_path.to_string()),
				});
				return;
			};
			let ctx = AdapterContext {
				file_uid,
				file_path,
			};
			adapter.adapt_callsites(&ctx, &result.resolved_callsites)
		};

		if callsites.is_empty() {
			return;
		}

		// SB-7A multi-language: Get the emitter for this specific
		// language. Each language has its own emitter to ensure
		// correct binding-table dispatch.
		let Some(emitter) = self.get_or_create_emitter(snapshot_uid, language) else {
			// repo_uid invalid → no emission, diagnostic already
			// recorded at construction.
			return;
		};

		// Feed callsites to emitter. Collect errors locally to avoid
		// borrowing self.diagnostics while emitter borrows self.emitters.
		let mut errors: Vec<ExtractionHookDiagnostic> = Vec::new();
		for site in &callsites {
			if let Err(e) = emitter.emit_for_callsite(site) {
				errors.push(ExtractionHookDiagnostic {
					code: "state_boundary_emit_error".into(),
					message: format!("state-boundary emit failed: {}", e),
					file_uid: Some(file_uid.to_string()),
					file_path: Some(file_path.to_string()),
				});
			}
		}
		// Now emitter borrow is released; append collected errors.
		self.diagnostics.extend(errors);
	}

	fn drain_snapshot_extras(&mut self) -> ExtractionExtras {
		// SB-7A multi-language: Aggregate all per-language emitters.
		let emitters = std::mem::take(&mut self.emitters);
		if emitters.is_empty() {
			// No emitters were ever initialized (no resolved
			// callsites seen, or repo_uid invalid).
			return ExtractionExtras {
				nodes: vec![],
				edges: vec![],
				diagnostics: std::mem::take(&mut self.diagnostics),
			};
		}

		let mut all_nodes = Vec::new();
		let mut all_edges = Vec::new();

		// Drain each language's emitter and aggregate results.
		// Order is deterministic per Language enum order (HashMap
		// iteration order is not stable, but the final merged
		// output is order-independent for correctness).
		for (_language, emitter) in emitters {
			let facts = emitter.drain();
			all_nodes.extend(facts.nodes);
			all_edges.extend(facts.edges);
		}

		ExtractionExtras {
			nodes: all_nodes,
			edges: all_edges,
			diagnostics: std::mem::take(&mut self.diagnostics),
		}
	}
}
