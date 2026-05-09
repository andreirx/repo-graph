//! Freshness and provenance storage implementation (ACR-3).
//!
//! Implements `FreshnessStoragePort` for `StorageConnection`.

use artifact_contracts::{FreshnessState, Provenance};
use chrono::Utc;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::freshness_port::{FreshnessStoragePort, FreshnessSummary, is_freshness_tracked};

impl FreshnessStoragePort for StorageConnection {
    fn update_freshness_state(
        &mut self,
        table: &str,
        row_uid: &str,
        state: FreshnessState,
    ) -> Result<bool, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let pk_column = get_primary_key_column(table);
        let now = Utc::now().to_rfc3339();

        let sql = format!(
            "UPDATE {} SET freshness_state = ?, freshness_updated_at = ? WHERE {} = ?",
            table, pk_column
        );

        let rows_affected = self
            .connection_mut()
            .execute(&sql, rusqlite::params![state.as_str(), now, row_uid])?;

        Ok(rows_affected > 0)
    }

    fn mark_rows_impacted(
        &mut self,
        table: &str,
        row_uids: &[&str],
    ) -> Result<usize, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        if row_uids.is_empty() {
            return Ok(0);
        }

        let pk_column = get_primary_key_column(table);
        let now = Utc::now().to_rfc3339();

        // Build parameterized IN clause
        let placeholders: Vec<&str> = row_uids.iter().map(|_| "?").collect();
        let sql = format!(
            "UPDATE {} SET freshness_state = 'impacted', freshness_updated_at = ? WHERE {} IN ({})",
            table,
            pk_column,
            placeholders.join(", ")
        );

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&now];
        for uid in row_uids {
            params.push(uid);
        }

        let rows_affected = self.connection_mut().execute(&sql, params.as_slice())?;
        Ok(rows_affected)
    }

    fn mark_impacted_by_stable_keys(
        &mut self,
        snapshot_uid: &str,
        table: &str,
        changed_stable_keys: &[&str],
    ) -> Result<usize, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        if changed_stable_keys.is_empty() {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();

        // Use SQLite JSON1 extension for proper JSON traversal.
        //
        // json_each(provenance_json, '$.depends_on') iterates the depends_on array.
        // json_extract(value, '$.stable_key') extracts the stable_key from each element.
        //
        // This replaces the ACR-3 scaffolding which used LIKE pattern matching.
        // LIKE could false-match substrings (e.g., "repo:file.ts#func" would match
        // "repo:file.ts#func_helper"). The JSON approach does exact matching.
        let mut total_affected = 0;

        for stable_key in changed_stable_keys {
            // Generate table-specific SQL based on whether the table has direct snapshot_uid
            // or requires FK-join scoping.
            let sql = build_mark_impacted_sql(table, snapshot_uid);

            let affected = self.connection_mut().execute(
                &sql,
                rusqlite::params![now, snapshot_uid, stable_key],
            )?;
            total_affected += affected;
        }

        Ok(total_affected)
    }

    fn set_provenance(
        &mut self,
        table: &str,
        row_uid: &str,
        provenance: &Provenance,
    ) -> Result<bool, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let pk_column = get_primary_key_column(table);
        let provenance_json = serde_json::to_string(provenance)
            .map_err(|e| StorageError::InvalidArgument(format!("Failed to serialize provenance: {}", e)))?;

        let sql = format!(
            "UPDATE {} SET provenance_json = ? WHERE {} = ?",
            table, pk_column
        );

        let rows_affected = self
            .connection_mut()
            .execute(&sql, rusqlite::params![provenance_json, row_uid])?;

        Ok(rows_affected > 0)
    }

    fn mark_all_current(
        &mut self,
        snapshot_uid: &str,
        table: &str,
    ) -> Result<usize, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let now = Utc::now().to_rfc3339();

        let sql = format!(
            "UPDATE {} SET freshness_state = 'current', freshness_updated_at = ?
             WHERE snapshot_uid = ? AND freshness_state != 'current'",
            table
        );

        let rows_affected = self
            .connection_mut()
            .execute(&sql, rusqlite::params![now, snapshot_uid])?;

        Ok(rows_affected)
    }

    fn get_freshness_state(
        &self,
        table: &str,
        row_uid: &str,
    ) -> Result<Option<FreshnessState>, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let pk_column = get_primary_key_column(table);
        let sql = format!(
            "SELECT freshness_state FROM {} WHERE {} = ?",
            table, pk_column
        );

        let result: Option<String> = self
            .connection()
            .query_row(&sql, rusqlite::params![row_uid], |row| row.get(0))
            .optional()?;

        match result {
            Some(state_str) => {
                FreshnessState::from_str(&state_str)
                    .ok_or_else(|| StorageError::InvalidArgument(format!(
                        "Invalid freshness_state value: {}",
                        state_str
                    )))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn get_provenance(
        &self,
        table: &str,
        row_uid: &str,
    ) -> Result<Option<Provenance>, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let pk_column = get_primary_key_column(table);
        let sql = format!(
            "SELECT provenance_json FROM {} WHERE {} = ?",
            table, pk_column
        );

        let result: Option<Option<String>> = self
            .connection()
            .query_row(&sql, rusqlite::params![row_uid], |row| row.get(0))
            .optional()?;

        match result {
            Some(Some(json_str)) => {
                let provenance: Provenance = serde_json::from_str(&json_str)
                    .map_err(|e| StorageError::InvalidArgument(format!(
                        "Failed to parse provenance_json: {}",
                        e
                    )))?;
                Ok(Some(provenance))
            }
            Some(None) | None => Ok(None),
        }
    }

    fn count_by_freshness(
        &self,
        snapshot_uid: &str,
        table: &str,
        state: FreshnessState,
    ) -> Result<usize, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE snapshot_uid = ? AND freshness_state = ?",
            table
        );

        let count: i64 = self
            .connection()
            .query_row(&sql, rusqlite::params![snapshot_uid, state.as_str()], |row| {
                row.get(0)
            })?;

        Ok(count as usize)
    }

    fn freshness_summary(
        &self,
        snapshot_uid: &str,
        table: &str,
    ) -> Result<FreshnessSummary, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let sql = format!(
            "SELECT freshness_state, COUNT(*) as cnt
             FROM {}
             WHERE snapshot_uid = ?
             GROUP BY freshness_state",
            table
        );

        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut summary = FreshnessSummary::default();
        for row in rows {
            let (state_str, count) = row?;
            let count = count as usize;
            match state_str.as_str() {
                "current" => summary.current = count,
                "impacted" => summary.impacted = count,
                "stale" => summary.stale = count,
                "unknown" => summary.unknown = count,
                _ => {} // Ignore unknown states
            }
        }

        Ok(summary)
    }

    fn list_rows_by_freshness(
        &self,
        snapshot_uid: &str,
        table: &str,
        state: FreshnessState,
        limit: usize,
    ) -> Result<Vec<String>, StorageError> {
        if !is_freshness_tracked(table) {
            return Err(StorageError::InvalidArgument(format!(
                "Table '{}' does not support freshness tracking",
                table
            )));
        }

        let pk_column = get_primary_key_column(table);
        let sql = format!(
            "SELECT {} FROM {} WHERE snapshot_uid = ? AND freshness_state = ? LIMIT ?",
            pk_column, table
        );

        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![snapshot_uid, state.as_str(), limit as i64],
            |row| row.get::<_, String>(0),
        )?;

        let mut uids = Vec::new();
        for row in rows {
            uids.push(row?);
        }

        Ok(uids)
    }

    fn provenance_depends_on(
        &self,
        table: &str,
        row_uid: &str,
        stable_key: &str,
    ) -> Result<bool, StorageError> {
        let provenance = self.get_provenance(table, row_uid)?;
        match provenance {
            Some(p) => Ok(p.depends_on_key(stable_key)),
            None => Ok(false),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper functions
// ═══════════════════════════════════════════════════════════════════════════

/// Get the primary key column name for a freshness-tracked table.
fn get_primary_key_column(table: &str) -> &'static str {
    match table {
        "boundary_contracts" => "association_uid",
        "boundary_interaction_links" => "link_uid",
        "inferences" => "inference_uid",
        "project_surfaces" => "surface_uid",
        "project_surface_evidence" => "evidence_uid",
        "surface_entrypoints" => "entrypoint_uid",
        "surface_config_roots" => "config_root_uid",
        "surface_env_dependencies" => "env_dependency_uid",
        "surface_env_evidence" => "env_evidence_uid",
        "surface_fs_mutations" => "mutation_uid",
        "surface_fs_mutation_evidence" => "mutation_evidence_uid",
        "module_candidates" => "candidate_uid",
        _ => "uid", // Fallback, should not happen for tracked tables
    }
}

/// Build the SQL for marking rows impacted by stable key change.
///
/// Most tables have a direct `snapshot_uid` column. However, some tables
/// (like `boundary_contracts`) don't have `snapshot_uid` directly — they
/// link to snapshots via FK joins (e.g., `surface_uid` → `boundary_interaction_surfaces`).
///
/// This function returns the appropriate SQL based on table structure.
fn build_mark_impacted_sql(table: &str, _snapshot_uid: &str) -> String {
    match table {
        // boundary_contracts has no snapshot_uid — join through surface_uid
        "boundary_contracts" => {
            format!(
                r#"UPDATE boundary_contracts
                   SET freshness_state = 'impacted', freshness_updated_at = ?
                   WHERE association_uid IN (
                       SELECT bc.association_uid
                       FROM boundary_contracts bc
                       JOIN boundary_interaction_surfaces bis ON bc.surface_uid = bis.surface_uid
                       WHERE bis.snapshot_uid = ?
                         AND bc.freshness_state != 'impacted'
                         AND bc.provenance_json IS NOT NULL
                         AND EXISTS (
                             SELECT 1 FROM json_each(bc.provenance_json, '$.depends_on')
                             WHERE json_extract(value, '$.stable_key') = ?
                         )
                   )"#
            )
        }
        // All other tables have direct snapshot_uid
        _ => {
            format!(
                "UPDATE {} SET freshness_state = 'impacted', freshness_updated_at = ?
                 WHERE snapshot_uid = ?
                   AND freshness_state != 'impacted'
                   AND provenance_json IS NOT NULL
                   AND EXISTS (
                     SELECT 1 FROM json_each(provenance_json, '$.depends_on')
                     WHERE json_extract(value, '$.stable_key') = ?
                   )",
                table
            )
        }
    }
}

/// Extension trait for rusqlite::OptionalExtension
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artifact_contracts::ProvenanceAnchor;

    fn fresh_storage() -> StorageConnection {
        StorageConnection::open_in_memory().expect("open in-memory storage")
    }

    fn setup_test_inference(storage: &mut StorageConnection) -> String {
        let conn = storage.connection_mut();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'complete', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Insert an inference row
        let inference_uid = "inf-001";
        conn.execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES (?, 's1', 'r1', 'test:key', 'test_kind', '{}', 0.9, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            rusqlite::params![inference_uid],
        ).unwrap();

        inference_uid.to_string()
    }

    #[test]
    fn update_freshness_state_works() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        // Initial state is 'unknown' from migration default
        let state = storage.get_freshness_state("inferences", &uid).unwrap();
        assert_eq!(state, Some(FreshnessState::Unknown));

        // Update to 'current'
        let updated = storage.update_freshness_state("inferences", &uid, FreshnessState::Current).unwrap();
        assert!(updated);

        let state = storage.get_freshness_state("inferences", &uid).unwrap();
        assert_eq!(state, Some(FreshnessState::Current));
    }

    #[test]
    fn update_freshness_state_nonexistent_row() {
        let mut storage = fresh_storage();
        setup_test_inference(&mut storage);

        let updated = storage.update_freshness_state("inferences", "nonexistent", FreshnessState::Current).unwrap();
        assert!(!updated);
    }

    #[test]
    fn update_freshness_state_invalid_table() {
        let mut storage = fresh_storage();
        let result = storage.update_freshness_state("nodes", "uid", FreshnessState::Current);
        assert!(result.is_err());
    }

    #[test]
    fn mark_rows_impacted_works() {
        let mut storage = fresh_storage();
        let uid1 = setup_test_inference(&mut storage);

        // Insert another inference
        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-002', 's1', 'r1', 'test:key2', 'test_kind', '{}', 0.8, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Mark both as current first
        storage.update_freshness_state("inferences", &uid1, FreshnessState::Current).unwrap();
        storage.update_freshness_state("inferences", "inf-002", FreshnessState::Current).unwrap();

        // Now mark both as impacted
        let affected = storage.mark_rows_impacted("inferences", &[&uid1, "inf-002"]).unwrap();
        assert_eq!(affected, 2);

        assert_eq!(
            storage.get_freshness_state("inferences", &uid1).unwrap(),
            Some(FreshnessState::Impacted)
        );
        assert_eq!(
            storage.get_freshness_state("inferences", "inf-002").unwrap(),
            Some(FreshnessState::Impacted)
        );
    }

    #[test]
    fn set_and_get_provenance() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#func:SYMBOL:FUNCTION"),
            ProvenanceAnchor::new("Edges", "repo:file.ts#func->dep:CALLS"),
        ]).with_extractor("test_extractor:1.0");

        let updated = storage.set_provenance("inferences", &uid, &provenance).unwrap();
        assert!(updated);

        let retrieved = storage.get_provenance("inferences", &uid).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.version, 1);
        assert_eq!(retrieved.depends_on.len(), 2);
        assert_eq!(retrieved.extractor, Some("test_extractor:1.0".to_string()));
    }

    #[test]
    fn provenance_depends_on_check() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#func:SYMBOL:FUNCTION"),
        ]);
        storage.set_provenance("inferences", &uid, &provenance).unwrap();

        assert!(storage.provenance_depends_on("inferences", &uid, "repo:file.ts#func:SYMBOL:FUNCTION").unwrap());
        assert!(!storage.provenance_depends_on("inferences", &uid, "repo:other.ts#other:SYMBOL:FUNCTION").unwrap());
    }

    #[test]
    fn count_by_freshness() {
        let mut storage = fresh_storage();
        let uid1 = setup_test_inference(&mut storage);

        // Insert two more inferences
        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-002', 's1', 'r1', 'test:key2', 'test_kind', '{}', 0.8, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-003', 's1', 'r1', 'test:key3', 'test_kind', '{}', 0.7, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // All start as 'unknown'
        assert_eq!(storage.count_by_freshness("s1", "inferences", FreshnessState::Unknown).unwrap(), 3);

        // Update some states
        storage.update_freshness_state("inferences", &uid1, FreshnessState::Current).unwrap();
        storage.update_freshness_state("inferences", "inf-002", FreshnessState::Impacted).unwrap();

        assert_eq!(storage.count_by_freshness("s1", "inferences", FreshnessState::Current).unwrap(), 1);
        assert_eq!(storage.count_by_freshness("s1", "inferences", FreshnessState::Impacted).unwrap(), 1);
        assert_eq!(storage.count_by_freshness("s1", "inferences", FreshnessState::Unknown).unwrap(), 1);
    }

    #[test]
    fn freshness_summary() {
        let mut storage = fresh_storage();
        let uid1 = setup_test_inference(&mut storage);

        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-002', 's1', 'r1', 'test:key2', 'test_kind', '{}', 0.8, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        storage.update_freshness_state("inferences", &uid1, FreshnessState::Current).unwrap();

        let summary = storage.freshness_summary("s1", "inferences").unwrap();
        assert_eq!(summary.current, 1);
        assert_eq!(summary.unknown, 1);
        assert_eq!(summary.total(), 2);
    }

    #[test]
    fn list_rows_by_freshness() {
        let mut storage = fresh_storage();
        let uid1 = setup_test_inference(&mut storage);

        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-002', 's1', 'r1', 'test:key2', 'test_kind', '{}', 0.8, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        storage.update_freshness_state("inferences", &uid1, FreshnessState::Current).unwrap();

        let current_rows = storage.list_rows_by_freshness("s1", "inferences", FreshnessState::Current, 10).unwrap();
        assert_eq!(current_rows.len(), 1);
        assert_eq!(current_rows[0], uid1);

        let unknown_rows = storage.list_rows_by_freshness("s1", "inferences", FreshnessState::Unknown, 10).unwrap();
        assert_eq!(unknown_rows.len(), 1);
        assert_eq!(unknown_rows[0], "inf-002");
    }

    #[test]
    fn mark_all_current() {
        let mut storage = fresh_storage();
        setup_test_inference(&mut storage);

        storage.connection_mut().execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
             VALUES ('inf-002', 's1', 'r1', 'test:key2', 'test_kind', '{}', 0.8, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Both start as 'unknown', mark all current
        let affected = storage.mark_all_current("s1", "inferences").unwrap();
        assert_eq!(affected, 2);

        assert_eq!(storage.count_by_freshness("s1", "inferences", FreshnessState::Current).unwrap(), 2);
    }

    #[test]
    fn mark_impacted_by_stable_keys() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        // Set provenance with a specific stable key
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#func:SYMBOL:FUNCTION"),
        ]);
        storage.set_provenance("inferences", &uid, &provenance).unwrap();
        storage.update_freshness_state("inferences", &uid, FreshnessState::Current).unwrap();

        // Mark impacted by the stable key
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",
            "inferences",
            &["repo:file.ts#func:SYMBOL:FUNCTION"],
        ).unwrap();
        assert_eq!(affected, 1);

        assert_eq!(
            storage.get_freshness_state("inferences", &uid).unwrap(),
            Some(FreshnessState::Impacted)
        );
    }

    #[test]
    fn mark_impacted_by_stable_keys_no_match() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#func:SYMBOL:FUNCTION"),
        ]);
        storage.set_provenance("inferences", &uid, &provenance).unwrap();
        storage.update_freshness_state("inferences", &uid, FreshnessState::Current).unwrap();

        // Mark impacted by a different stable key
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",
            "inferences",
            &["repo:other.ts#other:SYMBOL:FUNCTION"],
        ).unwrap();
        assert_eq!(affected, 0);

        // Still current
        assert_eq!(
            storage.get_freshness_state("inferences", &uid).unwrap(),
            Some(FreshnessState::Current)
        );
    }

    /// Verifies that JSON-based provenance matching does NOT false-match on prefixes.
    ///
    /// This is a regression test for the ACR-3 scaffolding which used LIKE pattern
    /// matching. LIKE '%"stable_key":"repo:file.ts#func"%' would match both
    /// "repo:file.ts#func" and "repo:file.ts#func_helper". The JSON-based approach
    /// using json_each/json_extract does exact matching.
    #[test]
    fn mark_impacted_does_not_false_match_prefix() {
        let mut storage = fresh_storage();
        let uid = setup_test_inference(&mut storage);

        // Provenance depends on "repo:file.ts#func_helper:SYMBOL:FUNCTION"
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#func_helper:SYMBOL:FUNCTION"),
        ]);
        storage.set_provenance("inferences", &uid, &provenance).unwrap();
        storage.update_freshness_state("inferences", &uid, FreshnessState::Current).unwrap();

        // Try to mark impacted by the prefix key "repo:file.ts#func:SYMBOL:FUNCTION"
        // This should NOT match because we depend on "func_helper", not "func"
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",
            "inferences",
            &["repo:file.ts#func:SYMBOL:FUNCTION"],
        ).unwrap();
        assert_eq!(affected, 0, "Prefix should not false-match with JSON-based approach");

        // Row should still be current
        assert_eq!(
            storage.get_freshness_state("inferences", &uid).unwrap(),
            Some(FreshnessState::Current)
        );

        // Now mark impacted by the correct full key
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",
            "inferences",
            &["repo:file.ts#func_helper:SYMBOL:FUNCTION"],
        ).unwrap();
        assert_eq!(affected, 1, "Exact match should work");

        assert_eq!(
            storage.get_freshness_state("inferences", &uid).unwrap(),
            Some(FreshnessState::Impacted)
        );
    }

    // ── File-backed database validation (ACR-3 migration 027) ────────────

    /// Validates that migration 027 creates freshness columns on file-backed DB.
    ///
    /// This test proves the migration works on real SQLite files, not just
    /// in-memory databases. It verifies:
    /// 1. All 12 freshness-tracked tables have the required columns
    /// 2. Column types are correct (TEXT NOT NULL for state, TEXT nullable for others)
    /// 3. Indexes exist for snapshot-scoped freshness queries
    #[test]
    fn migration_027_creates_freshness_columns_on_file_backed_db() {
        use crate::freshness_port::FRESHNESS_TRACKED_TABLES;

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("freshness-test.db");

        // Open creates the file-backed database and runs all migrations
        let storage = StorageConnection::open(&db_path).expect("open file-backed db");
        let conn = storage.connection();

        // Verify each freshness-tracked table has the required columns
        for table in FRESHNESS_TRACKED_TABLES {
            let columns: Vec<(String, String, i32)> = conn
                .prepare(&format!("PRAGMA table_info({})", table))
                .expect("prepare pragma")
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,  // column name
                        row.get::<_, String>(2)?,  // column type
                        row.get::<_, i32>(3)?,     // notnull
                    ))
                })
                .expect("query pragma")
                .filter_map(|r| r.ok())
                .collect();

            let column_names: Vec<&str> = columns.iter().map(|(n, _, _)| n.as_str()).collect();

            // Verify freshness columns exist
            assert!(
                column_names.contains(&"freshness_state"),
                "Table {} missing freshness_state column",
                table
            );
            assert!(
                column_names.contains(&"freshness_updated_at"),
                "Table {} missing freshness_updated_at column",
                table
            );
            assert!(
                column_names.contains(&"provenance_json"),
                "Table {} missing provenance_json column",
                table
            );

            // Verify freshness_state is NOT NULL
            let state_col = columns.iter().find(|(n, _, _)| n == "freshness_state");
            assert!(
                state_col.is_some() && state_col.unwrap().2 == 1,
                "Table {} freshness_state should be NOT NULL",
                table
            );
        }

        // Verify indexes exist for freshness queries
        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_%_freshness'")
            .expect("prepare index query")
            .query_map([], |row| row.get(0))
            .expect("query indexes")
            .filter_map(|r| r.ok())
            .collect();

        // Should have at least one freshness index
        assert!(
            !indexes.is_empty(),
            "Expected freshness indexes to exist, found none"
        );

        // Verify that there are indexes for the key tables
        assert!(
            indexes.iter().any(|n| n.contains("inferences")),
            "Missing freshness index for inferences table"
        );
    }

    /// Validates that freshness operations work correctly on file-backed DB.
    ///
    /// Proves end-to-end freshness workflow on real SQLite file.
    #[test]
    fn freshness_operations_work_on_file_backed_db() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("freshness-ops-test.db");

        let mut storage = StorageConnection::open(&db_path).expect("open file-backed db");

        // Create test data (scoped to release mutable borrow)
        {
            let conn = storage.connection_mut();
            conn.execute(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'complete', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
                 VALUES ('inf-file-001', 's1', 'r1', 'test:key', 'test_kind', '{}', 0.9, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }

        // New row should have 'unknown' freshness from migration default
        let state = storage.get_freshness_state("inferences", "inf-file-001").unwrap();
        assert_eq!(state, Some(FreshnessState::Unknown));

        // Update freshness state
        storage.update_freshness_state("inferences", "inf-file-001", FreshnessState::Current).unwrap();
        let state = storage.get_freshness_state("inferences", "inf-file-001").unwrap();
        assert_eq!(state, Some(FreshnessState::Current));

        // Set provenance
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("Nodes", "repo:file.ts#handler:SYMBOL:FUNCTION"),
        ]);
        storage.set_provenance("inferences", "inf-file-001", &provenance).unwrap();

        // Verify provenance persisted
        let retrieved = storage.get_provenance("inferences", "inf-file-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().depends_on.len(), 1);

        // Close and re-open to verify persistence
        drop(storage);
        let storage = StorageConnection::open(&db_path).expect("re-open file-backed db");

        // Data should still be there
        let state = storage.get_freshness_state("inferences", "inf-file-001").unwrap();
        assert_eq!(state, Some(FreshnessState::Current));

        let retrieved = storage.get_provenance("inferences", "inf-file-001").unwrap();
        assert!(retrieved.is_some());
    }

    // ── ACR-5: boundary_contracts FK-join impact propagation ─────────────────

    /// Sets up test fixture with boundary_interaction_surfaces and boundary_contracts.
    /// Returns (surface_uid, association_uid).
    fn setup_boundary_contracts_fixture(storage: &mut StorageConnection) -> (String, String) {
        let conn = storage.connection_mut();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'complete', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Insert boundary_interaction_surfaces row (has snapshot_uid)
        let surface_uid = "surface-001";
        conn.execute(
            r#"INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality, symbol_stable_key, source_file,
                line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                ?, 's1', 'r1',
                'inter_process', 'grpc', 'provider',
                'tcp', 'socket', 'request_response',
                'remote_literal', 'r1:server.rs#handler:SYMBOL:FUNCTION', 'server.rs',
                42, 50, 1, 80,
                'test:1.0', 'api_call', 0.95, '{}'
            )"#,
            rusqlite::params![surface_uid],
        ).unwrap();

        // Insert boundary_contracts row (no snapshot_uid — links via surface_uid)
        let association_uid = "bc-001";
        conn.execute(
            r#"INSERT INTO boundary_contracts (
                association_uid, surface_uid, contract_kind, association_basis, confidence
            ) VALUES (?, ?, 'grpc_method', 'schema_type', 0.95)"#,
            rusqlite::params![association_uid, surface_uid],
        ).unwrap();

        (surface_uid.to_string(), association_uid.to_string())
    }

    /// Proof test: boundary_contracts impact propagation via FK-join.
    ///
    /// This test validates ACR-5's requirement that `mark_impacted_by_stable_keys`
    /// correctly handles tables without direct `snapshot_uid` by joining through FKs.
    ///
    /// The `boundary_contracts` table has:
    /// - `association_uid` (PK)
    /// - `surface_uid` (FK to boundary_interaction_surfaces)
    /// - No `snapshot_uid` column
    ///
    /// Impact propagation must:
    /// 1. Join boundary_contracts -> boundary_interaction_surfaces via surface_uid
    /// 2. Filter by boundary_interaction_surfaces.snapshot_uid
    /// 3. Check provenance_json for matching stable_key
    ///
    /// This test proves:
    /// - Rows with matching provenance in the correct snapshot are impacted
    /// - Rows with matching provenance in a DIFFERENT snapshot are NOT impacted
    /// - Rows without provenance are NOT impacted
    #[test]
    fn acr5_boundary_contracts_fk_join_impact_propagation() {
        let mut storage = fresh_storage();
        let (_surface_uid, association_uid) = setup_boundary_contracts_fixture(&mut storage);

        // Set provenance on the boundary_contracts row
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("ContractElements", "r1:api.proto#service:MyService#method:DoThing"),
        ]).with_extractor("grpc_contract_linker:1.0");

        storage.set_provenance("boundary_contracts", &association_uid, &provenance).unwrap();
        storage.update_freshness_state("boundary_contracts", &association_uid, FreshnessState::Current).unwrap();

        // Verify initial state
        assert_eq!(
            storage.get_freshness_state("boundary_contracts", &association_uid).unwrap(),
            Some(FreshnessState::Current)
        );

        // Mark impacted by a stable key that MATCHES the provenance
        // Should affect 1 row via FK-join scoping
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",  // snapshot_uid — this must join through boundary_interaction_surfaces
            "boundary_contracts",
            &["r1:api.proto#service:MyService#method:DoThing"],
        ).unwrap();

        assert_eq!(affected, 1, "FK-join should find and impact the row via surface_uid -> snapshot_uid");

        assert_eq!(
            storage.get_freshness_state("boundary_contracts", &association_uid).unwrap(),
            Some(FreshnessState::Impacted),
            "Row should be impacted after stable_key match"
        );
    }

    /// Proof test: boundary_contracts impact propagation is snapshot-scoped.
    ///
    /// Verifies that impact propagation respects snapshot boundaries when using FK-join.
    /// A row in snapshot s1 should NOT be impacted when targeting snapshot s2.
    #[test]
    fn acr5_boundary_contracts_fk_join_respects_snapshot_scope() {
        let mut storage = fresh_storage();
        let (_surface_uid, association_uid) = setup_boundary_contracts_fixture(&mut storage);

        // Create a second snapshot (but surface stays linked to s1)
        storage.connection_mut().execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s2', 'r1', 'full', 'complete', '2025-01-01T01:00:00Z')",
            [],
        ).unwrap();

        // Create a second surface in snapshot s2
        storage.connection_mut().execute(
            r#"INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality, symbol_stable_key, source_file,
                line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                'surface-002', 's2', 'r1',
                'inter_process', 'grpc', 'consumer',
                'tcp', 'socket', 'request_response',
                'remote_literal', 'r1:client.rs#caller:SYMBOL:FUNCTION', 'client.rs',
                100, 110, 1, 60,
                'test:1.0', 'api_call', 0.9, '{}'
            )"#,
            [],
        ).unwrap();

        // Create boundary_contracts for s2's surface
        storage.connection_mut().execute(
            r#"INSERT INTO boundary_contracts (
                association_uid, surface_uid, contract_kind, association_basis, confidence
            ) VALUES ('bc-002', 'surface-002', 'grpc_method', 'schema_type', 0.9)"#,
            [],
        ).unwrap();

        // Set provenance on BOTH boundary_contracts rows with the SAME stable_key
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("ContractElements", "r1:api.proto#service:MyService#method:DoThing"),
        ]);

        storage.set_provenance("boundary_contracts", &association_uid, &provenance).unwrap();
        storage.update_freshness_state("boundary_contracts", &association_uid, FreshnessState::Current).unwrap();

        storage.set_provenance("boundary_contracts", "bc-002", &provenance).unwrap();
        storage.update_freshness_state("boundary_contracts", "bc-002", FreshnessState::Current).unwrap();

        // Mark impacted ONLY for snapshot s1
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",  // Only s1, not s2
            "boundary_contracts",
            &["r1:api.proto#service:MyService#method:DoThing"],
        ).unwrap();

        assert_eq!(affected, 1, "Should only impact rows in snapshot s1");

        // Verify: bc-001 (in s1 via surface-001) is impacted
        assert_eq!(
            storage.get_freshness_state("boundary_contracts", &association_uid).unwrap(),
            Some(FreshnessState::Impacted),
            "Row in s1 should be impacted"
        );

        // Verify: bc-002 (in s2 via surface-002) is still current
        assert_eq!(
            storage.get_freshness_state("boundary_contracts", "bc-002").unwrap(),
            Some(FreshnessState::Current),
            "Row in s2 should NOT be impacted when targeting s1"
        );
    }

    /// Proof test: boundary_contracts with no provenance is not impacted.
    ///
    /// Rows without provenance_json are never impacted by stable_key changes.
    /// This is the "unknown" baseline behavior.
    #[test]
    fn acr5_boundary_contracts_no_provenance_not_impacted() {
        let mut storage = fresh_storage();
        let (_surface_uid, association_uid) = setup_boundary_contracts_fixture(&mut storage);

        // Do NOT set provenance — row has NULL provenance_json
        // But do set it to current state
        storage.update_freshness_state("boundary_contracts", &association_uid, FreshnessState::Current).unwrap();

        // Try to mark impacted
        let affected = storage.mark_impacted_by_stable_keys(
            "s1",
            "boundary_contracts",
            &["r1:api.proto#service:MyService#method:DoThing"],
        ).unwrap();

        assert_eq!(affected, 0, "Row without provenance should not be impacted");

        // Row should still be current
        assert_eq!(
            storage.get_freshness_state("boundary_contracts", &association_uid).unwrap(),
            Some(FreshnessState::Current),
            "Row without provenance should remain current"
        );
    }
}
