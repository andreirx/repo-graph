//! Migration 008 — boundary fact tables + derived link table.
//!
//! Pure SQL: creates `boundary_provider_facts`,
//! `boundary_consumer_facts`, and `boundary_links` tables plus
//! nine indexes. Mirrors `008-boundary-facts.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS boundary_provider_facts (
			fact_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			mechanism           TEXT NOT NULL,
			operation           TEXT NOT NULL,
			address             TEXT NOT NULL,
			matcher_key         TEXT NOT NULL,
			handler_stable_key  TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			line_start          INTEGER NOT NULL,
			framework           TEXT NOT NULL,
			basis               TEXT NOT NULL,
			schema_ref          TEXT,
			metadata_json       TEXT,
			extractor           TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_mechanism ON boundary_provider_facts(snapshot_uid, mechanism);
		CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_matcher_key ON boundary_provider_facts(snapshot_uid, matcher_key);
		CREATE INDEX IF NOT EXISTS idx_bp_facts_snapshot_handler ON boundary_provider_facts(snapshot_uid, handler_stable_key);

		CREATE TABLE IF NOT EXISTS boundary_consumer_facts (
			fact_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			mechanism           TEXT NOT NULL,
			operation           TEXT NOT NULL,
			address             TEXT NOT NULL,
			matcher_key         TEXT NOT NULL,
			caller_stable_key   TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			line_start          INTEGER NOT NULL,
			basis               TEXT NOT NULL,
			confidence          REAL NOT NULL,
			schema_ref          TEXT,
			metadata_json       TEXT,
			extractor           TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_mechanism ON boundary_consumer_facts(snapshot_uid, mechanism);
		CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_matcher_key ON boundary_consumer_facts(snapshot_uid, matcher_key);
		CREATE INDEX IF NOT EXISTS idx_bc_facts_snapshot_caller ON boundary_consumer_facts(snapshot_uid, caller_stable_key);

		CREATE TABLE IF NOT EXISTS boundary_links (
			link_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			provider_fact_uid   TEXT NOT NULL REFERENCES boundary_provider_facts(fact_uid),
			consumer_fact_uid   TEXT NOT NULL REFERENCES boundary_consumer_facts(fact_uid),
			match_basis         TEXT NOT NULL,
			confidence          REAL NOT NULL,
			metadata_json       TEXT,
			materialized_at     TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_bl_snapshot ON boundary_links(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_bl_provider ON boundary_links(provider_fact_uid);
		CREATE INDEX IF NOT EXISTS idx_bl_consumer ON boundary_links(consumer_fact_uid);
		"#,
	)?;

	record_migration(conn, 8, "008-boundary-facts")?;
	Ok(())
}
