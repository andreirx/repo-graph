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

use repo_graph_classification::resolve_external_dependency_name;
use repo_graph_classification::types::{
    ClassifierEdgeInput, ImportBinding, PackageDependencySet, UnresolvedEdgeCategory,
};
use repo_graph_trust::storage_port::{
    BasisCodeCountRow, ClassificationCountRow, CountByClassificationInput,
    ExternalDependencyAttribution, NamedDependencyCount, PathPrefixModuleCycle,
    QueryUnresolvedEdgesInput, TrustModuleStats, TrustStorageRead, TrustUnresolvedEdgeSample,
    UnresolvedEdgeBasisCode,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// A source file's classifier signals, deserialized from the persisted `file_signals`
/// columns for the ATTRIBUTION-1 provenance join. The stored JSON IS the camelCase
/// serialization of the classifier's own `ImportBinding` / `PackageDependencySet`
/// (`indexer/orchestrator.rs` writes `serde_json::to_string(&import_bindings)`), so it
/// deserializes straight into those types — no local mirror struct, no field-name drift.
struct FileSignalsFacts {
    /// Every import binding in the file (identifier → specifier), used to name
    /// receiver/callee-via-external-import calls.
    bindings: Vec<ImportBinding>,
    /// The file's declared package dependencies (its nearest manifest).
    declared: PackageDependencySet,
}

impl FileSignalsFacts {
    /// Empty signals — a file with no persisted `file_signals` row, or an absent/malformed
    /// JSON column. Its external references degrade honestly to "dependency not identified"
    /// (the join returns `None` for them), never a fabricated name.
    fn empty() -> Self {
        Self {
            bindings: Vec::new(),
            declared: PackageDependencySet { names: Vec::new() },
        }
    }
}

/// Load every file's classifier signals for a snapshot, keyed by `file_uid`
/// (ATTRIBUTION-1). Absent/malformed content columns degrade to empty for that file (the
/// honest-degradation contract — its external refs become "dependency not identified"); a
/// real SQL failure still propagates via `?`, never a silent skip.
fn load_file_signals(
    conn: &rusqlite::Connection,
    snapshot_uid: &str,
) -> Result<std::collections::HashMap<String, FileSignalsFacts>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT file_uid, import_bindings_json, package_dependencies_json \
         FROM file_signals \
         WHERE snapshot_uid = ?",
    )?;
    let rows = stmt.query_map([snapshot_uid], |row| {
        let file_uid: String = row.get(0)?;
        let bindings_json: Option<String> = row.get(1)?;
        let deps_json: Option<String> = row.get(2)?;
        Ok((file_uid, bindings_json, deps_json))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (file_uid, bindings_json, deps_json) = row?;
        let bindings = bindings_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<Vec<ImportBinding>>(j).ok())
            .unwrap_or_default();
        let declared = deps_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<PackageDependencySet>(j).ok())
            .unwrap_or_else(|| PackageDependencySet { names: Vec::new() });
        map.insert(file_uid, FileSignalsFacts { bindings, declared });
    }
    Ok(map)
}

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

    fn count_active_declarations(&self, repo_uid: &str, kind: &str) -> Result<usize, StorageError> {
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
            .map(serialize_enum)
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

    fn count_unresolved_edges_by_basis_code(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<BasisCodeCountRow>, StorageError> {
        // ATTRIBUTION-1: the finer companion to
        // `count_unresolved_edges_by_classification`. A read-only GROUP BY over the
        // EXISTING `basis_code` column (idx_unresolved_edges_snapshot_class covers the
        // snapshot predicate; the group is a scan over the snapshot's rows). Unfiltered
        // — the full unresolved set — so the reader-frame breakdown names every
        // reference. Deterministic ORDER BY for stable output. `basis_code` is
        // revalidated against the typed enum on read (policy-boundary validation): an
        // unknown persisted value surfaces as Err, never a silent skip.
        let mut stmt = self.connection().prepare(
            "SELECT basis_code, COUNT(*) AS count \
			 FROM unresolved_edges \
			 WHERE snapshot_uid = ? \
			 GROUP BY basis_code \
			 ORDER BY basis_code ASC",
        )?;

        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            let basis_code_str: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((basis_code_str, count))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (basis_code_str, count) = row?;
            let basis_code = deserialize_enum(&basis_code_str, "basis_code")?;
            result.push(BasisCodeCountRow {
                basis_code,
                count: count as u64,
            });
        }
        Ok(result)
    }

    fn attribute_external_dependencies(
        &self,
        snapshot_uid: &str,
        limit: u32,
    ) -> Result<ExternalDependencyAttribution, StorageError> {
        // ATTRIBUTION-1 iteration 3 (OPERATOR_NOTE 2026-07-15): the provenance JOIN that
        // replaces the review-1 GROUP BY. It names EVERY external-import unresolved
        // reference — across all three bases the classifier resolves through imports — by
        // its DECLARED dependency, reusing the classifier's own reduction so a scoped
        // specifier (`repo_graph_indexer::types`) renders as the manifest name
        // (`repo-graph-indexer`), and receiver/callee calls are named via their import
        // binding rather than degraded.
        //
        // Step 1 — the external-import unresolved edges + their source file. The basis
        // filter values come from serializing the typed enums (not literals), so they
        // cannot drift from the classifier vocabulary. LEFT JOIN so an edge whose source
        // node has no file_uid is still counted (-> unidentified), keeping the class total
        // reconciled (`total_named + unidentified` == the ExternalDependency class total).
        let specifier_basis =
            serialize_enum(&UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency)?;
        let receiver_basis =
            serialize_enum(&UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport)?;
        let callee_basis = serialize_enum(&UnresolvedEdgeBasisCode::CalleeMatchesExternalImport)?;

        let mut edge_stmt = self.connection().prepare(
            "SELECT ue.target_key, ue.metadata_json, ue.category, ue.basis_code, n.file_uid \
             FROM unresolved_edges ue \
             LEFT JOIN nodes n ON n.node_uid = ue.source_node_uid \
             WHERE ue.snapshot_uid = ? \
               AND ue.basis_code IN (?, ?, ?)",
        )?;
        let edge_rows = edge_stmt.query_map(
            rusqlite::params![snapshot_uid, specifier_basis, receiver_basis, callee_basis],
            |row| {
                let target_key: String = row.get(0)?;
                let metadata_json: Option<String> = row.get(1)?;
                let category_str: String = row.get(2)?;
                let basis_code_str: String = row.get(3)?;
                let file_uid: Option<String> = row.get(4)?;
                Ok((
                    target_key,
                    metadata_json,
                    category_str,
                    basis_code_str,
                    file_uid,
                ))
            },
        )?;
        // Materialize before opening the file-signals statement on the same connection.
        let mut edges = Vec::new();
        for row in edge_rows {
            edges.push(row?);
        }

        // Step 2 — per-file signals (import bindings + declared deps), deserialized into the
        // classifier's own types (the persisted JSON is their serialization).
        let file_signals = load_file_signals(self.connection(), snapshot_uid)?;
        let empty_signals = FileSignalsFacts::empty();

        // Step 3 — resolve each reference to its declared dependency (the classifier's own
        // reduction), aggregating named counts (deterministic BTreeMap) and the honest
        // unidentified bucket. `category`/`basis_code` are revalidated against the typed
        // enum on read (policy-boundary validation, like the sibling reads).
        let mut named: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        let mut unidentified: u64 = 0;
        for (target_key, metadata_json, category_str, basis_code_str, file_uid) in edges {
            let category: UnresolvedEdgeCategory = deserialize_enum(&category_str, "category")?;
            let basis_code: UnresolvedEdgeBasisCode =
                deserialize_enum(&basis_code_str, "basis_code")?;
            let facts = file_uid
                .as_deref()
                .and_then(|f| file_signals.get(f))
                .unwrap_or(&empty_signals);
            let edge_input = ClassifierEdgeInput {
                target_key,
                metadata_json,
            };
            match resolve_external_dependency_name(
                &edge_input,
                category,
                basis_code,
                &facts.bindings,
                &facts.declared,
            ) {
                Some(name) => *named.entry(name).or_insert(0) += 1,
                None => unidentified += 1,
            }
        }

        // Step 4 — the bounded top (count-desc, name-asc) + the reconciling totals.
        let total_named: u64 = named.values().sum();
        let mut top: Vec<NamedDependencyCount> = named
            .into_iter()
            .map(|(name, count)| NamedDependencyCount { name, count })
            .collect();
        top.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        top.truncate(limit as usize);

        Ok(ExternalDependencyAttribution {
            top,
            total_named,
            unidentified,
        })
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
        // ORIENT-BUG-1: Rewritten to use module_candidates as source of truth.
        //
        // Previous implementation started from MODULE nodes in `nodes` table,
        // which are directory-based and don't align with module_candidates.
        // This caused trust to report different module counts than orient.
        //
        // New implementation:
        // 1. Start from module_candidates (semantic module model)
        // 2. Get file counts from module_file_ownership table
        // 3. LEFT JOIN to MODULE nodes for fan_in/fan_out metrics
        // 4. Modules without matching MODULE nodes get 0 fan_in/fan_out
        //
        // The stable_key is synthesized from repo_uid and canonical_root_path
        // to match the format used by MODULE nodes (repo_uid:path:MODULE).
        let mut stmt = self.connection().prepare(
            "SELECT \
               mc.repo_uid || ':' || mc.canonical_root_path || ':MODULE' AS stable_key, \
               mc.canonical_root_path AS path, \
               COALESCE(fan_in.cnt, 0) AS fan_in, \
               COALESCE(fan_out.cnt, 0) AS fan_out, \
               COALESCE(files.cnt, 0) AS file_count \
             FROM module_candidates mc \
             LEFT JOIN nodes m ON m.snapshot_uid = mc.snapshot_uid \
               AND m.kind = 'MODULE' \
               AND m.qualified_name = mc.canonical_root_path \
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
               SELECT module_candidate_uid, COUNT(*) AS cnt \
               FROM module_file_ownership \
               WHERE snapshot_uid = ?1 \
               GROUP BY module_candidate_uid \
             ) files ON files.module_candidate_uid = mc.module_candidate_uid \
             WHERE mc.snapshot_uid = ?1 \
               AND COALESCE(files.cnt, 0) > 0 \
             ORDER BY mc.canonical_root_path",
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

    fn get_file_paths_by_repo(&self, repo_uid: &str) -> Result<Vec<String>, StorageError> {
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
        let result = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid);
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
        let result = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid);
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
        let count = TrustStorageRead::count_active_declarations(&storage, "r1", "entrypoint");
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

    /// Insert a single unresolved_edges row with an EXPLICIT `target_key` — for the
    /// ATTRIBUTION-1 named-dependency read, whose GROUP BY is over `target_key`.
    #[allow(clippy::too_many_arguments)]
    fn insert_unresolved_edge_with_target(
        storage: &StorageConnection,
        snap_uid: &str,
        edge_uid: &str,
        source_node_uid: &str,
        target_key: &str,
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
                  ?, 'CALLS', 'unresolved', 'ts-base:1', \
                  ?, ?, 1, ?, '2025-01-01T00:00:00.000Z')",
                rusqlite::params![
                    edge_uid,
                    snap_uid,
                    source_node_uid,
                    target_key,
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
            UnresolvedEdgeBasisCode, UnresolvedEdgeCategory, UnresolvedEdgeClassification,
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
        let result = TrustStorageRead::find_path_prefix_module_cycles(&storage, &snap_uid);
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

        let cycles = TrustStorageRead::find_path_prefix_module_cycles(&storage, &snap_uid).unwrap();

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

        // ORIENT-BUG-1: The query now starts from module_candidates, not nodes.
        // We need: module_candidates + module_file_ownership for file count,
        // and MODULE nodes + IMPORTS edges for fan_in/fan_out.

        // 1. Insert module_candidates (source of truth for module list)
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidates \
                 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
                  module_kind, canonical_root_path, confidence) VALUES \
                 ('mc_core', '{snap_uid}', 'r1', 'dir:src/core', 'directory', 'src/core', 1.0), \
                 ('mc_api', '{snap_uid}', 'r1', 'dir:src/api', 'directory', 'src/api', 1.0), \
                 ('mc_util', '{snap_uid}', 'r1', 'dir:src/util', 'directory', 'src/util', 1.0)"
            ))
            .unwrap();

        // 2. Insert module_file_ownership (determines file_count)
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_file_ownership \
                 (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) VALUES \
                 ('{snap_uid}', 'r1', 'r1:src/core/index.ts', 'mc_core', 'directory', 1.0), \
                 ('{snap_uid}', 'r1', 'r1:src/api/handler.ts', 'mc_api', 'directory', 1.0), \
                 ('{snap_uid}', 'r1', 'r1:src/util/helpers.ts', 'mc_util', 'directory', 1.0)"
            ))
            .unwrap();

        // 3. Insert MODULE nodes (for fan_in/fan_out via IMPORTS edges)
        // qualified_name must match canonical_root_path for the JOIN to work.
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
            ])
            .unwrap();

        // 4. Insert IMPORTS edges (determines fan_in/fan_out)
        // m_core → m_api: fan_out for m_core, fan_in for m_api
        // m_api → m_util: fan_out for m_api, fan_in for m_util
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
            ])
            .unwrap();

        let stats = TrustStorageRead::compute_module_stats(&storage, &snap_uid).unwrap();

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

        let stats = TrustStorageRead::compute_module_stats(&storage, &snap_uid).unwrap();
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
    fn count_unresolved_by_basis_code_groups_counts_and_orders() {
        // ATTRIBUTION-1: the aggregate GROUP BY over the existing `basis_code` column.
        // Two edges share a basis code (→ count 2); a third differs (→ count 1). Rows
        // come back basis_code-ASC (deterministic): `no_supporting_signal` before
        // `specifier_matches_package_dependency`.
        use repo_graph_trust::storage_port::UnresolvedEdgeBasisCode;

        let mut storage = setup();
        let snap_uid = setup_with_snapshot(&storage);
        insert_dummy_node(&mut storage, &snap_uid, "n1");

        insert_unresolved_edge(
            &storage,
            &snap_uid,
            "ue1",
            "n1",
            "external_library_candidate",
            "imports_file_not_found",
            "specifier_matches_package_dependency",
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

        let rows =
            TrustStorageRead::count_unresolved_edges_by_basis_code(&storage, &snap_uid).unwrap();
        assert_eq!(rows.len(), 2);
        // basis_code ASC.
        assert_eq!(
            rows[0].basis_code,
            UnresolvedEdgeBasisCode::NoSupportingSignal
        );
        assert_eq!(rows[0].count, 1);
        assert_eq!(
            rows[1].basis_code,
            UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency
        );
        assert_eq!(rows[1].count, 2);
    }

    #[test]
    fn count_unresolved_by_basis_code_errors_on_bad_basis_value() {
        // Policy-boundary validation (parity with the classification/category error
        // tests): an unknown persisted basis_code surfaces as Err, never a silent skip.
        let mut storage = setup();
        let snap_uid = setup_with_snapshot(&storage);
        insert_dummy_node(&mut storage, &snap_uid, "n1");

        insert_unresolved_edge(
            &storage,
            &snap_uid,
            "ue_bad_basis",
            "n1",
            "unknown",
            "calls_function_ambiguous_or_missing",
            "totally_bogus_basis_code",
        );

        let result = TrustStorageRead::count_unresolved_edges_by_basis_code(&storage, &snap_uid);
        assert!(
            matches!(result, Err(StorageError::Sqlite(_))),
            "malformed basis_code must propagate as Err(StorageError::Sqlite), got {:?}",
            result
        );
    }

    /// Upsert a `files` row (satisfies the `nodes.file_uid` FK) and a node in that file.
    fn insert_node_in_file(
        storage: &mut StorageConnection,
        snap_uid: &str,
        node_uid: &str,
        file_uid: &str,
        path: &str,
    ) {
        storage
            .upsert_files(&[crate::types::TrackedFile {
                file_uid: file_uid.into(),
                repo_uid: "r1".into(),
                path: path.into(),
                language: Some("rust".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            }])
            .unwrap();
        storage
            .insert_nodes(&[crate::types::GraphNode {
                node_uid: node_uid.into(),
                snapshot_uid: snap_uid.into(),
                repo_uid: "r1".into(),
                stable_key: format!("r1:{path}:{node_uid}:SYMBOL"),
                kind: "SYMBOL".into(),
                subtype: None,
                name: node_uid.into(),
                qualified_name: None,
                file_uid: Some(file_uid.into()),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            }])
            .unwrap();
    }

    /// Insert a `file_signals` row (the persisted import-binding + declared-dependency facts
    /// the ATTRIBUTION-1 join reads). `import_bindings_json` is the camelCase serialization
    /// the indexer writes.
    fn insert_file_signals(
        storage: &StorageConnection,
        snap_uid: &str,
        file_uid: &str,
        import_bindings_json: &str,
        package_dependencies_json: &str,
    ) {
        storage
            .connection()
            .execute(
                "INSERT INTO file_signals \
                 (snapshot_uid, file_uid, import_bindings_json, package_dependencies_json) \
                 VALUES (?, ?, ?, ?)",
                rusqlite::params![
                    snap_uid,
                    file_uid,
                    import_bindings_json,
                    package_dependencies_json
                ],
            )
            .unwrap();
    }

    #[test]
    fn attribute_external_dependencies_joins_signals_to_declared_names_end_to_end() {
        // ATTRIBUTION-1 iteration 3 (OPERATOR_NOTE 2026-07-15): the REAL provenance join —
        // from persisted file_signals (import_bindings_json + package_dependencies_json)
        // THROUGH storage to the resolved DECLARED name. No injected attribution.
        use repo_graph_trust::storage_port::NamedDependencyCount;

        let mut storage = setup();
        let snap_uid = setup_with_snapshot(&storage);
        insert_node_in_file(&mut storage, &snap_uid, "n1", "r1:src/a.rs", "src/a.rs");

        // The file's persisted signals: declared deps + the two import bindings that name the
        // receiver/callee calls. `isRelative`/`location`/`isTypeOnly` match the indexer's
        // serialized shape so this deserializes into the classifier's `ImportBinding`.
        insert_file_signals(
            &storage,
            &snap_uid,
            "r1:src/a.rs",
            r#"[{"identifier":"app","specifier":"express","isRelative":false,"location":null,"isTypeOnly":false},{"identifier":"useState","specifier":"react","isRelative":false,"location":null,"isTypeOnly":false}]"#,
            r#"{"names":["repo-graph-indexer","serde","express","react"]}"#,
        );

        // The external-import unresolved references (target_key + basis + category):
        //   serde ×3            specifier basis          → serde (bare)
        //   repo_graph_indexer::types  specifier basis   → repo-graph-indexer (scoped → declared)
        //   app.listen ×1       receiver-external        → express (via binding app→express)
        //   useState ×2         callee-external          → react   (via binding useState→react)
        //   mystery.call ×1     receiver-external        → UNIDENTIFIED (no binding for `mystery`)
        let edges: &[(&str, &str, &str)] = &[
            (
                "serde",
                "imports_file_not_found",
                "specifier_matches_package_dependency",
            ),
            (
                "serde",
                "imports_file_not_found",
                "specifier_matches_package_dependency",
            ),
            (
                "serde",
                "imports_file_not_found",
                "specifier_matches_package_dependency",
            ),
            (
                "repo_graph_indexer::types",
                "imports_file_not_found",
                "specifier_matches_package_dependency",
            ),
            (
                "app.listen",
                "calls_obj_method_needs_type_info",
                "receiver_matches_external_import",
            ),
            (
                "useState",
                "calls_function_ambiguous_or_missing",
                "callee_matches_external_import",
            ),
            (
                "useState",
                "calls_function_ambiguous_or_missing",
                "callee_matches_external_import",
            ),
            (
                "mystery.call",
                "calls_obj_method_needs_type_info",
                "receiver_matches_external_import",
            ),
        ];
        for (i, (target_key, category, basis)) in edges.iter().enumerate() {
            insert_unresolved_edge_with_target(
                &storage,
                &snap_uid,
                &format!("ue_{i}"),
                "n1",
                target_key,
                "external_library_candidate",
                category,
                basis,
            );
        }

        // Bounded top-2, count-desc then name-asc: serde(3), react(2).
        let top2 =
            TrustStorageRead::attribute_external_dependencies(&storage, &snap_uid, 2).unwrap();
        assert_eq!(
            top2.top,
            vec![
                NamedDependencyCount {
                    name: "serde".into(),
                    count: 3
                },
                NamedDependencyCount {
                    name: "react".into(),
                    count: 2
                },
            ],
            "top-2 declared deps, count-desc then name-asc"
        );

        // Full: serde(3), react(2), then the equal-count-1 pair name-asc: express, repo-graph-indexer.
        let all =
            TrustStorageRead::attribute_external_dependencies(&storage, &snap_uid, 100).unwrap();
        assert_eq!(
            all.top,
            vec![
                NamedDependencyCount {
                    name: "serde".into(),
                    count: 3
                },
                NamedDependencyCount {
                    name: "react".into(),
                    count: 2
                },
                NamedDependencyCount {
                    name: "express".into(),
                    count: 1
                },
                NamedDependencyCount {
                    name: "repo-graph-indexer".into(),
                    count: 1
                },
            ],
            "all four declared deps: receiver/callee named via binding; scoped specifier reduced"
        );
        assert_eq!(
            all.total_named, 7,
            "3 serde + 2 react + 1 express + 1 repo-graph-indexer"
        );
        assert_eq!(
            all.unidentified, 1,
            "mystery.call has no binding → dependency not identified"
        );

        // The scoped specifier renders as the DECLARED name, NEVER the import path (review-2 defect).
        assert!(
            all.top.iter().any(|d| d.name == "repo-graph-indexer"),
            "scoped `repo_graph_indexer::types` must resolve to the declared `repo-graph-indexer`"
        );
        assert!(
            !all.top.iter().any(|d| d.name.contains("::") || d.name.contains('.')),
            "no name may be an import path or call expression (repo_graph_indexer::types / app.listen)"
        );

        // Reconciliation: named + unidentified == the ExternalDependency class total (every
        // external-import edge counted once). The class total is the sum of the three
        // external bases in the basis-code aggregate.
        let basis_counts =
            TrustStorageRead::count_unresolved_edges_by_basis_code(&storage, &snap_uid).unwrap();
        let external_total: u64 = basis_counts
            .iter()
            .filter(|r| {
                matches!(
                    r.basis_code,
                    UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency
                        | UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport
                        | UnresolvedEdgeBasisCode::CalleeMatchesExternalImport
                )
            })
            .map(|r| r.count)
            .sum();
        assert_eq!(
            external_total, 8,
            "3 serde + 1 scoped + 1 app + 2 useState + 1 mystery"
        );
        assert_eq!(
            all.total_named + all.unidentified,
            external_total,
            "named + unidentified must reconcile with the ExternalDependency class total"
        );
    }

    #[test]
    fn attribute_external_dependencies_degrades_when_no_manifest_signal() {
        // Missing-name degradation (operator point 3): external-import references whose file
        // has NO persisted signals cannot be named → all `unidentified`, none `top`. No
        // fabricated name.
        let mut storage = setup();
        let snap_uid = setup_with_snapshot(&storage);
        insert_node_in_file(&mut storage, &snap_uid, "n1", "r1:src/b.rs", "src/b.rs");
        // No file_signals row inserted for src/b.rs.

        insert_unresolved_edge_with_target(
            &storage,
            &snap_uid,
            "ue_0",
            "n1",
            "serde",
            "external_library_candidate",
            "imports_file_not_found",
            "specifier_matches_package_dependency",
        );
        insert_unresolved_edge_with_target(
            &storage,
            &snap_uid,
            "ue_1",
            "n1",
            "app.listen",
            "external_library_candidate",
            "calls_obj_method_needs_type_info",
            "receiver_matches_external_import",
        );

        let attr =
            TrustStorageRead::attribute_external_dependencies(&storage, &snap_uid, 100).unwrap();
        assert!(attr.top.is_empty(), "no declared-dep facts → nothing named");
        assert_eq!(attr.total_named, 0);
        assert_eq!(
            attr.unidentified, 2,
            "both external refs degrade to 'not identified'"
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
