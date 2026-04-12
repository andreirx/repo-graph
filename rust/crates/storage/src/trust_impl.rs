//! `TrustStorageRead` implementation for `StorageConnection`.
//!
//! This module implements the trust policy crate's read port on
//! top of the storage adapter's rusqlite connection. The
//! dependency direction is adapter → policy (storage crate
//! imports and implements the trait from the trust crate), which
//! follows the Clean Architecture dependency rule.
//!
//! R4-E adds the 3 simple methods + 1 delegating method.
//! R4-F adds the 4 complex methods with real SQL implementations.
//!
//! **Error handling:** every method propagates `StorageError`
//! through the `Result` return. No silent coercion of SQL errors
//! to zero/empty. The TS adapter methods throw on real SQL
//! failures; this Rust impl matches by using `?` propagation.
//!
//! **Enum deserialization (R4-F):** the `unresolved_edges` table
//! stores `classification`, `category`, and `basis_code` as
//! snake_case TEXT values. The adapter revalidates these against
//! the typed Rust enum vocabulary on every read via serde-based
//! deserialization. A value that no longer matches the current
//! enum set surfaces as `Err(StorageError::Sqlite(
//! FromSqlConversionFailure))`, not a silent skip or partial
//! output. This is policy-boundary validation: persisted machine
//! strings are checked against the Rust classification vocabulary
//! at the adapter boundary.

use repo_graph_trust::storage_port::{
	ClassificationCountRow, CountByClassificationInput, PathPrefixModuleCycle,
	QueryUnresolvedEdgesInput, TrustModuleStats, TrustStorageRead,
	TrustUnresolvedEdgeSample,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

// ── Enum serialization helpers ────────────────────────────────
//
// These bridge SQLite's TEXT columns (snake_case string values)
// and the typed Rust enums from the classification crate. This
// is policy-boundary validation: persisted machine strings are
// revalidated against the Rust policy vocabulary on every read.
// A value that was valid when written but no longer matches the
// current enum set (e.g., after a classification vocabulary
// change) surfaces as an explicit error, not a silent skip or
// partial output.
//
// The serde rename-aware machinery is reused to avoid
// duplicating the string↔variant mappings that the
// classification crate already defines via
// `#[serde(rename_all = "snake_case")]`.

/// Deserialize a raw SQL TEXT value into a typed enum variant
/// via serde's rename-aware deserialization. Returns
/// `Err(StorageError::Sqlite(FromSqlConversionFailure))` if the
/// string does not match any known variant — the standard
/// rusqlite pattern for "SQL value cannot be converted to the
/// required Rust type."
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

/// Serialize a typed enum variant to its snake_case SQL TEXT
/// representation via serde. Returns
/// `Err(StorageError::Sqlite(ToSqlConversionFailure))` if the
/// variant does not serialize to a string (should not happen for
/// correctly-derived enums, but defended against).
fn serialize_enum<T: serde::Serialize>(val: &T) -> Result<String, StorageError> {
	match serde_json::to_value(val) {
		Ok(serde_json::Value::String(s)) => Ok(s),
		_ => Err(StorageError::Sqlite(
			rusqlite::Error::ToSqlConversionFailure(
				"enum variant did not serialize to string".into(),
			),
		)),
	}
}

impl TrustStorageRead for StorageConnection {
	type Error = StorageError;

	fn get_snapshot_extraction_diagnostics(
		&self,
		snapshot_uid: &str,
	) -> Result<Option<String>, StorageError> {
		// Mirrors TS getSnapshotExtractionDiagnostics at
		// sqlite-storage.ts:332. Returns Ok(None) for missing
		// snapshots (QueryReturnedNoRows); propagates real errors.
		let result = self.connection().query_row(
			"SELECT extraction_diagnostics_json FROM snapshots WHERE snapshot_uid = ?",
			rusqlite::params![snapshot_uid],
			|row| row.get::<_, Option<String>>(0),
		);
		match result {
			Ok(v) => Ok(v),
			Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
			Err(e) => Err(StorageError::Sqlite(e)),
		}
	}

	fn count_edges_by_type(
		&self,
		snapshot_uid: &str,
		edge_type: &str,
	) -> Result<u64, StorageError> {
		// Mirrors TS countEdgesByType at sqlite-storage.ts:2768.
		let count: i64 = self.connection().query_row(
			"SELECT COUNT(*) FROM edges WHERE snapshot_uid = ? AND type = ?",
			rusqlite::params![snapshot_uid, edge_type],
			|row| row.get(0),
		)?;
		Ok(count as u64)
	}

	fn count_active_declarations(
		&self,
		repo_uid: &str,
		kind: &str,
	) -> Result<usize, StorageError> {
		// Narrowed from TS getActiveDeclarations (returns full
		// Declaration[]) to count-only. The trust service only
		// calls .length on the result.
		let count: i64 = self.connection().query_row(
			"SELECT COUNT(*) FROM declarations WHERE repo_uid = ? AND kind = ? AND is_active = 1",
			rusqlite::params![repo_uid, kind],
			|row| row.get(0),
		)?;
		Ok(count as usize)
	}

	// ── Complex methods (R4-F) ────────────────────────────────

	fn count_unresolved_edges_by_classification(
		&self,
		input: &CountByClassificationInput,
	) -> Result<Vec<ClassificationCountRow>, StorageError> {
		// Mirrors TS countUnresolvedEdges at sqlite-storage.ts:783,
		// narrowed to group-by-classification only (the trust trait
		// removed the generic groupBy axis at R4-D).
		//
		// Dynamic IN clause: only the placeholder count is dynamic;
		// values bind positionally via typed enum serialization.
		let mut sql = String::from(
			"SELECT classification, COUNT(*) AS count \
			 FROM unresolved_edges \
			 WHERE snapshot_uid = ?",
		);
		if !input.filter_categories.is_empty() {
			let placeholders: String = input
				.filter_categories
				.iter()
				.map(|_| "?")
				.collect::<Vec<_>>()
				.join(", ");
			sql.push_str(&format!(" AND category IN ({})", placeholders));
		}
		sql.push_str(" GROUP BY classification ORDER BY classification ASC");

		// Serialize category enums to their snake_case SQL strings.
		let category_strings: Vec<String> = input
			.filter_categories
			.iter()
			.map(|c| serialize_enum(c))
			.collect::<Result<Vec<_>, _>>()?;

		// Build parameter refs: snapshot_uid + optional category strings.
		let mut params: Vec<&dyn rusqlite::types::ToSql> =
			vec![&input.snapshot_uid as &dyn rusqlite::types::ToSql];
		for s in &category_strings {
			params.push(s as &dyn rusqlite::types::ToSql);
		}

		let mut stmt = self.connection().prepare(&sql)?;
		let rows = stmt.query_map(params.as_slice(), |row| {
			let classification_str: String = row.get(0)?;
			let count: i64 = row.get(1)?;
			Ok((classification_str, count))
		})?;

		let mut result = Vec::new();
		for row in rows {
			let (classification_str, count) = row?;
			let classification = deserialize_enum(&classification_str, "classification")?;
			result.push(ClassificationCountRow {
				classification,
				count: count as u64,
			});
		}
		Ok(result)
	}

	fn query_unresolved_edges(
		&self,
		input: &QueryUnresolvedEdgesInput,
	) -> Result<Vec<TrustUnresolvedEdgeSample>, StorageError> {
		// Mirrors TS queryUnresolvedEdges at sqlite-storage.ts:662,
		// narrowed to the 4 fields TrustUnresolvedEdgeSample carries.
		// The TS version returns 12 columns; we only SELECT the 4 the
		// trust service reads. No JOIN to files since
		// source_file_path is not in the output.
		let classification_str = serialize_enum(&input.classification)?;
		let limit = input.limit as i64;

		let mut stmt = self.connection().prepare(
			"SELECT \
			   ue.category, \
			   ue.basis_code, \
			   n.visibility AS source_node_visibility, \
			   ue.metadata_json \
			 FROM unresolved_edges ue \
			 LEFT JOIN nodes n ON n.node_uid = ue.source_node_uid \
			 WHERE ue.snapshot_uid = ? \
			   AND ue.classification = ? \
			 ORDER BY ue.category ASC, ue.basis_code ASC, ue.edge_uid ASC \
			 LIMIT ?",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![input.snapshot_uid, classification_str, limit],
			|row| {
				let category_str: String = row.get(0)?;
				let basis_code_str: String = row.get(1)?;
				let visibility: Option<String> = row.get(2)?;
				let metadata_json: Option<String> = row.get(3)?;
				Ok((category_str, basis_code_str, visibility, metadata_json))
			},
		)?;

		let mut result = Vec::new();
		for row in rows {
			let (category_str, basis_code_str, visibility, metadata_json) = row?;
			let category = deserialize_enum(&category_str, "category")?;
			let basis_code = deserialize_enum(&basis_code_str, "basis_code")?;
			result.push(TrustUnresolvedEdgeSample {
				category,
				basis_code,
				source_node_visibility: visibility,
				metadata_json,
			});
		}
		Ok(result)
	}

	fn find_path_prefix_module_cycles(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<PathPrefixModuleCycle>, StorageError> {
		// Mirrors TS findPathPrefixModuleCycles at
		// sqlite-storage.ts:2777.
		//
		// CTE finds mutual (bidirectional) IMPORTS edges between
		// MODULE-kind nodes, deduplicates with node_uid ordering,
		// then filters to pairs where one module's qualified_name
		// is a strict path-prefix of the other (separated by '/').
		// The shorter-path module is the ancestor.
		let mut stmt = self.connection().prepare(
			"WITH mutual_pairs AS ( \
			   SELECT \
			     e1.source_node_uid AS a_uid, \
			     e1.target_node_uid AS b_uid \
			   FROM edges e1 \
			   JOIN edges e2 \
			     ON e2.snapshot_uid = e1.snapshot_uid \
			     AND e2.type = 'IMPORTS' \
			     AND e2.source_node_uid = e1.target_node_uid \
			     AND e2.target_node_uid = e1.source_node_uid \
			   JOIN nodes a ON a.node_uid = e1.source_node_uid \
			   JOIN nodes b ON b.node_uid = e1.target_node_uid \
			   WHERE e1.snapshot_uid = ? \
			     AND e1.type = 'IMPORTS' \
			     AND a.kind = 'MODULE' \
			     AND b.kind = 'MODULE' \
			     AND a.node_uid < b.node_uid \
			 ) \
			 SELECT \
			   CASE \
			     WHEN LENGTH(a.qualified_name) < LENGTH(b.qualified_name) \
			       THEN a.stable_key \
			     ELSE b.stable_key \
			   END AS ancestor_key, \
			   CASE \
			     WHEN LENGTH(a.qualified_name) < LENGTH(b.qualified_name) \
			       THEN b.stable_key \
			     ELSE a.stable_key \
			   END AS descendant_key \
			 FROM mutual_pairs mp \
			 JOIN nodes a ON a.node_uid = mp.a_uid \
			 JOIN nodes b ON b.node_uid = mp.b_uid \
			 WHERE \
			   (b.qualified_name LIKE a.qualified_name || '/%' \
			     AND a.qualified_name != b.qualified_name) \
			   OR \
			   (a.qualified_name LIKE b.qualified_name || '/%' \
			     AND a.qualified_name != b.qualified_name) \
			 ORDER BY ancestor_key, descendant_key",
		)?;

		let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
			Ok(PathPrefixModuleCycle {
				ancestor_stable_key: row.get(0)?,
				descendant_stable_key: row.get(1)?,
			})
		})?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	fn compute_module_stats(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<TrustModuleStats>, StorageError> {
		// Mirrors TS computeModuleStats at sqlite-storage.ts:2846,
		// simplified to the 5 fields TrustModuleStats carries.
		// The TS version computes 10 fields with 12 parameter
		// bindings (symbol_count, abstract_count, type_count
		// correlated subqueries + instability, abstractness,
		// distance_from_main_sequence derived computations).
		// The trust service only reads stable_key, path, fan_in,
		// fan_out, file_count, so the 3 correlated subqueries and
		// derived metrics are dropped. 6 → 1 parameter bindings
		// via numbered parameter ?1.
		//
		// Null guard: `m.qualified_name IS NOT NULL` excludes
		// MODULE nodes that lack a qualified_name. Without this,
		// the row mapper would need to coerce NULL to empty string
		// (silent data fabrication) or fail on every such row.
		let mut stmt = self.connection().prepare(
			"SELECT \
			   m.stable_key, \
			   m.qualified_name AS path, \
			   COALESCE(fan_in.cnt, 0) AS fan_in, \
			   COALESCE(fan_out.cnt, 0) AS fan_out, \
			   COALESCE(files.cnt, 0) AS file_count \
			 FROM nodes m \
			 LEFT JOIN ( \
			   SELECT target_node_uid AS nid, COUNT(DISTINCT source_node_uid) AS cnt \
			   FROM edges \
			   WHERE snapshot_uid = ?1 AND type = 'IMPORTS' \
			     AND source_node_uid IN ( \
			       SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND kind = 'MODULE' \
			     ) \
			   GROUP BY target_node_uid \
			 ) fan_in ON fan_in.nid = m.node_uid \
			 LEFT JOIN ( \
			   SELECT source_node_uid AS nid, COUNT(DISTINCT target_node_uid) AS cnt \
			   FROM edges \
			   WHERE snapshot_uid = ?1 AND type = 'IMPORTS' \
			     AND target_node_uid IN ( \
			       SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND kind = 'MODULE' \
			     ) \
			   GROUP BY source_node_uid \
			 ) fan_out ON fan_out.nid = m.node_uid \
			 LEFT JOIN ( \
			   SELECT source_node_uid AS nid, COUNT(*) AS cnt \
			   FROM edges \
			   WHERE snapshot_uid = ?1 AND type = 'OWNS' \
			   GROUP BY source_node_uid \
			 ) files ON files.nid = m.node_uid \
			 WHERE m.snapshot_uid = ?1 AND m.kind = 'MODULE' \
			   AND m.qualified_name IS NOT NULL \
			   AND COALESCE(files.cnt, 0) > 0 \
			 ORDER BY m.qualified_name",
		)?;

		let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
			Ok(TrustModuleStats {
				stable_key: row.get(0)?,
				path: row.get(1)?,
				fan_in: row.get::<_, i64>(2).map(|v| v as u64)?,
				fan_out: row.get::<_, i64>(3).map(|v| v as u64)?,
				file_count: row.get::<_, i64>(4).map(|v| v as u64)?,
			})
		})?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	fn get_file_paths_by_repo(
		&self,
		repo_uid: &str,
	) -> Result<Vec<String>, StorageError> {
		// Narrowed from getFilesByRepo -> TrackedFile[] to
		// paths-only. Reuses the existing get_files_by_repo CRUD
		// method and extracts .path from each.
		let files = self.get_files_by_repo(repo_uid)?;
		Ok(files.into_iter().map(|f| f.path).collect())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::{CreateSnapshotInput, Repo, TrackedFile};

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

	fn setup_with_snapshot(storage: &StorageConnection) -> String {
		let snap = storage
			.create_snapshot(&CreateSnapshotInput {
				repo_uid: "r1".into(),
				kind: "full".into(),
				basis_ref: None,
				basis_commit: None,
				parent_snapshot_uid: None,
				label: None,
				toolchain_json: None,
			})
			.unwrap();
		snap.snapshot_uid
	}

	// ── get_snapshot_extraction_diagnostics ────────────────────

	#[test]
	fn extraction_diagnostics_returns_none_for_missing_snapshot() {
		let storage = setup();
		let result: Result<Option<String>, _> =
			TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, "nonexistent");
		assert_eq!(result.unwrap(), None);
	}

	#[test]
	fn extraction_diagnostics_returns_none_when_column_is_null() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		// Snapshot exists but extraction_diagnostics_json is NULL
		// (no diagnostics written yet).
		let result =
			TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid);
		assert_eq!(result.unwrap(), None);
	}

	#[test]
	fn extraction_diagnostics_returns_json_when_set() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		// Manually set extraction_diagnostics_json.
		storage
			.connection()
			.execute(
				"UPDATE snapshots SET extraction_diagnostics_json = ? WHERE snapshot_uid = ?",
				rusqlite::params!["{\"diagnostics_version\":1}", snap_uid],
			)
			.unwrap();
		let result =
			TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid);
		assert_eq!(
			result.unwrap(),
			Some("{\"diagnostics_version\":1}".to_string())
		);
	}

	// ── count_edges_by_type ───────────────────────────────────

	#[test]
	fn count_edges_by_type_returns_zero_for_empty_snapshot() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		let count = TrustStorageRead::count_edges_by_type(&storage, &snap_uid, "CALLS");
		assert_eq!(count.unwrap(), 0);
	}

	// ── count_active_declarations ─────────────────────────────

	#[test]
	fn count_active_declarations_returns_zero_when_none_exist() {
		let storage = setup();
		let count =
			TrustStorageRead::count_active_declarations(&storage, "r1", "entrypoint");
		assert_eq!(count.unwrap(), 0);
	}

	// ── get_file_paths_by_repo ────────────────────────────────

	#[test]
	fn get_file_paths_excludes_is_excluded_files() {
		let mut storage = setup();
		storage
			.upsert_files(&[
				TrackedFile {
					file_uid: "r1:src/a.ts".into(),
					repo_uid: "r1".into(),
					path: "src/a.ts".into(),
					language: Some("typescript".into()),
					is_test: false,
					is_generated: false,
					is_excluded: false,
				},
				TrackedFile {
					file_uid: "r1:node_modules/x.ts".into(),
					repo_uid: "r1".into(),
					path: "node_modules/x.ts".into(),
					language: Some("typescript".into()),
					is_test: false,
					is_generated: false,
					is_excluded: true,
				},
			])
			.unwrap();
		let paths = TrustStorageRead::get_file_paths_by_repo(&storage, "r1").unwrap();
		assert_eq!(paths, vec!["src/a.ts".to_string()]);
	}

	// ── real SQL error propagation ────────────────────────────

	#[test]
	fn count_edges_by_type_propagates_sql_error_through_adapter() {
		// Exercises the ACTUAL TrustStorageRead::count_edges_by_type
		// adapter path and asserts Err(StorageError::Sqlite(_)) when
		// the underlying SQL fails. If a future change re-introduces
		// error-coercion (e.g., catching the error and returning
		// Ok(0)), this test fails.
		//
		// Setup: open a real StorageConnection (runs migrations,
		// creates edges table), then DROP the edges table to force
		// the adapter's SELECT COUNT(*) FROM edges to fail.
		let storage = setup();
		storage
			.connection()
			.execute("DROP TABLE edges", [])
			.unwrap();
		let result = TrustStorageRead::count_edges_by_type(&storage, "s1", "CALLS");
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"real SQL error must propagate as Err(StorageError::Sqlite), got {:?}",
			result
		);
	}

	// ── count_unresolved_edges_by_classification ─────────────

	/// Insert a minimal SYMBOL node into the nodes table. Used to
	/// satisfy FK constraints when inserting unresolved_edges that
	/// reference a source_node_uid.
	fn insert_dummy_node(storage: &mut StorageConnection, snap_uid: &str, node_uid: &str) {
		storage
			.insert_nodes(&[crate::types::GraphNode {
				node_uid: node_uid.into(),
				snapshot_uid: snap_uid.into(),
				repo_uid: "r1".into(),
				stable_key: format!("r1:dummy:{}:SYMBOL", node_uid),
				kind: "SYMBOL".into(),
				subtype: None,
				name: "dummy".into(),
				qualified_name: None,
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}])
			.unwrap();
	}

	/// Insert a single unresolved_edges row. All NOT NULL columns
	/// are provided; the classification, category, and basis_code
	/// are raw strings (not typed enums) so tests can inject
	/// malformed values for regression coverage.
	fn insert_unresolved_edge(
		storage: &StorageConnection,
		snap_uid: &str,
		edge_uid: &str,
		source_node_uid: &str,
		classification: &str,
		category: &str,
		basis_code: &str,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO unresolved_edges \
				 (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
				  target_key, type, resolution, extractor, \
				  category, classification, classifier_version, \
				  basis_code, observed_at) \
				 VALUES (?, ?, 'r1', ?, \
				  'target::key', 'CALLS', 'unresolved', 'ts-base:1', \
				  ?, ?, 1, ?, '2025-01-01T00:00:00.000Z')",
				rusqlite::params![
					edge_uid,
					snap_uid,
					source_node_uid,
					category,
					classification,
					basis_code
				],
			)
			.unwrap();
	}

	#[test]
	fn count_unresolved_by_classification_empty_snapshot() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		let result = TrustStorageRead::count_unresolved_edges_by_classification(
			&storage,
			&CountByClassificationInput {
				snapshot_uid: snap_uid,
				filter_categories: vec![],
			},
		);
		assert_eq!(result.unwrap(), vec![]);
	}

	#[test]
	fn count_unresolved_by_classification_groups_correctly() {
		use repo_graph_trust::storage_port::UnresolvedEdgeClassification;

		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		insert_dummy_node(&mut storage, &snap_uid, "n1");

		// Insert 3 edges: 2 external_library_candidate, 1 unknown.
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue1",
			"n1",
			"external_library_candidate",
			"calls_function_ambiguous_or_missing",
			"callee_matches_external_import",
		);
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue2",
			"n1",
			"external_library_candidate",
			"imports_file_not_found",
			"specifier_matches_package_dependency",
		);
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue3",
			"n1",
			"unknown",
			"calls_function_ambiguous_or_missing",
			"no_supporting_signal",
		);

		let rows = TrustStorageRead::count_unresolved_edges_by_classification(
			&storage,
			&CountByClassificationInput {
				snapshot_uid: snap_uid,
				filter_categories: vec![],
			},
		)
		.unwrap();

		// ORDER BY classification ASC → external_library_candidate, unknown
		assert_eq!(rows.len(), 2);
		assert_eq!(
			rows[0].classification,
			UnresolvedEdgeClassification::ExternalLibraryCandidate
		);
		assert_eq!(rows[0].count, 2);
		assert_eq!(
			rows[1].classification,
			UnresolvedEdgeClassification::Unknown
		);
		assert_eq!(rows[1].count, 1);
	}

	#[test]
	fn count_unresolved_by_classification_filters_by_category() {
		use repo_graph_trust::storage_port::{
			UnresolvedEdgeCategory, UnresolvedEdgeClassification,
		};

		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		insert_dummy_node(&mut storage, &snap_uid, "n1");

		// One edge in calls_function category, one in imports_file category.
		// Both classified as external_library_candidate.
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue1",
			"n1",
			"external_library_candidate",
			"calls_function_ambiguous_or_missing",
			"callee_matches_external_import",
		);
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue2",
			"n1",
			"external_library_candidate",
			"imports_file_not_found",
			"specifier_matches_package_dependency",
		);

		// Filter to only imports_file_not_found.
		let rows = TrustStorageRead::count_unresolved_edges_by_classification(
			&storage,
			&CountByClassificationInput {
				snapshot_uid: snap_uid,
				filter_categories: vec![UnresolvedEdgeCategory::ImportsFileNotFound],
			},
		)
		.unwrap();

		assert_eq!(rows.len(), 1);
		assert_eq!(
			rows[0].classification,
			UnresolvedEdgeClassification::ExternalLibraryCandidate
		);
		assert_eq!(rows[0].count, 1);
	}

	// ── query_unresolved_edges ───────────────────────────────

	#[test]
	fn query_unresolved_edges_empty_snapshot() {
		use repo_graph_trust::storage_port::UnresolvedEdgeClassification;

		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		let result = TrustStorageRead::query_unresolved_edges(
			&storage,
			&QueryUnresolvedEdgesInput {
				snapshot_uid: snap_uid,
				classification: UnresolvedEdgeClassification::Unknown,
				limit: 10,
			},
		);
		assert_eq!(result.unwrap(), vec![]);
	}

	#[test]
	fn query_unresolved_edges_returns_typed_samples_with_visibility() {
		use repo_graph_trust::storage_port::{
			UnresolvedEdgeBasisCode, UnresolvedEdgeCategory,
			UnresolvedEdgeClassification,
		};

		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);

		// Insert a node so the LEFT JOIN resolves visibility.
		storage
			.insert_nodes(&[crate::types::GraphNode {
				node_uid: "n1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/a.ts:myFunc:SYMBOL".into(),
				kind: "SYMBOL".into(),
				subtype: Some("FUNCTION".into()),
				name: "myFunc".into(),
				qualified_name: Some("src/a.ts:myFunc".into()),
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: Some("export".into()),
				doc_comment: None,
				metadata_json: None,
			}])
			.unwrap();

		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue1",
			"n1",
			"external_library_candidate",
			"calls_function_ambiguous_or_missing",
			"callee_matches_external_import",
		);

		let samples = TrustStorageRead::query_unresolved_edges(
			&storage,
			&QueryUnresolvedEdgesInput {
				snapshot_uid: snap_uid,
				classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
				limit: 10,
			},
		)
		.unwrap();

		assert_eq!(samples.len(), 1);
		assert_eq!(
			samples[0].category,
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
		);
		assert_eq!(
			samples[0].basis_code,
			UnresolvedEdgeBasisCode::CalleeMatchesExternalImport
		);
		assert_eq!(
			samples[0].source_node_visibility,
			Some("export".to_string())
		);
		assert_eq!(samples[0].metadata_json, None);
	}

	// ── find_path_prefix_module_cycles ────────────────────────

	#[test]
	fn find_path_prefix_module_cycles_empty_snapshot() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		let result =
			TrustStorageRead::find_path_prefix_module_cycles(&storage, &snap_uid);
		assert_eq!(result.unwrap(), vec![]);
	}

	#[test]
	fn find_path_prefix_module_cycles_detects_ancestor_descendant() {
		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);

		// Create two MODULE nodes: src/core (ancestor) and
		// src/core/api (descendant). The qualified_name establishes
		// the path-prefix relationship.
		storage
			.insert_nodes(&[
				crate::types::GraphNode {
					node_uid: "m1".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/core:MODULE".into(),
					kind: "MODULE".into(),
					subtype: None,
					name: "core".into(),
					qualified_name: Some("src/core".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				crate::types::GraphNode {
					node_uid: "m2".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/core/api:MODULE".into(),
					kind: "MODULE".into(),
					subtype: None,
					name: "api".into(),
					qualified_name: Some("src/core/api".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
			])
			.unwrap();

		// Create mutual IMPORTS edges (m1 → m2 and m2 → m1).
		storage
			.insert_edges(&[
				crate::types::GraphEdge {
					edge_uid: "e1".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m1".into(),
					target_node_uid: "m2".into(),
					edge_type: "IMPORTS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
				crate::types::GraphEdge {
					edge_uid: "e2".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m2".into(),
					target_node_uid: "m1".into(),
					edge_type: "IMPORTS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
			])
			.unwrap();

		let cycles =
			TrustStorageRead::find_path_prefix_module_cycles(&storage, &snap_uid)
				.unwrap();

		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0].ancestor_stable_key, "r1:src/core:MODULE");
		assert_eq!(cycles[0].descendant_stable_key, "r1:src/core/api:MODULE");
	}

	// ── compute_module_stats ─────────────────────────────────

	#[test]
	fn compute_module_stats_empty_snapshot() {
		let storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		let result = TrustStorageRead::compute_module_stats(&storage, &snap_uid);
		assert_eq!(result.unwrap(), vec![]);
	}

	#[test]
	fn compute_module_stats_returns_fan_in_fan_out_file_count() {
		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);

		// Create 3 MODULE nodes and 1 FILE node.
		// m_core imports m_api (fan_out for m_core, fan_in for m_api).
		// m_api imports m_util (fan_out for m_api, fan_in for m_util).
		// m_core OWNS file f1 (file_count for m_core).
		// m_api OWNS nothing → excluded by file_count > 0 guard
		//   (we add an OWNS for m_api too to include it).
		storage
			.insert_nodes(&[
				crate::types::GraphNode {
					node_uid: "m_core".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/core:MODULE".into(),
					kind: "MODULE".into(),
					subtype: None,
					name: "core".into(),
					qualified_name: Some("src/core".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				crate::types::GraphNode {
					node_uid: "m_api".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/api:MODULE".into(),
					kind: "MODULE".into(),
					subtype: None,
					name: "api".into(),
					qualified_name: Some("src/api".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				crate::types::GraphNode {
					node_uid: "m_util".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/util:MODULE".into(),
					kind: "MODULE".into(),
					subtype: None,
					name: "util".into(),
					qualified_name: Some("src/util".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				// FILE nodes targeted by OWNS edges. file_uid is
				// None because these are graph nodes (the OWNS edge
				// links by node_uid), and the files table entries
				// are not needed for this test.
				crate::types::GraphNode {
					node_uid: "f1".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/core/index.ts:FILE".into(),
					kind: "FILE".into(),
					subtype: None,
					name: "index.ts".into(),
					qualified_name: Some("src/core/index.ts".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				crate::types::GraphNode {
					node_uid: "f2".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/api/handler.ts:FILE".into(),
					kind: "FILE".into(),
					subtype: None,
					name: "handler.ts".into(),
					qualified_name: Some("src/api/handler.ts".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
				crate::types::GraphNode {
					node_uid: "f3".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					stable_key: "r1:src/util/helpers.ts:FILE".into(),
					kind: "FILE".into(),
					subtype: None,
					name: "helpers.ts".into(),
					qualified_name: Some("src/util/helpers.ts".into()),
					file_uid: None,
					parent_node_uid: None,
					location: None,
					signature: None,
					visibility: None,
					doc_comment: None,
					metadata_json: None,
				},
			])
			.unwrap();

		// IMPORTS: m_core → m_api, m_api → m_util
		// OWNS: m_core → f1, m_api → f2, m_util → f3
		storage
			.insert_edges(&[
				crate::types::GraphEdge {
					edge_uid: "e_imp1".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m_core".into(),
					target_node_uid: "m_api".into(),
					edge_type: "IMPORTS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
				crate::types::GraphEdge {
					edge_uid: "e_imp2".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m_api".into(),
					target_node_uid: "m_util".into(),
					edge_type: "IMPORTS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
				crate::types::GraphEdge {
					edge_uid: "e_own1".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m_core".into(),
					target_node_uid: "f1".into(),
					edge_type: "OWNS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
				crate::types::GraphEdge {
					edge_uid: "e_own2".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m_api".into(),
					target_node_uid: "f2".into(),
					edge_type: "OWNS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
				crate::types::GraphEdge {
					edge_uid: "e_own3".into(),
					snapshot_uid: snap_uid.clone(),
					repo_uid: "r1".into(),
					source_node_uid: "m_util".into(),
					target_node_uid: "f3".into(),
					edge_type: "OWNS".into(),
					resolution: "static".into(),
					extractor: "ts-base:1".into(),
					location: None,
					metadata_json: None,
				},
			])
			.unwrap();

		let stats =
			TrustStorageRead::compute_module_stats(&storage, &snap_uid).unwrap();

		// ORDER BY qualified_name: src/api, src/core, src/util
		assert_eq!(stats.len(), 3);

		// src/api: fan_in=1 (from m_core), fan_out=1 (to m_util), file_count=1
		assert_eq!(stats[0].stable_key, "r1:src/api:MODULE");
		assert_eq!(stats[0].path, "src/api");
		assert_eq!(stats[0].fan_in, 1);
		assert_eq!(stats[0].fan_out, 1);
		assert_eq!(stats[0].file_count, 1);

		// src/core: fan_in=0, fan_out=1 (to m_api), file_count=1
		assert_eq!(stats[1].stable_key, "r1:src/core:MODULE");
		assert_eq!(stats[1].path, "src/core");
		assert_eq!(stats[1].fan_in, 0);
		assert_eq!(stats[1].fan_out, 1);
		assert_eq!(stats[1].file_count, 1);

		// src/util: fan_in=1 (from m_api), fan_out=0, file_count=1
		assert_eq!(stats[2].stable_key, "r1:src/util:MODULE");
		assert_eq!(stats[2].path, "src/util");
		assert_eq!(stats[2].fan_in, 1);
		assert_eq!(stats[2].fan_out, 0);
		assert_eq!(stats[2].file_count, 1);
	}

	#[test]
	fn compute_module_stats_excludes_modules_with_no_owned_files() {
		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);

		// Create a MODULE with no OWNS edges → should be excluded.
		storage
			.insert_nodes(&[crate::types::GraphNode {
				node_uid: "m_empty".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				stable_key: "r1:src/empty:MODULE".into(),
				kind: "MODULE".into(),
				subtype: None,
				name: "empty".into(),
				qualified_name: Some("src/empty".into()),
				file_uid: None,
				parent_node_uid: None,
				location: None,
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}])
			.unwrap();

		let stats =
			TrustStorageRead::compute_module_stats(&storage, &snap_uid).unwrap();
		assert_eq!(stats.len(), 0);
	}

	// ── malformed enum regression tests ──────────────────────
	//
	// These prove the adapter returns Err(StorageError::Sqlite(_))
	// when the DB contains classification/category/basis_code
	// values that do not match the typed Rust enum vocabulary.
	// Without these, a future vocabulary change could silently
	// produce partial output instead of an explicit error.

	#[test]
	fn count_unresolved_by_classification_errors_on_bad_classification_value() {
		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		insert_dummy_node(&mut storage, &snap_uid, "n1");

		// Insert a row with a classification value that is not in
		// the UnresolvedEdgeClassification enum.
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue_bad",
			"n1",
			"totally_bogus_classification",
			"calls_function_ambiguous_or_missing",
			"no_supporting_signal",
		);

		let result = TrustStorageRead::count_unresolved_edges_by_classification(
			&storage,
			&CountByClassificationInput {
				snapshot_uid: snap_uid,
				filter_categories: vec![],
			},
		);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed classification must propagate as Err(StorageError::Sqlite), got {:?}",
			result
		);
	}

	#[test]
	fn query_unresolved_edges_errors_on_bad_category_value() {
		use repo_graph_trust::storage_port::UnresolvedEdgeClassification;

		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		insert_dummy_node(&mut storage, &snap_uid, "n1");

		// Insert a row with a valid classification but invalid category.
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue_bad",
			"n1",
			"unknown",
			"not_a_real_category",
			"no_supporting_signal",
		);

		let result = TrustStorageRead::query_unresolved_edges(
			&storage,
			&QueryUnresolvedEdgesInput {
				snapshot_uid: snap_uid,
				classification: UnresolvedEdgeClassification::Unknown,
				limit: 10,
			},
		);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed category must propagate as Err(StorageError::Sqlite), got {:?}",
			result
		);
	}

	#[test]
	fn query_unresolved_edges_errors_on_bad_basis_code_value() {
		use repo_graph_trust::storage_port::UnresolvedEdgeClassification;

		let mut storage = setup();
		let snap_uid = setup_with_snapshot(&storage);
		insert_dummy_node(&mut storage, &snap_uid, "n1");

		// Insert a row with valid classification and category but
		// invalid basis_code.
		insert_unresolved_edge(
			&storage,
			&snap_uid,
			"ue_bad",
			"n1",
			"unknown",
			"calls_function_ambiguous_or_missing",
			"not_a_real_basis_code",
		);

		let result = TrustStorageRead::query_unresolved_edges(
			&storage,
			&QueryUnresolvedEdgesInput {
				snapshot_uid: snap_uid,
				classification: UnresolvedEdgeClassification::Unknown,
				limit: 10,
			},
		);
		assert!(
			matches!(result, Err(StorageError::Sqlite(_))),
			"malformed basis_code must propagate as Err(StorageError::Sqlite), got {:?}",
			result
		);
	}
}
