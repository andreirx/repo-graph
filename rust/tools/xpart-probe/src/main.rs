//! XPART-PROVE-1 research / probe tooling (NOT production) — cross-partition `callers`
//! answer-class semantics over two FRAKTAG TS partitions (api, engine) + an always-resident
//! global xref. Proves: no silent incomplete answer. See docs/slices/xpart-prove-1.md.
//!
//! Default semantics (ratified): xref-exact where sufficient, else partial-with-explicit-
//! degradation; load-on-demand is opt-in only; forced eager load rejected. `callers` only.

use repo_graph_ir::{IrNode, SourceRange};
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use scip::types::Index;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

// ── Always-resident global cross-reference index (partition-level summary) ──

struct Xref {
    /// canonical SCIP symbol -> defining partition.
    defining_partition: HashMap<String, String>,
    /// symbol -> {partition -> reference count}.
    referencing_counts: HashMap<String, BTreeMap<String, usize>>,
    /// epoch token for staleness detection.
    epoch: u64,
}

fn add_partition_to_xref(xref: &mut Xref, index: &Index, partition: &str) {
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.range.len() < 2 || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol_roles & 0x1 != 0 {
                xref.defining_partition
                    .entry(occ.symbol.clone())
                    .or_insert_with(|| partition.to_string());
            } else {
                *xref
                    .referencing_counts
                    .entry(occ.symbol.clone())
                    .or_default()
                    .entry(partition.to_string())
                    .or_default() += 1;
            }
        }
    }
}

/// An engine-defined symbol referenced by api; prefer one ALSO referenced by engine (so the
/// "engine unloaded" case has missing engine-side detail), max total references.
fn pick_target(xref: &Xref) -> Option<String> {
    let mut best: Option<(String, usize)> = None;
    for (sym, counts) in &xref.referencing_counts {
        let defined_in_engine = xref
            .defining_partition
            .get(sym)
            .map(|p| p == "engine")
            .unwrap_or(false);
        if defined_in_engine && counts.contains_key("api") {
            let score = counts.values().sum::<usize>() + if counts.len() > 1 { 1000 } else { 0 };
            if best.as_ref().is_none_or(|(_, b)| score > *b) {
                best = Some((sym.clone(), score));
            }
        }
    }
    best.map(|(s, _)| s)
}

// ── Residency-dependent detail: caller identities within a loaded partition ──

struct LoadedPartition {
    /// symbol -> caller canonical keys within this partition.
    callers_of: HashMap<String, BTreeSet<String>>,
}

fn load_partition(index: &Index, root: &str, repo_uid: &str, pid: &str) -> LoadedPartition {
    let outcome = ingest_partition(index, root, repo_uid, pid, "scip-typescript", "0.4.0", "h");
    let nodes = outcome.ir.nodes;
    let mut callers_of: HashMap<String, BTreeSet<String>> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & 0x1 != 0
                || occ.range.len() < 2
                || scip::symbol::is_local_symbol(&occ.symbol)
            {
                continue;
            }
            let line = occ.range[0] as i64 + 1;
            let col = occ.range[1] as i64;
            if let Some(caller) = enclosing_caller(&nodes, &doc.relative_path, line, col) {
                callers_of
                    .entry(occ.symbol.clone())
                    .or_default()
                    .insert(caller.to_string());
            }
        }
    }
    LoadedPartition { callers_of }
}

fn enclosing_caller<'a>(nodes: &'a [IrNode], file: &str, line: i64, col: i64) -> Option<&'a str> {
    nodes
        .iter()
        .filter(|n| {
            n.range
                .as_ref()
                .map(|r| r.file == file && sr_contains(r, line, col))
                .unwrap_or(false)
        })
        .min_by_key(|n| n.range.as_ref().map(sr_span).unwrap_or(i64::MAX))
        .map(|n| n.key.as_str())
}

fn sr_contains(r: &SourceRange, line: i64, col: i64) -> bool {
    let (ls, le, cs, ce) = (
        r.start_line as i64,
        r.end_line as i64,
        r.start_col as i64,
        r.end_col as i64,
    );
    let after = line > ls || (line == ls && col >= cs);
    let before = line < le || (line == le && col <= ce);
    after && before
}

fn sr_span(r: &SourceRange) -> i64 {
    (r.end_line as i64 - r.start_line as i64) * 100_000 + (r.end_col as i64 - r.start_col as i64)
}

// ── Answer-class contract ──

#[derive(Debug, PartialEq, Eq)]
enum AnswerClass {
    Exact,
    Partial,
    Unavailable,
    Stale,
}

#[derive(Debug)]
enum Granularity {
    PartitionSummary,
    CallerDetail,
}

struct CallersAnswer {
    class: AnswerClass,
    granularity: Granularity,
    per_partition_counts: BTreeMap<String, usize>,
    caller_identities: Vec<(String, String)>,
    loaded_partitions: Vec<String>,
    missing_partitions: Vec<String>,
    reason: String,
}

fn unavailable(loaded: &BTreeMap<String, &LoadedPartition>, reason: &str) -> CallersAnswer {
    CallersAnswer {
        class: AnswerClass::Unavailable,
        granularity: Granularity::PartitionSummary,
        per_partition_counts: BTreeMap::new(),
        caller_identities: vec![],
        loaded_partitions: loaded.keys().cloned().collect(),
        missing_partitions: vec![],
        reason: reason.to_string(),
    }
}

/// `callers(target)` under a residency set. Default semantics: xref gives exact per-partition
/// counts; caller identities only for resident referencing partitions; missing ones listed.
fn callers(
    target: &str,
    loaded: &BTreeMap<String, &LoadedPartition>,
    xref: &Xref,
    xref_resident: bool,
    loaded_epoch: u64,
) -> CallersAnswer {
    if !xref_resident {
        return unavailable(loaded, "global xref not resident / not built");
    }
    if loaded_epoch != xref.epoch {
        return CallersAnswer {
            class: AnswerClass::Stale,
            granularity: Granularity::PartitionSummary,
            per_partition_counts: BTreeMap::new(),
            caller_identities: vec![],
            loaded_partitions: loaded.keys().cloned().collect(),
            missing_partitions: vec![],
            reason: format!("xref epoch {} != loaded epoch {loaded_epoch}", xref.epoch),
        };
    }
    let counts = match xref.referencing_counts.get(target) {
        Some(c) => c.clone(),
        None => return unavailable(loaded, "symbol not referenced in any indexed partition"),
    };
    let referencing: Vec<String> = counts.keys().cloned().collect();
    let missing: Vec<String> = referencing
        .iter()
        .filter(|p| !loaded.contains_key(*p))
        .cloned()
        .collect();
    let mut caller_identities = Vec::new();
    for p in &referencing {
        if let Some(lp) = loaded.get(p) {
            if let Some(cs) = lp.callers_of.get(target) {
                for c in cs {
                    caller_identities.push((p.clone(), c.clone()));
                }
            }
        }
    }
    let (class, reason) = if missing.is_empty() {
        (
            AnswerClass::Exact,
            "all referencing partitions resident; caller identities complete".to_string(),
        )
    } else {
        let present: Vec<&String> = referencing
            .iter()
            .filter(|p| loaded.contains_key(*p))
            .collect();
        (
            AnswerClass::Partial,
            format!(
                "caller identities complete for loaded {present:?}; per-partition COUNTS exact \
                 for missing {missing:?} from xref, identities unavailable"
            ),
        )
    };
    CallersAnswer {
        class,
        granularity: Granularity::CallerDetail,
        per_partition_counts: counts,
        caller_identities,
        loaded_partitions: loaded.keys().cloned().collect(),
        missing_partitions: missing,
        reason,
    }
}

/// (a) xref-exact path: exact per-partition reference COUNTS without loading any partition.
fn callers_summary(target: &str, xref: &Xref) -> CallersAnswer {
    match xref.referencing_counts.get(target) {
        Some(counts) => CallersAnswer {
            class: AnswerClass::Exact,
            granularity: Granularity::PartitionSummary,
            per_partition_counts: counts.clone(),
            caller_identities: vec![],
            loaded_partitions: vec![],
            missing_partitions: vec![],
            reason: "exact per-partition reference counts from always-resident xref (no detail)"
                .to_string(),
        },
        None => CallersAnswer {
            class: AnswerClass::Unavailable,
            granularity: Granularity::PartitionSummary,
            per_partition_counts: BTreeMap::new(),
            caller_identities: vec![],
            loaded_partitions: vec![],
            missing_partitions: vec![],
            reason: "symbol not in xref".to_string(),
        },
    }
}

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

/// Like `env_or` but REQUIRED: exits (code 2) with a directive message if absent. Used for
/// inputs that have no safe default — there is no single valid api capture to fall back to.
fn env_required(k: &str, guidance: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| {
        eprintln!("error: {k} is required — {guidance}");
        std::process::exit(2);
    })
}

/// Cross-package divergence (the src-vs-dist finding): how many api references to
/// `@fraktag/engine` symbols MATCH an engine definition (source-aligned) vs are DIVERGENT
/// (e.g. published-interface `dist/index.d.ts/...` symbols engine.scip never defines).
fn cross_package_diagnostic(xref: &Xref, api_index: &Index) {
    let (mut total, mut matched, mut divergent) = (0usize, 0usize, 0usize);
    let mut sample: Vec<String> = Vec::new();
    for doc in &api_index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & 0x1 != 0 || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol.contains("@fraktag/engine") {
                total += 1;
                if xref
                    .defining_partition
                    .get(&occ.symbol)
                    .map(|p| p == "engine")
                    .unwrap_or(false)
                {
                    matched += 1;
                } else {
                    divergent += 1;
                    if sample.len() < 5 {
                        sample.push(occ.symbol.clone());
                    }
                }
            }
        }
    }
    println!(
        "CROSS-PACKAGE DIAGNOSTIC: api references to @fraktag/engine symbols = {total} \
         (source-aligned {matched}, DIVERGENT {divergent})"
    );
    for s in &sample {
        println!("    divergent (not an engine.scip def — published-interface symbol): {s}");
    }
}

fn print_case(title: &str, a: &CallersAnswer, expect: AnswerClass) {
    let ok = if a.class == expect { "OK" } else { "MISMATCH" };
    println!("{title}");
    println!(
        "  class={:?} [{ok}, expected {expect:?}]  granularity={:?}",
        a.class, a.granularity
    );
    println!("  per_partition_counts={:?}", a.per_partition_counts);
    println!(
        "  caller_identities={} (loaded {:?})  missing_partitions={:?}",
        a.caller_identities.len(),
        a.loaded_partitions,
        a.missing_partitions
    );
    println!("  reason: {}\n", a.reason);
    assert_eq!(a.class, expect, "answer class mismatch for {title}");
    // Forbid silent empty: every non-Exact answer must carry an explicit reason.
    assert!(
        matches!(a.class, AnswerClass::Exact) || !a.reason.is_empty(),
        "silent non-exact answer for {title}"
    );
}

fn main() {
    // XPART_API_SCIP is REQUIRED — there is no single valid api capture. The original
    // api.scip is discarded (engine was unlinked at index time). Force the operator to choose
    // which view they are probing:
    //   api-src.scip  — source-aligned; the 1A answer-class proof
    //   api-dist.scip — published-interface; the 1B dist<->src divergence proof
    let api_scip = env_required(
        "XPART_API_SCIP",
        "set it to a valid api capture: api-src.scip (source-aligned; 1A answer-class proof) \
         or api-dist.scip (published-interface; 1B divergence proof). The original api.scip is \
         discarded (engine was unlinked at index time).",
    );
    let engine_scip = env_or("XPART_ENGINE_SCIP", "/tmp/scip-spike/engine.scip");
    let api_root = env_or(
        "XPART_API_ROOT",
        "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/packages/api",
    );
    let engine_root = env_or(
        "XPART_ENGINE_ROOT",
        "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/packages/engine",
    );

    let api_index =
        decode_index(&fs::read(&api_scip).expect("read XPART_API_SCIP")).expect("decode api");
    let engine_index = decode_index(&fs::read(&engine_scip).expect("read XPART_ENGINE_SCIP"))
        .expect("decode engine");

    // Always-resident global xref (partition-level summary; built from both SCIP).
    let mut xref = Xref {
        defining_partition: HashMap::new(),
        referencing_counts: HashMap::new(),
        epoch: 1,
    };
    add_partition_to_xref(&mut xref, &engine_index, "engine");
    add_partition_to_xref(&mut xref, &api_index, "api");

    let target = match pick_target(&xref) {
        Some(t) => t,
        None => {
            cross_package_diagnostic(&xref, &api_index);
            println!(
                "\nNO source-aligned cross-partition target — raw SCIP symbol equality is \
                 INSUFFICIENT here (published-interface / dist-vs-src divergence; XPART-PROVE-1B). \
                 Re-run with a source-path api capture (XPART_API_SCIP=api-src.scip) for the 1A \
                 answer-class proof."
            );
            return;
        }
    };
    println!(
        "xref: {} defined symbols, {} referenced symbols (always-resident summary)",
        xref.defining_partition.len(),
        xref.referencing_counts.len()
    );
    println!(
        "TARGET (engine-defined, api-referenced): {target}\n  counts={:?}\n",
        xref.referencing_counts[&target]
    );

    // Ingest both partitions once; share by reference across residency cases.
    let engine_lp = load_partition(&engine_index, &engine_root, "fraktag", "engine");
    let api_lp = load_partition(&api_index, &api_root, "fraktag", "api");
    let mk = |which: &[&str]| -> BTreeMap<String, &LoadedPartition> {
        let mut m = BTreeMap::new();
        if which.contains(&"engine") {
            m.insert("engine".to_string(), &engine_lp);
        }
        if which.contains(&"api") {
            m.insert("api".to_string(), &api_lp);
        }
        m
    };

    print_case(
        "CASE 1 — both loaded",
        &callers(&target, &mk(&["engine", "api"]), &xref, true, 1),
        AnswerClass::Exact,
    );
    print_case(
        "CASE 2a — api loaded, engine unloaded: xref-exact summary",
        &callers_summary(&target, &xref),
        AnswerClass::Exact,
    );
    print_case(
        "CASE 2b — api loaded, engine unloaded: caller detail",
        &callers(&target, &mk(&["api"]), &xref, true, 1),
        AnswerClass::Partial,
    );
    print_case(
        "CASE 3 — engine loaded, api unloaded",
        &callers(&target, &mk(&["engine"]), &xref, true, 1),
        AnswerClass::Partial,
    );
    print_case(
        "CASE 4a — xref absent",
        &callers(&target, &mk(&["api"]), &xref, false, 1),
        AnswerClass::Unavailable,
    );
    print_case(
        "CASE 4b — xref stale (epoch mismatch)",
        &callers(&target, &mk(&["api"]), &xref, true, 2),
        AnswerClass::Stale,
    );

    println!("EXIT: every case returned a typed AnswerClass with an explicit reason; no silent-empty path.");
}
