//! CRUD methods for the `edges` table.
//!
//! Mirrors `insertEdges`, `deleteEdgesByUids` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:566-602`.
//!
//! - `insert_edges`: transaction-wrapped batch INSERT.
//! - `delete_edges_by_uids`: single statement (DELETE WHERE IN
//!   list), not wrapped.
//!
//! Note: there is NO public read method for edges in the
//! StoragePort or this CRUD layer. Edges are write-only here.
//! Read access happens via specialized query result shapes
//! elsewhere in the TS adapter (graph queries, callers/callees,
//! etc.) which are out of D4-Core scope.

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::GraphEdge;

impl StorageConnection {
	/// Insert a batch of edges. Mirrors TS `insertEdges`
	/// (sqlite-storage.ts:566).
	///
	/// Uses a prepared statement and a single transaction. Each
	/// edge's `location` field is unpacked into four position
	/// columns; if `location` is `None`, all four columns are
	/// written as NULL.
	///
	/// The `edge_type` Rust field maps to the SQL column `type`
	/// (which is a Rust reserved keyword) per the R2-B D-B4
	/// rename.
	///
	/// Empty input is a no-op (transaction opens and commits
	/// without doing any work).
	pub fn insert_edges(&mut self, edges: &[GraphEdge]) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO edges \
				 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, \
				  type, resolution, extractor, \
				  line_start, col_start, line_end, col_end, metadata_json) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for e in edges {
				let (line_start, col_start, line_end, col_end) = match &e.location {
					Some(loc) => (
						Some(loc.line_start),
						Some(loc.col_start),
						Some(loc.line_end),
						Some(loc.col_end),
					),
					None => (None, None, None, None),
				};
				stmt.execute(rusqlite::params![
					e.edge_uid,
					e.snapshot_uid,
					e.repo_uid,
					e.source_node_uid,
					e.target_node_uid,
					e.edge_type,
					e.resolution,
					e.extractor,
					line_start,
					col_start,
					line_end,
					col_end,
					e.metadata_json,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	/// Delete edges by their UIDs. Mirrors TS
	/// `deleteEdgesByUids` (sqlite-storage.ts:596).
	///
	/// Builds a single `DELETE FROM edges WHERE edge_uid IN
	/// (?, ?, ...)` statement with one placeholder per input UID.
	/// **Not transaction-wrapped** — matches the TS adapter
	/// which uses a single prepare+run with no transaction.
	///
	/// Empty input is a no-op (matches TS early return at
	/// sqlite-storage.ts:597).
	///
	/// The TS adapter spreads the UIDs as positional arguments
	/// (`stmt.run(...edgeUids)`); the Rust port uses
	/// `params_from_iter` to pass the same shape via rusqlite.
	pub fn delete_edges_by_uids(
		&self,
		edge_uids: &[String],
	) -> Result<(), StorageError> {
		if edge_uids.is_empty() {
			return Ok(());
		}
		let placeholders = vec!["?"; edge_uids.len()].join(", ");
		let sql = format!("DELETE FROM edges WHERE edge_uid IN ({})", placeholders);
		self.connection()
			.execute(&sql, rusqlite::params_from_iter(edge_uids.iter()))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::crud::test_helpers::{
		fresh_storage, make_edge, make_file, make_node, make_repo,
	};
	use crate::types::CreateSnapshotInput;

	fn setup_with_two_nodes(
		storage: &mut StorageConnection,
	) -> (String, String, String) {
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
			.upsert_files(&[make_file("r1", "src/a.ts")])
			.unwrap();

		let na = make_node(
			"na",
			&snap.snapshot_uid,
			"r1",
			"r1:src/a.ts#fnA:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"fnA",
		);
		let nb = make_node(
			"nb",
			&snap.snapshot_uid,
			"r1",
			"r1:src/a.ts#fnB:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"fnB",
		);
		storage
			.insert_nodes(&[na.clone(), nb.clone()])
			.unwrap();

		(snap.snapshot_uid, na.node_uid, nb.node_uid)
	}

	#[test]
	fn insert_edges_persists_rows() {
		let mut storage = fresh_storage();
		let (snap_uid, na, nb) = setup_with_two_nodes(&mut storage);

		let e = make_edge("e1", &snap_uid, "r1", &na, &nb);
		storage.insert_edges(&[e]).unwrap();

		let count: i64 = storage
			.connection()
			.query_row(
				"SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?",
				rusqlite::params![snap_uid],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(count, 1);
	}

	#[test]
	fn insert_edges_empty_input_is_a_noop() {
		let mut storage = fresh_storage();
		setup_with_two_nodes(&mut storage);
		storage.insert_edges(&[]).unwrap();
	}

	#[test]
	fn delete_edges_by_uids_removes_only_listed_uids() {
		let mut storage = fresh_storage();
		let (snap_uid, na, nb) = setup_with_two_nodes(&mut storage);

		let e1 = make_edge("e1", &snap_uid, "r1", &na, &nb);
		let e2 = make_edge("e2", &snap_uid, "r1", &nb, &na);
		let e3 = make_edge("e3", &snap_uid, "r1", &na, &nb);
		storage.insert_edges(&[e1, e2, e3]).unwrap();

		// Delete e1 and e3, keep e2.
		storage
			.delete_edges_by_uids(&["e1".to_string(), "e3".to_string()])
			.unwrap();

		let remaining: i64 = storage
			.connection()
			.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
			.unwrap();
		assert_eq!(remaining, 1);

		let kept_uid: String = storage
			.connection()
			.query_row("SELECT edge_uid FROM edges", [], |row| row.get(0))
			.unwrap();
		assert_eq!(kept_uid, "e2");
	}

	#[test]
	fn delete_edges_by_uids_empty_input_is_a_noop() {
		let mut storage = fresh_storage();
		let (snap_uid, na, nb) = setup_with_two_nodes(&mut storage);
		let e = make_edge("e1", &snap_uid, "r1", &na, &nb);
		storage.insert_edges(&[e]).unwrap();

		storage.delete_edges_by_uids(&[]).unwrap();

		// The edge is still there.
		let count: i64 = storage
			.connection()
			.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
			.unwrap();
		assert_eq!(count, 1);
	}
}
