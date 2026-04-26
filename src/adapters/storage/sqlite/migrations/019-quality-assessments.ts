/**
 * Migration 019: Add quality_assessments table.
 *
 * Supports the quality policy assessment model (quality-policy-design.md).
 * Assessments are derived facts: the result of evaluating quality policies
 * against measurements for a given snapshot.
 *
 * Assessment identity:
 * - Absolute policies: (snapshot_uid, policy_uid)
 * - Comparative policies: (snapshot_uid, policy_uid, baseline_snapshot_uid)
 *
 * The table stores computed verdicts and violation details. Waivers are
 * applied at query time via the declarations table (kind='quality_policy_waiver').
 */

import type Database from "better-sqlite3";

export function runMigration019(db: Database.Database): void {
	db.exec(`
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
	`);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (19, '019-quality-assessments', datetime('now'))",
	);
}
