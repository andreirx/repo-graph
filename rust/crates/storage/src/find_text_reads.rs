//! FIND-GREP-1 (§2.2/§2.3) — the two read-only joins the `find --text` live scan
//! needs to (a) annotate a working-tree hit with its ENCLOSING stored symbol and
//! (b) label a per-file staleness note when the working tree diverged from the
//! snapshot the spans came from.
//!
//! Both are additive, bounded `SELECT`s over the CURRENT snapshot — no writes, no
//! schema. They follow the precedent set by [`crate::find_facts_reads`]
//! (FIND-FACTS-1): a `find`-serving read that its own file owns rather than growing
//! an existing reader. The two are BULK (whole-snapshot) rather than per-file: the
//! scan already holds every matched file in memory, and one indexed sweep of
//! `file_versions` / `nodes` is cheaper and more deterministic than N point lookups.
//!
//! Honesty (STANDING RULE): every method returns `Result<_, StorageError>`. A read
//! FAILURE propagates as `Err` so the caller degrades the whole scan
//! honestly-with-reason — never a silent empty that reads as "no spans / never
//! stale". A NULL span bound is DROPPED here (a span needs both ends to bound a
//! line); the caller then renders that hit WITHOUT annotation (visible absence,
//! never a guessed enclosing symbol).

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// One stored SYMBOL span, for enclosing-symbol annotation of a working-tree hit
/// (FIND-GREP-1 §2.2). Only spans with BOTH bounds present are emitted — a span
/// cannot bound a line without both, and a guessed bound is forbidden (STANDING
/// HONESTY RULE 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpanRow {
    /// Owning file's repo-relative path (the join key to a scanned file).
    pub path: String,
    /// Stored 1-based start line (`nodes.line_start`), never NULL here.
    pub line_start: i64,
    /// Stored 1-based end line (`nodes.line_end`), never NULL here.
    pub line_end: i64,
    /// Stored symbol KIND (`nodes.subtype`, e.g. `FUNCTION`, `CLASS`). `None` when
    /// the producer emitted no subtype — rendered as a bare `[<qualified_name>]`
    /// with no kind word, never a guessed kind.
    pub subtype: Option<String>,
    /// Stored bare name (`nodes.name`, NOT NULL) — the annotation fallback when no
    /// qualified name was extracted.
    pub name: String,
    /// Stored fully-qualified name (`nodes.qualified_name`). `None`/empty → the
    /// annotation falls back to `name`.
    pub qualified_name: Option<String>,
}

/// One stored file version's content hash, for the staleness compare
/// (FIND-GREP-1 §2.3). The hash is the SAME `SHA-256(bytes).hex[..16]` the scanner
/// computes (`repo_graph_repo_index::scanner::hash_content`), so a byte-for-byte
/// working-tree match compares equal and a single edited byte compares unequal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFileVersionRow {
    pub path: String,
    pub content_hash: String,
}

impl StorageConnection {
    /// FIND-GREP-1 (§2.2): every SYMBOL span of the snapshot with BOTH line bounds
    /// present, joined to its owning file path. The caller buckets these by path and
    /// picks the innermost containing span per hit line. Deterministic order (path,
    /// then start line) so a tie in containment resolves reproducibly.
    pub fn find_text_symbol_spans(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<TextSpanRow>, StorageError> {
        let mut stmt = self.connection().prepare(
            "SELECT f.path, n.line_start, n.line_end, n.subtype, n.name, n.qualified_name
             FROM nodes n
             JOIN files f ON n.file_uid = f.file_uid
             WHERE n.snapshot_uid = ?1
               AND n.kind = 'SYMBOL'
               AND n.line_start IS NOT NULL
               AND n.line_end IS NOT NULL
             ORDER BY f.path ASC, n.line_start ASC, n.line_end ASC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid], |row| {
                Ok(TextSpanRow {
                    path: row.get(0)?,
                    line_start: row.get(1)?,
                    line_end: row.get(2)?,
                    subtype: row.get(3)?,
                    name: row.get(4)?,
                    qualified_name: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// FIND-GREP-1 (§2.3): every tracked file version of the snapshot with its stored
    /// content hash. The caller maps `path -> content_hash` and compares each scanned
    /// file's live hash: equal → fresh; unequal → the working tree diverged since the
    /// snapshot (the staleness note); absent → the file is not in the snapshot at all
    /// (no stored span context to be stale about).
    pub fn find_text_file_versions(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<TextFileVersionRow>, StorageError> {
        let mut stmt = self.connection().prepare(
            "SELECT f.path, fv.content_hash
             FROM file_versions fv
             JOIN files f ON f.file_uid = fv.file_uid
             WHERE fv.snapshot_uid = ?1
             ORDER BY f.path ASC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid], |row| {
                Ok(TextFileVersionRow {
                    path: row.get(0)?,
                    content_hash: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
