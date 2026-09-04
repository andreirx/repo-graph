//! `SeedCorpusRead` impl + seed-vector CRUD for `StorageConnection` (SEED-CHUNK-1).
//!
//! Adapter → policy: the pure `repo-graph-seed` crate defines the ports + DTOs;
//! this outer SQLite adapter fills them. Identical direction to `AgentStorageRead`
//! (`src/agent_impl.rs`). The pure seed logic never imports this crate.
//!
//! The corpus is now per-SYMBOL **chunks** from `nodes` (spec §2.1), joined to
//! `files` (path + is_test) and the READY snapshot's `file_versions.content_hash`
//! pin. Test symbols are INCLUDED — the serving partition demotes them, it no
//! longer drops them (build-0 handoff phase 4). Vectors live in the per-snapshot
//! `seed_vectors` table (migration 033), replacing the retired `.vec` sidecar.

use std::collections::HashMap;

use rusqlite::OptionalExtension;

use repo_graph_seed::{
    SeedCorpus, SeedCorpusEntry, SeedCorpusError, SeedCorpusRead, SeedVectorEntry,
    StoredSeedVectors,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Encode an f32 vector as a little-endian BLOB (`dim * 4` bytes).
fn encode_vector(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Decode a little-endian f32 BLOB. A length not divisible by 4 is a corrupt row —
/// surfaced as a read error, never a silently truncated/padded vector.
fn decode_vector(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if !bytes.len().is_multiple_of(4) {
        return Err(format!(
            "seed vector blob length {} is not a multiple of 4",
            bytes.len()
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

impl SeedCorpusRead for StorageConnection {
    fn seed_corpus(&self, repo_uid: &str) -> Result<SeedCorpus, SeedCorpusError> {
        // Resolve the current READY snapshot. Un-indexed repo / no READY snapshot ⇒
        // `snapshot_uid = None` (not indexed), NOT an error.
        let snapshot = self
            .get_latest_snapshot(repo_uid)
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;
        let snapshot_uid = match snapshot {
            Some(s) => s.snapshot_uid,
            None => {
                return Ok(SeedCorpus {
                    snapshot_uid: None,
                    entries: Vec::new(),
                })
            }
        };

        let conn = self.connection();
        // One chunk per SYMBOL node WITH a span. Test symbols INCLUDED (the partition
        // demotes them, spec §5). Generated/excluded files are still never embedded.
        let mut stmt = conn
            .prepare(
                "SELECT n.node_uid, n.stable_key, n.file_uid, f.path, n.qualified_name, \
                        n.doc_comment, n.line_start, n.line_end, f.is_test, fv.content_hash \
                 FROM nodes n \
                 JOIN files f ON f.file_uid = n.file_uid \
                 JOIN file_versions fv \
                   ON fv.file_uid = n.file_uid AND fv.snapshot_uid = ? \
                 WHERE n.snapshot_uid = ? \
                   AND n.kind = 'SYMBOL' \
                   AND n.line_start IS NOT NULL \
                   AND f.is_generated = 0 AND f.is_excluded = 0 \
                 ORDER BY f.path ASC, n.line_start ASC",
            )
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, snapshot_uid], |row| {
                let is_test_i: i64 = row.get(8)?;
                Ok(SeedCorpusEntry {
                    node_uid: row.get(0)?,
                    stable_key: row.get(1)?,
                    file_uid: row.get(2)?,
                    path: row.get(3)?,
                    qualified_name: row.get(4)?,
                    doc_comment: row.get(5)?,
                    line_start: row.get(6)?,
                    line_end: row.get(7)?,
                    is_test: is_test_i != 0,
                    content_hash: row.get(9)?,
                })
            })
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let entries = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;
        Ok(SeedCorpus {
            snapshot_uid: Some(snapshot_uid),
            entries,
        })
    }

    fn read_seed_vectors(&self, snapshot_uid: &str) -> Result<StoredSeedVectors, SeedCorpusError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT node_uid, stable_key, file_uid, path, line, qualified_name, \
                        is_test, content_hash, model_id, model_checksum, dim, vector \
                 FROM seed_vectors WHERE snapshot_uid = ?",
            )
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid], |row| {
                let is_test_i: i64 = row.get(6)?;
                let model_id: String = row.get(8)?;
                let model_checksum: String = row.get(9)?;
                let dim: i64 = row.get(10)?;
                let blob: Vec<u8> = row.get(11)?;
                Ok((
                    model_id,
                    model_checksum,
                    dim as u32,
                    SeedVectorEntry {
                        node_uid: row.get(0)?,
                        stable_key: row.get(1)?,
                        file_uid: row.get(2)?,
                        path: row.get(3)?,
                        line: row.get(4)?,
                        qualified_name: row.get(5)?,
                        is_test: is_test_i != 0,
                        content_hash: row.get(7)?,
                        vector: Vec::new(), // filled after blob decode
                    },
                    blob,
                ))
            })
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        // A snapshot's rows are written in ONE pass under ONE model, so the set must be
        // homogeneous in (model_id, model_checksum, dim) and every vector must decode to
        // exactly `dim` floats. A row that violates either is CORRUPTION — surfaced as a
        // read error (rendered "present but unreadable; rebuild on next index"), NEVER
        // zip-truncated into a cosine score (review-1 gap b; STANDING HONESTY RULE 1).
        let mut stamp: Option<(String, String, u32)> = None;
        let mut entries: Vec<SeedVectorEntry> = Vec::new();
        for row in rows {
            let (mid, ck, d, mut entry, blob) =
                row.map_err(|e| SeedCorpusError::Read(e.to_string()))?;
            match &stamp {
                Some((m, c, dd)) if *m != mid || *c != ck || *dd != d => {
                    return Err(SeedCorpusError::Read(format!(
                        "seed vectors are not homogeneous: row stamp ({mid}, {ck}, dim {d}) \
                         differs from ({m}, {c}, dim {dd}); rebuild on next index"
                    )));
                }
                Some(_) => {}
                None => stamp = Some((mid, ck, d)),
            }
            let decoded = decode_vector(&blob).map_err(SeedCorpusError::Read)?;
            if decoded.len() != d as usize {
                return Err(SeedCorpusError::Read(format!(
                    "seed vector for node {} decoded to {} floats but its row dim is {}; \
                     rebuild on next index",
                    entry.node_uid,
                    decoded.len(),
                    d
                )));
            }
            entry.vector = decoded;
            entries.push(entry);
        }
        let (model_id, model_checksum, dim) = match stamp {
            Some((m, c, d)) => (Some(m), Some(c), Some(d)),
            None => (None, None, None),
        };
        Ok(StoredSeedVectors {
            model_id,
            model_checksum,
            dim,
            entries,
        })
    }

    fn module_owners(
        &self,
        snapshot_uid: &str,
        file_uids: &[String],
    ) -> Result<HashMap<String, String>, SeedCorpusError> {
        if file_uids.is_empty() {
            return Ok(HashMap::new());
        }

        // Genuine ownership (operator ruling 2026-08-25): `module_file_ownership` ⨝
        // `module_candidates`, most-specific (longest `canonical_root_path`) winner.
        let placeholders = vec!["?"; file_uids.len()].join(", ");
        let sql = format!(
            "SELECT o.file_uid AS file_uid, mc.canonical_root_path AS module_path \
             FROM module_file_ownership o \
             JOIN module_candidates mc \
               ON mc.module_candidate_uid = o.module_candidate_uid \
              AND mc.snapshot_uid = o.snapshot_uid \
             WHERE o.snapshot_uid = ? \
               AND o.file_uid IN ({placeholders})"
        );

        let conn = self.connection();
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(file_uids.len() + 1);
        params.push(&snapshot_uid);
        for uid in file_uids {
            params.push(uid);
        }

        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>("file_uid")?,
                    row.get::<_, String>("module_path")?,
                ))
            })
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let mut owners: HashMap<String, String> = HashMap::new();
        for row in rows {
            let (file_uid, module_path) = row.map_err(|e| SeedCorpusError::Read(e.to_string()))?;
            owners
                .entry(file_uid)
                .and_modify(|cur| {
                    if module_path.len() > cur.len() {
                        *cur = module_path.clone();
                    }
                })
                .or_insert(module_path);
        }
        Ok(owners)
    }
}

impl StorageConnection {
    /// Persist a snapshot's seed vectors (SEED-CHUNK-1, spec §3). Idempotent rebuild:
    /// DELETEs any prior rows for `snapshot_uid` then inserts the new set, in ONE
    /// transaction, so a serve never observes a half-written set. `model_id`/`dim`
    /// are the homogeneous stamp; a later model change is detected on read.
    ///
    /// The caller (the daemon seed pass) invokes this UNDER the DB write-slot guard,
    /// preserving the forget-vs-seed / generation-supersede publication invariants
    /// (review-5 #2 / review-10 #3) — the write is the fast publish, never the embed.
    pub fn write_seed_vectors(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        model_id: &str,
        model_checksum: &str,
        dim: u32,
        entries: &[SeedVectorEntry],
    ) -> Result<(), StorageError> {
        let conn = self.connection();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM seed_vectors WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO seed_vectors \
                 (snapshot_uid, node_uid, repo_uid, stable_key, file_uid, path, line, \
                  qualified_name, is_test, content_hash, model_id, model_checksum, dim, vector) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )?;
            for e in entries {
                stmt.execute(rusqlite::params![
                    snapshot_uid,
                    e.node_uid,
                    repo_uid,
                    e.stable_key,
                    e.file_uid,
                    e.path,
                    e.line,
                    e.qualified_name,
                    e.is_test as i64,
                    e.content_hash,
                    model_id,
                    model_checksum,
                    dim as i64,
                    encode_vector(&e.vector),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Read the PARENT snapshot's seed vectors for copy-forward (spec §5), filtered to
    /// the CALLER'S CURRENT model IDENTITY — `(model_id, model_checksum, dim)`. Only a
    /// vector written by EXACTLY this model build is eligible to copy forward; anything
    /// else (a different model id, the SAME id but DIFFERENT bytes, or a different dim)
    /// is not returned, so a model change hands back an empty vec → full re-embed, the
    /// rebuild semantics (spec §3, zg precedent).
    ///
    /// The `model_checksum` filter is the fix for review-1: filtering on `model_id`
    /// alone let a silently-swapped model at the SAME repo id copy stale vectors forward
    /// and re-stamp them with the new checksum in `write_seed_vectors` — a false
    /// provenance claim. The checksum in the WHERE clause makes byte-level model change
    /// invalidate reuse. `dim` is included so a (hypothetical) same-id/same-checksum row
    /// of the wrong length is never fed to the cosine ranker.
    ///
    /// Returns empty when `snapshot_uid` has no parent, the parent has no matching
    /// vectors, or every matching row's blob is corrupt. A blob whose decoded length ≠
    /// `dim` is CORRUPTION — surfaced as an error (the caller, a pure build-time
    /// optimization read, degrades to a full re-embed), never a truncated vector.
    pub fn read_prior_seed_vectors(
        &self,
        snapshot_uid: &str,
        model_id: &str,
        model_checksum: &str,
        dim: u32,
    ) -> Result<Vec<SeedVectorEntry>, StorageError> {
        let conn = self.connection();
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_snapshot_uid FROM snapshots WHERE snapshot_uid = ?",
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let parent_uid = match parent {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let mut stmt = conn.prepare(
            "SELECT node_uid, stable_key, file_uid, path, line, qualified_name, \
                    is_test, content_hash, vector \
             FROM seed_vectors \
             WHERE snapshot_uid = ? AND model_id = ? AND model_checksum = ? AND dim = ?",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![parent_uid, model_id, model_checksum, dim as i64],
            |row| {
                let is_test_i: i64 = row.get(6)?;
                let blob: Vec<u8> = row.get(8)?;
                Ok((
                    SeedVectorEntry {
                        node_uid: row.get(0)?,
                        stable_key: row.get(1)?,
                        file_uid: row.get(2)?,
                        path: row.get(3)?,
                        line: row.get(4)?,
                        qualified_name: row.get(5)?,
                        is_test: is_test_i != 0,
                        content_hash: row.get(7)?,
                        vector: Vec::new(),
                    },
                    blob,
                ))
            },
        )?;
        let mut out = Vec::new();
        for row in rows {
            let (mut e, blob) = row?;
            let decoded = decode_vector(&blob).map_err(StorageError::InvalidArgument)?;
            if decoded.len() != dim as usize {
                return Err(StorageError::InvalidArgument(format!(
                    "prior seed vector for node {} decoded to {} floats but its row dim is {}; \
                     full re-embed",
                    e.node_uid,
                    decoded.len(),
                    dim
                )));
            }
            e.vector = decoded;
            out.push(e);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::StorageConnection;

    fn seed_scaffold(storage: &StorageConnection) {
        let conn = storage.connection();
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
    }

    /// Raw INSERT of one seed_vectors row with an explicit (model_id, checksum, dim, blob).
    fn insert_row(
        storage: &StorageConnection,
        node: &str,
        model_id: &str,
        checksum: &str,
        dim: i64,
        blob: &[u8],
    ) {
        storage
            .connection()
            .execute(
                "INSERT INTO seed_vectors (snapshot_uid,node_uid,repo_uid,stable_key,file_uid,path,\
                 line,qualified_name,is_test,content_hash,model_id,model_checksum,dim,vector) \
                 VALUES ('s1',?,'r1',?,'fu1','a.rs',10,'a::f',0,'h1',?,?,?,?)",
                rusqlite::params![node, node, model_id, checksum, dim, blob],
            )
            .unwrap();
    }

    fn f32_blob(v: &[f32]) -> Vec<u8> {
        encode_vector(v)
    }

    #[test]
    fn write_then_read_roundtrips_the_model_checksum() {
        let storage = StorageConnection::open_in_memory().unwrap();
        seed_scaffold(&storage);
        let entry = SeedVectorEntry {
            node_uid: "n1".into(),
            stable_key: "k1".into(),
            file_uid: "fu1".into(),
            path: "a.rs".into(),
            line: Some(10),
            qualified_name: Some("a::f".into()),
            is_test: false,
            content_hash: "h1".into(),
            vector: vec![1.0, 0.0],
        };
        storage
            .write_seed_vectors(
                "s1",
                "r1",
                "m1",
                "sha256:deadbeef",
                2,
                std::slice::from_ref(&entry),
            )
            .unwrap();
        let stored = storage.read_seed_vectors("s1").unwrap();
        assert_eq!(stored.model_id.as_deref(), Some("m1"));
        assert_eq!(stored.model_checksum.as_deref(), Some("sha256:deadbeef"));
        assert_eq!(stored.dim, Some(2));
        assert_eq!(stored.entries.len(), 1);
        assert_eq!(stored.entries[0].vector, vec![1.0, 0.0]);
    }

    #[test]
    fn mixed_model_stamps_are_rejected_at_read_not_scored() {
        // gap b: a heterogeneous store is corruption — surfaced as a read error, never
        // handed to the ranker (which would score across differing regimes).
        let storage = StorageConnection::open_in_memory().unwrap();
        seed_scaffold(&storage);
        insert_row(&storage, "n1", "modelA", "ckA", 2, &f32_blob(&[1.0, 0.0]));
        insert_row(&storage, "n2", "modelB", "ckB", 2, &f32_blob(&[0.0, 1.0]));
        let err = storage.read_seed_vectors("s1").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not homogeneous"), "surfaced reason: {msg}");
    }

    #[test]
    fn vector_length_not_matching_dim_is_rejected_at_read() {
        // gap b: a blob whose decoded length ≠ its row `dim` is corruption — rejected,
        // never zip-truncated into a cosine score.
        let storage = StorageConnection::open_in_memory().unwrap();
        seed_scaffold(&storage);
        // dim stamped 3, but the blob decodes to only 2 f32s.
        insert_row(&storage, "n1", "m1", "ck1", 3, &f32_blob(&[1.0, 0.0]));
        let err = storage.read_seed_vectors("s1").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("decoded to 2 floats") && msg.contains("dim is 3"),
            "surfaced reason names the mismatch: {msg}"
        );
    }

    #[test]
    fn blob_not_multiple_of_four_is_rejected_at_read() {
        let storage = StorageConnection::open_in_memory().unwrap();
        seed_scaffold(&storage);
        insert_row(&storage, "n1", "m1", "ck1", 1, &[0u8, 1u8, 2u8]); // 3 bytes
        let err = storage.read_seed_vectors("s1").unwrap_err();
        assert!(format!("{err}").contains("not a multiple of 4"));
    }

    /// Scaffold a parent→child snapshot pair so copy-forward has a parent to read.
    /// `s1` (the scaffold's READY snapshot) is the PARENT holding vectors; `s2` is the
    /// child whose refresh reads the parent's vectors.
    fn parent_child_scaffold(storage: &StorageConnection) {
        seed_scaffold(storage); // inserts repo r1 + snapshot s1
        storage
            .connection()
            .execute(
                "INSERT INTO snapshots \
                 (snapshot_uid, repo_uid, kind, status, created_at, parent_snapshot_uid) \
                 VALUES ('s2','r1','full','ready','2025-01-02T00:00:00Z','s1')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn copy_forward_reuses_only_vectors_of_the_exact_model_stamp() {
        // review-1 defect: the copy-forward read filtered on `model_id` ALONE, so a
        // model whose BYTES changed under the same id would copy stale vectors forward
        // and `write_seed_vectors` would re-stamp them with the NEW checksum — a false
        // provenance claim. The read now filters on (model_id, model_checksum, dim):
        //  - a matching stamp copies forward;
        //  - the SAME id with a DIFFERENT checksum returns EMPTY → full re-embed.
        let storage = StorageConnection::open_in_memory().unwrap();
        parent_child_scaffold(&storage);
        // Parent s1 holds one vector stamped model "m1" / checksum "sha256:AAA" / dim 2.
        insert_row(
            &storage,
            "n1",
            "m1",
            "sha256:AAA",
            2,
            &f32_blob(&[1.0, 0.0]),
        );

        // Same model + same checksum ⇒ eligible to copy forward.
        let reused = storage
            .read_prior_seed_vectors("s2", "m1", "sha256:AAA", 2)
            .unwrap();
        assert_eq!(reused.len(), 1, "matching-stamp vector copies forward");
        assert_eq!(reused[0].vector, vec![1.0, 0.0]);

        // Same model id, CHANGED bytes (different checksum) ⇒ NOT reused → full re-embed.
        let changed = storage
            .read_prior_seed_vectors("s2", "m1", "sha256:BBB", 2)
            .unwrap();
        assert!(
            changed.is_empty(),
            "a byte-changed model under the same id must NOT copy stale vectors forward"
        );

        // A different dim under the same id/checksum is also ineligible.
        let wrong_dim = storage
            .read_prior_seed_vectors("s2", "m1", "sha256:AAA", 3)
            .unwrap();
        assert!(wrong_dim.is_empty(), "a different dim is not reused");
    }

    #[test]
    fn copy_forward_rejects_a_length_mismatched_prior_row() {
        // A parent row whose stamped dim matches the filter but whose blob decodes to a
        // different length is corruption — surfaced as an error so the caller degrades
        // to a full re-embed, never a truncated vector copied forward.
        let storage = StorageConnection::open_in_memory().unwrap();
        parent_child_scaffold(&storage);
        // dim stamped 3 (passes the WHERE filter) but the blob decodes to only 2 f32s.
        insert_row(
            &storage,
            "n1",
            "m1",
            "sha256:AAA",
            3,
            &f32_blob(&[1.0, 0.0]),
        );
        let err = storage
            .read_prior_seed_vectors("s2", "m1", "sha256:AAA", 3)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("decoded to 2 floats") && msg.contains("dim is 3"),
            "surfaced reason names the mismatch: {msg}"
        );
    }
}
