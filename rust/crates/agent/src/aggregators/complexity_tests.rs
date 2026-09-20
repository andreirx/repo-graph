//! Tests for the complexity aggregator.
//!
//! Split out of `complexity.rs` via `#[path]` (the `orient_tests.rs` idiom) to
//! keep the source module under the 500-line structural guardrail — the test
//! module is dominated by a ~290-line `FakeStorage` trait-stub. Pure relocation.

use super::*;
use crate::dto::signal::{ComplexityScope, SignalEvidence};
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

/// A production-code measurement (not test / not generated) at `path`.
fn prod(key: &str, path: &str, complexity: u64) -> AgentComplexityMeasurement {
    AgentComplexityMeasurement {
        stable_key: key.to_string(),
        symbol_name: format!("sym_{key}"),
        file_path: Some(path.to_string()),
        line: None,
        complexity,
        is_test: false,
        is_generated: false,
    }
}

/// A measurement at `path` with the two file facts set explicitly.
fn meas(
    key: &str,
    path: Option<&str>,
    complexity: u64,
    is_test: bool,
    is_generated: bool,
) -> AgentComplexityMeasurement {
    AgentComplexityMeasurement {
        stable_key: key.to_string(),
        symbol_name: format!("sym_{key}"),
        file_path: path.map(|p| p.to_string()),
        line: None,
        complexity,
        is_test,
        is_generated,
    }
}

/// The HIGH_COMPLEXITY evidence from a single-signal aggregator output.
fn complexity_evidence(out: &AggregatorOutput) -> &crate::dto::signal::HighComplexityEvidence {
    match out.signals[0].evidence() {
        SignalEvidence::HighComplexity(ev) => ev,
        other => panic!("expected HighComplexity, got {other:?}"),
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
            tracked_only_count: 0,
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
            tracked_only_count: 0,
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
            tracked_only_count: 0,
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

    // EXPLAIN-TYPE-SECTIONS-1: the complexity fake carries no type members / importers — this
    // double drives the complexity aggregator only, never a type-focus explain. Empty is honest
    // here (an unused fixture), NOT a defaulted read on a serving path.
    fn list_members_of_type(
        &self,
        _snapshot_uid: &str,
        _qualified_name: &str,
    ) -> Result<Vec<crate::AgentMemberEntry>, AgentStorageError> {
        Ok(Vec::new())
    }

    fn find_file_importers(
        &self,
        _snapshot_uid: &str,
        _file_path: &str,
    ) -> Result<Vec<crate::AgentFileImporter>, AgentStorageError> {
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
        line: None,
        complexity: 10, // Below default threshold of 20
        is_test: false,
        is_generated: false,
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
        line: None,
        complexity: 25,
        is_test: false,
        is_generated: false,
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
        line: None,
        complexity: 15,
        is_test: false,
        is_generated: false,
    }]);
    // Default threshold (20) - should not emit
    let result = aggregate(&storage, "snap1", Budget::Small).unwrap();
    assert!(result.signals.is_empty());

    // Lower threshold (10) - should emit
    let result = aggregate_with_threshold(&storage, "snap1", 10, Budget::Small, false).unwrap();
    assert_eq!(result.signals.len(), 1);
}

#[test]
fn evidence_contains_top_complex_symbols() {
    let storage = FakeStorage::with_measurements(vec![
        AgentComplexityMeasurement {
            stable_key: "k1".into(),
            symbol_name: "very_complex".into(),
            file_path: Some("a.rs".into()),
            line: None,
            complexity: 50,
            is_test: false,
            is_generated: false,
        },
        AgentComplexityMeasurement {
            stable_key: "k2".into(),
            symbol_name: "also_complex".into(),
            file_path: Some("b.rs".into()),
            line: None,
            complexity: 30,
            is_test: false,
            is_generated: false,
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
            line: None,
            complexity: 100 - i as u64, // descending → deterministic order
            is_test: false,
            is_generated: false,
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

// ── COMPLEXITY-SCOPE-1 (RG-REQ-009-L01): production scope + --include-all ──────

#[test]
fn generated_vendored_and_test_rows_are_excluded_and_counted() {
    let storage = FakeStorage::with_measurements(vec![
        prod("p1", "src/a.rs", 50),
        prod("p2", "src/b.rs", 40),
        prod("p3", "src/c.rs", 30),
        meas("t1", Some("tests/x.rs"), 60, true, false), // is_test
        meas("g1", Some("gen/p.c"), 55, false, true),    // is_generated
        meas(
            "v1",
            Some("dependencies/pcre2/src/pcre2_match.c"),
            70,
            false,
            false,
        ), // vendored
    ]);
    // Budget::Full so no budget truncation hides an unexpected row.
    let out = aggregate(&storage, "snap1", Budget::Full).unwrap();
    assert_eq!(out.signals.len(), 1);
    let ev = complexity_evidence(&out);
    assert_eq!(
        ev.high_complexity_count, 3,
        "count is the production subset"
    );
    assert_eq!(ev.scope, ComplexityScope::Production { excluded_count: 3 });
    let syms: Vec<&str> = ev.top_complex.iter().map(|c| c.symbol.as_str()).collect();
    assert_eq!(syms, vec!["sym_p1", "sym_p2", "sym_p3"]);
    // The vendored row (cx 70) was the highest; excluded, it does not lead the ranking.
    for s in &ev.top_complex {
        assert!(!s.symbol.starts_with("sym_t"), "no test symbol");
        assert!(!s.symbol.starts_with("sym_g"), "no generated symbol");
        assert!(!s.symbol.starts_with("sym_v"), "no vendored symbol");
    }
}

#[test]
fn include_all_keeps_every_row_and_reports_scope_all() {
    let storage = FakeStorage::with_measurements(vec![
        prod("p1", "src/a.rs", 50),
        meas("t1", Some("tests/x.rs"), 60, true, false),
        meas("g1", Some("gen/p.c"), 55, false, true),
        meas(
            "v1",
            Some("dependencies/pcre2/src/pcre2_match.c"),
            70,
            false,
            false,
        ),
    ]);
    let out = aggregate_with_threshold(&storage, "snap1", 20, Budget::Full, true).unwrap();
    let ev = complexity_evidence(&out);
    assert_eq!(
        ev.high_complexity_count, 4,
        "every above-threshold row kept"
    );
    assert_eq!(ev.scope, ComplexityScope::All);
    assert_eq!(ev.top_complex.len(), 4);
}

#[test]
fn all_rows_excluded_still_emits_the_signal_with_the_excluded_count() {
    let storage = FakeStorage::with_measurements(vec![
        meas("t1", Some("tests/a.rs"), 30, true, false),
        meas("t2", Some("tests/b.rs"), 31, true, false),
        meas("g1", Some("gen/c.c"), 32, false, true),
        meas("v1", Some("vendor/d.rs"), 33, false, false),
        meas("v2", Some("node_modules/e.js"), 34, false, false),
        meas("v3", Some("third_party/f.c"), 35, false, false),
    ]);
    let out = aggregate(&storage, "snap1", Budget::Full).unwrap();
    assert_eq!(
        out.signals.len(),
        1,
        "the signal is emitted even when scoping excludes every row"
    );
    let ev = complexity_evidence(&out);
    assert_eq!(ev.high_complexity_count, 0);
    assert!(ev.top_complex.is_empty());
    assert_eq!(ev.scope, ComplexityScope::Production { excluded_count: 6 });
}

#[test]
fn high_complexity_count_is_the_filtered_total_not_the_storage_count() {
    // The fake's `count_high_complexity_symbols` returns the UNFILTERED count (6); the
    // aggregator must report the SCOPED count (3), proving it counts the retained rows and
    // never the storage count.
    let storage = FakeStorage::with_measurements(vec![
        prod("p1", "src/a.rs", 50),
        prod("p2", "src/b.rs", 40),
        prod("p3", "src/c.rs", 30),
        meas("t1", Some("tests/x.rs"), 60, true, false),
        meas("g1", Some("gen/p.c"), 55, false, true),
        meas("v1", Some("vendor/y.rs"), 70, false, false),
    ]);
    assert_eq!(
        storage.count_high_complexity_symbols("snap1", 20).unwrap(),
        6,
        "the fake's storage count is the unfiltered total"
    );
    let out = aggregate(&storage, "snap1", Budget::Full).unwrap();
    let ev = complexity_evidence(&out);
    assert_eq!(ev.high_complexity_count, 3);
}

#[test]
fn a_row_without_a_file_stays_ranked() {
    // No owning file → no persisted fact → the row is KEPT (never a fabricated exclusion).
    let storage = FakeStorage::with_measurements(vec![
        meas("nf", None, 40, false, false),
        meas("t1", Some("tests/x.rs"), 50, true, false),
    ]);
    let out = aggregate(&storage, "snap1", Budget::Full).unwrap();
    let ev = complexity_evidence(&out);
    assert_eq!(ev.high_complexity_count, 1);
    assert_eq!(ev.scope, ComplexityScope::Production { excluded_count: 1 });
    assert_eq!(ev.top_complex.len(), 1);
    assert_eq!(ev.top_complex[0].symbol, "sym_nf");
    assert!(ev.top_complex[0].file.is_none());
}

#[test]
fn no_rows_above_threshold_emits_no_signal() {
    // Every measurement is below threshold — the unfiltered read is empty → no signal
    // (the former `count == 0` early return, now keyed on the empty read).
    let storage =
        FakeStorage::with_measurements(vec![prod("p1", "src/a.rs", 5), prod("p2", "src/b.rs", 10)]);
    let out = aggregate(&storage, "snap1", Budget::Full).unwrap();
    assert!(out.signals.is_empty());
    assert!(out.limits.is_empty());
}
