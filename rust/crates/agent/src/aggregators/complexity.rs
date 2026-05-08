//! Complexity aggregator.
//!
//! Queries cyclomatic complexity measurements and emits
//! `HIGH_COMPLEXITY` when symbols exceed the threshold.
//! Evidence includes the count and top N complex symbols.

use super::AggregatorOutput;
use crate::dto::signal::{ComplexSymbolEvidence, HighComplexityEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::AgentStorageRead;

/// Default complexity threshold for HIGH_COMPLEXITY signal.
/// Symbols with cyclomatic complexity >= this value are flagged.
pub const DEFAULT_COMPLEXITY_THRESHOLD: u64 = 20;

/// Maximum number of complex symbols to include in evidence.
const COMPLEXITY_TOP_N: usize = 5;

/// Aggregate complexity data and emit HIGH_COMPLEXITY if warranted.
///
/// Returns a signal when at least one symbol exceeds the threshold.
/// Returns empty output when no measurements exist or none exceed threshold.
pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	aggregate_with_threshold(storage, snapshot_uid, DEFAULT_COMPLEXITY_THRESHOLD)
}

/// Aggregate with a custom threshold (for testing or configuration).
pub fn aggregate_with_threshold<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	threshold: u64,
) -> Result<AggregatorOutput, AgentStorageError> {
	// Get the true count of symbols exceeding threshold (not limited)
	let count = storage.count_high_complexity_symbols(snapshot_uid, threshold)?;

	if count == 0 {
		return Ok(AggregatorOutput::empty());
	}

	// Get top N for evidence (sample, not full list)
	let high_complexity = storage.query_high_complexity_symbols(
		snapshot_uid,
		threshold,
		COMPLEXITY_TOP_N,
	)?;

	let top: Vec<ComplexSymbolEvidence> = high_complexity
		.into_iter()
		.map(|m| ComplexSymbolEvidence {
			symbol: m.symbol_name,
			file: m.file_path,
			complexity: m.complexity,
		})
		.collect();

	let evidence = HighComplexityEvidence {
		high_complexity_count: count,
		threshold,
		top_complex: top,
	};

	Ok(AggregatorOutput {
		signals: vec![Signal::high_complexity(evidence)],
		limits: Vec::new(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::storage_port::AgentComplexityMeasurement;

	struct FakeStorage {
		measurements: Vec<AgentComplexityMeasurement>,
	}

	impl FakeStorage {
		fn empty() -> Self {
			Self { measurements: Vec::new() }
		}

		fn with_measurements(measurements: Vec<AgentComplexityMeasurement>) -> Self {
			Self { measurements }
		}
	}

	// Minimal AgentStorageRead implementation for testing
	impl AgentStorageRead for FakeStorage {
		fn query_high_complexity_symbols(
			&self,
			_snapshot_uid: &str,
			min_threshold: u64,
			limit: usize,
		) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
			let filtered: Vec<_> = self
				.measurements
				.iter()
				.filter(|m| m.complexity >= min_threshold)
				.take(limit)
				.cloned()
				.collect();
			Ok(filtered)
		}

		fn has_complexity_measurements(
			&self,
			_snapshot_uid: &str,
		) -> Result<bool, AgentStorageError> {
			Ok(!self.measurements.is_empty())
		}

		fn count_high_complexity_symbols(
			&self,
			_snapshot_uid: &str,
			min_threshold: u64,
		) -> Result<u64, AgentStorageError> {
			let count = self
				.measurements
				.iter()
				.filter(|m| m.complexity >= min_threshold)
				.count();
			Ok(count as u64)
		}

		// Stub implementations for other required methods
		fn get_repo(
			&self,
			_repo_uid: &str,
		) -> Result<Option<crate::AgentRepo>, AgentStorageError> {
			Ok(None)
		}

		fn get_latest_snapshot(
			&self,
			_repo_uid: &str,
		) -> Result<Option<crate::AgentSnapshot>, AgentStorageError> {
			Ok(None)
		}

		fn get_stale_files(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<crate::AgentStaleFile>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_module_cycles(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<crate::AgentCycle>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_dead_nodes(
			&self,
			_snapshot_uid: &str,
			_repo_uid: &str,
			_kind_filter: Option<&str>,
		) -> Result<Vec<crate::AgentDeadNode>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn get_active_boundary_declarations(
			&self,
			_repo_uid: &str,
		) -> Result<Vec<crate::AgentBoundaryDeclaration>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_imports_between_paths(
			&self,
			_snapshot_uid: &str,
			_source_prefix: &str,
			_target_prefix: &str,
		) -> Result<Vec<crate::AgentImportEdge>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn compute_repo_summary(
			&self,
			_snapshot_uid: &str,
		) -> Result<crate::AgentRepoSummary, AgentStorageError> {
			Ok(crate::AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			})
		}

		fn get_trust_summary(
			&self,
			_repo_uid: &str,
			_snapshot_uid: &str,
		) -> Result<crate::AgentTrustSummary, AgentStorageError> {
			unimplemented!("not needed for complexity tests")
		}

		fn resolve_path_focus(
			&self,
			_snapshot_uid: &str,
			_path: &str,
		) -> Result<crate::AgentPathResolution, AgentStorageError> {
			unimplemented!("not needed for complexity tests")
		}

		fn resolve_stable_key_focus(
			&self,
			_snapshot_uid: &str,
			_stable_key: &str,
		) -> Result<Option<crate::AgentFocusCandidate>, AgentStorageError> {
			Ok(None)
		}

		fn find_dead_nodes_in_path(
			&self,
			_snapshot_uid: &str,
			_repo_uid: &str,
			_path_prefix: &str,
		) -> Result<Vec<crate::AgentDeadNode>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_dead_nodes_in_file(
			&self,
			_snapshot_uid: &str,
			_repo_uid: &str,
			_file_path: &str,
		) -> Result<Vec<crate::AgentDeadNode>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn compute_path_summary(
			&self,
			_snapshot_uid: &str,
			_path_prefix: &str,
		) -> Result<crate::AgentRepoSummary, AgentStorageError> {
			Ok(crate::AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			})
		}

		fn compute_file_summary(
			&self,
			_snapshot_uid: &str,
			_file_path: &str,
		) -> Result<crate::AgentRepoSummary, AgentStorageError> {
			Ok(crate::AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			})
		}

		fn find_boundary_declarations_in_path(
			&self,
			_repo_uid: &str,
			_path_prefix: &str,
		) -> Result<Vec<crate::AgentBoundaryDeclaration>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_cycles_involving_path(
			&self,
			_snapshot_uid: &str,
			_path_prefix: &str,
		) -> Result<Vec<crate::AgentCycle>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn resolve_symbol_name(
			&self,
			_snapshot_uid: &str,
			_name: &str,
		) -> Result<Vec<crate::AgentFocusCandidate>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn get_symbol_context(
			&self,
			_snapshot_uid: &str,
			_symbol_stable_key: &str,
		) -> Result<Option<crate::AgentSymbolContext>, AgentStorageError> {
			Ok(None)
		}

		fn find_symbol_callers(
			&self,
			_snapshot_uid: &str,
			_symbol_stable_key: &str,
		) -> Result<Vec<crate::AgentCallerRow>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_symbol_callees(
			&self,
			_snapshot_uid: &str,
			_symbol_stable_key: &str,
		) -> Result<Vec<crate::AgentCalleeRow>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_cycles_involving_module(
			&self,
			_snapshot_uid: &str,
			_module_qualified_name: &str,
		) -> Result<Vec<crate::AgentCycle>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn list_symbols_in_file(
			&self,
			_snapshot_uid: &str,
			_file_path: &str,
		) -> Result<Vec<crate::AgentSymbolEntry>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn list_files_in_path(
			&self,
			_snapshot_uid: &str,
			_path_prefix: &str,
		) -> Result<Vec<crate::AgentFileEntry>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn find_file_imports(
			&self,
			_snapshot_uid: &str,
			_file_path: &str,
		) -> Result<Vec<crate::AgentImportEntry>, AgentStorageError> {
			Ok(Vec::new())
		}

		fn get_doc_inventory(
			&self,
			_repo_uid: &str,
		) -> Result<Vec<crate::AgentDocEntry>, AgentStorageError> {
			Ok(Vec::new())
		}
	}

	#[test]
	fn empty_when_no_measurements() {
		let storage = FakeStorage::empty();
		let result = aggregate(&storage, "snap1").unwrap();
		assert!(result.signals.is_empty());
		assert!(result.limits.is_empty());
	}

	#[test]
	fn empty_when_below_threshold() {
		let storage = FakeStorage::with_measurements(vec![
			AgentComplexityMeasurement {
				stable_key: "k1".into(),
				symbol_name: "foo".into(),
				file_path: Some("foo.rs".into()),
				complexity: 10, // Below default threshold of 20
			},
		]);
		let result = aggregate(&storage, "snap1").unwrap();
		assert!(result.signals.is_empty());
	}

	#[test]
	fn emits_signal_when_above_threshold() {
		let storage = FakeStorage::with_measurements(vec![
			AgentComplexityMeasurement {
				stable_key: "k1".into(),
				symbol_name: "complex_func".into(),
				file_path: Some("src/complex.rs".into()),
				complexity: 25,
			},
		]);
		let result = aggregate(&storage, "snap1").unwrap();
		assert_eq!(result.signals.len(), 1);
		assert_eq!(result.signals[0].code().as_str(), "HIGH_COMPLEXITY");
	}

	#[test]
	fn custom_threshold_works() {
		let storage = FakeStorage::with_measurements(vec![
			AgentComplexityMeasurement {
				stable_key: "k1".into(),
				symbol_name: "moderate".into(),
				file_path: Some("mod.rs".into()),
				complexity: 15,
			},
		]);
		// Default threshold (20) - should not emit
		let result = aggregate(&storage, "snap1").unwrap();
		assert!(result.signals.is_empty());

		// Lower threshold (10) - should emit
		let result = aggregate_with_threshold(&storage, "snap1", 10).unwrap();
		assert_eq!(result.signals.len(), 1);
	}

	#[test]
	fn evidence_contains_top_complex_symbols() {
		let storage = FakeStorage::with_measurements(vec![
			AgentComplexityMeasurement {
				stable_key: "k1".into(),
				symbol_name: "very_complex".into(),
				file_path: Some("a.rs".into()),
				complexity: 50,
			},
			AgentComplexityMeasurement {
				stable_key: "k2".into(),
				symbol_name: "also_complex".into(),
				file_path: Some("b.rs".into()),
				complexity: 30,
			},
		]);
		let result = aggregate(&storage, "snap1").unwrap();
		assert_eq!(result.signals.len(), 1);

		// Verify summary mentions count
		let summary = result.signals[0].summary();
		assert!(summary.contains("2 symbols"));
	}
}
