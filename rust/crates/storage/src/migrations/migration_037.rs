//! Migration 037 — per-CHUNK embedding-document digest on `seed_vectors` (SEED-CHUNK-3, review-3).
//!
//! Adds ONE additive column, `document_hash TEXT`, to the `seed_vectors` table. It stores the
//! 16-hex digest (`repo_graph_seed::hash::content_hash`) of the ASSEMBLED chunk document
//! (`build_chunk_document`: qualified_name + doc_comment + capped span) — the exact bytes the
//! embedder saw. It is the copy-forward REUSE discriminator:
//! `read_prior_seed_vectors` returns it, and `build_store` keys reuse on
//! `(stable_key, content_hash, document_hash)`.
//!
//! ```text
//! seed_vectors ADD COLUMN document_hash TEXT          (NULLABLE, no default)
//! ```
//!
//! # Why this column exists — the review-3 defect
//!
//! Copy-forward previously reused a prior vector when `(stable_key, content_hash)` was unchanged,
//! where `content_hash` is the FILE hash. That silently assumed the file hash captures everything
//! that goes into the embedded document. It does NOT: the document also carries the chunk's
//! `doc_comment`, which is EXTRACTOR-DERIVED and — as of SEED-CHUNK-3 — can change WITHOUT the file
//! bytes changing (a property whose leading `//` run the extractor previously discarded now becomes
//! its doc on the next re-index, the file byte-identical). Reuse-by-file-hash would then re-stamp
//! the OLD (docless) vector as current — presenting an embedding built from a document that no
//! longer matches the chunk (STANDING HONESTY RULE 1). Keying reuse on the document digest as well
//! makes ANY change to the embedded document — file bytes OR extractor logic — invalidate the reuse.
//!
//! # Why NULLABLE with NO default — and why NO serve-time refusal (unlike 034/036)
//!
//! `document_hash` is a BUILD-TIME reuse key only; it is NEVER a served/rendered fact (the
//! query/render path ranks stored vectors, it does not re-embed). So a legacy row that predates
//! this migration does not make the SERVE path dishonest — its `is_test`/`is_decl`/`is_field`
//! (migrations 034/036) are still correct and served. It only makes that row ineligible for
//! copy-forward: `read_prior_seed_vectors` reads the column as `Option`, and `build_store` DROPS a
//! prior entry with a NULL digest from the reuse map, so it is RE-EMBEDDED (the one-time self-heal
//! transition after the upgrade), never reused blind. Hence — unlike migration 034 (`is_decl`) and
//! 036 (`is_field`), whose NULLs the serve path must REFUSE — this migration needs no
//! `StaleClassification` refusal and no SeedCoordinator latch: the very next index/refresh
//! re-embeds unchanged chunks against a NULL-digest parent and writes their digests, and every
//! subsequent refresh reuses correctly.
//!
//! # Idempotence
//!
//! `ALTER TABLE … ADD COLUMN` is not itself `IF NOT EXISTS`-guarded in SQLite, so the runner is
//! version-gated (`max_version < 37` in `run_migrations`); re-running this function directly checks
//! `PRAGMA table_info` first so a double-apply is a no-op.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 037 against the given connection. Idempotent: a second run (or a run against a DB
/// that already has the column) is a no-op. The column is NULLABLE with no default, so existing
/// (pre-037) rows are backfilled to NULL — the "document identity unknown; re-embed on next index"
/// marker `read_prior_seed_vectors` + `build_store` key on (see the module doc).
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    if !column_exists(conn, "seed_vectors", "document_hash")? {
        conn.execute_batch("ALTER TABLE seed_vectors ADD COLUMN document_hash TEXT;")?;
    }
    record_migration(conn, 37, "037-seed-chunk-document-hash")?;
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
    use crate::migrations::{migration_001, migration_033, migration_034, migration_036};

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        migration_033::run(&mut conn).unwrap();
        migration_034::run(&mut conn).unwrap();
        migration_036::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn adds_document_hash_column() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        assert!(column_exists(&conn, "seed_vectors", "document_hash").unwrap());
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error (column already present)
        assert!(column_exists(&conn, "seed_vectors", "document_hash").unwrap());
    }

    #[test]
    fn preexisting_row_backfills_document_hash_to_null_the_reembed_marker() {
        // A row written BEFORE this migration must backfill to a NULL document_hash — the marker
        // `build_store` keys on to EXCLUDE the row from copy-forward (re-embed it) rather than
        // reuse a vector whose embedding-document identity is unknown. The column is NULLABLE with
        // no default.
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
        // A pre-037 row: is_decl + is_field present (post-036) but no document_hash column yet.
        conn.execute(
            "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,path,line,\
             qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector,is_decl,is_field) \
             VALUES ('s1','n1','r1','k','a.rs',1,'a::f',0,'h','m','ck',1,x'00000000',0,0)",
            [],
        )
        .unwrap();
        run(&mut conn).unwrap();
        let document_hash: Option<String> = conn
            .query_row(
                "SELECT document_hash FROM seed_vectors WHERE node_uid='n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            document_hash, None,
            "pre-existing row backfills to NULL (the re-embed marker for copy-forward)"
        );
    }
}
