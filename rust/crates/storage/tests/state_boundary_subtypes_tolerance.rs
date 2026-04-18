//! SB-2-pre-2 tolerance proof (Rust half).
//!
//! Asserts that the Rust storage layer accepts insert + read of
//! nodes whose `subtype` column matches the seven canonical
//! strings added by SB-2-pre-2 for state-boundary resources
//! (CONNECTION, FILE_PATH, DIRECTORY_PATH, LOGICAL, CACHE,
//! BUCKET, CONTAINER) plus reuse of the pre-existing `NAMESPACE`
//! subtype for BLOB context.
//!
//! Scope mirrors SB-2-pre: canonical vocabulary alignment only.
//! No emitter logic, no binding logic, no `state-extractor` code.
//!
//! Pair test: `test/adapters/storage/sb-2-pre-2-node-subtype-tolerance.test.ts`
//! exercises the same vocabulary through the TS storage adapter.
//! Cross-runtime tolerance is transitive via the shared SQL
//! schema and per-runtime non-validating subtype deserialization
//! (confirmed at SB-2-pre).

use repo_graph_storage::types::{CreateSnapshotInput, GraphNode, Repo};
use repo_graph_storage::StorageConnection;

fn open_temp_storage() -> (tempfile::TempDir, StorageConnection) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("sb_2_pre_2_tolerance.db");
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
		node_uid: format!(
			"node-{}",
			stable_key.replace([':', '/', '\\'], "-")
		),
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

// ── Each subtype round-trips through storage ─────────────────────

#[test]
fn storage_accepts_all_new_resource_subtypes() {
	let (_dir, mut storage) = open_temp_storage();
	let repo_uid = make_repo(&storage);
	let snapshot_uid = make_snapshot(&storage, &repo_uid);

	// Cover every (kind, subtype) pair that SB-2-pre-2 canonically
	// authorizes for slice 1. Each node has a unique stable_key
	// so they can coexist in one snapshot.
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
			"myservice:fs:/var/cache:FS_PATH",
			"FS_PATH",
			"DIRECTORY_PATH",
			"/var/cache",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:fs:CACHE_DIR:FS_PATH",
			"FS_PATH",
			"LOGICAL",
			"CACHE_DIR",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:cache:redis:REDIS_URL:STATE",
			"STATE",
			"CACHE",
			"REDIS_URL",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:blob:s3:artifacts-bucket:BLOB",
			"BLOB",
			"BUCKET",
			"artifacts-bucket",
		),
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:blob:azure:logs-container:BLOB",
			"BLOB",
			"CONTAINER",
			"logs-container",
		),
		// Reuse case: NAMESPACE already existed for MODULE; it
		// is reused for BLOB (e.g., GCP Storage namespace) in
		// SB-2-pre-2.
		make_resource_node(
			&repo_uid,
			&snapshot_uid,
			"myservice:blob:gcs:my-ns:BLOB",
			"BLOB",
			"NAMESPACE",
			"my-ns",
		),
	];

	storage
		.insert_nodes(&nodes)
		.expect("insert of all new subtypes must succeed");

	let roundtrip = storage.query_all_nodes(&snapshot_uid).unwrap();
	assert_eq!(roundtrip.len(), 8, "all 8 nodes must round-trip");

	let subtypes: Vec<String> = roundtrip
		.iter()
		.filter_map(|n| n.subtype.clone())
		.collect();

	for expected in [
		"CONNECTION",
		"FILE_PATH",
		"DIRECTORY_PATH",
		"LOGICAL",
		"CACHE",
		"BUCKET",
		"CONTAINER",
		"NAMESPACE",
	] {
		assert!(
			subtypes.iter().any(|s| s == expected),
			"subtype {:?} must round-trip, got {:?}",
			expected,
			subtypes
		);
	}
}

#[test]
fn namespace_subtype_context_is_carried_by_parent_kind_not_subtype() {
	// Insert two nodes both with subtype=NAMESPACE but different
	// kind (MODULE vs BLOB). Both must round-trip with kind and
	// subtype preserved independently. This pins the "flat enum,
	// context-from-kind" design.
	let (_dir, mut storage) = open_temp_storage();
	let repo_uid = make_repo(&storage);
	let snapshot_uid = make_snapshot(&storage, &repo_uid);

	let module_namespace = make_resource_node(
		&repo_uid,
		&snapshot_uid,
		"myservice:src/utils:MODULE",
		"MODULE",
		"NAMESPACE",
		"utils",
	);
	let blob_namespace = make_resource_node(
		&repo_uid,
		&snapshot_uid,
		"myservice:blob:gcs:shared-ns:BLOB",
		"BLOB",
		"NAMESPACE",
		"shared-ns",
	);

	storage
		.insert_nodes(&[module_namespace, blob_namespace])
		.expect("two NAMESPACE-subtype nodes of different kinds must coexist");

	let roundtrip = storage.query_all_nodes(&snapshot_uid).unwrap();
	assert_eq!(roundtrip.len(), 2);

	let module = roundtrip
		.iter()
		.find(|n| n.kind == "MODULE")
		.expect("MODULE node must be present");
	assert_eq!(module.subtype.as_deref(), Some("NAMESPACE"));

	let blob = roundtrip
		.iter()
		.find(|n| n.kind == "BLOB")
		.expect("BLOB node must be present");
	assert_eq!(blob.subtype.as_deref(), Some("NAMESPACE"));
}

// ── NodeSubtype enum variants serialize to contract strings ─────

#[test]
fn new_node_subtype_variants_serialize_to_contract_strings() {
	use repo_graph_indexer::types::NodeSubtype;

	let cases = [
		(NodeSubtype::Connection, "\"CONNECTION\""),
		(NodeSubtype::FilePath, "\"FILE_PATH\""),
		(NodeSubtype::DirectoryPath, "\"DIRECTORY_PATH\""),
		(NodeSubtype::Logical, "\"LOGICAL\""),
		(NodeSubtype::Cache, "\"CACHE\""),
		(NodeSubtype::Bucket, "\"BUCKET\""),
		(NodeSubtype::Container, "\"CONTAINER\""),
	];

	for (variant, expected) in cases {
		let json = serde_json::to_string(&variant).unwrap();
		assert_eq!(
			json, expected,
			"variant {:?} must serialize to {}",
			variant, expected
		);
	}

	// Pre-existing Namespace variant: also serializes to
	// "NAMESPACE" — this is the intentional reuse for BLOB
	// namespace context.
	let namespace_json = serde_json::to_string(&NodeSubtype::Namespace).unwrap();
	assert_eq!(namespace_json, "\"NAMESPACE\"");
}

#[test]
fn existing_node_subtype_variants_unaffected() {
	// Pin a handful of pre-existing variants to confirm
	// SB-2-pre-2 did not accidentally rename or disturb them.
	use repo_graph_indexer::types::NodeSubtype;

	assert_eq!(
		serde_json::to_string(&NodeSubtype::Function).unwrap(),
		"\"FUNCTION\""
	);
	assert_eq!(
		serde_json::to_string(&NodeSubtype::TypeAlias).unwrap(),
		"\"TYPE_ALIAS\""
	);
	assert_eq!(
		serde_json::to_string(&NodeSubtype::TestSuite).unwrap(),
		"\"TEST_SUITE\""
	);
	assert_eq!(
		serde_json::to_string(&NodeSubtype::Directory).unwrap(),
		"\"DIRECTORY\""
	);
}
