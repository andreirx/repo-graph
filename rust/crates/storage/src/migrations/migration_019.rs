//! Migration 019 — add quality_assessments table.
//!
//! Supports the quality policy assessment model (quality-policy-design.md).
//! Assessments are derived facts: the result of evaluating quality policies
//! against measurements for a given snapshot.
//!
//! Assessment identity:
//! - Absolute policies: (snapshot_uid, policy_uid)
//! - Comparative policies: (snapshot_uid, policy_uid, baseline_snapshot_uid)
//!
//! The table stores computed verdicts and violation details. Waivers are
//! applied at query time via the declarations table (kind='quality_policy_waiver').

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS quality_assessments (
			assessment_uid         TEXT PRIMARY KEY,
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			policy_uid             TEXT NOT NULL,
			baseline_snapshot_uid  TEXT REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			computed_verdict       TEXT NOT NULL,
			measurements_evaluated INTEGER NOT NULL,
			violations_json        TEXT NOT NULL,
			new_violations         INTEGER,
			worsened_violations    INTEGER,
			created_at             TEXT NOT NULL
		);

		CREATE INDEX IF NOT EXISTS idx_quality_assessments_snapshot
			ON quality_assessments(snapshot_uid);

		CREATE INDEX IF NOT EXISTS idx_quality_assessments_policy
			ON quality_assessments(policy_uid);
		"#,
	)?;

	record_migration(conn, 19, "019-quality-assessments")?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use crate::migrations::run_migrations;
	use rusqlite::Connection;

	fn fresh_conn() -> Connection {
		Connection::open_in_memory().expect("open in-memory db")
	}

	#[test]
	fn migration_019_creates_quality_assessments_table() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all migrations");

		// Verify table exists
		let count: i64 = conn
			.query_row(
				"SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='quality_assessments'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(count, 1);

		// Verify columns
		let cols: Vec<String> = {
			let mut stmt = conn.prepare("PRAGMA table_info(quality_assessments)").unwrap();
			stmt.query_map([], |row| row.get::<_, String>("name"))
				.unwrap()
				.collect::<Result<Vec<_>, _>>()
				.unwrap()
		};

		assert!(cols.contains(&"assessment_uid".to_string()));
		assert!(cols.contains(&"snapshot_uid".to_string()));
		assert!(cols.contains(&"policy_uid".to_string()));
		assert!(cols.contains(&"baseline_snapshot_uid".to_string()));
		assert!(cols.contains(&"computed_verdict".to_string()));
		assert!(cols.contains(&"measurements_evaluated".to_string()));
		assert!(cols.contains(&"violations_json".to_string()));
		assert!(cols.contains(&"new_violations".to_string()));
		assert!(cols.contains(&"worsened_violations".to_string()));
		assert!(cols.contains(&"created_at".to_string()));
	}

	#[test]
	fn migration_019_indexes_exist() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all migrations");

		let indexes: Vec<String> = {
			let mut stmt = conn
				.prepare("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='quality_assessments'")
				.unwrap();
			stmt.query_map([], |row| row.get::<_, String>(0))
				.unwrap()
				.collect::<Result<Vec<_>, _>>()
				.unwrap()
		};

		assert!(
			indexes.iter().any(|n| n.contains("snapshot")),
			"snapshot index should exist"
		);
		assert!(
			indexes.iter().any(|n| n.contains("policy")),
			"policy index should exist"
		);
	}
}
