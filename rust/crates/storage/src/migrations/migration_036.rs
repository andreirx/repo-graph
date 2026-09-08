//! Migration 036 — per-CHUNK FIELD-tier flag on `seed_vectors` (SEED-CHUNK-3).
//!
//! Adds ONE additive column, `is_field`, to the `seed_vectors` table (migration 033,
//! extended by 034's `is_decl`). `is_field = 1` marks a chunk in the FIELD tier — a
//! PROPERTY/FIELD data member (any language) or an undocumented one-line chunk whose
//! embedded document is ~90% its own qualified name (the D11 name-domination geometry).
//! The serving path ranks a field-tier chunk BELOW every body-bearing chunk of its
//! PARTITION and labels it `[field]` (spec §2.1/§2.4). `is_field = 0` is an ordinary
//! (body-bearing or documented) chunk.
//!
//! ```text
//! seed_vectors ADD COLUMN is_field INTEGER          (NULLABLE, no default)
//! ```
//!
//! # Why NULLABLE with NO default — identical honesty rationale to 034's `is_decl`
//!
//! The seed pass rewrites a snapshot's whole vector set (DELETE + INSERT) on every
//! index/refresh and ALWAYS writes `is_field` explicitly (0/1) from the chunk's stored
//! subtype + span line count + doc presence. So after this migration a row's `is_field`
//! is NULL **iff** the row was written by a PRE-SEED-CHUNK-3 pass and has not yet been
//! re-seeded. That NULL is the per-row DISCRIMINATOR the serving path needs: such a row
//! carries no field-tier classification, so serving it would silently drop the tier for a
//! property (presenting a one-line field as if it were ranked honestly). A
//! `NOT NULL DEFAULT 0` column could NOT be distinguished from a genuine `is_field = 0`
//! body-bearing chunk, so the honest state would be unrecoverable.
//!
//! With NULL as the marker, `read_seed_vectors` (seed_impl.rs) refuses to serve a
//! snapshot that still holds any row with a NULL `is_field` — the SAME StaleClassification
//! refusal migration 034 established for `is_decl`, mapped upstream to the self-healing
//! "seeds re-embedding (pending)" state (the daemon schedules a background re-seed via the
//! SeedCoordinator latch). No data is deleted; no unclassified row is ever served.
//!
//! A snapshot is HOMOGENEOUS: all of its rows are (re)written in ONE transaction, and this
//! migration backfills every existing row's `is_field` to NULL at once — so a snapshot is
//! all-legacy (NULL) or all-classified (0/1), never a mix.
//!
//! # Idempotence
//!
//! `ALTER TABLE … ADD COLUMN` is not itself `IF NOT EXISTS`-guarded in SQLite, so the
//! runner is version-gated (`max_version < 36` in `run_migrations`); re-running this
//! function directly checks `PRAGMA table_info` first so a double-apply is a no-op.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 036 against the given connection. Idempotent: a second run (or a run
/// against a DB that already has the column) is a no-op. The column is NULLABLE with no
/// default, so existing (pre-036) rows are backfilled to NULL — the "predates the field
/// tier" marker the serving path keys on (see the module doc).
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    if !column_exists(conn, "seed_vectors", "is_field")? {
        conn.execute_batch("ALTER TABLE seed_vectors ADD COLUMN is_field INTEGER;")?;
    }
    record_migration(conn, 36, "036-seed-chunk-field")?;
    Ok(())
}

/// Does `table` already have a column named `column`? Reads `PRAGMA table_info`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, StorageError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?; // column 1 of table_info is the column name
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::{migration_001, migration_033, migration_034};

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        migration_033::run(&mut conn).unwrap();
        migration_034::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn adds_is_field_column() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        assert!(column_exists(&conn, "seed_vectors", "is_field").unwrap());
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error (column already present)
        assert!(column_exists(&conn, "seed_vectors", "is_field").unwrap());
    }

    #[test]
    fn preexisting_row_backfills_is_field_to_null_the_legacy_marker() {
        // A row written BEFORE this migration must be distinguishable from a genuine
        // `is_field = 0` body-bearing chunk, so the serving path can refuse to serve it
        // unclassified. The column is NULLABLE with no default, so the backfilled value is
        // NULL — the "predates the field tier" marker `read_seed_vectors` keys on.
        let mut conn = setup_db();
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES ('r1','t','/t','2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
             VALUES ('s1','r1','full','ready','2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // A pre-036 row: is_decl present (post-034) but no is_field column yet.
        conn.execute(
            "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,path,line,\
             qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector,is_decl) \
             VALUES ('s1','n1','r1','k','a.rs',1,'a::f',0,'h','m','ck',1,x'00000000',0)",
            [],
        )
        .unwrap();
        run(&mut conn).unwrap();
        let is_field: Option<i64> = conn
            .query_row(
                "SELECT is_field FROM seed_vectors WHERE node_uid='n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            is_field, None,
            "pre-existing row backfills to NULL (the legacy / not-yet-field-classified marker)"
        );
    }
}
