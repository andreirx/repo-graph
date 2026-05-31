//! CJOIN-PROVE-1 research / probe tooling (NOT production) — C/C++ AST<->SCIP
//! body-bearing-callable value-fact join reliability on leveldb. Reproducible evidence;
//! lives under rust/tools. See `docs/slices/cjoin-prove-1.md`.
//!
//! Usage: cjoin-probe <index.scip> <source_root> <repo_uid>
//!
//! Method (per the ratified denominator): for each SCIP `Method` definition in a
//! product-source document, it is BODY-BEARING + JOINED iff its `(file,range)` falls
//! within a cpp-extractor body-function span (a node carrying a cyclomatic metric).
//! Unjoined Method defs are classified by SOURCE INSPECTION at the def site (anti-
//! circular: a function with a body that simply failed to join stays in the
//! denominator; only bodiless declarations are excluded).

use repo_graph_cpp_extractor::CppExtractor;
use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_scip_ingest::{decode_index, scip_definitions};
use std::fs;

/// A body-bearing C++ function from cpp-extractor: full-span range (1-based lines,
/// 0-based cols) + cyclomatic complexity.
struct BodyFn {
    line_start: i64,
    col_start: i64,
    line_end: i64,
    col_end: i64,
    cyclomatic: u32,
}

/// Body-bearing functions = cpp-extractor nodes that carry a cyclomatic metric
/// (computed only for nodes with a body).
fn body_functions(source: &str, file_path: &str, repo_uid: &str) -> Vec<BodyFn> {
    let mut ex = CppExtractor::new();
    if ex.initialize().is_err() {
        return Vec::new();
    }
    let file_uid = format!("{repo_uid}:{file_path}");
    let result = match ex.extract(source, file_path, &file_uid, repo_uid, "cjoin-prove-1") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let metrics = result.metrics;
    result
        .nodes
        .into_iter()
        .filter_map(|n| {
            let cyc = metrics.get(&n.stable_key)?.cyclomatic_complexity;
            let loc = n.location?;
            Some(BodyFn {
                line_start: loc.line_start,
                col_start: loc.col_start,
                line_end: loc.line_end,
                col_end: loc.col_end,
                cyclomatic: cyc,
            })
        })
        .collect()
}

fn file_kind(path: &str) -> &'static str {
    if path.ends_with(".cc")
        || path.ends_with(".cpp")
        || path.ends_with(".cxx")
        || path.ends_with(".c")
    {
        "impl"
    } else if path.ends_with(".h") || path.ends_with(".hpp") || path.ends_with(".hh") {
        "header"
    } else {
        "other"
    }
}

fn contains(f: &BodyFn, line: i64, col: i64) -> bool {
    let after = line > f.line_start || (line == f.line_start && col >= f.col_start);
    let before = line < f.line_end || (line == f.line_end && col <= f.col_end);
    after && before
}

/// Leading identifier of a SCIP descriptor name (`operator=` -> `operator`,
/// `SkipList<...>` -> `SkipList`, `~DBImpl` -> "").
fn leading_ident(name: &str) -> &str {
    let end = name
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(name.len());
    &name[..end]
}

/// Classify an UNJOINED Method def by source at the SCIP def site (0-based line/col).
/// Returns: "macro" | "declaration" | "coverage" | "bug".
fn classify_unjoined(lines: &[&str], name: &str, line0: i64, col0: i64) -> &'static str {
    let li = line0 as usize;
    if li >= lines.len() {
        return "bug";
    }
    let line = lines[li];
    let ci = col0 as usize;
    // Macro/preprocessor shift: a clear identifier name that does NOT appear at the SCIP
    // range start means the preprocessor moved/expanded the range off the source token.
    let ident = leading_ident(name);
    if !ident.is_empty() {
        let here = line
            .get(ci..)
            .map(|s| s.starts_with(ident))
            .unwrap_or(false);
        if !here {
            return "macro";
        }
    }
    // Name is where SCIP says: decide body vs declaration by `{` before `;` at decl scope.
    let window: String = lines[li..(li + 16).min(lines.len())].join("\n");
    let start = ci.min(window.len());
    classify_body_or_decl(&window[start..])
}

/// `{` before `;` (after the parameter list) => has a body (coverage gap, body-bearing);
/// `;` first => bodiless declaration.
fn classify_body_or_decl(rest: &str) -> &'static str {
    let mut depth = 0i32;
    let mut seen_paren = false;
    for c in rest.chars() {
        match c {
            '(' => {
                depth += 1;
                seen_paren = true;
            }
            ')' => depth -= 1,
            '{' if depth <= 0 => return "coverage",
            ';' if depth <= 0 && seen_paren => return "declaration",
            ';' if depth <= 0 && !seen_paren => return "declaration",
            _ => {}
        }
    }
    "bug"
}

#[derive(Default)]
struct Counts {
    docs_impl: usize,
    docs_header: usize,
    total_methods: usize,
    excluded_decl: usize,
    excluded_nosrc: usize,
    joined: usize,
    joined_with_cyc: usize,
    macro_mismatch: usize,
    coverage_gap: usize,
    join_bug: usize,
    denom_impl: usize,
    denom_header: usize,
    joined_impl: usize,
    joined_header: usize,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let scip = args
        .next()
        .expect("usage: cjoin-probe <index.scip> <source_root> <repo_uid>");
    let root = args.next().expect("source_root");
    let repo_uid = args.next().unwrap_or_else(|| "leveldb".to_string());

    let index = decode_index(&fs::read(&scip).expect("read scip")).expect("decode scip");
    let mut c = Counts::default();
    let mut sample_decl: Vec<String> = Vec::new();
    let mut sample_macro: Vec<String> = Vec::new();
    let mut sample_coverage: Vec<String> = Vec::new();
    let mut sample_bug: Vec<String> = Vec::new();

    for doc in &index.documents {
        let fk = file_kind(&doc.relative_path);
        match fk {
            "impl" => c.docs_impl += 1,
            "header" => c.docs_header += 1,
            _ => {}
        }
        let src = match fs::read_to_string(format!("{root}/{}", doc.relative_path)) {
            Ok(s) => s,
            Err(_) => {
                for d in scip_definitions(doc) {
                    if !d.is_local && d.kind == "Method" {
                        c.total_methods += 1;
                        c.excluded_nosrc += 1;
                    }
                }
                continue;
            }
        };
        let lines: Vec<&str> = src.lines().collect();
        let funcs = body_functions(&src, &doc.relative_path, &repo_uid);

        for d in scip_definitions(doc) {
            if d.is_local || d.kind != "Method" {
                continue;
            }
            c.total_methods += 1;
            let line0 = d.start_line0 as i64;
            let col0 = d.start_char0 as i64;
            let scip_line_1based = line0 + 1; // SCIP 0-based -> cpp-extractor 1-based
            let in_denom = |c: &mut Counts| match fk {
                "impl" => c.denom_impl += 1,
                "header" => c.denom_header += 1,
                _ => {}
            };
            match funcs.iter().find(|f| contains(f, scip_line_1based, col0)) {
                Some(f) => {
                    c.joined += 1;
                    if f.cyclomatic >= 1 {
                        c.joined_with_cyc += 1;
                    }
                    in_denom(&mut c);
                    match fk {
                        "impl" => c.joined_impl += 1,
                        "header" => c.joined_header += 1,
                        _ => {}
                    }
                }
                None => {
                    let cause = classify_unjoined(&lines, &d.name, line0, col0);
                    let label = format!("{:<26} {} L{}", d.name, doc.relative_path, line0 + 1);
                    match cause {
                        "declaration" => {
                            c.excluded_decl += 1;
                            if sample_decl.len() < 15 {
                                sample_decl.push(label);
                            }
                        }
                        "macro" => {
                            c.macro_mismatch += 1;
                            in_denom(&mut c);
                            if sample_macro.len() < 15 {
                                sample_macro.push(label);
                            }
                        }
                        "coverage" => {
                            c.coverage_gap += 1;
                            in_denom(&mut c);
                            if sample_coverage.len() < 15 {
                                sample_coverage.push(label);
                            }
                        }
                        _ => {
                            c.join_bug += 1;
                            in_denom(&mut c);
                            if sample_bug.len() < 15 {
                                sample_bug.push(label);
                            }
                        }
                    }
                }
            }
        }
    }

    let denom = c.joined + c.macro_mismatch + c.coverage_gap + c.join_bug;
    let rate = if denom == 0 {
        0.0
    } else {
        c.joined as f64 / denom as f64 * 100.0
    };

    println!("=== CJOIN-PROVE-1: {scip} ===");
    println!(
        "valid SCIP docs: {} (impl {} / header {})",
        index.documents.len(),
        c.docs_impl,
        c.docs_header
    );
    println!("total SCIP Method defs: {}", c.total_methods);
    println!("excluded declaration_without_body: {}", c.excluded_decl);
    println!("excluded source_unavailable: {}", c.excluded_nosrc);
    println!("BODY-BEARING DENOMINATOR: {denom}");
    println!(
        "  joined (cyclomatic attaches): {}  (with cyc>=1: {})",
        c.joined, c.joined_with_cyc
    );
    println!("  macro_preprocessor_mismatch: {}", c.macro_mismatch);
    println!("  cpp_extractor_coverage_gap:  {}", c.coverage_gap);
    println!("  genuine_join_bug:            {}", c.join_bug);
    println!("JOIN RATE (joined / body-bearing denom): {rate:.1}%");
    println!(
        "header/impl split: denom impl {} / header {}; joined impl {} / header {}",
        c.denom_impl, c.denom_header, c.joined_impl, c.joined_header
    );
    let dump = |title: &str, v: &[String]| {
        println!("--- {title} (sample) ---");
        for s in v {
            println!("    {s}");
        }
    };
    dump("declaration_without_body", &sample_decl);
    dump("cpp_extractor_coverage_gap", &sample_coverage);
    dump("macro_preprocessor_mismatch", &sample_macro);
    dump("genuine_join_bug", &sample_bug);
}
