//! Integration tests for the `AgentStorageRead` adapter impl.
//!
//! Proves that `StorageConnection` correctly implements the
//! `AgentStorageRead` trait defined by the agent crate. These
//! tests live on the storage side (not the agent side) because
//! they exercise SQLite through the real adapter; the agent's
//! own test suite uses an in-memory fake to avoid this
//! dependency direction.
//!
//! Coverage intent:
//!   - DTO mapping: storage row shapes → agent-owned DTOs
//!   - Missing-row semantics: get_repo / get_latest_snapshot
//!     return `Ok(None)` not errors
//!   - compute_repo_summary: distinct-language rollup from the
//!     file_versions ∖ files join
//!   - get_stale_files: surfaces rows whose parse_status = 'stale'
//!
//! Not covered (intentional Rust-42 scope):
//!   - find_module_cycles, find_dead_nodes,
//!     get_active_boundary_declarations,
//!     find_imports_between_paths, get_trust_summary — these
//!     already have storage-level tests at the raw query path
//!     (`queries.rs`). The agent impl is a mechanical forwarder
//!     for them; duplicating the coverage would be theatre.

use repo_graph_agent::AgentStorageRead;
use repo_graph_storage::types::{
    CreateSnapshotInput, FileVersion, GraphEdge, GraphNode, MeasurementInput, Repo, SourceLocation,
    TrackedFile, UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;

// ── Helpers ──────────────────────────────────────────────────────

fn open_temp_storage() -> (tempfile::TempDir, StorageConnection) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("agent_impl_test.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    (dir, storage)
}

fn insert_repo(storage: &StorageConnection, uid: &str, name: &str) {
    storage
        .add_repo(&Repo {
            repo_uid: uid.to_string(),
            name: name.to_string(),
            root_path: format!("/tmp/{}", uid),
            default_branch: None,
            created_at: "2026-04-15T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
}

fn create_ready_snapshot(storage: &StorageConnection, repo_uid: &str) -> String {
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: repo_uid.to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap();
    storage
        .update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snap.snapshot_uid.clone(),
            status: "ready".to_string(),
            completed_at: Some("2026-04-15T00:01:00Z".to_string()),
        })
        .unwrap();
    snap.snapshot_uid
}

// ── get_repo ─────────────────────────────────────────────────────

#[test]
fn get_repo_returns_mapped_agent_repo() {
    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");

    let result = <StorageConnection as AgentStorageRead>::get_repo(&storage, "r1").unwrap();
    let repo = result.expect("repo exists");
    assert_eq!(repo.repo_uid, "r1");
    assert_eq!(repo.name, "my-repo");
}

#[test]
fn get_repo_returns_none_when_missing() {
    let (_tmp, storage) = open_temp_storage();

    let result = <StorageConnection as AgentStorageRead>::get_repo(&storage, "absent").unwrap();
    assert!(result.is_none());
}

// ── get_latest_snapshot ──────────────────────────────────────────

#[test]
fn get_latest_snapshot_maps_kind_to_scope() {
    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    let result =
        <StorageConnection as AgentStorageRead>::get_latest_snapshot(&storage, "r1").unwrap();
    let snap = result.expect("READY snapshot exists");
    assert_eq!(snap.snapshot_uid, snapshot_uid);
    assert_eq!(snap.repo_uid, "r1");
    // Storage column `kind` surfaces as agent DTO `scope`.
    assert_eq!(snap.scope, "full");
}

#[test]
fn get_latest_snapshot_returns_none_when_no_ready_snapshot() {
    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    // Repo exists but no snapshot → Ok(None).

    let result =
        <StorageConnection as AgentStorageRead>::get_latest_snapshot(&storage, "r1").unwrap();
    assert!(result.is_none());
}

// ── compute_repo_summary ─────────────────────────────────────────

#[test]
fn compute_repo_summary_rolls_up_languages_deterministically() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Seed three files with two distinct languages.
    storage
        .upsert_files(&[
            TrackedFile {
                file_uid: "f1".into(),
                repo_uid: "r1".into(),
                path: "src/a.rs".into(),
                language: Some("rust".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f2".into(),
                repo_uid: "r1".into(),
                path: "src/b.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f3".into(),
                repo_uid: "r1".into(),
                path: "src/c.rs".into(),
                language: Some("rust".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
        ])
        .unwrap();
    storage
        .upsert_file_versions(&[
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f1".into(),
                content_hash: "h1".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(10),
                line_count: Some(2),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f2".into(),
                content_hash: "h2".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(10),
                line_count: Some(2),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f3".into(),
                content_hash: "h3".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(10),
                line_count: Some(2),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
        ])
        .unwrap();

    let summary =
        <StorageConnection as AgentStorageRead>::compute_repo_summary(&storage, &snapshot_uid)
            .unwrap();
    assert_eq!(summary.file_count, 3);
    // symbol_count is zero until we seed nodes, and we deliberately
    // do NOT seed nodes here — this test's focus is language rollup.
    assert_eq!(summary.symbol_count, 0);
    // Languages are sorted ascending and deduplicated.
    assert_eq!(
        summary.languages,
        vec!["rust".to_string(), "typescript".to_string()]
    );
}

// ── get_stale_files ──────────────────────────────────────────────

#[test]
fn get_stale_files_maps_to_agent_paths() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    storage
        .upsert_files(&[TrackedFile {
            file_uid: "f1".into(),
            repo_uid: "r1".into(),
            path: "src/stale.rs".into(),
            language: Some("rust".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();
    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.clone(),
            file_uid: "f1".into(),
            content_hash: "h1".into(),
            ast_hash: None,
            extractor: None,
            parse_status: "stale".into(),
            size_bytes: Some(10),
            line_count: Some(2),
            indexed_at: "2026-04-15T00:00:00Z".into(),
        }])
        .unwrap();

    let stale =
        <StorageConnection as AgentStorageRead>::get_stale_files(&storage, &snapshot_uid).unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].path, "src/stale.rs");
}

// ── Trust summary: enrichment state disambiguation (P2) ─────────
//
// Regression coverage for the spike-follow-up P2 review: when
// the trust report has `enrichment_status = None`, the adapter
// must distinguish "no eligible samples" (NotApplicable) from
// "eligible samples but phase did not run" (NotRun). The
// distinguishing signal is `TrustReport.enrichment_eligible_count`.
//
// These tests use the real `StorageConnection` impl of
// `AgentStorageRead::get_trust_summary`, which is the call
// path that exercises the disambiguator end-to-end. An empty
// snapshot has zero eligible samples → `NotApplicable`.
//
// A NotRun case requires seeding an unresolved
// `CallsObjMethodNeedsTypeInfo` edge, which is more involved
// to fixture and is already covered by the spike re-run
// captured in `docs/spikes/2026-04-15-orient-on-repo-graph.md`.
// Here we pin the cheaper case that the storage adapter must
// also handle correctly.

#[test]
fn empty_snapshot_maps_to_enrichment_state_not_applicable() {
    use repo_graph_agent::EnrichmentState;

    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    let trust =
        <StorageConnection as AgentStorageRead>::get_trust_summary(&storage, "r1", &snapshot_uid)
            .unwrap();

    // Empty snapshot has zero CallsObjMethodNeedsTypeInfo
    // samples. The adapter must NOT report NotRun (the
    // pre-P2-fix bug) — it must report NotApplicable so the
    // agent pipeline does not fire a spurious
    // TRUST_NO_ENRICHMENT signal and does not penalize
    // confidence on the enrichment axis.
    assert_eq!(
        trust.enrichment_state,
        EnrichmentState::NotApplicable,
        "empty snapshot must map to NotApplicable; pre-P2 the adapter \
		 conflated this with NotRun and the spike re-run would have \
		 reported a false positive"
    );
    assert_eq!(trust.enrichment_eligible, 0);
    assert_eq!(trust.enrichment_enriched, 0);
}

#[test]
fn snapshot_with_unresolved_obj_method_call_maps_to_enrichment_state_not_run() {
    // The other branch of the P2 disambiguator. Seeds a
    // `CallsObjMethodNeedsTypeInfo` unresolved edge with NO
    // enrichment metadata. The trust layer's compute path will:
    //
    //   1. Count the row in `all_classification_counts`
    //      (non-empty), so the blast/enrichment computation runs.
    //   2. Sample it via `query_unresolved_edges(classification =
    //      Unknown)` — the row's `classification` is `"unknown"`,
    //      so it appears.
    //   3. Find no `enrichment` key in `metadata_json` (NULL),
    //      so `enrichment_was_run` stays false.
    //   4. Return `enrichment_status = None`,
    //      `enrichment_eligible_count = 1`.
    //
    // The adapter must then map this to `EnrichmentState::NotRun`
    // — NOT `NotApplicable`. This is the actual code path that
    // drives `TRUST_NO_ENRICHMENT` emission and the confidence
    // penalty in production. Pre-P2 the adapter conflated this
    // with `NotApplicable`; the regression at the manual spike
    // caught it. This test pins the behavior in CI.
    use repo_graph_agent::EnrichmentState;
    use repo_graph_storage::types::GraphNode;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("notrun.db");
    let mut storage = StorageConnection::open(&db_path).unwrap();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Insert a SYMBOL node so the unresolved_edges
    // `source_node_uid` foreign key resolves. The visibility
    // is `"export"` so the trust sample carries a sensible
    // blast-radius input — not strictly required for this
    // test but matches realistic data.
    storage
        .insert_nodes(&[GraphNode {
            node_uid: "n1".into(),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: "r1".into(),
            stable_key: "r1:src/a.ts:caller:SYMBOL".into(),
            kind: "SYMBOL".into(),
            subtype: Some("FUNCTION".into()),
            name: "caller".into(),
            qualified_name: Some("src/a.ts:caller".into()),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: Some("export".into()),
            doc_comment: None,
            metadata_json: None,
        }])
        .unwrap();

    // Insert one unresolved edge directly via a parallel
    // rusqlite connection. The storage crate has a private
    // helper for this in trust_impl tests; integration tests
    // do not have access to it, so the SQL is inlined here.
    // Schema reference: migration_007.rs.
    //
    // Critical fields:
    //   - category = "calls_obj_method_needs_type_info" so
    //     trust counts it as enrichment-eligible.
    //   - classification = "unknown" so trust's
    //     `query_unresolved_edges(classification=Unknown)`
    //     surfaces it as a sample.
    //   - metadata_json = NULL so `enrichment_was_run` stays
    //     false on the compute side, producing
    //     `enrichment_status = None` with eligible_count = 1.
    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        raw.execute(
            "INSERT INTO unresolved_edges \
			 (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
			  target_key, type, resolution, extractor, \
			  category, classification, classifier_version, \
			  basis_code, observed_at) \
			 VALUES (?, ?, 'r1', 'n1', \
			  'target::key', 'CALLS', 'unresolved', 'ts-base:1', \
			  'calls_obj_method_needs_type_info', 'unknown', 1, \
			  'no_supporting_signal', '2025-01-01T00:00:00.000Z')",
            rusqlite::params!["ue1", &snapshot_uid],
        )
        .unwrap();
    }

    // Adapter call.
    let trust =
        <StorageConnection as AgentStorageRead>::get_trust_summary(&storage, "r1", &snapshot_uid)
            .unwrap();

    assert_eq!(
        trust.enrichment_state,
        EnrichmentState::NotRun,
        "snapshot with eligible CallsObjMethodNeedsTypeInfo sample but no \
		 enrichment metadata must map to NotRun. Pre-P2 the adapter could \
		 not see this case (Option<EnrichmentStatus> alone collapsed it \
		 with NotApplicable). The fix added `enrichment_eligible_count` to \
		 the TrustReport so the adapter can disambiguate."
    );
    assert_eq!(
        trust.enrichment_eligible, 1,
        "the eligible count must be preserved through the adapter so \
		 downstream consumers see the same value the trust layer computed"
    );
    assert_eq!(trust.enrichment_enriched, 0);
}

// ── end-to-end orient over real storage ──────────────────────────

#[test]
fn orient_runs_over_real_storage_connection() {
    // Prove the full orient pipeline works when driven through
    // a real StorageConnection, not a fake. This is the single
    // smoke test that exercises the whole policy ↔ adapter
    // boundary end-to-end. It intentionally uses an almost-empty
    // repo to keep the fixture trivial; signal correctness is
    // covered by the agent crate's own test suite against the
    // fake.
    //
    // ── Expected limit set on an empty snapshot ──
    //
    // Limits on this fixture (3 total):
    //   1. MODULE_DATA_UNAVAILABLE (no module_candidates)
    //   2. GATE_NOT_CONFIGURED (no requirement declarations)
    //   3. COMPLEXITY_UNAVAILABLE (no complexity measurements)
    //
    // When data exists:
    //   - MODULE_DATA_UNAVAILABLE suppressed when module_candidates has data
    //     (Phase 4: MODULE nodes are NOT used as fallback)
    //   - COMPLEXITY_UNAVAILABLE suppressed when complexity measurements exist
    //
    // Dead-code surface is withdrawn — no DEAD_CODE signal or
    // DEAD_CODE_UNRELIABLE limit. Internal substrate preserved
    // but not surfaced to orient output.
    use repo_graph_agent::{orient, Budget, ORIENT_SCHEMA};

    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    let result = orient(&storage, "r1", None, Budget::Large, "2026-04-15T00:00:00Z").unwrap();
    assert_eq!(result.schema, ORIENT_SCHEMA);
    assert_eq!(result.repo, "my-repo");
    assert_eq!(result.snapshot, snapshot_uid);

    assert_eq!(
        result.limits.len(),
        3,
        "empty snapshot must emit MODULE_DATA_UNAVAILABLE + \
		 GATE_NOT_CONFIGURED + COMPLEXITY_UNAVAILABLE; actual: {:?}",
        result.limits.iter().map(|l| l.code).collect::<Vec<_>>()
    );

    // No dead-code vocabulary should appear in limits or signals.
    for limit in &result.limits {
        assert!(
            !limit.code.as_str().contains("DEAD"),
            "no dead-code limit should appear: {}",
            limit.code.as_str()
        );
    }
    for signal in &result.signals {
        assert!(
            !signal.code().as_str().contains("DEAD"),
            "no dead-code signal should appear: {}",
            signal.code().as_str()
        );
    }

    // At minimum MODULE_SUMMARY + SNAPSHOT_INFO fire.
    assert!(result.signals.len() >= 2);
}

// ── Characterization: trust reliability axes on empty snapshot ──
//
// Pins the adapter seam behavior that `check` will reduce. These
// are NOT tests of check — they pin what the adapter currently
// returns when the trust crate processes specific data shapes.

#[test]
fn trust_reliability_axes_on_empty_snapshot() {
    // Characterization: get_trust_summary on a READY snapshot with
    // zero files/nodes/edges. Pins the trust crate's behavior for
    // the empty-data case so check can rely on these values.
    use repo_graph_agent::{AgentReliabilityLevel, EnrichmentState};

    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    let trust =
        <StorageConnection as AgentStorageRead>::get_trust_summary(&storage, "r1", &snapshot_uid)
            .unwrap();

    // call_graph_reliability: trust rule returns HIGH when total
    // calls = 0 (no data to be unreliable about).
    assert_eq!(
        trust.call_graph_reliability.level,
        AgentReliabilityLevel::High,
        "empty snapshot call_graph_reliability must be High \
		 (trust returns HIGH when total=0)"
    );

    // dead_code_reliability: trust rule fires
    // missing_entrypoint_declarations when active_entrypoint_count
    // = 0, which downgrades dead_code to LOW.
    assert_eq!(
        trust.dead_code_reliability.level,
        AgentReliabilityLevel::Low,
        "empty snapshot dead_code_reliability must be Low \
		 (missing_entrypoint_declarations fires when active_entrypoint_count=0)"
    );

    // call_resolution_rate: trust defaults to 1.0 when total
    // calls = 0 (no unresolved data → nothing to penalize).
    assert!(
        (trust.call_resolution_rate - 1.0).abs() < f64::EPSILON,
        "empty snapshot call_resolution_rate must be 1.0 (no-data default); \
		 actual: {}",
        trust.call_resolution_rate
    );

    // enrichment_state: already tested separately, but pin it
    // alongside the reliability axes for completeness.
    assert_eq!(
        trust.enrichment_state,
        EnrichmentState::NotApplicable,
        "empty snapshot enrichment_state must be NotApplicable"
    );
}

// ── Characterization: trust reliability axes with call data ─────

#[test]
fn trust_reliability_axes_with_call_data() {
    // Characterization: get_trust_summary on a snapshot with
    // resolved CALLS edges AND extraction diagnostics recording
    // unresolved calls. Pins the non-trivial reliability
    // computation path.
    //
    // The trust crate reads `resolved_calls` from
    // `count_edges_by_type(snapshot_uid, "CALLS")` (the edges
    // table) and `unresolved_calls` from
    // `ExtractionDiagnostics.unresolved_breakdown` (the
    // `extraction_diagnostics_json` column on snapshots). Both
    // must be seeded for a non-trivial call_resolution_rate.
    use repo_graph_agent::AgentReliabilityLevel;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("trust_calls.db");
    let mut storage = StorageConnection::open(&db_path).unwrap();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Insert two SYMBOL nodes so the CALLS edge has valid
    // source/target references.
    storage
        .insert_nodes(&[
            GraphNode {
                node_uid: "n1".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/a.ts:caller:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "caller".into(),
                qualified_name: Some("src/a.ts:caller".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: Some("export".into()),
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "n2".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/b.ts:callee:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "callee".into(),
                qualified_name: Some("src/b.ts:callee".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: Some("export".into()),
                doc_comment: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    // Insert one resolved CALLS edge. This drives
    // `resolved_calls = 1` through `count_edges_by_type`.
    storage
        .insert_edges(&[GraphEdge {
            edge_uid: "e1".into(),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: "r1".into(),
            source_node_uid: "n1".into(),
            target_node_uid: "n2".into(),
            edge_type: "CALLS".into(),
            resolution: "static".into(),
            extractor: "ts-base:1".into(),
            location: None,
            metadata_json: None,
        }])
        .unwrap();

    // Seed extraction diagnostics with 1 unresolved call in a
    // CALLS-family category. The trust crate reads unresolved
    // calls from this JSON, not from the unresolved_edges table.
    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        let diagnostics_json = serde_json::json!({
            "diagnostics_version": 1,
            "edges_total": 2,
            "unresolved_total": 1,
            "unresolved_breakdown": {
                "calls_function_ambiguous_or_missing": 1
            }
        });
        raw.execute(
            "UPDATE snapshots SET extraction_diagnostics_json = ? \
			 WHERE snapshot_uid = ?",
            rusqlite::params![diagnostics_json.to_string(), &snapshot_uid],
        )
        .unwrap();
    }

    let trust =
        <StorageConnection as AgentStorageRead>::get_trust_summary(&storage, "r1", &snapshot_uid)
            .unwrap();

    // call_resolution_rate: 1 resolved / (1 resolved + 1
    // unresolved) = 0.5. Must be between 0 and 1 (not the
    // empty-default 1.0).
    assert!(
        trust.call_resolution_rate > 0.0 && trust.call_resolution_rate < 1.0,
        "call_resolution_rate with mixed resolved/unresolved must be \
		 between 0 and 1; actual: {}",
        trust.call_resolution_rate
    );
    assert!(
        (trust.call_resolution_rate - 0.5).abs() < f64::EPSILON,
        "expected call_resolution_rate = 0.5 (1 resolved, 1 unresolved \
		 internal-like); actual: {}",
        trust.call_resolution_rate
    );

    // call_graph_reliability: the trust rule uses rate < 0.5 →
    // LOW, rate <= 0.85 → MEDIUM, rate > 0.85 → HIGH. At exactly
    // 0.5, the rate is not < 0.5, so it falls into MEDIUM.
    assert_eq!(
        trust.call_graph_reliability.level,
        AgentReliabilityLevel::Medium,
        "call_graph_reliability at 50% resolution rate must be Medium"
    );

    // dead_code_reliability: still no entrypoints → LOW.
    assert_eq!(
        trust.dead_code_reliability.level,
        AgentReliabilityLevel::Low,
        "dead_code_reliability must still be Low (no entrypoints seeded)"
    );
}

// ── Characterization: stale-files filtering ─────────────────────

#[test]
fn get_stale_files_returns_only_stale_not_ok() {
    // Characterization: pin that get_stale_files returns only rows
    // whose parse_status = 'stale', and that adding an 'ok' file
    // does not inflate the stale count.
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Seed one file with parse_status = 'stale'.
    storage
        .upsert_files(&[TrackedFile {
            file_uid: "f1".into(),
            repo_uid: "r1".into(),
            path: "src/stale_file.rs".into(),
            language: Some("rust".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();
    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.clone(),
            file_uid: "f1".into(),
            content_hash: "h1".into(),
            ast_hash: None,
            extractor: None,
            parse_status: "stale".into(),
            size_bytes: Some(10),
            line_count: Some(2),
            indexed_at: "2026-04-15T00:00:00Z".into(),
        }])
        .unwrap();

    // First call: exactly 1 stale file.
    let stale =
        <StorageConnection as AgentStorageRead>::get_stale_files(&storage, &snapshot_uid).unwrap();
    assert_eq!(
        stale.len(),
        1,
        "must return exactly 1 stale file before adding ok file"
    );
    assert_eq!(stale[0].path, "src/stale_file.rs");

    // Seed a second file with parse_status = 'ok'.
    storage
        .upsert_files(&[TrackedFile {
            file_uid: "f2".into(),
            repo_uid: "r1".into(),
            path: "src/ok_file.rs".into(),
            language: Some("rust".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();
    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.clone(),
            file_uid: "f2".into(),
            content_hash: "h2".into(),
            ast_hash: None,
            extractor: None,
            parse_status: "ok".into(),
            size_bytes: Some(20),
            line_count: Some(5),
            indexed_at: "2026-04-15T00:00:00Z".into(),
        }])
        .unwrap();

    // Second call: still exactly 1 stale file.
    let stale_after =
        <StorageConnection as AgentStorageRead>::get_stale_files(&storage, &snapshot_uid).unwrap();
    assert_eq!(
        stale_after.len(),
        1,
        "stale count must not increase when an 'ok' file is added; \
		 actual stale files: {:?}",
        stale_after.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
    assert_eq!(stale_after[0].path, "src/stale_file.rs");
}

// ── Explain port methods ────────────────────────────────────────────

#[test]
fn list_symbols_in_file_returns_ordered_entries() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Seed a file.
    storage
        .upsert_files(&[TrackedFile {
            file_uid: "f1".into(),
            repo_uid: "r1".into(),
            path: "src/service.ts".into(),
            language: Some("typescript".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();
    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.clone(),
            file_uid: "f1".into(),
            content_hash: "h1".into(),
            ast_hash: None,
            extractor: None,
            parse_status: "ok".into(),
            size_bytes: Some(100),
            line_count: Some(20),
            indexed_at: "2026-04-15T00:00:00Z".into(),
        }])
        .unwrap();

    // Seed two SYMBOL nodes in the file (line 10 and line 5).
    storage
        .insert_nodes(&[
            GraphNode {
                node_uid: "n1".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/service.ts:beta:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "beta".into(),
                qualified_name: Some("src/service.ts:beta".into()),
                file_uid: Some("f1".into()),
                parent_node_uid: None,
                location: Some(SourceLocation {
                    line_start: 10,
                    col_start: 0,
                    line_end: 15,
                    col_end: 0,
                }),
                signature: None,
                visibility: Some("export".into()),
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "n2".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/service.ts:alpha:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("CLASS".into()),
                name: "alpha".into(),
                qualified_name: Some("src/service.ts:alpha".into()),
                file_uid: Some("f1".into()),
                parent_node_uid: None,
                location: Some(SourceLocation {
                    line_start: 5,
                    col_start: 0,
                    line_end: 8,
                    col_end: 0,
                }),
                signature: None,
                visibility: Some("export".into()),
                doc_comment: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    let symbols = <StorageConnection as AgentStorageRead>::list_symbols_in_file(
        &storage,
        &snapshot_uid,
        "src/service.ts",
    )
    .unwrap();

    assert_eq!(symbols.len(), 2);
    // Ordered by line_start ASC: alpha (5) before beta (10).
    assert_eq!(symbols[0].name, "alpha");
    assert_eq!(symbols[0].subtype.as_deref(), Some("CLASS"));
    assert_eq!(symbols[0].line_start, Some(5));
    assert_eq!(symbols[1].name, "beta");
    assert_eq!(symbols[1].subtype.as_deref(), Some("FUNCTION"));
    assert_eq!(symbols[1].line_start, Some(10));
}

#[test]
fn list_files_in_path_returns_files_under_prefix() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Seed two files under src/core and one under src/adapters.
    storage
        .upsert_files(&[
            TrackedFile {
                file_uid: "f1".into(),
                repo_uid: "r1".into(),
                path: "src/core/model.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f2".into(),
                repo_uid: "r1".into(),
                path: "src/core/service.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f3".into(),
                repo_uid: "r1".into(),
                path: "src/adapters/storage.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
        ])
        .unwrap();
    storage
        .upsert_file_versions(&[
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f1".into(),
                content_hash: "h1".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(100),
                line_count: Some(10),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f2".into(),
                content_hash: "h2".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(200),
                line_count: Some(20),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f3".into(),
                content_hash: "h3".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(50),
                line_count: Some(5),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
        ])
        .unwrap();

    let files = <StorageConnection as AgentStorageRead>::list_files_in_path(
        &storage,
        &snapshot_uid,
        "src/core",
    )
    .unwrap();

    assert_eq!(files.len(), 2, "only files under src/core");
    // Ordered by path ASC.
    assert_eq!(files[0].path, "src/core/model.ts");
    assert_eq!(files[1].path, "src/core/service.ts");
    // symbol_count is 0 since we did not seed nodes.
    assert_eq!(files[0].symbol_count, 0);
    assert!(!files[0].is_test);
}

#[test]
fn find_file_imports_returns_distinct_targets() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Seed two files.
    storage
        .upsert_files(&[
            TrackedFile {
                file_uid: "f1".into(),
                repo_uid: "r1".into(),
                path: "src/a.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f2".into(),
                repo_uid: "r1".into(),
                path: "src/b.ts".into(),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
        ])
        .unwrap();
    storage
        .upsert_file_versions(&[
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f1".into(),
                content_hash: "h1".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(10),
                line_count: Some(2),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
            FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: "f2".into(),
                content_hash: "h2".into(),
                ast_hash: None,
                extractor: None,
                parse_status: "ok".into(),
                size_bytes: Some(10),
                line_count: Some(2),
                indexed_at: "2026-04-15T00:00:00Z".into(),
            },
        ])
        .unwrap();

    // Seed nodes in both files.
    storage
        .insert_nodes(&[
            GraphNode {
                node_uid: "n1".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/a.ts:foo:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "foo".into(),
                qualified_name: None,
                file_uid: Some("f1".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "n2".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/b.ts:bar:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "bar".into(),
                qualified_name: None,
                file_uid: Some("f2".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            // Second node in f1 to create a duplicate import target.
            GraphNode {
                node_uid: "n3".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/a.ts:baz:SYMBOL".into(),
                kind: "SYMBOL".into(),
                subtype: Some("FUNCTION".into()),
                name: "baz".into(),
                qualified_name: None,
                file_uid: Some("f1".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    // Two IMPORTS edges from a.ts nodes -> b.ts node.
    storage
        .insert_edges(&[
            GraphEdge {
                edge_uid: "e1".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "n1".into(),
                target_node_uid: "n2".into(),
                edge_type: "IMPORTS".into(),
                resolution: "static".into(),
                extractor: "ts-base:1".into(),
                location: None,
                metadata_json: None,
            },
            GraphEdge {
                edge_uid: "e2".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "n3".into(),
                target_node_uid: "n2".into(),
                edge_type: "IMPORTS".into(),
                resolution: "static".into(),
                extractor: "ts-base:1".into(),
                location: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    let imports = <StorageConnection as AgentStorageRead>::find_file_imports(
        &storage,
        &snapshot_uid,
        "src/a.ts",
    )
    .unwrap();

    // Two edges but they both target the same file → 1 distinct result.
    assert_eq!(
        imports.len(),
        1,
        "DISTINCT must deduplicate same target file"
    );
    assert_eq!(imports[0].target_file, "src/b.ts");
}

// ── get_module_summary ───────────────────────────────────────────

#[test]
fn get_module_summary_returns_none_when_no_candidates() {
    // Empty module_candidates table → None (no fallback after Phase 4).
    let (_tmp, storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    let summary =
        <StorageConnection as AgentStorageRead>::get_module_summary(&storage, &snapshot_uid)
            .unwrap();

    assert!(
        summary.is_none(),
        "get_module_summary must return None when module_candidates is empty"
    );
}

#[test]
fn get_module_summary_returns_counts_grouped_by_kind() {
    // Seed module_candidates with different kinds → returns grouped counts.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("module_summary.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Insert module candidates with different kinds directly via SQL.
    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        raw.execute(
            "INSERT INTO module_candidates \
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
			  module_kind, canonical_root_path, confidence, display_name) \
			 VALUES (?, ?, 'r1', 'npm:@test/core', 'declared', 'packages/core', 1.0, '@test/core')",
            rusqlite::params!["mc1", &snapshot_uid],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO module_candidates \
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
			  module_kind, canonical_root_path, confidence, display_name) \
			 VALUES (?, ?, 'r1', 'npm:@test/utils', 'declared', 'packages/utils', 1.0, '@test/utils')",
            rusqlite::params!["mc2", &snapshot_uid],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO module_candidates \
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
			  module_kind, canonical_root_path, confidence, display_name) \
			 VALUES (?, ?, 'r1', 'op:cli', 'operational', 'apps/cli', 1.0, 'cli')",
            rusqlite::params!["mc3", &snapshot_uid],
        )
        .unwrap();
        raw.execute(
            "INSERT INTO module_candidates \
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
			  module_kind, canonical_root_path, confidence, display_name) \
			 VALUES (?, ?, 'r1', 'inf:vendor', 'inferred', 'vendor', 0.8, NULL)",
            rusqlite::params!["mc4", &snapshot_uid],
        )
        .unwrap();
    }

    let summary =
        <StorageConnection as AgentStorageRead>::get_module_summary(&storage, &snapshot_uid)
            .unwrap();

    let summary = summary.expect("get_module_summary must return Some when candidates exist");
    assert_eq!(summary.discovered_module_count, 4, "total module count");
    assert_eq!(summary.declared_count, 2, "declared module count");
    assert_eq!(summary.operational_count, 1, "operational module count");
    assert_eq!(summary.inferred_count, 1, "inferred module count");
}

#[test]
fn get_module_summary_treats_directory_kind_as_inferred() {
    // Inferred modules have module_kind = "directory" or "inferred".
    // Both should be counted in the inferred_count.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("module_summary_dir.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        raw.execute(
            "INSERT INTO module_candidates \
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
			  module_kind, canonical_root_path, confidence, display_name) \
			 VALUES (?, ?, 'r1', 'r1:src:MODULE', 'directory', 'src', 1.0, 'src')",
            rusqlite::params!["mc1", &snapshot_uid],
        )
        .unwrap();
    }

    let summary =
        <StorageConnection as AgentStorageRead>::get_module_summary(&storage, &snapshot_uid)
            .unwrap();

    let summary = summary.expect("get_module_summary must return Some");
    assert_eq!(summary.discovered_module_count, 1);
    assert_eq!(summary.declared_count, 0);
    assert_eq!(summary.operational_count, 0);
    assert_eq!(summary.inferred_count, 1, "directory kind maps to inferred");
}

#[test]
fn get_module_summary_no_fallback_after_phase_4() {
    // Phase 4 (2026-05-10): MODULE-node fallback removed.
    // When module_candidates is empty, get_module_summary returns None
    // even if MODULE nodes exist. This surfaces repos that need module
    // detection configured rather than silently providing degraded data.
    use repo_graph_storage::types::GraphNode;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("module_no_fallback.db");
    let mut storage = StorageConnection::open(&db_path).unwrap();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Insert MODULE nodes directly (legacy Rust indexer path).
    // These are NOT in module_candidates, only in nodes table.
    storage
        .insert_nodes(&[
            GraphNode {
                node_uid: "mod1".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src:MODULE".into(),
                kind: "MODULE".into(),
                subtype: None,
                name: "src".into(),
                qualified_name: Some("src".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "mod2".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/core:MODULE".into(),
                kind: "MODULE".into(),
                subtype: None,
                name: "core".into(),
                qualified_name: Some("src/core".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "mod3".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:src/adapters:MODULE".into(),
                kind: "MODULE".into(),
                subtype: None,
                name: "adapters".into(),
                qualified_name: Some("src/adapters".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    // module_candidates is empty, MODULE nodes exist but are NOT used.
    let summary =
        <StorageConnection as AgentStorageRead>::get_module_summary(&storage, &snapshot_uid)
            .unwrap();

    assert!(
        summary.is_none(),
        "Phase 4: get_module_summary must return None when module_candidates \
		 is empty, even if MODULE nodes exist. No fallback."
    );
}

// ── EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): list_members_of_type + find_file_importers ──

/// Build a SYMBOL GraphNode for a member fixture: `qualified_name`-keyed, in `file_uid`, with an
/// optional line and optional raw `metadata_json` (for the forward_decl tri-state).
#[allow(clippy::too_many_arguments)]
fn member_node(
    uid: &str,
    snapshot_uid: &str,
    stable_key: &str,
    name: &str,
    qualified_name: &str,
    subtype: &str,
    file_uid: &str,
    line: Option<i64>,
    metadata_json: Option<&str>,
) -> GraphNode {
    GraphNode {
        node_uid: uid.into(),
        snapshot_uid: snapshot_uid.into(),
        repo_uid: "r1".into(),
        stable_key: stable_key.into(),
        kind: "SYMBOL".into(),
        subtype: Some(subtype.into()),
        name: name.into(),
        qualified_name: Some(qualified_name.into()),
        file_uid: Some(file_uid.into()),
        parent_node_uid: None,
        location: line.map(|l| SourceLocation {
            line_start: l,
            col_start: 0,
            line_end: l,
            col_end: 0,
        }),
        signature: None,
        visibility: None,
        doc_comment: None,
        metadata_json: metadata_json.map(|s| s.to_string()),
    }
}

fn seed_file(storage: &mut StorageConnection, snapshot_uid: &str, file_uid: &str, path: &str) {
    storage
        .upsert_files(&[TrackedFile {
            file_uid: file_uid.into(),
            repo_uid: "r1".into(),
            path: path.into(),
            language: Some("cpp".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();
    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.into(),
            file_uid: file_uid.into(),
            content_hash: "h".into(),
            ast_hash: None,
            extractor: None,
            parse_status: "ok".into(),
            size_bytes: Some(1),
            line_count: Some(1),
            indexed_at: "2026-04-15T00:00:00Z".into(),
        }])
        .unwrap();
}

#[test]
fn list_members_of_type_returns_direct_members_only() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");
    seed_file(&mut storage, &snapshot_uid, "f1", "src/a.h");
    seed_file(&mut storage, &snapshot_uid, "f2", "src/other.py");

    storage
        .insert_nodes(&[
            // The type itself (subtype CLASS) — used only for the focus-file ordering probe.
            member_node(
                "nt",
                &snapshot_uid,
                "r1:src/a.h#A:SYMBOL:CLASS",
                "A",
                "A",
                "CLASS",
                "f1",
                Some(1),
                None,
            ),
            // Direct members via `::`.
            member_node(
                "n1",
                &snapshot_uid,
                "r1:src/a.h#A::m1:SYMBOL:METHOD",
                "m1",
                "A::m1",
                "METHOD",
                "f1",
                Some(3),
                None,
            ),
            member_node(
                "n2",
                &snapshot_uid,
                "r1:src/a.h#A::m2:SYMBOL:METHOD",
                "m2",
                "A::m2",
                "METHOD",
                "f1",
                Some(4),
                None,
            ),
            // A nested TYPE `A::Inner` — a direct member of A (one more `::` segment).
            member_node(
                "n3",
                &snapshot_uid,
                "r1:src/a.h#A::Inner:SYMBOL:CLASS",
                "Inner",
                "A::Inner",
                "CLASS",
                "f1",
                Some(5),
                None,
            ),
            // `A::Inner::m3` — NESTED under Inner, NOT a direct member of A (two `::` segments).
            member_node(
                "n4",
                &snapshot_uid,
                "r1:src/a.h#A::Inner::m3:SYMBOL:METHOD",
                "m3",
                "A::Inner::m3",
                "METHOD",
                "f1",
                Some(6),
                None,
            ),
            // A dotted sibling `A.m4` in ANOTHER file — a direct member via the `.` separator.
            member_node(
                "n5",
                &snapshot_uid,
                "r1:src/other.py#A.m4:SYMBOL:METHOD",
                "m4",
                "A.m4",
                "METHOD",
                "f2",
                Some(2),
                None,
            ),
        ])
        .unwrap();

    let members =
        <StorageConnection as AgentStorageRead>::list_members_of_type(&storage, &snapshot_uid, "A")
            .unwrap();

    let qns: Vec<&str> = members.iter().map(|m| m.qualified_name.as_str()).collect();
    // Direct members only: `::` members, the nested TYPE, AND the dotted `.` member — never the
    // doubly-nested `A::Inner::m3`.
    assert!(qns.contains(&"A::m1"), "A::m1 is a direct member: {qns:?}");
    assert!(qns.contains(&"A::m2"), "A::m2 is a direct member: {qns:?}");
    assert!(
        qns.contains(&"A::Inner"),
        "A::Inner is a direct member: {qns:?}"
    );
    assert!(
        qns.contains(&"A.m4"),
        "A.m4 (dotted sibling) is a direct member: {qns:?}"
    );
    assert!(
        !qns.contains(&"A::Inner::m3"),
        "A::Inner::m3 is nested, NOT a direct member of A: {qns:?}"
    );
    assert!(
        !qns.contains(&"A"),
        "the type itself is not its own member: {qns:?}"
    );
    assert_eq!(members.len(), 4, "exactly the four direct members: {qns:?}");
}

#[test]
fn list_members_of_type_orders_focus_file_first_then_other_files_by_path_and_line() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");
    // The type is DEFINED in the header (focus file); one member is defined out-of-line in a `.cpp`
    // whose path sorts BEFORE the header ("a.cpp" < "a.h"), proving focus-file-first beats path order.
    seed_file(&mut storage, &snapshot_uid, "fh", "src/a.h");
    seed_file(&mut storage, &snapshot_uid, "fc", "src/a.cpp");

    storage
        .insert_nodes(&[
            member_node(
                "nt",
                &snapshot_uid,
                "r1:src/a.h#A:SYMBOL:CLASS",
                "A",
                "A",
                "CLASS",
                "fh",
                Some(1),
                None,
            ),
            // Header members (focus file), out of line order — later line first to prove line-sort.
            member_node(
                "nh2",
                &snapshot_uid,
                "r1:src/a.h#A::later:SYMBOL:METHOD",
                "later",
                "A::later",
                "METHOD",
                "fh",
                Some(20),
                None,
            ),
            member_node(
                "nh1",
                &snapshot_uid,
                "r1:src/a.h#A::early:SYMBOL:METHOD",
                "early",
                "A::early",
                "METHOD",
                "fh",
                Some(10),
                None,
            ),
            // A definition in a.cpp (other file, path sorts first alphabetically).
            member_node(
                "nc",
                &snapshot_uid,
                "r1:src/a.cpp#A::defd:SYMBOL:METHOD",
                "defd",
                "A::defd",
                "METHOD",
                "fc",
                Some(81),
                None,
            ),
        ])
        .unwrap();

    let members =
        <StorageConnection as AgentStorageRead>::list_members_of_type(&storage, &snapshot_uid, "A")
            .unwrap();

    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    // Focus file (src/a.h) members first, by line (early < later); THEN the other file (src/a.cpp).
    assert_eq!(
        names,
        vec!["early", "later", "defd"],
        "focus-file-first by line, then other files by path/line: {names:?}"
    );
}

#[test]
fn list_members_of_type_marks_forward_declarations() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");
    seed_file(&mut storage, &snapshot_uid, "f1", "src/a.h");

    storage
        .insert_nodes(&[
            member_node(
                "nt",
                &snapshot_uid,
                "r1:src/a.h#A:SYMBOL:CLASS",
                "A",
                "A",
                "CLASS",
                "f1",
                Some(1),
                None,
            ),
            // forward_decl: true → true.
            member_node(
                "n1",
                &snapshot_uid,
                "r1:src/a.h#A::decl:SYMBOL:METHOD",
                "decl",
                "A::decl",
                "METHOD",
                "f1",
                Some(3),
                Some(r#"{"forward_decl": true}"#),
            ),
            // No metadata (absent key) → false.
            member_node(
                "n2",
                &snapshot_uid,
                "r1:src/a.h#A::def:SYMBOL:METHOD",
                "def",
                "A::def",
                "METHOD",
                "f1",
                Some(4),
                None,
            ),
        ])
        .unwrap();

    let members =
        <StorageConnection as AgentStorageRead>::list_members_of_type(&storage, &snapshot_uid, "A")
            .unwrap();
    let decl = members.iter().find(|m| m.name == "decl").unwrap();
    let def = members.iter().find(|m| m.name == "def").unwrap();
    assert!(decl.forward_decl, "forward_decl:true is read as a fact");
    assert!(!def.forward_decl, "absent metadata is false, not an error");

    // A PRESENT-but-unreadable forward_decl value (a string) is an ERROR, never defaulted to false.
    storage
        .insert_nodes(&[member_node(
            "n3",
            &snapshot_uid,
            "r1:src/a.h#A::weird:SYMBOL:METHOD",
            "weird",
            "A::weird",
            "METHOD",
            "f1",
            Some(5),
            Some(r#"{"forward_decl": "yes"}"#),
        )])
        .unwrap();
    let err =
        <StorageConnection as AgentStorageRead>::list_members_of_type(&storage, &snapshot_uid, "A");
    assert!(
        err.is_err(),
        "an unreadable forward_decl value is an error, never a silent default"
    );
}

#[test]
fn find_file_importers_lists_importing_files_with_owning_module() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");
    seed_file(&mut storage, &snapshot_uid, "ft", "zzz/target.h");
    seed_file(&mut storage, &snapshot_uid, "fa", "aaa/a.cpp");
    seed_file(&mut storage, &snapshot_uid, "fb", "bbb/b.cpp");

    // FILE nodes for each file; MODULE nodes for aaa/bbb with OWNS edges to their files.
    storage
        .insert_nodes(&[
            GraphNode {
                node_uid: "nft".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:zzz/target.h:FILE".into(),
                kind: "FILE".into(),
                subtype: None,
                name: "target.h".into(),
                qualified_name: None,
                file_uid: Some("ft".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "nfa".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:aaa/a.cpp:FILE".into(),
                kind: "FILE".into(),
                subtype: None,
                name: "a.cpp".into(),
                qualified_name: None,
                file_uid: Some("fa".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "nfb".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:bbb/b.cpp:FILE".into(),
                kind: "FILE".into(),
                subtype: None,
                name: "b.cpp".into(),
                qualified_name: None,
                file_uid: Some("fb".into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "nma".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:aaa:MODULE".into(),
                kind: "MODULE".into(),
                subtype: None,
                name: "aaa".into(),
                qualified_name: Some("aaa".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
            GraphNode {
                node_uid: "nmb".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                stable_key: "r1:bbb:MODULE".into(),
                kind: "MODULE".into(),
                subtype: None,
                name: "bbb".into(),
                qualified_name: Some("bbb".into()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            },
        ])
        .unwrap();
    storage
        .insert_edges(&[
            // OWNS module → file.
            GraphEdge {
                edge_uid: "eoa".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "nma".into(),
                target_node_uid: "nfa".into(),
                edge_type: "OWNS".into(),
                resolution: "resolved".into(),
                extractor: "t".into(),
                location: None,
                metadata_json: None,
            },
            GraphEdge {
                edge_uid: "eob".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "nmb".into(),
                target_node_uid: "nfb".into(),
                edge_type: "OWNS".into(),
                resolution: "resolved".into(),
                extractor: "t".into(),
                location: None,
                metadata_json: None,
            },
            // IMPORTS: a.cpp and b.cpp both include target.h (FILE → FILE).
            GraphEdge {
                edge_uid: "eia".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "nfa".into(),
                target_node_uid: "nft".into(),
                edge_type: "IMPORTS".into(),
                resolution: "static".into(),
                extractor: "t".into(),
                location: None,
                metadata_json: None,
            },
            GraphEdge {
                edge_uid: "eib".into(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: "r1".into(),
                source_node_uid: "nfb".into(),
                target_node_uid: "nft".into(),
                edge_type: "IMPORTS".into(),
                resolution: "static".into(),
                extractor: "t".into(),
                location: None,
                metadata_json: None,
            },
        ])
        .unwrap();

    let importers = <StorageConnection as AgentStorageRead>::find_file_importers(
        &storage,
        &snapshot_uid,
        "zzz/target.h",
    )
    .unwrap();

    assert_eq!(
        importers.len(),
        2,
        "two files import target.h: {importers:?}"
    );
    // Ordered by path: aaa/a.cpp before bbb/b.cpp, each with its owning module.
    assert_eq!(importers[0].file, "aaa/a.cpp");
    assert_eq!(importers[0].module_path.as_deref(), Some("aaa"));
    assert_eq!(importers[1].file, "bbb/b.cpp");
    assert_eq!(importers[1].module_path.as_deref(), Some("bbb"));
}

#[test]
fn find_file_importers_is_empty_for_a_file_nobody_imports() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");
    seed_file(&mut storage, &snapshot_uid, "ft", "zzz/lonely.h");
    storage
        .insert_nodes(&[GraphNode {
            node_uid: "nft".into(),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: "r1".into(),
            stable_key: "r1:zzz/lonely.h:FILE".into(),
            kind: "FILE".into(),
            subtype: None,
            name: "lonely.h".into(),
            qualified_name: None,
            file_uid: Some("ft".into()),
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        }])
        .unwrap();

    let importers = <StorageConnection as AgentStorageRead>::find_file_importers(
        &storage,
        &snapshot_uid,
        "zzz/lonely.h",
    )
    .unwrap();
    assert!(
        importers.is_empty(),
        "a file nobody imports has no importers"
    );
}

// ── COMPLEXITY-SCOPE-1 (RG-REQ-009-L01): the complexity read carries file flags ──

#[test]
fn query_high_complexity_symbols_carries_the_file_flags_and_keeps_every_row() {
    let (_tmp, mut storage) = open_temp_storage();
    insert_repo(&storage, "r1", "my-repo");
    let snapshot_uid = create_ready_snapshot(&storage, "r1");

    // Three files: production, test, generated.
    storage
        .upsert_files(&[
            TrackedFile {
                file_uid: "f_a".into(),
                repo_uid: "r1".into(),
                path: "src/a.rs".into(),
                language: Some("rust".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f_t".into(),
                repo_uid: "r1".into(),
                path: "tests/t.rs".into(),
                language: Some("rust".into()),
                is_test: true,
                is_generated: false,
                is_excluded: false,
            },
            TrackedFile {
                file_uid: "f_g".into(),
                repo_uid: "r1".into(),
                path: "gen/g.rs".into(),
                language: Some("rust".into()),
                is_test: false,
                is_generated: true,
                is_excluded: false,
            },
        ])
        .unwrap();

    // One SYMBOL node per file, linked via file_uid so the files LEFT JOIN resolves.
    let node = |uid: &str, key: &str, name: &str, file_uid: &str| GraphNode {
        node_uid: uid.into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: "r1".into(),
        stable_key: key.into(),
        kind: "SYMBOL".into(),
        subtype: Some("FUNCTION".into()),
        name: name.into(),
        qualified_name: Some(name.into()),
        file_uid: Some(file_uid.into()),
        parent_node_uid: None,
        location: Some(SourceLocation {
            line_start: 10,
            col_start: 0,
            line_end: 20,
            col_end: 0,
        }),
        signature: None,
        visibility: Some("pub".into()),
        doc_comment: None,
        metadata_json: None,
    };
    storage
        .insert_nodes(&[
            node("n_a", "r1:src/a.rs:fa:SYMBOL", "fa", "f_a"),
            node("n_t", "r1:tests/t.rs:ft:SYMBOL", "ft", "f_t"),
            node("n_g", "r1:gen/g.rs:fg:SYMBOL", "fg", "f_g"),
        ])
        .unwrap();

    // Four measurements at cx 30: one per node, plus one whose target has NO node.
    let meas = |uid: &str, target: &str| MeasurementInput {
        measurement_uid: uid.into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: "r1".into(),
        target_stable_key: target.into(),
        kind: "cyclomatic_complexity".into(),
        value_json: "{\"value\": 30}".into(),
        source: "test".into(),
        created_at: "2026-04-15T00:02:00Z".into(),
    };
    storage
        .insert_measurements(&[
            meas("m_a", "r1:src/a.rs:fa:SYMBOL"),
            meas("m_t", "r1:tests/t.rs:ft:SYMBOL"),
            meas("m_g", "r1:gen/g.rs:fg:SYMBOL"),
            meas("m_nf", "r1:nowhere:missing:SYMBOL"), // fileless (no node)
        ])
        .unwrap();

    // The read returns ALL FOUR rows (no filter in SQL — the certificate's set is unchanged).
    let rows = <StorageConnection as AgentStorageRead>::query_high_complexity_symbols(
        &storage,
        &snapshot_uid,
        20,
        i64::MAX as usize,
    )
    .unwrap();
    assert_eq!(
        rows.len(),
        4,
        "the read keeps every above-threshold row: {rows:?}"
    );

    let by_key = |key: &str| {
        rows.iter()
            .find(|m| m.stable_key == key)
            .unwrap_or_else(|| panic!("row {key} missing: {rows:?}"))
    };
    // Flags per row.
    let a = by_key("r1:src/a.rs:fa:SYMBOL");
    assert!(!a.is_test && !a.is_generated, "production row: 0/0");
    let t = by_key("r1:tests/t.rs:ft:SYMBOL");
    assert!(t.is_test && !t.is_generated, "test row: is_test");
    let g = by_key("r1:gen/g.rs:fg:SYMBOL");
    assert!(!g.is_test && g.is_generated, "generated row: is_generated");
    // The fileless row reads false/false (no persisted fact) and has no file path.
    let nf = by_key("r1:nowhere:missing:SYMBOL");
    assert!(
        !nf.is_test && !nf.is_generated && nf.file_path.is_none(),
        "fileless row: 0/0, no file path: {nf:?}",
    );

    // count_high_complexity_symbols still counts all four (unfiltered).
    let count = <StorageConnection as AgentStorageRead>::count_high_complexity_symbols(
        &storage,
        &snapshot_uid,
        20,
    )
    .unwrap();
    assert_eq!(count, 4, "count is the unfiltered above-threshold total");
}
