//! DEPS-LIST-REWRITE-1 §2.2: per-language file counts for dominant-language ecosystem selection.
//!
//! Crate-private module holding a single `StorageConnection` read, kept OUT of the already
//! 5800-line `queries.rs` (gap 7 — new logic goes in its own module, never grows the god-file).
//! Mirrors the `http_surface_read` split pattern.
//!
//! The read is `pub(crate)` and reached cross-crate ONLY through the `AgentStorageRead` port
//! (operator ruling 2, 2026-08-26 — no new public inherent storage API for a single consumer).

use crate::error::StorageError;
use crate::StorageConnection;

impl StorageConnection {
    /// File count per language for a snapshot (DEPS-LIST-REWRITE-1 §2.2 — dominant indexed
    /// language). Read-only; the same `files ⋈ file_versions` join `compute_repo_summary` uses for
    /// its DISTINCT-language list, here grouped with a count so the caller can pick the plurality
    /// language (which selects the dependency manifest ecosystem) instead of "any TS/JS file
    /// present". Sorted by count DESC then language ASC so ties are deterministic.
    pub(crate) fn query_file_count_by_language(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<(String, u64)>, StorageError> {
        let mut stmt = self.connection().prepare(
            "SELECT f.language, COUNT(DISTINCT f.file_uid) AS n \
             FROM files f \
             JOIN file_versions fv ON fv.file_uid = f.file_uid \
             WHERE fv.snapshot_uid = ? \
               AND f.language IS NOT NULL \
             GROUP BY f.language \
             ORDER BY n DESC, f.language ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u64,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}
