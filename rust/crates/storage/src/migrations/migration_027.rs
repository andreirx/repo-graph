//! Migration 027 — per-row freshness and provenance schema (ACR-3).
//!
//! Adds `freshness_state`, `freshness_updated_at`, and `provenance_json` columns
//! to artifact tables that require per-row freshness tracking per the artifact
//! contract model.
//!
//! # Semantic Separation
//!
//! For the `inferences` table, this migration adds `provenance_json` as a
//! **separate** column from the existing `basis_json`. These are distinct:
//!
//! - `basis_json`: inference-specific evidence/rationale (family-specific, variable shape)
//! - `provenance_json`: canonical provenance (versioned, Layer 0 anchors, cross-family contract)
//!
//! This separation is critical for ACR-4 impact propagation. The provenance
//! column must have a canonical shape that can be reliably queried to mark
//! rows as `impacted` when their Layer 0 dependencies change.
//!
//! # Tables Modified
//!
//! **Deterministic Relationships (Layer 2):**
//! - `boundary_contracts`
//! - `boundary_interaction_links`
//!
//! **Hints/Inferences (Layer 3):**
//! - `inferences`
//! - `project_surfaces`
//! - `project_surface_evidence`
//! - `surface_entrypoints`
//! - `surface_config_roots`
//! - `surface_env_dependencies`
//! - `surface_env_evidence`
//! - `surface_fs_mutations`
//! - `surface_fs_mutation_evidence`
//! - `module_candidates`
//!
//! # Migration Strategy
//!
//! Existing rows are migrated with:
//! - `freshness_state = 'unknown'` (honest: we don't know provenance of legacy rows)
//! - `freshness_updated_at = NULL`
//! - `provenance_json = NULL`
//!
//! New rows created after this migration must populate these fields appropriately.
//!
//! # Freshness States
//!
//! - `current`: computed from current Layer 0 state
//! - `impacted`: upstream Layer 0 changed; row may be stale but still useful
//! - `stale`: known to be out of date
//! - `unknown`: freshness cannot be determined (legacy/migrated rows)
//!
//! # References
//!
//! - `docs/slices/acr-3-provenance-and-freshness-schema.md`
//! - `docs/architecture/artifact-contract-model.md`
//! - `rust/crates/artifact-contracts/src/freshness.rs`
//! - `rust/crates/artifact-contracts/src/provenance.rs`

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Tables that require per-row freshness and provenance tracking.
///
/// These are Layer 2+ artifact families per the artifact contract model.
/// Layer 0-1 families have implicit freshness from source file currency.
const FRESHNESS_TABLES: &[&str] = &[
    // Layer 2: Deterministic Relationships
    "boundary_contracts",
    "boundary_interaction_links",
    // Layer 3: Hints/Inferences
    "inferences",
    "project_surfaces",
    "project_surface_evidence",
    "surface_entrypoints",
    "surface_config_roots",
    "surface_env_dependencies",
    "surface_env_evidence",
    "surface_fs_mutations",
    "surface_fs_mutation_evidence",
    "module_candidates",
];

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    // Add freshness and provenance columns to each table
    for table in FRESHNESS_TABLES {
        add_freshness_columns(conn, table)?;
    }

    // Create indexes for freshness queries
    create_freshness_indexes(conn)?;

    record_migration(conn, 27, "027-freshness-provenance")?;
    Ok(())
}

/// Add freshness_state, freshness_updated_at, and provenance_json columns to a table.
///
/// Uses column-existence checks for idempotency.
fn add_freshness_columns(conn: &Connection, table: &str) -> Result<(), StorageError> {
    // Check if table exists first (some tables may not exist in all DB states)
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            [table],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if !table_exists {
        // Table doesn't exist yet - skip. This handles forward-compatibility
        // where the migration runs before the table-creating migration.
        return Ok(());
    }

    let cols = pragma_table_columns(conn, table)?;

    // freshness_state: default 'unknown' for existing rows
    if !cols.contains(&"freshness_state".to_string()) {
        conn.execute(
            &format!(
                "ALTER TABLE {} ADD COLUMN freshness_state TEXT NOT NULL DEFAULT 'unknown'",
                table
            ),
            [],
        )?;
    }

    // freshness_updated_at: NULL for existing rows
    if !cols.contains(&"freshness_updated_at".to_string()) {
        conn.execute(
            &format!(
                "ALTER TABLE {} ADD COLUMN freshness_updated_at TEXT",
                table
            ),
            [],
        )?;
    }

    // provenance_json: NULL for existing rows (we don't know their provenance)
    if !cols.contains(&"provenance_json".to_string()) {
        conn.execute(
            &format!(
                "ALTER TABLE {} ADD COLUMN provenance_json TEXT",
                table
            ),
            [],
        )?;
    }

    Ok(())
}

/// Create indexes for efficient freshness-based queries.
///
/// These indexes support:
/// - Filtering by freshness state (e.g., find all 'impacted' rows)
/// - Impact propagation queries (find rows to mark impacted)
fn create_freshness_indexes(conn: &Connection) -> Result<(), StorageError> {
    // Index pattern: (snapshot_uid, freshness_state) for snapshot-scoped queries
    // Only create for tables that have snapshot_uid column

    let snapshot_scoped_tables = [
        "boundary_contracts",
        "boundary_interaction_links",
        "inferences",
        "project_surfaces",
        "project_surface_evidence",
        "surface_entrypoints",
        "surface_config_roots",
        "surface_env_dependencies",
        "surface_env_evidence",
        "surface_fs_mutations",
        "surface_fs_mutation_evidence",
        "module_candidates",
    ];

    for table in snapshot_scoped_tables {
        // Check if table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
                [table],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !table_exists {
            continue;
        }

        // Check if table has snapshot_uid column
        let cols = pragma_table_columns(conn, table)?;
        if !cols.contains(&"snapshot_uid".to_string()) {
            continue;
        }

        // Create index if not exists
        let index_name = format!("idx_{}_freshness", table);
        conn.execute(
            &format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(snapshot_uid, freshness_state)",
                index_name, table
            ),
            [],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::migration_001;

    fn fresh_conn() -> Connection {
        Connection::open_in_memory().expect("open in-memory db")
    }

    fn bootstrap_to_026(conn: &mut Connection) {
        // Apply all migrations up to 026 to get all tables created
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .unwrap();
        crate::migrations::run_migrations(conn).unwrap();
    }

    #[test]
    fn migration_027_adds_freshness_columns_to_inferences() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        // Verify columns exist
        let cols = pragma_table_columns(&conn, "inferences").unwrap();
        assert!(
            cols.contains(&"freshness_state".to_string()),
            "freshness_state column should exist"
        );
        assert!(
            cols.contains(&"freshness_updated_at".to_string()),
            "freshness_updated_at column should exist"
        );
        assert!(
            cols.contains(&"provenance_json".to_string()),
            "provenance_json column should exist"
        );
        // basis_json should still exist (separate from provenance_json)
        assert!(
            cols.contains(&"basis_json".to_string()),
            "basis_json column should still exist (separate semantic)"
        );
    }

    #[test]
    fn migration_027_adds_freshness_columns_to_boundary_contracts() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        let cols = pragma_table_columns(&conn, "boundary_contracts").unwrap();
        assert!(cols.contains(&"freshness_state".to_string()));
        assert!(cols.contains(&"freshness_updated_at".to_string()));
        assert!(cols.contains(&"provenance_json".to_string()));
    }

    #[test]
    fn migration_027_adds_freshness_columns_to_boundary_interaction_links() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        let cols = pragma_table_columns(&conn, "boundary_interaction_links").unwrap();
        assert!(cols.contains(&"freshness_state".to_string()));
        assert!(cols.contains(&"freshness_updated_at".to_string()));
        assert!(cols.contains(&"provenance_json".to_string()));
    }

    #[test]
    fn migration_027_adds_freshness_columns_to_module_candidates() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        let cols = pragma_table_columns(&conn, "module_candidates").unwrap();
        assert!(cols.contains(&"freshness_state".to_string()));
        assert!(cols.contains(&"freshness_updated_at".to_string()));
        assert!(cols.contains(&"provenance_json".to_string()));
    }

    #[test]
    fn migration_027_creates_freshness_indexes() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        // Check that freshness indexes exist
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%_freshness'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Should have indexes for most tables
        assert!(
            index_count >= 10,
            "expected at least 10 freshness indexes, got {}",
            index_count
        );
    }

    #[test]
    fn migration_027_is_idempotent() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        // Run again - should not error
        run(&mut conn).unwrap();

        // Columns should still exist exactly once
        let cols = pragma_table_columns(&conn, "inferences").unwrap();
        let freshness_count = cols.iter().filter(|c| *c == "freshness_state").count();
        assert_eq!(freshness_count, 1, "freshness_state should appear exactly once");
    }

    #[test]
    fn migration_027_existing_rows_get_unknown_freshness() {
        let mut conn = fresh_conn();

        // Bootstrap up to migration 026
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .unwrap();
        migration_001::run(&mut conn).unwrap();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'complete', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Insert an inference row BEFORE migration 027
        conn.execute(
            "INSERT INTO inferences (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at) VALUES ('i1', 's1', 'r1', 'test:key', 'test_kind', '{}', 0.9, '{}', 'test:1.0', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Now run migration 027
        run(&mut conn).unwrap();

        // Verify existing row has freshness_state = 'unknown'
        let freshness: String = conn
            .query_row(
                "SELECT freshness_state FROM inferences WHERE inference_uid = 'i1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(freshness, "unknown", "existing rows should have freshness_state = 'unknown'");

        // Verify freshness_updated_at is NULL
        let updated_at: Option<String> = conn
            .query_row(
                "SELECT freshness_updated_at FROM inferences WHERE inference_uid = 'i1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(updated_at.is_none(), "existing rows should have freshness_updated_at = NULL");

        // Verify provenance_json is NULL
        let provenance: Option<String> = conn
            .query_row(
                "SELECT provenance_json FROM inferences WHERE inference_uid = 'i1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(provenance.is_none(), "existing rows should have provenance_json = NULL");
    }

    #[test]
    fn migration_027_recorded_in_schema_migrations() {
        let mut conn = fresh_conn();
        bootstrap_to_026(&mut conn);

        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '027-freshness-provenance'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 27);
    }
}
