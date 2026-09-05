//! Migration 034 — per-CHUNK declaration kind on `seed_vectors` (SEED-CHUNK-2).
//!
//! Adds ONE additive column, `is_decl`, to the `seed_vectors` table (migration 033,
//! shipped v0.16.0 — never edited). `is_decl = 1` marks a chunk whose span is a
//! DECLARATION without a body (prototype / trait-method decl / interface member /
//! `declare`), so the serving path ranks an implementation ABOVE its own declaration
//! and labels the decl `(decl)` (spec §2.2). `is_decl = 0` is an implementation.
//!
//! ```text
//! seed_vectors ADD COLUMN is_decl INTEGER          (NULLABLE, no default)
//! ```
//!
//! # Why NULLABLE with NO default (review-1 item 2 — honesty)
//!
//! The seed pass rewrites a snapshot's whole vector set (DELETE + INSERT) on every
//! index/refresh and ALWAYS writes `is_decl` explicitly (0/1) from the chunk's span
//! structure. So after this migration a row's `is_decl` is NULL **iff** the row was
//! written by a PRE-SEED-CHUNK-2 pass and has not yet been re-seeded. That NULL is the
//! per-row DISCRIMINATOR the serving path needs: such a row ALSO still carries the
//! obsolete per-FILE `is_test` value (033 semantics), so serving it as a definite
//! per-chunk classification would present stale test/decl labels as fact (VISION Fact
//! Certainty Model). A `NOT NULL DEFAULT 0` column could NOT be distinguished from a
//! genuine `is_decl = 0` implementation, so the honest state would be unrecoverable.
//!
//! With NULL as the marker, `read_seed_vectors` (seed_impl.rs) refuses to serve a
//! snapshot that still holds any legacy row — it surfaces a read error that the serve
//! path and doctor already map to the honest degraded state "seed vectors present but
//! unreadable; they rebuild on next index" (StoreUnreadable's documented meaning:
//! "rows may exist but cannot be used"). The very next index/refresh re-seeds the
//! snapshot (copy-forward reuses the vectors and RECOMPUTES is_test/is_decl from the
//! current corpus), so this is a transient, self-healing upgrade window — no data is
//! deleted, and no stale classification is ever served.
//!
//! Because a snapshot's rows are all (re)written in ONE transaction and this migration
//! backfills every existing row's `is_decl` to NULL at once, a snapshot is HOMOGENEOUS:
//! all-legacy (NULL) or all-classified (0/1) — never a mix.
//!
//! # Idempotence
//!
//! `ALTER TABLE … ADD COLUMN` is not itself `IF NOT EXISTS`-guarded in SQLite, so the
//! runner is version-gated (`max_version < 34` in `run_migrations`); re-running this
//! function directly checks `PRAGMA table_info` first so a double-apply is a no-op.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 034 against the given connection. Idempotent: a second run (or a run
/// against a DB that already has the column) is a no-op. The column is NULLABLE with no
/// default, so existing (pre-034) rows are backfilled to NULL — the "predates per-chunk
/// classification" marker the serving path keys on (see the module doc).
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    if !column_exists(conn, "seed_vectors", "is_decl")? {
        conn.execute_batch("ALTER TABLE seed_vectors ADD COLUMN is_decl INTEGER;")?;
    }
    record_migration(conn, 34, "034-seed-chunk-decl")?;
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
    use crate::migrations::{migration_001, migration_033};

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        migration_033::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn adds_is_decl_column() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        assert!(column_exists(&conn, "seed_vectors", "is_decl").unwrap());
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error (column already present)
        assert!(column_exists(&conn, "seed_vectors", "is_decl").unwrap());
    }

    #[test]
    fn preexisting_row_backfills_is_decl_to_null_the_legacy_marker() {
        // review-1 item 2: a row written BEFORE this migration must be distinguishable
        // from a genuine `is_decl = 0` implementation, so the serving path can refuse to
        // present its stale per-file classification as fact. The column is NULLABLE with
        // no default, so the backfilled value is NULL — the "predates per-chunk
        // classification" marker `read_seed_vectors` keys on.
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
        // A pre-034 row (no is_decl column yet).
        conn.execute(
            "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,path,line,\
             qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector) \
             VALUES ('s1','n1','r1','k','a.rs',1,'a::f',0,'h','m','ck',1,x'00000000')",
            [],
        )
        .unwrap();
        run(&mut conn).unwrap();
        let is_decl: Option<i64> = conn
            .query_row(
                "SELECT is_decl FROM seed_vectors WHERE node_uid='n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            is_decl, None,
            "pre-existing row backfills to NULL (the legacy / not-yet-reclassified marker)"
        );
    }
}
