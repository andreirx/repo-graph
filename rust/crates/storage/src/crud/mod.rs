//! Per-entity CRUD methods for the D4-Core entity set.
//!
//! Per R2-E decision D-E2: each D4-Core entity has its own
//! `crud/<entity>.rs` file containing `impl StorageConnection`
//! blocks with that entity's CRUD methods. `connection.rs` stays
//! focused on lifecycle (open / open_in_memory / accessors); the
//! per-entity CRUD logic lives here.
//!
//! Module layout (one file per D4-Core entity):
//!
//! - `repos`         — `add_repo`, `get_repo` (with `RepoRef`),
//!                     `list_repos`, `remove_repo`
//! - `snapshots`     — `create_snapshot`, `get_snapshot`,
//!                     `get_latest_snapshot`,
//!                     `update_snapshot_status`,
//!                     `update_snapshot_counts`
//! - `files`         — `upsert_files`, `get_files_by_repo`,
//!                     `get_stale_files`
//! - `file_versions` — `upsert_file_versions`,
//!                     `query_file_version_hashes`
//! - `nodes`         — `insert_nodes` (with preflight stable-key
//!                     collision detection), `query_all_nodes`,
//!                     `delete_nodes_by_file`
//! - `edges`         — `insert_edges`, `delete_edges_by_uids`
//!
//! Total: **19 methods** across 6 entity files. This matches the
//! exact set the user locked at R2-E decision D-E1, using the
//! real TS port method names from `src/core/ports/storage.ts`
//! translated to Rust snake_case.
//!
//! ── Transaction policy (D-E5 override) ────────────────────────
//!
//! Each method's transaction wrap matches the corresponding TS
//! adapter method exactly (override of the original "every method
//! gets a transaction" default):
//!
//!   - **Wrapped:** `upsert_files`, `upsert_file_versions`,
//!     `insert_nodes`, `insert_edges`, `delete_nodes_by_file`
//!     (composite delete: edges then nodes)
//!   - **Not wrapped:** `add_repo`, `get_repo`, `list_repos`,
//!     `remove_repo`, `create_snapshot`, `get_snapshot`,
//!     `get_latest_snapshot`, `update_snapshot_status`,
//!     `update_snapshot_counts`, `get_files_by_repo`,
//!     `get_stale_files`, `query_file_version_hashes`,
//!     `query_all_nodes`, `delete_edges_by_uids`
//!
//! The `&mut self` vs `&self` split on the public methods is
//! determined by whether the method opens a transaction. rusqlite's
//! `Connection::transaction()` requires `&mut Connection`, so
//! transaction-wrapped methods take `&mut self`. Single-statement
//! methods take `&self`.
//!
//! ── Parity-critical behaviors (R2-E user lock) ────────────────
//!
//! - `insert_nodes` preserves the preflight stable-key collision
//!   check from `sqlite-storage.ts:425-457`. The Rust mapper
//!   walks the input batch, builds a `HashMap<&str, &GraphNode>`,
//!   detects duplicates, and groups them into `NodeCollision`
//!   structures. On any collision, returns
//!   `StorageError::NodeStableKeyCollision { collisions: ... }`
//!   BEFORE issuing any SQL. The error message format mirrors
//!   the TS class output line by line.
//!
//! - `get_latest_snapshot` filters by `status = 'ready'`, NOT by
//!   timestamp alone. A BUILDING snapshot is not "the latest"
//!   for the purpose of this method.
//!
//! - `get_files_by_repo` excludes rows with `is_excluded = 1`.
//!
//! - `delete_nodes_by_file` is a single transaction containing
//!   two DELETE statements: incident edges first, then nodes.
//!   Without the wrap, a partial failure between the two would
//!   leave dangling edges referencing missing nodes.
//!
//! - `create_snapshot` generates the snapshot UID via
//!   `<repo_uid>/<ISO-timestamp>/<UUID-prefix>`, sets initial
//!   status to `BUILDING`, and reads the row back via
//!   `get_snapshot` after insert. Returns the read-back DTO.
//!   Throws if the read-back fails (which should never happen
//!   in practice but is a defensive check inherited from TS).
//!
//! - `query_file_version_hashes` returns a `HashMap<file_uid,
//!   content_hash>`. Map semantics: no defined ordering. The TS
//!   adapter returns `Map<string, string>`, which JavaScript
//!   `Map` preserves insertion order, but the Rust `HashMap`
//!   does not. Per the user's R2-E lock, this is treated as
//!   lookup semantics, not ordered-report semantics.

pub mod declarations;
pub mod edges;
pub mod file_versions;
pub mod files;
pub mod module_candidates;
pub mod nodes;
pub mod repos;
pub mod snapshots;

#[cfg(test)]
pub(crate) mod test_helpers {
	//! Shared test helpers for CRUD tests across the per-entity
	//! modules. Each test in `repos`, `snapshots`, `files`,
	//! `file_versions`, `nodes`, `edges` can `use
	//! crate::crud::test_helpers::...` to construct fixtures
	//! without duplicating boilerplate.

	use crate::connection::StorageConnection;
	use crate::types::{
		FileVersion, GraphEdge, GraphNode, Repo, SourceLocation, TrackedFile,
	};

	/// Open a fresh in-memory `StorageConnection` with all 16
	/// migrations applied.
	pub fn fresh_storage() -> StorageConnection {
		StorageConnection::open_in_memory().expect("open_in_memory")
	}

	pub fn make_repo(uid: &str) -> Repo {
		Repo {
			repo_uid: uid.to_string(),
			name: format!("name-{}", uid),
			root_path: format!("/tmp/{}", uid),
			default_branch: Some("main".to_string()),
			created_at: "2025-01-01T00:00:00Z".to_string(),
			metadata_json: None,
		}
	}

	pub fn make_file(repo_uid: &str, path: &str) -> TrackedFile {
		TrackedFile {
			file_uid: format!("{}:{}", repo_uid, path),
			repo_uid: repo_uid.to_string(),
			path: path.to_string(),
			language: Some("typescript".to_string()),
			is_test: false,
			is_generated: false,
			is_excluded: false,
		}
	}

	pub fn make_file_version(snapshot_uid: &str, file_uid: &str) -> FileVersion {
		FileVersion {
			snapshot_uid: snapshot_uid.to_string(),
			file_uid: file_uid.to_string(),
			content_hash: "deadbeef".to_string(),
			ast_hash: None,
			extractor: Some("ts-core:0.1.0".to_string()),
			parse_status: "parsed".to_string(),
			size_bytes: Some(1024),
			line_count: Some(42),
			indexed_at: "2025-01-01T00:00:00Z".to_string(),
		}
	}

	pub fn make_node(
		node_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		stable_key: &str,
		file_uid: &str,
		name: &str,
	) -> GraphNode {
		GraphNode {
			node_uid: node_uid.to_string(),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			stable_key: stable_key.to_string(),
			kind: "SYMBOL".to_string(),
			subtype: Some("FUNCTION".to_string()),
			name: name.to_string(),
			qualified_name: Some(name.to_string()),
			file_uid: Some(file_uid.to_string()),
			parent_node_uid: None,
			location: Some(SourceLocation {
				line_start: 1,
				col_start: 0,
				line_end: 10,
				col_end: 0,
			}),
			signature: None,
			visibility: Some("export".to_string()),
			doc_comment: None,
			metadata_json: None,
		}
	}

	pub fn make_edge(
		edge_uid: &str,
		snapshot_uid: &str,
		repo_uid: &str,
		source: &str,
		target: &str,
	) -> GraphEdge {
		GraphEdge {
			edge_uid: edge_uid.to_string(),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			source_node_uid: source.to_string(),
			target_node_uid: target.to_string(),
			edge_type: "CALLS".to_string(),
			resolution: "static".to_string(),
			extractor: "test:0.0.1".to_string(),
			location: None,
			metadata_json: None,
		}
	}
}
