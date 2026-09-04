//! Migration 033 — per-snapshot seed vectors (`seed_vectors`) for SEED-CHUNK-1.
//!
//! Adds ONE additive table holding the static-embedding vector for each SYMBOL
//! chunk of a snapshot. This is the ratified replacement for the retired `.vec`
//! sidecar: the vectors now live per-snapshot in SQLite, keyed to node identity,
//! model-stamped, and copy-forward-reused on refresh (spec §3).
//!
//! ```text
//! seed_vectors(
//!   snapshot_uid   -- the snapshot these vectors belong to (CASCADE on delete)
//!   node_uid       -- the SYMBOL node (snapshot-scoped unique); PK part
//!   repo_uid       -- owning repo (CASCADE on delete)
//!   stable_key     -- the node's cross-snapshot identity; the copy-forward key
//!   file_uid       -- the owning file's uid (the module-ownership lookup key); nullable
//!   path           -- repo-relative file path (the `path:line` anchor, denormalized)
//!   line           -- line_start anchor; NULL when the node had no span (never a 0)
//!   qualified_name -- the symbol's qualified name for the anchor; NULL when absent
//!   is_test        -- the file's is_test at embed time (0/1); the partition input
//!   content_hash   -- the file_versions pin; the copy-forward freshness key
//!   model_id       -- the embedding model identity stamp; a change invalidates
//!   model_checksum -- sha256 of the model FILES the writing pass loaded (spec §2
//!                     "checksum recorded"): recorded provenance of the embedding
//!                     regime, homogeneous per snapshot, surfaced by the doctor
//!   dim            -- vector dimensionality
//!   vector         -- BLOB, `dim` little-endian f32 (dim*4 bytes)
//! )
//! PRIMARY KEY (snapshot_uid, node_uid)
//! ```
//!
//! # Why per-snapshot + node-keyed (not a single sidecar)
//!
//! Vectors are derived per-snapshot data, exactly like `measurements` and
//! `semantic_facts` (the house norm). Serving reads the CURRENT snapshot's rows;
//! when the latest snapshot has no rows yet (the async pass has not run, or a
//! pre-migration snapshot), the seed tier renders honestly absent WITH ITS REASON
//! — never a stale cross-snapshot fallback (STANDING HONESTY RULE 1).
//!
//! # Copy-forward identity is (stable_key, content_hash, model_id, model_checksum, dim), NOT node_uid
//!
//! `node_uid` is snapshot-scoped (a symbol has a different `node_uid` each
//! snapshot), so copy-forward reuse matches across snapshots on `stable_key` (stable
//! identity) + `content_hash` (unchanged file version), and is eligible ONLY when the
//! prior row's embedding regime equals the current one on ALL THREE regime fields:
//! `model_id` + `model_checksum` (the model BYTES — a byte change at the same repo id
//! is a different regime) + `dim`. `read_prior_seed_vectors` enforces this in its WHERE
//! clause (`snapshot_uid, model_id, model_checksum, dim`); a mismatch on any field
//! yields no reuse → full re-embed (rebuild semantics, spec §3). Filtering on `model_id`
//! ALONE (the pre-review-1 bug) would copy stale vectors forward under a silently-swapped
//! model and re-stamp them with the new checksum — a false provenance claim.
//!
//! The `idx_seed_vectors_carryforward` index below is a `(snapshot_uid, stable_key,
//! content_hash, model_id)` prefix accelerator for the parent read; `model_checksum`/`dim`
//! are applied as residual filters (`snapshot_uid` is already highly selective).
//!
//! # Refresh behavior
//!
//! Rows are written per-snapshot by the background seed pass; unchanged file
//! versions copy their vector forward from the prior snapshot (no re-embed). The
//! `ON DELETE CASCADE` on `snapshot_uid` means snapshot retention prunes old
//! vectors with their snapshot — no orphan sweep needed.
//!
//! # Idempotence
//!
//! `CREATE TABLE IF NOT EXISTS` + `INSERT OR IGNORE` migration row: safe to re-run.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 033 against the given connection.
///
/// Idempotent: re-running on a database that already has the table is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS seed_vectors (
           snapshot_uid   TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
           node_uid       TEXT NOT NULL,
           repo_uid       TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
           stable_key     TEXT NOT NULL,
           file_uid       TEXT,
           path           TEXT NOT NULL,
           line           INTEGER,
           qualified_name TEXT,
           is_test        INTEGER NOT NULL,
           content_hash   TEXT NOT NULL,
           model_id       TEXT NOT NULL,
           model_checksum TEXT NOT NULL,
           dim            INTEGER NOT NULL,
           vector         BLOB NOT NULL,
           PRIMARY KEY (snapshot_uid, node_uid)
         );
         CREATE INDEX IF NOT EXISTS idx_seed_vectors_snapshot ON seed_vectors(snapshot_uid);
         CREATE INDEX IF NOT EXISTS idx_seed_vectors_carryforward
           ON seed_vectors(snapshot_uid, stable_key, content_hash, model_id);",
    )?;

    record_migration(conn, 33, "033-seed-vectors")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::migration_001;

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        conn
    }

    fn table_exists(conn: &Connection, table: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            rusqlite::params![table],
            |_| Ok(()),
        )
        .is_ok()
    }

    #[test]
    fn creates_seed_vectors_table() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "seed_vectors"));
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error
    }

    #[test]
    fn stores_and_reads_a_vector_blob_roundtrip() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
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
        let vec_bytes: Vec<u8> = [1.0f32, 0.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        conn.execute(
            "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,path,line,\
             qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector) \
             VALUES ('s1','n1','r1','r1:a:SYMBOL','a.rs',10,'a::f',0,'h1','m1','ck1',2,?)",
            rusqlite::params![vec_bytes],
        )
        .unwrap();
        let got: Vec<u8> = conn
            .query_row(
                "SELECT vector FROM seed_vectors WHERE snapshot_uid='s1' AND node_uid='n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(got, vec_bytes);
    }

    #[test]
    fn deleting_snapshot_cascades_vectors() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
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
        conn.execute(
            "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,path,line,\
             qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector) \
             VALUES ('s1','n1','r1','r1:a:SYMBOL','a.rs',10,'a::f',0,'h1','m1','ck1',1,x'00000000')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM snapshots WHERE snapshot_uid='s1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM seed_vectors", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "vectors must cascade with their snapshot");
    }
}
