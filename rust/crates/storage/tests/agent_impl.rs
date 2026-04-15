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
	CreateSnapshotInput, FileVersion, Repo, TrackedFile, UpdateSnapshotStatusInput,
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
	use repo_graph_agent::{orient, Budget, ORIENT_SCHEMA};

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
	// Static limits always present.
	assert_eq!(result.limits.len(), 3);
	// At minimum MODULE_SUMMARY + SNAPSHOT_INFO fire.
	assert!(result.signals.len() >= 2);
}
