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
}
