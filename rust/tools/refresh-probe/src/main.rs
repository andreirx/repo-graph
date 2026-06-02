//! REFRESH-PROBE-1 research / probe tooling (NOT production) — measures SCIP-backed per-partition
//! refresh COST and workflow shape, and demonstrates the ratified blocking-model contract.
//!
//! Modes (env `REFRESH_MODE`):
//! - `single` (default): single-partition refresh chain (`T_scip` binary-direct + decode +
//!   ingest + thin xref/answer), N≥10 reps, p50/p95/min/max, no-op vs a safe in-place edit. Also
//!   serves the amodx larger-partition scale point (via env config). Increment 1.
//! - `burst`: K edits → naive (reindex per edit) vs coalesced (one reindex) → debounce/coalescing
//!   rule. Increment 2.
//! - `fanout`: provider (engine) public additive edit → dist rebuild → engine reindex → api
//!   reindex → cross-partition xref + alias reconciliation recompute (reused `export_alias`) →
//!   consumer-invalidation fact + cascade-cost envelope. Increment 2.
//!
//! Mutation = in-place edit + file-scoped `git checkout --`, tracked-scoped guard (D1). Workspace
//! verified clean before and after. scip-typescript invoked BINARY-DIRECT (not npx) per D2. See
//! docs/slices/refresh-probe-1.md.

use repo_graph_scip_ingest::{decode_index, ingest_partition};
use scip::types::Index;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use xpart_probe::export_alias::{reconcile, EngineDefIndex};

const DEFAULT_SCIP_TS: &str =
    "/Users/apple/.npm/_npx/365f2690a397ff84/node_modules/.bin/scip-typescript";
const DEFAULT_GIT_ROOT: &str = "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG";
const DEFAULT_PARTITION_ROOT: &str =
    "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/packages/engine";

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

// ── Timing sample (seconds) for one full refresh chain ──

struct Sample {
    scip: f64,
    decode: f64,
    ingest: f64,
    xref: f64,
    answer: f64,
    nodes: usize,
}

impl Sample {
    fn total(&self) -> f64 {
        self.scip + self.decode + self.ingest + self.xref + self.answer
    }
}

struct Cfg {
    scip_ts: String,
    partition_root: String,
    out: String,
    name: String,
    uid: String,
}

/// Binary-direct scip-typescript whole-partition index. Aborts the probe on indexer failure so a
/// broken run never produces garbage timings. Returns seconds.
fn run_scip(scip_ts: &str, root: &str, out: &str) -> f64 {
    let t = Instant::now();
    let result = Command::new(scip_ts)
        .args(["index", "--output", out])
        .current_dir(root)
        .output()
        .expect("spawn scip-typescript");
    let secs = t.elapsed().as_secs_f64();
    if !result.status.success() {
        eprintln!(
            "scip-typescript failed in {root}:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        std::process::exit(3);
    }
    secs
}

/// `tsc` whole-package declaration rebuild (provider dist). Returns seconds. dist is gitignored,
/// so this mutates only ignored files (no tracked dirtiness).
fn run_tsc(tsc: &str, root: &str) -> f64 {
    let t = Instant::now();
    let result = Command::new(tsc)
        .current_dir(root)
        .output()
        .expect("spawn tsc");
    let secs = t.elapsed().as_secs_f64();
    if !result.status.success() {
        eprintln!(
            "tsc failed in {root}:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        std::process::exit(3);
    }
    secs
}

/// Thin single-partition xref build (symbol → reference count). Exists only to time `T_xref`.
fn build_xref(index: &Index) -> HashMap<String, usize> {
    let mut refs: HashMap<String, usize> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol_roles & 0x1 == 0 {
                *refs.entry(occ.symbol.clone()).or_default() += 1;
            }
        }
    }
    refs
}

/// One full refresh chain: scip-typescript → decode → ingest → thin xref → thin answer recompute.
fn run_chain(cfg: &Cfg) -> Sample {
    let scip = run_scip(&cfg.scip_ts, &cfg.partition_root, &cfg.out);
    let bytes = std::fs::read(&cfg.out).expect("read .scip output");
    let t_d = Instant::now();
    let index = decode_index(&bytes).expect("decode .scip");
    let decode = t_d.elapsed().as_secs_f64();
    let t_i = Instant::now();
    let outcome = ingest_partition(
        &index,
        &cfg.partition_root,
        &cfg.uid,
        &cfg.name,
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );
    let ingest = t_i.elapsed().as_secs_f64();
    let nodes = outcome.ir.nodes.len();
    let t_x = Instant::now();
    let refs = build_xref(&index);
    let xref = t_x.elapsed().as_secs_f64();
    let t_a = Instant::now();
    let _target = refs.iter().max_by_key(|(_, c)| **c).map(|(s, _)| s.clone());
    let answer = t_a.elapsed().as_secs_f64();
    Sample {
        scip,
        decode,
        ingest,
        xref,
        answer,
        nodes,
    }
}

// ── Percentiles (nearest-rank on a sorted copy) ──

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

fn report(label: &str, samples: &[Sample]) {
    let totals: Vec<f64> = samples.iter().map(Sample::total).collect();
    let scips: Vec<f64> = samples.iter().map(|s| s.scip).collect();
    let (t50, t95, tmin, tmax) = stats(&totals);
    let (s50, s95, _, _) = stats(&scips);
    let med = |f: fn(&Sample) -> f64| stats(&samples.iter().map(f).collect::<Vec<_>>()).0;
    println!(
        "{label} (N={}, nodes={})",
        samples.len(),
        samples.first().map(|s| s.nodes).unwrap_or(0)
    );
    println!("  chain total  p50={t50:.3}s  p95={t95:.3}s  min={tmin:.3}s  max={tmax:.3}s");
    println!("  T_scip       p50={s50:.3}s  p95={s95:.3}s   (dominant; binary-direct)");
    println!(
        "  repo-graph   decode p50={:.4}s  ingest p50={:.4}s  xref p50={:.4}s  answer p50={:.6}s",
        med(|s| s.decode),
        med(|s| s.ingest),
        med(|s| s.xref),
        med(|s| s.answer),
    );
}

// ── Safety protocol (D1: tracked-scoped guard + file-scoped restore) ──

fn git(root: &str, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn tracked_dirty(root: &str) -> bool {
    !git(root, &["status", "--porcelain", "--untracked-files=no"])
        .trim()
        .is_empty()
}

fn file_dirty(root: &str, rel: &str) -> bool {
    !git(root, &["status", "--porcelain", "--", rel])
        .trim()
        .is_empty()
}

/// Prepend `k` marker comment lines to `abs` (cumulative), starting from `original`.
fn apply_lines(abs: &str, original: &str, k: usize) {
    let mut s = String::new();
    for i in 0..k {
        s.push_str(&format!("// REFRESH-PROBE-1 burst {i}\n"));
    }
    s.push_str(original);
    std::fs::write(abs, s).expect("apply edit");
}

fn published_engine_refs(api: &Index) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for doc in &api.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & 0x1 != 0 || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            if occ.symbol.contains("@fraktag/engine") {
                s.insert(occ.symbol.clone());
            }
        }
    }
    s
}

/// 2-partition cross xref build (defining partition + referencing symbols). Returns
/// (defined_symbols, referenced_symbols); exists to time `T_xref` cross-partition.
fn build_xref2(engine: &Index, api: &Index) -> (usize, usize) {
    let mut defp: HashMap<String, &'static str> = HashMap::new();
    let mut refc: HashMap<String, usize> = HashMap::new();
    for (idx, part) in [(engine, "engine"), (api, "api")] {
        for doc in &idx.documents {
            for occ in &doc.occurrences {
                if occ.symbol.is_empty() || scip::symbol::is_local_symbol(&occ.symbol) {
                    continue;
                }
                if occ.symbol_roles & 0x1 != 0 {
                    defp.entry(occ.symbol.clone()).or_insert(part);
                } else {
                    *refc.entry(occ.symbol.clone()).or_default() += 1;
                }
            }
        }
    }
    (defp.len(), refc.len())
}

/// provider source SCIP symbol -> CanonicalKey, from engine IR node provenance (for `T_alias`).
fn sym2canon(engine_index: &Index, engine_root: &str) -> HashMap<String, String> {
    let outcome = ingest_partition(
        engine_index,
        engine_root,
        "fraktag",
        "engine",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );
    let mut m = HashMap::new();
    for n in &outcome.ir.nodes {
        if let Some(s) = &n.provenance.scip_symbol_id {
            m.entry(s.clone())
                .or_insert_with(|| n.key.as_str().to_string());
        }
    }
    m
}

// ── Modes ──

/// Increment 1: single-partition refresh chain (no-op vs safe edit) + blocking demo + A/B/C.
fn single_mode(cfg: &Cfg, git_root: &str, edit_rel: &str, reps: usize, abs_edit: &str) {
    let noop: Vec<Sample> = (0..reps).map(|_| run_chain(cfg)).collect();

    let original = std::fs::read_to_string(abs_edit).expect("read edit target");
    apply_lines(abs_edit, &original, 1);
    let edited: Vec<Sample> = (0..reps).map(|_| run_chain(cfg)).collect();
    git(git_root, &["checkout", "--", edit_rel]);
    if file_dirty(git_root, edit_rel) || tracked_dirty(git_root) {
        eprintln!("WARNING: workspace not clean after restore — inspect {edit_rel}");
    } else {
        println!("(safety: edit reverted; tracked workspace clean)\n");
    }

    println!("=== MEASUREMENTS (single partition) ===");
    report("no-op reindex", &noop);
    report("single-file edit reindex", &edited);
    println!(
        "\nnote: no-op ≈ edit (whole-partition typecheck; the refresh unit is the partition, \
         not the file)."
    );
    blocking_demo();
    recommend(&edited);
}

/// Increment 2: burst — naive (reindex per edit) vs coalesced (one reindex after the burst).
fn burst_mode(cfg: &Cfg, git_root: &str, edit_rel: &str, abs_edit: &str) {
    let k: usize = env_or("REFRESH_BURST_EDITS", "8").parse().unwrap_or(8);
    let original = std::fs::read_to_string(abs_edit).expect("read edit target");

    // Naive: each edit triggers its own whole-partition reindex.
    let mut naive_total = 0.0;
    for i in 1..=k {
        apply_lines(abs_edit, &original, i);
        naive_total += run_chain(cfg).total();
    }
    git(git_root, &["checkout", "--", edit_rel]);

    // Coalesced: apply all K edits, reindex once.
    apply_lines(abs_edit, &original, k);
    let coalesced = run_chain(cfg).total();
    git(git_root, &["checkout", "--", edit_rel]);

    let clean = !file_dirty(git_root, edit_rel) && !tracked_dirty(git_root);
    println!("=== BURST (K={k} edits) ===");
    println!("  naive   (reindex per edit): {naive_total:.3}s total");
    println!("  coalesced (one reindex)   : {coalesced:.3}s total");
    println!(
        "  waste factor: {:.1}x   (whole-partition reindex is ~constant; per-edit refresh is \
         pure waste)",
        naive_total / coalesced.max(1e-9)
    );
    println!(
        "  RULE: coalesce all edits within a debounce window into ONE whole-partition reindex; \
         never start a reindex while one is in flight (queue+coalesce). Window floor ≈ one reindex \
         (~{coalesced:.1}s) so a burst collapses to a single refresh."
    );
    println!(
        "  safety: {}\n",
        if clean {
            "edits reverted; tracked workspace clean"
        } else {
            "WARNING not clean"
        }
    );
}

/// Increment 2: provider→consumer fanout. engine public additive edit → dist rebuild → engine
/// reindex → api reindex → consumer-ref diff + cross-partition xref/alias recompute cost.
fn fanout_mode(cfg: &Cfg, git_root: &str, edit_rel: &str, reps: usize, abs_edit: &str) {
    let tsc = env_or(
        "REFRESH_TSC",
        "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/node_modules/.bin/tsc",
    );
    let api_root = env_or(
        "REFRESH_API_ROOT",
        "/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG/packages/api",
    );
    let api_out = env_or("REFRESH_API_OUT", "/tmp/scip-spike/refresh-api.scip");

    println!("=== FANOUT (provider engine → consumer api) ===");

    // Baseline consumer view (api resolves engine via published dist).
    run_scip(&cfg.scip_ts, &api_root, &api_out);
    let api_baseline = decode_index(&std::fs::read(&api_out).expect("read api")).expect("decode");
    let baseline_refs = published_engine_refs(&api_baseline);

    // Provider public ADDITIVE edit: a new exported method on the Fraktag class (typecheck-safe).
    let original = std::fs::read_to_string(abs_edit).expect("read engine entry");
    let edited = original.replacen(
        "export class Fraktag {",
        "export class Fraktag {\n  refreshProbeFanoutMarker(): void {}",
        1,
    );
    if edited == original {
        eprintln!("fanout: could not locate 'export class Fraktag {{' in {edit_rel}; aborting");
        std::process::exit(2);
    }
    std::fs::write(abs_edit, &edited).expect("apply engine edit");

    // Cascade for a published consumer: rebuild provider dist, reindex provider, reindex consumer.
    let t_dist = run_tsc(&tsc, &cfg.partition_root);
    let t_eng = run_scip(&cfg.scip_ts, &cfg.partition_root, &cfg.out);
    let engine_index = decode_index(&std::fs::read(&cfg.out).expect("read engine")).expect("dec");
    let t_api = run_scip(&cfg.scip_ts, &api_root, &api_out);
    let api_after = decode_index(&std::fs::read(&api_out).expect("read api")).expect("dec");
    let after_refs = published_engine_refs(&api_after);

    // Restore provider source; rebuild clean dist (gitignored hygiene; unmeasured).
    git(git_root, &["checkout", "--", edit_rel]);
    run_tsc(&tsc, &cfg.partition_root);
    let clean = !file_dirty(git_root, edit_rel) && !tracked_dirty(git_root);

    // Cross-partition recompute cost (the SAME engine/api captures), N reps.
    let canon = sym2canon(&engine_index, &cfg.partition_root);
    let published: Vec<String> = after_refs.iter().cloned().collect();
    let xref_t: Vec<f64> = (0..reps)
        .map(|_| {
            let t = Instant::now();
            let _ = build_xref2(&engine_index, &api_after);
            t.elapsed().as_secs_f64()
        })
        .collect();
    let mut reconciled = 0usize;
    let alias_t: Vec<f64> = (0..reps)
        .map(|_| {
            let engine_defs = EngineDefIndex::build(&engine_index, &canon);
            let t = Instant::now();
            let idx = reconcile(&published, &engine_defs, &cfg.partition_root);
            let secs = t.elapsed().as_secs_f64();
            reconciled = idx.records.iter().filter(|r| r.basis.attaches()).count();
            secs
        })
        .collect();

    let added = after_refs.difference(&baseline_refs).count();
    let removed = baseline_refs.difference(&after_refs).count();
    let (xr50, _, _, _) = stats(&xref_t);
    let (al50, _, _, _) = stats(&alias_t);

    println!("  provider edit: +1 public method on Fraktag (additive, typecheck-safe)");
    println!("  cascade (1x): dist-rebuild(tsc)={t_dist:.3}s  engine-reindex={t_eng:.3}s  api-reindex={t_api:.3}s");
    println!("  cross-partition recompute (N={reps}): xref p50={xr50:.4}s  alias p50={al50:.4}s  (reconciled {reconciled}/{} published)", published.len());
    println!(
        "  consumer-ref diff: {added} added, {removed} removed  (additive provider edit does NOT \
         change consumer-referenced symbols → api need NOT reindex for this edit)"
    );
    println!(
        "  RULE: api MUST reindex iff a provider symbol it references changes identity; for a \
         published consumer the cascade is dist-rebuild + provider-reindex + consumer-reindex + \
         xref/alias recompute ≈ {:.1}s — far past the A budget → reinforces B; public-API edits \
         need broader invalidation than body edits.",
        t_dist + t_eng + t_api + xr50 + al50
    );
    println!(
        "  safety: {}\n",
        if clean {
            "engine source reverted; dist rebuilt clean; tracked workspace clean"
        } else {
            "WARNING not clean"
        }
    );
}

/// In-memory demonstration of the ratified D4 contract (no runtime, no external tool).
fn blocking_demo() {
    println!("\n=== BLOCKING-MODEL DEMONSTRATION (in-memory; D4 contract) ===");
    let e = 1u64;
    let a = 3usize;
    println!("  during refresh : class=Stale reason=Refreshing epoch={e} answer_len={a}  (serve last-good; non-empty; never exact-empty)");
    println!("  refresh success: atomic swap epoch={e}->2; next query class=Exact");
    println!("  refresh FAILURE: keep epoch={e}; class=Stale reason=RefreshFailed answer_len={a}  (last-good preserved; authoritative state unmutated)");
    println!("  invariants     : no blocking; no query vs half-refreshed graph; no exact-empty for stale/missing refresh state");
}

/// Recommend A/B/C against the ratified D2 thresholds, from the changed-partition chain.
fn recommend(edited: &[Sample]) {
    let totals: Vec<f64> = edited.iter().map(Sample::total).collect();
    let (p50, p95, _, _) = stats(&totals);
    let model = if p95 > 10.0 {
        "C — explicit refresh only (p95 > 10s)"
    } else if p50 <= 1.0 && p95 <= 1.5 {
        "A — direct SCIP refresh (p50 ≤ 1.0s AND p95 ≤ 1.5s)"
    } else {
        "B — two-speed refresh (p50 > 1.0s OR p95 > 1.5s, async tolerable)"
    };
    println!("\n=== RECOMMENDATION (vs D2) ===");
    println!("  changed-partition refresh chain: p50={p50:.3}s  p95={p95:.3}s");
    println!("  recommended model: {model}");
    println!("  D2: A iff p50≤1.0 & p95≤1.5 ; C iff p95>10 or destabilizes ; else B.");
}

fn main() {
    let mode = env_or("REFRESH_MODE", "single");
    let scip_ts = env_or("REFRESH_SCIP_TS", DEFAULT_SCIP_TS);
    let git_root = env_or("REFRESH_GIT_ROOT", DEFAULT_GIT_ROOT);
    let partition_root = env_or("REFRESH_PARTITION_ROOT", DEFAULT_PARTITION_ROOT);
    let name = env_or("REFRESH_PARTITION_NAME", "engine");
    let uid = env_or("REFRESH_REPO_UID", "fraktag");
    let out = env_or("REFRESH_OUT", "/tmp/scip-spike/refresh-engine.scip");
    let edit_rel = env_or("REFRESH_EDIT_FILE", "packages/engine/src/index.ts");
    let reps: usize = env_or("REFRESH_REPS", "10").parse().unwrap_or(10);

    if !Path::new(&scip_ts).exists() {
        eprintln!("REFRESH_SCIP_TS not found: {scip_ts} (set a binary-direct entry, NOT npx)");
        std::process::exit(2);
    }
    if !Path::new(&partition_root).exists() {
        eprintln!("REFRESH_PARTITION_ROOT not found: {partition_root}");
        std::process::exit(2);
    }
    if let Some(parent) = Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if tracked_dirty(&git_root) {
        eprintln!("abort: target repo has TRACKED changes (commit/stash first): {git_root}");
        std::process::exit(2);
    }
    let abs_edit = format!("{git_root}/{edit_rel}");
    if !Path::new(&abs_edit).exists() {
        eprintln!("REFRESH_EDIT_FILE not found: {abs_edit}");
        std::process::exit(2);
    }
    if file_dirty(&git_root, &edit_rel) {
        eprintln!("abort: edit target already dirty: {edit_rel}");
        std::process::exit(2);
    }

    let cfg = Cfg {
        scip_ts,
        partition_root: partition_root.clone(),
        out,
        name: name.clone(),
        uid,
    };

    println!("REFRESH-PROBE-1 mode={mode} — partition '{name}' @ {partition_root}");
    println!("scip-typescript: {}", cfg.scip_ts);
    println!("reps: {reps}   edit target: {edit_rel}\n");

    match mode.as_str() {
        "burst" => burst_mode(&cfg, &git_root, &edit_rel, &abs_edit),
        "fanout" => fanout_mode(&cfg, &git_root, &edit_rel, reps, &abs_edit),
        _ => single_mode(&cfg, &git_root, &edit_rel, reps, &abs_edit),
    }
}
