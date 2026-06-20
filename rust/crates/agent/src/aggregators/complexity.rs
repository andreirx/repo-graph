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

/// "Fetch every above-threshold symbol" sentinel for the storage `limit` parameter.
///
/// TRUNCATION-AUDIT-1: `i64::MAX` is a valid SQLite `LIMIT` (no snapshot has 9.2e18 symbols), so
/// the adapter returns the FULL above-threshold set; we then sort + cut to `COMPLEXITY_TOP_N` in
/// the agent for a deterministic, source-independent top-N (see `aggregate_with_threshold`).
/// `usize::MAX` is NOT usable as the sentinel: rusqlite binds the limit as `i64` and errors on the
/// `u64::MAX` overflow, whereas `i64::MAX as usize` round-trips cleanly.
const FETCH_ALL: usize = i64::MAX as usize;

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

    // TRUNCATION-AUDIT-1: fetch the FULL above-threshold set, then apply a TOTAL deterministic
    // sort (complexity DESC, then the unique stable_key) and cut to COMPLEXITY_TOP_N HERE in the
    // agent. We deliberately do NOT accept storage's `ORDER BY complexity DESC LIMIT N` cut: its
    // ties at the cut boundary fall to SQLite rowid order, so the surviving sample would depend on
    // storage row order rather than on the SET (the DR-EXPLAIN-CALLER-ORDER hazard). Owning the cut
    // here makes the top-N sample a pure function of the above-threshold set — identical regardless
    // of which store answered. Cost (honest): the table SCAN is already paid — `count_*` above passes
    // over the same `cyclomatic_complexity` measurements. The added work is MATERIALISING the rows
    // (`query_*` joins nodes+files and JSON-parses each, and filters the threshold in Rust, so it
    // materialises every complexity row, not just the above-threshold minority) to pick the top-5.
    // This is bounded and paid once per orient (not a hot loop); the cost-OPTIMAL fix — a total
    // `ORDER BY complexity DESC, target_stable_key LIMIT N` in the storage SQL so the LIMIT cut is
    // itself deterministic — lives in the storage adapter, outside this slice's file scope, and is
    // recorded as the follow-up. Determinism here is required NOW and is achievable agent-side.
    let mut high_complexity =
        storage.query_high_complexity_symbols(snapshot_uid, threshold, FETCH_ALL)?;
    crate::ordering::sort_complexity(&mut high_complexity);
    high_complexity.truncate(COMPLEXITY_TOP_N);

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
            Self {
                measurements: Vec::new(),
            }
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
        fn get_repo(&self, _repo_uid: &str) -> Result<Option<crate::AgentRepo>, AgentStorageError> {
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

        fn get_module_summary(
            &self,
            _snapshot_uid: &str,
        ) -> Result<Option<crate::AgentModuleSummary>, AgentStorageError> {
            Ok(None)
        }

        fn get_boundary_links_freshness(
            &self,
            _snapshot_uid: &str,
        ) -> Result<crate::AgentBoundaryLinksFreshness, AgentStorageError> {
            Ok(crate::AgentBoundaryLinksFreshness {
                total: 0,
                current: 0,
                impacted: 0,
                unknown: 0,
                earliest_impacted_at: None,
            })
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
        let storage = FakeStorage::with_measurements(vec![AgentComplexityMeasurement {
            stable_key: "k1".into(),
            symbol_name: "foo".into(),
            file_path: Some("foo.rs".into()),
            complexity: 10, // Below default threshold of 20
        }]);
        let result = aggregate(&storage, "snap1").unwrap();
        assert!(result.signals.is_empty());
    }

    #[test]
    fn emits_signal_when_above_threshold() {
        let storage = FakeStorage::with_measurements(vec![AgentComplexityMeasurement {
            stable_key: "k1".into(),
            symbol_name: "complex_func".into(),
            file_path: Some("src/complex.rs".into()),
            complexity: 25,
        }]);
        let result = aggregate(&storage, "snap1").unwrap();
        assert_eq!(result.signals.len(), 1);
        assert_eq!(result.signals[0].code().as_str(), "HIGH_COMPLEXITY");
    }

    #[test]
    fn custom_threshold_works() {
        let storage = FakeStorage::with_measurements(vec![AgentComplexityMeasurement {
            stable_key: "k1".into(),
            symbol_name: "moderate".into(),
            file_path: Some("mod.rs".into()),
            complexity: 15,
        }]);
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
