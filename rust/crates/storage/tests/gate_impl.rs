//! Integration tests for the `GateStorageRead` adapter impl.
//!
//! Proves that `StorageConnection` correctly implements the
//! `GateStorageRead` trait defined by the gate crate. These
//! tests live on the storage side (not the gate side) because
//! they exercise SQLite through the real adapter; the gate's
//! own test suite uses an in-module fake to avoid this
//! dependency direction.
//!
//! Coverage intent (Rust-43A tech-debt closure):
//!   - DTO mapping: storage row shapes -> gate-owned DTOs
//!   - Requirements: projection with obligation sub-list
//!   - Waivers: projection with all optional fields
//!   - Boundary declarations: module path + forbids + reason
//!   - Import edges: IMPORTS between file-path prefixes
//!   - Coverage measurements: line_coverage kind filter
//!   - Hotspot inferences: hotspot_score kind filter
//!   - Storage error mapping: rusqlite errors -> GateStorageError

use repo_graph_gate::GateStorageRead;
use repo_graph_storage::types::{
	CreateSnapshotInput, GraphEdge, GraphNode, Repo, TrackedFile,
	UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;

// ── Helpers ──────────────────────────────────────────────────────

fn open_temp_storage() -> (tempfile::TempDir, StorageConnection) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("gate_impl_test.db");
	let storage = StorageConnection::open(&db_path).unwrap();
	(dir, storage)
}

fn open_temp_storage_with_path() -> (tempfile::TempDir, std::path::PathBuf, StorageConnection) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("gate_impl_test.db");
	let storage = StorageConnection::open(&db_path).unwrap();
	let path = db_path.clone();
	(dir, path, storage)
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

fn insert_requirement_declaration(
	storage: &StorageConnection,
	repo_uid: &str,
	req_id: &str,
	version: i64,
) {
	use repo_graph_storage::crud::declarations::{
		requirement_identity_key, DeclarationInsert,
	};
	let target_key = format!("{}:requirement:{}:{}", repo_uid, req_id, version);
	let value = serde_json::json!({
		"req_id": req_id,
		"version": version,
		"verification": [{
			"obligation_id": "obl-1",
			"obligation": "No boundary violations",
			"method": "arch_violations",
			"target": "src/core",
			"threshold": 0.0,
			"operator": "lte",
		}],
	});
	storage
		.insert_declaration(&DeclarationInsert {
			identity_key: requirement_identity_key(repo_uid, req_id, version),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "requirement".to_string(),
			value_json: value.to_string(),
			created_at: "2026-04-15T00:00:00Z".to_string(),
			created_by: Some("test".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		})
		.unwrap();
}

fn insert_waiver_declaration(
	storage: &StorageConnection,
	repo_uid: &str,
	req_id: &str,
	requirement_version: i64,
	obligation_id: &str,
) {
	use repo_graph_storage::crud::declarations::{
		waiver_identity_key, DeclarationInsert,
	};
	let target_key = format!("{}:waiver:{}#{}", repo_uid, req_id, obligation_id);
	let value = serde_json::json!({
		"req_id": req_id,
		"requirement_version": requirement_version,
		"obligation_id": obligation_id,
		"reason": "accepted risk for test",
		"created_at": "2026-04-14T00:00:00Z",
		"created_by": "test-author",
		"expires_at": "2027-01-01T00:00:00Z",
		"rationale_category": "risk_accepted",
		"policy_basis": "POLICY-42",
	});
	storage
		.insert_declaration(&DeclarationInsert {
			identity_key: waiver_identity_key(
				repo_uid,
				req_id,
				requirement_version,
				obligation_id,
			),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "waiver".to_string(),
			value_json: value.to_string(),
			created_at: "2026-04-14T00:00:00Z".to_string(),
			created_by: Some("test-author".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		})
		.unwrap();
}

fn insert_boundary_declaration(
	storage: &StorageConnection,
	repo_uid: &str,
	module_path: &str,
	forbids: &str,
	reason: Option<&str>,
) {
	use repo_graph_storage::crud::declarations::{
		boundary_identity_key, DeclarationInsert,
	};
	let target_key = format!("{}:{}:MODULE", repo_uid, module_path);
	let mut value = serde_json::json!({ "forbids": forbids });
	if let Some(r) = reason {
		value["reason"] = serde_json::Value::String(r.to_string());
	}
	storage
		.insert_declaration(&DeclarationInsert {
			identity_key: boundary_identity_key(repo_uid, module_path, forbids),
			repo_uid: repo_uid.to_string(),
			target_stable_key: target_key,
			kind: "boundary".to_string(),
			value_json: value.to_string(),
			created_at: "2026-04-15T00:00:00Z".to_string(),
			created_by: Some("test".to_string()),
			supersedes_uid: None,
			authored_basis_json: None,
		})
		.unwrap();
}

// ── 1. Requirements query projection ─────────────────────────────

#[test]
fn requirements_projection_maps_to_gate_dtos() {
	let (_tmp, storage) = open_temp_storage();
	insert_repo(&storage, "r1", "test-repo");
	insert_requirement_declaration(&storage, "r1", "REQ-1", 2);

	let reqs = <StorageConnection as GateStorageRead>::get_active_requirements(
		&storage, "r1",
	)
	.unwrap();

	assert_eq!(reqs.len(), 1);
	let req = &reqs[0];
	assert_eq!(req.req_id, "REQ-1");
	assert_eq!(req.version, 2);
	assert_eq!(req.obligations.len(), 1);

	let obl = &req.obligations[0];
	assert_eq!(obl.obligation_id, "obl-1");
	assert_eq!(obl.obligation, "No boundary violations");
	assert_eq!(obl.method, "arch_violations");
	assert_eq!(obl.target.as_deref(), Some("src/core"));
	assert_eq!(obl.threshold, Some(0.0));
	assert_eq!(obl.operator.as_deref(), Some("lte"));
}

// ── 2. Waivers query projection ─────────────────────────────────

#[test]
fn waivers_projection_maps_all_fields_including_optionals() {
	let (_tmp, storage) = open_temp_storage();
	insert_repo(&storage, "r1", "test-repo");
	insert_waiver_declaration(&storage, "r1", "REQ-1", 1, "obl-1");

	// Query with `now` before the waiver's expires_at so it is
	// not filtered out by the expiry check.
	let waivers = <StorageConnection as GateStorageRead>::find_waivers(
		&storage,
		"r1",
		"REQ-1",
		1,
		"obl-1",
		"2026-04-16T00:00:00Z",
	)
	.unwrap();

	assert_eq!(waivers.len(), 1);
	let w = &waivers[0];
	assert!(!w.waiver_uid.is_empty(), "waiver_uid must be populated");
	assert_eq!(w.reason, "accepted risk for test");
	assert_eq!(w.created_at, "2026-04-14T00:00:00Z");
	assert_eq!(w.created_by.as_deref(), Some("test-author"));
	assert_eq!(w.expires_at.as_deref(), Some("2027-01-01T00:00:00Z"));
	assert_eq!(w.rationale_category.as_deref(), Some("risk_accepted"));
	assert_eq!(w.policy_basis.as_deref(), Some("POLICY-42"));
}

// ── 3. Boundary declarations projection ─────────────────────────

#[test]
fn boundary_declarations_projection_maps_module_forbids_reason() {
	let (_tmp, storage) = open_temp_storage();
	insert_repo(&storage, "r1", "test-repo");
	insert_boundary_declaration(
		&storage,
		"r1",
		"src/core",
		"src/adapters",
		Some("clean architecture"),
	);

	let boundaries =
		<StorageConnection as GateStorageRead>::get_boundary_declarations(
			&storage, "r1",
		)
		.unwrap();

	assert_eq!(boundaries.len(), 1);
	let b = &boundaries[0];
	assert_eq!(b.boundary_module, "src/core");
	assert_eq!(b.forbids, "src/adapters");
	assert_eq!(b.reason.as_deref(), Some("clean architecture"));
}

// ── 4. Imports-between-paths projection ─────────────────────────

#[test]
fn find_boundary_imports_returns_gate_import_edges() {
	let (_tmp, mut storage) = open_temp_storage();
	insert_repo(&storage, "r1", "test-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	// Seed files.
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
				path: "src/adapters/storage.ts".into(),
				language: Some("typescript".into()),
				is_test: false,
				is_generated: false,
				is_excluded: false,
			},
		])
		.unwrap();

	// Seed FILE nodes that reference the tracked files.
	storage
		.insert_nodes(&[
			GraphNode {
				node_uid: "n1".into(),
				snapshot_uid: snapshot_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/core/model.ts:FILE".into(),
				kind: "FILE".into(),
				subtype: None,
				name: "model.ts".into(),
				qualified_name: Some("src/core/model.ts".into()),
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
				stable_key: "r1:src/adapters/storage.ts:FILE".into(),
				kind: "FILE".into(),
				subtype: None,
				name: "storage.ts".into(),
				qualified_name: Some("src/adapters/storage.ts".into()),
				file_uid: Some("f2".into()),
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			},
		])
		.unwrap();

	// Seed an IMPORTS edge from core -> adapters (a violation).
	storage
		.insert_edges(&[GraphEdge {
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
		}])
		.unwrap();

	let imports =
		<StorageConnection as GateStorageRead>::find_boundary_imports(
			&storage,
			&snapshot_uid,
			"src/core",
			"src/adapters",
		)
		.unwrap();

	assert_eq!(imports.len(), 1);
	let edge = &imports[0];
	assert_eq!(edge.source_file, "src/core/model.ts");
	assert_eq!(edge.target_file, "src/adapters/storage.ts");
}

// ── 5. Coverage measurements projection ─────────────────────────

#[test]
fn coverage_measurements_projection_maps_to_gate_dtos() {
	let (_tmp, _path, storage) = open_temp_storage_with_path();
	insert_repo(&storage, "r1", "test-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	// Insert a measurement row via the parallel raw connection.
	// The storage crate may not have a public insert_measurement
	// method, so we use the db_path to open a second connection.
	let db_path = _path;
	{
		let raw = rusqlite::Connection::open(&db_path).unwrap();
		raw.execute(
			"INSERT INTO measurements \
			 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, \
			  kind, value_json, source, created_at) \
			 VALUES (?, ?, 'r1', 'r1:src/core/model.ts:FILE', \
			  'line_coverage', '{\"value\":85.5}', 'istanbul', '2026-04-15T00:00:00Z')",
			rusqlite::params!["m1", &snapshot_uid],
		)
		.unwrap();
	}

	let measurements =
		<StorageConnection as GateStorageRead>::get_coverage_measurements(
			&storage, &snapshot_uid,
		)
		.unwrap();

	assert_eq!(measurements.len(), 1);
	let m = &measurements[0];
	assert_eq!(m.target_stable_key, "r1:src/core/model.ts:FILE");
	assert_eq!(m.value_json, "{\"value\":85.5}");
}

// ── 6. Hotspot inferences projection ────────────────────────────

#[test]
fn hotspot_inferences_projection_maps_to_gate_dtos() {
	let (_tmp, path, storage) = open_temp_storage_with_path();
	insert_repo(&storage, "r1", "test-repo");
	let snapshot_uid = create_ready_snapshot(&storage, "r1");

	// Insert an inference row via a parallel raw connection.
	{
		let raw = rusqlite::Connection::open(&path).unwrap();
		raw.execute(
			"INSERT INTO inferences \
			 (inference_uid, snapshot_uid, repo_uid, target_stable_key, \
			  kind, value_json, confidence, basis_json, extractor, created_at) \
			 VALUES (?, ?, 'r1', 'r1:src/core/model.ts:FILE', \
			  'hotspot_score', '{\"normalized_score\":0.9}', 0.8, \
			  '{\"churn\":10,\"complexity\":5}', 'hotspot:1', '2026-04-15T00:00:00Z')",
			rusqlite::params!["i1", &snapshot_uid],
		)
		.unwrap();
	}

	let inferences =
		<StorageConnection as GateStorageRead>::get_hotspot_inferences(
			&storage, &snapshot_uid,
		)
		.unwrap();

	assert_eq!(inferences.len(), 1);
	let inf = &inferences[0];
	assert_eq!(inf.target_stable_key, "r1:src/core/model.ts:FILE");
	assert_eq!(inf.value_json, "{\"normalized_score\":0.9}");
}

// ── 7. Storage error mapping ────────────────────────────────────

#[test]
fn storage_error_maps_to_gate_storage_error() {
	let (_tmp, path, storage) = open_temp_storage_with_path();
	insert_repo(&storage, "r1", "test-repo");

	// Drop the declarations table via a parallel connection to
	// corrupt the schema. The StorageConnection's internal
	// connection() is pub(crate), so we use a raw rusqlite handle.
	{
		let raw = rusqlite::Connection::open(&path).unwrap();
		raw.execute_batch("DROP TABLE declarations").unwrap();
	}

	// Any GateStorageRead method that reads declarations should
	// fail with a GateStorageError.
	let result =
		<StorageConnection as GateStorageRead>::get_active_requirements(
			&storage, "r1",
		);

	assert!(result.is_err(), "must fail after table is dropped");
	let err = result.unwrap_err();
	assert_eq!(
		err.operation, "get_active_requirements",
		"operation field must match the adapter method"
	);
	assert!(
		!err.message.is_empty(),
		"message must contain the storage error diagnostic"
	);
}
