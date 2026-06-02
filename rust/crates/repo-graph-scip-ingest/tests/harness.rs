//! INGEST-CORE-1 4c acceptance harness (default CI, portable, off-target).
//!
//! Asserts the ten invariant groups against the committed synthetic fixture
//! (`tests/fixtures/synthetic`), using the headless `ingest_partition` use-case
//! surface — no `/tmp`, no FRAKTAG checkout, no network, no fixture generation at
//! test time. Expected counts are MEASURED from the frozen fixture and asserted as
//! semantic counts + known-example existence + invariants (never vector order).
//!
//! Exact engine counts (185/717/2799/634) are guarded separately by the ignored
//! `engine_ignored` test (B) and recorded in `docs/audits/ingest-core-1/findings.md`.

use repo_graph_ir::{EdgeBasis, EdgeType, IdentitySource};
use repo_graph_scip_ingest::{ast_facts_for_source, decode_index, ingest_partition, IngestOutcome};
use std::collections::HashSet;
use std::fs;

const REPO_UID: &str = "synthetic";

fn fixture_root() -> String {
    format!("{}/tests/fixtures/synthetic", env!("CARGO_MANIFEST_DIR"))
}

fn ingest_fixture() -> IngestOutcome {
    let root = fixture_root();
    let scip = fs::read(format!("{root}/index.scip")).expect("read fixture index.scip");
    let index = decode_index(&scip).expect("decode fixture scip");
    ingest_partition(
        &index,
        &root,
        REPO_UID,
        "synthetic",
        "scip-typescript",
        "0.4.0",
        "h",
    )
}

fn node_keys(o: &IngestOutcome) -> HashSet<&str> {
    o.ir.nodes.iter().map(|n| n.key.as_str()).collect()
}

// 1. Deterministic re-ingest: identical canonical keys + edge endpoints on re-run.
#[test]
fn group1_deterministic_reingest() {
    let a = ingest_fixture();
    let b = ingest_fixture();
    assert_eq!(node_keys(&a), node_keys(&b), "node keys must be identical");
    assert_eq!(a.ir.nodes.len(), b.ir.nodes.len(), "node count stable");
    let edges = |o: &IngestOutcome| -> HashSet<(String, String, bool)> {
        o.ir.edges
            .iter()
            .map(|e| {
                (
                    e.src.as_str().to_string(),
                    e.dst.as_str().to_string(),
                    e.edge_type == EdgeType::Calls,
                )
            })
            .collect()
    };
    assert_eq!(edges(&a), edges(&b), "edge set must be identical");
}

// 2. Identity adoption: matched defs adopt byte-equal ts-extractor keys.
#[test]
fn group2_identity_adoption_byte_equal() {
    let o = ingest_fixture();
    let root = fixture_root();
    let mut ts_keys: HashSet<String> = HashSet::new();
    for rel in ["src/shapes.ts", "src/main.ts"] {
        let src = fs::read_to_string(format!("{root}/{rel}")).unwrap();
        for n in ast_facts_for_source(&src, rel, REPO_UID).nodes {
            ts_keys.insert(n.stable_key);
        }
    }
    let adopted: Vec<&str> =
        o.ir.nodes
            .iter()
            .filter(|n| n.identity_source == IdentitySource::AstAdopted)
            .map(|n| n.key.as_str())
            .collect();
    assert!(!adopted.is_empty(), "expected adopted nodes");
    for k in &adopted {
        assert!(
            ts_keys.contains(*k),
            "adopted key is not byte-equal to a ts-extractor key: {k}"
        );
    }
    // Known example: the describe method adopts its ts-extractor key.
    assert!(
        adopted
            .iter()
            .any(|k| k.ends_with("Circle.describe:SYMBOL:METHOD")),
        "expected Circle.describe among adopted keys"
    );
}

// 3. Fallback bounded and labeled; FILE nodes labeled AstFileScope.
#[test]
fn group3_fallback_bounded_and_labeled() {
    let o = ingest_fixture();
    assert_eq!(o.node_counts.fallback, 4, "fixture fallback count");
    let fb =
        o.ir.nodes
            .iter()
            .filter(|n| n.identity_source == IdentitySource::ScipSynthesizedFallback)
            .count();
    assert_eq!(fb, 4, "fallback-labeled node count");
    let file_nodes =
        o.ir.nodes
            .iter()
            .filter(|n| n.identity_source == IdentitySource::AstFileScope)
            .count();
    assert_eq!(file_nodes, 2, "two FILE nodes");
    // The abstract-class constructor remains a labeled fallback (proven coverage gap).
    assert!(
        o.ir.nodes.iter().any(
            |n| n.identity_source == IdentitySource::ScipSynthesizedFallback
                && n.key.as_str().contains("<constructor>")
        ),
        "abstract-class <constructor> must be a labeled fallback"
    );
}

// 4. Calls strict: count, all SyntaxConfirmedCall, zero FILE-scope calls.
#[test]
fn group4_calls_strict() {
    let o = ingest_fixture();
    assert_eq!(o.edges_report.calls, 2, "fixture strict call count");
    let calls: Vec<_> =
        o.ir.edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|e| e.basis == EdgeBasis::SyntaxConfirmedCall),
        "every Calls edge must be SyntaxConfirmedCall"
    );
    assert!(
        !o.ir
            .edges
            .iter()
            .any(|e| e.edge_type == EdgeType::Calls && e.basis == EdgeBasis::FileScopeReference),
        "no FILE-scope Calls allowed"
    );
    assert!(
        o.edges_report.calls <= o.ts_call_sites,
        "strict calls must not exceed raw ts-extractor call-sites"
    );
}

// 5. References split: declaration refs + file-scope refs.
#[test]
fn group5_references_split() {
    let o = ingest_fixture();
    assert_eq!(o.edges_report.references, 6, "declaration references");
    assert_eq!(o.edges_report.file_scope_refs, 3, "file-scope references");
    let fsr =
        o.ir.edges
            .iter()
            .filter(|e| e.basis == EdgeBasis::FileScopeReference)
            .count();
    assert_eq!(fsr, 3, "FileScopeReference basis count");
    assert!(
        o.ir.edges
            .iter()
            .filter(|e| e.basis == EdgeBasis::FileScopeReference)
            .all(|e| e.edge_type == EdgeType::References),
        "every FileScopeReference must be a References edge"
    );
}

// 6. Graph closure: no dangling edge endpoints.
#[test]
fn group6_graph_closure_no_dangling() {
    let o = ingest_fixture();
    let keys = node_keys(&o);
    let dangling_src =
        o.ir.edges
            .iter()
            .filter(|e| !keys.contains(e.src.as_str()))
            .count();
    let dangling_dst =
        o.ir.edges
            .iter()
            .filter(|e| !keys.contains(e.dst.as_str()))
            .count();
    assert_eq!(dangling_src, 0, "dangling edge sources");
    assert_eq!(dangling_dst, 0, "dangling edge destinations");
}

// 7. Cross-file representation: at least one in-partition cross-file edge.
#[test]
fn group7_cross_file_edge_exists() {
    let o = ingest_fixture();
    let cross = o.ir.edges.iter().any(|e| {
        let sf =
            o.ir.node(&e.src)
                .and_then(|n| n.range.as_ref())
                .map(|r| r.file.as_str());
        let df =
            o.ir.node(&e.dst)
                .and_then(|n| n.range.as_ref())
                .map(|r| r.file.as_str());
        matches!((sf, df), (Some(a), Some(b)) if a != b)
    });
    assert!(cross, "expected at least one in-partition cross-file edge");
}

// 8. Complexity attachment to a canonical key.
#[test]
fn group8_complexity_attached() {
    let o = ingest_fixture();
    assert!(!o.complexity.is_empty(), "complexity map must be non-empty");
    let describe_key =
        o.ir.nodes
            .iter()
            .find(|n| n.key.as_str().ends_with("Circle.describe:SYMBOL:METHOD"))
            .map(|n| n.key.as_str().to_string())
            .expect("Circle.describe node");
    let c = o
        .complexity
        .get(&describe_key)
        .copied()
        .expect("describe complexity attached to canonical key");
    assert!(c >= 2, "describe cyclomatic should be >= 2, got {c}");
}

// 9. Provenance: SCIP-derived nodes/edges carry SCIP symbol; FILE nodes do not.
#[test]
fn group9_provenance() {
    let o = ingest_fixture();
    for n in &o.ir.nodes {
        match n.identity_source {
            IdentitySource::AstAdopted | IdentitySource::ScipSynthesizedFallback => assert!(
                n.provenance.scip_symbol_id.is_some(),
                "SCIP-derived node missing provenance: {}",
                n.key.as_str()
            ),
            IdentitySource::AstFileScope => assert!(
                n.provenance.scip_symbol_id.is_none(),
                "FILE node must carry no SCIP provenance: {}",
                n.key.as_str()
            ),
        }
    }
    // SCIP-derived edges (Calls/References) carry the SCIP referent symbol; AST-derived import edges
    // (EdgeBasis::AstImport, IMPORTS-MODULE-INGEST-1) carry none — like FILE nodes above.
    for e in &o.ir.edges {
        match e.basis {
            EdgeBasis::AstImport => assert!(
                e.provenance.scip_symbol_id.is_none(),
                "AST import edge must carry no SCIP provenance: {} -> {}",
                e.src.as_str(),
                e.dst.as_str()
            ),
            _ => assert!(
                e.provenance.scip_symbol_id.is_some(),
                "every SCIP-derived edge must carry its SCIP referent provenance"
            ),
        }
    }
}

// 10. Dependency boundary: repo-graph-ir has zero scip/sqlite/tree-sitter/storage deps.
#[test]
fn group10_dependency_boundary() {
    let ir_cargo = format!("{}/../repo-graph-ir/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let toml = fs::read_to_string(&ir_cargo).expect("read repo-graph-ir Cargo.toml");
    // Isolate the [dependencies] table, ignoring comments and the description string.
    let deps = toml.split("[dependencies]").nth(1).unwrap_or("");
    let deps = deps.split("\n[").next().unwrap_or(deps);
    let dep_keys: Vec<String> = deps
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split('=').next())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    assert!(
        dep_keys.is_empty(),
        "repo-graph-ir [dependencies] must be empty (pure domain); found {dep_keys:?}"
    );
    for forbidden in [
        "scip",
        "rusqlite",
        "sqlite",
        "tree-sitter",
        "tree_sitter",
        "storage",
        "repo-graph-indexer",
        "repo-graph-ts-extractor",
        "repo-graph-classification",
    ] {
        for k in &dep_keys {
            assert!(
                !k.contains(forbidden),
                "repo-graph-ir must not depend on `{k}` (forbidden: {forbidden})"
            );
        }
    }
}
