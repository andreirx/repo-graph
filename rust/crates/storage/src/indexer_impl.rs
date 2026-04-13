//! Indexer storage sub-trait implementations for `StorageConnection`.
//!
//! This module implements the indexer policy crate's storage
//! sub-traits on top of the storage adapter's rusqlite connection.
//! Dependency direction: adapter → policy (storage crate imports
//! and implements traits from the indexer crate).
//!
//! R5-B/C: `SnapshotLifecyclePort`, `FileCatalogPort`
//! R5-F:   `NodeStorePort`, `EdgeStorePort`, `UnresolvedEdgePort`,
//!         `FileSignalPort`
//!
//! Snapshot/file methods delegate to existing inherent CRUD methods
//! with type conversion. Node/edge/signal methods include new SQL
//! for tables that had no CRUD methods in the R2 scope (extraction_edges,
//! unresolved_edges, file_signals).

use std::collections::BTreeMap;

use repo_graph_classification::types::SourceLocation as ClassificationSourceLocation;
use repo_graph_indexer::storage_port::{
	self as ixp, CopyForwardInput, CopyForwardResult, DeltaCopyPort,
	EdgeStorePort, ExtractionEdgeRow, FileCatalogPort, FileSignalPort,
	FileSignalRow, NodeStorePort, PersistedUnresolvedEdge,
	SnapshotLifecyclePort, UnresolvedEdgePort,
};
use repo_graph_indexer::resolver::ResolverNode;
use repo_graph_indexer::types::{SnapshotKind, SnapshotStatus};


use crate::connection::StorageConnection;
use crate::error::StorageError;

// ── Type conversion helpers ──────────────────────────────────────

/// Serialize a typed enum to its SQL TEXT string via serde.
fn serialize_enum<T: serde::Serialize + std::fmt::Debug>(val: &T) -> String {
	serde_json::to_value(val)
		.ok()
		.and_then(|v| match v {
			serde_json::Value::String(s) => Some(s),
			_ => None,
		})
		.unwrap_or_else(|| format!("{:?}", val))
}

fn to_storage_create_snapshot(
	input: &ixp::CreateSnapshotInput,
) -> crate::types::CreateSnapshotInput {
	crate::types::CreateSnapshotInput {
		repo_uid: input.repo_uid.clone(),
		kind: serde_json::to_value(&input.kind)
			.ok()
			.and_then(|v| v.as_str().map(|s| s.to_string()))
			.unwrap_or_else(|| format!("{:?}", input.kind)),
		basis_ref: input.basis_ref.clone(),
		basis_commit: input.basis_commit.clone(),
		parent_snapshot_uid: input.parent_snapshot_uid.clone(),
		label: input.label.clone(),
		toolchain_json: input.toolchain_json.clone(),
	}
}

/// Deserialize a persisted string into a typed enum via serde.
/// Returns `Err(StorageError::Sqlite(FromSqlConversionFailure))`
/// on unknown values — the same policy-boundary validation
/// pattern used in `trust_impl.rs` for classification enums.
fn deserialize_enum<T: serde::de::DeserializeOwned>(
	raw: &str,
	column_name: &str,
) -> Result<T, StorageError> {
	serde_json::from_value::<T>(serde_json::Value::String(raw.to_owned())).map_err(|_| {
		StorageError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
			0,
			rusqlite::types::Type::Text,
			format!("unknown {} value: {:?}", column_name, raw).into(),
		))
	})
}

fn from_storage_snapshot(s: crate::types::Snapshot) -> Result<ixp::Snapshot, StorageError> {
	let kind: SnapshotKind = deserialize_enum(&s.kind, "snapshot kind")?;
	let status: SnapshotStatus = deserialize_enum(&s.status, "snapshot status")?;
	Ok(ixp::Snapshot {
		snapshot_uid: s.snapshot_uid,
		repo_uid: s.repo_uid,
		parent_snapshot_uid: s.parent_snapshot_uid,
		kind,
		basis_ref: s.basis_ref,
		basis_commit: s.basis_commit,
		dirty_hash: s.dirty_hash,
		status,
		files_total: s.files_total as u64,
		nodes_total: s.nodes_total as u64,
		edges_total: s.edges_total as u64,
		created_at: s.created_at,
		completed_at: s.completed_at,
		label: s.label,
		toolchain_json: s.toolchain_json,
	})
}

fn to_storage_update_status(
	input: &ixp::UpdateSnapshotStatusInput,
) -> crate::types::UpdateSnapshotStatusInput {
	crate::types::UpdateSnapshotStatusInput {
		snapshot_uid: input.snapshot_uid.clone(),
		status: serde_json::to_value(&input.status)
			.ok()
			.and_then(|v| v.as_str().map(|s| s.to_string()))
			.unwrap_or_else(|| format!("{:?}", input.status)),
		completed_at: input.completed_at.clone(),
	}
}

fn to_storage_tracked_file(f: &ixp::TrackedFile) -> crate::types::TrackedFile {
	crate::types::TrackedFile {
		file_uid: f.file_uid.clone(),
		repo_uid: f.repo_uid.clone(),
		path: f.path.clone(),
		language: f.language.clone(),
		is_test: f.is_test,
		is_generated: f.is_generated,
		is_excluded: f.is_excluded,
	}
}

fn from_storage_tracked_file(f: crate::types::TrackedFile) -> ixp::TrackedFile {
	ixp::TrackedFile {
		file_uid: f.file_uid,
		repo_uid: f.repo_uid,
		path: f.path,
		language: f.language,
		is_test: f.is_test,
		is_generated: f.is_generated,
		is_excluded: f.is_excluded,
	}
}

fn to_storage_file_version(v: &ixp::FileVersion) -> crate::types::FileVersion {
	crate::types::FileVersion {
		snapshot_uid: v.snapshot_uid.clone(),
		file_uid: v.file_uid.clone(),
		content_hash: v.content_hash.clone(),
		ast_hash: v.ast_hash.clone(),
		extractor: v.extractor.clone(),
		parse_status: serde_json::to_value(&v.parse_status)
			.ok()
			.and_then(|val| val.as_str().map(|s| s.to_string()))
			.unwrap_or_else(|| format!("{:?}", v.parse_status)),
		size_bytes: v.size_bytes.map(|n| n as i64),
		line_count: v.line_count.map(|n| n as i64),
		indexed_at: v.indexed_at.clone(),
	}
}

// ── SnapshotLifecyclePort ────────────────────────────────────────

impl SnapshotLifecyclePort for StorageConnection {
	type Error = StorageError;

	fn create_snapshot(
		&mut self,
		input: &ixp::CreateSnapshotInput,
	) -> Result<ixp::Snapshot, StorageError> {
		let si = to_storage_create_snapshot(input);
		// Delegate to the inherent method. The inherent method takes
		// &self; &mut self coerces to &self.
		let snap = <StorageConnection>::create_snapshot(self, &si)?;
		from_storage_snapshot(snap)
	}

	fn get_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Option<ixp::Snapshot>, StorageError> {
		let snap = <StorageConnection>::get_snapshot(self, snapshot_uid)?;
		snap.map(from_storage_snapshot).transpose()
	}

	fn get_latest_snapshot(
		&self,
		repo_uid: &str,
	) -> Result<Option<ixp::Snapshot>, StorageError> {
		let snap = <StorageConnection>::get_latest_snapshot(self, repo_uid)?;
		snap.map(from_storage_snapshot).transpose()
	}

	fn update_snapshot_status(
		&mut self,
		input: &ixp::UpdateSnapshotStatusInput,
	) -> Result<(), StorageError> {
		let si = to_storage_update_status(input);
		<StorageConnection>::update_snapshot_status(self, &si)
	}

	fn update_snapshot_counts(
		&mut self,
		snapshot_uid: &str,
	) -> Result<(), StorageError> {
		<StorageConnection>::update_snapshot_counts(self, snapshot_uid)
	}

	fn update_snapshot_extraction_diagnostics(
		&mut self,
		snapshot_uid: &str,
		diagnostics_json: &str,
	) -> Result<(), StorageError> {
		// No pre-existing inherent method for this. Implemented
		// directly. Mirrors TS updateSnapshotExtractionDiagnostics
		// at sqlite-storage.ts:332.
		self.connection().execute(
			"UPDATE snapshots SET extraction_diagnostics_json = ? WHERE snapshot_uid = ?",
			rusqlite::params![diagnostics_json, snapshot_uid],
		)?;
		Ok(())
	}
}

// ── FileCatalogPort ──────────────────────────────────────────────

impl FileCatalogPort for StorageConnection {
	type Error = StorageError;

	fn upsert_files(
		&mut self,
		files: &[ixp::TrackedFile],
	) -> Result<(), StorageError> {
		let storage_files: Vec<crate::types::TrackedFile> =
			files.iter().map(to_storage_tracked_file).collect();
		<StorageConnection>::upsert_files(self, &storage_files)
	}

	fn upsert_file_versions(
		&mut self,
		versions: &[ixp::FileVersion],
	) -> Result<(), StorageError> {
		let storage_versions: Vec<crate::types::FileVersion> =
			versions.iter().map(to_storage_file_version).collect();
		<StorageConnection>::upsert_file_versions(self, &storage_versions)
	}

	fn get_files_by_repo(
		&self,
		repo_uid: &str,
	) -> Result<Vec<ixp::TrackedFile>, StorageError> {
		let files = <StorageConnection>::get_files_by_repo(self, repo_uid)?;
		Ok(files.into_iter().map(from_storage_tracked_file).collect())
	}

	fn get_stale_files(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ixp::TrackedFile>, StorageError> {
		let files = <StorageConnection>::get_stale_files(self, snapshot_uid)?;
		Ok(files.into_iter().map(from_storage_tracked_file).collect())
	}

	fn query_file_version_hashes(
		&self,
		snapshot_uid: &str,
	) -> Result<BTreeMap<String, String>, StorageError> {
		// The inherent method returns HashMap; the trait requires
		// BTreeMap (no-HashMap rule on public port boundary).
		let hash_map = <StorageConnection>::query_file_version_hashes(self, snapshot_uid)?;
		Ok(hash_map.into_iter().collect())
	}
}

// ── NodeStorePort ────────────────────────────────────────────────

impl NodeStorePort for StorageConnection {
	type Error = StorageError;

	fn insert_nodes(
		&mut self,
		nodes: &[ixp::ExtractedNode],
	) -> Result<(), StorageError> {
		// Convert to storage-crate GraphNode and delegate.
		let storage_nodes: Vec<crate::types::GraphNode> = nodes
			.iter()
			.map(|n| crate::types::GraphNode {
				node_uid: n.node_uid.clone(),
				snapshot_uid: n.snapshot_uid.clone(),
				repo_uid: n.repo_uid.clone(),
				stable_key: n.stable_key.clone(),
				kind: serde_json::to_value(&n.kind)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default(),
				subtype: n.subtype.as_ref().and_then(|st| {
					serde_json::to_value(st)
						.ok()
						.and_then(|v| v.as_str().map(|s| s.to_string()))
				}),
				name: n.name.clone(),
				qualified_name: n.qualified_name.clone(),
				file_uid: n.file_uid.clone(),
				parent_node_uid: n.parent_node_uid.clone(),
				location: n.location.map(|loc| crate::types::SourceLocation {
					line_start: loc.line_start,
					col_start: loc.col_start,
					line_end: loc.line_end,
					col_end: loc.col_end,
				}),
				signature: n.signature.clone(),
				visibility: n.visibility.as_ref().and_then(|v| {
					serde_json::to_value(v)
						.ok()
						.and_then(|val| val.as_str().map(|s| s.to_string()))
				}),
				doc_comment: n.doc_comment.clone(),
				metadata_json: n.metadata_json.clone(),
			})
			.collect();
		<StorageConnection>::insert_nodes(self, &storage_nodes)
	}

	fn query_all_nodes(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ixp::ExtractedNode>, StorageError> {
		let storage_nodes = <StorageConnection>::query_all_nodes(self, snapshot_uid)?;
		let mut result = Vec::with_capacity(storage_nodes.len());
		for n in storage_nodes {
			let kind = deserialize_enum(&n.kind, "nodes.kind")?;
			let subtype = match &n.subtype {
				Some(s) => Some(deserialize_enum(s, "nodes.subtype")?),
				None => None,
			};
			let visibility = match &n.visibility {
				Some(s) => Some(deserialize_enum(s, "nodes.visibility")?),
				None => None,
			};
			result.push(ixp::ExtractedNode {
				node_uid: n.node_uid,
				snapshot_uid: n.snapshot_uid,
				repo_uid: n.repo_uid,
				stable_key: n.stable_key,
				kind,
				subtype,
				name: n.name,
				qualified_name: n.qualified_name,
				file_uid: n.file_uid,
				parent_node_uid: n.parent_node_uid,
				location: n.location.map(|loc| ClassificationSourceLocation {
					line_start: loc.line_start,
					col_start: loc.col_start,
					line_end: loc.line_end,
					col_end: loc.col_end,
				}),
				signature: n.signature,
				visibility,
				doc_comment: n.doc_comment,
				metadata_json: n.metadata_json,
			});
		}
		Ok(result)
	}

	fn query_resolver_nodes(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ResolverNode>, StorageError> {
		// Slim query — only the 7 fields ResolverNode needs.
		let mut stmt = self.connection().prepare(
			"SELECT node_uid, stable_key, name, qualified_name, kind, subtype, file_uid \
			 FROM nodes WHERE snapshot_uid = ?",
		)?;
		let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
			Ok(ResolverNode {
				node_uid: row.get(0)?,
				stable_key: row.get(1)?,
				name: row.get(2)?,
				qualified_name: row.get(3)?,
				kind: row.get(4)?,
				subtype: row.get(5)?,
				file_uid: row.get(6)?,
			})
		})?;
		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	fn delete_nodes_by_file(
		&mut self,
		snapshot_uid: &str,
		file_uid: &str,
	) -> Result<(), StorageError> {
		<StorageConnection>::delete_nodes_by_file(self, snapshot_uid, file_uid)
	}
}

// ── EdgeStorePort ────────────────────────────────────────────────

impl EdgeStorePort for StorageConnection {
	type Error = StorageError;

	fn insert_resolved_edges(
		&mut self,
		edges: &[ixp::ResolvedEdge],
	) -> Result<(), StorageError> {
		let storage_edges: Vec<crate::types::GraphEdge> = edges
			.iter()
			.map(|e| crate::types::GraphEdge {
				edge_uid: e.edge_uid.clone(),
				snapshot_uid: e.snapshot_uid.clone(),
				repo_uid: e.repo_uid.clone(),
				source_node_uid: e.source_node_uid.clone(),
				target_node_uid: e.target_node_uid.clone(),
				edge_type: serde_json::to_value(&e.edge_type)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default(),
				resolution: serde_json::to_value(&e.resolution)
					.ok()
					.and_then(|v| v.as_str().map(|s| s.to_string()))
					.unwrap_or_default(),
				extractor: e.extractor.clone(),
				location: e.location.map(|loc| crate::types::SourceLocation {
					line_start: loc.line_start,
					col_start: loc.col_start,
					line_end: loc.line_end,
					col_end: loc.col_end,
				}),
				metadata_json: e.metadata_json.clone(),
			})
			.collect();
		<StorageConnection>::insert_edges(self, &storage_edges)
	}

	fn insert_extraction_edges(
		&mut self,
		edges: &[ExtractionEdgeRow],
	) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO extraction_edges \
				 (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
				  target_key, type, resolution, extractor, \
				  line_start, col_start, line_end, col_end, \
				  metadata_json, source_file_uid) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for e in edges {
				let edge_type_str = serialize_enum(&e.edge_type);
				let resolution_str = serialize_enum(&e.resolution);
				stmt.execute(rusqlite::params![
					e.edge_uid,
					e.snapshot_uid,
					e.repo_uid,
					e.source_node_uid,
					e.target_key,
					edge_type_str,
					resolution_str,
					e.extractor,
					e.line_start,
					e.col_start,
					e.line_end,
					e.col_end,
					e.metadata_json,
					e.source_file_uid,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	fn query_extraction_edges_batch(
		&self,
		snapshot_uid: &str,
		limit: usize,
		after_edge_uid: Option<&str>,
	) -> Result<Vec<ExtractionEdgeRow>, StorageError> {
		let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
			match after_edge_uid {
				Some(cursor) => (
					"SELECT edge_uid, snapshot_uid, repo_uid, source_node_uid, \
					 target_key, type, resolution, extractor, \
					 line_start, col_start, line_end, col_end, \
					 metadata_json, source_file_uid \
					 FROM extraction_edges \
					 WHERE snapshot_uid = ? AND edge_uid > ? \
					 ORDER BY edge_uid ASC LIMIT ?"
						.into(),
					vec![
						Box::new(snapshot_uid.to_string()),
						Box::new(cursor.to_string()),
						Box::new(limit as i64),
					],
				),
				None => (
					"SELECT edge_uid, snapshot_uid, repo_uid, source_node_uid, \
					 target_key, type, resolution, extractor, \
					 line_start, col_start, line_end, col_end, \
					 metadata_json, source_file_uid \
					 FROM extraction_edges \
					 WHERE snapshot_uid = ? \
					 ORDER BY edge_uid ASC LIMIT ?"
						.into(),
					vec![
						Box::new(snapshot_uid.to_string()),
						Box::new(limit as i64),
					],
				),
			};
		let param_refs: Vec<&dyn rusqlite::types::ToSql> =
			params.iter().map(|p| p.as_ref()).collect();
		let mut stmt = self.connection().prepare(&sql)?;
		let rows = stmt.query_map(param_refs.as_slice(), |row| {
			Ok((
				row.get::<_, String>(0)?,  // edge_uid
				row.get::<_, String>(1)?,  // snapshot_uid
				row.get::<_, String>(2)?,  // repo_uid
				row.get::<_, String>(3)?,  // source_node_uid
				row.get::<_, String>(4)?,  // target_key
				row.get::<_, String>(5)?,  // edge_type (raw)
				row.get::<_, String>(6)?,  // resolution (raw)
				row.get::<_, String>(7)?,  // extractor
				row.get::<_, Option<i64>>(8)?,  // line_start
				row.get::<_, Option<i64>>(9)?,  // col_start
				row.get::<_, Option<i64>>(10)?, // line_end
				row.get::<_, Option<i64>>(11)?, // col_end
				row.get::<_, Option<String>>(12)?, // metadata_json
				row.get::<_, Option<String>>(13)?, // source_file_uid
			))
		})?;
		let mut result = Vec::new();
		for row in rows {
			let (edge_uid, snapshot_uid, repo_uid, source_node_uid,
				 target_key, edge_type_str, resolution_str, extractor,
				 line_start, col_start, line_end, col_end,
				 metadata_json, source_file_uid) = row?;
			let edge_type = deserialize_enum(&edge_type_str, "extraction_edges.type")?;
			let resolution = deserialize_enum(&resolution_str, "extraction_edges.resolution")?;
			result.push(ExtractionEdgeRow {
				edge_uid, snapshot_uid, repo_uid, source_node_uid,
				target_key, edge_type, resolution, extractor,
				line_start, col_start, line_end, col_end,
				metadata_json, source_file_uid,
			});
		}
		Ok(result)
	}

	fn delete_edges_by_uids(
		&mut self,
		edge_uids: &[String],
	) -> Result<(), StorageError> {
		<StorageConnection>::delete_edges_by_uids(self, edge_uids)
	}
}

// ── UnresolvedEdgePort ───────────────────────────────────────────

impl UnresolvedEdgePort for StorageConnection {
	type Error = StorageError;

	fn insert_unresolved_edges(
		&mut self,
		edges: &[PersistedUnresolvedEdge],
	) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT INTO unresolved_edges \
				 (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
				  target_key, type, resolution, extractor, \
				  line_start, col_start, line_end, col_end, \
				  metadata_json, category, classification, \
				  classifier_version, basis_code, observed_at) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for e in edges {
				let edge_type_str = serialize_enum(&e.edge_type);
				let resolution_str = serialize_enum(&e.resolution);
				let category_str = serialize_enum(&e.category);
				let classification_str = serialize_enum(&e.classification);
				let basis_code_str = serialize_enum(&e.basis_code);
				stmt.execute(rusqlite::params![
					e.edge_uid,
					e.snapshot_uid,
					e.repo_uid,
					e.source_node_uid,
					e.target_key,
					edge_type_str,
					resolution_str,
					e.extractor,
					e.line_start,
					e.col_start,
					e.line_end,
					e.col_end,
					e.metadata_json,
					category_str,
					classification_str,
					e.classifier_version,
					basis_code_str,
					e.observed_at,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}
}

// ── FileSignalPort ───────────────────────────────────────────────

impl FileSignalPort for StorageConnection {
	type Error = StorageError;

	fn insert_file_signals(
		&mut self,
		signals: &[FileSignalRow],
	) -> Result<(), StorageError> {
		let tx = self.connection_mut().transaction()?;
		{
			let mut stmt = tx.prepare(
				"INSERT OR REPLACE INTO file_signals \
				 (snapshot_uid, file_uid, import_bindings_json, \
				  package_dependencies_json, tsconfig_aliases_json) \
				 VALUES (?, ?, ?, ?, ?)",
			)?;
			for s in signals {
				stmt.execute(rusqlite::params![
					s.snapshot_uid,
					s.file_uid,
					s.import_bindings_json,
					s.package_dependencies_json,
					s.tsconfig_aliases_json,
				])?;
			}
		}
		tx.commit()?;
		Ok(())
	}

	fn query_file_signals_batch(
		&self,
		snapshot_uid: &str,
		file_uids: &[String],
	) -> Result<Vec<FileSignalRow>, StorageError> {
		if file_uids.is_empty() {
			return Ok(vec![]);
		}
		let placeholders: String = file_uids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
		let sql = format!(
			"SELECT snapshot_uid, file_uid, import_bindings_json, \
			 package_dependencies_json, tsconfig_aliases_json \
			 FROM file_signals \
			 WHERE snapshot_uid = ? AND file_uid IN ({})",
			placeholders
		);
		let mut params: Vec<&dyn rusqlite::types::ToSql> =
			vec![&snapshot_uid as &dyn rusqlite::types::ToSql];
		for uid in file_uids {
			params.push(uid as &dyn rusqlite::types::ToSql);
		}
		let mut stmt = self.connection().prepare(&sql)?;
		let rows = stmt.query_map(params.as_slice(), |row| {
			Ok(FileSignalRow {
				snapshot_uid: row.get(0)?,
				file_uid: row.get(1)?,
				import_bindings_json: row.get(2)?,
				package_dependencies_json: row.get(3)?,
				tsconfig_aliases_json: row.get(4)?,
			})
		})?;
		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}
}

// ── DeltaCopyPort ────────────────────────────────────────────────

impl DeltaCopyPort for StorageConnection {
	type Error = StorageError;

	fn copy_forward_unchanged_files(
		&mut self,
		input: &CopyForwardInput,
	) -> Result<CopyForwardResult, StorageError> {
		if input.file_uids.is_empty() {
			return Ok(CopyForwardResult::default());
		}

		let placeholders: String = input
			.file_uids
			.iter()
			.map(|_| "?")
			.collect::<Vec<_>>()
			.join(", ");

		let tx = self.connection_mut().transaction()?;
		let mut result = CopyForwardResult::default();

		// 1. Copy nodes with new UIDs, preserving stable_keys.
		//    Build old_uid → new_uid map for edge remapping.
		let mut node_uid_map: std::collections::HashMap<String, String> =
			std::collections::HashMap::new();
		{
			let select_sql = format!(
				"SELECT node_uid, stable_key, kind, subtype, name, \
				 qualified_name, file_uid, parent_node_uid, \
				 line_start, col_start, line_end, col_end, \
				 signature, visibility, doc_comment, metadata_json \
				 FROM nodes WHERE snapshot_uid = ? AND file_uid IN ({})",
				placeholders
			);
			let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
				vec![Box::new(input.from_snapshot_uid.clone())];
			for uid in &input.file_uids {
				params.push(Box::new(uid.clone()));
			}
			let param_refs: Vec<&dyn rusqlite::types::ToSql> =
				params.iter().map(|p| p.as_ref()).collect();

			let mut stmt = tx.prepare(&select_sql)?;
			// Collect rows before inserting (can't read+write same table).
			struct NodeRow {
				old_uid: String,
				stable_key: String,
				kind: String,
				subtype: Option<String>,
				name: String,
				qualified_name: Option<String>,
				file_uid: Option<String>,
				parent_node_uid: Option<String>,
				line_start: Option<i64>,
				col_start: Option<i64>,
				line_end: Option<i64>,
				col_end: Option<i64>,
				signature: Option<String>,
				visibility: Option<String>,
				doc_comment: Option<String>,
				metadata_json: Option<String>,
			}
			let rows: Vec<NodeRow> = stmt
				.query_map(param_refs.as_slice(), |row| {
					Ok(NodeRow {
						old_uid: row.get(0)?,
						stable_key: row.get(1)?,
						kind: row.get(2)?,
						subtype: row.get(3)?,
						name: row.get(4)?,
						qualified_name: row.get(5)?,
						file_uid: row.get(6)?,
						parent_node_uid: row.get(7)?,
						line_start: row.get(8)?,
						col_start: row.get(9)?,
						line_end: row.get(10)?,
						col_end: row.get(11)?,
						signature: row.get(12)?,
						visibility: row.get(13)?,
						doc_comment: row.get(14)?,
						metadata_json: row.get(15)?,
					})
				})?
				.collect::<Result<Vec<_>, _>>()?;
			drop(stmt);

			let mut insert_stmt = tx.prepare(
				"INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, \
				 stable_key, kind, subtype, name, qualified_name, \
				 file_uid, parent_node_uid, line_start, col_start, \
				 line_end, col_end, signature, visibility, \
				 doc_comment, metadata_json) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			// First pass: assign new UIDs to all nodes.
			for r in &rows {
				let new_uid = uuid::Uuid::new_v4().to_string();
				node_uid_map.insert(r.old_uid.clone(), new_uid);
			}
			// Second pass: insert with remapped parent_node_uid.
			for r in &rows {
				let new_uid = node_uid_map.get(&r.old_uid).unwrap();
				// Remap parent_node_uid if the parent was also copied.
				let new_parent = r.parent_node_uid.as_ref().map(|old_parent| {
					node_uid_map
						.get(old_parent)
						.cloned()
						.unwrap_or_else(|| old_parent.clone())
				});
				insert_stmt.execute(rusqlite::params![
					new_uid, input.to_snapshot_uid, input.repo_uid,
					r.stable_key, r.kind, r.subtype, r.name,
					r.qualified_name, r.file_uid, new_parent,
					r.line_start, r.col_start, r.line_end, r.col_end,
					r.signature, r.visibility, r.doc_comment, r.metadata_json,
				])?;
				result.nodes_copied += 1;
			}
		}

		// 2. Copy extraction_edges with remapped source_node_uids.
		{
			let select_sql = format!(
				"SELECT edge_uid, source_node_uid, target_key, type, \
				 resolution, extractor, line_start, col_start, \
				 line_end, col_end, metadata_json, source_file_uid \
				 FROM extraction_edges \
				 WHERE snapshot_uid = ? AND source_file_uid IN ({})",
				placeholders
			);
			let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
				vec![Box::new(input.from_snapshot_uid.clone())];
			for uid in &input.file_uids {
				params.push(Box::new(uid.clone()));
			}
			let param_refs: Vec<&dyn rusqlite::types::ToSql> =
				params.iter().map(|p| p.as_ref()).collect();

			struct EdgeRow {
				old_source_uid: String,
				target_key: String,
				edge_type: String,
				resolution: String,
				extractor: String,
				line_start: Option<i64>,
				col_start: Option<i64>,
				line_end: Option<i64>,
				col_end: Option<i64>,
				metadata_json: Option<String>,
				source_file_uid: Option<String>,
			}
			let mut stmt = tx.prepare(&select_sql)?;
			let rows: Vec<EdgeRow> = stmt
				.query_map(param_refs.as_slice(), |row| {
					Ok(EdgeRow {
						old_source_uid: row.get(1)?,
						target_key: row.get(2)?,
						edge_type: row.get(3)?,
						resolution: row.get(4)?,
						extractor: row.get(5)?,
						line_start: row.get(6)?,
						col_start: row.get(7)?,
						line_end: row.get(8)?,
						col_end: row.get(9)?,
						metadata_json: row.get(10)?,
						source_file_uid: row.get(11)?,
					})
				})?
				.collect::<Result<Vec<_>, _>>()?;
			drop(stmt);

			let mut insert_stmt = tx.prepare(
				"INSERT INTO extraction_edges \
				 (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
				  target_key, type, resolution, extractor, \
				  line_start, col_start, line_end, col_end, \
				  metadata_json, source_file_uid) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for r in &rows {
				let new_source = node_uid_map
					.get(&r.old_source_uid)
					.cloned()
					.unwrap_or_else(|| r.old_source_uid.clone());
				insert_stmt.execute(rusqlite::params![
					uuid::Uuid::new_v4().to_string(),
					input.to_snapshot_uid, input.repo_uid, new_source,
					r.target_key, r.edge_type, r.resolution, r.extractor,
					r.line_start, r.col_start, r.line_end, r.col_end,
					r.metadata_json, r.source_file_uid,
				])?;
				result.extraction_edges_copied += 1;
			}
		}

		// 3. Copy file_signals.
		{
			let select_sql = format!(
				"SELECT file_uid, import_bindings_json, \
				 package_dependencies_json, tsconfig_aliases_json \
				 FROM file_signals \
				 WHERE snapshot_uid = ? AND file_uid IN ({})",
				placeholders
			);
			let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
				vec![Box::new(input.from_snapshot_uid.clone())];
			for uid in &input.file_uids {
				params.push(Box::new(uid.clone()));
			}
			let param_refs: Vec<&dyn rusqlite::types::ToSql> =
				params.iter().map(|p| p.as_ref()).collect();

			let mut stmt = tx.prepare(&select_sql)?;
			let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
				.query_map(param_refs.as_slice(), |row| {
					Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
				})?
				.collect::<Result<Vec<_>, _>>()?;
			drop(stmt);

			let mut insert_stmt = tx.prepare(
				"INSERT OR REPLACE INTO file_signals \
				 (snapshot_uid, file_uid, import_bindings_json, \
				  package_dependencies_json, tsconfig_aliases_json) \
				 VALUES (?, ?, ?, ?, ?)",
			)?;
			for (fuid, bindings, deps, aliases) in &rows {
				insert_stmt.execute(rusqlite::params![
					input.to_snapshot_uid, fuid, bindings, deps, aliases,
				])?;
				result.file_signals_copied += 1;
			}
		}

		// 4. Copy file_versions.
		{
			let select_sql = format!(
				"SELECT file_uid, content_hash, ast_hash, extractor, \
				 parse_status, size_bytes, line_count, indexed_at \
				 FROM file_versions \
				 WHERE snapshot_uid = ? AND file_uid IN ({})",
				placeholders
			);
			let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
				vec![Box::new(input.from_snapshot_uid.clone())];
			for uid in &input.file_uids {
				params.push(Box::new(uid.clone()));
			}
			let param_refs: Vec<&dyn rusqlite::types::ToSql> =
				params.iter().map(|p| p.as_ref()).collect();

			let mut stmt = tx.prepare(&select_sql)?;
			let rows: Vec<(String, String, Option<String>, Option<String>, String, Option<i64>, Option<i64>, String)> = stmt
				.query_map(param_refs.as_slice(), |row| {
					Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
						row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?))
				})?
				.collect::<Result<Vec<_>, _>>()?;
			drop(stmt);

			let mut insert_stmt = tx.prepare(
				"INSERT OR REPLACE INTO file_versions \
				 (snapshot_uid, file_uid, content_hash, ast_hash, \
				  extractor, parse_status, size_bytes, line_count, indexed_at) \
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
			)?;
			for (fuid, hash, ast, ext, status, size, lines, indexed) in &rows {
				insert_stmt.execute(rusqlite::params![
					input.to_snapshot_uid, fuid, hash, ast, ext, status, size, lines, indexed,
				])?;
				result.file_versions_copied += 1;
			}
		}

		tx.commit()?;
		Ok(result)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::Repo;
	use repo_graph_indexer::types::{SnapshotKind, SnapshotStatus};

	fn setup() -> StorageConnection {
		let storage = StorageConnection::open_in_memory().unwrap();
		storage
			.add_repo(&Repo {
				repo_uid: "r1".into(),
				name: "test".into(),
				root_path: "/tmp/test".into(),
				default_branch: Some("main".into()),
				created_at: "2025-01-01T00:00:00.000Z".into(),
				metadata_json: None,
			})
			.unwrap();
		storage
	}

	// ── SnapshotLifecyclePort ────────────────────────────────

	#[test]
	fn create_snapshot_via_trait_returns_typed_snapshot() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: Some("abc123".into()),
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();
		assert_eq!(snap.repo_uid, "r1");
		assert_eq!(snap.kind, SnapshotKind::Full);
		assert_eq!(snap.status, SnapshotStatus::Building);
		assert_eq!(snap.basis_commit, Some("abc123".into()));
	}

	#[test]
	fn get_snapshot_via_trait_returns_none_for_missing() {
		let storage = setup();
		let result =
			SnapshotLifecyclePort::get_snapshot(&storage, "nonexistent").unwrap();
		assert!(result.is_none());
	}

	#[test]
	fn update_snapshot_status_via_trait() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();

		SnapshotLifecyclePort::update_snapshot_status(
			&mut storage,
			&ixp::UpdateSnapshotStatusInput {
				snapshot_uid: snap.snapshot_uid.clone(),
				status: SnapshotStatus::Ready,
				completed_at: None,
			},
		)
		.unwrap();

		let updated =
			SnapshotLifecyclePort::get_snapshot(&storage, &snap.snapshot_uid)
				.unwrap()
				.unwrap();
		assert_eq!(updated.status, SnapshotStatus::Ready);
	}

	#[test]
	fn update_extraction_diagnostics_via_trait() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();

		SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(
			&mut storage,
			&snap.snapshot_uid,
			r#"{"diagnostics_version":1}"#,
		)
		.unwrap();

		// Verify via the trust port (which reads the same column).
		use repo_graph_trust::TrustStorageRead;
		let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(
			&storage,
			&snap.snapshot_uid,
		)
		.unwrap();
		assert_eq!(
			diag,
			Some(r#"{"diagnostics_version":1}"#.to_string())
		);
	}

	// ── FileCatalogPort ──────────────────────────────────────

	#[test]
	fn upsert_files_and_get_files_via_trait() {
		let mut storage = setup();
		FileCatalogPort::upsert_files(
			&mut storage,
			&[ixp::TrackedFile {
				file_uid: "r1:src/a.ts".into(),
				repo_uid: "r1".into(),
				path: "src/a.ts".into(),
				language: Some("typescript".into()),
				is_test: false,
				is_generated: false,
				is_excluded: false,
			}],
		)
		.unwrap();

		let files = FileCatalogPort::get_files_by_repo(&storage, "r1").unwrap();
		assert_eq!(files.len(), 1);
		assert_eq!(files[0].path, "src/a.ts");
		assert_eq!(files[0].language, Some("typescript".to_string()));
	}

	#[test]
	fn query_file_version_hashes_returns_btreemap() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();

		// Must insert the tracked file first (FK on file_uid).
		FileCatalogPort::upsert_files(
			&mut storage,
			&[ixp::TrackedFile {
				file_uid: "r1:src/a.ts".into(),
				repo_uid: "r1".into(),
				path: "src/a.ts".into(),
				language: Some("typescript".into()),
				is_test: false,
				is_generated: false,
				is_excluded: false,
			}],
		)
		.unwrap();

		FileCatalogPort::upsert_file_versions(
			&mut storage,
			&[ixp::FileVersion {
				snapshot_uid: snap.snapshot_uid.clone(),
				file_uid: "r1:src/a.ts".into(),
				content_hash: "hash123".into(),
				ast_hash: None,
				extractor: Some("ts-base:1".into()),
				parse_status: repo_graph_indexer::types::ParseStatus::Parsed,
				size_bytes: Some(100),
				line_count: Some(10),
				indexed_at: "2025-01-01T00:00:00.000Z".into(),
			}],
		)
		.unwrap();

		let hashes =
			FileCatalogPort::query_file_version_hashes(&storage, &snap.snapshot_uid)
				.unwrap();
		assert_eq!(hashes.len(), 1);
		assert_eq!(hashes.get("r1:src/a.ts"), Some(&"hash123".to_string()));
	}

	// ── Malformed enum regression tests ──────────────────────
	//
	// These prove the adapter returns Err on unknown snapshot
	// kind/status values rather than silently fabricating valid
	// policy enums from corrupt data.

	#[test]
	fn get_snapshot_errors_on_bad_kind_value() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();

		// Corrupt the kind column to an invalid value.
		storage
			.connection()
			.execute(
				"UPDATE snapshots SET kind = 'BOGUS_KIND' WHERE snapshot_uid = ?",
				rusqlite::params![snap.snapshot_uid],
			)
			.unwrap();

		let result =
			SnapshotLifecyclePort::get_snapshot(&storage, &snap.snapshot_uid);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed snapshot kind must propagate as Err, got {:?}",
			result
		);
	}

	#[test]
	fn get_snapshot_errors_on_bad_status_value() {
		let mut storage = setup();
		let snap = SnapshotLifecyclePort::create_snapshot(
			&mut storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap();

		// Corrupt the status column to an invalid value.
		storage
			.connection()
			.execute(
				"UPDATE snapshots SET status = 'BOGUS_STATUS' WHERE snapshot_uid = ?",
				rusqlite::params![snap.snapshot_uid],
			)
			.unwrap();

		let result =
			SnapshotLifecyclePort::get_snapshot(&storage, &snap.snapshot_uid);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed snapshot status must propagate as Err, got {:?}",
			result
		);
	}

	// ── NodeStorePort ────────────────────────────────────────

	fn make_snap(storage: &mut StorageConnection) -> String {
		SnapshotLifecyclePort::create_snapshot(
			storage,
			&ixp::CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: SnapshotKind::Full,
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			},
		)
		.unwrap()
		.snapshot_uid
	}

	#[test]
	fn insert_and_query_resolver_nodes_via_trait() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);

		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "n1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/a.ts:foo:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: Some(repo_graph_indexer::types::NodeSubtype::Function),
				name: "foo".into(),
				qualified_name: Some("src/a.ts:foo".into()),
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();

		let resolver_nodes =
			NodeStorePort::query_resolver_nodes(&storage, &snap_uid).unwrap();
		assert_eq!(resolver_nodes.len(), 1);
		assert_eq!(resolver_nodes[0].node_uid, "n1");
		assert_eq!(resolver_nodes[0].name, "foo");
		assert_eq!(resolver_nodes[0].kind, "SYMBOL");
		assert_eq!(resolver_nodes[0].subtype, Some("FUNCTION".to_string()));
	}

	// ── EdgeStorePort ────────────────────────────────────────

	#[test]
	fn insert_and_query_extraction_edges_via_trait() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);

		EdgeStorePort::insert_extraction_edges(
			&mut storage,
			&[ExtractionEdgeRow {
				edge_uid: "ee1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				source_node_uid: "n1".into(),
				target_key: "./utils".into(),
				edge_type: repo_graph_indexer::types::EdgeType::Imports,
				resolution: repo_graph_indexer::types::Resolution::Static,
				extractor: "ts-base:1".into(),
				line_start: Some(1),
				col_start: Some(0),
				line_end: Some(1),
				col_end: Some(20),
				metadata_json: None,
				source_file_uid: Some("r1:src/a.ts".into()),
			}],
		)
		.unwrap();

		let batch = EdgeStorePort::query_extraction_edges_batch(
			&storage,
			&snap_uid,
			10,
			None,
		)
		.unwrap();
		assert_eq!(batch.len(), 1);
		assert_eq!(batch[0].edge_uid, "ee1");
		assert_eq!(batch[0].target_key, "./utils");
		assert_eq!(batch[0].edge_type, repo_graph_indexer::types::EdgeType::Imports);
		assert_eq!(batch[0].resolution, repo_graph_indexer::types::Resolution::Static);
		assert_eq!(batch[0].source_file_uid, Some("r1:src/a.ts".to_string()));
	}

	#[test]
	fn extraction_edges_cursor_pagination() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);

		// Insert 3 edges with alphabetically ordered UIDs.
		for id in ["ee_a", "ee_b", "ee_c"] {
			EdgeStorePort::insert_extraction_edges(
				&mut storage,
				&[ExtractionEdgeRow {
					edge_uid: id.into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "n1".into(),
					target_key: "target".into(),
					edge_type: repo_graph_indexer::types::EdgeType::Calls,
					resolution: repo_graph_indexer::types::Resolution::Static,
					extractor: "test:1".into(),
					line_start: None,
					col_start: None,
					line_end: None,
					col_end: None,
					metadata_json: None,
					source_file_uid: None,
				}],
			)
			.unwrap();
		}

		// First page: limit 2.
		let page1 = EdgeStorePort::query_extraction_edges_batch(
			&storage, &snap_uid, 2, None,
		)
		.unwrap();
		assert_eq!(page1.len(), 2);
		assert_eq!(page1[0].edge_uid, "ee_a");
		assert_eq!(page1[1].edge_uid, "ee_b");

		// Second page: after "ee_b".
		let page2 = EdgeStorePort::query_extraction_edges_batch(
			&storage,
			&snap_uid,
			2,
			Some("ee_b"),
		)
		.unwrap();
		assert_eq!(page2.len(), 1);
		assert_eq!(page2[0].edge_uid, "ee_c");
	}

	// ── UnresolvedEdgePort ───────────────────────────────────

	#[test]
	fn insert_unresolved_edges_via_trait() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);

		// Need a node for the FK on source_node_uid.
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "n1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/a.ts:n1:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: None,
				name: "n1".into(),
				qualified_name: None,
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();

		use repo_graph_classification::types::{
			UnresolvedEdgeBasisCode, UnresolvedEdgeCategory,
			UnresolvedEdgeClassification,
		};
		UnresolvedEdgePort::insert_unresolved_edges(
			&mut storage,
			&[PersistedUnresolvedEdge {
				edge_uid: "ue1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				source_node_uid: "n1".into(),
				target_key: "unknownFunc".into(),
				edge_type: repo_graph_indexer::types::EdgeType::Calls,
				resolution: repo_graph_indexer::types::Resolution::Static,
				extractor: "ts-base:1".into(),
				line_start: None,
				col_start: None,
				line_end: None,
				col_end: None,
				metadata_json: None,
				category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
				classification: UnresolvedEdgeClassification::Unknown,
				classifier_version: 1,
				basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
				observed_at: "2025-01-01T00:00:00.000Z".into(),
			}],
		)
		.unwrap();

		// Verify the unresolved edge exists via direct SQL.
		let ue_count: i64 = storage
			.connection()
			.query_row(
				"SELECT COUNT(*) FROM unresolved_edges WHERE snapshot_uid = ?",
				rusqlite::params![snap_uid],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(ue_count, 1);
	}

	// ── FileSignalPort ───────────────────────────────────────

	#[test]
	fn insert_and_query_file_signals_via_trait() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);

		FileSignalPort::insert_file_signals(
			&mut storage,
			&[FileSignalRow {
				snapshot_uid: snap_uid.clone(),
				file_uid: "r1:src/a.ts".into(),
				import_bindings_json: Some(r#"[{"identifier":"foo","specifier":"./foo","isRelative":true,"location":null,"isTypeOnly":false}]"#.into()),
				package_dependencies_json: Some(r#"{"names":["express"]}"#.into()),
				tsconfig_aliases_json: None,
			}],
		)
		.unwrap();

		let signals = FileSignalPort::query_file_signals_batch(
			&storage,
			&snap_uid,
			&["r1:src/a.ts".into()],
		)
		.unwrap();
		assert_eq!(signals.len(), 1);
		assert_eq!(signals[0].file_uid, "r1:src/a.ts");
		assert!(signals[0].import_bindings_json.is_some());
		assert!(signals[0].package_dependencies_json.is_some());
		assert!(signals[0].tsconfig_aliases_json.is_none());
	}

	#[test]
	fn query_file_signals_empty_uids_returns_empty() {
		let storage = setup();
		let signals =
			FileSignalPort::query_file_signals_batch(&storage, "snap1", &[])
				.unwrap();
		assert_eq!(signals.len(), 0);
	}

	// ── Malformed node enum regression tests ─────────────────

	#[test]
	fn query_all_nodes_errors_on_bad_kind() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);
		// Insert a valid node, then corrupt its kind.
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "n_bad".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:bad:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: None,
				name: "bad".into(),
				qualified_name: None,
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();
		storage
			.connection()
			.execute(
				"UPDATE nodes SET kind = 'BOGUS_KIND' WHERE node_uid = 'n_bad'",
				[],
			)
			.unwrap();
		let result = NodeStorePort::query_all_nodes(&storage, &snap_uid);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed node kind must propagate as Err, got {:?}",
			result
		);
	}

	#[test]
	fn query_all_nodes_errors_on_bad_subtype() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "n_bad2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:bad2:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: Some(repo_graph_indexer::types::NodeSubtype::Function),
				name: "bad2".into(),
				qualified_name: None,
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();
		storage
			.connection()
			.execute(
				"UPDATE nodes SET subtype = 'BOGUS_SUBTYPE' WHERE node_uid = 'n_bad2'",
				[],
			)
			.unwrap();
		let result = NodeStorePort::query_all_nodes(&storage, &snap_uid);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed node subtype must propagate as Err, got {:?}",
			result
		);
	}

	#[test]
	fn query_all_nodes_errors_on_bad_visibility() {
		let mut storage = setup();
		let snap_uid = make_snap(&mut storage);
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "n_bad3".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:bad3:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: None,
				name: "bad3".into(),
				qualified_name: None,
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: Some(repo_graph_indexer::types::Visibility::Export),
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();
		storage
			.connection()
			.execute(
				"UPDATE nodes SET visibility = 'BOGUS_VIS' WHERE node_uid = 'n_bad3'",
				[],
			)
			.unwrap();
		let result = NodeStorePort::query_all_nodes(&storage, &snap_uid);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed node visibility must propagate as Err, got {:?}",
			result
		);
	}

	// ── DeltaCopyPort ────────────────────────────────────────

	/// Helper: insert a tracked file for FK satisfaction.
	fn insert_tracked_file(storage: &mut StorageConnection, file_uid: &str, path: &str) {
		FileCatalogPort::upsert_files(
			storage,
			&[ixp::TrackedFile {
				file_uid: file_uid.into(),
				repo_uid: "r1".into(),
				path: path.into(),
				language: Some("typescript".into()),
				is_test: false,
				is_generated: false,
				is_excluded: false,
			}],
		)
		.unwrap();
	}

	#[test]
	fn copy_forward_copies_nodes_with_new_uids() {
		let mut storage = setup();
		let snap1 = make_snap(&mut storage);
		insert_tracked_file(&mut storage, "r1:src/a.ts", "src/a.ts");

		// Insert a node in snap1.
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "orig_n1".into(),
				snapshot_uid: snap1.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/a.ts:foo:SYMBOL".into(),
				kind: repo_graph_indexer::types::NodeKind::Symbol,
				subtype: Some(repo_graph_indexer::types::NodeSubtype::Function),
				name: "foo".into(),
				qualified_name: None,
				file_uid: Some("r1:src/a.ts".into()),
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();

		// Create snap2 and copy forward.
		let snap2 = make_snap(&mut storage);
		let result = DeltaCopyPort::copy_forward_unchanged_files(
			&mut storage,
			&CopyForwardInput {
				from_snapshot_uid: snap1.clone(),
				to_snapshot_uid: snap2.clone(),
				repo_uid: "r1".into(),
				file_uids: vec!["r1:src/a.ts".into()],
			},
		)
		.unwrap();

		assert_eq!(result.nodes_copied, 1);

		// The copied node should have a NEW uid but same stable_key.
		let snap2_nodes = NodeStorePort::query_resolver_nodes(&storage, &snap2).unwrap();
		assert_eq!(snap2_nodes.len(), 1);
		assert_ne!(snap2_nodes[0].node_uid, "orig_n1", "must get new UID");
		assert_eq!(snap2_nodes[0].stable_key, "r1:src/a.ts:foo:SYMBOL");
	}

	#[test]
	fn copy_forward_remaps_parent_node_uid() {
		let mut storage = setup();
		let snap1 = make_snap(&mut storage);
		insert_tracked_file(&mut storage, "r1:src/a.ts", "src/a.ts");

		// Insert parent + child nodes.
		NodeStorePort::insert_nodes(
			&mut storage,
			&[
				ixp::ExtractedNode {
					node_uid: "parent_n".into(),
					snapshot_uid: snap1.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/a.ts:FILE".into(),
					kind: repo_graph_indexer::types::NodeKind::File,
					subtype: None,
					name: "a.ts".into(),
					qualified_name: Some("src/a.ts".into()),
					file_uid: Some("r1:src/a.ts".into()),
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				ixp::ExtractedNode {
					node_uid: "child_n".into(),
					snapshot_uid: snap1.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/a.ts:foo:SYMBOL".into(),
					kind: repo_graph_indexer::types::NodeKind::Symbol,
					subtype: None,
					name: "foo".into(),
					qualified_name: None,
					file_uid: Some("r1:src/a.ts".into()),
					parent_node_uid: Some("parent_n".into()),
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
			],
		)
		.unwrap();

		let snap2 = make_snap(&mut storage);
		DeltaCopyPort::copy_forward_unchanged_files(
			&mut storage,
			&CopyForwardInput {
				from_snapshot_uid: snap1,
				to_snapshot_uid: snap2.clone(),
				repo_uid: "r1".into(),
				file_uids: vec!["r1:src/a.ts".into()],
			},
		)
		.unwrap();

		// Both nodes should be in snap2 with remapped parent_node_uid.
		let nodes = NodeStorePort::query_all_nodes(&storage, &snap2).unwrap();
		assert_eq!(nodes.len(), 2);
		let child = nodes.iter().find(|n| n.name == "foo").unwrap();
		let parent = nodes.iter().find(|n| n.name == "a.ts").unwrap();
		assert_eq!(
			child.parent_node_uid.as_deref(),
			Some(parent.node_uid.as_str()),
			"child's parent_node_uid must point to the NEW parent UID in snap2"
		);
	}

	#[test]
	fn copy_forward_remaps_extraction_edge_source_node_uid() {
		let mut storage = setup();
		let snap1 = make_snap(&mut storage);
		insert_tracked_file(&mut storage, "r1:src/a.ts", "src/a.ts");

		// Insert a node and an extraction edge referencing it.
		NodeStorePort::insert_nodes(
			&mut storage,
			&[ixp::ExtractedNode {
				node_uid: "src_n".into(),
				snapshot_uid: snap1.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/a.ts:FILE".into(),
				kind: repo_graph_indexer::types::NodeKind::File,
				subtype: None,
				name: "a.ts".into(),
				qualified_name: None,
				file_uid: Some("r1:src/a.ts".into()),
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
		)
		.unwrap();

		EdgeStorePort::insert_extraction_edges(
			&mut storage,
			&[ExtractionEdgeRow {
				edge_uid: "ee1".into(),
				snapshot_uid: snap1.clone(),
				repo_uid: "r1".into(),
				source_node_uid: "src_n".into(),
				target_key: "./utils".into(),
				edge_type: repo_graph_indexer::types::EdgeType::Imports,
				resolution: repo_graph_indexer::types::Resolution::Static,
				extractor: "ts-base:1".into(),
				line_start: None,
				col_start: None,
				line_end: None,
				col_end: None,
				metadata_json: None,
				source_file_uid: Some("r1:src/a.ts".into()),
			}],
		)
		.unwrap();

		let snap2 = make_snap(&mut storage);
		let result = DeltaCopyPort::copy_forward_unchanged_files(
			&mut storage,
			&CopyForwardInput {
				from_snapshot_uid: snap1,
				to_snapshot_uid: snap2.clone(),
				repo_uid: "r1".into(),
				file_uids: vec!["r1:src/a.ts".into()],
			},
		)
		.unwrap();

		assert_eq!(result.nodes_copied, 1);
		assert_eq!(result.extraction_edges_copied, 1);

		// The copied edge should reference the NEW node UID.
		let edges = EdgeStorePort::query_extraction_edges_batch(&storage, &snap2, 10, None).unwrap();
		assert_eq!(edges.len(), 1);
		assert_ne!(edges[0].edge_uid, "ee1", "edge must get new UID");
		assert_ne!(edges[0].source_node_uid, "src_n", "source must be remapped");

		// Verify the remapped source matches the copied node.
		let nodes = NodeStorePort::query_resolver_nodes(&storage, &snap2).unwrap();
		assert_eq!(edges[0].source_node_uid, nodes[0].node_uid);
	}

	#[test]
	fn copy_forward_copies_file_signals_and_versions() {
		let mut storage = setup();
		let snap1 = make_snap(&mut storage);

		// Need a tracked file for FK.
		FileCatalogPort::upsert_files(
			&mut storage,
			&[ixp::TrackedFile {
				file_uid: "r1:src/a.ts".into(),
				repo_uid: "r1".into(),
				path: "src/a.ts".into(),
				language: Some("typescript".into()),
				is_test: false,
				is_generated: false,
				is_excluded: false,
			}],
		)
		.unwrap();

		FileSignalPort::insert_file_signals(
			&mut storage,
			&[FileSignalRow {
				snapshot_uid: snap1.clone(),
				file_uid: "r1:src/a.ts".into(),
				import_bindings_json: Some("[]".into()),
				package_dependencies_json: None,
				tsconfig_aliases_json: None,
			}],
		)
		.unwrap();

		FileCatalogPort::upsert_file_versions(
			&mut storage,
			&[ixp::FileVersion {
				snapshot_uid: snap1.clone(),
				file_uid: "r1:src/a.ts".into(),
				content_hash: "h1".into(),
				ast_hash: None,
				extractor: Some("ts-base:1".into()),
				parse_status: repo_graph_indexer::types::ParseStatus::Parsed,
				size_bytes: Some(100),
				line_count: Some(10),
				indexed_at: "2025-01-01T00:00:00.000Z".into(),
			}],
		)
		.unwrap();

		let snap2 = make_snap(&mut storage);
		let result = DeltaCopyPort::copy_forward_unchanged_files(
			&mut storage,
			&CopyForwardInput {
				from_snapshot_uid: snap1,
				to_snapshot_uid: snap2.clone(),
				repo_uid: "r1".into(),
				file_uids: vec!["r1:src/a.ts".into()],
			},
		)
		.unwrap();

		assert_eq!(result.file_signals_copied, 1);
		assert_eq!(result.file_versions_copied, 1);

		// Verify signals copied to snap2.
		let signals = FileSignalPort::query_file_signals_batch(
			&storage,
			&snap2,
			&["r1:src/a.ts".into()],
		)
		.unwrap();
		assert_eq!(signals.len(), 1);

		// Verify versions copied to snap2.
		let hashes = FileCatalogPort::query_file_version_hashes(&storage, &snap2).unwrap();
		assert_eq!(hashes.get("r1:src/a.ts"), Some(&"h1".to_string()));
	}
}
