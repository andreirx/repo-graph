//! Diagnostic: dump SCIP-def names vs AST-node names for files matching a filter,
//! to design the narrow constructor/getter name reconciliation precisely.
//!
//! Usage:
//!   cargo run -p repo-graph-scip-ingest --example name_probe -- \
//!     <index.scip> <partition_root> <repo_uid> <file_substr>

use repo_graph_scip_ingest::{ast_nodes_for_source, decode_index, scip_definitions};
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let scip_path = args.next().expect("scip");
    let root = args.next().expect("root");
    let repo_uid = args.next().unwrap_or_else(|| "partition".to_string());
    let filter = args.next().unwrap_or_default();

    let index = decode_index(&fs::read(&scip_path).expect("read")).expect("decode");

    for doc in &index.documents {
        if !doc.relative_path.contains(&filter) {
            continue;
        }
        let source = match fs::read_to_string(format!("{root}/{}", doc.relative_path)) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let nodes = ast_nodes_for_source(&source, &doc.relative_path, &repo_uid);
        println!("=== {} ===", doc.relative_path);
        println!("SCIP defs (non-local):");
        for d in scip_definitions(doc) {
            if d.is_local {
                continue;
            }
            println!(
                "  name={:?} kind={} enc={} line={}",
                d.name,
                d.kind,
                d.enclosing_kind,
                d.start_line0 + 1
            );
        }
        println!("AST nodes:");
        for n in &nodes {
            println!(
                "  name={:?} key={} span={}..{}",
                n.name, n.stable_key, n.line_start, n.line_end
            );
        }
    }
}
