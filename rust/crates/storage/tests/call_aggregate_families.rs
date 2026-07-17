//! EC-M3A-AGG-REHOME-1 — pre-migration fallback + read-swap identity for
//! the two persisted FC2a-agg families (g2 degrees, g3 file pairs).
//!
//! A snapshot built through public CRUD without ever calling the family
//! writers IS the pre-migration shape: migration 031 leaves both
//! `snapshots` presence markers NULL, and `create_snapshot` never writes
//! them. Against the REAL SQLite adapter this suite proves:
//!
//! 1. **Fallback:** with no persisted family and CALLS rows present,
//!    `find_dead_nodes` liveness and map's dep sketch serve the live
//!    row-derived answers — never fabricated ("everything dead" /
//!    "sketch thinned") zeros.
//! 2. **Identity while parity holds:** the same snapshot answers
//!    IDENTICALLY before and after the families are persisted with
//!    parity-true values — the read swap is invisible to the two
//!    consuming surfaces' inputs (the surface-level byte-compare rides
//!    on this).
//! 3. **The swap is real (discriminating):** with a family present, the
//!    CALLS share comes ONLY from the family — a persisted degree/pair
//!    with no backing CALLS row still answers (the post-M-6 direction:
//!    liveness must not flip false-dead, the sketch must not thin), and
//!    row-only evidence outside the family no longer answers. The 7-type
//!    FC2b membership and the IMPORTS share remain live owner-reads in
//!    both regimes.

use repo_graph_storage::crud::call_aggregates::{ResolvedCallFilePairRow, SymbolCallDegreeRow};
use repo_graph_storage::types::{CreateSnapshotInput, GraphEdge, GraphNode, Repo, TrackedFile};
use repo_graph_storage::StorageConnection;

fn node_in_file(node_uid: &str, snap: &str, name: &str, file_uid: Option<&str>) -> GraphNode {
    GraphNode {
        node_uid: node_uid.to_string(),
        snapshot_uid: snap.to_string(),
        repo_uid: "r1".to_string(),
        stable_key: format!("r1:{name}:SYMBOL:FUNCTION"),
        kind: "SYMBOL".to_string(),
        subtype: Some("FUNCTION".to_string()),
        name: name.to_string(),
        qualified_name: Some(name.to_string()),
        file_uid: file_uid.map(|s| s.to_string()),
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: Some("export".to_string()),
        doc_comment: None,
        metadata_json: None,
    }
}

fn edge(edge_uid: &str, snap: &str, source: &str, target: &str, edge_type: &str) -> GraphEdge {
    GraphEdge {
        edge_uid: edge_uid.to_string(),
        snapshot_uid: snap.to_string(),
        repo_uid: "r1".to_string(),
        source_node_uid: source.to_string(),
        target_node_uid: target.to_string(),
        edge_type: edge_type.to_string(),
        resolution: "static".to_string(),
        extractor: "test:0.0.1".to_string(),
        location: None,
        metadata_json: None,
    }
}

fn file(file_uid: &str, path: &str) -> TrackedFile {
    TrackedFile {
        file_uid: file_uid.to_string(),
        repo_uid: "r1".to_string(),
        path: path.to_string(),
        language: Some("typescript".to_string()),
        is_test: false,
        is_generated: false,
        is_excluded: false,
    }
}

/// Fixture: three files, three symbols, and the two edge families the
/// swapped reads consume.
///
/// - `n-caller` (src/a.ts) —CALLS→ `n-called` (src/b.ts): the CALLS-only
///   liveness evidence AND the only cross-file CALLS pair.
/// - `n-caller` —IMPORTS→ `n-imported` (src/c.ts): 7-type liveness
///   evidence AND the IMPORTS share of the sketch.
/// - `n-caller` itself has NO incoming edge — genuinely dead.
fn seed_pre_migration_snapshot() -> (StorageConnection, String) {
    let mut storage = StorageConnection::open_in_memory().unwrap();
    storage
        .add_repo(&Repo {
            repo_uid: "r1".to_string(),
            name: "repo".to_string(),
            root_path: "/tmp/r1".to_string(),
            default_branch: None,
            created_at: "2026-07-17T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: "r1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap()
        .snapshot_uid;

    storage
        .upsert_files(&[
            file("r1:src/a.ts", "src/a.ts"),
            file("r1:src/b.ts", "src/b.ts"),
            file("r1:src/c.ts", "src/c.ts"),
        ])
        .unwrap();
    storage
        .insert_nodes(&[
            node_in_file("n-caller", &snap, "caller", Some("r1:src/a.ts")),
            node_in_file("n-called", &snap, "called", Some("r1:src/b.ts")),
            node_in_file("n-imported", &snap, "imported", Some("r1:src/c.ts")),
        ])
        .unwrap();
    storage
        .insert_edges(&[
            edge("e-call", &snap, "n-caller", "n-called", "CALLS"),
            edge("e-import", &snap, "n-caller", "n-imported", "IMPORTS"),
        ])
        .unwrap();

    (storage, snap)
}

fn dead_names(storage: &StorageConnection, snap: &str) -> Vec<String> {
    storage
        .find_dead_nodes(snap, "r1", Some("SYMBOL"))
        .unwrap()
        .into_iter()
        .map(|d| d.symbol)
        .collect()
}

fn sketch(storage: &StorageConnection, snap: &str) -> Vec<(String, String, String)> {
    storage
        .map_resolved_dep_edges_in_path(snap, "src")
        .unwrap()
        .into_iter()
        .map(|e| (e.source_file, e.target_file, e.edge_type))
        .collect()
}

/// Persist BOTH families with values parity-equal to the live rows of
/// [`seed_pre_migration_snapshot`] (what the pipeline producer supplies
/// while every resolved CALLS result materializes).
fn persist_parity_true_families(storage: &StorageConnection, snap: &str) {
    storage
        .persist_symbol_call_degrees(
            snap,
            &[
                SymbolCallDegreeRow {
                    node_uid: "n-called".into(),
                    call_fan_in: 1,
                    call_fan_out: 0,
                },
                SymbolCallDegreeRow {
                    node_uid: "n-caller".into(),
                    call_fan_in: 0,
                    call_fan_out: 1,
                },
            ],
        )
        .unwrap();
    storage
        .persist_resolved_call_file_pairs(
            snap,
            &[ResolvedCallFilePairRow {
                source_file: "src/a.ts".into(),
                target_file: "src/b.ts".into(),
                call_edge_count: 1,
            }],
        )
        .unwrap();
}

// ── 1. Pre-migration fallback ────────────────────────────────────────

#[test]
fn pre_migration_snapshot_serves_live_liveness_never_false_dead() {
    let (storage, snap) = seed_pre_migration_snapshot();

    let dead = dead_names(&storage, &snap);
    assert_eq!(
        dead,
        vec!["caller".to_string()],
        "fallback: CALLS/IMPORTS targets live via row membership; only the \
         un-referenced caller is dead — never an 'everything dead' fabrication"
    );
}

#[test]
fn pre_migration_snapshot_serves_live_sketch_never_thinned() {
    let (storage, snap) = seed_pre_migration_snapshot();

    assert_eq!(
        sketch(&storage, &snap),
        vec![
            ("src/a.ts".into(), "src/b.ts".into(), "CALLS".into()),
            ("src/a.ts".into(), "src/c.ts".into(), "IMPORTS".into()),
        ],
        "fallback: both shares served from live rows"
    );
}

// ── 2. Identity across the swap while parity holds ───────────────────

#[test]
fn answers_identical_across_fallback_and_persisted_sources() {
    let (storage, snap) = seed_pre_migration_snapshot();

    let dead_fallback = dead_names(&storage, &snap);
    let sketch_fallback = sketch(&storage, &snap);

    persist_parity_true_families(&storage, &snap);

    assert_eq!(
        dead_names(&storage, &snap),
        dead_fallback,
        "dead-liveness must be identical across the two sources while parity holds"
    );
    assert_eq!(
        sketch(&storage, &snap),
        sketch_fallback,
        "the dep sketch must be identical across the two sources while parity holds"
    );
}

// ── 3. The swap is real (the post-M-6 direction) ─────────────────────

#[test]
fn persisted_degree_keeps_symbol_alive_without_any_calls_row() {
    let (storage, snap) = seed_pre_migration_snapshot();
    persist_parity_true_families(&storage, &snap);

    // Simulate the post-M-6 world: the CALLS row is gone, the persisted
    // family (computed from the full stream) remains.
    storage
        .delete_edges_by_uids(&["e-call".to_string()])
        .unwrap();

    let dead = dead_names(&storage, &snap);
    assert!(
        !dead.contains(&"called".to_string()),
        "a persisted fan-in > 0 must keep the symbol alive with NO CALLS row \
         present — the false-dead flip M-3a exists to prevent; dead = {dead:?}"
    );
    assert!(
        dead.contains(&"caller".to_string()),
        "the genuinely un-referenced symbol stays dead"
    );
}

#[test]
fn persisted_pair_keeps_sketch_edge_without_any_calls_row() {
    let (storage, snap) = seed_pre_migration_snapshot();
    persist_parity_true_families(&storage, &snap);

    storage
        .delete_edges_by_uids(&["e-call".to_string()])
        .unwrap();

    assert_eq!(
        sketch(&storage, &snap),
        vec![
            ("src/a.ts".into(), "src/b.ts".into(), "CALLS".into()),
            ("src/a.ts".into(), "src/c.ts".into(), "IMPORTS".into()),
        ],
        "the persisted pair must keep the CALLS share with NO CALLS row present \
         (no silent thinning); the IMPORTS share stays a live owner-read"
    );
}

#[test]
fn with_family_present_the_calls_share_comes_only_from_the_family() {
    let (storage, snap) = seed_pre_migration_snapshot();

    // Families persisted EMPTY: measured zero (marker stamped, no rows).
    storage.persist_symbol_call_degrees(&snap, &[]).unwrap();
    storage
        .persist_resolved_call_file_pairs(&snap, &[])
        .unwrap();

    // The CALLS row still exists, but with the family present the CALLS
    // share is served from the family alone — the row no longer answers.
    // (In production this state is a parity FINDING pre-M-6 and the
    // normal state post-M-6; the test pins WHICH source serves.)
    let dead = dead_names(&storage, &snap);
    assert!(
        dead.contains(&"called".to_string()),
        "measured-zero family: the CALLS row is no longer consulted; dead = {dead:?}"
    );
    assert!(
        !dead.contains(&"imported".to_string()),
        "the 7-type FC2b membership stays a live owner-read"
    );

    assert_eq!(
        sketch(&storage, &snap),
        vec![("src/a.ts".into(), "src/c.ts".into(), "IMPORTS".into())],
        "measured-zero pair family: only the live IMPORTS share renders"
    );
}

// ── Promotion coherence through the served surfaces ──────────────────
//
// (Value-level promotion assertions — exact degree/pair arithmetic,
// idempotency counts, rollback, never-seeded row counts — live in the
// crate-internal unit tests beside `apply_promotion`
// [enrichment_impl.rs] and the writers [crud/call_aggregates.rs], which
// can inspect raw rows. Here: what the two SERVED read paths observe.)

#[test]
fn promotion_adjusts_families_atomically_with_rows() {
    use enrichment::{EnrichmentStoragePort, PromotedEdge};

    let (storage, snap) = seed_pre_migration_snapshot();
    persist_parity_true_families(&storage, &snap);
    storage.persist_resolved_call_aggregate(&snap, 1).unwrap();

    // Promote a NEW resolved call src/b.ts → src/c.ts (called → imported).
    let promoted = PromotedEdge {
        edge_uid: "promoted:e1".to_string(),
        snapshot_uid: snap.clone(),
        repo_uid: "r1".to_string(),
        source_node_uid: "n-called".to_string(),
        target_node_uid: "n-imported".to_string(),
        edge_type: "CALLS",
        resolution: "enriched",
        extractor: "enrichment:0.1.0".to_string(),
        location: None,
        metadata_json: "{}".to_string(),
    };
    let inserted =
        EnrichmentStoragePort::apply_promotion(&storage, &snap, std::slice::from_ref(&promoted))
            .unwrap();
    assert_eq!(inserted, 1);

    // Post-promotion parity: liveness and sketch reflect the promoted
    // edge through the PERSISTED families (identical to what the live
    // rows would say — the parity window).
    let dead = dead_names(&storage, &snap);
    assert!(
        !dead.contains(&"imported".to_string()) && !dead.contains(&"called".to_string()),
        "promotion target gains persisted fan-in; dead = {dead:?}"
    );
    let expected_sketch: Vec<(String, String, String)> = vec![
        ("src/a.ts".into(), "src/b.ts".into(), "CALLS".into()),
        ("src/a.ts".into(), "src/c.ts".into(), "IMPORTS".into()),
        ("src/b.ts".into(), "src/c.ts".into(), "CALLS".into()),
    ];
    assert_eq!(
        sketch(&storage, &snap),
        expected_sketch,
        "the promoted cross-file call appears in the persisted sketch share"
    );

    // Idempotent re-promotion: delete + insert of the same uid nets 0 —
    // both served surfaces unchanged (a double-counted pair would not
    // change the sketch's SET, but a net-negative bug would drop it and
    // a family-seeding bug would flip liveness; the exact arithmetic is
    // pinned value-level in the unit tests).
    let inserted = EnrichmentStoragePort::apply_promotion(&storage, &snap, &[promoted]).unwrap();
    assert_eq!(inserted, 1);
    assert_eq!(
        dead_names(&storage, &snap),
        dead,
        "re-promotion leaves liveness unchanged"
    );
    assert_eq!(
        sketch(&storage, &snap),
        expected_sketch,
        "re-promotion leaves the sketch unchanged"
    );
}

#[test]
fn promotion_never_seeds_families_on_a_pre_migration_snapshot() {
    use enrichment::{EnrichmentStoragePort, PromotedEdge};

    let (storage, snap) = seed_pre_migration_snapshot();
    // NO families persisted — pre-migration shape.

    let inserted = EnrichmentStoragePort::apply_promotion(
        &storage,
        &snap,
        &[PromotedEdge {
            edge_uid: "promoted:e1".to_string(),
            snapshot_uid: snap.clone(),
            repo_uid: "r1".to_string(),
            source_node_uid: "n-called".to_string(),
            target_node_uid: "n-imported".to_string(),
            edge_type: "CALLS",
            resolution: "enriched",
            extractor: "enrichment:0.1.0".to_string(),
            location: None,
            metadata_json: "{}".to_string(),
        }],
    )
    .unwrap();
    assert_eq!(inserted, 1, "the promotion itself lands");

    // The snapshot must STILL serve through the live-derived fallback —
    // proof the families were never seeded: had promotion seeded them
    // (partial, promoted-edges-only), the family branch would take over
    // and `called` (whose only liveness evidence is the ORIGINAL e-call
    // row, absent from any seeded delta) would flip falsely dead, and
    // the sketch would drop the (a.ts → b.ts) CALLS pair.
    let dead = dead_names(&storage, &snap);
    assert!(
        !dead.contains(&"called".to_string()),
        "fallback still serving (families never seeded); dead = {dead:?}"
    );
    assert!(
        !dead.contains(&"imported".to_string()),
        "fallback liveness sees the promoted row via live membership"
    );
    assert_eq!(
        sketch(&storage, &snap),
        vec![
            ("src/a.ts".into(), "src/b.ts".into(), "CALLS".into()),
            ("src/a.ts".into(), "src/c.ts".into(), "IMPORTS".into()),
            ("src/b.ts".into(), "src/c.ts".into(), "CALLS".into()),
        ],
        "fallback sketch serves all live rows incl. the promoted one"
    );
}
