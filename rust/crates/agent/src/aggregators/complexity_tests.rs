//! Tests for the complexity aggregator.
//!
//! Split out of `complexity.rs` via `#[path]` (the `orient_tests.rs` idiom) to
//! keep the source module under the 500-line structural guardrail — the test
//! module is dominated by a ~290-line `FakeStorage` trait-stub. Pure relocation.

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

    fn has_complexity_measurements(&self, _snapshot_uid: &str) -> Result<bool, AgentStorageError> {
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
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
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
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
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
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
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
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
    assert!(result.signals.is_empty());

    // Lower threshold (10) - should emit
    let result = aggregate_with_threshold(&storage, "snap1", 10, Budget::Small).unwrap();
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
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
    assert_eq!(result.signals.len(), 1);

    // Verify summary mentions count
    let summary = result.signals[0].summary();
    assert!(summary.contains("2 symbols"));
}

#[test]
fn budget_trades_complexity_evidence_depth_small_subset_of_full() {
    // ORIENT-DENSITY-1 review-1 #2: the EVIDENCE depth scales with budget —
    // `small` is a lean set, `--full` is every center — and small ⊂ full,
    // while the true total stays honest (the count, not the cap) at both.
    let measurements: Vec<AgentComplexityMeasurement> = (0..8)
        .map(|i| AgentComplexityMeasurement {
            stable_key: format!("k{i}"),
            symbol_name: format!("fn{i}"),
            file_path: Some(format!("src/f{i}.rs")),
            complexity: 100 - i as u64, // descending → deterministic order
        })
        .collect();
    let storage = FakeStorage::with_measurements(measurements);
    let ev = |b| {
        serde_json::to_value(&aggregate(&storage, "snap1", b).unwrap().signals[0]).unwrap()
            ["evidence"]
            .clone()
    };
    let syms = |e: &serde_json::Value| -> Vec<String> {
        e["top_complex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["symbol"].as_str().unwrap().to_string())
            .collect()
    };
    let (small, full) = (ev(Budget::Small), ev(Budget::Full));

    assert_eq!(syms(&small).len(), 5, "small caps the evidence depth");
    assert_eq!(syms(&full).len(), 8, "--full carries every center");
    assert_eq!(syms(&small), syms(&full)[..5], "small ⊂ full (prefix)");
    assert_eq!(
        small["high_complexity_count"], 8,
        "true total honest at small"
    );
}
