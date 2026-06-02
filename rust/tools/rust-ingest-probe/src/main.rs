//! RUST-INGEST-PROVE-1 research / probe tooling (NOT production) — measures Rust SCIP ingestion
//! (`rust-analyzer scip` per crate) to DEFINE THE RUST SUPPORT BOUNDARY honestly.
//!
//! Per selected crate (N≥3): T_scip (binary-direct rust-analyzer), decode, ingest
//! (`ingest_partition`, panic-guarded), duplicate-symbol count, definition-not-in-document count
//! (parsed from rust-analyzer stderr), local/global symbol ratio, fallback-identity rate, and
//! cross-crate reference evidence (self / other repo-graph crate / stdlib / external). Whole-
//! workspace export attempted ONCE (diagnostic only; panic → record unsupported, do not chase).
//!
//! Refresh class is EVIDENCE-GATED (D4): classified from measured N≥3 p50/p95, NOT from TS
//! thresholds. This probe MEASURES and BOUNDS; it does not fix rust-analyzer or change ingest
//! core. See docs/slices/rust-ingest-prove-1.md.

use repo_graph_scip_ingest::{decode_index, ingest_partition};
use scip::types::Index;
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const DEFAULT_RA: &str = "/Users/apple/.cargo/bin/rust-analyzer";
const DEFAULT_WORKSPACE: &str = "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust";
const DEFAULT_CRATES: &str = "storage,indexer,rgr";
const DEFAULT_OUT_DIR: &str = "/tmp/scip-spike";

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn stats(values: &[f64]) -> (f64, f64, f64, f64) {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pctl = |q: f64| -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        let idx = (q * (v.len() as f64 - 1.0)).round() as usize;
        v[idx.min(v.len() - 1)]
    };
    (pctl(0.5), pctl(0.95), v[0], v[v.len() - 1])
}

/// Binary-direct `rust-analyzer scip <root> --output <out>`. Returns (seconds, success, combined
/// stderr+stdout diagnostics). rust-analyzer never aborts the probe — a failure/panic is itself
/// evidence (recorded).
fn run_ra(ra: &str, root: &str, out: &str) -> (f64, bool, String) {
    let t = Instant::now();
    let res = Command::new(ra)
        .args(["scip", root, "--output", out])
        .output();
    let secs = t.elapsed().as_secs_f64();
    match res {
        Ok(o) => {
            // Combine BOTH streams: rust-analyzer spreads diagnostics (def-not-in-document,
            // duplicate warnings) across stderr and stdout; under-reporting would violate the
            // honest-boundary purpose (D3).
            let mut diag = String::from_utf8_lossy(&o.stderr).to_string();
            diag.push('\n');
            diag.push_str(&String::from_utf8_lossy(&o.stdout));
            (secs, o.status.success(), diag)
        }
        Err(e) => (secs, false, format!("spawn error: {e}")),
    }
}

fn count_substr(haystack: &str, needle: &str) -> usize {
    haystack
        .lines()
        .filter(|l| l.to_lowercase().contains(needle))
        .count()
}

/// Count rust-analyzer "definition not in document" diagnostics across known message variants
/// (do NOT under-report — D3 honesty).
fn count_def_not_in_doc(diag: &str) -> usize {
    diag.lines()
        .filter(|l| {
            let l = l.to_lowercase();
            l.contains("not in document")
                || l.contains("not found in document")
                || (l.contains("definition") && l.contains("document"))
        })
        .count()
}

#[derive(Default)]
struct ScipMetrics {
    docs: usize,
    occurrences: usize,
    definitions: usize,
    dup_def_symbols: usize,
    dup_def_occurrences: usize,
    local: usize,
    global: usize,
    distinct_local: usize,
    distinct_global: usize,
    self_refs: usize,
    other_repo_graph_refs: usize,
    stdlib_refs: usize,
    external_refs: usize,
    self_pkg: String,
    sample_path: String,
}

fn package_name(symbol: &str) -> Option<String> {
    // rust-analyzer symbol: `rust-analyzer cargo <crate> <version> <descriptors>`.
    scip::symbol::parse_symbol(symbol)
        .ok()
        .and_then(|s| s.package.into_option().map(|p| p.name))
        .filter(|n| !n.is_empty())
}

fn analyze_scip(index: &Index) -> ScipMetrics {
    let mut m = ScipMetrics {
        docs: index.documents.len(),
        ..Default::default()
    };
    let mut def_symbols: HashMap<String, usize> = HashMap::new();
    let mut def_pkg_freq: HashMap<String, usize> = HashMap::new();
    let mut local_syms: HashSet<String> = HashSet::new();
    let mut global_syms: HashSet<String> = HashSet::new();

    for doc in &index.documents {
        if m.sample_path.is_empty() {
            m.sample_path = doc.relative_path.clone();
        }
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() {
                continue;
            }
            m.occurrences += 1;
            let is_def = occ.symbol_roles & 0x1 != 0;
            if scip::symbol::is_local_symbol(&occ.symbol) {
                m.local += 1;
                local_syms.insert(occ.symbol.clone());
                if is_def {
                    m.definitions += 1;
                }
                continue;
            }
            m.global += 1;
            global_syms.insert(occ.symbol.clone());
            if is_def {
                m.definitions += 1;
                *def_symbols.entry(occ.symbol.clone()).or_default() += 1;
                if let Some(p) = package_name(&occ.symbol) {
                    *def_pkg_freq.entry(p).or_default() += 1;
                }
            }
        }
    }
    // Self crate = most common package among (global) definitions.
    m.self_pkg = def_pkg_freq
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(p, _)| p)
        .unwrap_or_default();
    m.distinct_local = local_syms.len();
    m.distinct_global = global_syms.len();

    for (_, c) in def_symbols.iter().filter(|(_, c)| **c > 1) {
        m.dup_def_symbols += 1;
        m.dup_def_occurrences += c - 1;
    }

    // Cross-crate reference classification (global references only).
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol.is_empty()
                || occ.symbol_roles & 0x1 != 0
                || scip::symbol::is_local_symbol(&occ.symbol)
            {
                continue;
            }
            match package_name(&occ.symbol) {
                Some(p) if p == m.self_pkg => m.self_refs += 1,
                Some(p) if p.starts_with("repo-graph") => m.other_repo_graph_refs += 1,
                Some(p) if matches!(p.as_str(), "std" | "core" | "alloc" | "proc_macro") => {
                    m.stdlib_refs += 1
                }
                Some(_) => m.external_refs += 1,
                None => {}
            }
        }
    }
    m
}

fn main() {
    let ra = env_or("RUST_INGEST_RA", DEFAULT_RA);
    let workspace = env_or("RUST_INGEST_WORKSPACE", DEFAULT_WORKSPACE);
    let crates_csv = env_or("RUST_INGEST_CRATES", DEFAULT_CRATES);
    let out_dir = env_or("RUST_INGEST_OUT_DIR", DEFAULT_OUT_DIR);
    let reps: usize = env_or("RUST_INGEST_REPS", "3").parse().unwrap_or(3);
    let do_whole = env_or("RUST_INGEST_WHOLE", "1") == "1";

    if !Path::new(&ra).exists() {
        eprintln!("RUST_INGEST_RA not found: {ra}");
        std::process::exit(2);
    }
    let _ = std::fs::create_dir_all(&out_dir);

    println!("RUST-INGEST-PROVE-1 — workspace {workspace}");
    println!("rust-analyzer: {ra}");
    println!("crates: {crates_csv}   reps: {reps}\n");

    for crate_name in crates_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let crate_dir = format!("{workspace}/crates/{crate_name}");
        if !Path::new(&crate_dir).exists() {
            println!("crate '{crate_name}': dir not found ({crate_dir}) — SKIP\n");
            continue;
        }
        let out = format!("{out_dir}/rust-{crate_name}.scip");

        println!("=== crate '{crate_name}' @ {crate_dir} ===");
        let mut scip_secs = Vec::new();
        let mut last_stderr = String::new();
        let mut ok = true;
        for _ in 0..reps {
            let (secs, success, stderr) = run_ra(&ra, &crate_dir, &out);
            scip_secs.push(secs);
            last_stderr = stderr;
            if !success {
                ok = false;
            }
        }
        if !ok && !Path::new(&out).exists() {
            println!(
                "  rust-analyzer scip FAILED (no output produced) — crate UNSUPPORTED this run."
            );
            println!(
                "  stderr tail: {}\n",
                last_stderr
                    .lines()
                    .rev()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
            continue;
        }
        let (s50, s95, smin, smax) = stats(&scip_secs);
        let def_not_in_doc = count_def_not_in_doc(&last_stderr);
        let panics = count_substr(&last_stderr, "panic");

        let bytes = match std::fs::read(&out) {
            Ok(b) => b,
            Err(e) => {
                println!("  cannot read {out}: {e} — SKIP analysis\n");
                continue;
            }
        };
        let t_d = Instant::now();
        let index = match decode_index(&bytes) {
            Ok(i) => i,
            Err(e) => {
                println!("  decode failed: {e} — SKIP\n");
                continue;
            }
        };
        let decode = t_d.elapsed().as_secs_f64();
        let m = analyze_scip(&index);

        // ingest_partition is TS-coupled (AST join). Guard against a Rust-specific panic so it is
        // recorded as a boundary fact, not a probe crash.
        let ingest = catch_unwind(AssertUnwindSafe(|| {
            let t = Instant::now();
            let outcome = ingest_partition(
                &index,
                &crate_dir,
                "repo-graph",
                crate_name,
                "rust-analyzer",
                "x",
                "h",
                "",
            );
            let secs = t.elapsed().as_secs_f64();
            let nodes = outcome.ir.nodes.len();
            let fb = outcome.ir.fallback_node_count();
            (secs, nodes, fb)
        }));

        println!("  T_scip   p50={s50:.2}s p95={s95:.2}s min={smin:.2}s max={smax:.2}s (N={reps}, binary-direct)");
        println!(
            "  decode   {decode:.4}s   sample doc path: {}",
            m.sample_path
        );
        println!(
            "  scip     docs={} occ={} defs={} self_pkg={}",
            m.docs, m.occurrences, m.definitions, m.self_pkg
        );
        println!(
            "  local/global: local={} global={} (occurrence local ratio {:.1}%)",
            m.local,
            m.global,
            100.0 * m.local as f64 / (m.occurrences.max(1)) as f64
        );
        println!(
            "  distinct symbols: local={} global={} (distinct-local ratio {:.1}%)",
            m.distinct_local,
            m.distinct_global,
            100.0 * m.distinct_local as f64 / (m.distinct_local + m.distinct_global).max(1) as f64
        );
        println!(
            "  duplicates: {} symbols defined >1x ({} dup occurrences)",
            m.dup_def_symbols, m.dup_def_occurrences
        );
        println!(
            "  definition-not-in-document (stderr): {def_not_in_doc}   panics-in-stderr: {panics}"
        );
        println!(
            "  cross-crate refs: self={} other-repo-graph={} stdlib={} external={}",
            m.self_refs, m.other_repo_graph_refs, m.stdlib_refs, m.external_refs
        );
        match ingest {
            Ok((secs, nodes, fb)) => println!(
                "  ingest   {secs:.3}s  nodes={nodes}  fallback={fb} ({:.1}% SCIP-synthesized identity)",
                100.0 * fb as f64 / nodes.max(1) as f64
            ),
            Err(_) => println!(
                "  ingest   PANICKED (ingest_partition is TS-coupled; Rust needs a separate ingest path — boundary finding, deferred)"
            ),
        }
        println!();
    }

    // D1: whole-workspace export is a DIAGNOSTIC ONLY — run once, never chase.
    if do_whole {
        let out = format!("{out_dir}/rust-workspace.scip");
        println!("=== whole-workspace export (DIAGNOSTIC, one shot) ===");
        let (secs, success, stderr) = run_ra(&ra, &workspace, &out);
        let panicked = count_substr(&stderr, "panic") > 0;
        if success && Path::new(&out).exists() {
            println!(
                "  whole-workspace SUCCEEDED in {secs:.1}s (prior spike panicked — re-check D1)."
            );
        } else {
            println!(
                "  whole-workspace UNSUPPORTED ({:.1}s; {}). Recorded; not chased (D1: per-crate only).",
                secs,
                if panicked { "panicked" } else { "failed/non-zero" }
            );
            println!(
                "  stderr tail: {}",
                stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ")
            );
        }
        println!();
    }

    println!(
        "NOTE: refresh class is evidence-gated (D4) — classify from T_scip p50/p95 above, NOT TS \
         thresholds. Maturity default PARTIAL/BETA (D5). Write the support boundary from these \
         numbers; no GO without it."
    );
}
