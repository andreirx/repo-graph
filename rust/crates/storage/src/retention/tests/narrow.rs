//! EC-M7-BASELINE-STAMP-1: baseline-stamp narrowing tests.
//!
//! Covers the slice §4 storage-level retention proofs:
//! - stamp default semantics (graph families deleted; stamp + measurements kept)
//! - FK-order safety: the `boundary_links` → provider/consumer-fact trio
//!   narrows successfully with real linked rows (review-1 required change #1),
//!   plus a schema-derived guard proving the WHOLE narrow order is FK-safe
//! - cost/removal accounting includes cascade-linked child rows and separates
//!   measurement families from declaration authority (review-1 #2/#3)
//! - serving-pair protection (latest READY + its delta-base parent never narrow)
//! - back-compat (row-retaining `baseline_user` marks keep every row)
//! - idempotence (a second narrow deletes nothing)
//! - the schema-introspection GUARDS: every snapshot-scoped table must be
//!   explicitly classified keep-or-drop, and every cascade-linked child of a
//!   narrow table must be listed for accounting (a future migration adding
//!   either goes RED here until a human decides)

use super::{insert_current_epoch_snapshot, insert_repo, setup_storage};
use crate::connection::StorageConnection;
use crate::retention::{
    RetentionClass, STAMP_KEEP_AUTHORITY_TABLES, STAMP_KEEP_MEASUREMENT_TABLES,
    STAMP_NARROW_CASCADE_CHILDREN, STAMP_NARROW_TABLES,
};

/// Populate a snapshot with rows across representative families:
/// graph (nodes, edges, unresolved_edges), the FK-linked boundary trio
/// (provider facts + consumer facts + a boundary_links row referencing BOTH),
/// two CASCADE-child chains (boundary_interaction_surfaces →
/// boundary_channel_details + boundary_contracts; contract_schemas →
/// contract_elements), the stamp-retained `measurements` family, and one
/// snapshot-attributed `declarations` authority row.
fn populate_snapshot(storage: &StorageConnection, snap: &str) {
    let conn = storage.connection();
    for i in 0..2 {
        conn.execute(
            "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, name) \
             VALUES (?1, ?2, 'r1', ?3, 'SYMBOL', ?4)",
            rusqlite::params![
                format!("{snap}-n{i}"),
                snap,
                format!("r1:{snap}-n{i}:SYMBOL"),
                format!("fn{i}")
            ],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, \
         type, resolution, extractor) \
         VALUES (?1, ?2, 'r1', ?3, ?4, 'CALLS', 'exact', 'test:1.0')",
        rusqlite::params![
            format!("{snap}-e0"),
            snap,
            format!("{snap}-n0"),
            format!("{snap}-n1")
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO unresolved_edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
         target_key, type, resolution, extractor, category, classification, classifier_version, \
         basis_code, observed_at) \
         VALUES (?1, ?2, 'r1', ?3, 'mystery()', 'CALLS', 'unresolved', 'test:1.0', \
                 'unknown', 'unclassified', 1, 'b0', '2025-01-01T00:00:00Z')",
        rusqlite::params![format!("{snap}-u0"), snap, format!("{snap}-n0")],
    )
    .unwrap();
    // The FK-linked boundary trio (review-1 #1): a boundary_links row carries
    // NO-ACTION FKs into BOTH fact tables — with FK enforcement ON, narrowing
    // only commits if the links row is deleted before the fact rows.
    conn.execute(
        "INSERT INTO boundary_provider_facts (fact_uid, snapshot_uid, repo_uid, mechanism, \
         operation, address, matcher_key, handler_stable_key, source_file, line_start, \
         framework, basis, extractor, observed_at) \
         VALUES (?1, ?2, 'r1', 'http', 'GET', '/api/x', 'GET /api/x', 'k-handler', \
                 'src/api.ts', 1, 'express', 'api_call', 'test:1.0', '2025-01-01T00:00:00Z')",
        rusqlite::params![format!("{snap}-bpf0"), snap],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO boundary_consumer_facts (fact_uid, snapshot_uid, repo_uid, mechanism, \
         operation, address, matcher_key, caller_stable_key, source_file, line_start, basis, \
         confidence, extractor, observed_at) \
         VALUES (?1, ?2, 'r1', 'http', 'GET', '/api/x', 'GET /api/x', 'k-caller', \
                 'src/client.ts', 5, 'api_call', 0.9, 'test:1.0', '2025-01-01T00:00:00Z')",
        rusqlite::params![format!("{snap}-bcf0"), snap],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO boundary_links (link_uid, snapshot_uid, repo_uid, provider_fact_uid, \
         consumer_fact_uid, match_basis, confidence, materialized_at) \
         VALUES (?1, ?2, 'r1', ?3, ?4, 'exact', 0.9, '2025-01-01T00:00:00Z')",
        rusqlite::params![
            format!("{snap}-bl0"),
            snap,
            format!("{snap}-bpf0"),
            format!("{snap}-bcf0")
        ],
    )
    .unwrap();
    // CASCADE-child chain 1: surface (has snapshot_uid) → channel detail +
    // boundary contract (both without snapshot_uid).
    conn.execute(
        "INSERT INTO boundary_interaction_surfaces (surface_uid, snapshot_uid, repo_uid, \
         boundary_scope, channel_kind, direction, protocol, protocol_family, interaction_pattern, \
         endpoint_locality, symbol_stable_key, source_file, line_start, line_end, col_start, \
         col_end, extractor, basis, confidence, evidence_json) \
         VALUES (?1, ?2, 'r1', 'inter_process', 'unix_socket', 'provider', 'unix', 'socket', \
                 'request_response', 'loopback', 'k', 'src/a.rs', 1, 1, 0, 0, 'test:1.0', \
                 'api_call', 0.9, '{}')",
        rusqlite::params![format!("{snap}-surf0"), snap],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO boundary_channel_details (channel_uid, surface_uid, channel_kind, \
         channel_identity) VALUES (?1, ?2, 'unix_socket', 'sock:/tmp/x')",
        rusqlite::params![format!("{snap}-chan0"), format!("{snap}-surf0")],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO boundary_contracts (association_uid, surface_uid, contract_kind, \
         association_basis, confidence) \
         VALUES (?1, ?2, 'none', 'usage_site', 0.8)",
        rusqlite::params![format!("{snap}-bc0"), format!("{snap}-surf0")],
    )
    .unwrap();
    // CASCADE-child chain 2: contract schema (has snapshot_uid) → two
    // contract elements (without snapshot_uid).
    conn.execute(
        "INSERT INTO contract_schemas (schema_uid, snapshot_uid, repo_uid, schema_kind, \
         file_path, content_hash, extractor, parsed_at) \
         VALUES (?1, ?2, 'r1', 'protobuf', 'api.proto', 'h0', 'test:1.0', \
                 '2025-01-01T00:00:00Z')",
        rusqlite::params![format!("{snap}-cs0"), snap],
    )
    .unwrap();
    for i in 0..2 {
        conn.execute(
            "INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, \
             full_name) VALUES (?1, ?2, 'message', ?3, ?4)",
            rusqlite::params![
                format!("{snap}-ce{i}"),
                format!("{snap}-cs0"),
                format!("Msg{i}"),
                format!("api.Msg{i}")
            ],
        )
        .unwrap();
    }
    // Stamp-retained FC4 family.
    for i in 0..2 {
        conn.execute(
            "INSERT INTO measurements (measurement_uid, snapshot_uid, repo_uid, \
             target_stable_key, kind, value_json, source, created_at) \
             VALUES (?1, ?2, 'r1', ?3, 'cyclomatic_complexity', '{\"value\": 3}', 'test', \
                     '2025-01-01T00:00:00Z')",
            rusqlite::params![
                format!("{snap}-m{i}"),
                snap,
                format!("r1:{snap}-n{i}:SYMBOL")
            ],
        )
        .unwrap();
    }
    // Stamp-retained Tier-A authority row, snapshot-attributed.
    conn.execute(
        "INSERT INTO declarations (declaration_uid, repo_uid, snapshot_uid, target_stable_key, \
         kind, value_json, created_at, is_active) \
         VALUES (?1, 'r1', ?2, 'r1:REPO', 'quality_policy', '{}', '2025-01-01T00:00:00Z', 1)",
        rusqlite::params![format!("{snap}-d0"), snap],
    )
    .unwrap();
}

/// Direct rows this test fixture creates per snapshot in narrow tables
/// (2 nodes + 1 edge + 1 unresolved + 3 boundary trio + 1 surface + 1 schema).
const FIXTURE_DIRECT_GRAPH_ROWS: i64 = 9;
/// Cascade-child rows the fixture creates (1 channel detail + 1 boundary
/// contract + 2 contract elements).
const FIXTURE_CASCADE_CHILD_ROWS: i64 = 4;

fn count(storage: &StorageConnection, table: &str, snap: &str) -> i64 {
    storage
        .connection()
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE snapshot_uid = ?1"),
            rusqlite::params![snap],
            |row| row.get(0),
        )
        .unwrap()
}

/// Rows in a snapshot-less child table still attached to a given parent row.
fn count_by(storage: &StorageConnection, table: &str, key_col: &str, key: &str) -> i64 {
    storage
        .connection()
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {key_col} = ?1"),
            rusqlite::params![key],
            |row| row.get(0),
        )
        .unwrap()
}

/// Three-snapshot chain s1←s2←s3 with s1 populated + marked, so s3 is the
/// latest READY (serving) and s2 its delta-base parent — s1 is eligible.
fn chain_with_marked_s1(class: RetentionClass) -> StorageConnection {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z");
    populate_snapshot(&storage, "s1");
    storage.mark_snapshot_retention("s1", class).unwrap();
    storage.classify_repo_retention("r1").unwrap();
    storage
}

// ── The stamp default: graph families deleted, stamp + measurements kept ──

#[test]
fn narrow_deletes_graph_families_keeps_stamp_row_and_measurements() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);

    let narrowed = storage.narrow_stamp_baselines("r1").unwrap();
    assert_eq!(narrowed.len(), 1, "exactly the s1 stamp narrows");
    assert_eq!(narrowed[0].snapshot_uid, "s1");
    assert_eq!(
        narrowed[0].rows_deleted,
        FIXTURE_DIRECT_GRAPH_ROWS + FIXTURE_CASCADE_CHILD_ROWS,
        "rows_deleted counts direct AND cascade-child rows exactly (review-1 #2)"
    );

    // Graph families gone.
    assert_eq!(count(&storage, "nodes", "s1"), 0);
    assert_eq!(count(&storage, "edges", "s1"), 0);
    assert_eq!(count(&storage, "unresolved_edges", "s1"), 0);
    assert_eq!(count(&storage, "boundary_interaction_surfaces", "s1"), 0);
    assert_eq!(count(&storage, "contract_schemas", "s1"), 0);
    // The CASCADE children (no snapshot_uid of their own) followed their
    // narrowed parents.
    assert_eq!(
        count_by(
            &storage,
            "boundary_channel_details",
            "surface_uid",
            "s1-surf0"
        ),
        0,
        "cascade child rows follow their narrowed parent"
    );
    assert_eq!(
        count_by(&storage, "boundary_contracts", "surface_uid", "s1-surf0"),
        0
    );
    assert_eq!(
        count_by(&storage, "contract_elements", "schema_uid", "s1-cs0"),
        0
    );

    // The stamp survives: snapshots row + measurements + declarations.
    let snap_rows: i64 = storage
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE snapshot_uid = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(snap_rows, 1, "the stamp (snapshots row) is never deleted");
    assert_eq!(count(&storage, "measurements", "s1"), 2);
    assert_eq!(
        count(&storage, "declarations", "s1"),
        1,
        "narrowing never deletes Tier-A authority"
    );

    // The mark's class is untouched (still a preserved user mark).
    let class: String = storage
        .connection()
        .query_row(
            "SELECT retention_class FROM snapshots WHERE snapshot_uid = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(class, "baseline_stamp");
}

/// Review-1 required change #1, isolated: with FK enforcement ON and a REAL
/// `boundary_links` row referencing both fact tables, narrowing COMMITS
/// (the trio is deleted links-first). Before the order fix this aborted with
/// `FOREIGN KEY constraint failed`, leaving the stamp permanently un-narrowed.
#[test]
fn narrow_commits_with_fk_linked_boundary_trio_populated() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);

    // Preconditions: FK enforcement is ON and the trio is really linked.
    let fk_on: i64 = storage
        .connection()
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(fk_on, 1, "the connection enforces foreign keys");
    assert_eq!(count(&storage, "boundary_links", "s1"), 1);
    assert_eq!(count(&storage, "boundary_provider_facts", "s1"), 1);
    assert_eq!(count(&storage, "boundary_consumer_facts", "s1"), 1);

    let narrowed = storage.narrow_stamp_baselines("r1").unwrap();
    assert_eq!(
        narrowed.len(),
        1,
        "narrowing committed despite the FK links"
    );
    assert_eq!(count(&storage, "boundary_links", "s1"), 0);
    assert_eq!(count(&storage, "boundary_provider_facts", "s1"), 0);
    assert_eq!(count(&storage, "boundary_consumer_facts", "s1"), 0);
}

#[test]
fn narrow_is_idempotent() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);
    let first = storage.narrow_stamp_baselines("r1").unwrap();
    assert_eq!(first.len(), 1);
    let second = storage.narrow_stamp_baselines("r1").unwrap();
    assert!(
        second.is_empty(),
        "an already-narrowed stamp deletes nothing and is not re-reported"
    );
}

// ── Serving-pair protection (the W-B window + delta base, C-8 frozen) ──

#[test]
fn stamp_on_latest_ready_is_never_narrowed() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    populate_snapshot(&storage, "s1");
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineStamp)
        .unwrap();
    storage.classify_repo_retention("r1").unwrap();

    let narrowed = storage.narrow_stamp_baselines("r1").unwrap();
    assert!(narrowed.is_empty(), "the serving snapshot never narrows");
    assert_eq!(count(&storage, "nodes", "s1"), 2, "rows untouched");
}

#[test]
fn stamp_on_delta_base_parent_is_never_narrowed() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");
    populate_snapshot(&storage, "s1");
    // s1 is the latest's parent (the delta-refresh base / copy-forward source).
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineStamp)
        .unwrap();
    storage.classify_repo_retention("r1").unwrap();

    let narrowed = storage.narrow_stamp_baselines("r1").unwrap();
    assert!(narrowed.is_empty(), "the delta-base parent never narrows");
    assert_eq!(count(&storage, "nodes", "s1"), 2, "rows untouched");
}

// ── Back-compat: row-retaining `baseline_user` marks are untouched ──

#[test]
fn row_retaining_baseline_user_keeps_all_rows() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineUser);

    let narrowed = storage.narrow_stamp_baselines("r1").unwrap();
    assert!(
        narrowed.is_empty(),
        "a row-retaining mark is never narrowed (clause 7: no silent data loss on upgrade)"
    );
    assert_eq!(count(&storage, "nodes", "s1"), 2);
    assert_eq!(count(&storage, "edges", "s1"), 1);
    assert_eq!(count(&storage, "unresolved_edges", "s1"), 1);
    assert_eq!(count(&storage, "boundary_links", "s1"), 1);
    assert_eq!(count(&storage, "measurements", "s1"), 2);
}

// ── Classification interplay: the stamp is a preserved user mark ──

#[test]
fn classify_preserves_stamp_marks_and_excludes_them_from_serving_roles() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);
    // chain_with_marked_s1 already classified; s1 must still be the stamp and
    // s3/s2 must hold the serving roles.
    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.baseline_stamp, 1);
    assert_eq!(stats.current, 1);
    assert_eq!(stats.parent, 1);
    assert_eq!(stats.prunable, 0);

    // The pruner never touches it either (protected class).
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 0);
    assert!(RetentionClass::BaselineStamp.is_protected());
}

#[test]
fn stale_epoch_never_reclassifies_a_stamp_mark() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // Stale-epoch snapshot marked as a stamp: an epoch bump must not delete a
    // human's mark (the stamp's retained content is epoch-independent provenance).
    super::insert_snapshot(
        &storage,
        "s1",
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some("0.9"),
    );
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineStamp)
        .unwrap();

    let marked = storage.mark_stale_epochs_prunable("r1").unwrap();
    assert_eq!(marked, 0, "the stamp mark survives the stale-epoch sweep");

    storage.classify_repo_retention("r1").unwrap();
    let class: String = storage
        .connection()
        .query_row(
            "SELECT retention_class FROM snapshots WHERE snapshot_uid = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(class, "baseline_stamp");
}

// ── Cost measurement: exact rows (incl. cascade children), honest bytes,
//    measurement/authority split ──

#[test]
fn snapshot_family_cost_counts_cascade_children_and_splits_declarations() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);
    let cost = storage.snapshot_family_cost("s1").unwrap();

    let family = |families: &[crate::retention::FamilyRows], table: &str| -> Option<i64> {
        families.iter().find(|f| f.table == table).map(|f| f.rows)
    };

    assert_eq!(family(&cost.graph_families, "nodes"), Some(2));
    assert_eq!(family(&cost.graph_families, "edges"), Some(1));
    assert_eq!(family(&cost.graph_families, "unresolved_edges"), Some(1));
    assert_eq!(family(&cost.graph_families, "boundary_links"), Some(1));
    assert_eq!(
        family(&cost.graph_families, "boundary_interaction_surfaces"),
        Some(1)
    );
    // Cascade-linked children are COUNTED (review-1 #2), attributed via their
    // parent's snapshot.
    assert_eq!(
        family(&cost.graph_families, "boundary_channel_details"),
        Some(1)
    );
    assert_eq!(family(&cost.graph_families, "boundary_contracts"), Some(1));
    assert_eq!(family(&cost.graph_families, "contract_elements"), Some(2));
    assert_eq!(
        cost.graph_rows_total,
        FIXTURE_DIRECT_GRAPH_ROWS + FIXTURE_CASCADE_CHILD_ROWS,
        "the graph total includes cascade-child rows"
    );

    // Measurement families are measurements + assessments ONLY; declarations
    // are reported separately (review-1 #3 — a declaration is not a
    // measurement).
    assert_eq!(family(&cost.measurement_families, "measurements"), Some(2));
    assert_eq!(cost.measurement_rows_total, 2);
    assert!(
        !cost
            .measurement_families
            .iter()
            .any(|f| f.table == "declarations"),
        "declarations must not be reported as a measurement family"
    );
    assert_eq!(cost.declaration_rows, 1);

    // Bytes are estimates: either measurable (dbstat available → nonzero for
    // nonzero rows) or honestly unknown (None) — never a fabricated zero.
    match cost.graph_estimated_bytes {
        Some(b) => assert!(b > 0, "measurable estimate must be nonzero for 13 rows"),
        None => assert!(cost.estimate_basis.contains("unknown")),
    }

    // After narrowing, the same query reports the stamp's true residual state.
    storage.narrow_stamp_baselines("r1").unwrap();
    let after = storage.snapshot_family_cost("s1").unwrap();
    assert_eq!(after.graph_rows_total, 0);
    assert_eq!(after.measurement_rows_total, 2);
    assert_eq!(after.declaration_rows, 1, "authority survives the narrow");
}

/// When dbstat is available, the per-table byte map must cover index pages
/// too: a table with a substantial index (nodes has several) must report
/// MORE bytes than its data pages alone would... we cannot observe the
/// data-only figure from here, so this asserts the estimate at least exists
/// and the basis names the index inclusion — the exactness proof is the
/// sqlite_master join in the query itself (deterministic, reviewed).
#[test]
fn cost_estimate_basis_names_index_pages_or_honest_unknown() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);
    let cost = storage.snapshot_family_cost("s1").unwrap();
    match cost.graph_estimated_bytes {
        Some(_) => assert!(
            cost.estimate_basis.contains("index pages"),
            "a measured estimate must disclose its basis includes index pages: {}",
            cost.estimate_basis
        ),
        None => assert!(
            cost.estimate_basis.contains("unknown"),
            "an unmeasurable estimate must say so: {}",
            cost.estimate_basis
        ),
    }
}

#[test]
fn graph_rows_present_flips_after_narrow() {
    let storage = chain_with_marked_s1(RetentionClass::BaselineStamp);
    assert!(storage.snapshot_graph_rows_present("s1").unwrap());
    storage.narrow_stamp_baselines("r1").unwrap();
    assert!(!storage.snapshot_graph_rows_present("s1").unwrap());
}

// ── The GUARDS: schema-derived completeness + FK-order safety ──

/// Every table (type='table', non-sqlite-internal) in the live schema.
fn all_tables(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// Column names of a table.
fn table_columns(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// (referenced_table, from_column, on_delete) for every FK of `table`.
fn foreign_keys(conn: &rusqlite::Connection, table: &str) -> Vec<(String, String, String)> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .unwrap();
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(2)?, // referenced table
            row.get::<_, String>(3)?, // from column
            row.get::<_, String>(6)?, // on_delete action
        ))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

/// Enforce-by-script (CLAUDE.md): a migration adding a snapshot-scoped table
/// goes RED here until the table is explicitly classified in
/// [`STAMP_NARROW_TABLES`] (deleted on narrow), [`STAMP_KEEP_MEASUREMENT_TABLES`]
/// or [`STAMP_KEEP_AUTHORITY_TABLES`] (retained on the stamp). This is the
/// narrow-list analogue of the prune module's hand-maintained no-CASCADE list
/// — with drift made impossible.
#[test]
fn every_snapshot_scoped_table_is_classified_keep_or_drop() {
    let storage = setup_storage();
    let conn = storage.connection();
    let tables = all_tables(conn);

    let keep_tables: Vec<&str> = STAMP_KEEP_MEASUREMENT_TABLES
        .iter()
        .chain(STAMP_KEEP_AUTHORITY_TABLES)
        .copied()
        .collect();

    let mut unclassified = Vec::new();
    let mut snapshot_scoped = Vec::new();
    for table in &tables {
        if !table_columns(conn, table)
            .iter()
            .any(|c| c == "snapshot_uid")
        {
            continue;
        }
        snapshot_scoped.push(table.clone());
        let classified = table == "snapshots"
            || STAMP_NARROW_TABLES.contains(&table.as_str())
            || keep_tables.contains(&table.as_str());
        if !classified {
            unclassified.push(table.clone());
        }
    }

    assert!(
        unclassified.is_empty(),
        "snapshot-scoped table(s) not classified for baseline-stamp narrowing: {unclassified:?}. \
         Decide explicitly: add to STAMP_NARROW_TABLES (deleted when a stamp mark is narrowed), \
         STAMP_KEEP_MEASUREMENT_TABLES, or STAMP_KEEP_AUTHORITY_TABLES (retained on the stamp) in \
         storage/src/retention/narrow.rs."
    );

    // No-stale + no-overlap: every listed table exists, and no table is both.
    for listed in STAMP_NARROW_TABLES.iter().chain(keep_tables.iter()) {
        assert!(
            tables.iter().any(|t| t == listed),
            "stale narrow/keep list entry (table no longer exists): {listed}"
        );
    }
    for keep in &keep_tables {
        assert!(
            !STAMP_NARROW_TABLES.contains(keep),
            "table classified both keep and drop: {keep}"
        );
    }
    // Sanity: the guard actually saw the schema (not a vacuous pass).
    assert!(
        snapshot_scoped.len() > 30,
        "schema introspection saw {} snapshot-scoped tables — expected the full inventory",
        snapshot_scoped.len()
    );
}

/// FK-ORDER GUARD (review-1 #1, generalized): re-derive the delete-order
/// constraints from the live schema. For every non-CASCADE / non-SET-NULL FK
/// from table R into a narrow table T, R's rows can block T's DELETE — so R
/// must itself be narrow-listed BEFORE T (a self-FK is exempt: one statement,
/// checked at its conclusion). A future migration adding such an FK goes RED
/// here instead of failing at narrow time in the field.
#[test]
fn narrow_order_is_fk_safe_against_the_live_schema() {
    let storage = setup_storage();
    let conn = storage.connection();

    let position = |t: &str| STAMP_NARROW_TABLES.iter().position(|n| *n == t);
    let mut violations = Vec::new();
    for table in all_tables(conn) {
        for (referenced, from_col, on_delete) in foreign_keys(conn, &table) {
            let Some(target_pos) = position(&referenced) else {
                continue; // FK into a non-narrow table — narrowing never deletes it
            };
            if table == referenced {
                continue; // self-FK: whole-snapshot single-statement delete is safe
            }
            if on_delete == "CASCADE" || on_delete == "SET NULL" {
                continue; // FK action resolves the reference automatically
            }
            match position(&table) {
                Some(source_pos) if source_pos < target_pos => {} // deleted first — safe
                Some(_) => violations.push(format!(
                    "{table}.{from_col} → {referenced} (ON DELETE {on_delete}): \
                     '{table}' must be listed BEFORE '{referenced}' in STAMP_NARROW_TABLES"
                )),
                None => violations.push(format!(
                    "{table}.{from_col} → {referenced} (ON DELETE {on_delete}): '{table}' is not \
                     narrow-listed, so its rows would permanently block narrowing '{referenced}'"
                )),
            }
        }
    }
    assert!(
        violations.is_empty(),
        "the narrow delete order violates live-schema FK constraints:\n{}",
        violations.join("\n")
    );
}

/// CASCADE-CHILD ACCOUNTING GUARD (review-1 #2, generalized): every table
/// without a `snapshot_uid` column whose rows are removed via `ON DELETE
/// CASCADE` when narrow tables delete (transitively) must be listed in
/// [`STAMP_NARROW_CASCADE_CHILDREN`] — otherwise its rows vanish uncounted
/// and the cost surfaces understate what narrowing removes. Also validates
/// each listed child's join metadata against the live schema (the FK exists,
/// is CASCADE, and is NOT NULL — the presence-check completeness argument).
#[test]
fn cascade_children_list_is_complete_and_schema_true() {
    let storage = setup_storage();
    let conn = storage.connection();
    let tables = all_tables(conn);

    // Fixpoint over cascade reachability from the narrow set.
    let mut reachable: Vec<String> = STAMP_NARROW_TABLES.iter().map(|s| s.to_string()).collect();
    let mut missing = Vec::new();
    loop {
        let mut grew = false;
        for table in &tables {
            if reachable.contains(table) {
                continue;
            }
            let cascades_from_reachable = foreign_keys(conn, table)
                .iter()
                .any(|(referenced, _, od)| od == "CASCADE" && reachable.contains(referenced));
            if !cascades_from_reachable {
                continue;
            }
            reachable.push(table.clone());
            grew = true;
            if table_columns(conn, table)
                .iter()
                .any(|c| c == "snapshot_uid")
            {
                continue; // direct-listed or caught by the keep-or-drop guard
            }
            if !STAMP_NARROW_CASCADE_CHILDREN
                .iter()
                .any(|c| c.table == *table)
            {
                missing.push(table.clone());
            }
        }
        if !grew {
            break;
        }
    }
    assert!(
        missing.is_empty(),
        "snapshot-less table(s) cascade-deleted by narrowing but not listed in \
         STAMP_NARROW_CASCADE_CHILDREN (their rows would vanish UNCOUNTED): {missing:?}"
    );

    // Each listed child's join metadata is schema-true.
    for child in STAMP_NARROW_CASCADE_CHILDREN {
        assert!(
            STAMP_NARROW_TABLES.contains(&child.parent_table),
            "{}: parent '{}' is not a narrow table",
            child.table,
            child.parent_table
        );
        let fk = foreign_keys(conn, child.table)
            .into_iter()
            .find(|(referenced, from_col, _)| {
                referenced == child.parent_table && from_col == child.child_fk_column
            });
        let (_, _, on_delete) = fk.unwrap_or_else(|| {
            panic!(
                "{}.{} has no FK to {} in the live schema",
                child.table, child.child_fk_column, child.parent_table
            )
        });
        assert_eq!(
            on_delete, "CASCADE",
            "{}.{} FK must be ON DELETE CASCADE for the child accounting to be true",
            child.table, child.child_fk_column
        );
        // NOT NULL FK → a child row cannot exist without its parent, which is
        // what keeps snapshot_graph_rows_present complete over direct tables.
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", child.table))
            .unwrap();
        let notnull = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, i64>(3)?))
            })
            .unwrap()
            .flatten()
            .any(|(name, nn)| name == child.child_fk_column && nn == 1);
        assert!(
            notnull,
            "{}.{} must be NOT NULL (presence-check completeness)",
            child.table, child.child_fk_column
        );
    }
}
