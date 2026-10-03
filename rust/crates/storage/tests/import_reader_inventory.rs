//! TEST-EDGE-SCOPE-1B (D-TESB-READERS-1 §1, D-TESB-14; review-0 F-5): every storage function
//! that reads IMPORTS rows in SQL is in the reader inventory of the slice document's §2.3 with its
//! disposition on both axes.
//!
//! SCOPE — storage SQL only. The scan covers the storage crate's production sources (every
//! `#[cfg(test)]` item removed by brace matching; comment lines ignored) and detects:
//! - a SQL string naming the IMPORTS edge type (`'IMPORTS'`) or a Rust literal `"IMPORTS"`;
//! - a function taking an edge-type parameter (`edge_type:` / `edge_types:`), through which a
//!   caller can pass IMPORTS.
//!
//! A function found outside the inventory FAILS the test (a new reader must be added to §2.3 and
//! here with its disposition, or the slice STOPs). An inventory row whose function no longer exists
//! in its file also fails, so the table cannot rot. The daemon call sites are the daemon-runtime
//! guard's scope (`every_daemon_import_read_call_site_is_in_the_inventory`); the agent,
//! module-queries, gate and trust crates read imports only through storage functions this scan
//! enumerates. A reader written in another crate is outside both scans — a limit of the guards.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// (file relative to `crates/storage/src`, function, §2.3 row and disposition).
const INVENTORY: &[(&str, &str, &str)] = &[
    (
        "import_partition_reads.rs",
        "file_imports_with_partition",
        "R1/R2/R3b: the one partitioned read — each row classified on both axes; unreadable = error",
    ),
    (
        "import_partition_reads.rs",
        "reject_unreadable_import_resolutions",
        "D-TESB-14: the vocabulary check the SQL-filtering readers run first",
    ),
    (
        "import_partition_reads.rs",
        "file_import_targets",
        "R12: explain <file> Imports — certain by default, inferred remainder; precheck; test status kept",
    ),
    (
        "trust_impl.rs",
        "compute_module_stats",
        "R2: fans over the partitioned read (default view); its SQL reads unresolved alias IMPORTS only",
    ),
    (
        "trust_impl.rs",
        "find_path_prefix_module_cycles",
        "R6: persisted graph, precheck; test imports kept (resolution-symptom rule)",
    ),
    (
        "queries.rs",
        "find_cycles_cancellable",
        "R3: persisted graph for certificates/compare/audit, precheck; unpartitioned by design",
    ),
    (
        "queries.rs",
        "compute_module_stats",
        "R5: stats directory fans, persisted graph, precheck; test imports kept and stated",
    ),
    (
        "queries.rs",
        "find_imports",
        "R7: imports <file> — each row's class checked; test status kept (per-file subject)",
    ),
    (
        "queries.rs",
        "all_imports",
        "R8: import readiness certificate — a non-static value counts unknown and turns it RED",
    ),
    (
        "queries.rs",
        "find_shortest_path",
        "R9: path — certain walk by default, --include-inferred admits inferred; precheck",
    ),
    (
        "queries.rs",
        "map_resolved_dep_edges_in_path",
        "R10: map — certain IMPORTS only; precheck",
    ),
    (
        "queries.rs",
        "map_inferred_import_counts_in_path",
        "R10: map — the per-file inferred import count; precheck",
    ),
    (
        "queries.rs",
        "find_imports_between_paths",
        "R11: boundaries/gate/violations — each row's class checked; certain judged, inferred counted",
    ),
    (
        "agent_impl.rs",
        "find_file_importers",
        "R13: Referenced-by — row-checked through the classification vocabulary",
    ),
    (
        "queries.rs",
        "find_dead_nodes",
        "R14: inert — callers ask kind SYMBOL; IMPORTS target FILE/MODULE nodes only",
    ),
    (
        "agent_impl.rs",
        "find_dead_nodes_in_path",
        "R14: inert — as find_dead_nodes",
    ),
    (
        "agent_impl.rs",
        "find_dead_nodes_in_file",
        "R14: inert — as find_dead_nodes",
    ),
    (
        "queries.rs",
        "find_direct_callers",
        "R15: caller-typed edge list; every production caller passes CALLS",
    ),
    (
        "queries.rs",
        "find_direct_callees",
        "R15: caller-typed edge list; every production caller passes CALLS",
    ),
    (
        "trust_impl.rs",
        "count_edges_by_type",
        "R16: a total of one edge type, not an import relationship",
    ),
    (
        "queries.rs",
        "map_unresolved_imports_in_path",
        "unresolved_edges only — reads no edge",
    ),
    (
        "queries.rs",
        "find_unresolved_file_imports",
        "R18: imports <file> / explain <file> unresolved rows — unresolved_edges only, every IMPORTS row of the file, category/classification/basis/candidates carried, never filtered",
    ),
    (
        "crud/module_edges_support.rs",
        "get_external_imports_for_snapshot",
        "deps list: unresolved_edges rows classified external_library_candidate (its own SQL reads no edge), plus the checked workspace import sites of checked_workspace_import_sites",
    ),
    (
        "crud/module_edges_support.rs",
        "checked_workspace_import_sites",
        "deps list/why (TS-WORKSPACE-RESOLUTION-1): TS inferred IMPORTS edges only, each carrier checked against its own edge (named error otherwise); counted as an import site of the spelled package, never as a module edge; test status kept, as for the unresolved rows beside it",
    ),
];

fn storage_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read storage src") {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The source with every `#[cfg(test)]` item removed (the attribute and the brace-matched item
/// that follows it) and every line comment dropped.
fn production_lines(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = src.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if t.starts_with("#[cfg(test)]") {
            // Skip to the item's opening brace, then to its matching close.
            let mut depth: i64 = 0;
            let mut opened = false;
            for l in lines.by_ref() {
                for c in l.chars() {
                    match c {
                        '{' => {
                            depth += 1;
                            opened = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if opened && depth <= 0 {
                    break;
                }
                if !opened && l.trim_end().ends_with(';') {
                    break;
                }
            }
            continue;
        }
        if t.starts_with("//") {
            out.push(String::new());
            continue;
        }
        out.push(line.to_string());
    }
    out
}

fn enclosing_fn(lines: &[String], at: usize) -> Option<String> {
    (0..=at).rev().find_map(|k| {
        let l = &lines[k];
        let i = l.find("fn ")?;
        let rest = &l[i + 3..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty() && (i == 0 || !l.as_bytes()[i - 1].is_ascii_alphanumeric()))
            .then_some(name)
    })
}

/// Every (file, function) of the storage production sources that reads IMPORTS in SQL.
fn detected_readers() -> BTreeSet<(String, String)> {
    let root = storage_src();
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    let mut found = BTreeSet::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let lines = production_lines(&std::fs::read_to_string(&f).expect("read source"));
        for (n, l) in lines.iter().enumerate() {
            let names_imports = l.contains("'IMPORTS'") || l.contains("\"IMPORTS\"");
            let takes_edge_type = l.contains("edge_types: &[") || l.contains("edge_type: &str");
            if names_imports || takes_edge_type {
                if let Some(name) = enclosing_fn(&lines, n) {
                    found.insert((rel.clone(), name));
                }
            }
        }
    }
    found
}

#[test]
fn every_storage_imports_sql_reader_is_in_the_inventory_with_a_disposition() {
    let inventory: BTreeSet<(String, String)> = INVENTORY
        .iter()
        .map(|(f, n, _)| (f.to_string(), n.to_string()))
        .collect();
    let detected = detected_readers();
    let unlisted: Vec<&(String, String)> = detected.difference(&inventory).collect();
    assert!(
        unlisted.is_empty(),
        "storage functions reading IMPORTS in SQL that are not in the §2.3 inventory (add each \
         with its disposition, or STOP): {unlisted:?}"
    );
    // No row rots: each listed function is still defined in its file.
    let root = storage_src();
    for (file, name, disposition) in INVENTORY {
        assert!(!disposition.is_empty());
        let src = std::fs::read_to_string(root.join(file)).expect("inventory file exists");
        assert!(
            src.contains(&format!("fn {name}(")) || src.contains(&format!("fn {name}<")),
            "inventory row {file}::{name} names a function that no longer exists"
        );
    }
}
