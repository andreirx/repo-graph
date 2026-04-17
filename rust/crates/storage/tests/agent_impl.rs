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
	CreateSnapshotInput, FileVersion, GraphEdge, GraphNode, Repo, TrackedFile,
	UpdateSnapshotStatusInput,
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

fn create_ready_snapshot(
	storage: &StorageConnection,
	repo_uid: &str,
) -> String {
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
	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");

	let result = <StorageConnection as AgentStorageRead>::get_repo(
		&mut storage,
		"r1",
	)
	.unwrap();
	let repo = result.expect("repo exists");
	assert_eq!(repo.repo_uid, "r1");
	assert_eq!(repo.name, "my-repo");
}

#[test]
fn get_repo_returns_none_when_missing() {
	let (_tmp, mut storage) = open_temp_storage();

	let result = <StorageConnection as AgentStorageRead>::get_repo(
		&mut storage,
		"absent",
	)
	.unwrap();
	assert!(result.is_none());
}

// ── get_latest_snapshot ──────────────────────────────────────────

#[test]
fn get_latest_snapshot_maps_kind_to_scope() {
	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	let result =
		<StorageConnection as AgentStorageRead>::get_latest_snapshot(
			&mut storage,
			"r1",
		)
		.unwrap();
	let snap = result.expect("READY snapshot exists");
	assert_eq!(snap.snapshot_uid, snapshot_uid);
	assert_eq!(snap.repo_uid, "r1");
	// Storage column `kind` surfaces as agent DTO `scope`.
	assert_eq!(snap.scope, "full");
}

#[test]
fn get_latest_snapshot_returns_none_when_no_ready_snapshot() {
	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");
	// Repo exists but no snapshot → Ok(None).

	let result =
		<StorageConnection as AgentStorageRead>::get_latest_snapshot(
			&mut storage,
			"r1",
		)
		.unwrap();
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

	let summary = <StorageConnection as AgentStorageRead>::compute_repo_summary(
		&mut storage,
		&snapshot_uid,
	)
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

	let stale = <StorageConnection as AgentStorageRead>::get_stale_files(
		&mut storage,
		&snapshot_uid,
	)
	.unwrap();
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

	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	let trust =
		<StorageConnection as AgentStorageRead>::get_trust_summary(
			&mut storage,
			"r1",
			&snapshot_uid,
		)
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
		<StorageConnection as AgentStorageRead>::get_trust_summary(
			&mut storage,
			"r1",
			&snapshot_uid,
		)
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
	// ── Expected limit set on an empty snapshot (post Rust-43 F1 fix) ──
	//
	// An empty snapshot has zero entrypoint declarations, so
	// the trust crate's `detect_missing_entrypoint_declarations`
	// fires, which makes `reliability.dead_code.level = LOW`.
	// The agent dead-code aggregator therefore emits
	// `DEAD_CODE_UNRELIABLE` instead of a DEAD_CODE signal.
	//
	// Limits on this fixture (4 total):
	//   1. MODULE_DATA_UNAVAILABLE (always)
	//   2. GATE_NOT_CONFIGURED (no requirement declarations)
	//   3. COMPLEXITY_UNAVAILABLE (always)
	//   4. DEAD_CODE_UNRELIABLE (trust reports Low
	//      dead_code_reliability → reliability gate fires)
	//
	// This is the smoke-test line of defense for F1: if the
	// reliability gate ever regresses, the empty-snapshot
	// path is the simplest reproducer.
	use repo_graph_agent::{orient, Budget, LimitCode, ORIENT_SCHEMA};

	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	let result = orient(
		&mut storage,
		"r1",
		None,
		Budget::Large,
		"2026-04-15T00:00:00Z",
	)
	.unwrap();
	assert_eq!(result.schema, ORIENT_SCHEMA);
	assert_eq!(result.repo, "my-repo");
	assert_eq!(result.snapshot, snapshot_uid);

	assert_eq!(
		result.limits.len(),
		4,
		"empty snapshot must emit MODULE_DATA_UNAVAILABLE + \
		 GATE_NOT_CONFIGURED + COMPLEXITY_UNAVAILABLE + \
		 DEAD_CODE_UNRELIABLE; actual: {:?}",
		result.limits.iter().map(|l| l.code).collect::<Vec<_>>()
	);

	// DEAD_CODE_UNRELIABLE must carry the trust reason
	// verbatim. The exact reason string the trust crate
	// produces for this fixture is
	// "missing_entrypoint_declarations".
	let dc_unreliable = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::DeadCodeUnreliable)
		.expect("DEAD_CODE_UNRELIABLE must fire on empty snapshot");
	assert!(
		dc_unreliable
			.reasons
			.iter()
			.any(|r| r == "missing_entrypoint_declarations"),
		"reasons must include trust's missing_entrypoint_declarations: {:?}",
		dc_unreliable.reasons
	);

	// DEAD_CODE signal must NOT appear.
	let has_dead_code_signal = result
		.signals
		.iter()
		.any(|s| s.code() == repo_graph_agent::SignalCode::DeadCode);
	assert!(
		!has_dead_code_signal,
		"DEAD_CODE signal must be suppressed when reliability is Low"
	);

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
	use repo_graph_agent::{
		AgentReliabilityLevel, EnrichmentState,
	};

	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "my-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	let trust =
		<StorageConnection as AgentStorageRead>::get_trust_summary(
			&mut storage,
			"r1",
			&snapshot_uid,
		)
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
		<StorageConnection as AgentStorageRead>::get_trust_summary(
			&mut storage,
			"r1",
			&snapshot_uid,
		)
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
	let stale = <StorageConnection as AgentStorageRead>::get_stale_files(
		&mut storage,
		&snapshot_uid,
	)
	.unwrap();
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
	let stale_after = <StorageConnection as AgentStorageRead>::get_stale_files(
		&mut storage,
		&snapshot_uid,
	)
	.unwrap();
	assert_eq!(
		stale_after.len(),
		1,
		"stale count must not increase when an 'ok' file is added; \
		 actual stale files: {:?}",
		stale_after.iter().map(|s| &s.path).collect::<Vec<_>>()
	);
	assert_eq!(stale_after[0].path, "src/stale_file.rs");
}
