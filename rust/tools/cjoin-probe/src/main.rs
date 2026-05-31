//! CJOIN-PROVE-1/2 research / probe tooling (NOT production) — C/C++ AST<->SCIP
//! body-bearing-callable value-fact join reliability. Reproducible evidence; lives under
//! rust/tools. See `docs/slices/cjoin-prove-1.md` and `cjoin-prove-2.md`.
//!
//! Rule proven by CJOIN-PROVE-2: a C/C++ value fact may attach to a SCIP identity only
//! when BOTH range containment AND name correspondence agree; a name mismatch is a
//! REJECTED misattachment that must degrade to raw-source-anchored (range-only joining
//! silently misattaches — e.g. 15.1% on leveldb's annotation-macro classes).
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
    name: String,
    line_start: i64,
    col_start: i64,
    line_end: i64,
    col_end: i64,
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
            metrics.get(&n.stable_key)?; // body-bearing: cpp-extractor computed a cyclomatic metric
            let name = n.name;
            let loc = n.location?;
            Some(BodyFn {
                name,
                line_start: loc.line_start,
                col_start: loc.col_start,
                line_end: loc.line_end,
                col_end: loc.col_end,
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

/// Tri-state name correspondence. Only `Confirmed` may attach; `Mismatch` and
/// `Uncomparable` raw-anchor (never silently attached — that was the original flaw).
#[derive(Debug, PartialEq, Eq)]
enum NameMatch {
    Confirmed,
    Mismatch,
    Uncomparable,
}

/// Canonical comparable terminal name. Qualified `Foo::Bar`/`Foo.bar` -> `Bar`; preserves
/// destructor spelling (`~Class`) and FULL operator spelling (`operator=`, `operator()`,
/// `operator[]`) so distinct operators never collapse; returns `None` for empty /
/// unparseable names (e.g. a `*` extracted from a pointer-return signature).
fn canonical_name(name: &str) -> Option<String> {
    let seg = name.rsplit("::").next().unwrap_or(name);
    let seg = seg.rsplit('.').next().unwrap_or(seg).trim();
    if seg.is_empty() {
        return None;
    }
    if let Some(rest) = seg.strip_prefix('~') {
        let id = leading_ident(rest.trim_start());
        return if id.is_empty() {
            None
        } else {
            Some(format!("~{id}"))
        };
    }
    if let Some(rest) = seg.strip_prefix("operator") {
        // `operator` is an operator name only when followed by a symbol/space, not an
        // identifier char (so `operatorfoo` stays a plain function name).
        let is_op = rest
            .chars()
            .next()
            .map(|ch| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(false);
        if is_op {
            return normalize_operator(rest);
        }
    }
    let id = leading_ident(seg);
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Normalize the token after `operator` into a stable `operator<symbol>` form.
fn normalize_operator(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    if rest.starts_with("()") {
        return Some("operator()".to_string());
    }
    if rest.starts_with("[]") {
        return Some("operator[]".to_string());
    }
    let lead = leading_ident(rest); // conversion operator: `operator Type`
    if !lead.is_empty() {
        return Some(format!("operator {lead}"));
    }
    let sym: String = rest
        .chars()
        .take_while(|ch| !ch.is_alphanumeric() && *ch != '(' && *ch != '_' && !ch.is_whitespace())
        .collect();
    if sym.is_empty() {
        None
    } else {
        Some(format!("operator{sym}"))
    }
}

/// Compare a joined SCIP def name to the AST function it landed in (tri-state).
fn compare_names(scip_name: &str, fn_name: &str) -> NameMatch {
    match (canonical_name(scip_name), canonical_name(fn_name)) {
        (Some(a), Some(b)) if a == b => NameMatch::Confirmed,
        (Some(_), Some(_)) => NameMatch::Mismatch,
        _ => NameMatch::Uncomparable,
    }
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
    confirmed: usize,
    misattach: usize,
    uncomparable: usize,
    macro_mismatch: usize,
    coverage_gap: usize,
    join_bug: usize,
    denom_impl: usize,
    denom_header: usize,
    conf_impl: usize,
    conf_header: usize,
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
    let mut sample_misattach: Vec<String> = Vec::new();
    let mut sample_uncomparable: Vec<String> = Vec::new();

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
                    in_denom(&mut c);
                    let label = format!(
                        "scip:{:<24} -> fn:{:<24} {} L{}",
                        d.name,
                        f.name,
                        doc.relative_path,
                        line0 + 1
                    );
                    match compare_names(&d.name, &f.name) {
                        NameMatch::Confirmed => {
                            c.confirmed += 1;
                            match fk {
                                "impl" => c.conf_impl += 1,
                                "header" => c.conf_header += 1,
                                _ => {}
                            }
                        }
                        NameMatch::Mismatch => {
                            c.misattach += 1;
                            if sample_misattach.len() < 25 {
                                sample_misattach.push(label);
                            }
                        }
                        NameMatch::Uncomparable => {
                            c.uncomparable += 1;
                            if sample_uncomparable.len() < 25 {
                                sample_uncomparable.push(label);
                            }
                        }
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

    let denom =
        c.confirmed + c.misattach + c.uncomparable + c.macro_mismatch + c.coverage_gap + c.join_bug;
    let attach_rate = if denom == 0 {
        0.0
    } else {
        c.confirmed as f64 / denom as f64 * 100.0
    };

    println!("=== CJOIN-PROVE-2: {scip} ===");
    println!(
        "valid SCIP docs: {} (impl {} / header {})",
        index.documents.len(),
        c.docs_impl,
        c.docs_header
    );
    println!("total SCIP Method defs: {}", c.total_methods);
    println!("excluded declaration_without_body: {}", c.excluded_decl);
    println!("excluded source_unavailable: {}", c.excluded_nosrc);
    let range_contained = c.confirmed + c.misattach + c.uncomparable;
    let raw_anchored =
        c.misattach + c.uncomparable + c.macro_mismatch + c.coverage_gap + c.join_bug;
    println!("BODY-BEARING DENOMINATOR: {denom}");
    println!(
        "  range-contained joins:   {range_contained}  (range-only would attach ALL of these)"
    );
    println!(
        "  name-CONFIRMED attach:   {}  (range + name agree -> value fact binds)",
        c.confirmed
    );
    println!(
        "  REJECTED mismatch:       {}  (name mismatch -> raw-anchored)",
        c.misattach
    );
    println!(
        "  UNCOMPARABLE unverified: {}  (name not comparable -> raw-anchored, NOT attached)",
        c.uncomparable
    );
    println!(
        "  raw-anchored total:      {raw_anchored}  (mismatch {} + uncomparable {} + coverage {} + macro {} + bug {})",
        c.misattach, c.uncomparable, c.coverage_gap, c.macro_mismatch, c.join_bug
    );
    println!(
        "STRONG-ATTACH RATE (confirmed/denom): {attach_rate:.1}%    rejected attaches: {}",
        c.misattach + c.uncomparable
    );
    println!(
        "header/impl split: denom impl {} / header {}; confirmed impl {} / header {}",
        c.denom_impl, c.denom_header, c.conf_impl, c.conf_header
    );
    let dump = |title: &str, v: &[String]| {
        println!("--- {title} (sample) ---");
        for s in v {
            println!("    {s}");
        }
    };
    dump(
        "REJECTED mismatch (name mismatch -> raw-anchored)",
        &sample_misattach,
    );
    dump(
        "UNCOMPARABLE (name not comparable -> raw-anchored)",
        &sample_uncomparable,
    );
    dump("macro_preprocessor_mismatch", &sample_macro);
    dump("cpp_extractor_coverage_gap", &sample_coverage);
    dump("declaration_without_body", &sample_decl);
    dump("genuine_join_bug", &sample_bug);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_method_match() {
        assert_eq!(
            compare_names("ngx_log_init", "ngx_log_init"),
            NameMatch::Confirmed
        );
    }

    #[test]
    fn qualified_method_match() {
        assert_eq!(compare_names("DBImpl::Get", "Get"), NameMatch::Confirmed);
        assert_eq!(compare_names("Get", "DBImpl::Get"), NameMatch::Confirmed);
    }

    #[test]
    fn constructor_match() {
        assert_eq!(
            compare_names("MutexLock", "MutexLock"),
            NameMatch::Confirmed
        );
    }

    #[test]
    fn destructor_match_and_collapse_mismatch() {
        assert_eq!(
            compare_names("~MutexLock", "~MutexLock"),
            NameMatch::Confirmed
        );
        // a destructor landing in the macro-collapsed class/ctor node is a mismatch
        assert_eq!(
            compare_names("~MutexLock", "MutexLock"),
            NameMatch::Mismatch
        );
    }

    #[test]
    fn distinct_operators_do_not_match() {
        assert_eq!(
            compare_names("operator=", "operator()"),
            NameMatch::Mismatch
        );
        assert_eq!(
            compare_names("operator=", "operator=="),
            NameMatch::Mismatch
        );
        assert_eq!(compare_names("operator<", "operator>"), NameMatch::Mismatch);
        assert_eq!(
            compare_names("operator=", "operator="),
            NameMatch::Confirmed
        );
        assert_eq!(
            compare_names("operator()", "operator()"),
            NameMatch::Confirmed
        );
    }

    #[test]
    fn operator_vs_class_name_is_mismatch() {
        assert_eq!(compare_names("operator=", "MutexLock"), NameMatch::Mismatch);
    }

    #[test]
    fn empty_or_unparseable_is_uncomparable() {
        assert_eq!(compare_names("", "foo"), NameMatch::Uncomparable);
        assert_eq!(compare_names("foo", ""), NameMatch::Uncomparable);
        assert_eq!(
            compare_names("ngx_rbtree_min", "*"),
            NameMatch::Uncomparable
        );
        assert_eq!(compare_names("*", "*"), NameMatch::Uncomparable);
    }

    #[test]
    fn operatorfoo_is_plain_identifier() {
        assert_eq!(
            compare_names("operatorfoo", "operatorfoo"),
            NameMatch::Confirmed
        );
        assert_eq!(
            compare_names("operatorfoo", "operator="),
            NameMatch::Mismatch
        );
    }
}
