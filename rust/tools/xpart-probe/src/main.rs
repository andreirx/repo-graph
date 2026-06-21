//! XPART-PROVE-1 research / probe tooling (NOT production).
//!
//! 1A — cross-partition `callers` answer-class semantics over two FRAKTAG TS partitions
//! (api, engine) + an always-resident global xref. Proves: no silent incomplete answer.
//!
//! 1B — export-surface reconciliation: a consumer references the provider's *published*
//! `dist/*.d.ts` symbols while the provider partition defines *source* `src/*.ts` symbols, so
//! raw SCIP equality misses the edge. The `export_alias` module reconciles published→source via
//! the declaration map (`.d.ts.map` `sources[]` + descriptor-exact reconstruction), then the
//! answer-class cases are re-run over the dist capture keyed by reconciled identity.
//!
//! Mode is chosen by measured divergence: a source-aligned capture (`api-src.scip`, 0 divergent)
//! runs the 1A path; a published-interface capture (`api-dist.scip`, 95 divergent) runs 1B.
//! See docs/slices/xpart-prove-1.md and xpart-prove-1b.md.
//!
//! Default semantics (ratified): xref-exact where sufficient, else partial-with-explicit-
//! degradation; load-on-demand is opt-in only; forced eager load rejected. `callers` only.

use repo_graph_ir::{IrNode, SourceRange};
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use scip::types::Index;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use xpart_probe::export_alias::{reconcile, Basis, EngineDefIndex};

// ── Always-resident global cross-reference index (partition-level summary) ──

struct Xref {
    /// canonical SCIP symbol -> defining partition.
    defining_partition: HashMap<String, String>,
    /// symbol -> {partition -> reference count}.
    referencing_counts: HashMap<String, BTreeMap<String, usize>>,
    /// epoch token for staleness detection.
    epoch: u64,
}

/// Add a partition's defs + references to the xref. `alias` rewrites a reference symbol to its
/// reconciled provider identity before counting (empty for the provider partition and for 1A).
fn add_partition_to_xref(
    xref: &mut Xref,
    index: &Index,
    partition: &str,
    alias: &HashMap<String, String>,
) {
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
                let sym = alias
                    .get(&occ.symbol)
                    .cloned()
                    .unwrap_or_else(|| occ.symbol.clone());
                *xref
                    .referencing_counts
                    .entry(sym)
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

/// Build caller identities for a partition from its already-ingested IR nodes. `alias` rewrites a
/// reference symbol to its reconciled provider identity so callers key by canonical identity.
fn load_partition(
    index: &Index,
    nodes: &[IrNode],
    alias: &HashMap<String, String>,
) -> LoadedPartition {
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
            if let Some(caller) = enclosing_caller(nodes, &doc.relative_path, line, col) {
                let sym = alias
                    .get(&occ.symbol)
                    .cloned()
                    .unwrap_or_else(|| occ.symbol.clone());
                callers_of
                    .entry(sym)
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

/// Raw cross-package measurement (the before-state, BEFORE any reconciliation): of api's
/// references to `@fraktag/engine` symbols, how many are source-aligned (the symbol is an engine
/// def) vs DIVERGENT (e.g. published-interface `dist/index.d.ts/...` engine never defines).
/// Returns (total_refs, source_aligned, divergent, unique_engine_ref_symbols).
fn raw_cross_package(xref: &Xref, api_index: &Index) -> (usize, usize, usize, Vec<String>) {
    let (mut total, mut matched, mut divergent) = (0usize, 0usize, 0usize);
    let mut uniq: BTreeSet<String> = BTreeSet::new();
    for doc in &api_index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & 0x1 != 0 || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol.contains("@fraktag/engine") {
                total += 1;
                uniq.insert(occ.symbol.clone());
                if xref
                    .defining_partition
                    .get(&occ.symbol)
                    .map(|p| p == "engine")
                    .unwrap_or(false)
                {
                    matched += 1;
                } else {
                    divergent += 1;
                }
            }
        }
    }
    (total, matched, divergent, uniq.into_iter().collect())
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

/// The six answer-class cases, shared by 1A (source-aligned xref) and 1B (reconciled xref).
fn run_answer_class_cases(
    target: &str,
    xref: &Xref,
    engine_lp: &LoadedPartition,
    api_lp: &LoadedPartition,
) {
    println!(
        "xref: {} defined symbols, {} referenced symbols (always-resident summary)",
        xref.defining_partition.len(),
        xref.referencing_counts.len()
    );
    println!(
        "TARGET (engine-defined, api-referenced): {target}\n  counts={:?}\n",
        xref.referencing_counts.get(target)
    );

    let mk = |which: &[&str]| -> BTreeMap<String, &LoadedPartition> {
        let mut m = BTreeMap::new();
        if which.contains(&"engine") {
            m.insert("engine".to_string(), engine_lp);
        }
        if which.contains(&"api") {
            m.insert("api".to_string(), api_lp);
        }
        m
    };

    print_case(
        "CASE 1 — both loaded",
        &callers(target, &mk(&["engine", "api"]), xref, true, 1),
        AnswerClass::Exact,
    );
    print_case(
        "CASE 2a — api loaded, engine unloaded: xref-exact summary",
        &callers_summary(target, xref),
        AnswerClass::Exact,
    );
    print_case(
        "CASE 2b — api loaded, engine unloaded: caller detail",
        &callers(target, &mk(&["api"]), xref, true, 1),
        AnswerClass::Partial,
    );
    print_case(
        "CASE 3 — engine loaded, api unloaded",
        &callers(target, &mk(&["engine"]), xref, true, 1),
        AnswerClass::Partial,
    );
    print_case(
        "CASE 4a — xref absent",
        &callers(target, &mk(&["api"]), xref, false, 1),
        AnswerClass::Unavailable,
    );
    print_case(
        "CASE 4b — xref stale (epoch mismatch)",
        &callers(target, &mk(&["api"]), xref, true, 2),
        AnswerClass::Stale,
    );

    println!("EXIT: every case returned a typed AnswerClass with an explicit reason; no silent-empty path.");
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

    // Ingest both partitions once; reuse the IR nodes for caller resolution and for the
    // provider source-symbol -> CanonicalKey map (D1).
    let engine_outcome = ingest_partition(
        &engine_index,
        &engine_root,
        "fraktag",
        "engine",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );
    let api_outcome = ingest_partition(
        &api_index,
        &api_root,
        "fraktag",
        "api",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );
    let engine_nodes = &engine_outcome.ir.nodes;
    let api_nodes = &api_outcome.ir.nodes;

    // provider source SCIP symbol -> repo-graph CanonicalKey (from IR node provenance).
    let mut sym2canon: HashMap<String, String> = HashMap::new();
    for n in engine_nodes {
        if let Some(s) = &n.provenance.scip_symbol_id {
            sym2canon
                .entry(s.clone())
                .or_insert_with(|| n.key.as_str().to_string());
        }
    }

    let empty: HashMap<String, String> = HashMap::new();

    // Raw xref (no alias) — drives the before-state diagnostic and the 1A path.
    let mut raw_xref = Xref {
        defining_partition: HashMap::new(),
        referencing_counts: HashMap::new(),
        epoch: 1,
    };
    add_partition_to_xref(&mut raw_xref, &engine_index, "engine", &empty);
    add_partition_to_xref(&mut raw_xref, &api_index, "api", &empty);

    let (total, matched, divergent, engine_refs) = raw_cross_package(&raw_xref, &api_index);
    println!("raw:");
    println!("  api->engine refs: {total}");
    println!("  source-aligned: {matched}");
    println!("  divergent: {divergent}\n");

    // Discriminate the two captures by HOW the provider is referenced: published interface
    // (`dist/*.d.ts`) vs source path (`src/*.ts`). Divergence alone does NOT distinguish them —
    // both capture ~17 anonymous `typeLiteralNN` members whose numbering is compilation-unit-
    // relative (unstable across indexes even for the same source file).
    let published_interface = engine_refs.iter().any(|s| s.contains(".d.ts"));

    if !published_interface {
        // ── 1A: source-path capture — answer-class machinery on raw identity ──
        println!("MODE: 1A (source-path capture; provider referenced via src/*.ts)\n");
        if divergent > 0 {
            let tl = engine_refs
                .iter()
                .filter(|s| s.contains("typeLiteral"))
                .count();
            println!(
                "  note: {divergent}/{total} references still diverge — anonymous inline \
                 type-literal members ({tl} unique typeLiteralNN symbols) whose numbering is \
                 compilation-unit-relative and differs between api's reindex of engine src and \
                 engine's own index. The named API surface (incl. the target) is source-aligned.\n"
            );
        }
        let target = match pick_target(&raw_xref) {
            Some(t) => t,
            None => {
                println!("NO source-aligned cross-partition target found.");
                return;
            }
        };
        let engine_lp = load_partition(&engine_index, engine_nodes, &empty);
        let api_lp = load_partition(&api_index, api_nodes, &empty);
        run_answer_class_cases(&target, &raw_xref, &engine_lp, &api_lp);
        return;
    }

    // ── 1B: published-interface capture — export-surface reconciliation ──
    println!(
        "MODE: 1B (published-interface capture; raw SCIP equality misses {divergent}/{total} \
         api->engine references)\n"
    );
    for s in engine_refs.iter().take(3) {
        println!("  sample divergent (published symbol): {s}");
    }
    println!();

    let engine_defs = EngineDefIndex::build(&engine_index, &sym2canon);
    let alias_index = reconcile(&engine_refs, &engine_defs, &engine_root);

    // Occurrence-level tally (matches the raw `total`), so every reference is classified.
    let (mut rec, mut amb, mut unr) = (0usize, 0usize, 0usize);
    let (mut rec_dm, mut rec_ne) = (0usize, 0usize);
    for doc in &api_index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & 0x1 != 0 || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol.contains("@fraktag/engine") {
                match alias_index.basis_of(&occ.symbol) {
                    Some(Basis::DeclarationMapExact) => {
                        rec += 1;
                        rec_dm += 1;
                    }
                    Some(Basis::NameExactUnique) => {
                        rec += 1;
                        rec_ne += 1;
                    }
                    Some(Basis::Ambiguous) => amb += 1,
                    Some(Basis::Unresolved) | None => unr += 1,
                }
            }
        }
    }
    // Invariant: no reference may be silently dropped.
    assert_eq!(
        rec + amb + unr,
        total,
        "silent miss: every api->engine reference must carry a class"
    );

    let uniq_attached = alias_index
        .records
        .iter()
        .filter(|r| r.basis.attaches())
        .count();
    println!("after export alias reconciliation (reference-level):");
    println!("  reconciled: {rec}   (DeclarationMapExact {rec_dm}, NameExactUnique {rec_ne})");
    println!("  ambiguous: {amb}");
    println!("  unreconciled: {unr}");
    println!("  silent misses: 0");
    println!(
        "  (unique published symbols: {}, attached: {})\n",
        alias_index.records.len(),
        uniq_attached
    );

    // Sample a reconciled alias with provenance (D3: alias carries basis + evidence).
    if let Some(r) = alias_index.records.iter().find(|r| r.basis.attaches()) {
        println!("sample alias [{}]:", r.basis.label());
        println!("  published : {}", r.published_symbol);
        println!("  package   : {}@{}", r.package_name, r.package_version);
        println!(
            "  decl file : {}",
            r.declaration_file.as_deref().unwrap_or("-")
        );
        println!(
            "  provider  : {}",
            r.provider_source_symbol.as_deref().unwrap_or("-")
        );
        println!(
            "  canonical : {}",
            r.canonical_key.as_deref().unwrap_or("-")
        );
        println!(
            "  via       : {} (source {})\n",
            r.declaration_map.as_deref().unwrap_or("-"),
            r.source_file.as_deref().unwrap_or("-")
        );
    }
    // Surface any non-reconciled cases explicitly (never silent).
    for r in alias_index.records.iter().filter(|r| !r.basis.attaches()) {
        println!(
            "  NON-RECONCILED [{}] {} — {}",
            r.basis.label(),
            r.published_symbol,
            r.reason
        );
    }
    println!();

    // Build the CANONICAL xref: api references rewritten through the alias map to provider
    // identity; engine unchanged. Then the answer-class cases run on the dist capture.
    let alias_map = alias_index.alias_map();
    let mut canon = Xref {
        defining_partition: HashMap::new(),
        referencing_counts: HashMap::new(),
        epoch: 1,
    };
    add_partition_to_xref(&mut canon, &engine_index, "engine", &empty);
    add_partition_to_xref(&mut canon, &api_index, "api", alias_map);

    let target = match pick_target(&canon) {
        Some(t) => t,
        None => {
            println!(
                "NO reconciled cross-partition target — reconciliation attached no api->engine \
                 alias that engine also defines. (Not a silent failure: see the records above.)"
            );
            return;
        }
    };
    let engine_lp = load_partition(&engine_index, engine_nodes, &empty);
    let api_lp = load_partition(&api_index, api_nodes, alias_map);
    run_answer_class_cases(&target, &canon, &engine_lp, &api_lp);
}
