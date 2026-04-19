//! Extraction-result hook port.
//!
//! The indexer orchestrator calls this hook after each file's
//! extraction completes. A composition root provides the concrete
//! implementation; the indexer depends only on this trait and the
//! shared DTOs (`ExtractedNode`, `ExtractedEdge`).
//!
//! SB-4-pre introduced this hook to keep state-boundary wiring
//! in the composition root (`repo-index/compose.rs`) rather than
//! coupling the indexer to `repo-graph-state-extractor`.
//!
//! Lifecycle:
//!
//! 1. The caller (composition root) constructs the hook before
//!    calling `index_repo` / `refresh_repo`.
//! 2. The orchestrator calls `on_extraction_result` per file
//!    inside the extraction loop.
//! 3. After the extraction loop completes, the orchestrator
//!    calls `drain_snapshot_extras` ONCE and merges the returned
//!    nodes + edges into its persistence batch.
//!
//! Diagnostics: the hook accumulates structured diagnostics
//! internally and returns them on drain. The orchestrator renders
//! them to stderr in slice 1; future slices may persist them
//! through the extraction-diagnostics substrate.

use crate::types::{ExtractedEdge, ExtractedNode, ExtractionResult};

/// Structured diagnostic from an extraction hook.
///
/// Machine-readable, testable, future-persistable. Stderr
/// rendering is one output of this data, not the data model
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionHookDiagnostic {
	/// Short machine-readable code. Examples:
	/// `"state_boundary_invalid_repo_uid"`,
	/// `"state_boundary_emit_error"`,
	/// `"state_boundary_payload_skip"`.
	pub code: String,
	/// Human-readable diagnostic message.
	pub message: String,
	/// File UID associated with the diagnostic, if applicable.
	pub file_uid: Option<String>,
	/// File path associated with the diagnostic, if applicable.
	pub file_path: Option<String>,
}

/// Extra nodes + edges + diagnostics returned by a hook at
/// snapshot close.
#[derive(Debug, Default)]
pub struct ExtractionExtras {
	/// Additional nodes to merge into the persistence batch
	/// (e.g. resource nodes from state-boundary emission).
	pub nodes: Vec<ExtractedNode>,
	/// Additional edges to merge into the persistence batch
	/// (e.g. READS / WRITES edges from state-boundary emission).
	pub edges: Vec<ExtractedEdge>,
	/// Diagnostics accumulated during hook processing.
	pub diagnostics: Vec<ExtractionHookDiagnostic>,
}

/// Hook invoked by the orchestrator per extraction result.
///
/// The trait is object-safe (`&mut dyn ExtractionResultHook`) so
/// the orchestrator can accept it without being generic over the
/// hook implementation.
pub trait ExtractionResultHook {
	/// Called once per file after extraction completes.
	///
	/// The hook observes the `ExtractionResult` and may
	/// accumulate internal state (e.g. resource-node dedup,
	/// edge buffering). It does NOT write to the orchestrator's
	/// accumulators directly; instead, it returns accumulated
	/// facts via `drain_snapshot_extras`.
	///
	/// `file_uid` and `file_path` identify the source file. The
	/// hook carries them into diagnostics so emit failures can
	/// point at the offending file.
	fn on_extraction_result(
		&mut self,
		repo_uid: &str,
		snapshot_uid: &str,
		file_uid: &str,
		file_path: &str,
		result: &ExtractionResult,
	);

	/// Called once at snapshot close (after the extraction loop).
	///
	/// Returns all accumulated nodes, edges, and diagnostics.
	/// The orchestrator merges the returned nodes + edges into
	/// its phase-1 persistence batch and renders diagnostics to
	/// stderr.
	///
	/// After this call the hook's internal state is consumed;
	/// calling `on_extraction_result` again is only valid if the
	/// hook re-initializes internally for a new snapshot.
	fn drain_snapshot_extras(&mut self) -> ExtractionExtras;
}
