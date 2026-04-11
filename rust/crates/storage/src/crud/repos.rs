//! CRUD methods for the `repos` table.
//!
//! Mirrors `addRepo`, `getRepo`, `listRepos`, `removeRepo` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:201-244`.
//!
//! All four methods are single-statement operations and are NOT
//! transaction-wrapped (matching the TS adapter exactly).

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::{Repo, RepoRef};

impl StorageConnection {
	/// Insert a new repo row. Mirrors TS `addRepo`
	/// (sqlite-storage.ts:201).
	///
	/// Single statement, no transaction. The TS method has the
	/// same shape: one prepared INSERT, no batching, no wrap.
	///
	/// Returns `StorageError::Sqlite` if the insert fails (e.g.,
	/// duplicate `repo_uid`, FK violation, etc.). The caller is
	/// responsible for handling duplicates if they need
	/// upsert semantics.
	pub fn add_repo(&self, repo: &Repo) -> Result<(), StorageError> {
		self.connection().execute(
			"INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at, metadata_json) \
			 VALUES (?, ?, ?, ?, ?, ?)",
			rusqlite::params![
				repo.repo_uid,
				repo.name,
				repo.root_path,
				repo.default_branch,
				repo.created_at,
				repo.metadata_json,
			],
		)?;
		Ok(())
	}

	/// Look up a repo by uid, name, or root_path. Mirrors TS
	/// `getRepo` (sqlite-storage.ts:217).
	///
	/// Dispatches on the `RepoRef` variant to choose which SQL
	/// column to query. Returns `Ok(None)` if no row matches
	/// (parity with TS `null`). Returns `Ok(Some(Repo))` on hit.
	/// Returns `Err(StorageError::Sqlite)` on actual SQL error.
	pub fn get_repo(&self, repo_ref: &RepoRef) -> Result<Option<Repo>, StorageError> {
		let conn = self.connection();
		let result = match repo_ref {
			RepoRef::Uid(uid) => conn.query_row(
				"SELECT * FROM repos WHERE repo_uid = ?",
				rusqlite::params![uid],
				Repo::from_row,
			),
			RepoRef::Name(name) => conn.query_row(
				"SELECT * FROM repos WHERE name = ?",
				rusqlite::params![name],
				Repo::from_row,
			),
			RepoRef::RootPath(path) => conn.query_row(
				"SELECT * FROM repos WHERE root_path = ?",
				rusqlite::params![path],
				Repo::from_row,
			),
		};
		match result {
			Ok(repo) => Ok(Some(repo)),
			Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
			Err(e) => Err(StorageError::Sqlite(e)),
		}
	}

	/// List all repos ordered by name ascending. Mirrors TS
	/// `listRepos` (sqlite-storage.ts:235).
	///
	/// The `ORDER BY name` clause matches the TS adapter exactly.
	/// Tests pin this ordering to keep the parity contract
	/// observable.
	pub fn list_repos(&self) -> Result<Vec<Repo>, StorageError> {
		let conn = self.connection();
		let mut stmt = conn.prepare("SELECT * FROM repos ORDER BY name")?;
		let rows = stmt
			.query_map([], Repo::from_row)?
			.collect::<Result<Vec<_>, _>>()?;
		Ok(rows)
	}

	/// Delete a repo row by uid. Mirrors TS `removeRepo`
	/// (sqlite-storage.ts:242).
	///
	/// Single statement, no transaction. Cascades through the
	/// FK constraints declared in migration 001 (snapshots,
	/// files, file_versions, nodes, edges, etc. all reference
	/// repos with `ON DELETE CASCADE`).
	///
	/// Returns `Ok(())` whether or not a row was actually
	/// deleted. The TS method has the same shape — no rowsAffected
	/// check, no error on missing repo.
	pub fn remove_repo(&self, repo_uid: &str) -> Result<(), StorageError> {
		self.connection().execute(
			"DELETE FROM repos WHERE repo_uid = ?",
			rusqlite::params![repo_uid],
		)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{fresh_storage, make_repo};

	#[test]
	fn add_repo_then_get_repo_by_uid_returns_the_inserted_row() {
		let storage = fresh_storage();
		let repo = make_repo("r1");
		storage.add_repo(&repo).unwrap();

		let retrieved = storage
			.get_repo(&RepoRef::Uid("r1".to_string()))
			.unwrap()
			.expect("repo should be found");
		assert_eq!(retrieved.repo_uid, "r1");
		assert_eq!(retrieved.name, "name-r1");
	}

	#[test]
	fn get_repo_by_name_returns_match() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let retrieved = storage
			.get_repo(&RepoRef::Name("name-r1".to_string()))
			.unwrap()
			.expect("repo should be found");
		assert_eq!(retrieved.repo_uid, "r1");
	}

	#[test]
	fn get_repo_by_root_path_returns_match() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		let retrieved = storage
			.get_repo(&RepoRef::RootPath("/tmp/r1".to_string()))
			.unwrap()
			.expect("repo should be found");
		assert_eq!(retrieved.repo_uid, "r1");
	}

	#[test]
	fn get_repo_returns_none_for_nonexistent() {
		let storage = fresh_storage();
		let result = storage
			.get_repo(&RepoRef::Uid("nope".to_string()))
			.unwrap();
		assert!(result.is_none());
	}

	#[test]
	fn list_repos_returns_all_in_name_order() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r2")).unwrap();
		storage.add_repo(&make_repo("r1")).unwrap();
		storage.add_repo(&make_repo("r3")).unwrap();

		let repos = storage.list_repos().unwrap();
		assert_eq!(repos.len(), 3);
		// ORDER BY name → name-r1, name-r2, name-r3
		assert_eq!(repos[0].name, "name-r1");
		assert_eq!(repos[1].name, "name-r2");
		assert_eq!(repos[2].name, "name-r3");
	}

	#[test]
	fn remove_repo_deletes_the_row() {
		let storage = fresh_storage();
		storage.add_repo(&make_repo("r1")).unwrap();

		storage.remove_repo("r1").unwrap();

		let result = storage
			.get_repo(&RepoRef::Uid("r1".to_string()))
			.unwrap();
		assert!(result.is_none());
	}

	#[test]
	fn remove_repo_is_no_op_for_nonexistent_uid() {
		let storage = fresh_storage();
		// No row to delete; should not error.
		storage.remove_repo("nope").unwrap();
	}
}
