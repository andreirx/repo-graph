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

use repo_graph_indexer::hook::{
	ExtractionExtras, ExtractionHookDiagnostic, ExtractionResultHook,
};
use repo_graph_indexer::types::ExtractionResult;
use repo_graph_state_bindings::{BindingTable, Language, RepoUid};
use repo_graph_state_extractor::languages::typescript::emit_from_resolved_callsites;
use repo_graph_state_extractor::{EmitterContext, StateBoundaryEmitter};

/// State-boundary extraction-result hook.
///
/// Constructed by `compose.rs` before each indexing run. Holds
/// the binding table reference, the emitter (lazily initialized
/// on first `on_extraction_result` call), and accumulated
/// diagnostics.
///
/// Lifecycle:
/// 1. `compose.rs` validates `repo_uid` via `RepoUid::new`. If
///    invalid, a diagnostic is recorded and the hook returns
///    empty extras on every call (no emission, no abort).
/// 2. Per file, `on_extraction_result` feeds
///    `result.resolved_callsites` into the TS adapter +
///    emitter.
/// 3. At snapshot close, `drain_snapshot_extras` consumes the
///    emitter and returns nodes + edges + diagnostics.
pub struct StateBoundaryHook {
	/// Validated repo_uid for emitter construction.
	/// `None` if `RepoUid::new` failed at hook-construction time
	/// (the hook degrades gracefully: no emission, diagnostic
	/// recorded).
	repo_uid: Option<RepoUid>,
	/// Reference to the embedded binding table.
	table: &'static BindingTable,
	/// Lazy emitter: initialized on the first
	/// `on_extraction_result` call once `snapshot_uid` is known.
	emitter: Option<StateBoundaryEmitter<'static>>,
	/// Accumulated diagnostics.
	diagnostics: Vec<ExtractionHookDiagnostic>,
}

/// Extractor name stamped on every emitted state-boundary edge.
const STATE_EXTRACTOR_NAME: &str = "state-extractor:0.1.0";

impl StateBoundaryHook {
	/// Construct a new hook. If `repo_uid` fails `RepoUid::new`
	/// validation, a diagnostic is recorded and the hook will
	/// produce no state-boundary output.
	pub fn new(repo_uid: &str) -> Self {
		let table = BindingTable::load_embedded();
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
			emitter: None,
			diagnostics,
		}
	}

	/// Ensure the emitter is initialized for the given snapshot.
	/// Returns `None` if repo_uid validation failed at
	/// construction.
	fn ensure_emitter(&mut self, snapshot_uid: &str) -> Option<&mut StateBoundaryEmitter<'static>> {
		let repo_uid = self.repo_uid.as_ref()?;
		if self.emitter.is_none() {
			self.emitter = Some(StateBoundaryEmitter::new(
				self.table,
				EmitterContext {
					repo_uid: repo_uid.clone(),
					snapshot_uid: snapshot_uid.to_string(),
					language: Language::Typescript,
					extractor_name: STATE_EXTRACTOR_NAME.to_string(),
				},
			));
		}
		self.emitter.as_mut()
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
		let Some(emitter) = self.ensure_emitter(snapshot_uid) else {
			// repo_uid invalid → no emission, diagnostic already
			// recorded at construction.
			return;
		};
		if let Err(e) = emit_from_resolved_callsites(
			&result.resolved_callsites,
			emitter,
		) {
			self.diagnostics.push(ExtractionHookDiagnostic {
				code: "state_boundary_emit_error".into(),
				message: format!("state-boundary emit failed: {}", e),
				file_uid: Some(file_uid.to_string()),
				file_path: Some(file_path.to_string()),
			});
		}
	}

	fn drain_snapshot_extras(&mut self) -> ExtractionExtras {
		let Some(emitter) = self.emitter.take() else {
			// No emitter was ever initialized (no resolved
			// callsites seen, or repo_uid invalid).
			return ExtractionExtras {
				nodes: vec![],
				edges: vec![],
				diagnostics: std::mem::take(&mut self.diagnostics),
			};
		};
		let facts = emitter.drain();
		ExtractionExtras {
			nodes: facts.nodes,
			// `EmittedFacts.edges` and `ExtractionExtras.edges`
			// are both `Vec<ExtractedEdge>` from the indexer types
			// crate. Direct pass-through, no conversion.
			edges: facts.edges,
			diagnostics: std::mem::take(&mut self.diagnostics),
		}
	}
}
