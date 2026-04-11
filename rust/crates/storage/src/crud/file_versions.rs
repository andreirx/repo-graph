//! CRUD methods for the `file_versions` table.
//!
//! Mirrors `upsertFileVersions`
//! (`sqlite-storage.ts:371-401`) and `queryFileVersionHashes`
//! (`sqlite-storage.ts:1524-1533`).
//!
//! `upsert_file_versions` is transaction-wrapped (batch upsert
//! with composite-PK ON CONFLICT). `query_file_version_hashes`
//! is a single SELECT and not wrapped.
//!
//! Per the R2-E lock: `query_file_version_hashes` is treated as
//! lookup semantics (HashMap), not ordered-report semantics. The
//! TS adapter returns `Map<string, string>`, JavaScript Maps
//! preserve insertion order, but ordering is not part of the
//! parity contract for this method.
//!
//! Note: `get_stale_files` ALSO touches `file_versions` (joining
//! against the `files` table) but lives in `crud/files.rs`
//! because it returns `TrackedFile` DTOs, not `FileVersion`.

use std::collections::HashMap;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::FileVersion;

impl StorageConnection {
	/// Batch upsert file versions into the `file_versions`
	/// table. Mirrors TS `upsertFileVersions`
	/// (sqlite-storage.ts:371).
	///
	/// Uses `INSERT ... ON CONFLICT(snapshot_uid, file_uid) DO
	/// UPDATE SET ...` to upsert each row. The composite primary
	/// key `(snapshot_uid, file_uid)` is the conflict target.
	/// Wrapped in a single transaction.
	///
	/// On conflict, all columns except the primary key are
	/// overwritten with the new values from the input row.
	pub fn upsert_file_versions(
		&mut self,
		versions: &[FileVersion],
	) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO file_versions \
				 (snapshot_uid, file_uid, content_hash, ast_hash, extractor, parse_status, size_bytes, line_count, indexed_at) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
				 ON CONFLICT(snapshot_uid, file_uid) DO UPDATE SET \
				   content_hash = excluded.content_hash, \
				   ast_hash = excluded.ast_hash, \
				   extractor = excluded.extractor, \
				   parse_status = excluded.parse_status, \
				   size_bytes = excluded.size_bytes, \
				   line_count = excluded.line_count, \
				   indexed_at = excluded.indexed_at",
			)?;
			for fv in versions {
				stmt.execute(rusqlite::params![
					fv.snapshot_uid,
					fv.file_uid,
					fv.content_hash,
					fv.ast_hash,
					fv.extractor,
					fv.parse_status,
					fv.size_bytes,
					fv.line_count,
					fv.indexed_at,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	/// Read content_hash for every file version in a snapshot,
	/// returned as a HashMap keyed by file_uid.
	///
	/// Mirrors TS `queryFileVersionHashes`
	/// (sqlite-storage.ts:1524). The TS method returns a JS
	/// `Map<string, string>` which preserves insertion order;
	/// the Rust port returns `HashMap<String, String>` which
	/// does not. **Per the R2-E user lock:** this method is
	/// lookup semantics, not ordered-report semantics. The map
	/// supports `.get(file_uid)` queries; ordering is not part
	/// of the contract.
	///
	/// Used by the invalidation planner to detect changed files
	/// across snapshots: compare the result map against current
	/// file content hashes to find which entries need
	/// re-extraction.
	pub fn query_file_version_hashes(
		&self,
		snapshot_uid: &str,
	) -> Result<HashMap<String, String>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT file_uid, content_hash FROM file_versions WHERE snapshot_uid = ?",
		)?;
		let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
			Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
		})?;
		let mut map = HashMap::new();
		for row in rows {
			let (file_uid, content_hash) = row?;
			map.insert(file_uid, content_hash);
		}
		Ok(map)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{
		fresh_storage, make_file, make_file_version, make_repo,
	};
	use crate::types::CreateSnapshotInput;

	fn setup_with_snapshot(storage: &mut StorageConnection) -> String {
		storage.add_repo(&make_repo("r1")).unwrap();
		let snap = storage
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: "r1".to_string(),
				kind: "full".to_string(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.unwrap();
		storage
			.upsert_files(&[
				make_file("r1", "src/a.ts"),
				make_file("r1", "src/b.ts"),
			])
			.unwrap();
		snap.snapshot_uid
	}

	#[test]
	fn upsert_file_versions_then_query_hashes_returns_map() {
		let mut storage = fresh_storage();
		let snap_uid = setup_with_snapshot(&mut storage);

		let mut fv_a = make_file_version(&snap_uid, "r1:src/a.ts");
		fv_a.content_hash = "hash-a".to_string();
		let mut fv_b = make_file_version(&snap_uid, "r1:src/b.ts");
		fv_b.content_hash = "hash-b".to_string();

		storage.upsert_file_versions(&[fv_a, fv_b]).unwrap();

		let hashes = storage.query_file_version_hashes(&snap_uid).unwrap();
		assert_eq!(hashes.len(), 2);
		assert_eq!(hashes.get("r1:src/a.ts").map(|s| s.as_str()), Some("hash-a"));
		assert_eq!(hashes.get("r1:src/b.ts").map(|s| s.as_str()), Some("hash-b"));
	}

	#[test]
	fn upsert_file_versions_on_conflict_updates_columns() {
		let mut storage = fresh_storage();
		let snap_uid = setup_with_snapshot(&mut storage);

		let mut fv = make_file_version(&snap_uid, "r1:src/a.ts");
		fv.content_hash = "v1".to_string();
		storage.upsert_file_versions(&[fv.clone()]).unwrap();

		// Re-upsert with new content_hash; the composite-PK conflict
		// should trigger the DO UPDATE clause.
		fv.content_hash = "v2".to_string();
		storage.upsert_file_versions(&[fv]).unwrap();

		let hashes = storage.query_file_version_hashes(&snap_uid).unwrap();
		assert_eq!(hashes.len(), 1, "no duplicate composite-PK row");
		assert_eq!(hashes.get("r1:src/a.ts").map(|s| s.as_str()), Some("v2"));
	}

	#[test]
	fn query_file_version_hashes_returns_empty_map_for_unknown_snapshot() {
		let storage = fresh_storage();
		let hashes = storage.query_file_version_hashes("nope").unwrap();
		assert!(hashes.is_empty());
	}
}
