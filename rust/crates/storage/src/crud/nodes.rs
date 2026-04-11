//! CRUD methods for the `nodes` table.
//!
//! Mirrors `insertNodes` (with preflight stable-key collision
//! detection), `queryAllNodes`, `deleteNodesByFile` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:425-622`.
//!
//! - `insert_nodes`: transaction-wrapped batch INSERT, with a
//!   preflight collision check that runs BEFORE any SQL.
//! - `query_all_nodes`: single SELECT, not wrapped.
//! - `delete_nodes_by_file`: composite delete (edges then
//!   nodes) wrapped in a single transaction.
//!
//! ── Parity-critical: insert_nodes preflight ───────────────────
//!
//! Pinned by the R2-E user lock. The TS adapter at
//! `sqlite-storage.ts:439-457` walks the input batch, builds a
//! `Map<stable_key, GraphNode>`, detects duplicates, and groups
//! them into a structured collision report. On any collision,
//! it throws `NodeStableKeyCollisionError` BEFORE issuing any
//! SQL. The Rust port mirrors this exactly:
//!
//!   1. Build a HashMap<stable_key, &GraphNode> walking the
//!      input batch.
//!   2. On the first sight of a duplicate stable_key, find or
//!      create the corresponding `NodeCollision` group and
//!      append the duplicate.
//!   3. After the walk, if collisions is non-empty, return
//!      `Err(StorageError::NodeStableKeyCollision { collisions })`
//!      WITHOUT touching the database.
//!   4. Only on a clean preflight does the actual transaction
//!      and INSERT loop run.
//!
//! Without this check, the underlying SQLite UNIQUE constraint
//! (`idx_nodes_snapshot_key` on `(snapshot_uid, stable_key)`)
//! would fire and the rusqlite error would propagate as a
//! generic "UNIQUE constraint failed" message that does not
//! identify which rows collided. The structured error makes
//! identity-defect diagnosis tractable on real codebases.

use crate::connection::StorageConnection;
use crate::error::{NodeCollision, NodeCollisionInfo, StorageError};
use crate::types::GraphNode;

impl StorageConnection {
	/// Insert a batch of nodes. Mirrors TS `insertNodes`
	/// (sqlite-storage.ts:425).
	///
	/// **Parity-critical preflight:** detects stable_key
	/// collisions within the input batch and returns
	/// `StorageError::NodeStableKeyCollision` if any are found,
	/// BEFORE issuing any SQL. See module-level docs.
	///
	/// On clean preflight, opens a transaction and bulk-inserts
	/// all nodes via a prepared statement. The transaction
	/// commits on success or rolls back on any individual row
	/// failure.
	pub fn insert_nodes(&mut self, nodes: &[GraphNode]) -> Result<(), StorageError> {
		// ── Preflight collision check ─────────────────────────
		//
		// Walk the input batch, building a map from stable_key
		// to the FIRST seen node with that key. On the second
		// sighting of any stable_key, find or create the
		// corresponding collision group and append.
		let mut seen: std::collections::HashMap<&str, &GraphNode> =
			std::collections::HashMap::new();
		let mut collisions: Vec<(String, Vec<&GraphNode>)> = Vec::new();
		for n in nodes {
			let key: &str = &n.stable_key;
			if let Some(existing) = seen.get(key) {
				// Find or create the collision group for this
				// stable_key. Linear search through `collisions`
				// is fine because the expected count is
				// near-zero in production (collisions are
				// pathological cases).
				if let Some(group) = collisions.iter_mut().find(|(k, _)| k == key) {
					group.1.push(n);
				} else {
					collisions.push((key.to_string(), vec![*existing, n]));
				}
			} else {
				seen.insert(key, n);
			}
		}
		if !collisions.is_empty() {
			let structured: Vec<NodeCollision> = collisions
				.into_iter()
				.map(|(key, nodes)| NodeCollision {
					stable_key: key,
					nodes: nodes
						.into_iter()
						.map(|n| NodeCollisionInfo {
							node_uid: n.node_uid.clone(),
							stable_key: n.stable_key.clone(),
							kind: n.kind.clone(),
							subtype: n.subtype.clone(),
							name: n.name.clone(),
							qualified_name: n.qualified_name.clone(),
							file_uid: n.file_uid.clone(),
							line_start: n.location.as_ref().map(|loc| loc.line_start),
						})
						.collect(),
				})
				.collect();
			return Err(StorageError::NodeStableKeyCollision {
				collisions: structured,
			});
		}

		// ── Clean preflight → bulk insert in a transaction ──
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO nodes \
				 (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, \
				  qualified_name, file_uid, parent_node_uid, \
				  line_start, col_start, line_end, col_end, \
				  signature, visibility, doc_comment, metadata_json) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for n in nodes {
				let (line_start, col_start, line_end, col_end) = match &n.location {
					Some(loc) => (
						Some(loc.line_start),
						Some(loc.col_start),
						Some(loc.line_end),
						Some(loc.col_end),
					),
					None => (None, None, None, None),
				};
				stmt.execute(rusqlite::params![
					n.node_uid,
					n.snapshot_uid,
					n.repo_uid,
					n.stable_key,
					n.kind,
					n.subtype,
					n.name,
					n.qualified_name,
					n.file_uid,
					n.parent_node_uid,
					line_start,
					col_start,
					line_end,
					col_end,
					n.signature,
					n.visibility,
					n.doc_comment,
					n.metadata_json,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	/// Read all nodes for a snapshot. Mirrors TS `queryAllNodes`
	/// (sqlite-storage.ts:494).
	///
	/// Returns a `Vec<GraphNode>` (matches the TS `GraphNode[]`
	/// shape). The mapping uses the canonical
	/// `GraphNode::from_row` from R2-B, which encapsulates the
	/// asymmetric location null-handling rule.
	///
	/// Single statement, not transaction-wrapped.
	pub fn query_all_nodes(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GraphNode>, StorageError> {
		let conn = self.connection();
		let mut stmt =
			conn.prepare("SELECT * FROM nodes WHERE snapshot_uid = ?")?;
		let rows = stmt
			.query_map(rusqlite::params![snapshot_uid], GraphNode::from_row)?
			.collect::<Result<Vec<_>, _>>()?;
		Ok(rows)
	}

	/// Delete all nodes for a file in a snapshot, plus all
	/// edges incident to those nodes. Composite delete in a
	/// single transaction.
	///
	/// Mirrors TS `deleteNodesByFile` (sqlite-storage.ts:604).
	///
	/// **Parity-critical:** the two DELETE statements (edges
	/// first, then nodes) MUST run inside a single transaction.
	/// Without the wrap, a partial failure between them would
	/// leave the database with dangling edges that reference
	/// nodes that no longer exist (or, depending on FK config,
	/// with orphaned nodes if the edge delete fails first).
	///
	/// The edge delete is keyed by snapshot_uid plus a subquery
	/// matching nodes whose file_uid equals the input file_uid.
	/// The node delete is keyed by snapshot_uid + file_uid
	/// directly.
	pub fn delete_nodes_by_file(
		&mut self,
		snapshot_uid: &str,
		file_uid: &str,
	) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;

		// Delete incident edges first.
		tx.execute(
			"DELETE FROM edges WHERE snapshot_uid = ? AND ( \
			   source_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND file_uid = ?) \
			   OR target_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND file_uid = ?) \
			 )",
			rusqlite::params![snapshot_uid, snapshot_uid, file_uid, snapshot_uid, file_uid],
		)?;

		// Then delete the nodes themselves.
		tx.execute(
			"DELETE FROM nodes WHERE snapshot_uid = ? AND file_uid = ?",
			rusqlite::params![snapshot_uid, file_uid],
		)?;

		tx.commit()?;
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

	fn setup_snapshot_with_files(storage: &mut StorageConnection) -> String {
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
	fn insert_nodes_then_query_all_nodes_returns_inserted_rows() {
		let mut storage = fresh_storage();
		let snap_uid = setup_snapshot_with_files(&mut storage);

		let n1 = make_node(
			"n1",
			&snap_uid,
			"r1",
			"r1:src/a.ts#fnA:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"fnA",
		);
		let n2 = make_node(
			"n2",
			&snap_uid,
			"r1",
			"r1:src/a.ts#fnB:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"fnB",
		);
		storage.insert_nodes(&[n1, n2]).unwrap();

		let all = storage.query_all_nodes(&snap_uid).unwrap();
		assert_eq!(all.len(), 2);
		let names: Vec<&str> = all.iter().map(|n| n.name.as_str()).collect();
		assert!(names.contains(&"fnA"));
		assert!(names.contains(&"fnB"));
	}

	#[test]
	fn insert_nodes_empty_input_is_a_noop() {
		let mut storage = fresh_storage();
		let snap_uid = setup_snapshot_with_files(&mut storage);
		storage.insert_nodes(&[]).unwrap();
		assert_eq!(storage.query_all_nodes(&snap_uid).unwrap().len(), 0);
	}

	#[test]
	fn insert_nodes_preflight_detects_stable_key_collision_within_batch() {
		// PARITY-CRITICAL: insert_nodes must detect duplicate
		// stable_keys WITHIN the input batch and return
		// NodeStableKeyCollision BEFORE issuing any SQL. The
		// error must surface the colliding key and the candidate
		// nodes' identity fields.
		let mut storage = fresh_storage();
		let snap_uid = setup_snapshot_with_files(&mut storage);

		let n1 = make_node(
			"n1",
			&snap_uid,
			"r1",
			"r1:src/a.ts#duplicated:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"firstEmission",
		);
		let n2 = make_node(
			"n2",
			&snap_uid,
			"r1",
			"r1:src/a.ts#duplicated:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"secondEmission",
		);

		let result = storage.insert_nodes(&[n1, n2]);
		assert!(result.is_err());

		match result.unwrap_err() {
			StorageError::NodeStableKeyCollision { collisions } => {
				assert_eq!(collisions.len(), 1, "exactly one colliding key");
				assert_eq!(
					collisions[0].stable_key,
					"r1:src/a.ts#duplicated:SYMBOL:FUNCTION"
				);
				assert_eq!(
					collisions[0].nodes.len(),
					2,
					"both candidate nodes carried in the collision"
				);
				let names: Vec<&str> = collisions[0]
					.nodes
					.iter()
					.map(|n| n.name.as_str())
					.collect();
				assert!(names.contains(&"firstEmission"));
				assert!(names.contains(&"secondEmission"));
			}
			other => panic!("expected NodeStableKeyCollision, got {:?}", other),
		}

		// Critical: NO rows must have been inserted (preflight
		// runs BEFORE any SQL).
		assert_eq!(
			storage.query_all_nodes(&snap_uid).unwrap().len(),
			0,
			"preflight must reject the batch BEFORE any insert"
		);
	}

	#[test]
	fn collision_error_message_includes_stable_key_and_node_identity() {
		// Pin the parity-critical error message format. The user
		// lock requires the message to surface the stable_key and
		// the colliding nodes' identity fields, mirroring the TS
		// NodeStableKeyCollisionError class output.
		let mut storage = fresh_storage();
		let snap_uid = setup_snapshot_with_files(&mut storage);

		let n1 = make_node(
			"node-uid-1",
			&snap_uid,
			"r1",
			"r1:src/a.ts#dup:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"alpha",
		);
		let n2 = make_node(
			"node-uid-2",
			&snap_uid,
			"r1",
			"r1:src/a.ts#dup:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"beta",
		);

		let err = storage.insert_nodes(&[n1, n2]).unwrap_err();
		let msg = err.to_string();

		// The message must contain the stable_key.
		assert!(
			msg.contains("r1:src/a.ts#dup:SYMBOL:FUNCTION"),
			"error message must contain the colliding stable_key, got:\n{}",
			msg
		);
		// And the candidate node identity fields.
		assert!(msg.contains("alpha"));
		assert!(msg.contains("beta"));
		assert!(msg.contains("node-uid-1"));
		assert!(msg.contains("node-uid-2"));
		// And the closing identity-defect note.
		assert!(msg.contains("identity-model defect"));
	}

	#[test]
	fn delete_nodes_by_file_deletes_nodes_and_incident_edges_in_one_transaction() {
		// PARITY-CRITICAL: delete_nodes_by_file must delete BOTH
		// the nodes AND the edges incident to them, in one
		// transaction.
		let mut storage = fresh_storage();
		let snap_uid = setup_snapshot_with_files(&mut storage);

		let node_a = make_node(
			"na",
			&snap_uid,
			"r1",
			"r1:src/a.ts#fnA:SYMBOL:FUNCTION",
			"r1:src/a.ts",
			"fnA",
		);
		let node_b = make_node(
			"nb",
			&snap_uid,
			"r1",
			"r1:src/b.ts#fnB:SYMBOL:FUNCTION",
			"r1:src/b.ts",
			"fnB",
		);
		storage
			.insert_nodes(&[node_a.clone(), node_b.clone()])
			.unwrap();

		// Edge from node_a (in src/a.ts) to node_b (in src/b.ts).
		let edge = make_edge(
			"e1",
			&snap_uid,
			"r1",
			&node_a.node_uid,
			&node_b.node_uid,
		);
		storage.insert_edges(&[edge]).unwrap();

		// Delete src/a.ts → should drop node_a AND the edge.
		storage
			.delete_nodes_by_file(&snap_uid, "r1:src/a.ts")
			.unwrap();

		// node_a is gone.
		let remaining = storage.query_all_nodes(&snap_uid).unwrap();
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].name, "fnB");

		// The edge that referenced node_a is gone too.
		let edge_count: i64 = storage
			.connection()
			.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
			.unwrap();
		assert_eq!(edge_count, 0, "incident edge must be deleted along with node");
	}
}
