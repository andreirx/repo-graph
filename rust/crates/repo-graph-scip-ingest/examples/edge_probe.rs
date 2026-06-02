//! Diagnostic probe: ingest one partition and print the strict semantic split +
//! structural invariants. Uses the same `ingest_partition` entrypoint the 4c harness
//! asserts against (no duplicated orchestration).
//!
//! Usage:
//!   cargo run -p repo-graph-scip-ingest --example edge_probe -- \
//!     <index.scip> <partition_root> <repo_uid>

use repo_graph_ir::{EdgeBasis, EdgeType, IdentitySource};
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use std::collections::HashSet;
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let scip_path = args
        .next()
        .expect("usage: edge_probe <index.scip> <partition_root> <repo_uid>");
    let root = args.next().expect("partition_root");
    let repo_uid = args.next().unwrap_or_else(|| "partition".to_string());

    let index = decode_index(&fs::read(&scip_path).expect("read scip")).expect("decode scip");
    let o = ingest_partition(
        &index,
        &root,
        &repo_uid,
        "partition",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );

    let node_keys: HashSet<&str> = o.ir.nodes.iter().map(|n| n.key.as_str()).collect();
    let dangling_src =
        o.ir.edges
            .iter()
            .filter(|e| !node_keys.contains(e.src.as_str()))
            .count();
    let dangling_dst =
        o.ir.edges
            .iter()
            .filter(|e| !node_keys.contains(e.dst.as_str()))
            .count();
    let fsr =
        o.ir.edges
            .iter()
            .filter(|e| e.basis == EdgeBasis::FileScopeReference)
            .count();
    let fs_calls =
        o.ir.edges
            .iter()
            .filter(|e| e.basis == EdgeBasis::FileScopeReference && e.edge_type == EdgeType::Calls)
            .count();
    let r = &o.edges_report;
    let basis_sum_ok = r.calls + r.references + r.file_scope_refs == o.ir.edges.len();

    let fb_keys: HashSet<&str> =
        o.ir.nodes
            .iter()
            .filter(|n| n.identity_source == IdentitySource::ScipSynthesizedFallback)
            .map(|n| n.key.as_str())
            .collect();
    let mut endpoints: HashSet<&str> = HashSet::new();
    for e in &o.ir.edges {
        endpoints.insert(e.src.as_str());
        endpoints.insert(e.dst.as_str());
    }
    let orphan_fallbacks = fb_keys.iter().filter(|k| !endpoints.contains(*k)).count();

    let nc = &o.node_counts;
    println!(
        "NODES: total={} matched={} (reconciled={}) fallback={} file_scope={} missing_src={}",
        o.ir.nodes.len(),
        nc.matched,
        nc.reconciled,
        nc.fallback,
        nc.file_scope,
        o.missing_source
    );
    println!("EDGES (strict-default, semantic split):");
    println!("  total_ref_occurrences      = {}", r.total_refs);
    println!("  callee_resolved(in-part)   = {}", r.callee_resolved);
    println!("  emitted_edges              = {}", o.ir.edges.len());
    println!(
        "  syntax_confirmed_calls     = {}   [SyntaxConfirmedCall]",
        r.calls
    );
    println!(
        "  declaration_references     = {}   [DerivedReference]",
        r.references
    );
    println!(
        "  file_scope_references      = {}   [FileScopeReference]",
        r.file_scope_refs
    );
    println!("  fallback_target_edges      = {}", r.fallback_target);
    println!("INVARIANTS:");
    println!("  file_scope_calls           = {fs_calls}    (MUST be 0)");
    println!(
        "  no_caller                  = {}    (MUST be 0)",
        r.no_caller
    );
    println!(
        "  fs_basis == report.fsrefs  = {}",
        fsr == r.file_scope_refs
    );
    println!("  calls+refs+fsrefs==emitted = {basis_sum_ok}");
    println!("  dangling_edge_src          = {dangling_src}    (MUST be 0)");
    println!("  dangling_edge_dst          = {dangling_dst}    (MUST be 0)");
    println!(
        "  orphan_fallback_nodes      = {orphan_fallbacks} / {}",
        nc.fallback
    );
    println!("RMAP BOUND:");
    println!(
        "  ts_extractor_call_sites    = {}   (raw; strict calls {} <= this)",
        o.ts_call_sites, r.calls
    );
}
