//! Stable enumeration of the data sources an agent signal can be
//! traced back to.
//!
//! Each aggregator tags the signals it produces with a `SourceRef`
//! so the output JSON is self-describing: an agent reading a
//! signal can see exactly which port method or derivation
//! produced it, without resorting to string-matching on free-form
//! text.
//!
//! `SourceRef` is NOT a generic free-form string. New sources
//! require a new enum variant, so adding an aggregator forces an
//! explicit, compile-time decision about what it calls itself.
//!
//! Serialization shape: every variant serializes to a stable
//! dotted identifier of the form `"<namespace>::<operation>"`.
//! The serialization is manual (not derived) so the on-the-wire
//! format is independent of Rust variant naming.

use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRef {
	/// Port method: `AgentStorageRead::find_module_cycles`.
	StorageFindModuleCycles,
	/// Port method: `AgentStorageRead::find_dead_nodes`.
	StorageFindDeadNodes,
	/// Port method: `AgentStorageRead::get_active_boundary_declarations`.
	StorageGetActiveBoundaryDeclarations,
	/// Port method: `AgentStorageRead::find_imports_between_paths`.
	StorageFindImportsBetweenPaths,
	/// Port method: `AgentStorageRead::get_latest_snapshot`.
	StorageGetLatestSnapshot,
	/// Port method: `AgentStorageRead::get_stale_files`.
	StorageGetStaleFiles,
	/// Port method: `AgentStorageRead::compute_repo_summary`.
	StorageComputeRepoSummary,
	/// Port method: `AgentStorageRead::get_trust_summary`.
	StorageGetTrustSummary,
	/// Gate crate: `repo_graph_gate::assemble_from_requirements`.
	/// Emitted by the agent gate aggregator for `GATE_PASS`,
	/// `GATE_FAIL`, and `GATE_INCOMPLETE` signals.
	GateAssemble,
	/// Port method: `AgentStorageRead::find_symbol_callers`.
	StorageFindSymbolCallers,
	/// Port method: `AgentStorageRead::find_symbol_callees`.
	StorageFindSymbolCallees,
	/// Check use case: two-phase reducer (`check::check`).
	/// Emitted by `CHECK_PASS`, `CHECK_FAIL`, `CHECK_INCOMPLETE`
	/// signals.
	CheckReducer,
	/// Explain use case: symbol/file/path explain pipeline.
	ExplainPipeline,
}

impl SourceRef {
	/// Wire-format identifier, stable across releases.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::StorageFindModuleCycles => "storage::find_module_cycles",
			Self::StorageFindDeadNodes => "storage::find_dead_nodes",
			Self::StorageGetActiveBoundaryDeclarations => {
				"storage::get_active_boundary_declarations"
			}
			Self::StorageFindImportsBetweenPaths => {
				"storage::find_imports_between_paths"
			}
			Self::StorageGetLatestSnapshot => "storage::get_latest_snapshot",
			Self::StorageGetStaleFiles => "storage::get_stale_files",
			Self::StorageComputeRepoSummary => "storage::compute_repo_summary",
			Self::StorageGetTrustSummary => "storage::get_trust_summary",
			Self::GateAssemble => "gate::assemble",
			Self::StorageFindSymbolCallers => "storage::find_symbol_callers",
			Self::StorageFindSymbolCallees => "storage::find_symbol_callees",
			Self::CheckReducer => "check::reducer",
			Self::ExplainPipeline => "explain::pipeline",
		}
	}
}

impl Serialize for SourceRef {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn serializes_as_dotted_identifier() {
		let s = serde_json::to_string(&SourceRef::StorageFindModuleCycles).unwrap();
		assert_eq!(s, "\"storage::find_module_cycles\"");
	}

	#[test]
	fn all_variants_have_stable_strings() {
		// Round-trip via as_str to confirm no variant panics.
		for s in [
			SourceRef::StorageFindModuleCycles,
			SourceRef::StorageFindDeadNodes,
			SourceRef::StorageGetActiveBoundaryDeclarations,
			SourceRef::StorageFindImportsBetweenPaths,
			SourceRef::StorageGetLatestSnapshot,
			SourceRef::StorageGetStaleFiles,
			SourceRef::StorageComputeRepoSummary,
			SourceRef::StorageGetTrustSummary,
			SourceRef::StorageFindSymbolCallers,
			SourceRef::StorageFindSymbolCallees,
			SourceRef::CheckReducer,
			SourceRef::ExplainPipeline,
		] {
			assert!(s.as_str().contains("::"));
		}
	}
}
