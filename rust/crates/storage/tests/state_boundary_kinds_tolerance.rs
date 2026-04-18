//! SB-2-pre tolerance proof (Rust half).
//!
//! Asserts that the Rust storage layer accepts insert + read of
//! nodes whose `kind` column matches the three canonical strings
//! added by SB-2-pre: `"DB_RESOURCE"`, `"FS_PATH"`, `"BLOB"`.
//!
//! This is part of the canonical-vocabulary-alignment slice for
//! the state-boundary program. It does NOT exercise the state-
//! extractor crate (which ships in SB-2) nor the emission pipeline
//! (SB-3). The test's only job is to prove the storage layer
//! tolerates the new vocabulary, so downstream emission slices
//! can rely on persistence behaving correctly.
//!
//! Pair test: `test/interop/sb-2-pre-node-kind-tolerance.test.ts`
//! exercises the same vocabulary through the TS storage adapter.
//! Cross-runtime tolerance is transitive: both adapters share the
//! same SQL schema (migration 001 is a single embedded file
//! consumed by both), and neither validates `nodes.kind` against
//! the typed enum at the SQL boundary. A DB written by one
//! therefore reads correctly through the other for these kinds.

use repo_graph_storage::types::{CreateSnapshotInput, GraphNode, Repo};
use repo_graph_storage::StorageConnection;

fn open_temp_storage() -> (tempfile::TempDir, StorageConnection) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("sb_2_pre_tolerance.db");
	let storage = StorageConnection::open(&db_path).unwrap();
	(dir, storage)
}

fn make_repo(storage: &StorageConnection) -> String {
	let uid = "myservice".to_string();
	storage
		.add_repo(&Repo {
			repo_uid: uid.clone(),
			name: "myservice".to_string(),
			root_path: "/tmp/myservice".to_string(),
			default_branch: None,
			created_at: "2026-04-18T00:00:00Z".to_string(),
			metadata_json: None,
		})
		.unwrap();
	uid
}

fn make_snapshot(storage: &StorageConnection, repo_uid: &str) -> String {
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
	snap.snapshot_uid
}

fn make_resource_node(
	repo_uid: &str,
	snapshot_uid: &str,
	stable_key: &str,
	kind: &str,
	subtype: &str,
	name: &str,
) -> GraphNode {
	GraphNode {
		node_uid: format!("node-{}", stable_key.replace([':', '/'], "-")),
		snapshot_uid: snapshot_uid.to_string(),
		repo_uid: repo_uid.to_string(),
		stable_key: stable_key.to_string(),
		kind: kind.to_string(),
		subtype: Some(subtype.to_string()),
		name: name.to_string(),
		qualified_name: None,
		file_uid: None,
		parent_node_uid: None,
		location: None,
		signature: None,
		visibility: None,
		doc_comment: None,
		metadata_json: None,
	}
}

#[test]
fn storage_accepts_db_resource_fs_path_blob_kinds() {
	let (_dir, mut storage) = open_temp_storage();
	let repo_uid = make_repo(&storage);
	let snapshot_uid = make_snapshot(&storage, &repo_uid);
	// Resource nodes have `file_uid = None` by construction; no
	// file row is needed for this test.

	// Three resource nodes representing the canonical shapes from
	// state-boundary-contract §5.1. stable_key strings follow the
	// contract's format exactly.
	let nodes = vec![
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:db:postgres:DATABASE_URL:DB_RESOURCE",
			"DB_RESOURCE",
			"CONNECTION",
			"DATABASE_URL",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:fs:/etc/app/settings.yaml:FS_PATH",
			"FS_PATH",
			"FILE_PATH",
			"/etc/app/settings.yaml",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:blob:s3:artifacts-bucket:BLOB",
			"BLOB",
			"BUCKET",
			"artifacts-bucket",
		),
	];

	storage.insert_nodes(&nodes).expect("insert must succeed");

	let roundtrip = storage.query_all_nodes(&snapshot_uid).unwrap();
	assert_eq!(
		roundtrip.len(),
		3,
		"expected exactly the 3 resource nodes we inserted"
	);

	let kinds: Vec<&str> = roundtrip.iter().map(|n| n.kind.as_str()).collect();
	assert!(
		kinds.contains(&"DB_RESOURCE"),
		"DB_RESOURCE kind must round-trip, got {:?}",
		kinds
	);
	assert!(
		kinds.contains(&"FS_PATH"),
		"FS_PATH kind must round-trip, got {:?}",
		kinds
	);
	assert!(
		kinds.contains(&"BLOB"),
		"BLOB kind must round-trip, got {:?}",
		kinds
	);
}

#[test]
fn stable_key_shape_is_preserved_byte_for_byte() {
	// The FS stable-key shape includes `:` inside the path payload
	// (contract §5.1 parsing-semantics note). Round-trip must
	// preserve the payload exactly even though the key contains
	// multiple colons.
	let (_dir, mut storage) = open_temp_storage();
	let repo_uid = make_repo(&storage);
	let snapshot_uid = make_snapshot(&storage, &repo_uid);

	let windows_key = "myservice:fs:C:\\Windows\\path:FS_PATH";
	let uri_key = "myservice:fs:file:///etc/config:FS_PATH";

	storage
		.insert_nodes(&[
			make_resource_node(
				&repo_uid,
				&snapshot_uid,
				windows_key,
				"FS_PATH",
				"FILE_PATH",
				"C:\\Windows\\path",
			),
			make_resource_node(
				&repo_uid,
				&snapshot_uid,
				uri_key,
				"FS_PATH",
				"FILE_PATH",
				"file:///etc/config",
			),
		])
		.expect("insert with colon-bearing fs payloads must succeed");

	let roundtrip = storage.query_all_nodes(&snapshot_uid).unwrap();
	let keys: Vec<&str> = roundtrip.iter().map(|n| n.stable_key.as_str()).collect();
	assert!(
		keys.contains(&windows_key),
		"Windows-style FS stable key must round-trip byte-for-byte"
	);
	assert!(
		keys.contains(&uri_key),
		"URI-style FS stable key must round-trip byte-for-byte"
	);
}

#[test]
fn node_kind_enum_variants_serialize_to_contract_strings() {
	// Independent of storage: verify the indexer-crate enum
	// variants produce the exact strings the contract pins.
	// Any future rename that drifts the serialized form is caught
	// here.
	use repo_graph_indexer::types::NodeKind;

	let db = serde_json::to_string(&NodeKind::DbResource).unwrap();
	assert_eq!(db, "\"DB_RESOURCE\"");

	let fs = serde_json::to_string(&NodeKind::FsPath).unwrap();
	assert_eq!(fs, "\"FS_PATH\"");

	let blob = serde_json::to_string(&NodeKind::Blob).unwrap();
	assert_eq!(blob, "\"BLOB\"");
}
