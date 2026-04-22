//! CRUD methods for the `measurements` table.
//!
//! RS-MS-3c-prereq: Batch insert for metrics computed during extraction.
//! Mirrors TS `insertMeasurements` from `sqlite-storage.ts`.
//!
//! Transaction-wrapped: yes (batch insert).

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::MeasurementInput;

impl StorageConnection {
	/// Delete measurements by kind for a snapshot.
	///
	/// RS-MS-4-prereq-b: Called before inserting coverage measurements
	/// to ensure idempotency. Deleting existing measurements of the
	/// same kind before insert prevents accumulation drift across
	/// repeated imports.
	///
	/// Only deletes measurements of the specified kinds. Other measurement
	/// kinds (e.g., complexity metrics) are untouched.
	pub fn delete_measurements_by_kind(
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
			"DELETE FROM measurements WHERE snapshot_uid = ? AND kind IN ({})",
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

	/// Atomically replace measurements of specified kinds for a snapshot.
	///
	/// RS-MS-4-prereq-b: Called by coverage import to ensure idempotency.
	/// Deletes all existing measurements of the specified kinds, then
	/// inserts the new measurements, all within a single transaction.
	///
	/// If insert fails (e.g., primary key collision), the delete is
	/// rolled back and no data is lost.
	///
	/// # Arguments
	/// * `snapshot_uid` - Snapshot to replace measurements for
	/// * `kinds` - Measurement kinds to delete before insert (e.g., `["line_coverage"]`)
	/// * `measurements` - New measurements to insert
	///
	/// # Returns
	/// * `Ok(deleted_count)` - Number of measurements deleted before insert
	/// * `Err(StorageError)` - On any failure (transaction rolled back)
	pub fn replace_measurements_by_kind(
		&mut self,
		snapshot_uid: &str,
		kinds: &[&str],
		measurements: &[MeasurementInput],
	) -> Result<u64, StorageError> {
		let tx = self.connection_mut().transaction()?;

		// Delete existing measurements of specified kinds
		let deleted = if kinds.is_empty() {
			0
		} else {
			let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
			let sql = format!(
				"DELETE FROM measurements WHERE snapshot_uid = ? AND kind IN ({})",
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

		// Insert new measurements
		if !measurements.is_empty() {
			let mut stmt = tx.prepare(
				"INSERT INTO measurements
				 (measurement_uid, snapshot_uid, repo_uid, target_stable_key,
				  kind, value_json, source, created_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
			)?;

			for m in measurements {
				stmt.execute(rusqlite::params![
					m.measurement_uid,
					m.snapshot_uid,
					m.repo_uid,
					m.target_stable_key,
					m.kind,
					m.value_json,
					m.source,
					m.created_at,
				])?;
			}
		}

		tx.commit()?;
		Ok(deleted)
	}

	/// Batch insert measurements. Transaction-wrapped.
	///
	/// RS-MS-3c-prereq: Called by the compose layer after indexing
	/// to persist metrics (cyclomatic_complexity, parameter_count,
	/// max_nesting_depth) computed by extractors.
	///
	/// No collision detection — measurements are keyed by generated
	/// UUID, not by target_stable_key. Multiple measurements for the
	/// same target are valid (e.g., complexity + coverage).
	pub fn insert_measurements(
		&mut self,
		measurements: &[MeasurementInput],
	) -> Result<(), StorageError> {
		if measurements.is_empty() {
			return Ok(());
		}

		let tx = self.connection_mut().transaction()?;

		{
			let mut stmt = tx.prepare(
				"INSERT INTO measurements
				 (measurement_uid, snapshot_uid, repo_uid, target_stable_key,
				  kind, value_json, source, created_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
			)?;

			for m in measurements {
				stmt.execute(rusqlite::params![
					m.measurement_uid,
					m.snapshot_uid,
					m.repo_uid,
					m.target_stable_key,
					m.kind,
					m.value_json,
					m.source,
					m.created_at,
				])?;
			}
		}

		tx.commit()?;
		Ok(())
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

	#[test]
	fn insert_measurements_empty_is_noop() {
		let (mut storage, _snap_uid) = setup_db_with_snapshot();
		let result = storage.insert_measurements(&[]);
		assert!(result.is_ok());
	}

	#[test]
	fn insert_measurements_batch_insert() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		let measurements = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/foo.ts#bar:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":5}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/foo.ts#bar:SYMBOL:FUNCTION".into(),
				kind: "parameter_count".into(),
				value_json: r#"{"value":2}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];

		storage.insert_measurements(&measurements).unwrap();

		// Verify via query
		let rows = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].target_stable_key, "r1:src/foo.ts#bar:SYMBOL:FUNCTION");

		let param_rows = storage
			.query_measurements_by_kind(&snap_uid, "parameter_count")
			.unwrap();
		assert_eq!(param_rows.len(), 1);
	}

	#[test]
	fn insert_measurements_multiple_targets() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		let measurements = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts#foo:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":3}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/b.ts#bar:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":7}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];

		storage.insert_measurements(&measurements).unwrap();

		let rows = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(rows.len(), 2);
	}

	// ── delete_measurements_by_kind tests ──────────────────────────

	#[test]
	fn delete_measurements_by_kind_empty_kinds_is_noop() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		let result = storage.delete_measurements_by_kind(&snap_uid, &[]);
		assert_eq!(result.unwrap(), 0);
	}

	#[test]
	fn delete_measurements_by_kind_deletes_matching_kind() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		// Insert two measurements with different kinds
		let measurements = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.8}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts#foo:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":5}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];
		storage.insert_measurements(&measurements).unwrap();

		// Delete only line_coverage
		let deleted = storage
			.delete_measurements_by_kind(&snap_uid, &["line_coverage"])
			.unwrap();
		assert_eq!(deleted, 1);

		// Verify line_coverage is gone
		let coverage_rows = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert!(coverage_rows.is_empty());

		// Verify cyclomatic_complexity is still there
		let complexity_rows = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(complexity_rows.len(), 1);
	}

	#[test]
	fn delete_measurements_by_kind_multiple_kinds() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		let measurements = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.8}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "branch_coverage".into(),
				value_json: r#"{"value":0.6}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m3".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts#foo:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":5}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];
		storage.insert_measurements(&measurements).unwrap();

		// Delete both coverage kinds but not complexity
		let deleted = storage
			.delete_measurements_by_kind(&snap_uid, &["line_coverage", "branch_coverage"])
			.unwrap();
		assert_eq!(deleted, 2);

		// Verify complexity is still there
		let complexity_rows = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(complexity_rows.len(), 1);
	}

	#[test]
	fn delete_measurements_by_kind_scoped_to_snapshot() {
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

		// Insert coverage in both snapshots
		let measurements = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap1_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.8}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap2_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.9}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];
		storage.insert_measurements(&measurements).unwrap();

		// Delete only from snap1
		let deleted = storage
			.delete_measurements_by_kind(&snap1_uid, &["line_coverage"])
			.unwrap();
		assert_eq!(deleted, 1);

		// Verify snap1 coverage is gone
		let snap1_rows = storage
			.query_measurements_by_kind(&snap1_uid, "line_coverage")
			.unwrap();
		assert!(snap1_rows.is_empty());

		// Verify snap2 coverage is still there
		let snap2_rows = storage
			.query_measurements_by_kind(&snap2_uid, "line_coverage")
			.unwrap();
		assert_eq!(snap2_rows.len(), 1);
	}

	#[test]
	fn delete_then_insert_is_idempotent() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		// First import
		let measurements1 = vec![MeasurementInput {
			measurement_uid: "m1".into(),
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.5}"#.into(),
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];
		storage.insert_measurements(&measurements1).unwrap();

		// Second import with different value - delete first, then insert
		storage
			.delete_measurements_by_kind(&snap_uid, &["line_coverage"])
			.unwrap();

		let measurements2 = vec![MeasurementInput {
			measurement_uid: "m2".into(), // Different UID
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.8}"#.into(), // Different value
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];
		storage.insert_measurements(&measurements2).unwrap();

		// Should have exactly one measurement with the new value
		let rows = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(rows.len(), 1);
		assert!(rows[0].value_json.contains("0.8"));
	}

	// ── replace_measurements_by_kind tests ─────────────────────────

	#[test]
	fn replace_measurements_by_kind_basic() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		// Insert initial coverage
		let initial = vec![MeasurementInput {
			measurement_uid: "m1".into(),
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.5}"#.into(),
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];
		storage.insert_measurements(&initial).unwrap();

		// Replace with new coverage
		let replacement = vec![MeasurementInput {
			measurement_uid: "m2".into(),
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.9}"#.into(),
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];

		let deleted = storage
			.replace_measurements_by_kind(&snap_uid, &["line_coverage"], &replacement)
			.unwrap();
		assert_eq!(deleted, 1);

		// Verify only new measurement exists
		let rows = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(rows.len(), 1);
		assert!(rows[0].value_json.contains("0.9"));
	}

	#[test]
	fn replace_measurements_by_kind_atomic_rollback() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		// Insert initial coverage
		let initial = vec![MeasurementInput {
			measurement_uid: "m1".into(),
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.5}"#.into(),
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];
		storage.insert_measurements(&initial).unwrap();

		// Attempt replace with duplicate UIDs WITHIN the replacement batch.
		// The second insert will fail with primary key collision.
		let replacement = vec![
			MeasurementInput {
				measurement_uid: "m-dup".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/b.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.9}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m-dup".into(), // DUPLICATE within batch
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/c.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.8}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];

		// Replace should fail on the duplicate UID
		let result = storage.replace_measurements_by_kind(&snap_uid, &["line_coverage"], &replacement);
		assert!(result.is_err(), "duplicate UIDs in batch must cause failure");

		// CRITICAL: original data should still exist (transaction rolled back)
		let rows = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(rows.len(), 1, "original data must survive failed replace");
		assert!(rows[0].value_json.contains("0.5"), "original value must be preserved");
	}

	#[test]
	fn replace_measurements_by_kind_preserves_other_kinds() {
		let (mut storage, snap_uid) = setup_db_with_snapshot();

		// Insert both coverage and complexity
		let initial = vec![
			MeasurementInput {
				measurement_uid: "m1".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts:FILE".into(),
				kind: "line_coverage".into(),
				value_json: r#"{"value":0.5}"#.into(),
				source: "coverage-istanbul:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
			MeasurementInput {
				measurement_uid: "m2".into(),
				snapshot_uid: snap_uid.clone(),
				repo_uid: "r1".into(),
				target_stable_key: "r1:src/a.ts#foo:SYMBOL:FUNCTION".into(),
				kind: "cyclomatic_complexity".into(),
				value_json: r#"{"value":5}"#.into(),
				source: "indexer:0.1.0".into(),
				created_at: "2026-01-01T00:00:00Z".into(),
			},
		];
		storage.insert_measurements(&initial).unwrap();

		// Replace only coverage
		let replacement = vec![MeasurementInput {
			measurement_uid: "m3".into(),
			snapshot_uid: snap_uid.clone(),
			repo_uid: "r1".into(),
			target_stable_key: "r1:src/a.ts:FILE".into(),
			kind: "line_coverage".into(),
			value_json: r#"{"value":0.9}"#.into(),
			source: "coverage-istanbul:0.1.0".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
		}];

		storage
			.replace_measurements_by_kind(&snap_uid, &["line_coverage"], &replacement)
			.unwrap();

		// Complexity must still exist
		let complexity = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(complexity.len(), 1);
		assert!(complexity[0].value_json.contains("5"));
	}
}
