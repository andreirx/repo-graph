//! INGEST-CORE-1 4c — optional (B) engine regression. IGNORED by default.
//!
//! Runs only with `cargo test -- --ignored` AND the engine fixtures present. Those
//! fixtures are EXTERNAL and NOT committed (the SCIP captures live outside the repo,
//! the source is the separate FRAKTAG checkout) — see decision C+B. Default CI skips
//! this; the synthetic `harness.rs` is the portable acceptance gate. Paths are
//! env-overridable for portability.

use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
use std::collections::HashSet;
use std::fs;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
fn engine_scip() -> String {
    env_or("RMAP_ENGINE_SCIP", "/tmp/scip-spike/engine.scip")
}
fn engine_scip2() -> String {
    env_or("RMAP_ENGINE_SCIP2", "/tmp/scip-spike/engine2.scip")
}
fn engine_root() -> String {
    env_or(
        "RMAP_ENGINE_ROOT",
        "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/packages/engine",
    )
}

fn ingest(scip_path: &str) -> IngestOutcome {
    let bytes = fs::read(scip_path).unwrap_or_else(|_| panic!("read {scip_path}"));
    let index = decode_index(&bytes).expect("decode engine scip");
    ingest_partition(
        &index,
        &engine_root(),
        "fraktag",
        "fraktag-engine",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    )
}

// Exact @fraktag/engine counts (post reconciliation + bubble-up + FILE materialization).
#[test]
#[ignore = "external engine fixtures (not committed); run with --ignored where present"]
fn engine_exact_counts() {
    let o = ingest(&engine_scip());
    assert_eq!(o.node_counts.fallback, 185, "engine fallback count");
    assert_eq!(
        o.edges_report.calls, 717,
        "engine strict syntax-confirmed calls"
    );
    assert_eq!(
        o.edges_report.references, 2799,
        "engine declaration references"
    );
    assert_eq!(
        o.edges_report.file_scope_refs, 634,
        "engine file-scope references"
    );

    let keys: HashSet<&str> = o.ir.nodes.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(
        o.ir.edges
            .iter()
            .filter(|e| !keys.contains(e.src.as_str()))
            .count(),
        0,
        "no dangling edge sources"
    );
    assert_eq!(
        o.ir.edges
            .iter()
            .filter(|e| !keys.contains(e.dst.as_str()))
            .count(),
        0,
        "no dangling edge destinations"
    );
    assert!(
        o.edges_report.calls <= o.ts_call_sites,
        "strict calls <= raw call-sites"
    );
}

// Deterministic re-ingest across two independent SCIP captures of the same source.
#[test]
#[ignore = "external engine fixtures (not committed); run with --ignored where present"]
fn engine_deterministic_reingest_two_captures() {
    let keys = |o: &IngestOutcome| -> HashSet<String> {
        o.ir.nodes
            .iter()
            .map(|n| n.key.as_str().to_string())
            .collect()
    };
    let a = ingest(&engine_scip());
    let b = ingest(&engine_scip2());
    assert_eq!(
        keys(&a),
        keys(&b),
        "two SCIP captures of identical source must yield identical canonical keys"
    );
}
