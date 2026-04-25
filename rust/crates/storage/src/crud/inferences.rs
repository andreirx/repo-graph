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
            let mut stmt = tx.prepare(
                "INSERT INTO inferences
                 (inference_uid, snapshot_uid, repo_uid, target_stable_key,
                  kind, value_json, confidence, basis_json, extractor, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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

        // Insert new inferences
        if !inferences.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO inferences
                 (inference_uid, snapshot_uid, repo_uid, target_stable_key,
                  kind, value_json, confidence, basis_json, extractor, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
}
