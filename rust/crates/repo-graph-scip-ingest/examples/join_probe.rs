//! Step-3: declaration-join measurement + residual diagnosis + kind-split.
//!
//! Usage:
//!   cargo run -p repo-graph-scip-ingest --example join_probe -- \
//!     <index.scip> <partition_root> <repo_uid>

use repo_graph_scip_ingest::{
    ast_nodes_for_source, decode_index, diagnose_unmatched, match_def, scip_definitions,
    symbol_kinds,
};
use std::collections::BTreeMap;
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let scip_path = args
        .next()
        .expect("usage: join_probe <index.scip> <partition_root> <repo_uid>");
    let root = args.next().expect("partition_root");
    let repo_uid = args.next().unwrap_or_else(|| "partition".to_string());

    let bytes = fs::read(&scip_path).expect("read scip file");
    let index = decode_index(&bytes).expect("decode scip index");

    let decl_kinds = ["Namespace", "Type", "Method", "Term"];
    let excluded_kinds = ["Parameter", "TypeParameter", "Meta"];

    let mut by_kind: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut causes: BTreeMap<&str, (usize, usize, String)> = BTreeMap::new();
    // SCIP SymbolInformation.kind of unmatched declarations -> (term, method)
    let mut kind_split: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut ast_nodes_total = 0usize;
    let mut docs_joined = 0usize;
    let mut docs_missing = 0usize;

    for doc in &index.documents {
        let src_path = format!("{root}/{}", doc.relative_path);
        let source = match fs::read_to_string(&src_path) {
            Ok(s) => s,
            Err(_) => {
                docs_missing += 1;
                continue;
            }
        };
        docs_joined += 1;
        let nodes = ast_nodes_for_source(&source, &doc.relative_path, &repo_uid);
        ast_nodes_total += nodes.len();
        let kinds = symbol_kinds(doc);

        for d in &scip_definitions(doc) {
            if d.is_local {
                continue;
            }
            let matched = match_def(d, &nodes);
            let e = by_kind.entry(d.kind.clone()).or_insert((0, 0));
            e.1 += 1;
            if matched {
                e.0 += 1;
            } else if d.kind == "Term" || d.kind == "Method" {
                let cause = diagnose_unmatched(d, &doc.relative_path, &nodes);
                let c = causes.entry(cause).or_insert((0, 0, String::new()));
                if d.kind == "Term" {
                    c.0 += 1;
                } else {
                    c.1 += 1;
                }
                if c.2.is_empty() {
                    c.2 = format!("{}::{}", doc.relative_path, d.name);
                }
                let sk = kinds
                    .get(&d.symbol)
                    .cloned()
                    .unwrap_or_else(|| "NotInSymbols".to_string());
                let ks = kind_split.entry(sk).or_insert((0, 0));
                if d.kind == "Term" {
                    ks.0 += 1;
                } else {
                    ks.1 += 1;
                }
            }
        }
    }

    let sum = |kinds: &[&str]| -> (usize, usize) {
        kinds.iter().fold((0, 0), |(m, t), k| {
            let (km, kt) = by_kind.get(*k).copied().unwrap_or((0, 0));
            (m + km, t + kt)
        })
    };

    let (dm, dt) = sum(&decl_kinds);
    let drate = if dt == 0 {
        0.0
    } else {
        dm as f64 / dt as f64 * 100.0
    };
    println!(
        "docs_joined={docs_joined} missing_source={docs_missing} ast_nodes_total={ast_nodes_total}"
    );
    println!("DECLARATION denominator (Namespace/Type/Method/Term): {dm}/{dt} = {drate:.1}%");
    for k in decl_kinds {
        if let Some((m, t)) = by_kind.get(k) {
            println!("  {k:<10} {m}/{t}");
        }
    }
    println!("EXCLUDED SCIP-only granularity (not failure evidence):");
    for k in excluded_kinds {
        if let Some((m, t)) = by_kind.get(k) {
            println!("  {k:<14} {m}/{t}");
        }
    }
    println!("RESIDUAL unmatched Term/Method by cause [term / method] e.g.:");
    for (cause, (tc, mc, ex)) in &causes {
        println!("  {cause:<26} {tc:>4} / {mc:<4}  e.g. {ex}");
    }
    println!("KIND-SPLIT of unmatched declarations (SCIP SymbolInformation.kind) [term / method]:");
    for (k, (tc, mc)) in &kind_split {
        println!("  {k:<22} {tc:>4} / {mc:<4}");
    }
}
