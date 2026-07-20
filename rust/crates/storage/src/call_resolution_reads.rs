//! RESOLUTION-BREAKDOWN-CLI-1: per-language / per-module call-resolution grouping
//! reads.
//!
//! These are the GROUPING half of the breakdown surface: additive read-only
//! `StorageConnection` methods that count the SAME populations the aggregate
//! reliability view uses, grouped by `files.language` and by owning module, and
//! further partitioned by `files.is_test` (production vs test). They carry NO
//! reliability policy — the rate, band, caveats, and reader-frame wording all live
//! in the shared `repo_graph_agent::reliability` / `reliability_breakdown`
//! projection this feeds. (The exact mirror of how `query_measurement_coverage`
//! produces `LanguageFunctionCount` for the pure `measurement_coverage` verdict.)
//!
//! ## Populations (slice §2 — identical to the aggregate)
//!
//!   * resolved   = `edges` rows of `type='CALLS'` (resolved by construction);
//!   * unresolved = `unresolved_edges` rows in the four CALLS categories
//!     (the SAME filter `assemble_trust_report` applies for
//!     `calls_classification_counts`);
//!   * external   = of those, `classification='external_library_candidate'`;
//!   * unknown    = of those, `classification='unknown'`.
//!
//! The classification/category strings are obtained by serializing the typed
//! `repo_graph_classification` enums (never hand-typed literals), so this read can
//! never drift from the vocabulary the classifier writes.
//!
//! ## Grouping keys and the reconciliation invariant
//!
//!   * per language — `COALESCE(files.language, '(unknown)')` via
//!     `edge.source_node_uid → nodes → files`.
//!   * per module — the SEMANTIC module population: `module_candidates`
//!     (declared/operational/inferred modules — VISION Layer-1/2), attributed via the
//!     STORED `module_file_ownership` edge (the SAME tables the established
//!     `module_sizes` orient read consumes — RESOLUTION-BREAKDOWN-CLI-1 review-1 #2).
//!     The prior version grouped by raw leaf-directory `MODULE` nodes (`nodes
//!     kind='MODULE'` ⋈ `OWNS`) and LABELLED them "modules" — a Layer-0/1 directory
//!     topology mislabelled as the Layer-1/2 module notion the VISION says must never
//!     be collapsed (`package_groups.rs` states the distinction). On glamCRM that
//!     produced 120 leaf-directory rows instead of the 4 inferred modules
//!     (backend/frontend/serverless/tools) an agent orients by. Each file resolves to
//!     exactly ONE candidate — the most specific (longest `canonical_root_path`),
//!     mirroring the write-time longest-prefix rule in
//!     `repo-index::compose::compute_cargo_file_ownership` — so every edge lands in
//!     exactly one module scope and the parts still reconcile to the whole. A file with
//!     no candidate (unowned, or a repo with no module discovery) folds into
//!     `'(unknown)'`, never dropped (honest degradation, not a fabricated zero).
//!   * the test partition — `files.is_test` of the SOURCE file (review-0 F4). This is
//!     the deterministic persisted flag, NOT a path-name heuristic (a module path
//!     containing "test" is incidental). Each grouped row is `(scope, is_test)`.
//!
//! Every edge is attributed to exactly one `(scope, is_test)` cell (`LEFT JOIN` +
//! `COALESCE` '(unknown)'/`0` catches edges whose source has no file/module), so
//! `Σ by_language == total` and `Σ by_module == total` across BOTH partitions by
//! construction — the parts-reconcile-to-whole invariant (slice §4), tested below.
//!
//! ## Present-but-callless scopes are SEEDED, never dropped (review-0 F2)
//!
//! A grouping query that starts FROM `edges`/`unresolved_edges` can only surface
//! scopes that HAVE call rows. A language or module present in the snapshot's symbol
//! inventory but with zero calls would silently vanish — a coverage lie (VISION:
//! "coverage is part of the fact"; a present scope with no measured calls is UNKNOWN,
//! not absent). So each grouped read first SEEDS its scope list from the snapshot's
//! function/method symbol inventory (`scope_inventory_*`, the SAME `SYMBOL`
//! FUNCTION/METHOD population `query_measurement_coverage` enumerates), then merges
//! the counts on top. A seeded scope with no counts stays all-zero → the projection
//! renders it UNKNOWN (never a fabricated 0%/100%). Seeds carry zero counts, so they
//! never perturb the reconciliation sum.

use std::collections::BTreeMap;

use repo_graph_agent::reliability_breakdown::{CallResolutionCounts, ScopeCountRow};
use repo_graph_classification::types::{UnresolvedEdgeCategory, UnresolvedEdgeClassification};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// The four CALLS-family unresolved categories — the SAME set
/// `assemble_trust_report` filters `calls_classification_counts` to. Kept as typed
/// variants (serialized at query time) so the filter tracks the enum, not a copy
/// of its string spelling.
const CALLS_CATEGORIES: [UnresolvedEdgeCategory; 4] = [
    UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
    UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
    UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
    UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
];

/// The `'(unknown)'` scope key for edges whose source has no attributable
/// language/module — an honest reconciliation bucket, never a real scope. The
/// `rgr` presentation renders it as UNKNOWN; the parenthesized form keeps it out
/// of the real-language/-module namespace.
pub const UNATTRIBUTED_SCOPE: &str = "(unknown)";

/// Serialize a typed `repo_graph_classification` enum to its snake_case SQL TEXT
/// value (the exact string the classifier persists). Mirrors the private
/// `serialize_enum` in `trust_impl.rs` — a five-line serialization glue helper
/// (NOT reliability policy), duplicated deliberately rather than widening the
/// other module's private surface for one caller.
fn enum_sql<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        _ => Err(StorageError::Sqlite(
            rusqlite::Error::ToSqlConversionFailure(
                "classification enum did not serialize to a string".into(),
            ),
        )),
    }
}

/// The serialized CALLS category strings, in `CALLS_CATEGORIES` order.
fn calls_category_sql() -> Result<[String; 4], StorageError> {
    Ok([
        enum_sql(&CALLS_CATEGORIES[0])?,
        enum_sql(&CALLS_CATEGORIES[1])?,
        enum_sql(&CALLS_CATEGORIES[2])?,
        enum_sql(&CALLS_CATEGORIES[3])?,
    ])
}

/// The `file_candidate(file_uid, scope)` CTE: each file's ONE owning module candidate
/// — the most specific (longest `canonical_root_path`) — from the stored
/// `module_file_ownership` edge (review-1 #2, semantic per-module grouping).
///
/// `snapshot_param` is the query's snapshot-uid placeholder (`"?1"` or `"?3"` — the
/// grouped-resolved/-unresolved binds put snapshot_uid in different slots), inlined
/// (NOT a value — a fixed `?N` token, never user input) so the CTE prepends cleanly to
/// each grouped query.
///
/// The single-`MAX()` + bare-column rule picks, per `file_uid`, the row with the
/// longest `canonical_root_path` (SQLite's documented min/max companion behaviour), so
/// even were a file to carry more than one ownership row (the `UNIQUE` is on the triple)
/// it still resolves to exactly ONE candidate — a strict partition, so `Σ scopes ==
/// total` holds by construction. This mirrors the write-time longest-prefix winner in
/// `repo-index::compose::compute_cargo_file_ownership`, so a call's module matches the
/// module the file is displayed under everywhere else.
fn file_candidate_cte(snapshot_param: &str) -> String {
    format!(
        "WITH file_candidate AS ( \
            SELECT o.file_uid AS file_uid, \
                   mc.canonical_root_path AS scope, \
                   MAX(LENGTH(mc.canonical_root_path)) AS pick_most_specific \
            FROM module_file_ownership o \
            JOIN module_candidates mc \
               ON mc.module_candidate_uid = o.module_candidate_uid \
              AND mc.snapshot_uid = o.snapshot_uid \
            WHERE o.snapshot_uid = {snapshot_param} \
            GROUP BY o.file_uid \
         )"
    )
}

/// A grouping cell: `(scope_key, is_test)`. `is_test` = whether the SOURCE file is a
/// test file (`files.is_test`); edges with no attributable file fall in
/// `('(unknown)', false)`.
type Cell = (String, bool);
/// Grouped resolved counts: `(cell, resolved)`.
type CellResolved = Vec<(Cell, u64)>;
/// Grouped unresolved counts: `(cell, (unresolved, external, unknown))`.
type CellUnresolved = Vec<(Cell, (u64, u64, u64))>;

/// Fold a seed cell list plus resolved-side and unresolved-side grouped results
/// into one ordered `Vec<ScopeCountRow>`. Deterministic (BTreeMap cell order). A
/// cell present in only one side gets zeros for the other; a SEEDED cell with no
/// counts stays all-zero (→ UNKNOWN, never dropped — review-0 F2).
fn merge_cells(
    seed: Vec<Cell>,
    resolved: CellResolved,
    unresolved: CellUnresolved,
) -> Vec<ScopeCountRow> {
    let zero = || CallResolutionCounts {
        resolved: 0,
        unresolved: 0,
        external: 0,
        unknown: 0,
    };
    let mut map: BTreeMap<Cell, CallResolutionCounts> = BTreeMap::new();
    // Seed present-but-callless scopes first (zero counts, so reconciliation holds).
    for cell in seed {
        map.entry(cell).or_insert_with(zero);
    }
    for (cell, resolved_count) in resolved {
        map.entry(cell).or_insert_with(zero).resolved += resolved_count;
    }
    for (cell, (unresolved, external, unknown)) in unresolved {
        let e = map.entry(cell).or_insert_with(zero);
        e.unresolved += unresolved;
        e.external += external;
        e.unknown += unknown;
    }
    map.into_iter()
        .map(|((key, is_test), counts)| ScopeCountRow {
            key,
            is_test,
            counts,
        })
        .collect()
}

impl StorageConnection {
    /// The whole-snapshot CALLS resolution counts (ungrouped, both partitions) — the
    /// "whole" the per-language / per-module parts reconcile to.
    pub fn query_call_resolution_total(
        &self,
        snapshot_uid: &str,
    ) -> Result<CallResolutionCounts, StorageError> {
        let resolved: i64 = self.connection().query_row(
            "SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?1 AND type = 'CALLS'",
            rusqlite::params![snapshot_uid],
            |row| row.get(0),
        )?;

        let cats = calls_category_sql()?;
        let ext = enum_sql(&UnresolvedEdgeClassification::ExternalLibraryCandidate)?;
        let unk = enum_sql(&UnresolvedEdgeClassification::Unknown)?;
        let (unresolved, external, unknown): (i64, i64, i64) = self.connection().query_row(
            "SELECT \
                COUNT(*), \
                SUM(CASE WHEN classification = ?1 THEN 1 ELSE 0 END), \
                SUM(CASE WHEN classification = ?2 THEN 1 ELSE 0 END) \
             FROM unresolved_edges \
             WHERE snapshot_uid = ?3 AND category IN (?4, ?5, ?6, ?7)",
            rusqlite::params![ext, unk, snapshot_uid, cats[0], cats[1], cats[2], cats[3]],
            |row| {
                // SUM over zero rows is SQL NULL → 0 (never a fabricated count).
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            },
        )?;

        Ok(CallResolutionCounts {
            resolved: resolved as u64,
            unresolved: unresolved as u64,
            external: external as u64,
            unknown: unknown as u64,
        })
    }

    /// Per-`(files.language, files.is_test)` CALLS resolution counts. Seeded from the
    /// present function/method symbol inventory so a language with symbols but no
    /// calls surfaces as UNKNOWN (F2). Edges whose source has no file/language fold
    /// into `('(unknown)', false)` so the parts reconcile to
    /// [`query_call_resolution_total`].
    pub fn query_call_resolution_by_language(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ScopeCountRow>, StorageError> {
        let seed = self.scope_inventory(
            snapshot_uid,
            &format!(
                "SELECT DISTINCT COALESCE(f.language, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test \
                 FROM nodes n \
                 JOIN files f ON n.file_uid = f.file_uid \
                 WHERE n.snapshot_uid = ?1 AND n.kind = 'SYMBOL' \
                   AND n.subtype IN ('FUNCTION', 'METHOD')"
            ),
        )?;
        let resolved = self.grouped_resolved(
            snapshot_uid,
            &format!(
                "SELECT COALESCE(f.language, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test, COUNT(*) AS cnt \
                 FROM edges e \
                 JOIN nodes n ON e.source_node_uid = n.node_uid \
                 LEFT JOIN files f ON n.file_uid = f.file_uid \
                 WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' \
                 GROUP BY scope, is_test"
            ),
        )?;
        let unresolved = self.grouped_unresolved(
            snapshot_uid,
            &format!(
                "SELECT COALESCE(f.language, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test, \
                    COUNT(*) AS unresolved, \
                    SUM(CASE WHEN ue.classification = ?1 THEN 1 ELSE 0 END) AS external, \
                    SUM(CASE WHEN ue.classification = ?2 THEN 1 ELSE 0 END) AS unknown_cnt \
                 FROM unresolved_edges ue \
                 JOIN nodes n ON ue.source_node_uid = n.node_uid \
                 LEFT JOIN files f ON n.file_uid = f.file_uid \
                 WHERE ue.snapshot_uid = ?3 AND ue.category IN (?4, ?5, ?6, ?7) \
                 GROUP BY scope, is_test"
            ),
        )?;
        Ok(merge_cells(seed, resolved, unresolved))
    }

    /// Per-`(owning module, files.is_test)` CALLS resolution counts over the SEMANTIC
    /// module population — `module_candidates` attributed via the stored
    /// `module_file_ownership` edge (review-1 #2), NOT the raw leaf-directory `MODULE`
    /// nodes the prior version mislabelled "modules". `is_test` is the SOURCE file's
    /// flag. Seeded from the present symbol inventory (F2). A call whose source file
    /// has no candidate folds into `('(unknown)', false)`.
    ///
    /// Each file resolves to exactly ONE candidate via `file_candidate_cte` (the most
    /// specific — longest `canonical_root_path`), so an edge lands in exactly one scope
    /// and `Σ by_module == total` still holds. The `WITH file_candidate` CTE is prepended
    /// to each of the three grouped queries; the snapshot param slot differs per query
    /// (`?1` for the seed/resolved binds, `?3` for the unresolved bind), so the CTE is
    /// materialised with the matching slot.
    pub fn query_call_resolution_by_module(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ScopeCountRow>, StorageError> {
        let seed = self.scope_inventory(
            snapshot_uid,
            &format!(
                "{cte} \
                 SELECT DISTINCT COALESCE(fc.scope, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test \
                 FROM nodes n \
                 JOIN files f ON n.file_uid = f.file_uid \
                 LEFT JOIN file_candidate fc ON fc.file_uid = n.file_uid \
                 WHERE n.snapshot_uid = ?1 AND n.kind = 'SYMBOL' \
                   AND n.subtype IN ('FUNCTION', 'METHOD')",
                cte = file_candidate_cte("?1"),
            ),
        )?;
        let resolved = self.grouped_resolved(
            snapshot_uid,
            &format!(
                "{cte} \
                 SELECT COALESCE(fc.scope, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test, COUNT(*) AS cnt \
                 FROM edges e \
                 JOIN nodes n ON e.source_node_uid = n.node_uid \
                 LEFT JOIN files f ON f.file_uid = n.file_uid \
                 LEFT JOIN file_candidate fc ON fc.file_uid = n.file_uid \
                 WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' \
                 GROUP BY scope, is_test",
                cte = file_candidate_cte("?1"),
            ),
        )?;
        let unresolved = self.grouped_unresolved(
            snapshot_uid,
            &format!(
                "{cte} \
                 SELECT COALESCE(fc.scope, '{UNATTRIBUTED_SCOPE}') AS scope, \
                    COALESCE(f.is_test, 0) AS is_test, \
                    COUNT(*) AS unresolved, \
                    SUM(CASE WHEN ue.classification = ?1 THEN 1 ELSE 0 END) AS external, \
                    SUM(CASE WHEN ue.classification = ?2 THEN 1 ELSE 0 END) AS unknown_cnt \
                 FROM unresolved_edges ue \
                 JOIN nodes n ON ue.source_node_uid = n.node_uid \
                 LEFT JOIN files f ON f.file_uid = n.file_uid \
                 LEFT JOIN file_candidate fc ON fc.file_uid = n.file_uid \
                 WHERE ue.snapshot_uid = ?3 AND ue.category IN (?4, ?5, ?6, ?7) \
                 GROUP BY scope, is_test",
                cte = file_candidate_cte("?3"),
            ),
        )?;
        Ok(merge_cells(seed, resolved, unresolved))
    }

    /// The DISTINCT `(scope, is_test)` cells present in the snapshot's function/method
    /// symbol inventory — the seed set (F2). `?1` = snapshot_uid.
    fn scope_inventory(&self, snapshot_uid: &str, sql: &str) -> Result<Vec<Cell>, StorageError> {
        let mut stmt = self.connection().prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Run a grouped resolved-CALLS query (`?1` = snapshot_uid). Columns:
    /// `scope, is_test, count`.
    fn grouped_resolved(
        &self,
        snapshot_uid: &str,
        sql: &str,
    ) -> Result<CellResolved, StorageError> {
        let mut stmt = self.connection().prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0),
                row.get::<_, i64>(2)? as u64,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Run a grouped unresolved-CALLS query
    /// (`?1`=external class, `?2`=unknown class, `?3`=snapshot_uid, `?4..?7`=CALLS
    /// categories). Columns: `scope, is_test, unresolved, external, unknown`.
    fn grouped_unresolved(
        &self,
        snapshot_uid: &str,
        sql: &str,
    ) -> Result<CellUnresolved, StorageError> {
        let cats = calls_category_sql()?;
        let ext = enum_sql(&UnresolvedEdgeClassification::ExternalLibraryCandidate)?;
        let unk = enum_sql(&UnresolvedEdgeClassification::Unknown)?;
        let mut stmt = self.connection().prepare(sql)?;
        let rows = stmt.query_map(
            rusqlite::params![ext, unk, snapshot_uid, cats[0], cats[1], cats[2], cats[3]],
            |row| {
                Ok((
                    (row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0),
                    (
                        row.get::<_, i64>(2)? as u64,
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0) as u64,
                        row.get::<_, Option<i64>>(4)?.unwrap_or(0) as u64,
                    ),
                ))
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}

#[cfg(test)]
#[path = "call_resolution_reads_tests.rs"]
mod tests;

// review-0 F5: reconcile the read's TOTAL to the REAL `assemble_trust_report`
// aggregate (a separate module so the test-only `repo_graph_trust` import stays
// isolated from the read code).
#[cfg(test)]
#[path = "call_resolution_aggregate_tests.rs"]
mod aggregate_tests;
