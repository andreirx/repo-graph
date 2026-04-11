//! CRUD methods for the `files` table.
//!
//! Mirrors `upsertFiles`, `getFilesByRepo`, `getStaleFiles` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:345-421`.
//!
//! `upsert_files` is transaction-wrapped (batch upsert with
//! ON CONFLICT). The two read methods are single-statement and
//! not wrapped.
//!
//! ── Parity-critical behaviors ─────────────────────────────────
//!
//! - `upsert_files` writes booleans as `1` (true) or `0` (false)
//!   to match the strict-parity `== 1` read path established in
//!   R2-B's `TrackedFile::from_row`.
//!
//! - `get_files_by_repo` includes `WHERE is_excluded = 0` in the
//!   SQL. The R2-E user lock pinned this: rows with
//!   `is_excluded = 1` MUST NOT appear in the result. The
//!   ordering `ORDER BY path` matches TS exactly so result row
//!   order is observable and parity-checkable.
//!
//! - `get_stale_files` joins `files` with `file_versions` and
//!   filters by `parse_status = 'stale'`. Returns `TrackedFile`
//!   rows for the stale files.

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::TrackedFile;

impl StorageConnection {
	/// Batch upsert files into the `files` table.
	///
	/// Mirrors TS `upsertFiles` (sqlite-storage.ts:345). Uses
	/// `INSERT ... ON CONFLICT(file_uid) DO UPDATE SET ...` to
	/// upsert each row, wrapped in a single transaction so all
	/// rows commit or none do.
	///
	/// The boolean columns (`is_test`, `is_generated`,
	/// `is_excluded`) are written as integer 1 or 0 to match the
	/// strict `== 1` read semantics in `TrackedFile::from_row`
	/// (the R2-B parity correction).
	///
	/// Empty input is a no-op (the transaction opens and commits
	/// without doing any work).
	pub fn upsert_files(&mut self, files: &[TrackedFile]) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO files (file_uid, repo_uid, path, language, is_test, is_generated, is_excluded) \
				 VALUES (?, ?, ?, ?, ?, ?, ?) \
				 ON CONFLICT(file_uid) DO UPDATE SET \
				   language = excluded.language, \
				   is_test = excluded.is_test, \
				   is_generated = excluded.is_generated, \
				   is_excluded = excluded.is_excluded",
			)?;
			for f in files {
				stmt.execute(rusqlite::params![
					f.file_uid,
					f.repo_uid,
					f.path,
					f.language,
					if f.is_test { 1_i64 } else { 0 },
					if f.is_generated { 1_i64 } else { 0 },
					if f.is_excluded { 1_i64 } else { 0 },
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	/// List all files for a repo, EXCLUDING rows where
	/// `is_excluded = 1`. Ordered by `path` ascending.
	///
	/// Mirrors TS `getFilesByRepo` (sqlite-storage.ts:403).
	///
	/// **Parity-critical:** the WHERE clause includes
	/// `is_excluded = 0`. This is locked at the R2-E parity
	/// surface: callers expect this method to return only the
	/// "active" files. To list all files including excluded, a
	/// future method would need to be added — out of scope for
	/// R2-E.
	pub fn get_files_by_repo(
		&self,
		repo_uid: &str,
	) -> Result<Vec<TrackedFile>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT * FROM files WHERE repo_uid = ? AND is_excluded = 0 ORDER BY path",
		)?;
		let rows = stmt
			.query_map(rusqlite::params![repo_uid], TrackedFile::from_row)?
			.collect::<Result<Vec<_>, _>>()?;
		Ok(rows)
	}

	/// List files marked as stale in a snapshot. Joins `files`
	/// with `file_versions` and filters by
	/// `file_versions.parse_status = 'stale'`.
	///
	/// Mirrors TS `getStaleFiles` (sqlite-storage.ts:412). The
	/// SQL shape is identical to TS:
	///
	/// ```sql
	/// SELECT f.* FROM files f
	/// JOIN file_versions fv ON f.file_uid = fv.file_uid
	/// WHERE fv.snapshot_uid = ? AND fv.parse_status = 'stale'
	/// ```
	///
	/// Returns the matching `TrackedFile` DTOs (the SELECT only
	/// pulls columns from the `files` table via `f.*`).
	pub fn get_stale_files(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<TrackedFile>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare(
			"SELECT f.* FROM files f \
			 JOIN file_versions fv ON f.file_uid = fv.file_uid \
			 WHERE fv.snapshot_uid = ? AND fv.parse_status = 'stale'",
		)?;
		let rows = stmt
			.query_map(rusqlite::params![snapshot_uid], TrackedFile::from_row)?
			.collect::<Result<Vec<_>, _>>()?;
		Ok(rows)
	}
}

#[cfg(test)]
mod tests {
	// Note: no `use super::*;` because the test functions
	// dispatch CRUD methods via the `storage` value (e.g.
	// `storage.upsert_files(...)`) and never reference parent-
	// module items by name. Adding `use super::*;` here would
	// trigger an unused-import warning.
	use crate::crud::test_helpers::{
		fresh_storage, make_file, make_file_version, make_repo,
	};
	use crate::types::{CreateSnapshotInput, FileVersion};

	#[test]
	fn upsert_files_then_get_files_by_repo_returns_them_in_path_order() {
		let mut storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		// Insert in non-sorted order to verify ORDER BY path applies.
		let files = vec![
			make_file("r1", "src/c.ts"),
			make_file("r1", "src/a.ts"),
			make_file("r1", "src/b.ts"),
		];
		storage.upsert_files(&files).unwrap();

		let result = storage.get_files_by_repo("r1").unwrap();
		assert_eq!(result.len(), 3);
		assert_eq!(result[0].path, "src/a.ts");
		assert_eq!(result[1].path, "src/b.ts");
		assert_eq!(result[2].path, "src/c.ts");
	}

	#[test]
	fn get_files_by_repo_excludes_is_excluded_rows() {
		// PARITY-CRITICAL: getFilesByRepo MUST filter is_excluded = 0.
		let mut storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let mut excluded = make_file("r1", "src/excluded.ts");
		excluded.is_excluded = true;
		let active = make_file("r1", "src/active.ts");

		storage.upsert_files(&[excluded, active]).unwrap();

		let result = storage.get_files_by_repo("r1").unwrap();
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].path, "src/active.ts");
	}

	#[test]
	fn upsert_files_is_idempotent_and_updates_on_conflict() {
		let mut storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let mut file = make_file("r1", "src/a.ts");
		file.language = Some("typescript".to_string());
		storage.upsert_files(&[file.clone()]).unwrap();

		// Re-upsert with a different language; ON CONFLICT should
		// update.
		file.language = Some("javascript".to_string());
		storage.upsert_files(&[file]).unwrap();

		let result = storage.get_files_by_repo("r1").unwrap();
		assert_eq!(result.len(), 1, "no duplicate row");
		assert_eq!(result[0].language.as_deref(), Some("javascript"));
	}

	#[test]
	fn get_stale_files_returns_only_files_with_stale_parse_status() {
		let mut storage = fresh_storage();
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

		let f_stale = make_file("r1", "src/stale.ts");
		let f_parsed = make_file("r1", "src/parsed.ts");
		storage
			.upsert_files(&[f_stale.clone(), f_parsed.clone()])
			.unwrap();

		// One file_version with parse_status = stale, one with
		// parse_status = parsed.
		let mut fv_stale = make_file_version(&snap.snapshot_uid, &f_stale.file_uid);
		fv_stale.parse_status = "stale".to_string();
		let fv_parsed: FileVersion =
			make_file_version(&snap.snapshot_uid, &f_parsed.file_uid);

		storage
			.upsert_file_versions(&[fv_stale, fv_parsed])
			.unwrap();

		let stale = storage.get_stale_files(&snap.snapshot_uid).unwrap();
		assert_eq!(stale.len(), 1);
		assert_eq!(stale[0].path, "src/stale.ts");
	}

	#[test]
	fn upsert_files_empty_input_is_a_noop() {
		let mut storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();
		storage.upsert_files(&[]).unwrap();

		let result = storage.get_files_by_repo("r1").unwrap();
		assert_eq!(result.len(), 0);
	}
}
