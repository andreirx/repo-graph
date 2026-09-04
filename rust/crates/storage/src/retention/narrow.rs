//! EC-M7-BASELINE-STAMP-1: narrowing baseline marks to provenance stamps.
//!
//! # What this module is (abstraction ledger)
//!
//! - **What:** the storage primitive for the ratified D-EC-8-D baseline
//!   semantics (EC-1 §5.2 M-7): a `baseline_stamp`-classed mark retains the
//!   `snapshots` row (comparability/toolchain/epoch identity — including the
//!   M-3b persisted `resolved_call_count` aggregate, which lives ON that row)
//!   plus the FC4 measurement families and the Tier-A `declarations` rows; its
//!   graph-family rows are deleted once the snapshot leaves the serving pair.
//!   Row-retaining `baseline_user` marks are untouched (the explicit opt-in,
//!   and the back-compat class for every pre-M-7 mark).
//! - **Concrete current users:** the background retention pass
//!   (`daemon-runtime/src/retention_pass.rs`) and the maintenance lifecycle
//!   (`handlers::inventory::retention::enforce_retention_lifecycle`) call
//!   [`narrow_stamp_baselines`]; the `mark_baseline` handler and the
//!   `classify_retention` report call [`snapshot_family_cost`]
//!   (mark-time cost surfacing + per-mark retention reporting).
//! - **Named axis of variation:** none — the keep/drop split is the ratified
//!   contract, not a configuration point. New snapshot-scoped tables must be
//!   explicitly classified keep-or-drop, and new cascade-linked child tables
//!   must be explicitly listed for cost accounting; the introspection guard
//!   tests in `tests/narrow.rs` go RED until they are (enforce-by-script,
//!   CLAUDE.md).
//! - **Rejected simpler alternatives:** (a) reusing the prune cascade —
//!   impossible, the stamp's `snapshots` row must SURVIVE, and `ON DELETE
//!   CASCADE` hangs off exactly that row; (b) a dynamic introspected DELETE
//!   over every `snapshot_uid`-bearing table — rejected for the data-loss
//!   asymmetry (a future authoritative table would be silently deleted; with
//!   the explicit list + guard it goes red for a human decision instead);
//!   (c) `SELECT total_changes()` deltas for cascade accounting — rejected
//!   because FK `SET NULL` actions would count updates as "rows removed"
//!   (a name/semantics mismatch); the explicit child list counts deletions
//!   only, exactly.
//!
//! # Invariants preserved (C-8 — frozen, this module must not disturb them)
//!
//! - **Keep-set COUNT:** narrowing deletes FAMILY rows, never a `snapshots`
//!   row. The retained-mark count and `classify_repo_retention`'s keep-set
//!   semantics are untouched.
//! - **W-B window:** eligibility structurally excludes the serving pair — the
//!   latest READY snapshot and its `parent_snapshot_uid` (the delta-refresh
//!   base / copy-forward source). Publishing N+1 never narrows N.
//! - **Writer discipline:** callers hold the same write gates as the prune
//!   steps this runs beside (activity registry + DB write lock in the
//!   background pass; the handler write guard in the maintenance path). The
//!   DELETEs are ordinary WAL writes — a concurrent pinned reader keeps seeing
//!   its pre-delete snapshot exactly as it does across `prune_prunable_snapshots`.
//!
//! # Comparability contract (VISION rule 3 — never fake numbers)
//!
//! Graph-row comparability is keyed on the CLASS, not on physical row
//! presence: a `baseline_stamp` mark's graph rows are not promised from the
//! moment of marking (they may physically linger until the pass runs, but an
//! answer that flips when a background pass fires would be nondeterministic).
//! Measurement-level comparison keeps working — `measurements` rows are
//! retained and the assess path consumes baseline facts by
//! `(stable_key, kind) → value` only.

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// FC4 measurement/assessment tables RETAINED on a stamp: the families the
/// ratified stamp keeps so measurement-level baseline comparison keeps working
/// (`measurements`) and superseded verdicts stay auditable
/// (`quality_assessments` — "do not erase superseded records").
pub const STAMP_KEEP_MEASUREMENT_TABLES: &[&str] = &["measurements", "quality_assessments"];

/// Tier-A authority tables RETAINED on a stamp (and on every other class —
/// narrowing never deletes authority): `declarations` is user governance
/// input, non-reproducible, `snapshot_uid`-nullable. Reported SEPARATELY from
/// the measurement families — a declaration is not a measurement, and the
/// cost surfaces must not label it as one.
pub const STAMP_KEEP_AUTHORITY_TABLES: &[&str] = &["declarations"];

/// Snapshot-scoped tables whose rows narrowing DELETES for a stamp mark.
///
/// # Ordering is load-bearing (FK enforcement + exact accounting)
///
/// `PRAGMA foreign_keys = ON` is applied on every connection open
/// (migrations/mod.rs), and SQLite checks IMMEDIATE FK constraints at the end
/// of each STATEMENT — so a table whose rows are referenced by a NO-ACTION FK
/// from another table must be deleted AFTER that referencing table:
///
/// - `boundary_links` carries NO-ACTION FKs into `boundary_provider_facts`
///   AND `boundary_consumer_facts` (migration_008) → the links row set must
///   go first or the fact deletes abort (review-0 finding #1).
/// - `unresolved_edges` carries a NO-ACTION FK into `nodes` (migration_007)
///   → listed before `nodes`.
/// - `nodes.parent_node_uid` is a self-referencing NO-ACTION FK: safe, the
///   whole snapshot's rows go in ONE statement (checked at statement end).
/// - `generated_code_mappings` is listed BEFORE `contract_schemas` so its
///   per-snapshot rows are DIRECT-deleted and counted exactly; deleting
///   schemas first would cascade them away via `contract_elements`
///   (schemas → elements → mappings, both CASCADE), uncounted.
///
/// The FK-order introspection guard in `tests/narrow.rs` re-derives these
/// constraints from the live schema and goes RED if a future migration
/// invalidates this order.
///
/// Three snapshot-scoped CHILD tables carry no `snapshot_uid` of their own and
/// are removed via `ON DELETE CASCADE` from parents in this list; they are
/// listed in [`STAMP_NARROW_CASCADE_CHILDREN`] so their rows are counted in
/// the cost and removal figures.
pub const STAMP_NARROW_TABLES: &[&str] = &[
    // FC2a/FC2b relation rows (FK → nodes, CASCADE)
    "edges",
    // FC0 extraction stream + staging
    "extraction_edges",
    "staged_edges",
    // FC3 disposition (NO-ACTION FK → nodes; no CASCADE on snapshot delete
    // either — prune precedent)
    "unresolved_edges",
    // FC2a-agg persisted aggregate families (M-3a; the snapshot-LEVEL g1 count
    // lives on the snapshots row and therefore survives on the stamp)
    "symbol_call_degrees",
    "resolved_call_file_pairs",
    // FC1 skeleton
    "nodes",
    "file_versions",
    "file_signals",
    // Module catalog (children before parent candidates)
    "module_candidate_evidence",
    "module_file_ownership",
    "module_discovery_diagnostics",
    "module_candidates",
    // FC5 policy facts (extracted, rebuildable — Tier B)
    "status_mappings",
    "behavioral_markers",
    "return_fates",
    // FC6 derived architecture & hints (evidence/links before their subjects)
    "evidence_links",
    "artifacts",
    "inferences",
    "annotations",
    // boundary_links FIRST: NO-ACTION FKs into both fact tables below.
    "boundary_links",
    "boundary_provider_facts",
    "boundary_consumer_facts",
    "boundary_interaction_links",
    "boundary_interaction_surfaces",
    // generated_code_mappings BEFORE contract_schemas (exact direct-delete
    // accounting; see ordering doc above).
    "generated_code_mappings",
    "contract_schemas",
    // Surface hints (evidence/detail tables before project_surfaces)
    "surface_config_roots",
    "surface_entrypoints",
    "surface_env_dependencies",
    "surface_env_evidence",
    "surface_fs_mutations",
    "surface_fs_mutation_evidence",
    "project_surface_evidence",
    "project_surfaces",
    // SEED-CHUNK-1: per-snapshot seed vectors are a derived, rebuildable Layer-3
    // embedding cache (Tier B) — narrowed on a baseline stamp like the other derived
    // families; no dependents, own CASCADE on snapshot/repo delete.
    "seed_vectors",
];

/// A snapshot-scoped child table with no `snapshot_uid` column of its own,
/// removed via `ON DELETE CASCADE` when its parent (a
/// [`STAMP_NARROW_TABLES`] member) is narrowed. Listed explicitly so cost
/// measurement and removal accounting COUNT these rows (review-0 finding #2:
/// omitting them materially understated what narrowing removes). The
/// introspection guard test enforces this list stays complete as migrations
/// add tables.
#[derive(Debug, Clone, Copy)]
pub struct CascadeChild {
    /// The child table (no `snapshot_uid` column).
    pub table: &'static str,
    /// The narrow-listed parent whose deletion cascades into `table`.
    pub parent_table: &'static str,
    /// FK column on `table` referencing the parent (NOT NULL — so a child row
    /// cannot exist without its parent, which keeps
    /// [`snapshot_graph_rows_present`] complete over direct tables only).
    pub child_fk_column: &'static str,
    /// The referenced key column on the parent.
    pub parent_key_column: &'static str,
}

/// The cascade-only children of the narrow set (see [`CascadeChild`]).
pub const STAMP_NARROW_CASCADE_CHILDREN: &[CascadeChild] = &[
    CascadeChild {
        table: "boundary_channel_details",
        parent_table: "boundary_interaction_surfaces",
        child_fk_column: "surface_uid",
        parent_key_column: "surface_uid",
    },
    CascadeChild {
        table: "boundary_contracts",
        parent_table: "boundary_interaction_surfaces",
        child_fk_column: "surface_uid",
        parent_key_column: "surface_uid",
    },
    CascadeChild {
        table: "contract_elements",
        parent_table: "contract_schemas",
        child_fk_column: "schema_uid",
        parent_key_column: "schema_uid",
    },
];

/// One baseline mark narrowed to its stamp by [`narrow_stamp_baselines`].
#[derive(Debug, Clone)]
pub struct NarrowedBaseline {
    pub snapshot_uid: String,
    /// Rows deleted across all narrow tables, INCLUDING the cascade-linked
    /// child rows ([`STAMP_NARROW_CASCADE_CHILDREN`], counted inside the
    /// transaction before their parents delete).
    pub rows_deleted: i64,
}

/// Per-family row count for one snapshot (exact, `COUNT(*)`-measured).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FamilyRows {
    pub table: &'static str,
    pub rows: i64,
}

/// What one snapshot's rows cost, split into the graph families a stamp drops
/// and the two retained groups: FC4 measurement/assessment families and the
/// Tier-A `declarations` authority rows (reported separately — a declaration
/// is not a measurement). Row counts are EXACT measurements scoped to this
/// snapshot's rows; byte figures are ESTIMATES (dbstat-prorated, data + index
/// pages) and are `None` — never a fabricated number — when this SQLite build
/// lacks the `dbstat` vtab.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotFamilyCost {
    /// Graph families with rows > 0: the [`STAMP_NARROW_TABLES`] plus their
    /// [`STAMP_NARROW_CASCADE_CHILDREN`].
    pub graph_families: Vec<FamilyRows>,
    pub graph_rows_total: i64,
    pub graph_estimated_bytes: Option<u64>,
    /// FC4 measurement/assessment families ([`STAMP_KEEP_MEASUREMENT_TABLES`])
    /// with rows > 0.
    pub measurement_families: Vec<FamilyRows>,
    pub measurement_rows_total: i64,
    pub measurement_estimated_bytes: Option<u64>,
    /// Tier-A `declarations` rows attributed to this snapshot
    /// ([`STAMP_KEEP_AUTHORITY_TABLES`] — retained authority, not
    /// measurements).
    pub declaration_rows: i64,
    pub declaration_estimated_bytes: Option<u64>,
    /// Reader-frame basis for the byte figures (estimate provenance, or why
    /// they are unknown).
    pub estimate_basis: &'static str,
}

/// One family's exact counts before proration: this snapshot's rows and the
/// table's total rows (the proration denominator).
struct FamilyCount {
    table: &'static str,
    rows: i64,
    table_total: i64,
}

impl StorageConnection {
    /// Narrow every eligible `baseline_stamp` mark of `repo_uid` to its stamp:
    /// delete its [`STAMP_NARROW_TABLES`] rows (cascading into
    /// [`STAMP_NARROW_CASCADE_CHILDREN`]), keep its `snapshots` row, the
    /// [`STAMP_KEEP_MEASUREMENT_TABLES`] rows, and the
    /// [`STAMP_KEEP_AUTHORITY_TABLES`] rows.
    ///
    /// # Eligibility (the serving pair is structurally untouchable)
    ///
    /// A mark narrows only when ALL hold:
    /// - `retention_class = 'baseline_stamp'` and `status = 'ready'`;
    /// - it is NOT the latest READY snapshot (what reads serve);
    /// - it is NOT the latest READY snapshot's `parent_snapshot_uid` (the
    ///   delta-refresh base / copy-forward source — the W-B window's N).
    ///
    /// A stamp marked on the CURRENT snapshot therefore keeps its rows until
    /// at least two newer snapshots exist — narrowing never disturbs serving
    /// state or the refresh chain.
    ///
    /// # Atomicity & idempotence
    ///
    /// One transaction per snapshot (the prune pattern): a mark is either
    /// fully narrowed or untouched. Re-running deletes nothing (0-row marks
    /// are not reported), so the pass can run the narrow step on every cycle.
    ///
    /// # Accounting
    ///
    /// `rows_deleted` counts the direct per-table deletions PLUS the
    /// cascade-linked child rows, which are counted inside the same
    /// transaction immediately before their parents delete — exact, and
    /// deletions only (FK `SET NULL` updates are not rows removed).
    ///
    /// # Concurrency
    ///
    /// Caller must hold the same write discipline as the prune it runs beside
    /// (see module docs). DELETEs are WAL-safe for concurrent readers.
    pub fn narrow_stamp_baselines(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<NarrowedBaseline>, StorageError> {
        let conn = self.connection();

        // The serving pair: latest READY + its delta-base parent.
        let latest: Option<(String, Option<String>)> = match conn.query_row(
            "SELECT snapshot_uid, parent_snapshot_uid FROM snapshots \
             WHERE repo_uid = ?1 AND status = 'ready' \
             ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![repo_uid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(pair) => Some(pair),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(StorageError::Sqlite(e)),
        };
        let (latest_uid, latest_parent) = match latest {
            Some((uid, parent)) => (uid, parent),
            // No READY snapshot → nothing serves, and stamps only exist on
            // READY snapshots → nothing eligible.
            None => return Ok(Vec::new()),
        };

        let candidates: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT snapshot_uid FROM snapshots \
                 WHERE repo_uid = ?1 AND status = 'ready' \
                   AND retention_class = 'baseline_stamp' \
                   AND snapshot_uid != ?2 \
                   AND snapshot_uid != COALESCE(?3, '')",
            )?;
            let rows = stmt.query_map(
                rusqlite::params![repo_uid, latest_uid, latest_parent],
                |row| row.get(0),
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut narrowed = Vec::new();
        for snapshot_uid in candidates {
            let tx = conn.unchecked_transaction()?;
            let mut rows_deleted: i64 = 0;
            // Count the cascade-linked child rows FIRST (their parents still
            // exist here); the parent DELETEs below remove them via CASCADE,
            // where SQLite's change counter cannot see them.
            for child in STAMP_NARROW_CASCADE_CHILDREN {
                rows_deleted += tx.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {c} JOIN {p} ON {c}.{fk} = {p}.{pk} \
                         WHERE {p}.snapshot_uid = ?1",
                        c = child.table,
                        p = child.parent_table,
                        fk = child.child_fk_column,
                        pk = child.parent_key_column,
                    ),
                    rusqlite::params![snapshot_uid],
                    |row| row.get::<_, i64>(0),
                )?;
            }
            for table in STAMP_NARROW_TABLES {
                rows_deleted += tx.execute(
                    &format!("DELETE FROM {} WHERE snapshot_uid = ?1", table),
                    rusqlite::params![snapshot_uid],
                )? as i64;
            }
            tx.commit()?;
            if rows_deleted > 0 {
                narrowed.push(NarrowedBaseline {
                    snapshot_uid,
                    rows_deleted,
                });
            }
        }

        Ok(narrowed)
    }

    /// Measure one snapshot's per-family row counts (exact) and estimated
    /// bytes (dbstat-prorated, data + index pages; `None` when dbstat is
    /// unavailable), split into the graph families a stamp drops (direct
    /// tables + cascade-linked children) and the two retained groups
    /// (measurement families; declaration authority rows).
    ///
    /// This is the honest input for the mark-time cost surface and per-mark
    /// retention reporting (D-EC-8-D: "the GB cost is surfaced at mark time").
    /// Row counts are content-free `COUNT(*)` reads (the `rmap perf`
    /// precedent — operational diagnostics, not fact-class content reads),
    /// scoped to THIS snapshot's rows.
    pub fn snapshot_family_cost(
        &self,
        snapshot_uid: &str,
    ) -> Result<SnapshotFamilyCost, StorageError> {
        let conn = self.connection();

        // Per-table total bytes (data + index pages) from dbstat, when this
        // build has it. dbstat reports one row set per B-TREE — table and
        // index trees separately, keyed by the tree's own name — so the join
        // to sqlite_master rolls each index's pages up to its OWNING table
        // (m.tbl_name). Without the join, index pages would be silently
        // excluded while the basis string claimed them (review-0 finding #2).
        let table_bytes: Option<std::collections::HashMap<String, i64>> = {
            match conn.prepare(
                "SELECT m.tbl_name, SUM(d.pgsize) FROM dbstat d \
                 JOIN sqlite_master m ON m.name = d.name \
                 WHERE m.type IN ('table', 'index') \
                 GROUP BY m.tbl_name",
            ) {
                Ok(mut stmt) => {
                    let rows = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    });
                    match rows {
                        Ok(rows) => Some(rows.flatten().collect()),
                        Err(_) => None,
                    }
                }
                Err(_) => None,
            }
        };

        // Exact per-family counts: this snapshot's rows + the table total
        // (the proration denominator).
        let direct_count = |table: &'static str| -> Result<FamilyCount, StorageError> {
            let rows: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM {} WHERE snapshot_uid = ?1", table),
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )?;
            let table_total: i64 = if rows > 0 {
                conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
                    row.get(0)
                })?
            } else {
                0
            };
            Ok(FamilyCount {
                table,
                rows,
                table_total,
            })
        };
        let cascade_count = |child: &CascadeChild| -> Result<FamilyCount, StorageError> {
            let rows: i64 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {c} JOIN {p} ON {c}.{fk} = {p}.{pk} \
                     WHERE {p}.snapshot_uid = ?1",
                    c = child.table,
                    p = child.parent_table,
                    fk = child.child_fk_column,
                    pk = child.parent_key_column,
                ),
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )?;
            let table_total: i64 = if rows > 0 {
                conn.query_row(
                    &format!("SELECT COUNT(*) FROM {}", child.table),
                    [],
                    |row| row.get(0),
                )?
            } else {
                0
            };
            Ok(FamilyCount {
                table: child.table,
                rows,
                table_total,
            })
        };

        // Prorate a group's bytes from the (data+index) page totals. Bytes
        // accumulate only while every nonzero family is measurable; one
        // unmeasurable family → the whole estimate is unknown (honest).
        let summarize = |counts: Vec<FamilyCount>| -> (Vec<FamilyRows>, i64, Option<u64>) {
            let mut families = Vec::new();
            let mut total: i64 = 0;
            let mut bytes: Option<f64> = table_bytes.as_ref().map(|_| 0.0);
            for c in counts {
                if c.rows == 0 {
                    continue;
                }
                total += c.rows;
                families.push(FamilyRows {
                    table: c.table,
                    rows: c.rows,
                });
                if let (Some(acc), Some(sizes)) = (bytes.as_mut(), table_bytes.as_ref()) {
                    match sizes.get(c.table) {
                        Some(&tb) if c.table_total > 0 => {
                            *acc += tb as f64 * (c.rows as f64 / c.table_total as f64);
                        }
                        _ => bytes = None,
                    }
                }
            }
            (families, total, bytes.map(|b| b as u64))
        };

        let mut graph_counts = Vec::new();
        for table in STAMP_NARROW_TABLES {
            graph_counts.push(direct_count(table)?);
        }
        for child in STAMP_NARROW_CASCADE_CHILDREN {
            graph_counts.push(cascade_count(child)?);
        }
        let (graph_families, graph_rows_total, graph_estimated_bytes) = summarize(graph_counts);

        let mut measurement_counts = Vec::new();
        for table in STAMP_KEEP_MEASUREMENT_TABLES {
            measurement_counts.push(direct_count(table)?);
        }
        let (measurement_families, measurement_rows_total, measurement_estimated_bytes) =
            summarize(measurement_counts);

        let mut authority_counts = Vec::new();
        for table in STAMP_KEEP_AUTHORITY_TABLES {
            authority_counts.push(direct_count(table)?);
        }
        let (_, declaration_rows, declaration_estimated_bytes) = summarize(authority_counts);

        let estimate_basis = if table_bytes.is_some() {
            "estimated: per-table bytes (SQLite dbstat; table + index pages) prorated by this \
             snapshot's row share; row counts are exact and include cascade-linked child rows"
        } else {
            "sizes unknown: this SQLite build has no dbstat; row counts are exact and include \
             cascade-linked child rows"
        };

        Ok(SnapshotFamilyCost {
            graph_families,
            graph_rows_total,
            graph_estimated_bytes,
            measurement_families,
            measurement_rows_total,
            measurement_estimated_bytes,
            declaration_rows,
            declaration_estimated_bytes,
            estimate_basis,
        })
    }

    /// Whether any graph-family rows are physically present for a snapshot.
    ///
    /// Used for reader-frame reporting only (e.g. distinguishing "rows still
    /// present, re-mark with row retention to keep them" from "rows already
    /// narrowed — re-index, then mark the new snapshot"). The comparability
    /// CONTRACT is keyed on the retention class, never on this physical check
    /// (module docs).
    ///
    /// Checks the direct [`STAMP_NARROW_TABLES`] only — complete, because
    /// every [`STAMP_NARROW_CASCADE_CHILDREN`] row requires its (NOT NULL FK)
    /// parent row to exist in a direct table.
    pub fn snapshot_graph_rows_present(&self, snapshot_uid: &str) -> Result<bool, StorageError> {
        let conn = self.connection();
        for table in STAMP_NARROW_TABLES {
            let present: i64 = conn.query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM {} WHERE snapshot_uid = ?1)",
                    table
                ),
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )?;
            if present != 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
