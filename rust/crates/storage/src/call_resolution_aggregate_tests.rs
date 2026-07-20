//! RESOLUTION-BREAKDOWN-CLI-1 review-0 F5 — reconcile the grouping read's TOTAL to
//! the REAL shared aggregate `check`/`trust` report.
//!
//! The per-scope rows reconcile to `query_call_resolution_total` by construction
//! (same predicates, grouped vs not — proven in `call_resolution_reads_tests`). This
//! suite closes the OTHER half review-0 F5 named: that `query_call_resolution_total`
//! itself equals the population the aggregate surfaces actually report. It drives the
//! REAL `repo_graph_trust::assemble_trust_report` (the exact function the daemon
//! `trust`/`check` handlers call) over the SAME snapshot and asserts the reliability
//! read and the aggregate agree on resolved / external / in-scope-internal-like.
//!
//! CONSISTENCY NOTE (honest scope). The aggregate's `unresolved_calls` is read from
//! the stored `extraction_diagnostics_json` breakdown, while the reliability read
//! counts `unresolved_edges` rows. At INDEX time these are populated in one loop, 1:1
//! (indexer `orchestrator.rs` increments the breakdown for each edge it persists), so
//! they agree — the pre-enrichment state this slice targets, modeled by the fixture's
//! matching diagnostics JSON. If a later enrichment pass promoted unresolved edges to
//! resolved WITHOUT recomputing the stored diagnostics, the live `unresolved_edges`
//! table (what the reliability read uses) would be the fresher source; that divergence
//! is documented, not silently reconciled.

use repo_graph_trust::assemble_trust_report;

use crate::connection::StorageConnection;

const SNAP: &str = "snap-f5";

/// A snapshot whose stored diagnostics breakdown MATCHES its `unresolved_edges` rows
/// (the consistent, index-time / pre-enrichment state): 3 resolved CALLS; 5
/// CALLS-family unresolved (1 external, 2 unknown, 2 internal-candidate).
fn consistent_fixture() -> StorageConnection {
    let storage = StorageConnection::open_in_memory().unwrap();
    storage
        .connection()
        .execute_batch(&format!(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
               VALUES ('r1', 'repo', '/tmp/r1', '2024-01-01T00:00:00Z'); \
             INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at, extraction_diagnostics_json) \
               VALUES ('{SNAP}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z', \
                 '{{\"diagnostics_version\":1,\"edges_total\":8,\"unresolved_total\":5,\"unresolved_breakdown\":{{\"calls_obj_method_needs_type_info\":2,\"calls_this_method_needs_class_context\":1,\"calls_function_ambiguous_or_missing\":2}}}}'); \
             INSERT INTO files (file_uid, repo_uid, path, language, is_test) VALUES \
               ('r1:a.ts', 'r1', 'src/a.ts', 'typescript', 0); \
             INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
               ('fa', '{SNAP}', 'r1', 'r1:a.ts#fa', 'SYMBOL', 'FUNCTION', 'fa', NULL, 'r1:a.ts'), \
               ('fb', '{SNAP}', 'r1', 'r1:a.ts#fb', 'SYMBOL', 'FUNCTION', 'fb', NULL, 'r1:a.ts'); \
             INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
               ('e1', '{SNAP}', 'r1', 'fa', 'fb', 'CALLS', 'static', 't'), \
               ('e2', '{SNAP}', 'r1', 'fa', 'fb', 'CALLS', 'static', 't'), \
               ('e3', '{SNAP}', 'r1', 'fb', 'fa', 'CALLS', 'static', 't');"
        ))
        .unwrap();

    let ue = |edge: &str, class: &str, cat: &str| {
        storage
            .connection()
            .execute(
                "INSERT INTO unresolved_edges \
                 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                  resolution, extractor, category, classification, classifier_version, \
                  basis_code, observed_at) \
                 VALUES (?, ?, 'r1', 'fa', 'tk', 'CALLS', 'unresolved', 't', ?, ?, 1, 'no_supporting_signal', '2024-01-01T00:00:00Z')",
                rusqlite::params![edge, SNAP, cat, class],
            )
            .unwrap();
    };
    ue(
        "u1",
        "external_library_candidate",
        "calls_obj_method_needs_type_info",
    );
    ue("u2", "unknown", "calls_obj_method_needs_type_info");
    ue("u3", "unknown", "calls_this_method_needs_class_context");
    ue(
        "u4",
        "internal_candidate",
        "calls_function_ambiguous_or_missing",
    );
    ue(
        "u5",
        "internal_candidate",
        "calls_function_ambiguous_or_missing",
    );

    storage
}

#[test]
fn reliability_total_reconciles_to_the_real_trust_aggregate() {
    let storage = consistent_fixture();

    // The reliability read's whole-snapshot total.
    let total = storage.query_call_resolution_total(SNAP).unwrap();

    // The REAL aggregate the daemon `trust`/`check` handlers assemble.
    let report = assemble_trust_report(&storage, "r1", SNAP, None, None).unwrap();
    let s = &report.summary;

    assert_eq!(
        total.resolved, s.resolved_calls,
        "resolved CALLS must equal the aggregate's resolved_calls"
    );
    assert_eq!(
        total.external, s.unresolved_calls_external,
        "external share must equal the aggregate's unresolved_calls_external (same source)"
    );
    assert_eq!(
        total.internal_like(),
        s.unresolved_calls_internal_like,
        "in-scope-or-unclassified unresolved must equal the aggregate's internal_like"
    );

    // The in-scope RATE the reliability surface renders is the SAME number the
    // aggregate reports — both are the FRACTION `resolved / (resolved + internal_like)`
    // (trust's `call_resolution_rate` is a 0..1 fraction, not a percent).
    let denom = (total.resolved + total.internal_like()) as f64;
    let reliability_rate = total.resolved as f64 / denom;
    assert!(
        (reliability_rate - s.call_resolution_rate).abs() < 1e-9,
        "reliability in-scope rate {reliability_rate} must match aggregate {}",
        s.call_resolution_rate
    );
}
