//! Module edge derivation support — raw fact loading (RS-MG-2).
//!
//! Provides minimal CRUD methods to load the raw facts needed for
//! module edge derivation:
//! - Resolved import edges (source file → target file)
//! - File ownership (file → module)
//!
//! The derivation itself is pure policy and lives in the classification
//! crate. This module only loads the raw facts with minimal DTOs.

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// A resolved import edge between two files.
///
/// Minimal DTO for module edge derivation. Contains only the file UIDs
/// needed to determine cross-module edges via ownership lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImportWithFiles {
	pub source_file_uid: String,
	pub target_file_uid: String,
}

/// A file ownership assignment.
///
/// Minimal DTO mapping a file to its owning module candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOwnership {
	pub file_uid: String,
	pub module_candidate_uid: String,
}

impl StorageConnection {
	/// Read all resolved import edges for a snapshot.
	///
	/// Returns only IMPORTS edges with resolution = 'static' (resolved).
	/// Order is deterministic: sorted by (source_file_uid, target_file_uid).
	///
	/// The target_node_uid is resolved to its file_uid via the nodes table.
	/// Edges where either source or target node has no file_uid are excluded.
	pub fn get_resolved_imports_for_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ResolvedImportWithFiles>, StorageError> {
		let conn = self.connection();
		// No DISTINCT: derivation needs raw import multiplicity for import_count.
		// Distinct-source-file counting happens in the pure derivation via HashSet.
		let mut stmt = conn.prepare(
			"SELECT src_node.file_uid AS source_file_uid,
			        tgt_node.file_uid AS target_file_uid
			 FROM edges e
			 JOIN nodes src_node ON e.source_node_uid = src_node.node_uid
			 JOIN nodes tgt_node ON e.target_node_uid = tgt_node.node_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'IMPORTS'
			   AND e.resolution = 'static'
			   AND src_node.file_uid IS NOT NULL
			   AND tgt_node.file_uid IS NOT NULL
			 ORDER BY source_file_uid ASC, target_file_uid ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			Ok(ResolvedImportWithFiles {
				source_file_uid: row.get("source_file_uid")?,
				target_file_uid: row.get("target_file_uid")?,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Read all file ownership assignments for a snapshot.
	///
	/// Returns one row per (file, module) assignment from the
	/// module_file_ownership table. Order is deterministic: sorted by
	/// (file_uid, module_candidate_uid).
	///
	/// The derivation layer is responsible for detecting duplicate
	/// ownership (multiple modules claiming the same file) and handling
	/// it as an error condition.
	pub fn get_file_ownership_for_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<FileOwnership>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT file_uid, module_candidate_uid
			 FROM module_file_ownership
			 WHERE snapshot_uid = ?
			 ORDER BY file_uid ASC, module_candidate_uid ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			Ok(FileOwnership {
				file_uid: row.get("file_uid")?,
				module_candidate_uid: row.get("module_candidate_uid")?,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Fallback: derive file ownership from OWNS edges.
	///
	/// The Rust indexer creates OWNS edges (MODULE → FILE) in the edges
	/// table but does not populate `module_file_ownership`. This method
	/// provides a fallback path for rollup computation.
	///
	/// Returns ownership facts derived from OWNS edges where:
	/// - source_node is kind = 'MODULE'
	/// - target_node is kind = 'FILE'
	pub fn get_file_ownership_from_owns_edges(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<FileOwnership>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT f.file_uid, m.node_uid AS module_uid
			 FROM edges e
			 JOIN nodes m ON e.source_node_uid = m.node_uid
			 JOIN nodes f ON e.target_node_uid = f.node_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'OWNS'
			   AND m.kind = 'MODULE'
			   AND f.kind = 'FILE'
			 ORDER BY f.file_uid ASC, m.node_uid ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			Ok(FileOwnership {
				file_uid: row.get("file_uid")?,
				module_candidate_uid: row.get("module_uid")?,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Fallback: get owned files for rollup from OWNS edges.
	///
	/// Similar to `get_owned_files_for_rollup` but queries OWNS edges
	/// instead of `module_file_ownership` table.
	///
	/// Derives file path from FILE node's qualified_name and is_test
	/// from the files table via file_uid.
	pub fn get_owned_files_from_owns_edges(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<OwnedFileForRollup>, StorageError> {
		let conn = self.connection();
		// Join FILE node to files table via file_uid to get is_test.
		// FILE node's qualified_name is the repo-relative path.
		// FILE node's file_uid is the file identity (for modules files command).
		let mut stmt = conn.prepare(
			"SELECT f.file_uid, f.qualified_name AS path, m.node_uid AS module_uid, fi.is_test
			 FROM edges e
			 JOIN nodes m ON e.source_node_uid = m.node_uid
			 JOIN nodes f ON e.target_node_uid = f.node_uid
			 LEFT JOIN files fi ON f.file_uid = fi.file_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'OWNS'
			   AND m.kind = 'MODULE'
			   AND f.kind = 'FILE'
			   AND f.qualified_name IS NOT NULL
			 ORDER BY f.qualified_name ASC, m.node_uid ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			let is_test_int: Option<i64> = row.get("is_test").ok();
			Ok(OwnedFileForRollup {
				file_uid: row.get("file_uid")?,
				file_path: row.get("path")?,
				module_candidate_uid: row.get("module_uid")?,
				is_test: is_test_int.unwrap_or(0) == 1,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Read files owned by a specific module candidate.
	///
	/// Joins module_file_ownership with files to return ownership
	/// details plus file metadata. Order is deterministic: sorted by
	/// file path.
	///
	/// Returns empty Vec if the module has no owned files.
	pub fn get_files_for_module(
		&self,
		snapshot_uid: &str,
		module_candidate_uid: &str,
	) -> Result<Vec<ModuleFileEntry>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT f.file_uid, f.path, f.language,
			        o.assignment_kind, o.confidence
			 FROM module_file_ownership o
			 JOIN files f ON o.file_uid = f.file_uid
			 WHERE o.snapshot_uid = ?
			   AND o.module_candidate_uid = ?
			 ORDER BY f.path ASC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, module_candidate_uid],
			|row| {
				Ok(ModuleFileEntry {
					file_uid: row.get("file_uid")?,
					path: row.get("path")?,
					language: row.get("language")?,
					assignment_kind: row.get("assignment_kind")?,
					confidence: row.get("confidence")?,
				})
			},
		)?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}

	/// Read all owned files for a snapshot with is_test flag.
	///
	/// Joins module_file_ownership with files to return the fields
	/// needed for rollup computation: file_uid, file_path, module_candidate_uid, is_test.
	///
	/// Order is deterministic: sorted by (file_path, module_candidate_uid).
	pub fn get_owned_files_for_rollup(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<OwnedFileForRollup>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT f.file_uid, f.path, o.module_candidate_uid, f.is_test
			 FROM module_file_ownership o
			 JOIN files f ON o.file_uid = f.file_uid
			 WHERE o.snapshot_uid = ?
			 ORDER BY f.path ASC, o.module_candidate_uid ASC",
		)?;

		let rows = stmt.query_map([snapshot_uid], |row| {
			let is_test_int: i64 = row.get("is_test")?;
			Ok(OwnedFileForRollup {
				file_uid: row.get("file_uid")?,
				file_path: row.get("path")?,
				module_candidate_uid: row.get("module_candidate_uid")?,
				is_test: is_test_int == 1,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			results.push(row?);
		}
		Ok(results)
	}
}

/// A file ownership fact for rollup computation.
///
/// Minimal DTO combining file identity (path, is_test, file_uid) with module
/// ownership. Used by `get_owned_files_for_rollup` and fallback queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFileForRollup {
	pub file_uid: String,
	pub file_path: String,
	pub module_candidate_uid: String,
	pub is_test: bool,
}

/// A file owned by a module with ownership metadata.
///
/// Combines data from the `files` table (path, language) with
/// ownership metadata from `module_file_ownership` (assignment_kind, confidence).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleFileEntry {
	pub file_uid: String,
	pub path: String,
	pub language: Option<String>,
	pub assignment_kind: String,
	pub confidence: f64,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{fresh_storage, make_edge, make_file, make_node, make_repo};
	use crate::types::CreateSnapshotInput;

	fn setup_test_snapshot(conn: &StorageConnection) -> (String, String) {
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		let snapshot = conn
			.create_snapshot(&CreateSnapshotInput {
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

	fn insert_file_ownership(
		conn: &StorageConnection,
		snapshot_uid: &str,
		repo_uid: &str,
		file_uid: &str,
		module_candidate_uid: &str,
	) {
		conn.connection()
			.execute(
				"INSERT INTO module_file_ownership
				 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
				  assignment_kind, confidence, basis_json)
				 VALUES (?, ?, ?, ?, 'manifest', 1.0, NULL)",
				rusqlite::params![snapshot_uid, repo_uid, file_uid, module_candidate_uid],
			)
			.expect("insert file ownership");
	}

	fn insert_module_candidate(
		conn: &StorageConnection,
		uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		canonical_root_path: &str,
	) {
		conn.connection()
			.execute(
				"INSERT INTO module_candidates
				 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
				  module_kind, canonical_root_path, confidence, display_name, metadata_json)
				 VALUES (?, ?, ?, ?, 'npm_package', ?, 1.0, NULL, NULL)",
				rusqlite::params![
					uid,
					snapshot_uid,
					repo_uid,
					format!("npm:{}", uid),
					canonical_root_path
				],
			)
			.expect("insert module candidate");
	}

	// ── Resolved imports tests ─────────────────────────────────────

	#[test]
	fn get_resolved_imports_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_resolved_imports_for_snapshot(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_resolved_imports_returns_only_resolved_imports() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create files
		let file_a = make_file(&repo_uid, "src/a.ts");
		let file_b = make_file(&repo_uid, "src/b.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		// Create nodes with file_uid
		let node_a = make_node(
			"node-a",
			&snapshot_uid,
			&repo_uid,
			"key-a",
			&file_a.file_uid,
			"a",
		);
		let node_b = make_node(
			"node-b",
			&snapshot_uid,
			&repo_uid,
			"key-b",
			&file_b.file_uid,
			"b",
		);
		conn.insert_nodes(&[node_a, node_b]).expect("insert nodes");

		// Create resolved IMPORTS edge
		let mut edge = make_edge("edge-1", &snapshot_uid, &repo_uid, "node-a", "node-b");
		edge.edge_type = "IMPORTS".to_string();
		edge.resolution = "static".to_string();
		conn.insert_edges(&[edge]).expect("insert edge");

		let result = conn
			.get_resolved_imports_for_snapshot(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 1);
		assert_eq!(result[0].source_file_uid, file_a.file_uid);
		assert_eq!(result[0].target_file_uid, file_b.file_uid);
	}

	#[test]
	fn get_resolved_imports_excludes_unresolved_edges() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create files and nodes
		let file_a = make_file(&repo_uid, "src/a.ts");
		let file_b = make_file(&repo_uid, "src/b.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		let node_a = make_node(
			"node-a",
			&snapshot_uid,
			&repo_uid,
			"key-a",
			&file_a.file_uid,
			"a",
		);
		let node_b = make_node(
			"node-b",
			&snapshot_uid,
			&repo_uid,
			"key-b",
			&file_b.file_uid,
			"b",
		);
		conn.insert_nodes(&[node_a, node_b]).expect("insert nodes");

		// Create UNRESOLVED IMPORTS edge (resolution != 'static')
		let mut edge = make_edge("edge-1", &snapshot_uid, &repo_uid, "node-a", "node-b");
		edge.edge_type = "IMPORTS".to_string();
		edge.resolution = "unresolved".to_string();
		conn.insert_edges(&[edge]).expect("insert edge");

		let result = conn
			.get_resolved_imports_for_snapshot(&snapshot_uid)
			.expect("query");

		// Unresolved edge should not be included
		assert!(result.is_empty());
	}

	#[test]
	fn get_resolved_imports_excludes_non_import_edges() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create files and nodes
		let file_a = make_file(&repo_uid, "src/a.ts");
		let file_b = make_file(&repo_uid, "src/b.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		let node_a = make_node(
			"node-a",
			&snapshot_uid,
			&repo_uid,
			"key-a",
			&file_a.file_uid,
			"a",
		);
		let node_b = make_node(
			"node-b",
			&snapshot_uid,
			&repo_uid,
			"key-b",
			&file_b.file_uid,
			"b",
		);
		conn.insert_nodes(&[node_a, node_b]).expect("insert nodes");

		// Create CALLS edge (not IMPORTS)
		let mut edge = make_edge("edge-1", &snapshot_uid, &repo_uid, "node-a", "node-b");
		edge.edge_type = "CALLS".to_string();
		edge.resolution = "static".to_string();
		conn.insert_edges(&[edge]).expect("insert edge");

		let result = conn
			.get_resolved_imports_for_snapshot(&snapshot_uid)
			.expect("query");

		// CALLS edge should not be included
		assert!(result.is_empty());
	}

	#[test]
	fn get_resolved_imports_preserves_multiplicity() {
		// Storage returns raw import edges without deduplication.
		// Derivation layer handles distinct-file counting via HashSet.
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create files and multiple nodes per file
		let file_a = make_file(&repo_uid, "src/a.ts");
		let file_b = make_file(&repo_uid, "src/b.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		let node_a1 = make_node(
			"node-a1",
			&snapshot_uid,
			&repo_uid,
			"key-a1",
			&file_a.file_uid,
			"fn1",
		);
		let node_a2 = make_node(
			"node-a2",
			&snapshot_uid,
			&repo_uid,
			"key-a2",
			&file_a.file_uid,
			"fn2",
		);
		let node_b = make_node(
			"node-b",
			&snapshot_uid,
			&repo_uid,
			"key-b",
			&file_b.file_uid,
			"b",
		);
		conn.insert_nodes(&[node_a1, node_a2, node_b])
			.expect("insert nodes");

		// Two IMPORTS edges from different nodes in file_a to file_b
		let mut edge1 = make_edge("edge-1", &snapshot_uid, &repo_uid, "node-a1", "node-b");
		edge1.edge_type = "IMPORTS".to_string();
		edge1.resolution = "static".to_string();

		let mut edge2 = make_edge("edge-2", &snapshot_uid, &repo_uid, "node-a2", "node-b");
		edge2.edge_type = "IMPORTS".to_string();
		edge2.resolution = "static".to_string();

		conn.insert_edges(&[edge1, edge2]).expect("insert edges");

		let result = conn
			.get_resolved_imports_for_snapshot(&snapshot_uid)
			.expect("query");

		// Both edges returned — raw multiplicity preserved for import_count
		assert_eq!(result.len(), 2);
		// Both point to the same file pair
		assert!(result.iter().all(|r| r.source_file_uid == file_a.file_uid));
		assert!(result.iter().all(|r| r.target_file_uid == file_b.file_uid));
	}

	// ── File ownership tests ───────────────────────────────────────

	#[test]
	fn get_file_ownership_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_file_ownership_for_snapshot(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_file_ownership_returns_all_assignments() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create module candidates
		insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
		insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

		// Create files
		let file_a = make_file(&repo_uid, "packages/app/index.ts");
		let file_b = make_file(&repo_uid, "packages/core/lib.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		// Create ownership
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_a.file_uid, "mc-app");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_b.file_uid, "mc-core");

		let result = conn
			.get_file_ownership_for_snapshot(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 2);
		// Sorted by file_uid
		let file_a_ownership = result
			.iter()
			.find(|o| o.file_uid == file_a.file_uid)
			.expect("find file_a");
		assert_eq!(file_a_ownership.module_candidate_uid, "mc-app");
	}

	#[test]
	fn get_file_ownership_returns_duplicate_assignments() {
		// The CRUD method should return ALL assignments, including duplicates.
		// The derivation layer is responsible for detecting and handling them.
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create two module candidates
		insert_module_candidate(&conn, "mc-1", &snapshot_uid, &repo_uid, "packages/mod1");
		insert_module_candidate(&conn, "mc-2", &snapshot_uid, &repo_uid, "packages/mod2");

		// Create one file
		let file = make_file(&repo_uid, "shared/utils.ts");
		conn.upsert_files(std::slice::from_ref(&file)).expect("upsert file");

		// Assign the same file to BOTH modules (duplicate ownership)
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-1");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-2");

		let result = conn
			.get_file_ownership_for_snapshot(&snapshot_uid)
			.expect("query");

		// Should return both assignments — derivation layer handles the error
		assert_eq!(result.len(), 2);
	}

	#[test]
	fn get_file_ownership_is_scoped_to_snapshot() {
		let mut conn = fresh_storage();
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		// Create two snapshots
		let snap1 = conn
			.create_snapshot(&CreateSnapshotInput {
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
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot 2");

		// Set up module and file in snapshot 1 only
		insert_module_candidate(&conn, "mc-1", &snap1.snapshot_uid, &repo.repo_uid, "pkg");
		let file = make_file(&repo.repo_uid, "pkg/index.ts");
		conn.upsert_files(std::slice::from_ref(&file)).expect("upsert file");
		insert_file_ownership(
			&conn,
			&snap1.snapshot_uid,
			&repo.repo_uid,
			&file.file_uid,
			"mc-1",
		);

		// Snapshot 1 has ownership
		let result1 = conn
			.get_file_ownership_for_snapshot(&snap1.snapshot_uid)
			.expect("query");
		assert_eq!(result1.len(), 1);

		// Snapshot 2 has no ownership
		let result2 = conn
			.get_file_ownership_for_snapshot(&snap2.snapshot_uid)
			.expect("query");
		assert!(result2.is_empty());
	}

	// ── get_files_for_module tests ─────────────────────────────────

	#[test]
	fn get_files_for_module_returns_empty_for_empty_module() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_files_for_module(&snapshot_uid, "nonexistent-module")
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_files_for_module_returns_owned_files() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create module
		insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");

		// Create files
		let file_a = make_file(&repo_uid, "packages/app/index.ts");
		let file_b = make_file(&repo_uid, "packages/app/utils.ts");
		conn.upsert_files(&[file_a.clone(), file_b.clone()])
			.expect("upsert files");

		// Create ownership
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_a.file_uid, "mc-app");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_b.file_uid, "mc-app");

		let result = conn
			.get_files_for_module(&snapshot_uid, "mc-app")
			.expect("query");

		assert_eq!(result.len(), 2);
		// Sorted by path
		assert_eq!(result[0].path, "packages/app/index.ts");
		assert_eq!(result[1].path, "packages/app/utils.ts");
	}

	#[test]
	fn get_files_for_module_includes_all_fields() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create module
		insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

		// Create file with language
		let mut file = make_file(&repo_uid, "packages/core/lib.ts");
		file.language = Some("typescript".to_string());
		conn.upsert_files(std::slice::from_ref(&file)).expect("upsert file");

		// Create ownership
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-core");

		let result = conn
			.get_files_for_module(&snapshot_uid, "mc-core")
			.expect("query");

		assert_eq!(result.len(), 1);
		let entry = &result[0];
		assert_eq!(entry.file_uid, file.file_uid);
		assert_eq!(entry.path, "packages/core/lib.ts");
		assert_eq!(entry.language, Some("typescript".to_string()));
		assert_eq!(entry.assignment_kind, "manifest");
		assert!((entry.confidence - 1.0).abs() < f64::EPSILON);
	}

	#[test]
	fn get_files_for_module_only_returns_files_for_specified_module() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create two modules
		insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
		insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

		// Create files
		let file_app = make_file(&repo_uid, "packages/app/index.ts");
		let file_core = make_file(&repo_uid, "packages/core/lib.ts");
		conn.upsert_files(&[file_app.clone(), file_core.clone()])
			.expect("upsert files");

		// Assign to different modules
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_app.file_uid, "mc-app");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_core.file_uid, "mc-core");

		// Query for app module only
		let result = conn
			.get_files_for_module(&snapshot_uid, "mc-app")
			.expect("query");

		assert_eq!(result.len(), 1);
		assert_eq!(result[0].path, "packages/app/index.ts");
	}

	#[test]
	fn get_files_for_module_is_scoped_to_snapshot() {
		let mut conn = fresh_storage();
		let repo = make_repo("test-repo");
		conn.add_repo(&repo).expect("add repo");

		// Create two snapshots
		let snap1 = conn
			.create_snapshot(&CreateSnapshotInput {
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
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: repo.repo_uid.clone(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.expect("create snapshot 2");

		// Set up module and file in snapshot 1 only
		insert_module_candidate(&conn, "mc-1", &snap1.snapshot_uid, &repo.repo_uid, "pkg");
		let file = make_file(&repo.repo_uid, "pkg/index.ts");
		conn.upsert_files(std::slice::from_ref(&file)).expect("upsert file");
		insert_file_ownership(
			&conn,
			&snap1.snapshot_uid,
			&repo.repo_uid,
			&file.file_uid,
			"mc-1",
		);

		// Snapshot 1 has files for module
		let result1 = conn
			.get_files_for_module(&snap1.snapshot_uid, "mc-1")
			.expect("query");
		assert_eq!(result1.len(), 1);

		// Snapshot 2 has no files for this module
		let result2 = conn
			.get_files_for_module(&snap2.snapshot_uid, "mc-1")
			.expect("query");
		assert!(result2.is_empty());
	}

	// ── get_owned_files_for_rollup tests ───────────────────────────

	#[test]
	fn get_owned_files_for_rollup_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_owned_files_for_rollup(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_owned_files_for_rollup_returns_all_owned_files() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create modules
		insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
		insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

		// Create files
		let file_app = make_file(&repo_uid, "packages/app/index.ts");
		let file_core = make_file(&repo_uid, "packages/core/lib.ts");
		conn.upsert_files(&[file_app.clone(), file_core.clone()])
			.expect("upsert files");

		// Create ownership
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_app.file_uid, "mc-app");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_core.file_uid, "mc-core");

		let result = conn
			.get_owned_files_for_rollup(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 2);
		// Sorted by path
		assert_eq!(result[0].file_path, "packages/app/index.ts");
		assert_eq!(result[0].module_candidate_uid, "mc-app");
		assert_eq!(result[1].file_path, "packages/core/lib.ts");
		assert_eq!(result[1].module_candidate_uid, "mc-core");
	}

	#[test]
	fn get_owned_files_for_rollup_includes_is_test_flag() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");

		// Create non-test and test files
		let mut file_src = make_file(&repo_uid, "packages/app/service.ts");
		file_src.is_test = false;
		let mut file_test = make_file(&repo_uid, "packages/app/service.test.ts");
		file_test.is_test = true;
		conn.upsert_files(&[file_src.clone(), file_test.clone()])
			.expect("upsert files");

		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_src.file_uid, "mc-app");
		insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_test.file_uid, "mc-app");

		let result = conn
			.get_owned_files_for_rollup(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 2);
		// Sorted by path
		let src = result.iter().find(|r| r.file_path.contains("service.ts") && !r.file_path.contains(".test.")).unwrap();
		let test = result.iter().find(|r| r.file_path.contains(".test.")).unwrap();

		assert!(!src.is_test);
		assert!(test.is_test);
	}

	// ── OWNS edge fallback tests ───────────────────────────────────

	use crate::types::{GraphEdge, GraphNode};

	fn make_module_node(
		node_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		module_key: &str,
		canonical_path: &str,
	) -> GraphNode {
		GraphNode {
			node_uid: node_uid.to_string(),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			stable_key: format!("{}:{}:MODULE", repo_uid, canonical_path),
			kind: "MODULE".to_string(),
			subtype: None,
			name: module_key.to_string(),
			qualified_name: Some(canonical_path.to_string()),
			file_uid: None,
			parent_node_uid: None,
			location: None,
			signature: None,
			visibility: None,
			doc_comment: None,
			metadata_json: None,
		}
	}

	fn make_file_node(
		node_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		file_path: &str,
		file_uid: &str,
	) -> GraphNode {
		GraphNode {
			node_uid: node_uid.to_string(),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			stable_key: format!("{}:{}:FILE", repo_uid, file_path),
			kind: "FILE".to_string(),
			subtype: None,
			name: file_path.split('/').last().unwrap_or(file_path).to_string(),
			qualified_name: Some(file_path.to_string()),
			file_uid: Some(file_uid.to_string()),
			parent_node_uid: None,
			location: None,
			signature: None,
			visibility: None,
			doc_comment: None,
			metadata_json: None,
		}
	}

	fn make_owns_edge(
		edge_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		module_node_uid: &str,
		file_node_uid: &str,
	) -> GraphEdge {
		GraphEdge {
			edge_uid: edge_uid.to_string(),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			source_node_uid: module_node_uid.to_string(),
			target_node_uid: file_node_uid.to_string(),
			edge_type: "OWNS".to_string(),
			resolution: "static".to_string(),
			extractor: "test".to_string(),
			location: None,
			metadata_json: None,
		}
	}

	#[test]
	fn get_file_ownership_from_owns_edges_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_file_ownership_from_owns_edges(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_file_ownership_from_owns_edges_returns_ownership() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create MODULE node
		let module_node =
			make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

		// Create file record (required for FK constraint on nodes.file_uid)
		let file_record = make_file(&repo_uid, "packages/app/index.ts");
		conn.upsert_files(std::slice::from_ref(&file_record))
			.expect("upsert file");

		// Create FILE node with matching file_uid
		let file = make_file_node(
			"file-1",
			&snapshot_uid,
			&repo_uid,
			"packages/app/index.ts",
			&file_record.file_uid,
		);

		conn.insert_nodes(&[module_node, file]).expect("insert nodes");

		// Create OWNS edge: module -> file
		let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
		conn.insert_edges(&[owns]).expect("insert edge");

		let result = conn
			.get_file_ownership_from_owns_edges(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 1);
		// The result.file_uid is the FILE node's file_uid field (reference to files table)
		assert_eq!(result[0].file_uid, file_record.file_uid);
		assert_eq!(result[0].module_candidate_uid, "mod-1");
	}

	#[test]
	fn get_file_ownership_from_owns_edges_excludes_non_owns_edges() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		let module_node =
			make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

		// Create file record (required for FK constraint on nodes.file_uid)
		let file_record = make_file(&repo_uid, "packages/app/index.ts");
		conn.upsert_files(std::slice::from_ref(&file_record))
			.expect("upsert file");

		let file = make_file_node(
			"file-1",
			&snapshot_uid,
			&repo_uid,
			"packages/app/index.ts",
			&file_record.file_uid,
		);
		conn.insert_nodes(&[module_node, file]).expect("insert nodes");

		// Create IMPORTS edge (not OWNS)
		let mut edge = make_owns_edge("edge-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
		edge.edge_type = "IMPORTS".to_string();
		conn.insert_edges(&[edge]).expect("insert edge");

		let result = conn
			.get_file_ownership_from_owns_edges(&snapshot_uid)
			.expect("query");

		assert!(result.is_empty());
	}

	#[test]
	fn get_owned_files_from_owns_edges_returns_empty_for_empty_snapshot() {
		let conn = fresh_storage();
		let (_, snapshot_uid) = setup_test_snapshot(&conn);

		let result = conn
			.get_owned_files_from_owns_edges(&snapshot_uid)
			.expect("query");
		assert!(result.is_empty());
	}

	#[test]
	fn get_owned_files_from_owns_edges_returns_all_fields() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		// Create MODULE node
		let module_node =
			make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

		// Create file in files table (for is_test lookup)
		let file_record = make_file(&repo_uid, "packages/app/service.ts");
		conn.upsert_files(std::slice::from_ref(&file_record))
			.expect("upsert file");

		// Create FILE node with matching file_uid
		let file_node = make_file_node(
			"file-1",
			&snapshot_uid,
			&repo_uid,
			"packages/app/service.ts",
			&file_record.file_uid,
		);

		conn.insert_nodes(&[module_node, file_node]).expect("insert nodes");

		// Create OWNS edge
		let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
		conn.insert_edges(&[owns]).expect("insert edge");

		let result = conn
			.get_owned_files_from_owns_edges(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 1);
		assert_eq!(result[0].file_path, "packages/app/service.ts");
		assert_eq!(result[0].module_candidate_uid, "mod-1");
		assert!(!result[0].is_test);
	}

	#[test]
	fn get_owned_files_from_owns_edges_preserves_is_test() {
		let mut conn = fresh_storage();
		let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

		let module_node =
			make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

		// Create test file
		let mut file_record = make_file(&repo_uid, "packages/app/service.test.ts");
		file_record.is_test = true;
		conn.upsert_files(std::slice::from_ref(&file_record))
			.expect("upsert file");

		let file_node = make_file_node(
			"file-1",
			&snapshot_uid,
			&repo_uid,
			"packages/app/service.test.ts",
			&file_record.file_uid,
		);

		conn.insert_nodes(&[module_node, file_node]).expect("insert nodes");

		let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
		conn.insert_edges(&[owns]).expect("insert edge");

		let result = conn
			.get_owned_files_from_owns_edges(&snapshot_uid)
			.expect("query");

		assert_eq!(result.len(), 1);
		assert!(result[0].is_test);
	}
}
