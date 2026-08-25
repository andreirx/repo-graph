//! `SeedCorpusRead` impl for `StorageConnection` (EMBED-SEED-IMPL-1).
//!
//! Adapter → policy: the pure `repo-graph-seed` crate defines the port + DTO;
//! this outer SQLite adapter fills it. Identical direction to `AgentStorageRead`
//! (`src/agent_impl.rs`). The pure seed logic never imports this crate.
//!
//! The corpus is exactly what `files` already holds, filtered by the scanner's
//! own exclusion flags (spec §3.1/§3.3) and joined to the READY snapshot's
//! `file_versions.content_hash` pin — **no new extraction, no new classification**.

use std::collections::HashMap;

use repo_graph_seed::{SeedCorpusEntry, SeedCorpusError, SeedCorpusRead};

use crate::connection::StorageConnection;

impl SeedCorpusRead for StorageConnection {
    fn seed_corpus(&self, repo_uid: &str) -> Result<Vec<SeedCorpusEntry>, SeedCorpusError> {
        // Resolve the current READY snapshot (reuses the canonical query). An
        // un-indexed repo / no READY snapshot ⇒ empty corpus, NOT an error
        // (I4: "no vector store yet", never a failure).
        let snapshot = self
            .get_latest_snapshot(repo_uid)
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;
        let snapshot_uid = match snapshot {
            Some(s) => s.snapshot_uid,
            None => return Ok(Vec::new()),
        };

        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT f.file_uid, f.path, fv.content_hash \
                 FROM files f \
                 JOIN file_versions fv \
                   ON fv.file_uid = f.file_uid AND fv.snapshot_uid = ? \
                 WHERE f.repo_uid = ? \
                   AND f.is_test = 0 AND f.is_generated = 0 AND f.is_excluded = 0 \
                 ORDER BY f.path ASC",
            )
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, repo_uid], |row| {
                Ok(SeedCorpusEntry {
                    file_uid: row.get(0)?,
                    path: row.get(1)?,
                    content_hash: row.get(2)?,
                })
            })
            .map_err(|e| SeedCorpusError::Read(e.to_string()))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| SeedCorpusError::Read(e.to_string()))
    }

    fn module_owners(
        &self,
        snapshot_uid: &str,
        file_uids: &[String],
    ) -> Result<HashMap<String, String>, SeedCorpusError> {
        if file_uids.is_empty() {
            return Ok(HashMap::new());
        }

        // Genuine ownership: `module_file_ownership` ⨝ `module_candidates`, projecting
        // the module's display path (`canonical_root_path`). A file with several
        // ownership rows resolves to its MOST-SPECIFIC module (longest
        // `canonical_root_path`) — the same longest-prefix winner
        // `call_resolution_reads::file_candidate_cte` uses so a file's module here
        // matches the module it is displayed under everywhere else. No new table,
        // no new classification (operator ruling 2026-08-25).
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

        // Params: snapshot_uid, then each requested file_uid.
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

        // Keep the longest (most-specific) canonical_root_path per file.
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
