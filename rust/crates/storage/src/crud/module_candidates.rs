//! Module candidate CRUD methods (RS-MG-1).
//!
//! Read operations for the `module_candidates` table. The TS indexer
//! is the producer of module candidate facts; Rust `rmap` commands
//! consume them for module graph queries and boundary evaluation.
//!
//! Key operations:
//! - `get_module_candidates_for_snapshot` — read all module candidates
//! - `build_module_index_by_canonical_path` — identity index for evaluation

use std::collections::HashMap;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::ModuleCandidate;

impl StorageConnection {
	/// Read all module candidates for a snapshot.
	///
	/// Returns an empty vector if the snapshot has no discovered modules.
	/// Order is deterministic: sorted by `canonical_root_path` ascending.
	pub fn get_module_candidates_for_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ModuleCandidate>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT module_candidate_uid, snapshot_uid, repo_uid, module_key,
			        module_kind, canonical_root_path, confidence, display_name,
			        metadata_json
			 FROM module_candidates
			 WHERE snapshot_uid = ?
			 ORDER BY canonical_root_path ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], ModuleCandidate::from_row)?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Build an identity index mapping `canonical_root_path` to `module_candidate_uid`.
	///
	/// This is the stable identity anchor for:
	/// - Module edge derivation (file ownership lookup)
	/// - Boundary violation evaluation (stale detection)
	///
	/// Returns an empty map if the snapshot has no discovered modules.
	pub fn build_module_index_by_canonical_path(
		&self,
		snapshot_uid: &str,
	) -> Result<HashMap<String, String>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT canonical_root_path, module_candidate_uid
			 FROM module_candidates
			 WHERE snapshot_uid = ?",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			Ok((
				row.get::<_, String>("canonical_root_path")?,
				row.get::<_, String>("module_candidate_uid")?,
			))
		})?;

		let mut index = HashMap::new();
		for row in rows {
			let (path, uid) = row?;
			index.insert(path, uid);
		}
		Ok(index)
	}

	/// Resolve a module by `canonical_root_path` for a snapshot.
	///
	/// Returns `None` if no module with that path exists in the snapshot.
	/// This is the exact-match resolution required by the boundary
	/// authoring contract (no fuzzy matching).
	pub fn get_module_by_canonical_path(
		&self,
		snapshot_uid: &str,
		canonical_root_path: &str,
	) -> Result<Option<ModuleCandidate>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT module_candidate_uid, snapshot_uid, repo_uid, module_key,
			        module_kind, canonical_root_path, confidence, display_name,
			        metadata_json
			 FROM module_candidates
			 WHERE snapshot_uid = ? AND canonical_root_path = ?",
		)?;

		let mut rows = stmt.query([snapshot_uid, canonical_root_path])?;
		match rows.next()? {
			Some(row) => Ok(Some(ModuleCandidate::from_row(row)?)),
			None => Ok(None),
		}
	}

	/// Resolve a module by `module_key` for a snapshot.
	///
	/// Returns `None` if no module with that key exists in the snapshot.
	/// Used for backwards-compatible resolution (module_key is the
	/// generated identity, canonical_root_path is the stable architectural
	/// locator).
	pub fn get_module_by_key(
		&self,
		snapshot_uid: &str,
		module_key: &str,
	) -> Result<Option<ModuleCandidate>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT module_candidate_uid, snapshot_uid, repo_uid, module_key,
			        module_kind, canonical_root_path, confidence, display_name,
			        metadata_json
			 FROM module_candidates
			 WHERE snapshot_uid = ? AND module_key = ?",
		)?;

		let mut rows = stmt.query([snapshot_uid, module_key])?;
		match rows.next()? {
			Some(row) => Ok(Some(ModuleCandidate::from_row(row)?)),
			None => Ok(None),
		}
	}

	/// Resolve a module by `module_candidate_uid`.
	///
	/// Returns `None` if no module with that UID exists in the database.
	/// Used by surfaces show to look up the owning module when we have
	/// the surface's `module_candidate_uid` FK reference.
	pub fn get_module_by_uid(
		&self,
		module_candidate_uid: &str,
	) -> Result<Option<ModuleCandidate>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT module_candidate_uid, snapshot_uid, repo_uid, module_key,
			        module_kind, canonical_root_path, confidence, display_name,
			        metadata_json
			 FROM module_candidates
			 WHERE module_candidate_uid = ?",
		)?;

		let mut rows = stmt.query([module_candidate_uid])?;
		match rows.next()? {
			Some(row) => Ok(Some(ModuleCandidate::from_row(row)?)),
			None => Ok(None),
		}
	}

	/// Fallback: get MODULE nodes from the nodes table when
	/// `module_candidates` is empty.
	///
	/// The Rust indexer creates MODULE kind nodes in the nodes table
	/// but does not populate the `module_candidates` table. This method
	/// provides a fallback path for `modules list` to work with
	/// Rust-indexed repos.
	///
	/// Returns MODULE nodes converted to `ModuleCandidate` format with:
	/// - `module_candidate_uid` = node_uid
	/// - `module_key` = stable_key
	/// - `canonical_root_path` = qualified_name
	/// - `module_kind` = "directory" (derived from directory structure)
	/// - `display_name` = name
	/// - `confidence` = 1.0
	pub fn get_module_nodes_as_candidates(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ModuleCandidate>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT node_uid, snapshot_uid, repo_uid, stable_key,
			        name, qualified_name
			 FROM nodes
			 WHERE snapshot_uid = ? AND kind = 'MODULE'
			 ORDER BY qualified_name ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			let node_uid: String = row.get("node_uid")?;
			let snapshot_uid: String = row.get("snapshot_uid")?;
			let repo_uid: String = row.get("repo_uid")?;
			let stable_key: String = row.get("stable_key")?;
			let name: String = row.get("name")?;
			let qualified_name: Option<String> = row.get("qualified_name")?;

			Ok(ModuleCandidate {
				module_candidate_uid: node_uid,
				snapshot_uid,
				repo_uid,
				module_key: stable_key,
				module_kind: "directory".to_string(),
				canonical_root_path: qualified_name.unwrap_or_default(),
				confidence: 1.0,
				display_name: Some(name),
				metadata_json: None,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{fresh_storage, make_repo};

	/// Insert a module candidate directly for testing.
	#[allow(clippy::too_many_arguments)]
	fn insert_module_candidate(
		conn: &StorageConnection,
		uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		module_key: &str,
		canonical_root_path: &str,
		module_kind: &str,
		display_name: Option<&str>,
	) {
		conn.connection()
			.execute(
				"INSERT INTO module_candidates
				 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
				  module_kind, canonical_root_path, confidence, display_name, metadata_json)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
				rusqlite::params![
					uid,
					snapshot_uid,
					repo_uid,
					module_key,
					module_kind,
					canonical_root_path,
					1.0,
					display_name,
				],
			)
			.expect("insert module candidate");
	}

	fn setup_test_snapshot(conn: &StorageConnection) -> (String, String) {
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		let snapshot = conn
			.create_snapshot(&crate::types::CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot");

		(repo.repo_uid, snapshot.snapshot_uid)
	}

	#[test]
	fn get_module_candidates_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_module_candidates_for_snapshot(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_module_candidates_returns_all_modules_sorted_by_path() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Insert in non-alphabetical order
		insert_module_candidate(
			&conn,
			"mc-3",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/zebra",
			"packages/zebra",
			"npm_package",
			Some("@test/zebra"),
		);
		insert_module_candidate(
			&conn,
			"mc-1",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/alpha",
			"packages/alpha",
			"npm_package",
			Some("@test/alpha"),
		);
		insert_module_candidate(
			&conn,
			"mc-2",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/beta",
			"packages/beta",
			"npm_package",
			Some("@test/beta"),
		);

		let result = conn
			.get_module_candidates_for_snapshot(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 3);
		// Sorted by canonical_root_path
		assert_eq!(result[0].canonical_root_path, "packages/alpha");
		assert_eq!(result[1].canonical_root_path, "packages/beta");
		assert_eq!(result[2].canonical_root_path, "packages/zebra");
	}

	#[test]
	fn get_module_candidates_includes_all_fields() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-1",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/core",
			"packages/core",
			"npm_package",
			Some("@test/core"),
		);

		let result = conn
			.get_module_candidates_for_snapshot(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 1);
		let module = &result[0];
		assert_eq!(module.module_candidate_uid, "mc-1");
		assert_eq!(module.snapshot_uid, snapshot_uid);
		assert_eq!(module.repo_uid, repo_uid);
		assert_eq!(module.module_key, "npm:@test/core");
		assert_eq!(module.module_kind, "npm_package");
		assert_eq!(module.canonical_root_path, "packages/core");
		assert!((module.confidence - 1.0).abs() < f64::EPSILON);
		assert_eq!(module.display_name, Some("@test/core".to_string()));
		assert!(module.metadata_json.is_none());
	}

	#[test]
	fn build_module_index_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let index = conn
			.build_module_index_by_canonical_path(&snapshot_uid)
			.expect("query");
		assert!(index.is_empty());
	}

	#[test]
	fn build_module_index_maps_path_to_uid() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-app",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/app",
			"packages/app",
			"npm_package",
			None,
		);
		insert_module_candidate(
			&conn,
			"mc-core",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/core",
			"packages/core",
			"npm_package",
			None,
		);

		let index = conn
			.build_module_index_by_canonical_path(&snapshot_uid)
			.expect("query");

		assert_eq!(index.len(), 2);
		assert_eq!(index.get("packages/app"), Some(&"mc-app".to_string()));
		assert_eq!(index.get("packages/core"), Some(&"mc-core".to_string()));
	}

	#[test]
	fn get_module_by_canonical_path_returns_none_for_missing() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_module_by_canonical_path(&snapshot_uid, "packages/nonexistent")
			.expect("query");
		assert!(result.is_none());
	}

	#[test]
	fn get_module_by_canonical_path_returns_exact_match() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-core",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/core",
			"packages/core",
			"npm_package",
			Some("@test/core"),
		);

		let result = conn
			.get_module_by_canonical_path(&snapshot_uid, "packages/core")
			.expect("query");

		assert!(result.is_some());
		let module = result.unwrap();
		assert_eq!(module.module_candidate_uid, "mc-core");
		assert_eq!(module.canonical_root_path, "packages/core");
	}

	#[test]
	fn get_module_by_key_returns_none_for_missing() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_module_by_key(&snapshot_uid, "npm:@test/nonexistent")
			.expect("query");
		assert!(result.is_none());
	}

	#[test]
	fn get_module_by_key_returns_exact_match() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-core",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/core",
			"packages/core",
			"npm_package",
			Some("@test/core"),
		);

		let result = conn
			.get_module_by_key(&snapshot_uid, "npm:@test/core")
			.expect("query");

		assert!(result.is_some());
		let module = result.unwrap();
		assert_eq!(module.module_key, "npm:@test/core");
	}

	#[test]
	fn modules_are_scoped_to_snapshot() {
		let conn = fresh_storage();
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		// Create two snapshots
		let snap1 = conn
			.create_snapshot(&crate::types::CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot 1");

		let snap2 = conn
			.create_snapshot(&crate::types::CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot 2");

		// Insert module in snapshot 1 only
		insert_module_candidate(
			&conn,
			"mc-1",
			&snap1.snapshot_uid,
			&repo.repo_uid,
			"npm:@test/app",
			"packages/app",
			"npm_package",
			None,
		);

		// Snapshot 1 has the module
		let result1 = conn
			.get_module_candidates_for_snapshot(&snap1.snapshot_uid)
			.expect("query");
		assert_eq!(result1.len(), 1);

		// Snapshot 2 does not have the module
		let result2 = conn
			.get_module_candidates_for_snapshot(&snap2.snapshot_uid)
			.expect("query");
		assert!(result2.is_empty());
	}

	#[test]
	fn get_module_by_uid_returns_none_for_missing() {
		let conn = fresh_storage();
		let (_repo_uid, _snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_module_by_uid("nonexistent-uid")
			.expect("query");
		assert!(result.is_none());
	}

	#[test]
	fn get_module_by_uid_returns_exact_match() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-core-unique-uid",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/core",
			"packages/core",
			"npm_package",
			Some("@test/core"),
		);

		let result = conn
			.get_module_by_uid("mc-core-unique-uid")
			.expect("query");

		assert!(result.is_some());
		let module = result.unwrap();
		assert_eq!(module.module_candidate_uid, "mc-core-unique-uid");
		assert_eq!(module.module_key, "npm:@test/core");
		assert_eq!(module.canonical_root_path, "packages/core");
	}

	#[test]
	fn get_module_by_uid_does_not_require_snapshot_uid() {
		// P2-2 fix: surfaces show passes module_candidate_uid to lookup.
		// The method should not require snapshot_uid since UIDs are globally unique.
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(
			&conn,
			"mc-abc123",
			&snapshot_uid,
			&repo_uid,
			"npm:@test/lib",
			"packages/lib",
			"npm_package",
			None,
		);

		// Note: get_module_by_uid does NOT take snapshot_uid parameter.
		// This is correct because module_candidate_uid is a primary key.
		let result = conn.get_module_by_uid("mc-abc123").expect("query");
		assert!(result.is_some());
		assert_eq!(result.unwrap().module_candidate_uid, "mc-abc123");
	}

	// ── Fallback tests for Rust-indexed repos ────────────────────────

	/// Insert a MODULE node directly for fallback testing.
	fn insert_module_node(
		conn: &StorageConnection,
		node_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		stable_key: &str,
		name: &str,
		qualified_name: &str,
	) {
		conn.connection()
			.execute(
				"INSERT INTO nodes
				 (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, qualified_name)
				 VALUES (?, ?, ?, ?, 'MODULE', ?, ?)",
				rusqlite::params![
					node_uid,
					snapshot_uid,
					repo_uid,
					stable_key,
					name,
					qualified_name,
				],
			)
			.expect("insert module node");
	}

	#[test]
	fn get_module_nodes_as_candidates_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_module_nodes_as_candidates(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_module_nodes_as_candidates_returns_module_nodes() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_node(
			&conn,
			"mod-node-1",
			&snapshot_uid,
			&repo_uid,
			"test:src/core:MODULE",
			"core",
			"src/core",
		);
		insert_module_node(
			&conn,
			"mod-node-2",
			&snapshot_uid,
			&repo_uid,
			"test:src/utils:MODULE",
			"utils",
			"src/utils",
		);

		let result = conn
			.get_module_nodes_as_candidates(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 2);
		// Sorted by qualified_name (canonical_root_path)
		assert_eq!(result[0].canonical_root_path, "src/core");
		assert_eq!(result[0].module_candidate_uid, "mod-node-1");
		assert_eq!(result[0].module_key, "test:src/core:MODULE");
		assert_eq!(result[0].module_kind, "directory");
		assert_eq!(result[0].display_name, Some("core".to_string()));
		assert!((result[0].confidence - 1.0).abs() < f64::EPSILON);

		assert_eq!(result[1].canonical_root_path, "src/utils");
		assert_eq!(result[1].module_candidate_uid, "mod-node-2");
	}

	#[test]
	fn get_module_nodes_as_candidates_ignores_non_module_nodes() {
		let conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Insert a MODULE node
		insert_module_node(
			&conn,
			"mod-node",
			&snapshot_uid,
			&repo_uid,
			"test:src:MODULE",
			"src",
			"src",
		);

		// Insert a FILE node (should be ignored)
		conn.connection()
			.execute(
				"INSERT INTO nodes
				 (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, qualified_name)
				 VALUES (?, ?, ?, ?, 'FILE', ?, ?)",
				rusqlite::params![
					"file-node",
					snapshot_uid,
					repo_uid,
					"test:src/main.java:FILE",
					"main.java",
					"src/main.java",
				],
			)
			.expect("insert file node");

		let result = conn
			.get_module_nodes_as_candidates(&snapshot_uid)
			.expect("query");

		// Only the MODULE node should be returned
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].module_candidate_uid, "mod-node");
	}
}
