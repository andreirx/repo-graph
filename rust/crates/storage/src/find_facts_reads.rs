//! FIND-FACTS-1 (§2.1) — deterministic lexical (`LIKE`) reads over the CURRENT
//! snapshot's fact tables, serving the `find` verb's FACTS tier.
//!
//! These are the four fact classes whose corpus is a plain graph/discovery table
//! that a substring query reaches best with SQL-side filtering: SYMBOL nodes, file
//! paths, declared module candidates, and governance DECLARATIONS (the `boundary`
//! fact class). The three remaining fact classes the `find` handler assembles from
//! EXISTING read paths it already owns (HTTP routes via `http_surface_union`,
//! dependency names via the deps compose, framework inferences via
//! `list_inferences_for_snapshot`) — never re-queried here, so their sourcing stays
//! single-authority.
//!
//! FIND-FACTS-1 review-6 (operator-ratified 2026-08-30): the `boundary` class was
//! RE-HOMED off `surface_entrypoints` onto the governance DECLARATIONS store. The
//! entrypoints table had NO serving surface that renders it — every emitted
//! next-command (`surfaces list`) exited without rendering the hit's fact, dead-ending
//! the reader. `surface_entrypoints` is excluded from `find`'s corpus entirely (eligible
//! only if a renderer ever ships). The declarations store, by contrast, IS rendered:
//! `rmap violations` reads active `boundary`-kind rows and `rmap gate` reads
//! `requirement`/`quality_policy`-kind rows (verified: violations.rs
//! `get_active_boundary_declarations`; gate_impl.rs `get_active_requirement_declarations`
//! + `get_active_quality_policy_declarations`).
//!
//! Abstraction record — module: `find_facts_reads` (a `StorageConnection` impl
//! block, not a new type); concrete current user: `daemon-runtime`'s
//! `find_facts::gather_facts` (the `find` FACTS tier); axis: the file-size
//! guardrail — the new lexical-read responsibility gets its own file rather than
//! growing the 2.7k-line `queries.rs`; rejected simpler alternative: appending to
//! `queries.rs` (breaches the "do not append to files over 500 lines" guardrail).
//! Read-only: every method is a bounded `SELECT ... LIMIT ?`; no writes, no schema.
//!
//! Honesty (STANDING RULE): every method returns `Result<_, StorageError>`. A read
//! FAILURE propagates as `Err` so the caller renders that fact class as
//! `unavailable (<reason>)` — never a silent empty that a reader would misread as
//! "measured, none".

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// One matched SYMBOL node (fact class `symbol`, rendered by `explain`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactSymbolRow {
    pub stable_key: String,
    pub name: String,
    pub qualified_name: Option<String>,
    /// Owning file path (via the `files` join); `None` when the symbol has no
    /// resolvable file association — an explicit unknown, never fabricated.
    pub path: Option<String>,
    /// FIND-RANK-1 (§2.1a): the STORED `is_test` FACT of the symbol's DEFINING FILE
    /// (`files.is_test`, strict `== 1` per `TrackedFile` parity) — the ranking input
    /// that puts production symbols before test noise. `None` when the LEFT JOIN found
    /// no `files` row (the SAME unknown that leaves `path` `None`): an UNKNOWN test
    /// status, NEVER a fabricated "not a test". The comparator ranks unknown in the
    /// NON-TEST partition (§2.4 — never demoted on unknown). This is the `is_test`
    /// FACT, never a path-string classification (STANDING HONESTY RULE 2).
    pub is_test: Option<bool>,
    /// FIND-RANK-1 (§2.1b): the symbol's stored KIND (`nodes.subtype`, e.g. `CLASS`,
    /// `FUNCTION`, `VARIABLE`) — the kind-weight ranking input. `None` when the producer
    /// emitted no subtype; the comparator ranks unknown-kind Prominent (never demoted on
    /// unknown). Raw producer string; the daemon owns the kind→weight map.
    pub subtype: Option<String>,
    /// FIND-EVIDENCE-1 (§2.1): the symbol's stored start line (`nodes.line_start`), the
    /// `path:line` anchor an agent opens directly. `None` when the span is ABSENT in the
    /// DB (the row's `line_start` is NULL) — rendered as NO line (visibly absent), NEVER
    /// a fabricated 0/1/guess (STANDING HONESTY RULE 1). Raw stored value; no coercion.
    pub line: Option<i64>,
    /// FIND-EVIDENCE-1 (§2.2): the symbol's stored doc-comment (`nodes.doc_comment`) — the
    /// evidence line's FIRST choice. `None` when unstored. The stored fact verbatim; the
    /// daemon derives the single evidence line (first non-empty line) from it — no file
    /// I/O, no invented preview (the zg arbitrary-line defect is the anti-pattern).
    pub doc_comment: Option<String>,
    /// FIND-EVIDENCE-1 (§2.2): the symbol's stored signature (`nodes.signature`) — the
    /// evidence line's FALLBACK when no doc-comment is stored. `None` when unstored.
    pub signature: Option<String>,
}

/// One matched file path (fact class `file`, rendered by `explain`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactFileRow {
    pub path: String,
}

/// One matched declared module candidate (fact class `module`, rendered by `map`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactModuleRow {
    /// Declared display name (`@scope/pkg`, `Django`) when the detector recorded
    /// one; `None` for a directory/inferred module — rendered by path instead.
    pub display_name: Option<String>,
    pub canonical_root_path: String,
}

/// One matched governance declaration (fact class `boundary`). `kind` selects the
/// rendering command at the daemon: `boundary` → `rmap violations`,
/// `requirement`/`quality_policy` → `rmap gate` (FIND-FACTS-1 review-6). Raw row: the
/// daemon owns the kind→renderer map, not storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactDeclarationRow {
    /// The declaration kind as stored (`boundary` | `requirement` | `quality_policy`);
    /// the SQL below restricts the read to exactly this renderable set.
    pub kind: String,
    /// The declaration's stored target identity (`{repo}:{module}:MODULE` for a
    /// boundary, `{repo}:requirement:{id}:{ver}` for a requirement, …) — the searchable
    /// fact column the lexical match runs over.
    pub target_stable_key: String,
}

/// Escape the SQL `LIKE` metacharacters (`%`, `_`, and the escape char itself) in a
/// user needle so a query for `a_b` matches the literal `a_b`, not `axb`. Paired
/// with `ESCAPE '\'` in every statement below. The result is ALSO lowercased so it
/// compares against `LOWER(col)` (case-insensitive substring, spec §2.1).
fn like_needle(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');
    for ch in query.to_lowercase().chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

/// Clamp a caller-supplied fetch limit to an `i64` SQLite can bind, never panicking
/// on `usize::MAX` (the `--full`/`--exact` sentinel).
fn bind_limit(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

impl StorageConnection {
    /// Fact class `symbol` (§2.1): SYMBOL nodes whose `name` OR `qualified_name`
    /// contains `query` (case-insensitive), carrying the defining file's `is_test`
    /// FACT and the symbol `subtype` for the FIND-RANK-1 rank.
    ///
    /// FIND-RANK-1 ordering (§2.1, review-0): the SQL `ORDER BY` puts NON-TEST symbols
    /// first (`is_test = 1` last; a NULL from the LEFT JOIN falls to the ELSE = non-test
    /// partition, matching §2.4), then name/qualified_name/stable_key ASC. This is ONLY a
    /// deterministic pre-order — it does NOT reproduce the ratified display precedence
    /// (which weights KIND above name), so it must NEVER be trusted to "contain the
    /// winners" under a truncating `LIMIT`. It does not have to: the sole caller
    /// (`find_facts::queries::symbols`) fetches the COMPLETE matching set (`limit =
    /// usize::MAX` in every mode) and re-ranks it with the pure Rust comparator
    /// `find_facts::rank` (kind weight, match quality, qualified-name length, path) — the
    /// unit-tested contract (§4) and single source of truth. A bounded window ordered by
    /// (is_test, name) here would EXCLUDE a globally top-ranked symbol whose name sorts
    /// late (review-0 blocking defect), so no window is applied. `limit` is retained as a
    /// parameter (all callers pass `usize::MAX`) for signature parity with the other
    /// fact reads; the `ORDER BY` keeps the raw row stream stable for diagnostics.
    pub fn find_fact_symbols(
        &self,
        snapshot_uid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactSymbolRow>, StorageError> {
        let needle = like_needle(query);
        let mut stmt = self.connection().prepare(
            "SELECT n.stable_key, n.name, n.qualified_name, f.path, f.is_test, n.subtype,
                    n.line_start, n.doc_comment, n.signature
             FROM nodes n
             LEFT JOIN files f ON n.file_uid = f.file_uid
             WHERE n.snapshot_uid = ?1
               AND n.kind = 'SYMBOL'
               AND ( LOWER(n.name) LIKE ?2 ESCAPE '\\'
                  OR LOWER(COALESCE(n.qualified_name, '')) LIKE ?2 ESCAPE '\\' )
             ORDER BY CASE WHEN f.is_test = 1 THEN 1 ELSE 0 END ASC,
                      n.name ASC, COALESCE(n.qualified_name, '') ASC, n.stable_key ASC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, needle, bind_limit(limit)],
                |row| {
                    Ok(FactSymbolRow {
                        stable_key: row.get(0)?,
                        name: row.get(1)?,
                        qualified_name: row.get(2)?,
                        path: row.get(3)?,
                        // Strict `== 1` (TrackedFile parity); NULL (no files row) →
                        // `None` = UNKNOWN, never a fabricated `false`.
                        is_test: row.get::<_, Option<i64>>(4)?.map(|v| v == 1),
                        subtype: row.get(5)?,
                        // FIND-EVIDENCE-1: raw stored span + evidence facts. A NULL
                        // `line_start` reads as `None` (span absent → no anchor line),
                        // never coerced to a number (STANDING HONESTY RULE 1).
                        line: row.get(6)?,
                        doc_comment: row.get(7)?,
                        signature: row.get(8)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fact class `file` (§2.1): tracked files of the snapshot whose repo-relative
    /// path contains `query`. Scoped through `file_versions` (the snapshot's tracked
    /// universe — the same join the path/file summaries use).
    ///
    /// FIND-RANK-1 (§2.1, "files: non-test first, same basis"): NON-TEST files first
    /// (`is_test = 1` last), then path ASC. This class's whole rank is expressible in
    /// SQL (the trivial two-key order — no per-query match-quality dimension), so it
    /// stays here rather than routing through the symbol comparator. `files.is_test` is
    /// `NOT NULL DEFAULT 0` and the join is INNER (`file_versions` → `files`), so there
    /// is no unknown partition for this class.
    pub fn find_fact_files(
        &self,
        snapshot_uid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactFileRow>, StorageError> {
        let needle = like_needle(query);
        let mut stmt = self.connection().prepare(
            "SELECT f.path
             FROM file_versions fv
             JOIN files f ON f.file_uid = fv.file_uid
             WHERE fv.snapshot_uid = ?1
               AND LOWER(f.path) LIKE ?2 ESCAPE '\\'
             ORDER BY CASE WHEN f.is_test = 1 THEN 1 ELSE 0 END ASC, f.path ASC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, needle, bind_limit(limit)],
                |row| Ok(FactFileRow { path: row.get(0)? }),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fact class `module` (§2.1): declared module candidates whose `display_name`
    /// OR `canonical_root_path` contains `query`. `module_candidates` is the
    /// declared/inferred discovery surface `modules`/`map` render. Path-ASC order
    /// (then display_name) for determinism.
    pub fn find_fact_modules(
        &self,
        snapshot_uid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactModuleRow>, StorageError> {
        let needle = like_needle(query);
        let mut stmt = self.connection().prepare(
            "SELECT display_name, canonical_root_path
             FROM module_candidates
             WHERE snapshot_uid = ?1
               AND ( LOWER(COALESCE(display_name, '')) LIKE ?2 ESCAPE '\\'
                  OR LOWER(canonical_root_path) LIKE ?2 ESCAPE '\\' )
             ORDER BY canonical_root_path ASC, COALESCE(display_name, '') ASC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, needle, bind_limit(limit)],
                |row| {
                    Ok(FactModuleRow {
                        display_name: row.get(0)?,
                        canonical_root_path: row.get(1)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fact class `boundary` (§2.1, review-6 re-home): ACTIVE governance declarations
    /// whose `target_stable_key` contains `query`, restricted to the three RENDERABLE
    /// kinds — `boundary` (rendered by `rmap violations`), `requirement` and
    /// `quality_policy` (both rendered by `rmap gate`). Declarations are REPO-scoped
    /// (`snapshot_uid` is NULL on the row — they are authored inputs, not snapshot
    /// facts), so this reads by `repo_uid`, mirroring `get_active_boundary_declarations`
    /// / `get_active_requirement_declarations` which the renderers use.
    ///
    /// The match is over `target_stable_key` ALONE — the declaration's stored identity
    /// (source module / req id / policy id). `value_json` is deliberately NOT matched:
    /// substring-matching a JSON blob makes every declaration hit on its structural
    /// keys (`version`, `forbids`, `reason`), which is noise, not a fact match. Only
    /// active rows are read — exactly what `violations`/`gate` render, so a hit's
    /// next-command renders the same declaration.
    ///
    /// `kind IN (...)` matches the daemon's renderer map (`queries::boundary_declarations`);
    /// a kind outside this set (`waiver`, `quality_policy_waiver`, or a future kind) has
    /// no dedicated renderer among violations/gate and is excluded rather than emitting a
    /// dead-ending next-command (review-6 principle: a class joins the corpus only when a
    /// rendering surface exists). Deterministic order (kind, then target key).
    pub fn find_fact_declarations(
        &self,
        repo_uid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FactDeclarationRow>, StorageError> {
        let needle = like_needle(query);
        let mut stmt = self.connection().prepare(
            "SELECT kind, target_stable_key
             FROM declarations
             WHERE repo_uid = ?1
               AND is_active = 1
               AND kind IN ('boundary', 'requirement', 'quality_policy')
               AND LOWER(target_stable_key) LIKE ?2 ESCAPE '\\'
             ORDER BY kind ASC, target_stable_key ASC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![repo_uid, needle, bind_limit(limit)],
                |row| {
                    Ok(FactDeclarationRow {
                        kind: row.get(0)?,
                        target_stable_key: row.get(1)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_needle_escapes_wildcards_and_lowercases() {
        // A user substring with LIKE metacharacters matches them LITERALLY.
        assert_eq!(like_needle("a_b"), "%a\\_b%");
        assert_eq!(like_needle("100%"), "%100\\%%");
        assert_eq!(like_needle("A\\B"), "%a\\\\b%");
        // Plain substrings are just wrapped + lowercased.
        assert_eq!(like_needle("BnR"), "%bnr%");
    }

    #[test]
    fn bind_limit_clamps_usize_max() {
        assert_eq!(bind_limit(10), 10);
        assert_eq!(bind_limit(usize::MAX), i64::MAX);
    }
}
