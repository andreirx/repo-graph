//! CRUD methods for the `inferences` table.
//!
//! Batch insert for framework-liveness inferences computed during
//! or after extraction (e.g., `spring_container_managed`,
//! `framework_entrypoint`).
//!
//! Transaction-wrapped: yes (batch insert).

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::InferenceInput;

impl StorageConnection {
    /// Delete inferences by kind for a snapshot.
    ///
    /// Called before inserting replacement inferences to ensure
    /// idempotency across re-indexing.
    ///
    /// Only deletes inferences of the specified kinds. Other inference
    /// kinds are untouched.
    pub fn delete_inferences_by_kind(
        &self,
        snapshot_uid: &str,
        kinds: &[&str],
    ) -> Result<u64, StorageError> {
        if kinds.is_empty() {
            return Ok(0);
        }

        // Build placeholders for IN clause
        let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
        let sql = format!(
            "DELETE FROM inferences WHERE snapshot_uid = ? AND kind IN ({})",
            placeholders.join(", ")
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql)?;

        // Bind snapshot_uid as first param, then each kind
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
        params.push(&snapshot_uid);
        for kind in kinds {
            params.push(kind);
        }

        let deleted = stmt.execute(params.as_slice())?;
        Ok(deleted as u64)
    }

    /// Delete inferences by kind and target file paths.
    ///
    /// Deletes inferences of the specified kind whose `target_stable_key`
    /// matches any of the given file paths. Used during refresh to clear
    /// inferences for changed files before re-computing them.
    ///
    /// Uses exact prefix matching with `substr()` to avoid LIKE pattern
    /// false-matches (e.g., `path` matching `pathology`). The prefix
    /// `{repo_uid}:{path}#` is checked exactly, ensuring the `#` delimiter
    /// is at the correct position.
    ///
    /// Returns the total number of deleted rows.
    pub fn delete_inferences_by_kind_and_files(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        kind: &str,
        file_paths: &[&str],
    ) -> Result<u64, StorageError> {
        if file_paths.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut total_deleted: u64 = 0;

        for path in file_paths {
            // Exact prefix match for SYMBOL nodes: "repo:path#symbol:SYMBOL:type"
            // Using substr() instead of LIKE to avoid false-matching path prefixes
            // (e.g., "src/A.java" should not match "src/A.java2" or "src/A.javaX")
            let prefix = format!("{}:{}#", repo_uid, path);
            let prefix_len = prefix.len() as i64;
            let deleted = conn.execute(
                "DELETE FROM inferences
                 WHERE snapshot_uid = ?
                   AND kind = ?
                   AND substr(target_stable_key, 1, ?) = ?",
                rusqlite::params![snapshot_uid, kind, prefix_len, prefix],
            )?;
            total_deleted += deleted as u64;
        }

        Ok(total_deleted)
    }

    /// Batch insert inferences. Transaction-wrapped.
    ///
    /// Called by the compose layer after indexing to persist
    /// framework-liveness inferences (e.g., Spring container-managed
    /// symbols, Lambda entrypoints).
    ///
    /// No collision detection — inferences are keyed by generated
    /// UUID, not by target_stable_key. Multiple inferences for the
    /// same target are valid (though uncommon in practice).
    pub fn insert_inferences(
        &mut self,
        inferences: &[InferenceInput],
    ) -> Result<(), StorageError> {
        if inferences.is_empty() {
            return Ok(());
        }

        let tx = self.connection_mut().transaction()?;

        {
            // Include provenance_json and freshness_state (ACR-3/4).
            // freshness_state defaults to 'current' when provenance is populated,
            // 'unknown' when provenance is NULL.
            let mut stmt = tx.prepare(
                "INSERT INTO inferences
                 (inference_uid, snapshot_uid, repo_uid, target_stable_key,
                  kind, value_json, confidence, basis_json, extractor, created_at,
                  provenance_json, freshness_state)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                         CASE WHEN ?11 IS NOT NULL THEN 'current' ELSE 'unknown' END)",
            )?;

            for i in inferences {
                stmt.execute(rusqlite::params![
                    i.inference_uid,
                    i.snapshot_uid,
                    i.repo_uid,
                    i.target_stable_key,
                    i.kind,
                    i.value_json,
                    i.confidence,
                    i.basis_json,
                    i.extractor,
                    i.created_at,
                    i.provenance_json,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Atomically replace inferences of specified kinds for a snapshot.
    ///
    /// Deletes all existing inferences of the specified kinds, then
    /// inserts the new inferences, all within a single transaction.
    ///
    /// If insert fails (e.g., primary key collision), the delete is
    /// rolled back and no data is lost.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot to replace inferences for
    /// * `kinds` - Inference kinds to delete before insert
    /// * `inferences` - New inferences to insert
    ///
    /// # Returns
    /// * `Ok(deleted_count)` - Number of inferences deleted before insert
    /// * `Err(StorageError)` - On any failure (transaction rolled back)
    pub fn replace_inferences_by_kind(
        &mut self,
        snapshot_uid: &str,
        kinds: &[&str],
        inferences: &[InferenceInput],
    ) -> Result<u64, StorageError> {
        let tx = self.connection_mut().transaction()?;

        // Delete existing inferences of specified kinds
        let deleted = if kinds.is_empty() {
            0
        } else {
            let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
            let sql = format!(
                "DELETE FROM inferences WHERE snapshot_uid = ? AND kind IN ({})",
                placeholders.join(", ")
            );
            let mut stmt = tx.prepare(&sql)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(kinds.len() + 1);
            params.push(&snapshot_uid);
            for kind in kinds {
                params.push(kind);
            }
            stmt.execute(params.as_slice())? as u64
        };

        // Insert new inferences with provenance (ACR-3/4)
        if !inferences.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO inferences
                 (inference_uid, snapshot_uid, repo_uid, target_stable_key,
                  kind, value_json, confidence, basis_json, extractor, created_at,
                  provenance_json, freshness_state)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                         CASE WHEN ?11 IS NOT NULL THEN 'current' ELSE 'unknown' END)",
            )?;

            for i in inferences {
                stmt.execute(rusqlite::params![
                    i.inference_uid,
                    i.snapshot_uid,
                    i.repo_uid,
                    i.target_stable_key,
                    i.kind,
                    i.value_json,
                    i.confidence,
                    i.basis_json,
                    i.extractor,
                    i.created_at,
                    i.provenance_json,
                ])?;
            }
        }

        tx.commit()?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_repo};
    use crate::types::CreateSnapshotInput;

    fn setup_db_with_snapshot() -> (StorageConnection, String) {
        let storage = fresh_storage();
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

        (storage, snap.snapshot_uid)
    }

    fn make_inference(
        uid: &str,
        snapshot_uid: &str,
        target: &str,
        kind: &str,
    ) -> InferenceInput {
        InferenceInput {
            inference_uid: uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: "r1".to_string(),
            target_stable_key: target.to_string(),
            kind: kind.to_string(),
            value_json: r#"{"annotation":"Service"}"#.to_string(),
            confidence: 0.95,
            basis_json: r#"{"rule":"direct_annotation_match"}"#.to_string(),
            extractor: "spring-liveness:0.1.0".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            provenance_json: None,
        }
    }

    #[test]
    fn insert_inferences_empty_is_noop() {
        let (mut storage, _snap_uid) = setup_db_with_snapshot();
        let result = storage.insert_inferences(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn insert_inferences_batch_insert() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        let inferences = vec![
            make_inference(
                "i1",
                &snap_uid,
                "r1:src/UserService.java#UserService:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            make_inference(
                "i2",
                &snap_uid,
                "r1:src/UserController.java#UserController:SYMBOL:CLASS",
                "spring_container_managed",
            ),
        ];

        storage.insert_inferences(&inferences).unwrap();

        // Verify via query
        let rows = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn insert_inferences_multiple_kinds() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        let inferences = vec![
            make_inference(
                "i1",
                &snap_uid,
                "r1:src/Service.java#Svc:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            InferenceInput {
                inference_uid: "i2".to_string(),
                snapshot_uid: snap_uid.clone(),
                repo_uid: "r1".to_string(),
                target_stable_key: "r1:src/handler.ts#handler:SYMBOL:FUNCTION".to_string(),
                kind: "framework_entrypoint".to_string(),
                value_json: r#"{"convention":"lambda_exported_handler"}"#.to_string(),
                confidence: 0.9,
                basis_json: r#"{"rule":"lambda_handler_detection"}"#.to_string(),
                extractor: "lambda-detector:0.1.0".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                provenance_json: None,
            },
        ];

        storage.insert_inferences(&inferences).unwrap();

        let spring_rows = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(spring_rows.len(), 1);

        let lambda_rows = storage
            .query_inferences_by_kind(&snap_uid, "framework_entrypoint")
            .unwrap();
        assert_eq!(lambda_rows.len(), 1);
    }

    // ── delete_inferences_by_kind tests ──────────────────────────

    #[test]
    fn delete_inferences_by_kind_empty_kinds_is_noop() {
        let (storage, snap_uid) = setup_db_with_snapshot();
        let result = storage.delete_inferences_by_kind(&snap_uid, &[]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn delete_inferences_by_kind_deletes_matching_kind() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        let inferences = vec![
            make_inference(
                "i1",
                &snap_uid,
                "r1:src/Svc.java#Svc:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            InferenceInput {
                inference_uid: "i2".to_string(),
                snapshot_uid: snap_uid.clone(),
                repo_uid: "r1".to_string(),
                target_stable_key: "r1:src/handler.ts#handler:SYMBOL:FUNCTION".to_string(),
                kind: "framework_entrypoint".to_string(),
                value_json: r#"{}"#.to_string(),
                confidence: 0.9,
                basis_json: r#"{}"#.to_string(),
                extractor: "test".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                provenance_json: None,
            },
        ];
        storage.insert_inferences(&inferences).unwrap();

        // Delete only spring_container_managed
        let deleted = storage
            .delete_inferences_by_kind(&snap_uid, &["spring_container_managed"])
            .unwrap();
        assert_eq!(deleted, 1);

        // Verify spring inferences are gone
        let spring_rows = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert!(spring_rows.is_empty());

        // Verify framework_entrypoint is still there
        let lambda_rows = storage
            .query_inferences_by_kind(&snap_uid, "framework_entrypoint")
            .unwrap();
        assert_eq!(lambda_rows.len(), 1);
    }

    #[test]
    fn delete_inferences_by_kind_scoped_to_snapshot() {
        let (mut storage, snap1_uid) = setup_db_with_snapshot();

        // Create a second snapshot
        let snap2 = storage
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
        let snap2_uid = snap2.snapshot_uid;

        // Insert in both snapshots
        let inferences = vec![
            make_inference(
                "i1",
                &snap1_uid,
                "r1:src/A.java#A:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            make_inference(
                "i2",
                &snap2_uid,
                "r1:src/B.java#B:SYMBOL:CLASS",
                "spring_container_managed",
            ),
        ];
        storage.insert_inferences(&inferences).unwrap();

        // Delete only from snap1
        let deleted = storage
            .delete_inferences_by_kind(&snap1_uid, &["spring_container_managed"])
            .unwrap();
        assert_eq!(deleted, 1);

        // Verify snap1 is empty
        let snap1_rows = storage
            .query_inferences_by_kind(&snap1_uid, "spring_container_managed")
            .unwrap();
        assert!(snap1_rows.is_empty());

        // Verify snap2 still has data
        let snap2_rows = storage
            .query_inferences_by_kind(&snap2_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(snap2_rows.len(), 1);
    }

    // ── replace_inferences_by_kind tests ─────────────────────────

    #[test]
    fn replace_inferences_by_kind_basic() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Insert initial
        let initial = vec![make_inference(
            "i1",
            &snap_uid,
            "r1:src/Old.java#Old:SYMBOL:CLASS",
            "spring_container_managed",
        )];
        storage.insert_inferences(&initial).unwrap();

        // Replace with new
        let replacement = vec![make_inference(
            "i2",
            &snap_uid,
            "r1:src/New.java#New:SYMBOL:CLASS",
            "spring_container_managed",
        )];

        let deleted = storage
            .replace_inferences_by_kind(&snap_uid, &["spring_container_managed"], &replacement)
            .unwrap();
        assert_eq!(deleted, 1);

        // Verify only new inference exists
        let rows = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].target_stable_key.contains("New"));
    }

    #[test]
    fn replace_inferences_by_kind_atomic_rollback() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Insert initial
        let initial = vec![make_inference(
            "i1",
            &snap_uid,
            "r1:src/Old.java#Old:SYMBOL:CLASS",
            "spring_container_managed",
        )];
        storage.insert_inferences(&initial).unwrap();

        // Attempt replace with duplicate UIDs
        let replacement = vec![
            make_inference(
                "dup",
                &snap_uid,
                "r1:src/A.java#A:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            make_inference(
                "dup", // DUPLICATE
                &snap_uid,
                "r1:src/B.java#B:SYMBOL:CLASS",
                "spring_container_managed",
            ),
        ];

        let result = storage.replace_inferences_by_kind(
            &snap_uid,
            &["spring_container_managed"],
            &replacement,
        );
        assert!(result.is_err(), "duplicate UIDs must cause failure");

        // Original data should survive (transaction rolled back)
        let rows = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(rows.len(), 1, "original data must survive failed replace");
        assert!(rows[0].target_stable_key.contains("Old"));
    }

    #[test]
    fn replace_inferences_by_kind_preserves_other_kinds() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Insert both spring and lambda
        let initial = vec![
            make_inference(
                "i1",
                &snap_uid,
                "r1:src/Svc.java#Svc:SYMBOL:CLASS",
                "spring_container_managed",
            ),
            InferenceInput {
                inference_uid: "i2".to_string(),
                snapshot_uid: snap_uid.clone(),
                repo_uid: "r1".to_string(),
                target_stable_key: "r1:src/handler.ts#handler:SYMBOL:FUNCTION".to_string(),
                kind: "framework_entrypoint".to_string(),
                value_json: r#"{}"#.to_string(),
                confidence: 0.9,
                basis_json: r#"{}"#.to_string(),
                extractor: "test".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                provenance_json: None,
            },
        ];
        storage.insert_inferences(&initial).unwrap();

        // Replace only spring
        let replacement = vec![make_inference(
            "i3",
            &snap_uid,
            "r1:src/New.java#New:SYMBOL:CLASS",
            "spring_container_managed",
        )];

        storage
            .replace_inferences_by_kind(&snap_uid, &["spring_container_managed"], &replacement)
            .unwrap();

        // Lambda must still exist
        let lambda_rows = storage
            .query_inferences_by_kind(&snap_uid, "framework_entrypoint")
            .unwrap();
        assert_eq!(lambda_rows.len(), 1);
    }

    // ── ACR-4: Provenance and freshness tests ────────────────────────

    /// Make an inference with provenance populated (ACR-4).
    fn make_inference_with_provenance(
        uid: &str,
        snapshot_uid: &str,
        target: &str,
        kind: &str,
        depends_on_key: &str,
    ) -> InferenceInput {
        // Canonical provenance structure from artifact_contracts::Provenance
        let provenance_json = format!(
            r#"{{"version":1,"depends_on":[{{"family":"Nodes","stable_key":"{}"}}],"extractor":"test:1.0"}}"#,
            depends_on_key
        );
        InferenceInput {
            inference_uid: uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: "r1".to_string(),
            target_stable_key: target.to_string(),
            kind: kind.to_string(),
            value_json: r#"{"annotation":"Service"}"#.to_string(),
            confidence: 0.95,
            basis_json: r#"{"rule":"direct_annotation_match"}"#.to_string(),
            extractor: "spring-liveness:0.1.0".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            provenance_json: Some(provenance_json),
        }
    }

    /// Inferences with provenance_json populated should get freshness_state = 'current'.
    ///
    /// This is the ACR-4 contract: when provenance is known, the row starts as 'current'
    /// (not 'unknown'), enabling impact propagation to mark it 'impacted' when dependencies change.
    #[test]
    fn insert_inferences_with_provenance_sets_current_freshness() {
        use crate::freshness_port::FreshnessStoragePort;
        use artifact_contracts::FreshnessState;

        let (mut storage, snap_uid) = setup_db_with_snapshot();

        let inference = make_inference_with_provenance(
            "i-prov-1",
            &snap_uid,
            "r1:src/UserService.java#UserService:SYMBOL:CLASS",
            "spring_container_managed",
            "r1:src/UserService.java#UserService:SYMBOL:CLASS", // depends on itself (the node)
        );
        storage.insert_inferences(&[inference]).unwrap();

        // freshness_state should be 'current' because provenance_json is populated
        let state = storage.get_freshness_state("inferences", "i-prov-1").unwrap();
        assert_eq!(state, Some(FreshnessState::Current));
    }

    /// Inferences WITHOUT provenance_json should get freshness_state = 'unknown'.
    ///
    /// This preserves backward compatibility: legacy inferences without provenance
    /// get 'unknown' state, not 'current', because we can't trust their freshness.
    #[test]
    fn insert_inferences_without_provenance_sets_unknown_freshness() {
        use crate::freshness_port::FreshnessStoragePort;
        use artifact_contracts::FreshnessState;

        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Use helper without provenance (provenance_json: None)
        let inference = make_inference(
            "i-no-prov-1",
            &snap_uid,
            "r1:src/Legacy.java#Legacy:SYMBOL:CLASS",
            "spring_container_managed",
        );
        storage.insert_inferences(&[inference]).unwrap();

        // freshness_state should be 'unknown' because provenance_json is NULL
        let state = storage.get_freshness_state("inferences", "i-no-prov-1").unwrap();
        assert_eq!(state, Some(FreshnessState::Unknown));
    }

    /// End-to-end impact propagation: L0 change → inference marked impacted.
    ///
    /// This is the ACR-4 proof case:
    /// 1. Insert inference with provenance → freshness_state = 'current'
    /// 2. Call mark_impacted_by_stable_keys with matching stable key
    /// 3. Verify freshness_state transitions to 'impacted'
    #[test]
    fn mark_impacted_transitions_current_to_impacted() {
        use crate::freshness_port::FreshnessStoragePort;
        use artifact_contracts::FreshnessState;

        let (mut storage, snap_uid) = setup_db_with_snapshot();

        let target_node_key = "r1:src/UserService.java#UserService:SYMBOL:CLASS";
        let inference = make_inference_with_provenance(
            "i-impact-1",
            &snap_uid,
            target_node_key,
            "spring_container_managed",
            target_node_key, // provenance depends on this node
        );
        storage.insert_inferences(&[inference]).unwrap();

        // Pre-condition: freshness_state is 'current'
        let state = storage.get_freshness_state("inferences", "i-impact-1").unwrap();
        assert_eq!(state, Some(FreshnessState::Current), "pre-condition: should be current");

        // Simulate L0 change: the node changed during extraction
        let affected = storage
            .mark_impacted_by_stable_keys(&snap_uid, "inferences", &[target_node_key])
            .unwrap();
        assert_eq!(affected, 1, "one inference should be marked impacted");

        // Post-condition: freshness_state is now 'impacted'
        let state = storage.get_freshness_state("inferences", "i-impact-1").unwrap();
        assert_eq!(state, Some(FreshnessState::Impacted), "post-condition: should be impacted");
    }

    /// ACR-4 end-to-end proof: copy-forwarded inference with cross-file provenance
    /// gets marked `impacted` when its dependency changes.
    ///
    /// This is the canonical proof case for impact propagation:
    /// 1. InferenceX for ClassB (target in file B)
    /// 2. Provenance: depends on NodeA (in file A) — CROSS-FILE dependency
    /// 3. Simulate refresh: file A changed, file B unchanged
    /// 4. InferenceX is copy-forwarded (file B unchanged)
    /// 5. propagate_impact() marks InferenceX as `impacted` (provenance refs changed node)
    #[test]
    fn impact_propagation_marks_cross_file_dependency_impacted() {
        use crate::freshness_port::FreshnessStoragePort;
        use artifact_contracts::FreshnessState;

        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Create an inference for ClassB that depends on NodeA (cross-file provenance).
        // This simulates an inference like "ClassB implements InterfaceA" where the
        // inference about B depends on A's definition.
        let node_a_key = "r1:src/InterfaceA.java#InterfaceA:SYMBOL:INTERFACE";
        let class_b_key = "r1:src/ClassB.java#ClassB:SYMBOL:CLASS";

        // Provenance: inference about ClassB depends on InterfaceA's node
        let provenance_json = format!(
            r#"{{"version":1,"depends_on":[{{"family":"Nodes","stable_key":"{}"}}],"extractor":"test:1.0"}}"#,
            node_a_key
        );

        let inference = InferenceInput {
            inference_uid: "inf-cross-file".to_string(),
            snapshot_uid: snap_uid.clone(),
            repo_uid: "r1".to_string(),
            target_stable_key: class_b_key.to_string(),
            kind: "implements_interface".to_string(),
            value_json: r#"{"interface":"InterfaceA"}"#.to_string(),
            confidence: 0.9,
            basis_json: r#"{"rule":"implements_clause"}"#.to_string(),
            extractor: "test:1.0".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            provenance_json: Some(provenance_json),
        };
        storage.insert_inferences(&[inference]).unwrap();

        // Verify: inference starts as 'current' (provenance populated)
        let state_before = storage
            .get_freshness_state("inferences", "inf-cross-file")
            .unwrap();
        assert_eq!(
            state_before,
            Some(FreshnessState::Current),
            "inference should start as 'current'"
        );

        // Simulate refresh scenario: InterfaceA.java changed, ClassB.java unchanged.
        // The inference would be copy-forwarded (ClassB unchanged), but its provenance
        // references InterfaceA which DID change.
        //
        // Call mark_impacted_by_stable_keys with the changed node's stable key.
        let impacted_count = storage
            .mark_impacted_by_stable_keys(&snap_uid, "inferences", &[node_a_key])
            .unwrap();

        assert_eq!(impacted_count, 1, "one inference should be marked impacted");

        // Verify: inference is now 'impacted'
        let state_after = storage
            .get_freshness_state("inferences", "inf-cross-file")
            .unwrap();
        assert_eq!(
            state_after,
            Some(FreshnessState::Impacted),
            "inference should be 'impacted' after dependency changed"
        );
    }

    /// Delete by file path does NOT affect inferences from files with similar prefixes.
    ///
    /// Regression test: ensures `delete_inferences_by_kind_and_files("src/A.java")`
    /// does not delete inferences for `src/A.java2` or `src/A.javaX`.
    #[test]
    fn delete_by_file_does_not_false_match_path_prefix() {
        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Insert inferences for two files with similar prefixes
        let inference_a = InferenceInput {
            inference_uid: "i-file-a".to_string(),
            snapshot_uid: snap_uid.clone(),
            repo_uid: "r1".to_string(),
            target_stable_key: "r1:src/Service.java#Service:SYMBOL:CLASS".to_string(),
            kind: "spring_container_managed".to_string(),
            value_json: r#"{}"#.to_string(),
            confidence: 0.95,
            basis_json: r#"{}"#.to_string(),
            extractor: "test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            provenance_json: None,
        };
        let inference_b = InferenceInput {
            inference_uid: "i-file-b".to_string(),
            snapshot_uid: snap_uid.clone(),
            repo_uid: "r1".to_string(),
            // Similar prefix: "src/Service.java" vs "src/Service.javaX"
            target_stable_key: "r1:src/Service.javaX#ServiceX:SYMBOL:CLASS".to_string(),
            kind: "spring_container_managed".to_string(),
            value_json: r#"{}"#.to_string(),
            confidence: 0.95,
            basis_json: r#"{}"#.to_string(),
            extractor: "test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            provenance_json: None,
        };
        storage.insert_inferences(&[inference_a, inference_b]).unwrap();

        // Verify both exist
        let before = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(before.len(), 2);

        // Delete only inferences for "src/Service.java" (not "src/Service.javaX")
        let deleted = storage
            .delete_inferences_by_kind_and_files(
                &snap_uid,
                "r1",
                "spring_container_managed",
                &["src/Service.java"],
            )
            .unwrap();
        assert_eq!(deleted, 1, "should delete exactly one inference");

        // Verify only the exact match was deleted
        let after = storage
            .query_inferences_by_kind(&snap_uid, "spring_container_managed")
            .unwrap();
        assert_eq!(after.len(), 1, "one inference should remain");
        assert!(
            after[0].target_stable_key.contains("Service.javaX"),
            "the remaining inference should be for Service.javaX"
        );
    }

    /// Impact propagation does NOT affect inferences whose provenance doesn't match.
    #[test]
    fn mark_impacted_leaves_unrelated_inferences_current() {
        use crate::freshness_port::FreshnessStoragePort;
        use artifact_contracts::FreshnessState;

        let (mut storage, snap_uid) = setup_db_with_snapshot();

        // Insert two inferences with different provenance
        let inference1 = make_inference_with_provenance(
            "i-related",
            &snap_uid,
            "r1:src/A.java#A:SYMBOL:CLASS",
            "spring_container_managed",
            "r1:src/A.java#A:SYMBOL:CLASS", // depends on node A
        );
        let inference2 = make_inference_with_provenance(
            "i-unrelated",
            &snap_uid,
            "r1:src/B.java#B:SYMBOL:CLASS",
            "spring_container_managed",
            "r1:src/B.java#B:SYMBOL:CLASS", // depends on node B
        );
        storage.insert_inferences(&[inference1, inference2]).unwrap();

        // Both start as 'current'
        assert_eq!(
            storage.get_freshness_state("inferences", "i-related").unwrap(),
            Some(FreshnessState::Current)
        );
        assert_eq!(
            storage.get_freshness_state("inferences", "i-unrelated").unwrap(),
            Some(FreshnessState::Current)
        );

        // Only node A changed
        let affected = storage
            .mark_impacted_by_stable_keys(&snap_uid, "inferences", &["r1:src/A.java#A:SYMBOL:CLASS"])
            .unwrap();
        assert_eq!(affected, 1);

        // i-related is impacted, i-unrelated remains current
        assert_eq!(
            storage.get_freshness_state("inferences", "i-related").unwrap(),
            Some(FreshnessState::Impacted)
        );
        assert_eq!(
            storage.get_freshness_state("inferences", "i-unrelated").unwrap(),
            Some(FreshnessState::Current)
        );
    }
}
