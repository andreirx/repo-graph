//! LIVEGRAPH-INTEGRATION-1B: dev-only preload of a SUPPLIED SCIP index into a repo's in-memory
//! LiveGraph. Decode + ingest + feed ONLY — the daemon does NOT run scip-typescript or do package
//! discovery / refresh orchestration (that is LIVEGRAPH-INTEGRATION-1C).

use repo_graph_ir::CanonicalKey;
use repo_graph_livegraph::{
    FileImportCyclesAnswer, LiveGraph, ModuleImportCyclesAnswer, ModuleStatRow,
};
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::queries::{
    martin_metrics, CalleeResult, CallerResult, ModuleStatsResult, ResolvedSymbol,
};
use repo_graph_trust_model::{AnswerClass, FreshnessState, Granularity, LanguageSupport};
use serde::Serialize;
use serde_json::{json, Value};

use crate::state::RepoState;

/// Decode a supplied `index.scip`, ingest it into a `PartitionIr` + complexity map, and feed both
/// into `lg` (epoch-stamped). Pure over the runtime (no daemon state) so it is unit-testable against
/// the committed fixture. Returns a summary `{partition_id, nodes, edges, value_facts, epoch}`.
/// A partition's REPO-RELATIVE prefix = `source_root` relative to `repo_path` (POSIX, no trailing
/// slash). Empty when `source_root == repo_path` (a repo-root package) or when `source_root` is not
/// under `repo_path` (defensive). The repo-relative key namespace prepends this (KEY-NAMESPACE-REPO-RELATIVE-1).
pub fn repo_relative_prefix(repo_path: &str, source_root: &str) -> String {
    std::path::Path::new(source_root)
        .strip_prefix(repo_path)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn preload_into(
    lg: &mut LiveGraph,
    repo_uid: &str,
    partition_id: &str,
    scip_path: &str,
    source_root: &str,
    partition_prefix: &str,
) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(scip_path).map_err(|e| format!("read scip '{scip_path}': {e}"))?;
    let index = decode_index(&bytes).map_err(|e| format!("decode scip '{scip_path}': {e}"))?;
    // The daemon DECODES + ingests a supplied index; it does NOT run the indexer (1C). `partition_prefix`
    // is the partition's repo-relative root for the repo-relative key namespace (KEY-NAMESPACE-REPO-RELATIVE-1).
    let outcome = ingest_partition(
        &index,
        source_root,
        repo_uid,
        partition_id,
        "scip-typescript",
        "preload",
        "preload",
        partition_prefix,
    );
    let nodes = outcome.ir.nodes.len();
    let edges = outcome.ir.edges.len();
    let value_facts = outcome.complexity.len();
    repo_graph_livegraph_feed::feed_partition(
        lg,
        partition_id,
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );
    let epoch = lg.partition_epoch(partition_id).map(|e| e.0);
    Ok(serde_json::json!({
        "partition_id": partition_id,
        "nodes": nodes,
        "edges": edges,
        "value_facts": value_facts,
        "epoch": epoch,
    }))
}

/// Preload a partition into the repo's LiveGraph (creating it if absent). Write-locks the repo's
/// LiveGraph — interior mutability over the shared `Arc<RepoState>`.
#[allow(clippy::too_many_arguments)]
pub fn preload_partition(
    repo_state: &RepoState,
    repo_uid: &str,
    partition_id: &str,
    scip_path: &str,
    source_root: &str,
    partition_prefix: &str,
) -> Result<serde_json::Value, String> {
    let mut guard = repo_state.livegraph.write();
    let lg = guard.get_or_insert_with(LiveGraph::new);
    preload_into(
        lg,
        repo_uid,
        partition_id,
        scip_path,
        source_root,
        partition_prefix,
    )
}

// ── LIVEGRAPH-INTEGRATION-1B serving + comparison (S2 engine flag, S3 compare report) ──

/// Engine selector (S2 + QUERY-MIGRATION-CLI-1). Default `Auto`: serve LiveGraph when complete
/// (Exact + Fresh + TS-only), else fall back to SQLite — with `backend_used`/`fallback_reason` metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// LiveGraph-when-complete, else labelled SQLite fallback (the new default; QUERY-MIGRATION-CLI-1).
    Auto,
    /// Force the SQLite path (the old default; an explicit escape hatch).
    Sqlite,
    /// Force LiveGraph explicitly (strict; serves even Partial/Stale — but still falls back when the
    /// partition is unavailable, the existing 1B behavior). NOT a strict-failure mode in this slice.
    LiveGraph,
    /// SQLite answer + a LiveGraph compare report + sidecar (diagnostic).
    Compare,
}

impl Engine {
    /// Parse the `engine` param. No param / unknown → `Auto` (QUERY-MIGRATION-CLI-1 default). Explicit
    /// `sqlite`/`livegraph`/`compare`/`auto` select that engine.
    pub fn parse(s: Option<&str>) -> Engine {
        match s {
            Some("sqlite") => Engine::Sqlite,
            Some("livegraph") => Engine::LiveGraph,
            Some("compare") => Engine::Compare,
            _ => Engine::Auto,
        }
    }
}

/// Why the `Auto` (or strict-LiveGraph) path fell back to SQLite (QUERY-MIGRATION-CLI-1). Surfaced in
/// the JSON `fallback_reason`; `null` when the served answer IS from LiveGraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// No LiveGraph for this repo / target (not preloaded/refreshed, or `Unavailable`).
    LiveGraphUnavailable,
    /// LiveGraph answered but not `Exact` (e.g. a non-resident contributing partition).
    LiveGraphPartial,
    /// LiveGraph answer is not `Fresh` (Stale / RefreshFailed / PrecisionPending — incl. producer-absent).
    LiveGraphStale,
    /// Contributing languages are not exclusively `TypeScriptPrimary` (D4 scope).
    LiveGraphUnsupportedLanguage,
    /// The LiveGraph answer could not be rendered into the response shape (reserved; not hit for
    /// callers/callees, whose keys always render).
    LiveGraphRenderUnsupported,
    /// A rendered `path` node lacks display metadata (`file:line` not recoverable from the resident IR),
    /// so the DEFAULT (`Auto`) path falls back to SQLite rather than render `:0` (PATH-LIVEGRAPH-DEFAULT-1).
    /// Distinct from `RenderUnsupported`: the answer is trust-complete, only its presentation is.
    LiveGraphDisplayMetadataUnavailable,
    /// The LiveGraph engine errored (reserved; the callers/callees query path does not error today).
    LiveGraphError,
    /// IMPORTS-LIVEGRAPH-DEFAULT-1 (D5): the per-call no-loss compare found a SQLite resolved-local import the
    /// LiveGraph edge set LOST (a regression) -> the DEFAULT falls back to SQLite (never a silent loss).
    LiveGraphImportRegression,
    /// IMPORTS-LIVEGRAPH-DEFAULT-1 (D5): the file has an AMBIGUOUS SQLite import (a FILE target that is neither
    /// resolved-local nor external) the harness cannot confidently bucket -> conservative SQLite fallback.
    LiveGraphImportUnknown,
    /// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1): the repo MODULE-cycle no-loss cert is NOT GREEN -- the compare
    /// found a SQLite cycle the LiveGraph lacks (missing) or an over-claimed extra -> the DEFAULT serves SQLite
    /// (never a silent cycle loss). EXPECTED for repo-graph (its excluded-fixture cycle) + non-TS repos.
    LiveGraphCycleDivergence,
    /// STATS-LIVEGRAPH-IMPL-1 (D1): the repo STATS no-loss cert is NOT GREEN -- the field-exact compare found a
    /// per-module divergence (a module only one side has, or a count/metric mismatch) -> the DEFAULT serves the
    /// SQLite `compute_module_stats` answer (never a silent wrong stat). EXPECTED where the SQLite MODULE-node
    /// identities do not correspond to the dirname aggregation (RISK-1) + non-TS repos.
    LiveGraphStatsDivergence,
    /// ORIENT-LIVEGRAPH-IMPL: the repo COMPLEXITY no-loss cert is NOT GREEN -- the field-exact compare found
    /// the LiveGraph repo-wide `high_complexity` set diverges from the SQLite `measurements` high-complexity
    /// set (a missing/extra symbol or a value mismatch) -> orient's HIGH_COMPLEXITY leaf serves the SQLite
    /// signal, labelled. EXPECTED where the LiveGraph value_facts do not mirror the durable measurements
    /// (different index epoch / a partition never preloaded) + non-TS repos.
    LiveGraphComplexityDivergence,
    /// ORIENT-LIVEGRAPH-IMPL: a symbol-focus CALLERS/CALLEES per-symbol no-loss key compare found the
    /// LiveGraph callgraph key set diverges from SQLite `find_symbol_callers`/`find_symbol_callees` -> orient's
    /// summary leaf serves the SQLite value, labelled. The value-equivalence proof that gates the `livegraph`
    /// label (never a bare relabel of a SQLite-built summary).
    LiveGraphCallgraphDivergence,
    /// COHERENCE-LEAF-SERVE-IMPL-1 (review-3 item 1): the BOUNDED orient (b)-leaf serve was DECLINED — the
    /// bounded orient cert (focus-resolution ∧ callgraph no-loss) was not GREEN at this fingerprint, so
    /// `handle_orient` ran the agent over the BARE SQLite storage (NOT the `OrientServeDecorator`). The
    /// CALLERS_SUMMARY/CALLEES_SUMMARY value is therefore SQLite-sourced THIS call; the leaf is labelled
    /// honestly and is NEVER re-certified `livegraph` from the callgraph cert state alone (the served-path
    /// provenance follows the ACTUAL serve decision, not a hypothetical cert peek). Distinct from
    /// `LiveGraphCallgraphDivergence` (the callgraph contributor itself may be GREEN here — a DIFFERENT
    /// bounded contributor, e.g. focus-resolution, was RED).
    LiveGraphBoundedServeDeclined,
}

impl FallbackReason {
    /// Stable string for the JSON `fallback_reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackReason::LiveGraphUnavailable => "LiveGraphUnavailable",
            FallbackReason::LiveGraphPartial => "LiveGraphPartial",
            FallbackReason::LiveGraphStale => "LiveGraphStale",
            FallbackReason::LiveGraphUnsupportedLanguage => "LiveGraphUnsupportedLanguage",
            FallbackReason::LiveGraphRenderUnsupported => "LiveGraphRenderUnsupported",
            FallbackReason::LiveGraphDisplayMetadataUnavailable => {
                "LiveGraphDisplayMetadataUnavailable"
            }
            FallbackReason::LiveGraphError => "LiveGraphError",
            FallbackReason::LiveGraphImportRegression => "LiveGraphImportRegression",
            FallbackReason::LiveGraphImportUnknown => "LiveGraphImportUnknown",
            FallbackReason::LiveGraphCycleDivergence => "LiveGraphCycleDivergence",
            FallbackReason::LiveGraphStatsDivergence => "LiveGraphStatsDivergence",
            FallbackReason::LiveGraphComplexityDivergence => "LiveGraphComplexityDivergence",
            FallbackReason::LiveGraphCallgraphDivergence => "LiveGraphCallgraphDivergence",
            FallbackReason::LiveGraphBoundedServeDeclined => "LiveGraphBoundedServeDeclined",
        }
    }
}

/// A LiveGraph callers/callees answer reduced to what the `Auto` decision needs.
struct LgAuto {
    class: AnswerClass,
    freshness: FreshnessState,
    /// True iff the contributing languages are non-empty and ALL `TypeScriptPrimary` (D4).
    ts_only: bool,
    keys: Vec<String>,
}

pub(crate) fn ts_only(langs: &std::collections::BTreeSet<LanguageSupport>) -> bool {
    !langs.is_empty()
        && langs
            .iter()
            .all(|l| matches!(l, LanguageSupport::TypeScriptPrimary))
}

/// The `Auto` decision (QUERY-MIGRATION-CLI-1 D3/D4): serve LiveGraph keys ONLY when Exact + Fresh +
/// TS-only; otherwise the labelled SQLite fallback. Freshness is checked before class so a Stale answer
/// reports `LiveGraphStale` (not `LiveGraphPartial`).
fn auto_outcome(lg: Option<LgAuto>) -> Result<Vec<String>, FallbackReason> {
    match lg {
        None => Err(FallbackReason::LiveGraphUnavailable),
        Some(a) => {
            if a.freshness != FreshnessState::Fresh {
                Err(FallbackReason::LiveGraphStale)
            } else if a.class != AnswerClass::Exact {
                Err(FallbackReason::LiveGraphPartial)
            } else if !a.ts_only {
                Err(FallbackReason::LiveGraphUnsupportedLanguage)
            } else {
                Ok(a.keys)
            }
        }
    }
}

/// Classified comparison report (S3): six buckets + raw counts. Not overfit — `identity_mismatch`
/// and `edge_basis_mismatch` are reserved (empty in 1B); they need a cross-key entity map / per-edge
/// basis alignment that 1B does not build.
#[derive(Debug, Serialize)]
pub struct CompareReport {
    pub symbol: String,
    pub kind: String,
    pub sqlite_count: usize,
    pub livegraph_count: usize,
    pub livegraph_class: String,
    pub missing_in_livegraph: Vec<String>,
    pub extra_in_livegraph: Vec<String>,
    pub identity_mismatch: Vec<String>,
    pub edge_basis_mismatch: Vec<String>,
    pub partition_unavailable: bool,
    pub trust_class_mismatch: Vec<String>,
}

/// Pure key-set comparison (S3). `lg = None` means LiveGraph could not answer (no preloaded
/// partition / `Unavailable`) → `partition_unavailable`.
pub fn compare_keys(
    symbol: &str,
    kind: &str,
    sqlite_keys: &[String],
    lg: Option<(AnswerClass, Vec<String>)>,
) -> CompareReport {
    use std::collections::BTreeSet;
    let sql: BTreeSet<&str> = sqlite_keys.iter().map(|s| s.as_str()).collect();
    match lg {
        None => CompareReport {
            symbol: symbol.to_string(),
            kind: kind.to_string(),
            sqlite_count: sqlite_keys.len(),
            livegraph_count: 0,
            livegraph_class: "Unavailable".to_string(),
            missing_in_livegraph: Vec::new(),
            extra_in_livegraph: Vec::new(),
            identity_mismatch: Vec::new(),
            edge_basis_mismatch: Vec::new(),
            partition_unavailable: true,
            trust_class_mismatch: Vec::new(),
        },
        Some((class, lg_keys)) => {
            let lgs: BTreeSet<&str> = lg_keys.iter().map(|s| s.as_str()).collect();
            let missing = sql.difference(&lgs).map(|s| s.to_string()).collect();
            let extra = lgs.difference(&sql).map(|s| s.to_string()).collect();
            let trust_class_mismatch = if class != AnswerClass::Exact {
                vec![format!("livegraph_class={class:?}")]
            } else {
                Vec::new()
            };
            CompareReport {
                symbol: symbol.to_string(),
                kind: kind.to_string(),
                sqlite_count: sqlite_keys.len(),
                livegraph_count: lg_keys.len(),
                livegraph_class: format!("{class:?}"),
                missing_in_livegraph: missing,
                extra_in_livegraph: extra,
                identity_mismatch: Vec::new(),
                edge_basis_mismatch: Vec::new(),
                partition_unavailable: false,
                trust_class_mismatch,
            }
        }
    }
}

/// LiveGraph caller keys for `target`, if usable (`class != Unavailable`). `None` = not usable
/// (no preloaded LiveGraph or target absent) → caller falls back to SQLite.
pub fn livegraph_caller_keys(
    repo_state: &RepoState,
    target: &str,
) -> Option<(AnswerClass, Vec<String>)> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    let env = lg.callers(target, Granularity::CallerDetail);
    if env.class() == AnswerClass::Unavailable {
        return None;
    }
    let keys = env
        .data()
        .map(|d| d.caller_identities.iter().map(|(_, k)| k.clone()).collect())
        .unwrap_or_default();
    Some((env.class(), keys))
}

/// LiveGraph callee keys for `target`, if usable.
pub fn livegraph_callee_keys(
    repo_state: &RepoState,
    target: &str,
) -> Option<(AnswerClass, Vec<String>)> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    let env = lg.callees(target, Granularity::CallerDetail);
    if env.class() == AnswerClass::Unavailable {
        return None;
    }
    let keys = env
        .data()
        .map(|d| d.callee_identities.iter().map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    Some((env.class(), keys))
}

/// LiveGraph `callers` answer reduced for the `Auto` decision (class + freshness + TS-only + keys).
/// `None` = not usable (`Unavailable` / no LiveGraph) → `Auto` falls back with `LiveGraphUnavailable`.
fn livegraph_callers_auto(repo_state: &RepoState, target: &str) -> Option<LgAuto> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    let env = lg.callers(target, Granularity::CallerDetail);
    if env.class() == AnswerClass::Unavailable {
        return None;
    }
    let keys = env
        .data()
        .map(|d| d.caller_identities.iter().map(|(_, k)| k.clone()).collect())
        .unwrap_or_default();
    Some(LgAuto {
        class: env.class(),
        freshness: env.freshness(),
        ts_only: ts_only(env.contributing_languages()),
        keys,
    })
}

/// LiveGraph `callees` answer reduced for the `Auto` decision (symmetric to [`livegraph_callers_auto`]).
fn livegraph_callees_auto(repo_state: &RepoState, target: &str) -> Option<LgAuto> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    let env = lg.callees(target, Granularity::CallerDetail);
    if env.class() == AnswerClass::Unavailable {
        return None;
    }
    let keys = env
        .data()
        .map(|d| d.callee_identities.iter().map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    Some(LgAuto {
        class: env.class(),
        freshness: env.freshness(),
        ts_only: ts_only(env.contributing_languages()),
        keys,
    })
}

fn caller_results_from_keys(keys: &[String]) -> Vec<CallerResult> {
    keys.iter()
        .map(|k| CallerResult {
            stable_key: k.clone(),
            name: k.clone(),
            qualified_name: None,
            kind: String::new(),
            subtype: None,
            // LiveGraph answers carry keys, not source locations; emit non-null placeholders the
            // presentation EdgeSymbol (file: String, line/column: u32) can parse.
            file: Some(String::new()),
            line: Some(0),
            column: Some(0),
            edge_type: "CALLS".to_string(),
            resolution: "livegraph".to_string(),
        })
        .collect()
}

fn callee_results_from_keys(keys: &[String]) -> Vec<CalleeResult> {
    keys.iter()
        .map(|k| CalleeResult {
            stable_key: k.clone(),
            name: k.clone(),
            qualified_name: None,
            kind: String::new(),
            subtype: None,
            // LiveGraph answers carry keys, not source locations; emit non-null placeholders the
            // presentation EdgeSymbol (file: String, line/column: u32) can parse.
            file: Some(String::new()),
            line: Some(0),
            column: Some(0),
            edge_type: "CALLS".to_string(),
            resolution: "livegraph".to_string(),
        })
        .collect()
}

/// Write the comparison report to a diagnostic sidecar `<repo_root>/.rgr/livegraph-compare/<ms>.json`
/// (S3). Best-effort: the caller must NOT fail the query on a sidecar error.
pub fn write_compare_sidecar(repo_root: &str, report: &CompareReport) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::path::Path::new(repo_root)
        .join(".rgr")
        .join("livegraph-compare");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create sidecar dir: {e}"))?;
    let path = dir.join(format!("{ts}.json"));
    let body =
        serde_json::to_string_pretty(report).map_err(|e| format!("serialize report: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write sidecar: {e}"))?;
    Ok(path.display().to_string())
}

/// `{target, callers, count, backend_used, fallback_reason}` (QUERY-MIGRATION-CLI-1). `fallback_reason`
/// is `null` iff `backend_used == "livegraph"`.
fn callers_value(
    target: &ResolvedSymbol,
    callers: Vec<CallerResult>,
    backend_used: &str,
    fallback_reason: Option<FallbackReason>,
) -> Value {
    let count = callers.len();
    json!({
        "target": target,
        "callers": callers,
        "count": count,
        "backend_used": backend_used,
        "fallback_reason": fallback_reason.map(|r| r.as_str()),
    })
}

/// Build the `callers` response for the selected engine (QUERY-MIGRATION-CLI-1). `Auto` serves
/// LiveGraph when Exact+Fresh+TS-only, else a labelled SQLite fallback. ALL responses carry
/// `backend_used` + `fallback_reason`; the human renderer ignores them (format unchanged), the
/// `--json` renderer surfaces them. `Compare` keeps the diagnostic sidecar (serves SQLite).
pub fn callers_engine_response(
    engine: Engine,
    repo_state: &RepoState,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, StorageError> {
    match engine {
        // Explicit SQLite escape hatch: ALWAYS read (the closure is called).
        Engine::Sqlite => callers_auto_or_sqlite(target, None, None, sqlite_fetch),
        // DEFAULT (QUERY-AUTO-LAZY-SQLITE-1): LiveGraph-first; SQLite read LAZILY only on fallback.
        Engine::Auto => {
            let (served, reason) =
                match auto_outcome(livegraph_callers_auto(repo_state, &target.stable_key)) {
                    Ok(keys) => (Some(keys), None),
                    Err(reason) => (None, Some(reason)),
                };
            callers_auto_or_sqlite(target, served, reason, sqlite_fetch)
        }
        // Explicit LiveGraph: serve LiveGraph; SQLite LAZILY only on the unavailable fallback (existing 1B
        // behavior; NOT a new strict-failure mode in this slice).
        Engine::LiveGraph => {
            let (served, reason) = match livegraph_caller_keys(repo_state, &target.stable_key) {
                Some((_class, keys)) => (Some(keys), None),
                None => (None, Some(FallbackReason::LiveGraphUnavailable)),
            };
            callers_auto_or_sqlite(target, served, reason, sqlite_fetch)
        }
        // Compare: ALWAYS reads SQLite (the served answer + the diagnostic compare report).
        Engine::Compare => {
            let sqlite_callers = sqlite_fetch()?;
            let sqlite_keys: Vec<String> = sqlite_callers
                .iter()
                .map(|c| c.stable_key.clone())
                .collect();
            let lg = livegraph_caller_keys(repo_state, &target.stable_key);
            let report = compare_keys(symbol, "callers", &sqlite_keys, lg);
            let sidecar = write_compare_sidecar(repo_root, &report).ok();
            // Compare deliberately SERVES the SQLite answer + the diagnostic report.
            let mut v = callers_value(target, sqlite_callers, "sqlite", None);
            v["livegraph_compare"] = serde_json::to_value(&report).unwrap_or(Value::Null);
            if let Some(p) = sidecar {
                v["livegraph_compare_sidecar"] = json!(p);
            }
            Ok(v)
        }
    }
}

/// QUERY-AUTO-LAZY-SQLITE-1: serve the LiveGraph keys (NO SQLite read) when `served` is `Some`, ELSE call
/// `sqlite_fetch` LAZILY (Engine::Sqlite -> `served=None, reason=None`; a fallback -> `served=None,
/// reason=Some`). The closure is STRUCTURALLY unreachable when `served` is `Some` -> the LiveGraph-served path
/// never touches `nodes`/`edges`. Pure (no RepoState) -> a panicking-closure unit test proves the served path
/// is lazy and the fallback path calls SQLite + propagates its error.
fn callers_auto_or_sqlite(
    target: &ResolvedSymbol,
    served: Option<Vec<String>>,
    fallback_reason: Option<FallbackReason>,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>,
) -> Result<Value, StorageError> {
    match served {
        Some(keys) => Ok(callers_value(
            target,
            caller_results_from_keys(&keys),
            "livegraph",
            None,
        )),
        None => Ok(callers_value(
            target,
            sqlite_fetch()?,
            "sqlite",
            fallback_reason,
        )),
    }
}

/// `{target, callees, count, backend_used, fallback_reason}` (QUERY-MIGRATION-CLI-1).
fn callees_value(
    target: &ResolvedSymbol,
    callees: Vec<CalleeResult>,
    backend_used: &str,
    fallback_reason: Option<FallbackReason>,
) -> Value {
    let count = callees.len();
    json!({
        "target": target,
        "callees": callees,
        "count": count,
        "backend_used": backend_used,
        "fallback_reason": fallback_reason.map(|r| r.as_str()),
    })
}

/// Build the `callees` response for the selected engine (symmetric to [`callers_engine_response`];
/// QUERY-MIGRATION-CLI-1).
pub fn callees_engine_response(
    engine: Engine,
    repo_state: &RepoState,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CalleeResult>, StorageError>,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, StorageError> {
    match engine {
        // Explicit SQLite escape hatch: ALWAYS read (the closure is called).
        Engine::Sqlite => callees_auto_or_sqlite(target, None, None, sqlite_fetch),
        // DEFAULT (QUERY-AUTO-LAZY-SQLITE-1): LiveGraph-first; SQLite read LAZILY only on fallback.
        Engine::Auto => {
            let (served, reason) =
                match auto_outcome(livegraph_callees_auto(repo_state, &target.stable_key)) {
                    Ok(keys) => (Some(keys), None),
                    Err(reason) => (None, Some(reason)),
                };
            callees_auto_or_sqlite(target, served, reason, sqlite_fetch)
        }
        // Explicit LiveGraph: serve LiveGraph; SQLite LAZILY only on the unavailable fallback.
        Engine::LiveGraph => {
            let (served, reason) = match livegraph_callee_keys(repo_state, &target.stable_key) {
                Some((_class, keys)) => (Some(keys), None),
                None => (None, Some(FallbackReason::LiveGraphUnavailable)),
            };
            callees_auto_or_sqlite(target, served, reason, sqlite_fetch)
        }
        // Compare: ALWAYS reads SQLite (the served answer + the diagnostic compare report).
        Engine::Compare => {
            let sqlite_callees = sqlite_fetch()?;
            let sqlite_keys: Vec<String> = sqlite_callees
                .iter()
                .map(|c| c.stable_key.clone())
                .collect();
            let lg = livegraph_callee_keys(repo_state, &target.stable_key);
            let report = compare_keys(symbol, "callees", &sqlite_keys, lg);
            let sidecar = write_compare_sidecar(repo_root, &report).ok();
            let mut v = callees_value(target, sqlite_callees, "sqlite", None);
            v["livegraph_compare"] = serde_json::to_value(&report).unwrap_or(Value::Null);
            if let Some(p) = sidecar {
                v["livegraph_compare_sidecar"] = json!(p);
            }
            Ok(v)
        }
    }
}

/// QUERY-AUTO-LAZY-SQLITE-1: callees analogue of [`callers_auto_or_sqlite`]. Serve LiveGraph keys (no SQLite
/// read) when `served` is `Some`; else call `sqlite_fetch` LAZILY. The closure is structurally unreachable on
/// the served path.
fn callees_auto_or_sqlite(
    target: &ResolvedSymbol,
    served: Option<Vec<String>>,
    fallback_reason: Option<FallbackReason>,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CalleeResult>, StorageError>,
) -> Result<Value, StorageError> {
    match served {
        Some(keys) => Ok(callees_value(
            target,
            callee_results_from_keys(&keys),
            "livegraph",
            None,
        )),
        None => Ok(callees_value(
            target,
            sqlite_fetch()?,
            "sqlite",
            fallback_reason,
        )),
    }
}

// ── PATH-CYCLES-LIVEGRAPH-1: path engine ──

/// Extract the symbol NAME from a canonical key `repo:file#name:KIND:SUBTYPE` → `name` (for comparing
/// across the SQLite/LiveGraph key spaces). Falls back to the whole key if there is no `#`.
fn key_name(key: &str) -> String {
    match key.find('#') {
        Some(h) => {
            let after = &key[h + 1..];
            after.split(':').next().unwrap_or(after).to_string()
        }
        None => key.to_string(),
    }
}

/// A rendered path node: its stable `key` plus the DISPLAY metadata (`file`, 1-based `line`) recovered
/// from the resident IR. `location == None` means the node carries no recoverable range — the DEFAULT
/// path then falls back to SQLite rather than render `:0` (PATH-LIVEGRAPH-DEFAULT-1).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathNodeDisplay {
    key: String,
    location: Option<(String, u32)>,
}

/// A LiveGraph path answer reduced to what the `Auto` decision needs (PATH-LIVEGRAPH-DEFAULT-1).
struct LgPathAuto {
    class: AnswerClass,
    freshness: FreshnessState,
    /// Contributing languages are non-empty and ALL `TypeScriptPrimary` (D2 TS-only).
    ts_only: bool,
    /// Whether a path was found (vs a no-path result).
    found: bool,
    /// Path nodes with display metadata (key + resolved `file:line`).
    nodes: Vec<PathNodeDisplay>,
}

/// LiveGraph path for `(from_key, to_key)`, if the LiveGraph can answer. `None` = no LiveGraph for this
/// repo. Carries class + freshness + TS-only + found + per-node display metadata. The `file:line` is
/// resolved here (under the read guard) via the read-only `node_location` lookup over the resident IR —
/// it does NOT affect path()/trust semantics, only presentation.
fn livegraph_path(repo_state: &RepoState, from_key: &str, to_key: &str) -> Option<LgPathAuto> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    let env = lg.path(from_key, to_key);
    let keys = env.data().map(|d| d.nodes.clone()).unwrap_or_default();
    let found = !keys.is_empty();
    let nodes = keys
        .into_iter()
        .map(|k| {
            let location = lg
                .node_location(&CanonicalKey::from_existing(k.clone()))
                .map(|r| (r.file, r.start_line));
            PathNodeDisplay { key: k, location }
        })
        .collect();
    Some(LgPathAuto {
        class: env.class(),
        freshness: env.freshness(),
        ts_only: ts_only(env.contributing_languages()),
        found,
        nodes,
    })
}

/// The `Auto` path decision (PATH-LIVEGRAPH-DEFAULT-1 D2/D3 + the display-metadata invariant): serve
/// LiveGraph ONLY when Exact + Fresh + TS-only AND every rendered node has `file:line`. Freshness is
/// checked before class so a Stale answer reports `LiveGraphStale`. The D3 no-path rule needs NO special
/// case: `path()` returns `Exact` only for a proven-complete result (a found path OR a proven no-path)
/// and `Partial` for an incomplete traversal — so an Exact no-path is served and a Partial no-path falls
/// back. A no-path serves with no nodes to render (the display gate is vacuous). Returns `Ok((found,
/// nodes))` to serve, or `Err(reason)` to fall back to SQLite.
fn path_auto_outcome(
    lg: Option<LgPathAuto>,
) -> Result<(bool, Vec<PathNodeDisplay>), FallbackReason> {
    match lg {
        None => Err(FallbackReason::LiveGraphUnavailable),
        Some(a) => {
            if a.freshness != FreshnessState::Fresh {
                Err(FallbackReason::LiveGraphStale)
            } else if a.class != AnswerClass::Exact {
                Err(FallbackReason::LiveGraphPartial)
            } else if !a.ts_only {
                Err(FallbackReason::LiveGraphUnsupportedLanguage)
            } else if a.nodes.iter().any(|n| n.location.is_none()) {
                // Trust-complete but a rendered node lacks file:line — never render `:0` by default.
                Err(FallbackReason::LiveGraphDisplayMetadataUnavailable)
            } else {
                Ok((a.found, a.nodes))
            }
        }
    }
}

/// Render LiveGraph path nodes into the `{found, path_length, path:[step]}` shape the CLI `PathResponse`
/// renders. `node_id` keeps the full stable key (identity); `symbol` is the clean NAME (so the human
/// renderer shows `report`, not the full key); `file`/`line` are the resolved display metadata. A node
/// with no location renders `file:""`/`line:0` — only reachable via explicit `--engine livegraph`, since
/// the DEFAULT `Auto` gates on all-present (PATH-LIVEGRAPH-DEFAULT-1).
fn livegraph_path_result(found: bool, nodes: &[PathNodeDisplay]) -> Value {
    let steps: Vec<Value> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (file, line) = n.location.clone().unwrap_or_else(|| (String::new(), 0));
            json!({
                "node_id": n.key,
                "symbol": key_name(&n.key),
                "file": file,
                "line": line,
                "edge_type": if i == 0 { "" } else { "CALLS" },
            })
        })
        .collect();
    json!({
        "found": found,
        "path_length": nodes.len().saturating_sub(1),
        "path": steps,
    })
}

/// Classified path comparison (PATH-CYCLES-LIVEGRAPH-1). Path-specific buckets — an "SQLite includes
/// IMPORTS" difference is `DifferentPath`/`PathOnlyInSqlite`, NOT a generic failure.
#[derive(Debug, Serialize)]
pub struct PathCompareReport {
    pub from: String,
    pub to: String,
    pub sqlite_found: bool,
    pub sqlite_path: Vec<String>,
    pub livegraph_found: bool,
    pub livegraph_class: String,
    pub livegraph_path: Vec<String>,
    pub partition_unavailable: bool,
    /// Classified buckets: PathOnlyInSqlite / PathOnlyInLiveGraph / DifferentPath / LiveGraphPartial /
    /// PartitionUnavailable / TrustClassMismatch. Empty = the two backends agree.
    pub buckets: Vec<String>,
}

fn compare_path(
    from: &str,
    to: &str,
    sqlite_found: bool,
    sqlite_path: Vec<String>,
    lg: Option<(AnswerClass, FreshnessState, bool, Vec<String>)>,
) -> PathCompareReport {
    let mut buckets = Vec::new();
    match lg {
        None => {
            buckets.push("PartitionUnavailable".to_string());
            PathCompareReport {
                from: from.to_string(),
                to: to.to_string(),
                sqlite_found,
                sqlite_path,
                livegraph_found: false,
                livegraph_class: "Unavailable".to_string(),
                livegraph_path: Vec::new(),
                partition_unavailable: true,
                buckets,
            }
        }
        Some((class, _freshness, lg_found, lg_path)) => {
            if class == AnswerClass::Partial {
                buckets.push("LiveGraphPartial".to_string());
            } else if class != AnswerClass::Exact {
                buckets.push("TrustClassMismatch".to_string());
            }
            if sqlite_found && !lg_found {
                buckets.push("PathOnlyInSqlite".to_string());
            } else if !sqlite_found && lg_found {
                buckets.push("PathOnlyInLiveGraph".to_string());
            } else if sqlite_found && lg_found && sqlite_path != lg_path {
                buckets.push("DifferentPath".to_string());
            }
            PathCompareReport {
                from: from.to_string(),
                to: to.to_string(),
                sqlite_found,
                sqlite_path,
                livegraph_found: lg_found,
                livegraph_class: format!("{class:?}"),
                livegraph_path: lg_path,
                partition_unavailable: false,
                buckets,
            }
        }
    }
}

/// Build the `path` response for the selected engine (PATH-CYCLES-LIVEGRAPH-1). `Sqlite`/`Auto` serve
/// the (unchanged) SQLite path — path does NOT auto-migrate this slice. `LiveGraph` serves the
/// LiveGraph BFS path (or falls back to SQLite when no LiveGraph). `Compare` serves SQLite + a path
/// compare report + sidecar. All responses carry `backend_used`; the human render is unaffected.
#[allow(clippy::too_many_arguments)]
pub fn path_engine_response(
    engine: Engine,
    repo_state: &RepoState,
    from_key: &str,
    to_key: &str,
    repo_uid: &str,
    snapshot_uid: &str,
    sqlite_fetch: impl FnOnce() -> Result<Value, StorageError>,
    repo_root: &str,
) -> Result<Value, StorageError> {
    match engine {
        // `--engine sqlite` FORCES SQLite (always reads).
        Engine::Sqlite => path_auto_or_sqlite(repo_uid, snapshot_uid, None, None, sqlite_fetch),
        // DEFAULT (PATH-LIVEGRAPH-DEFAULT-1 + QUERY-AUTO-LAZY-SQLITE-1): serve LiveGraph when Exact + Fresh +
        // TS-only (D3 no-path EXACT is served -- path() returns Exact only for a proven-complete found-OR-no-
        // path); ELSE a LAZY labelled SQLite fallback. The served path does NOT read nodes/edges.
        Engine::Auto => {
            let (served, reason) =
                match path_auto_outcome(livegraph_path(repo_state, from_key, to_key)) {
                    Ok((found, nodes)) => (Some((found, nodes)), None),
                    Err(reason) => (None, Some(reason)),
                };
            path_auto_or_sqlite(repo_uid, snapshot_uid, served, reason, sqlite_fetch)
        }
        // Explicit LiveGraph keeps trust_class/freshness as a diagnostic surface; SQLite LAZILY on fallback.
        Engine::LiveGraph => match livegraph_path(repo_state, from_key, to_key) {
            Some(a) => Ok(json!({
                "repo_uid": repo_uid,
                "snapshot_uid": snapshot_uid,
                "found": a.found,
                "path": livegraph_path_result(a.found, &a.nodes),
                "backend_used": "livegraph",
                "fallback_reason": Value::Null,
                "trust_class": format!("{:?}", a.class),
                "freshness": format!("{:?}", a.freshness),
            })),
            None => path_auto_or_sqlite(
                repo_uid,
                snapshot_uid,
                None,
                Some(FallbackReason::LiveGraphUnavailable),
                sqlite_fetch,
            ),
        },
        // Compare: ALWAYS reads SQLite. The sqlite_found/sqlite_names extraction (compare-only) moves HERE.
        Engine::Compare => {
            let mut sqlite_response = sqlite_fetch()?;
            let sqlite_found = sqlite_response
                .pointer("/path/found")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // Compare by SYMBOL NAME (SQLite node_ids are DB UUIDs; LiveGraph keys are stable keys) so a
            // representation difference is not mistaken for a real path difference.
            let sqlite_names: Vec<String> = sqlite_response
                .pointer("/path/path")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.get("symbol").and_then(|v| v.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let lg = livegraph_path(repo_state, from_key, to_key).map(|a| {
                (
                    a.class,
                    a.freshness,
                    a.found,
                    a.nodes.iter().map(|n| key_name(&n.key)).collect::<Vec<_>>(),
                )
            });
            let report = compare_path(
                &key_name(from_key),
                &key_name(to_key),
                sqlite_found,
                sqlite_names,
                lg,
            );
            let sidecar = write_path_compare_sidecar(repo_root, &report).ok();
            sqlite_response["backend_used"] = json!("sqlite");
            sqlite_response["fallback_reason"] = Value::Null;
            sqlite_response["livegraph_path_compare"] =
                serde_json::to_value(&report).unwrap_or(Value::Null);
            if let Some(p) = sidecar {
                sqlite_response["livegraph_path_compare_sidecar"] = json!(p);
            }
            Ok(sqlite_response)
        }
    }
}

/// QUERY-AUTO-LAZY-SQLITE-1: serve the LiveGraph path (NO SQLite read) when `served` is `Some`, ELSE call
/// `sqlite_fetch` LAZILY and stamp backend_used/fallback_reason. The closure is structurally unreachable on the
/// served path -> a served `path` (incl. a no-path EXACT) does not read nodes/edges.
fn path_auto_or_sqlite(
    repo_uid: &str,
    snapshot_uid: &str,
    served: Option<(bool, Vec<PathNodeDisplay>)>,
    fallback_reason: Option<FallbackReason>,
    sqlite_fetch: impl FnOnce() -> Result<Value, StorageError>,
) -> Result<Value, StorageError> {
    match served {
        Some((found, nodes)) => Ok(json!({
            "repo_uid": repo_uid,
            "snapshot_uid": snapshot_uid,
            "found": found,
            "path": livegraph_path_result(found, &nodes),
            "backend_used": "livegraph",
            "fallback_reason": Value::Null,
        })),
        None => {
            let mut sqlite_response = sqlite_fetch()?;
            sqlite_response["backend_used"] = json!("sqlite");
            sqlite_response["fallback_reason"] =
                fallback_reason.map_or(Value::Null, |r| json!(r.as_str()));
            Ok(sqlite_response)
        }
    }
}

/// Write a path compare report to `<repo_root>/.rgr/livegraph-compare/path-<ms>.json` (best-effort).
fn write_path_compare_sidecar(
    repo_root: &str,
    report: &PathCompareReport,
) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::path::Path::new(repo_root)
        .join(".rgr")
        .join("livegraph-compare");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create sidecar dir: {e}"))?;
    let path = dir.join(format!("path-{ts}.json"));
    let body =
        serde_json::to_string_pretty(report).map_err(|e| format!("serialize report: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write sidecar: {e}"))?;
    Ok(path.display().to_string())
}

/// Display a file-scope FILE node key (`{repo}:{path}:FILE`) as its partition-relative path
/// (`{path}`) for human cycle output. Falls back to the raw key if it has no `:FILE`/`:` shape.
fn file_display(key: &str) -> String {
    let no_file = key.strip_suffix(":FILE").unwrap_or(key);
    match no_file.find(':') {
        Some(i) => no_file[i + 1..].to_string(),
        None => no_file.to_string(),
    }
}

/// Map a [`FileImportCyclesAnswer`] into the `CyclesResponse` `cycles:[{nodes:[{node_id,name,file}]}]`
/// shape. `node_id` keeps the full FILE key; `name` is the partition-relative path; `file` is null
/// (the cycle node IS a file).
fn file_import_cycles_json(answer: &FileImportCyclesAnswer) -> Vec<Value> {
    answer
        .cycles
        .iter()
        .map(|c| {
            let nodes: Vec<Value> = c
                .members
                .iter()
                .map(|m| {
                    json!({
                        "node_id": m,
                        "name": file_display(m),
                        "file": Value::Null,
                    })
                })
                .collect();
            json!({ "nodes": nodes })
        })
        .collect()
}

/// Emit the D5 [`ImportCycleScope`] flag set as a STRUCTURED JSON object (IMPORTS-XPART-ENUMERATION-1 D6).
/// The JSON ALWAYS carries the structured fields; a human renderer may stringify from them. This replaces
/// the prior hard-coded scope string, which would under-report a multi-partition answer.
fn scope_json(answer: &FileImportCyclesAnswer) -> Value {
    let s = answer.scope;
    json!({
        "captured_resolved_relative": s.captured_resolved_relative,
        "intra_partition": s.intra_partition,
        "cross_partition": s.cross_partition,
        "xpart_edge_count": s.xpart_edge_count,
    })
}

/// Scope for an Unavailable answer (no resident LiveGraph): the query FAMILY is still captured
/// resolved-relative; nothing contributed (the Unavailable class carries the absence).
fn default_scope_json() -> Value {
    json!({
        "captured_resolved_relative": true,
        "intra_partition": false,
        "cross_partition": false,
        "xpart_edge_count": 0,
    })
}

/// CYCLES-LIVEGRAPH-CLI-1: build the `--engine livegraph --kind file-import` cycles response. Calls the
/// headless `file_import_cycles()` and maps it into the cycles shape + trust metadata. NO SQLite fallback
/// (D7): the trust class/scope are surfaced; the answer never silently becomes the SQLite MODULE graph.
pub fn file_import_cycles_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Value {
    let guard = repo_state.livegraph.read();
    let (class, freshness, missing, reasons, cycles, scope) = match guard.as_ref() {
        Some(lg) => {
            let env = lg.file_import_cycles();
            let data = env.data();
            let cycles = data.map(file_import_cycles_json).unwrap_or_default();
            // IMPORTS-XPART-ENUMERATION-1 (D6): emit the STRUCTURED D5 scope flag set (not the old
            // hard-coded string), so a multi-partition answer honestly reports cross_partition.
            let scope = data.map(scope_json).unwrap_or_else(default_scope_json);
            (
                format!("{:?}", env.class()),
                format!("{:?}", env.freshness()),
                env.missing_partitions().to_vec(),
                env.degradation_reasons()
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>(),
                cycles,
                scope,
            )
        }
        // No LiveGraph for this repo (never preloaded/refreshed) -> Unavailable, NOT a SQLite fallback.
        None => (
            "Unavailable".to_string(),
            "Unavailable".to_string(),
            Vec::new(),
            vec!["LiveGraphUnavailable".to_string()],
            Vec::new(),
            default_scope_json(),
        ),
    };
    let count = cycles.len();
    json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": cycles,
        "count": count,
        "backend_used": "livegraph",
        "kind": "file-import",
        "scope": scope,
        "answer_class": class,
        "freshness": freshness,
        "missing_partitions": missing,
        "degradation_reasons": reasons,
    })
}

/// Map a [`ModuleImportCyclesAnswer`] into the `cycles:[{nodes:[{node_id,name,file}]}]` shape
/// (MODULE-CYCLES-CLI-1 D2). The member `name` is the MODULE PATH (e.g. `packages/a/src`), NOT a short
/// name — so the human + compare are unambiguous (SQLite's short-name `src`/`src` collision is what we avoid).
fn module_import_cycles_json(answer: &ModuleImportCyclesAnswer) -> Vec<Value> {
    // CYCLES-OUTPUT-CONTRACT-1 (D1=B/D2=B, step 3): the LiveGraph module cycles share the SAME canonical,
    // qualified, deterministically-ordered output as the SQLite default (`cycle_output`), so the two render
    // byte-identically for the same cycle SET (the precondition for the deferred cycles fastpath). Members ARE
    // the qualified dirname module identities; the adapter sets node_id = qualified_name = member, name =
    // basename(member).
    let cycles: Vec<Vec<String>> = answer.cycles.iter().map(|c| c.members.clone()).collect();
    crate::cycle_output::livegraph_module_cycles_json(&cycles)
}

/// Emit the MODULE-import scope (MODULE-CYCLES-CLI-1 D2): the aggregated FILE scope flags + the
/// directory-aggregation markers (`module_aggregated`, `aggregation_basis="dirname"`).
fn module_scope_json(answer: &ModuleImportCyclesAnswer) -> Value {
    let s = answer.scope;
    json!({
        "captured_resolved_relative": s.file_scope.captured_resolved_relative,
        "intra_partition": s.file_scope.intra_partition,
        "cross_partition": s.file_scope.cross_partition,
        "xpart_edge_count": s.file_scope.xpart_edge_count,
        "module_aggregated": s.module_aggregated,
        "aggregation_basis": "dirname",
    })
}

/// MODULE-import scope for an Unavailable answer (no resident LiveGraph).
fn default_module_scope_json() -> Value {
    json!({
        "captured_resolved_relative": true,
        "intra_partition": false,
        "cross_partition": false,
        "xpart_edge_count": 0,
        "module_aggregated": true,
        "aggregation_basis": "dirname",
    })
}

/// MODULE-CYCLES-CLI-1 (D2): build the `--engine livegraph --kind module-import` response. Mirrors
/// [`file_import_cycles_response`] but over [`repo_graph_livegraph::LiveGraph::module_import_cycles`] — the
/// directory-aggregated MODULE cycle answer. NO SQLite fallback; the trust class/scope are surfaced.
pub fn module_import_cycles_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Value {
    let guard = repo_state.livegraph.read();
    let (class, freshness, missing, reasons, cycles, scope) = match guard.as_ref() {
        Some(lg) => {
            let env = lg.module_import_cycles();
            let data = env.data();
            let cycles = data.map(module_import_cycles_json).unwrap_or_default();
            let scope = data
                .map(module_scope_json)
                .unwrap_or_else(default_module_scope_json);
            (
                format!("{:?}", env.class()),
                format!("{:?}", env.freshness()),
                env.missing_partitions().to_vec(),
                env.degradation_reasons()
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>(),
                cycles,
                scope,
            )
        }
        None => (
            "Unavailable".to_string(),
            "Unavailable".to_string(),
            Vec::new(),
            vec!["LiveGraphUnavailable".to_string()],
            Vec::new(),
            default_module_scope_json(),
        ),
    };
    let count = cycles.len();
    json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": cycles,
        "count": count,
        "backend_used": "livegraph",
        "kind": "module-import",
        "scope": scope,
        "answer_class": class,
        "freshness": freshness,
        "missing_partitions": missing,
        "degradation_reasons": reasons,
    })
}

/// IMPORTS-LIVEGRAPH-CLI-1 (D2/D4/D5): build the `imports --engine livegraph` response -- the
/// captured/classified import READ-MODEL (EDGES = graph facts; OBSERVATIONS = completeness evidence,
/// SEPARATED) over [`repo_graph_livegraph::LiveGraph::live_import_view`]. The module-cycle trust signals are
/// named EXPLICITLY after their SOURCE (the module-cycle certificate / answer), NEVER a generic
/// import-listing-completeness claim (the ratified wording correction): `module_cycle_completeness` (the
/// certificate verdict), `module_cycle_answer_class` (the module-cycle answer class), `module_cycle_import_
/// scope` (the captured-graph universe). D6: `file_filter` `None` -> repo-wide. NO SQLite fallback; NO default
/// migration. The module-cycle baseline is assembled BEFORE the LiveGraph read lock (no re-entrant locking).
pub fn imports_view_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    repo_root: &str,
    include_fixtures: bool,
    file_filter: Option<&str>,
) -> Value {
    use repo_graph_livegraph::module_cycle_cert::evaluate_module_cycle_completeness;
    // Assemble the module-cycle baseline BEFORE taking the LiveGraph lock (no re-entrant locking). Best-effort:
    // a SQLite language-inventory error -> None -> the certificate reads `UnknownBaselineMissing` (honest, not
    // a false completeness claim). REUSES the audit's `build_baseline` (the single policy-versioned assembly).
    let baseline = repo_state
        .storage
        .distinct_file_languages(repo_uid)
        .ok()
        .map(|languages| {
            let discovered =
                crate::partition_discovery::discover_partition_roots(repo_root, include_fixtures);
            let expected: std::collections::BTreeSet<String> = discovered
                .included
                .iter()
                .map(|sr| crate::livegraph_refresh::derive_partition_target(repo_root, sr).1)
                .collect();
            crate::cycle_completeness_audit::build_baseline(expected, &languages, snapshot_uid).0
        });

    let guard = repo_state.livegraph.read();
    let body = match guard.as_ref() {
        Some(lg) => {
            let snapshot = lg.module_cycle_live_state();
            let certificate = evaluate_module_cycle_completeness(&snapshot, baseline.as_ref());
            let view = lg.live_import_view(file_filter);
            let env = lg.module_import_cycles();
            let scope = env
                .data()
                .map(module_scope_json)
                .unwrap_or_else(default_module_scope_json);
            import_view_body(
                &view,
                certificate.as_str(),
                &format!("{:?}", env.class()),
                scope,
                &format!("{:?}", env.freshness()),
                env.missing_partitions().to_vec(),
                env.degradation_reasons()
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect(),
            )
        }
        None => import_view_body_unavailable(),
    };
    drop(guard);
    // The common header; the `body` (data + trust) is spliced in (avoids a large tuple + header duplication).
    let mut out = json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "backend_used": "livegraph",
        "engine": "livegraph",
        "file_filter": file_filter,
        "scope_is_repo_wide": file_filter.is_none(),
    });
    if let (Value::Object(out_map), Value::Object(body_map)) = (&mut out, body) {
        for (k, v) in body_map {
            out_map.insert(k, v);
        }
    }
    out
}

/// IMPORTS-LIVEGRAPH-CLI-1 (D2/D4/D5): PURE JSON assembly of the import read-model body -- the SEPARATED
/// EDGES (facts) + OBSERVATIONS (evidence) + per-class counts + the module-cycle trust fields NAMED after
/// their source (`module_cycle_completeness` / `module_cycle_answer_class` / `module_cycle_import_scope`),
/// NEVER a bare `answer_class` / `completeness` that would imply the import LISTING is complete. No
/// RepoState / no lock -> unit-testable against a hand-built view (the trust-naming invariant is asserted).
fn import_view_body(
    view: &repo_graph_livegraph::import_view::LiveImportView,
    module_cycle_completeness: &str,
    module_cycle_answer_class: &str,
    module_cycle_import_scope: Value,
    freshness: &str,
    missing_partitions: Vec<String>,
    degradation_reasons: Vec<String>,
) -> Value {
    let mut class_counts: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    let mut blocking_observations = 0usize;
    for o in &view.observations {
        *class_counts.entry(o.class.as_str()).or_default() += 1;
        if o.blocking {
            blocking_observations += 1;
        }
    }
    let observation_class_counts: serde_json::Map<String, Value> = class_counts
        .iter()
        .map(|(k, v)| ((*k).to_string(), json!(v)))
        .collect();
    let edges: Vec<Value> = view
        .edges
        .iter()
        .map(|e| {
            json!({
                "src_file": e.src_file,
                "dst_file": e.dst_file,
                "basis": e.basis,
                "raw_specifier": e.raw_specifier,
            })
        })
        .collect();
    let observations: Vec<Value> = view
        .observations
        .iter()
        .map(|o| {
            json!({
                "source_file": o.source_file,
                "raw_specifier": o.raw_specifier,
                "class": o.class,
                "blocking": o.blocking,
            })
        })
        .collect();
    json!({
        "edges": edges,
        "edge_count": edges.len(),
        "observations": observations,
        "observation_count": observations.len(),
        "blocking_observation_count": blocking_observations,
        "observation_class_counts": observation_class_counts,
        "module_cycle_completeness": module_cycle_completeness,
        "module_cycle_answer_class": module_cycle_answer_class,
        "module_cycle_import_scope": module_cycle_import_scope,
        "freshness": freshness,
        "missing_partitions": missing_partitions,
        "degradation_reasons": degradation_reasons,
    })
}

/// The import read-model body when the LiveGraph is not resident (no fallback, D-no-SQLite): empty data +
/// an explicit Unavailable trust posture (NEVER a faked completeness).
fn import_view_body_unavailable() -> Value {
    import_view_body(
        &repo_graph_livegraph::import_view::LiveImportView::default(),
        "Unknown",
        "Unavailable",
        default_module_scope_json(),
        "Unavailable",
        Vec::new(),
        vec!["LiveGraphUnavailable".to_string()],
    )
}

/// IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 / -REPOWIDE-1 (D2/D4): the SHARED per-file directional verdict. The
/// precondition GATES -- an unmet precondition is a FALLBACK, NEVER a regression (a non-TS / non-resident file
/// legitimately has no LiveGraph data: the language gate, not a loss). Reused by the per-file sidecar AND the
/// repo-wide aggregate so the two cannot diverge.
fn directional_status(precondition_met: bool, has_missing: bool, has_extra: bool) -> &'static str {
    if !precondition_met {
        "FallbackPreconditionUnmet"
    } else if has_missing {
        "Regression"
    } else if has_extra {
        "NoLossLivegraphSuperset"
    } else {
        "NoLossEquivalent"
    }
}

/// IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (D2/D6): the PURE per-file directional-compare sidecar. EDGE
/// EQUIVALENCE is DIRECTIONAL no-loss -- every SQLite resolved-local target must appear in the LiveGraph edge
/// targets (a LiveGraph edge SQLite lacks is an IMPROVEMENT, not a failure). The D3 PRECONDITION (resident +
/// Fresh + TS-primary) gates the verdict: unmet -> `FallbackPreconditionUnmet` (SQLite is the sole source);
/// met + a missing SQLite import -> `Regression` (the dangerous case the measurement hunts); met + no loss ->
/// `NoLossEquivalent` / `NoLossLivegraphSuperset`. No RepoState / no lock -> unit-testable.
fn imports_compare_sidecar(
    sqlite_resolved_targets: &[String],
    view: &repo_graph_livegraph::import_view::LiveImportView,
    precondition: Option<&repo_graph_livegraph::import_view::FilePartitionStatus>,
) -> Value {
    let lg_edge_targets: std::collections::BTreeSet<&str> =
        view.edges.iter().map(|e| e.dst_file.as_str()).collect();
    let sqlite_set: std::collections::BTreeSet<&str> =
        sqlite_resolved_targets.iter().map(|s| s.as_str()).collect();
    let matched: Vec<&str> = sqlite_set
        .iter()
        .filter(|t| lg_edge_targets.contains(*t))
        .copied()
        .collect();
    let missing_in_livegraph: Vec<&str> = sqlite_set
        .iter()
        .filter(|t| !lg_edge_targets.contains(*t))
        .copied()
        .collect();
    let extra_livegraph_edges: Vec<Value> = view
        .edges
        .iter()
        .filter(|e| !sqlite_set.contains(e.dst_file.as_str()))
        .map(|e| {
            json!({
                "dst_file": e.dst_file,
                "basis": e.basis,
                "raw_specifier": e.raw_specifier,
            })
        })
        .collect();
    let blocking_observations: Vec<Value> = view
        .observations
        .iter()
        .filter(|o| o.blocking)
        .map(|o| {
            json!({
                "source_file": o.source_file,
                "raw_specifier": o.raw_specifier,
                "class": o.class,
            })
        })
        .collect();
    let precondition_met = precondition.is_some_and(|p| p.precondition_met());
    let status = directional_status(
        precondition_met,
        !missing_in_livegraph.is_empty(),
        !extra_livegraph_edges.is_empty(),
    );
    json!({
        "status": status,
        "precondition": precondition.map(|p| {
            json!({
                "partition": p.partition_id,
                "resident": p.resident,
                "fresh": p.fresh,
                "ts_primary": p.ts_primary,
                "precondition_met": p.precondition_met(),
            })
        }),
        "matched": matched,
        "missing_in_livegraph": missing_in_livegraph,
        "extra_livegraph_edges": extra_livegraph_edges,
        "blocking_observations": blocking_observations,
        "sqlite_resolved_local_count": sqlite_set.len(),
        "livegraph_edge_count": view.edges.len(),
    })
}

/// IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (D6): build the `imports --engine compare <file>` response. SQLite is
/// PRIMARY (the existing `{file, imports, count}`, byte-compatible -- D5); the directional compare rides as a
/// `comparison` SIDECAR. READ-ONLY; NO default change; NO fallback flip (this MEASURES the per-file gate). The
/// LiveGraph view + the D3 precondition are read under ONE lock.
pub fn imports_compare_response(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
    file_path: &str,
) -> Value {
    // SQLite PRIMARY (the existing per-file listing -- unchanged shape).
    let file_stable_key = format!("{repo_uid}:{file_path}:FILE");
    let imports = repo_state
        .storage
        .find_imports(snapshot_uid, &file_stable_key)
        .unwrap_or_default();
    // The SQLite RESOLVED-LOCAL subset (D2): kind=FILE, NOT external, resolution=static, a target file path.
    let sqlite_resolved_targets: Vec<String> = imports
        .iter()
        .filter(|r| {
            r.kind == "FILE"
                && r.subtype.as_deref() != Some("EXTERNAL")
                && r.resolution.as_deref() == Some("static")
                && !r.file.is_empty()
        })
        .map(|r| r.file.clone())
        .collect();
    // LiveGraph view + the D3 precondition (ONE lock).
    let guard = repo_state.livegraph.read();
    let (view, precondition) = match guard.as_ref() {
        Some(lg) => (
            lg.live_import_view(Some(file_path)),
            lg.file_partition_status(file_path),
        ),
        None => (
            repo_graph_livegraph::import_view::LiveImportView::default(),
            None,
        ),
    };
    drop(guard);
    let comparison =
        imports_compare_sidecar(&sqlite_resolved_targets, &view, precondition.as_ref());
    json!({
        "file": file_path,
        "imports": imports,
        "count": imports.len(),
        "backend_used": "sqlite",
        "engine": "compare",
        "comparison": comparison,
    })
}

/// IMPORTS-LIVEGRAPH-DEFAULT-1 (D3/D4): map a captured LiveGraph edge into the existing SQLite `ImportEntry`
/// JSON shape so the human renderer + JSON consumers stay byte-compatible. `evidence` carries the edge basis.
fn edge_to_import_entry(e: &repo_graph_livegraph::import_view::ImportEdgeView) -> Value {
    json!({
        "node_id": "",
        "symbol": e.dst_file,
        "kind": "FILE",
        "subtype": "SOURCE",
        "file": e.dst_file,
        "line": 0,
        "column": 0,
        "edge_type": "IMPORTS",
        "resolution": "static",
        "evidence": [e.basis],
        "depth": 1,
    })
}

/// IMPORTS-LIVEGRAPH-DEFAULT-1 (D5): the precondition-failure fallback reason (called only when the precondition
/// is NOT met). None -> no resident TS partition owns the file (`LiveGraphUnavailable`); a stale partition ->
/// `LiveGraphStale`; a non-TS partition -> `LiveGraphUnsupportedLanguage`.
fn precondition_fallback_reason(
    precond: Option<&repo_graph_livegraph::import_view::FilePartitionStatus>,
) -> FallbackReason {
    match precond {
        None => FallbackReason::LiveGraphUnavailable,
        Some(s) if !s.fresh => FallbackReason::LiveGraphStale,
        Some(s) if !s.ts_primary => FallbackReason::LiveGraphUnsupportedLanguage,
        // Precondition met -> not a fallback (defensive; this fn is only called on !met).
        Some(_) => FallbackReason::LiveGraphUnavailable,
    }
}

/// IMPORTS-LIVEGRAPH-DEFAULT-1 (D2=B / D3 / D4 / D5): the PURE per-file AUTO decision -- COMPARE-ON-CALL. Serve
/// the LiveGraph edge listing (mapped to ImportEntry, WITH the alias/dynamic extras) IFF the precondition is met
/// AND the directional no-loss passes (NO SQLite resolved-local import missing) AND there is no ambiguous SQLite
/// import; ELSE serve the SQLite listing (byte-identical) with a labelled `fallback_reason`. NEVER a silent loss.
/// No RepoState / no lock -> unit-testable.
fn imports_auto_body(
    file_path: &str,
    sqlite_imports: &[repo_graph_storage::queries::ImportResult],
    view: &repo_graph_livegraph::import_view::LiveImportView,
    precond: Option<&repo_graph_livegraph::import_view::FilePartitionStatus>,
) -> Value {
    use std::collections::BTreeSet;
    let sqlite_resolved: BTreeSet<&str> = sqlite_imports
        .iter()
        .filter(|r| {
            r.kind == "FILE"
                && r.subtype.as_deref() != Some("EXTERNAL")
                && r.resolution.as_deref() == Some("static")
                && !r.file.is_empty()
        })
        .map(|r| r.file.as_str())
        .collect();
    let has_unknown = sqlite_imports.iter().any(|r| {
        r.kind == "FILE"
            && r.subtype.as_deref() != Some("EXTERNAL")
            && r.resolution.as_deref() != Some("static")
            && !r.file.is_empty()
    });
    let lg_edge_targets: BTreeSet<&str> = view.edges.iter().map(|e| e.dst_file.as_str()).collect();
    let missing = sqlite_resolved
        .iter()
        .filter(|t| !lg_edge_targets.contains(*t))
        .count();
    let extra = lg_edge_targets
        .iter()
        .filter(|t| !sqlite_resolved.contains(*t))
        .count();
    let precond_met = precond.is_some_and(|p| p.precondition_met());
    // D1 / D2=B decision: the precondition GATES; then the per-call no-loss + unknown gate.
    let fallback_reason: Option<FallbackReason> = if !precond_met {
        Some(precondition_fallback_reason(precond))
    } else if has_unknown {
        Some(FallbackReason::LiveGraphImportUnknown)
    } else if missing > 0 {
        Some(FallbackReason::LiveGraphImportRegression)
    } else {
        None
    };
    let (backend_used, imports_value): (&str, Value) = match fallback_reason {
        None => (
            "livegraph",
            Value::Array(view.edges.iter().map(edge_to_import_entry).collect()),
        ),
        // SQLite fallback: serialize the Vec the SAME way as the existing sqlite path (byte-identical listing).
        Some(_) => (
            "sqlite",
            serde_json::to_value(sqlite_imports).unwrap_or_else(|_| Value::Array(Vec::new())),
        ),
    };
    let count = imports_value.as_array().map(|a| a.len()).unwrap_or(0);
    json!({
        "file": file_path,
        "imports": imports_value,
        "count": count,
        "backend_used": backend_used,
        "fallback_reason": fallback_reason.map(|r| r.as_str()),
        // D3: a compatible compare summary (JSON-only; stripped in human).
        "comparison": {
            "sqlite_resolved_local": sqlite_resolved.len(),
            "livegraph_edges": view.edges.len(),
            "missing_in_livegraph": missing,
            "extra_livegraph_edges": extra,
        },
    })
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1: the in-memory repo-level import NO-LOSS certificate. `verdict` is the
/// repo-wide compare verdict (`GREEN` = every TS file no-loss at the recorded fingerprint); `fingerprint` is the
/// import-cert fingerprint it was built at. A GREEN cert at the CURRENT fingerprint lets the default serve
/// LiveGraph WITHOUT reading SQLite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNoLossCert {
    /// The repo-wide compare verdict (`GREEN` / `YELLOW` / `RED`).
    pub verdict: String,
    /// The import-cert fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The cert's status for the CURRENT fingerprint (FASTPATH-1 D4).
enum ImportCertState {
    /// A valid cert at the current fingerprint, verdict GREEN -> the fastpath may serve LiveGraph.
    ValidGreen,
    /// A valid cert at the current fingerprint, verdict != GREEN -> compare-on-call.
    ValidNotGreen,
    /// No cert, or a cert at a DIFFERENT fingerprint -> (re)build, else compare-on-call.
    StaleOrMissing,
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D3): the SQLite-FREE import-cert fingerprint -- a digest over the
/// resident partition snapshot (epoch / fresh / ts / source_inputs_hash / producer_fingerprint), the
/// `snapshot_uid` (the repo index epoch -> SQLite-side changes), and the import-completeness policy version. Any
/// import-relevant change (a refresh / swap / re-index / policy bump) yields a different fingerprint.
pub(crate) fn import_cert_fingerprint(
    partitions: &[repo_graph_livegraph::module_cycle_cert::LivePartition],
    snapshot_uid: &str,
) -> String {
    let mut parts: Vec<String> = partitions
        .iter()
        .map(|p| {
            format!(
                "{}@{}:f{}:ts{}:{}:{}",
                p.id,
                p.epoch,
                p.fresh as u8,
                p.ts as u8,
                p.source_inputs_hash,
                p.producer_fingerprint
            )
        })
        .collect();
    parts.sort();
    format!(
        "imp|snap:{}|pol:{}|parts[{}]",
        snapshot_uid,
        crate::cycle_completeness_audit::IMPORT_COMPLETENESS_POLICY_VERSION,
        parts.join(",")
    )
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (the WIN): serve the LiveGraph import edges (mapped to the ImportEntry
/// shape, WITH the alias/dynamic extras) WITHOUT reading SQLite -- the GREEN-cert fastpath. The `imports` array
/// is byte-identical to compare-on-call's served-LiveGraph answer; `comparison.source` records the cert basis.
fn serve_import_fastpath(
    file_path: &str,
    view: &repo_graph_livegraph::import_view::LiveImportView,
) -> Value {
    let imports: Vec<Value> = view.edges.iter().map(edge_to_import_entry).collect();
    json!({
        "file": file_path,
        "imports": imports,
        "count": imports.len(),
        "backend_used": "livegraph",
        "fallback_reason": Value::Null,
        "comparison": { "source": "repo_no_loss_certificate" },
    })
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D4): the PURE fastpath/compare ladder. precondition UNMET -> the SQLite
/// fallback (the compare-on-call body via the find_imports closure) ; precondition met + GREEN cert -> serve
/// LiveGraph WITHOUT calling find_imports ; cert RED/YELLOW/stale/missing/build-failed -> compare-on-call (calls
/// find_imports). Pure (no RepoState) -> a panicking find_imports closure proves the GREEN fastpath skips SQLite;
/// the build_cert closure is invoked ONLY on StaleOrMissing.
fn imports_fastpath_or_compare(
    file_path: &str,
    view: &repo_graph_livegraph::import_view::LiveImportView,
    precond: Option<&repo_graph_livegraph::import_view::FilePartitionStatus>,
    cert: ImportCertState,
    find_imports: impl FnOnce() -> Vec<repo_graph_storage::queries::ImportResult>,
    build_cert: impl FnOnce() -> Option<bool>,
) -> Value {
    let precond_met = precond.is_some_and(|p| p.precondition_met());
    if !precond_met {
        // Non-TS / non-resident -> the SQLite fallback (imports_auto_body labels it).
        return imports_auto_body(file_path, &find_imports(), view, precond);
    }
    let green = match cert {
        ImportCertState::ValidGreen => true,
        ImportCertState::ValidNotGreen => false,
        ImportCertState::StaleOrMissing => build_cert().unwrap_or(false),
    };
    if green {
        serve_import_fastpath(file_path, view)
    } else {
        // RED / YELLOW / build-failed -> the proven compare-on-call (reads SQLite, verifies no-loss).
        imports_auto_body(file_path, &find_imports(), view, precond)
    }
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D2 build): run the repo-wide compare -> verdict, STORE the cert keyed
/// by `fingerprint`, return `Some(is_green)` (or `None` if no fingerprint -> the caller compare-on-calls). Reads
/// SQLite ONCE (the bulk all_imports) per fingerprint; subsequent GREEN calls fastpath without SQLite.
///
/// `pub(crate)` so EXPLAIN-LIVEGRAPH-IMPL's `explain_imports_outcome` (`explain_coherence.rs`) reuses the
/// SAME repo-wide import cert as the imports fastpath — mirroring how `orient_lg_decisions.rs` reuses
/// `build_and_store_cycles_cert`. No new producer; the cert build is shared, not duplicated.
pub(crate) fn build_and_store_import_cert(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let report = imports_readiness_response(repo_state, repo_uid, repo_uid, snapshot_uid);
    let verdict = report
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("RED")
        .to_string();
    let is_green = verdict == "GREEN";
    *repo_state.import_cert.write() = Some(ImportNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1=C / D4): the AUTO (default) `imports <file>` response. Tries the
/// GREEN-cert FASTPATH (serve LiveGraph WITHOUT SQLite) ; else the proven compare-on-call (reads SQLite). The
/// view + precondition + current fingerprint are SQLite-FREE; SQLite is read ONLY on the cert build + the
/// compare-on-call / SQLite-fallback paths.
pub fn imports_auto_response(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
    file_path: &str,
) -> Value {
    // SQLite-FREE: the file's view + precondition + the current import-cert fingerprint.
    let guard = repo_state.livegraph.read();
    let (view, precond, current_fp) = match guard.as_ref() {
        Some(lg) => (
            lg.live_import_view(Some(file_path)),
            lg.file_partition_status(file_path),
            Some(import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)),
        ),
        None => (
            repo_graph_livegraph::import_view::LiveImportView::default(),
            None,
            None,
        ),
    };
    drop(guard);
    // The cert's state for the CURRENT fingerprint.
    let cert = {
        let cached = repo_state.import_cert.read();
        match (&current_fp, cached.as_ref()) {
            (Some(fp), Some(c)) if &c.fingerprint == fp => {
                if c.verdict == "GREEN" {
                    ImportCertState::ValidGreen
                } else {
                    ImportCertState::ValidNotGreen
                }
            }
            _ => ImportCertState::StaleOrMissing,
        }
    };
    let file_stable_key = format!("{repo_uid}:{file_path}:FILE");
    imports_fastpath_or_compare(
        file_path,
        &view,
        precond.as_ref(),
        cert,
        || {
            repo_state
                .storage
                .find_imports(snapshot_uid, &file_stable_key)
                .unwrap_or_default()
        },
        || build_and_store_import_cert(repo_state, repo_uid, snapshot_uid, current_fp.clone()),
    )
}

/// IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1 (D2/D3/D4): the PURE repo-wide aggregation -- run the per-file
/// DIRECTIONAL verdict (`directional_status`) over the UNION of SQLite + LiveGraph import-bearing files and fold
/// the metrics + verdict. No RepoState / no lock -> unit-testable. `precond_map` = file -> resident TS partition
/// status (ABSENT = precondition unmet -> the language gate). UNKNOWN = a kind=FILE non-empty-target import that
/// is neither resolved-local (static) nor external -> could hide a loss -> forces RED.
#[allow(clippy::too_many_arguments)]
fn aggregate_readiness(
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    bulk: &[repo_graph_storage::queries::BulkImportRow],
    view: &repo_graph_livegraph::import_view::LiveImportView,
    precond_map: &std::collections::BTreeMap<
        String,
        repo_graph_livegraph::import_view::FilePartitionStatus,
    >,
) -> Value {
    use std::collections::{BTreeMap, BTreeSet};
    // SQLite grouped by source -> resolved-local targets + unknown rows (D2/D3).
    let mut sqlite_resolved: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut sqlite_unknown: Vec<(&str, &str, &str)> = Vec::new(); // (source, target, resolution)
    let mut sqlite_sources: BTreeSet<&str> = BTreeSet::new();
    for r in bulk {
        sqlite_sources.insert(r.source_file.as_str());
        let is_file = r.kind == "FILE" && !r.target_file.is_empty();
        let is_external = r.subtype.as_deref() == Some("EXTERNAL");
        let is_static = r.resolution.as_deref() == Some("static");
        if is_file && !is_external && is_static {
            sqlite_resolved
                .entry(r.source_file.as_str())
                .or_default()
                .insert(r.target_file.as_str());
        } else if is_file && !is_external {
            // a FILE-target import that is neither resolved-local nor external -> UNKNOWN.
            sqlite_unknown.push((
                r.source_file.as_str(),
                r.target_file.as_str(),
                r.resolution.as_deref().unwrap_or(""),
            ));
        }
        // else: external / no-file target -> excluded from edge equivalence.
    }
    // LiveGraph grouped by source.
    let mut lg_edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut lg_blocking: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut lg_sources: BTreeSet<&str> = BTreeSet::new();
    for e in &view.edges {
        lg_sources.insert(e.src_file.as_str());
        lg_edges
            .entry(e.src_file.as_str())
            .or_default()
            .insert(e.dst_file.as_str());
    }
    for o in &view.observations {
        lg_sources.insert(o.source_file.as_str());
        if o.blocking {
            lg_blocking
                .entry(o.source_file.as_str())
                .or_default()
                .push(o.class.as_str());
        }
    }
    // The compared file set = the UNION (D2).
    let mut all_files: BTreeSet<&str> = BTreeSet::new();
    all_files.extend(sqlite_sources.iter().copied());
    all_files.extend(lg_sources.iter().copied());
    let unknown_total = sqlite_unknown.len();
    let empty: BTreeSet<&str> = BTreeSet::new();
    let (mut files_total, mut met, mut fallback, mut regression) = (0usize, 0usize, 0usize, 0usize);
    let (mut missing_total, mut extra_total, mut blocking_total) = (0usize, 0usize, 0usize);
    let mut blocking_by_class: BTreeMap<&str, usize> = BTreeMap::new();
    let mut regressions: Vec<Value> = Vec::new();
    for file in &all_files {
        files_total += 1;
        let precond_met = precond_map.get(*file).is_some_and(|s| s.precondition_met());
        let st = sqlite_resolved.get(file).unwrap_or(&empty);
        let lt = lg_edges.get(file).unwrap_or(&empty);
        let missing: Vec<&str> = st.iter().filter(|t| !lt.contains(*t)).copied().collect();
        let extra = lt.iter().filter(|t| !st.contains(*t)).count();
        extra_total += extra;
        if let Some(bs) = lg_blocking.get(file) {
            for c in bs {
                blocking_total += 1;
                *blocking_by_class.entry(c).or_default() += 1;
            }
        }
        match directional_status(precond_met, !missing.is_empty(), extra > 0) {
            "FallbackPreconditionUnmet" => fallback += 1,
            "Regression" => {
                met += 1;
                regression += 1;
                missing_total += missing.len();
                regressions.push(json!({ "file": file, "missing": missing }));
            }
            _ => met += 1,
        }
    }
    let fallback_share = if files_total > 0 {
        fallback as f64 / files_total as f64
    } else {
        0.0
    };
    // fallback-heavy = > 50% of files fall back (documented threshold; ALWAYS reported, never hidden -- D5).
    let fallback_heavy = fallback_share > 0.5;
    // D4 verdict: RED on any regression OR unknown; YELLOW if safe but fallback-heavy / serves no file; else GREEN.
    let verdict = if regression > 0 || unknown_total > 0 {
        "RED"
    } else if fallback_heavy || met == 0 {
        "YELLOW"
    } else {
        "GREEN"
    };
    let blocking_by_class_json: serde_json::Map<String, Value> = blocking_by_class
        .iter()
        .map(|(k, v)| ((*k).to_string(), json!(v)))
        .collect();
    let unknowns_json: Vec<Value> = sqlite_unknown
        .iter()
        .map(|(f, t, r)| json!({ "file": f, "target": t, "resolution": r }))
        .collect();
    json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "engine": "compare",
        "scope": "repo-wide",
        "backend_used": "sqlite",
        "verdict": verdict,
        "coverage_complete": true,
        "metrics": {
            "files_total": files_total,
            "files_precondition_met": met,
            "files_fallback_required": fallback,
            "files_regression": regression,
            "missing_in_livegraph_total": missing_total,
            "extra_livegraph_edges_total": extra_total,
            "blocking_observation_total": blocking_total,
            "blocking_observation_by_class": blocking_by_class_json,
            "unknown_total": unknown_total,
            "sqlite_import_bearing_files": sqlite_sources.len(),
            "livegraph_import_bearing_files": lg_sources.len(),
            "fallback_share": fallback_share,
            "fallback_heavy": fallback_heavy,
        },
        "regressions": regressions,
        "unknowns": unknowns_json,
    })
}

/// IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1 (D6): the `imports --engine compare` NO-FILE response -- the repo-wide
/// readiness aggregate. Reads the bulk SQLite imports + the full LiveGraph view + the bulk precondition map ONCE
/// (the view + map under one lock), then the PURE `aggregate_readiness`. READ-ONLY; NO default flip.
pub fn imports_readiness_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Value {
    let bulk = repo_state
        .storage
        .all_imports(snapshot_uid)
        .unwrap_or_default();
    let guard = repo_state.livegraph.read();
    let (view, precond_map) = match guard.as_ref() {
        Some(lg) => (lg.live_import_view(None), lg.resident_file_statuses()),
        None => (
            repo_graph_livegraph::import_view::LiveImportView::default(),
            std::collections::BTreeMap::new(),
        ),
    };
    drop(guard);
    aggregate_readiness(
        repo_uid,
        display_name,
        snapshot_uid,
        &bulk,
        &view,
        &precond_map,
    )
}

/// One classed module-cycle divergence in the compare report (MODULE-CYCLES-CLI-1 D4=A).
#[derive(Debug, Serialize)]
pub struct ModuleDivergenceEntry {
    /// The diverging cycle as a canonical module-path set.
    pub cycle: Vec<String>,
    /// The divergence class (the `ModuleCycleDivergence` variant name).
    pub divergence: String,
}

/// The `--engine compare --kind module-import` report (D4=A: STRUCTURAL only — missing ->
/// UnknownDivergence, extra -> UnexpectedExtraInLiveGraph; NO automatic package/dynamic attribution this
/// slice, MODULE-CYCLES-COMPARE-CLASSIFY-1). Written to the sidecar + inlined in the response.
#[derive(Debug, Serialize)]
pub struct ModuleCycleCompareReport {
    /// SQLite MODULE cycle count (the primary answer).
    pub sqlite_count: usize,
    /// LiveGraph derived MODULE cycle count.
    pub livegraph_count: usize,
    /// The LiveGraph module-cycle answer's trust class.
    pub livegraph_class: String,
    /// Cycles present in BOTH (by module-path set).
    pub matched: usize,
    /// True iff the LiveGraph has NO extra cycle (the real-repo expectation; an extra is an overclaim/bug).
    pub livegraph_subset: bool,
    /// SQLite cycles the LiveGraph lacks (D4=A: each `UnknownDivergence` — cause attribution deferred).
    pub missing_in_livegraph: Vec<ModuleDivergenceEntry>,
    /// LiveGraph cycles SQLite lacks (each `UnexpectedExtraInLiveGraph` — an overclaim signal).
    pub extra_in_livegraph: Vec<ModuleDivergenceEntry>,
}

/// Write the module-cycle compare report to `<repo_root>/.rgr/livegraph-compare/module-<ms>.json` (the
/// callers/callees/path sidecar convention). Best-effort; the caller must not fail the query on error.
fn write_module_compare_sidecar(
    repo_root: &str,
    report: &ModuleCycleCompareReport,
) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::path::Path::new(repo_root)
        .join(".rgr")
        .join("livegraph-compare");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create sidecar dir: {e}"))?;
    let path = dir.join(format!("module-{ts}.json"));
    let body =
        serde_json::to_string_pretty(report).map_err(|e| format!("serialize report: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write sidecar: {e}"))?;
    Ok(path.display().to_string())
}

/// The shared MODULE-cycle comparison data — the SINGLE source of the compare verdict. Computed once from
/// SQLite (`find_cycles` + `module_qualified_names`) + the LiveGraph (`module_import_cycles`), consumed by BOTH
/// [`module_cycle_compare_response`] (the `--engine compare` surface) AND [`build_and_store_cycles_cert`] (the
/// default fastpath cert). Sharing the computation makes a GREEN cert PROVABLY equal to the compare verdict —
/// no drift, so the fastpath can never serve a repo the compare would have flagged missing/extra.
struct ModuleCycleCompareData {
    /// The structural comparison (matched / missing / extra) by qualified module-path SET.
    comparison: repo_graph_livegraph::module_cycle_compare::ModuleCycleComparison,
    /// The raw SQLite MODULE cycles (the compare's PRIMARY output shape — unchanged).
    sqlite_cycles: Vec<repo_graph_storage::queries::CycleResult>,
    /// SQLite MODULE cycle count.
    sqlite_count: usize,
    /// LiveGraph derived MODULE cycle count.
    livegraph_count: usize,
    /// The LiveGraph module-cycle answer's trust class (`Exact` / `Partial` / ... / `Unavailable`).
    livegraph_class: String,
    /// Per-module import observations (for the response's missing-cycle classification).
    obs_by_module: std::collections::BTreeMap<
        String,
        Vec<repo_graph_livegraph::module_cycle_compare::ObservationView>,
    >,
    /// The resident MODULE identities (for classification).
    lg_modules: std::collections::BTreeSet<String>,
}

/// Compute the shared [`ModuleCycleCompareData`] — the SQLite MODULE cycles mapped to QUALIFIED module paths vs
/// the LiveGraph derived module cycles, compared by SET. Reads SQLite once + the LiveGraph once (one read lock).
fn module_cycle_compare_data(
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> Result<ModuleCycleCompareData, repo_graph_storage::error::StorageError> {
    use repo_graph_livegraph::module_cycle_compare::compare_module_cycles;
    let sqlite_cycles = repo_state.storage.find_cycles(snapshot_uid, "module")?;
    let qnames = repo_state.storage.module_qualified_names(snapshot_uid)?;
    let sqlite_count = sqlite_cycles.len();
    // D5: the SHORT module name ("src") collides across packages; the compare diffs by the QUALIFIED path.
    let sqlite_qualified: Vec<Vec<String>> = sqlite_cycles
        .iter()
        .map(|c| {
            c.nodes
                .iter()
                .map(|n| {
                    qnames
                        .get(&n.node_id)
                        .cloned()
                        .unwrap_or_else(|| n.name.clone())
                })
                .collect()
        })
        .collect();
    let (lg_cycles, livegraph_class, obs_by_module, lg_modules) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let env = lg.module_import_cycles();
                let cycles = env
                    .data()
                    .map(|d| d.cycles.iter().map(|c| c.members.clone()).collect())
                    .unwrap_or_default();
                (
                    cycles,
                    format!("{:?}", env.class()),
                    lg.import_observations_by_module(),
                    lg.resident_module_paths(),
                )
            }
            None => (
                Vec::new(),
                "Unavailable".to_string(),
                std::collections::BTreeMap::new(),
                std::collections::BTreeSet::new(),
            ),
        }
    };
    let comparison = compare_module_cycles(&lg_cycles, &sqlite_qualified);
    Ok(ModuleCycleCompareData {
        comparison,
        sqlite_cycles,
        sqlite_count,
        livegraph_count: lg_cycles.len(),
        livegraph_class,
        obs_by_module,
        lg_modules,
    })
}

/// MODULE-CYCLES-CLI-1 (D4=A): build the `--engine compare --kind module-import` response. PRIMARY = the
/// SQLite MODULE cycles (unchanged shape); plus a STRUCTURAL compare of the LiveGraph derived module cycles
/// (by qualified module-path sets, D5) against SQLite + a diagnostic sidecar. Missing -> UnknownDivergence,
/// extra -> UnexpectedExtraInLiveGraph (no auto cause attribution this slice).
pub fn module_cycle_compare_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    repo_root: &str,
) -> Result<Value, String> {
    use repo_graph_livegraph::module_cycle_compare::classify_missing_module_cycle;
    // The SHARED comparison computation (identical basis to the fastpath cert -> no drift).
    let data = module_cycle_compare_data(repo_state, snapshot_uid).map_err(|e| e.to_string())?;
    let cmp = &data.comparison;
    // MODULE-CYCLES-COMPARE-CLASSIFY-1 (D2=A): classify each missing cycle from LiveGraph evidence
    // (replacing the blanket UnknownDivergence); evidence-backed or Unknown.
    let missing_in_livegraph: Vec<ModuleDivergenceEntry> = cmp
        .missing_in_livegraph
        .iter()
        .map(|c| {
            let cycle_set: std::collections::BTreeSet<String> = c.iter().cloned().collect();
            let class =
                classify_missing_module_cycle(&cycle_set, &data.obs_by_module, &data.lg_modules);
            ModuleDivergenceEntry {
                cycle: c.clone(),
                divergence: class.as_str().to_string(),
            }
        })
        .collect();
    let extra_in_livegraph: Vec<ModuleDivergenceEntry> = cmp
        .extra_in_livegraph
        .iter()
        .map(|c| ModuleDivergenceEntry {
            cycle: c.clone(),
            divergence: "UnexpectedExtraInLiveGraph".to_string(),
        })
        .collect();
    let report = ModuleCycleCompareReport {
        sqlite_count: data.sqlite_count,
        livegraph_count: data.livegraph_count,
        livegraph_class: data.livegraph_class.clone(),
        matched: cmp.matched.len(),
        livegraph_subset: cmp.is_livegraph_subset(),
        missing_in_livegraph,
        extra_in_livegraph,
    };
    let sidecar = write_module_compare_sidecar(repo_root, &report).ok();
    let mut v = json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": data.sqlite_cycles,
        "count": data.sqlite_count,
        "backend_used": "sqlite",
        "kind": "module-import",
        "livegraph_module_compare": serde_json::to_value(&report).unwrap_or(Value::Null),
    });
    if let Some(p) = sidecar {
        v["livegraph_module_compare_sidecar"] = json!(p);
    }
    Ok(v)
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: the in-memory repo-level MODULE-cycle NO-LOSS certificate. `verdict`
/// is `GREEN` iff the module-cycle compare is EXACT (no SQLite cycle missing from the LiveGraph, no extra);
/// `fingerprint` is the SQLite-free fingerprint (SHARED with `import_cert`) it was built at. A GREEN cert at the
/// CURRENT fingerprint lets the default `cycles` serve the LiveGraph module cycles WITHOUT `find_cycles`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleNoLossCert {
    /// The module-cycle compare verdict (`GREEN` = no-loss / `RED` = a missing or extra cycle).
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The cycles cert's status for the CURRENT fingerprint (mirrors `ImportCertState`).
enum CycleCertState {
    /// Valid at the current fingerprint, GREEN -> the fastpath may serve LiveGraph.
    ValidGreen,
    /// Valid at the current fingerprint, not GREEN -> SQLite.
    ValidNotGreen,
    /// No cert, or a cert at a DIFFERENT fingerprint -> (re)build, else SQLite.
    StaleOrMissing,
}

/// Serve the LiveGraph MODULE cycles in the CANONICAL, byte-identical output (`cycle_output`) WITHOUT reading
/// SQLite -- the GREEN-cert fastpath. The `cycles` array is byte-identical to the SQLite default's canonical
/// output (proven in CYCLES-OUTPUT-CONTRACT-1 on xpart + amodx). `backend_used=livegraph`, no fallback.
fn serve_cycles_fastpath(
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    lg_cycles: &[Vec<String>],
) -> Value {
    let cycles = crate::cycle_output::livegraph_module_cycles_json(lg_cycles);
    let count = cycles.len();
    json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": cycles,
        "count": count,
        "backend_used": "livegraph",
        "fallback_reason": Value::Null,
    })
}

/// The SQLite default cycles answer (CANONICAL, CYCLES-OUTPUT-CONTRACT-1) + the fastpath metadata. Reads SQLite
/// (`find_cycles` + `module_qualified_names`) -- the fallback / cert-not-green path.
fn serve_cycles_sqlite(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    fallback_reason: FallbackReason,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    let sqlite_cycles = repo_state.storage.find_cycles(snapshot_uid, "module")?;
    let qualified = repo_state.storage.module_qualified_names(snapshot_uid)?;
    let cycles = crate::cycle_output::sqlite_module_cycles_json(&sqlite_cycles, &qualified);
    let count = cycles.len();
    Ok(json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": cycles,
        "count": count,
        "backend_used": "sqlite",
        "fallback_reason": fallback_reason.as_str(),
    }))
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (build): run the SHARED module-cycle compare -> verdict, STORE the cert
/// keyed by `fingerprint`, return `Some(is_green)` (or `None` if no fingerprint / a storage error -> the caller
/// falls back to SQLite). Reads SQLite ONCE per fingerprint via the SAME `module_cycle_compare_data` the
/// `--engine compare` uses, so the GREEN verdict PROVABLY matches the compare (no drift -> no false GREEN).
pub(crate) fn build_and_store_cycles_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let data = module_cycle_compare_data(repo_state, snapshot_uid).ok()?;
    let is_green = data.comparison.is_exact();
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.cycles_cert.write() = Some(CycleNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1/D4): the PURE fastpath/SQLite ladder. precondition UNMET (the
/// LiveGraph module-cycle answer is not `Exact` -- non-resident / non-TS / degraded) -> SQLite (the labelled
/// `precondition_reason`) ; precondition met + GREEN cert -> serve LiveGraph WITHOUT find_cycles ; cert
/// RED/stale/missing/build-failed -> SQLite (`LiveGraphCycleDivergence`). Pure (no I/O itself): a panicking
/// `serve_sqlite` proves the GREEN path skips SQLite; `build_cert` is invoked ONLY on StaleOrMissing.
fn cycles_fastpath_or_sqlite(
    precondition_met: bool,
    precondition_reason: FallbackReason,
    cert: CycleCertState,
    serve_livegraph: impl FnOnce() -> Value,
    serve_sqlite: impl FnOnce(FallbackReason) -> Value,
    build_cert: impl FnOnce() -> Option<bool>,
) -> Value {
    if !precondition_met {
        return serve_sqlite(precondition_reason);
    }
    let green = match cert {
        CycleCertState::ValidGreen => true,
        CycleCertState::ValidNotGreen => false,
        CycleCertState::StaleOrMissing => build_cert().unwrap_or(false),
    };
    if green {
        serve_livegraph()
    } else {
        serve_sqlite(FallbackReason::LiveGraphCycleDivergence)
    }
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1=A / D4): the AUTO (default) `cycles` response. Tries the GREEN-cert
/// FASTPATH (serve the LiveGraph module cycles WITHOUT SQLite) ; else SQLite (canonical). The answer-class +
/// the LiveGraph cycles + the current fingerprint are SQLite-FREE; SQLite is read ONLY on the cert build + the
/// SQLite fallback. The served output is byte-identical either way (CYCLES-OUTPUT-CONTRACT-1).
pub fn cycles_auto_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    // SQLite-FREE: the module-cycle answer-class (the precondition), the LiveGraph cycles (the served answer),
    // and the current fingerprint -- all from a single LiveGraph read lock.
    let (precondition_met, precondition_reason, lg_cycles, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let env = lg.module_import_cycles();
                let met = format!("{:?}", env.class()) == "Exact";
                let cycles: Vec<Vec<String>> = env
                    .data()
                    .map(|d| d.cycles.iter().map(|c| c.members.clone()).collect())
                    .unwrap_or_default();
                let reason = if met {
                    FallbackReason::LiveGraphUnavailable // unused when met (defensive)
                } else {
                    FallbackReason::LiveGraphPartial
                };
                let fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
                (met, reason, cycles, Some(fp))
            }
            None => (
                false,
                FallbackReason::LiveGraphUnavailable,
                Vec::new(),
                None,
            ),
        }
    };
    // The cert's state for the CURRENT fingerprint.
    let cert = {
        let cached = repo_state.cycles_cert.read();
        match (&current_fp, cached.as_ref()) {
            (Some(fp), Some(c)) if &c.fingerprint == fp => {
                if c.verdict == "GREEN" {
                    CycleCertState::ValidGreen
                } else {
                    CycleCertState::ValidNotGreen
                }
            }
            _ => CycleCertState::StaleOrMissing,
        }
    };
    // The ladder. A storage error in the SQLite fallback surfaces as Err (the caller maps it).
    let mut sqlite_err: Option<repo_graph_storage::error::StorageError> = None;
    let result = cycles_fastpath_or_sqlite(
        precondition_met,
        precondition_reason,
        cert,
        || serve_cycles_fastpath(repo_uid, display_name, snapshot_uid, &lg_cycles),
        |reason| match serve_cycles_sqlite(repo_state, repo_uid, display_name, snapshot_uid, reason)
        {
            Ok(v) => v,
            Err(e) => {
                sqlite_err = Some(e);
                Value::Null
            }
        },
        || build_and_store_cycles_cert(repo_state, snapshot_uid, current_fp.clone()),
    );
    if let Some(e) = sqlite_err {
        return Err(e);
    }
    Ok(result)
}

// ════════════════════════════════════════════════════════════════════════════════════════════════
// STATS-LIVEGRAPH-IMPL-1: the cert-gated LiveGraph fastpath for the default `rmap stats`. Mirrors the
// imports/cycles fastpath EXACTLY: a repo-level field-exact STATS no-loss cert (keyed by the SHARED
// SQLite-free fingerprint) gates serving the LiveGraph module stats WITHOUT `compute_module_stats`;
// RED / stale / missing / precondition-unmet falls back to the proven SQLite answer (byte-identical
// human output — the renderer's deterministic per-section re-sort + qualified_name-ascending rows).
// ════════════════════════════════════════════════════════════════════════════════════════════════

/// STATS-LIVEGRAPH-IMPL-1: the in-memory repo-level STATS NO-LOSS certificate (`None` until lazily built
/// on the first eligible default `stats` query). `verdict` is the repo-wide field-exact compare verdict
/// (`GREEN` = every module field-exact between the LiveGraph and SQLite answers); `fingerprint` is the
/// SHARED SQLite-free fingerprint (the SAME one `import_cert`/`cycles_cert` use) it was built at — a
/// fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on restart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsNoLossCert {
    /// The repo-wide field-exact compare verdict (`GREEN` / `RED`).
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The stats cert's status for the CURRENT fingerprint (mirrors `CycleCertState`).
enum StatsCertState {
    /// A valid cert at the current fingerprint, verdict GREEN -> the fastpath may serve LiveGraph.
    ValidGreen,
    /// A valid cert at the current fingerprint, verdict != GREEN -> SQLite fallback.
    ValidNotGreen,
    /// No cert, or a cert at a DIFFERENT fingerprint -> (re)build, else SQLite fallback.
    StaleOrMissing,
}

/// Map the LiveGraph raw per-module stat rows -> the SQLite-compatible `ModuleStatsResult` DTO, deriving
/// the Martin metrics through the SHARED `martin_metrics` helper (the SAME one `compute_module_stats`
/// uses). Identical code + identical integer inputs => bit-identical floats, so a field-exact compare of
/// the two DTO vecs never spuriously REDs on float jitter (RISK-5). The rows arrive module-ascending
/// (the LiveGraph guarantee) and that ordering is preserved here.
fn livegraph_module_stats_dto(rows: &[ModuleStatRow]) -> Vec<ModuleStatsResult> {
    rows.iter()
        .map(|r| {
            let (instability, abstractness, distance_from_main_sequence) =
                martin_metrics(r.fan_in, r.fan_out, r.abstract_count, r.type_count);
            ModuleStatsResult {
                module: r.module.clone(),
                fan_in: r.fan_in,
                fan_out: r.fan_out,
                instability,
                abstractness,
                distance_from_main_sequence,
                file_count: r.file_count,
                symbol_count: r.symbol_count,
            }
        })
        .collect()
}

/// The shared STATS comparison data — the SINGLE source of the compare verdict. Computed once from SQLite
/// (`compute_module_stats`) + the LiveGraph (`module_stats` mapped through the shared metric helper),
/// consumed by BOTH [`stats_compare_response`] (the `--engine compare` surface) AND
/// [`build_and_store_stats_cert`] (the default fastpath cert). Sharing the computation makes a GREEN cert
/// PROVABLY equal to the compare verdict — no drift, so the fastpath can never serve a divergent stat.
struct StatsCompareData {
    /// The LiveGraph module stats mapped to the SQLite DTO shape (module-ascending).
    livegraph_stats: Vec<ModuleStatsResult>,
    /// The SQLite `compute_module_stats` answer (the proven primary).
    sqlite_stats: Vec<ModuleStatsResult>,
    /// The LiveGraph module-stats answer's trust class (`Exact` / `Partial` / ... / `Unavailable`).
    livegraph_class: String,
    /// True iff the repo-wide field-exact compare is EXACT (missing=0 ∧ extra=0 ∧ no field mismatch).
    is_exact: bool,
    /// Modules SQLite has that the LiveGraph lacks (a missing-module divergence).
    missing_in_livegraph: Vec<String>,
    /// Modules the LiveGraph has that SQLite lacks (an extra-module divergence).
    extra_in_livegraph: Vec<String>,
    /// Modules present in BOTH whose fields differ (a count/metric mismatch).
    field_mismatches: Vec<String>,
}

/// Compute the shared [`StatsCompareData`] — the SQLite `compute_module_stats` answer vs the LiveGraph
/// `module_stats` answer (mapped to the same DTO), compared per-module by module identity then field-
/// exact (`ModuleStatsResult: PartialEq`; the floats are bit-identical by the shared `martin_metrics`).
/// Reads SQLite once + the LiveGraph once (one read lock).
fn stats_compare_data(
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> Result<StatsCompareData, StorageError> {
    use std::collections::BTreeMap;
    let sqlite_stats = repo_state.storage.compute_module_stats(snapshot_uid)?;
    let (lg_rows, livegraph_class) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let env = lg.module_stats();
                let rows = env.data().map(|d| d.modules.clone()).unwrap_or_default();
                (rows, format!("{:?}", env.class()))
            }
            None => (Vec::new(), "Unavailable".to_string()),
        }
    };
    let livegraph_stats = livegraph_module_stats_dto(&lg_rows);
    // Field-exact set+field compare by module identity.
    let sq_map: BTreeMap<&str, &ModuleStatsResult> = sqlite_stats
        .iter()
        .map(|m| (m.module.as_str(), m))
        .collect();
    let lg_map: BTreeMap<&str, &ModuleStatsResult> = livegraph_stats
        .iter()
        .map(|m| (m.module.as_str(), m))
        .collect();
    let mut missing_in_livegraph = Vec::new();
    let mut field_mismatches = Vec::new();
    for (module, sq) in &sq_map {
        match lg_map.get(module) {
            None => missing_in_livegraph.push(module.to_string()),
            // PartialEq over all 8 fields; the module field is equal by the key, so this checks the
            // numeric fields (and the floats are bit-identical when the integer counts agree).
            Some(lg) => {
                if lg != sq {
                    field_mismatches.push(module.to_string());
                }
            }
        }
    }
    let extra_in_livegraph: Vec<String> = lg_map
        .keys()
        .filter(|m| !sq_map.contains_key(*m))
        .map(|m| m.to_string())
        .collect();
    let is_exact = missing_in_livegraph.is_empty()
        && extra_in_livegraph.is_empty()
        && field_mismatches.is_empty();
    Ok(StatsCompareData {
        livegraph_stats,
        sqlite_stats,
        livegraph_class,
        is_exact,
        missing_in_livegraph,
        extra_in_livegraph,
        field_mismatches,
    })
}

/// STATS-LIVEGRAPH-IMPL-1 (build): run the SHARED field-exact stats compare -> verdict, STORE the cert
/// keyed by `fingerprint`, return `Some(is_green)` (or `None` if no fingerprint / a storage error -> the
/// caller falls back to SQLite). Reads SQLite ONCE per fingerprint via the SAME `stats_compare_data` the
/// `--engine compare` uses, so the GREEN verdict PROVABLY matches the compare (no drift -> no false GREEN).
fn build_and_store_stats_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let data = stats_compare_data(repo_state, snapshot_uid).ok()?;
    let is_green = data.is_exact;
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.stats_cert.write() = Some(StatsNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// STATS-LIVEGRAPH-IMPL-1: the GREEN-cert fastpath body — serve the LiveGraph module stats WITHOUT
/// `compute_module_stats`. The `stats` array is byte-identical to the SQLite default's output (same
/// values via the GREEN field-exact cert; same module-ascending order). `backend_used=livegraph`.
fn serve_stats_fastpath(
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    lg_stats: &[ModuleStatsResult],
) -> Value {
    json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "display_name": display_name,
        "stats": lg_stats,
        "count": lg_stats.len(),
        "backend_used": "livegraph",
        "fallback_reason": Value::Null,
    })
}

/// STATS-LIVEGRAPH-IMPL-1: the SQLite stats answer + the fastpath metadata — the fallback / cert-not-green
/// path. Reads SQLite (`compute_module_stats`). Distinct from the forced `--engine sqlite` arm, which
/// returns the UNCHANGED body (no `backend_used`) per D4.
fn serve_stats_sqlite(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    fallback_reason: FallbackReason,
) -> Result<Value, StorageError> {
    let stats = repo_state.storage.compute_module_stats(snapshot_uid)?;
    Ok(json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "display_name": display_name,
        "stats": stats,
        "count": stats.len(),
        "backend_used": "sqlite",
        "fallback_reason": fallback_reason.as_str(),
    }))
}

/// STATS-LIVEGRAPH-IMPL-1 (D3): the PURE fastpath/SQLite ladder (mirrors `cycles_fastpath_or_sqlite`).
/// precondition UNMET (the LiveGraph module-stats answer is not `Exact` -- non-resident / non-TS /
/// degraded) -> SQLite (the labelled `precondition_reason`) ; precondition met + GREEN cert -> serve
/// LiveGraph WITHOUT `compute_module_stats` ; cert RED/stale/missing/build-failed -> SQLite
/// (`LiveGraphStatsDivergence`). Pure (no I/O itself): a panicking `serve_sqlite` proves the GREEN path
/// skips SQLite; `build_cert` is invoked ONLY on StaleOrMissing.
fn stats_fastpath_or_sqlite(
    precondition_met: bool,
    precondition_reason: FallbackReason,
    cert: StatsCertState,
    serve_livegraph: impl FnOnce() -> Value,
    serve_sqlite: impl FnOnce(FallbackReason) -> Value,
    build_cert: impl FnOnce() -> Option<bool>,
) -> Value {
    if !precondition_met {
        return serve_sqlite(precondition_reason);
    }
    let green = match cert {
        StatsCertState::ValidGreen => true,
        StatsCertState::ValidNotGreen => false,
        StatsCertState::StaleOrMissing => build_cert().unwrap_or(false),
    };
    if green {
        serve_livegraph()
    } else {
        serve_sqlite(FallbackReason::LiveGraphStatsDivergence)
    }
}

/// STATS-LIVEGRAPH-IMPL-1 (D3): the AUTO (default) `stats` response. Tries the GREEN-cert FASTPATH (serve
/// the LiveGraph module stats WITHOUT SQLite) ; else SQLite (the proven answer). The answer-class + the
/// LiveGraph stats + the current fingerprint are SQLite-FREE; SQLite is read ONLY on the cert build + the
/// SQLite fallback. The served human output is byte-identical either way (the byte-preserving contract).
pub fn stats_auto_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Result<Value, StorageError> {
    // SQLite-FREE: the module-stats answer-class (precondition), the mapped LiveGraph stats (the served
    // answer), and the current fingerprint -- all from a single LiveGraph read lock.
    let (precondition_met, precondition_reason, lg_stats, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                let env = lg.module_stats();
                let met = format!("{:?}", env.class()) == "Exact";
                let rows = env.data().map(|d| d.modules.clone()).unwrap_or_default();
                let dto = livegraph_module_stats_dto(&rows);
                let reason = if met {
                    FallbackReason::LiveGraphUnavailable // unused when met (defensive)
                } else {
                    FallbackReason::LiveGraphPartial
                };
                let fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
                (met, reason, dto, Some(fp))
            }
            None => (
                false,
                FallbackReason::LiveGraphUnavailable,
                Vec::new(),
                None,
            ),
        }
    };
    // The cert's state for the CURRENT fingerprint.
    let cert = {
        let cached = repo_state.stats_cert.read();
        match (&current_fp, cached.as_ref()) {
            (Some(fp), Some(c)) if &c.fingerprint == fp => {
                if c.verdict == "GREEN" {
                    StatsCertState::ValidGreen
                } else {
                    StatsCertState::ValidNotGreen
                }
            }
            _ => StatsCertState::StaleOrMissing,
        }
    };
    // The ladder. A storage error in the SQLite fallback surfaces as Err (the caller maps it).
    let mut sqlite_err: Option<StorageError> = None;
    let result = stats_fastpath_or_sqlite(
        precondition_met,
        precondition_reason,
        cert,
        || serve_stats_fastpath(repo_uid, display_name, snapshot_uid, &lg_stats),
        |reason| match serve_stats_sqlite(repo_state, repo_uid, display_name, snapshot_uid, reason)
        {
            Ok(v) => v,
            Err(e) => {
                sqlite_err = Some(e);
                Value::Null
            }
        },
        || build_and_store_stats_cert(repo_state, snapshot_uid, current_fp.clone()),
    );
    if let Some(e) = sqlite_err {
        return Err(e);
    }
    Ok(result)
}

/// STATS-LIVEGRAPH-IMPL-1: the explicit `--engine livegraph` diagnostic — serve the LiveGraph module
/// stats DIRECTLY (no cert gate, no SQLite fallback), labelled with the trust class/freshness/scope so a
/// non-`Exact` answer is never read as complete. Mirrors `module_import_cycles_response`. Lets an operator
/// inspect exactly what the LiveGraph half computes (the input to the cert compare).
pub fn stats_livegraph_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
) -> Value {
    let guard = repo_state.livegraph.read();
    let (class, freshness, missing, reasons, stats, scope) = match guard.as_ref() {
        Some(lg) => {
            let env = lg.module_stats();
            let data = env.data();
            let stats = data
                .map(|d| livegraph_module_stats_dto(&d.modules))
                .unwrap_or_default();
            let scope = data
                .map(|d| {
                    json!({
                        "captured_resolved_relative": d.scope.file_scope.captured_resolved_relative,
                        "intra_partition": d.scope.file_scope.intra_partition,
                        "cross_partition": d.scope.file_scope.cross_partition,
                        "xpart_edge_count": d.scope.file_scope.xpart_edge_count,
                        "module_aggregated": d.scope.module_aggregated,
                        "aggregation_basis": "dirname",
                    })
                })
                .unwrap_or_else(
                    || json!({ "module_aggregated": true, "aggregation_basis": "dirname" }),
                );
            (
                format!("{:?}", env.class()),
                format!("{:?}", env.freshness()),
                env.missing_partitions().to_vec(),
                env.degradation_reasons()
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>(),
                stats,
                scope,
            )
        }
        None => (
            "Unavailable".to_string(),
            "Unavailable".to_string(),
            Vec::new(),
            vec!["LiveGraphUnavailable".to_string()],
            Vec::new(),
            json!({ "module_aggregated": true, "aggregation_basis": "dirname" }),
        ),
    };
    json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "display_name": display_name,
        "stats": stats,
        "count": stats.len(),
        "backend_used": "livegraph",
        "scope": scope,
        "answer_class": class,
        "freshness": freshness,
        "missing_partitions": missing,
        "degradation_reasons": reasons,
    })
}

/// Write the stats compare report to `<repo_root>/.rgr/livegraph-compare/stats-<ms>.json` (the cycles
/// sidecar convention). Best-effort; the caller must not fail the query on error.
fn write_stats_compare_sidecar(repo_root: &str, report: &Value) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::path::Path::new(repo_root)
        .join(".rgr")
        .join("livegraph-compare");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create sidecar dir: {e}"))?;
    let path = dir.join(format!("stats-{ts}.json"));
    let body =
        serde_json::to_string_pretty(report).map_err(|e| format!("serialize report: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write sidecar: {e}"))?;
    Ok(path.display().to_string())
}

/// STATS-LIVEGRAPH-IMPL-1: the `--engine compare` diagnostic. PRIMARY = the SQLite `compute_module_stats`
/// answer (the unchanged shape the renderer already knows); plus a `livegraph_stats_compare` object (the
/// field-exact divergence breakdown from the SHARED `stats_compare_data` — the SAME data the cert uses)
/// + a sidecar path. The CLI prints the primary stats unchanged and one compare-summary line.
pub fn stats_compare_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    repo_root: &str,
) -> Result<Value, StorageError> {
    let data = stats_compare_data(repo_state, snapshot_uid)?;
    let compare = json!({
        "sqlite_count": data.sqlite_stats.len(),
        "livegraph_count": data.livegraph_stats.len(),
        "livegraph_class": data.livegraph_class,
        "is_exact": data.is_exact,
        "matched": data.sqlite_stats.len() - data.missing_in_livegraph.len() - data.field_mismatches.len(),
        "missing_in_livegraph": data.missing_in_livegraph,
        "extra_in_livegraph": data.extra_in_livegraph,
        "field_mismatches": data.field_mismatches,
    });
    let mut body = json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "display_name": display_name,
        "stats": data.sqlite_stats,
        "count": data.sqlite_stats.len(),
        "backend_used": "sqlite",
        "livegraph_stats_compare": compare.clone(),
    });
    // Best-effort sidecar (never fails the query).
    if let Ok(path) = write_stats_compare_sidecar(repo_root, &compare) {
        body["livegraph_stats_compare_sidecar"] = json!(path);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: the PURE fastpath/SQLite ladder ──
    // The ladder takes closures, so a PANICKING serve_sqlite/build_cert proves which path runs -- the GREEN
    // cached path must serve LiveGraph WITHOUT touching SQLite or rebuilding the cert (rule 9).

    #[test]
    fn cycles_fastpath_green_cached_serves_livegraph_without_sqlite_or_build() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            CycleCertState::ValidGreen,
            || json!({"backend_used": "livegraph"}),
            |_r| panic!("SQLite (find_cycles) must NOT be read on a GREEN cached cert"),
            || panic!("the cert must NOT be rebuilt when ValidGreen"),
        );
        assert_eq!(out["backend_used"], "livegraph");
    }

    #[test]
    fn cycles_fastpath_not_green_falls_back_to_sqlite() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            CycleCertState::ValidNotGreen,
            || panic!("must NOT serve LiveGraph when the cert is not GREEN"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || panic!("must NOT rebuild when ValidNotGreen"),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphCycleDivergence");
    }

    #[test]
    fn cycles_fastpath_precondition_unmet_falls_back_with_reason() {
        let out = cycles_fastpath_or_sqlite(
            false,
            FallbackReason::LiveGraphPartial,
            CycleCertState::ValidGreen, // ignored when the precondition is unmet
            || panic!("must NOT serve LiveGraph when the precondition is unmet"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || panic!("must NOT build the cert when the precondition is unmet"),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphPartial");
    }

    #[test]
    fn cycles_fastpath_stale_build_green_serves_livegraph() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            CycleCertState::StaleOrMissing,
            || json!({"backend_used": "livegraph"}),
            |_r| panic!("a GREEN build must serve LiveGraph, not SQLite"),
            || Some(true),
        );
        assert_eq!(out["backend_used"], "livegraph");
    }

    #[test]
    fn cycles_fastpath_stale_build_red_falls_back() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            CycleCertState::StaleOrMissing,
            || panic!("a RED build must fall back to SQLite"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || Some(false),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphCycleDivergence");
    }

    #[test]
    fn cycles_fastpath_build_failed_falls_back() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            CycleCertState::StaleOrMissing,
            || panic!("a build failure must fall back to SQLite"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || None,
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphCycleDivergence");
    }

    #[test]
    fn serve_cycles_fastpath_emits_canonical_livegraph_cycles() {
        // serve wraps the SAME canonical builder as the SQLite path -> byte-identical cycles; sorted by
        // qualified name (a/y before b/x), backend_used=livegraph, no fallback.
        let out = serve_cycles_fastpath(
            "repo",
            "disp",
            "snap",
            &[vec!["b/x".to_string(), "a/y".to_string()]],
        );
        assert_eq!(out["backend_used"], "livegraph");
        assert!(out["fallback_reason"].is_null());
        assert_eq!(out["count"], 1);
        let names: Vec<&str> = out["cycles"][0]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["qualified_name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a/y", "b/x"]);
    }

    // ── STATS-LIVEGRAPH-IMPL-1: the PURE stats fastpath/SQLite ladder ──
    // A PANICKING serve_sqlite/build_cert proves which path runs -- the GREEN cached path must serve
    // LiveGraph WITHOUT touching SQLite or rebuilding the cert.

    #[test]
    fn stats_fastpath_green_cached_serves_livegraph_without_sqlite_or_build() {
        let out = stats_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            StatsCertState::ValidGreen,
            || json!({"backend_used": "livegraph"}),
            |_r| panic!("SQLite (compute_module_stats) must NOT be read on a GREEN cached cert"),
            || panic!("the cert must NOT be rebuilt when ValidGreen"),
        );
        assert_eq!(out["backend_used"], "livegraph");
    }

    #[test]
    fn stats_fastpath_not_green_falls_back_to_sqlite() {
        let out = stats_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            StatsCertState::ValidNotGreen,
            || panic!("must NOT serve LiveGraph when the cert is not GREEN"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || panic!("must NOT rebuild when ValidNotGreen"),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphStatsDivergence");
    }

    #[test]
    fn stats_fastpath_precondition_unmet_falls_back_with_reason() {
        let out = stats_fastpath_or_sqlite(
            false,
            FallbackReason::LiveGraphPartial,
            StatsCertState::ValidGreen, // ignored when the precondition is unmet
            || panic!("must NOT serve LiveGraph when the precondition is unmet"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || panic!("must NOT build the cert when the precondition is unmet"),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphPartial");
    }

    #[test]
    fn stats_fastpath_stale_build_green_serves_livegraph() {
        let out = stats_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            StatsCertState::StaleOrMissing,
            || json!({"backend_used": "livegraph"}),
            |_r| panic!("a GREEN build must serve LiveGraph, not SQLite"),
            || Some(true),
        );
        assert_eq!(out["backend_used"], "livegraph");
    }

    #[test]
    fn stats_fastpath_stale_build_red_falls_back() {
        let out = stats_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            StatsCertState::StaleOrMissing,
            || panic!("a RED build must fall back to SQLite"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || Some(false),
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphStatsDivergence");
    }

    #[test]
    fn stats_fastpath_build_failed_falls_back() {
        let out = stats_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            StatsCertState::StaleOrMissing,
            || panic!("a build failure must fall back to SQLite"),
            |r| json!({"backend_used": "sqlite", "reason": r.as_str()}),
            || None,
        );
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphStatsDivergence");
    }

    #[test]
    fn serve_stats_fastpath_shape_is_sqlite_compatible_plus_backend_metadata() {
        let rows = vec![ModuleStatsResult {
            module: "src/a".to_string(),
            fan_in: 0,
            fan_out: 1,
            instability: 1.0,
            abstractness: 0.5,
            distance_from_main_sequence: 0.5,
            file_count: 2,
            symbol_count: 3,
        }];
        let out = serve_stats_fastpath("repo", "disp", "snap", &rows);
        assert_eq!(out["backend_used"], "livegraph");
        assert!(out["fallback_reason"].is_null());
        assert_eq!(out["count"], 1);
        assert_eq!(out["stats"][0]["module"], "src/a");
        assert_eq!(out["stats"][0]["fan_out"], 1);
        assert_eq!(out["stats"][0]["symbol_count"], 3);
        // The DTO field names mirror the SQLite ModuleStatsResult exactly (the renderer is untouched).
        assert!(out["stats"][0]["distance_from_main_sequence"].is_number());
    }

    #[test]
    fn livegraph_module_stats_dto_derives_metrics_via_shared_helper() {
        // The mapped DTO's floats MUST equal the shared `martin_metrics` on the same raw counts (RISK-5:
        // the byte-identity / no-spurious-RED property). fan_in=1, fan_out=3 -> instability=0.75;
        // abstract=1, type=4 -> abstractness=0.25; distance=|0.25+0.75-1|=0.
        let rows = [ModuleStatRow {
            module: "m".to_string(),
            fan_in: 1,
            fan_out: 3,
            file_count: 5,
            symbol_count: 9,
            abstract_count: 1,
            type_count: 4,
        }];
        let dto = livegraph_module_stats_dto(&rows);
        let (i, a, d) = martin_metrics(1, 3, 1, 4);
        assert_eq!(dto.len(), 1);
        assert_eq!(dto[0].instability, i);
        assert_eq!(dto[0].abstractness, a);
        assert_eq!(dto[0].distance_from_main_sequence, d);
        assert_eq!(dto[0].instability, 0.75);
        assert_eq!(dto[0].abstractness, 0.25);
        assert_eq!(dto[0].distance_from_main_sequence, 0.0);
        // The integer fields pass through unchanged.
        assert_eq!(dto[0].fan_in, 1);
        assert_eq!(dto[0].file_count, 5);
        assert_eq!(dto[0].symbol_count, 9);
    }

    fn synthetic_root() -> String {
        format!(
            "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    #[test]
    fn preload_into_real_index_populates_livegraph() {
        // Daemon-side decode → ingest → feed of the committed REAL index.scip (not hand-built).
        let root = synthetic_root();
        let scip = format!("{root}/index.scip");
        let mut lg = LiveGraph::new();
        let summary =
            preload_into(&mut lg, "synthetic", "synthetic", &scip, &root, "").expect("preload");
        assert!(summary["nodes"].as_u64().unwrap() > 0, "real nodes loaded");
        assert!(
            summary["value_facts"].as_u64().unwrap() > 0,
            "real complexity value facts loaded"
        );
        assert!(
            lg.partition_epoch("synthetic").is_some(),
            "partition resident after preload"
        );
    }

    #[test]
    fn import_view_body_names_module_cycle_trust_fields_explicitly() {
        use repo_graph_livegraph::import_view::{
            ImportEdgeView, ImportObservationView, LiveImportView,
        };
        use serde_json::json;
        let view = LiveImportView {
            edges: vec![ImportEdgeView {
                src_file: "a/src/x.ts".to_string(),
                dst_file: "a/src/y.ts".to_string(),
                basis: "AstImport".to_string(),
                raw_specifier: Some("./y".to_string()),
            }],
            observations: vec![
                ImportObservationView {
                    source_file: "a/src/x.ts".to_string(),
                    raw_specifier: "react".to_string(),
                    class: "ExternalNonLocal".to_string(),
                    blocking: false,
                },
                ImportObservationView {
                    source_file: "a/src/x.ts".to_string(),
                    raw_specifier: "@scope/wslocal".to_string(),
                    class: "WorkspaceLocalUnedgeable".to_string(),
                    blocking: true,
                },
            ],
        };
        let body = import_view_body(
            &view,
            "IncompleteImportClasses",
            "Exact",
            json!({"module_aggregated": true}),
            "Fresh",
            vec![],
            vec![],
        );
        // The module-cycle trust fields are NAMED after their source (the ratified wording correction).
        assert_eq!(body["module_cycle_completeness"], "IncompleteImportClasses");
        assert_eq!(body["module_cycle_answer_class"], "Exact");
        assert!(body["module_cycle_import_scope"].is_object());
        // NEVER a bare answer_class / completeness implying the import LISTING is complete.
        assert!(body.get("answer_class").is_none(), "no bare answer_class");
        assert!(body.get("completeness").is_none(), "no bare completeness");
        // EDGES (facts) + OBSERVATIONS (evidence), separated.
        assert_eq!(body["edge_count"], 1);
        assert_eq!(body["edges"][0]["basis"], "AstImport");
        assert_eq!(body["edges"][0]["src_file"], "a/src/x.ts");
        assert_eq!(body["observation_count"], 2);
        assert_eq!(body["blocking_observation_count"], 1);
        assert_eq!(body["observation_class_counts"]["ExternalNonLocal"], 1);
        assert_eq!(
            body["observation_class_counts"]["WorkspaceLocalUnedgeable"],
            1
        );
    }

    #[test]
    fn import_view_body_unavailable_is_honest() {
        let body = import_view_body_unavailable();
        assert_eq!(body["module_cycle_completeness"], "Unknown");
        assert_eq!(body["module_cycle_answer_class"], "Unavailable");
        assert_eq!(body["edge_count"], 0);
        assert_eq!(body["observation_count"], 0);
        assert_eq!(body["degradation_reasons"][0], "LiveGraphUnavailable");
    }

    #[test]
    fn imports_compare_sidecar_directional_verdicts() {
        use repo_graph_livegraph::import_view::{
            FilePartitionStatus, ImportEdgeView, LiveImportView,
        };
        let met = FilePartitionStatus {
            partition_id: "app".to_string(),
            resident: true,
            fresh: true,
            ts_primary: true,
        };
        let edge = |dst: &str, basis: &str| ImportEdgeView {
            src_file: "app/src/x.ts".to_string(),
            dst_file: dst.to_string(),
            basis: basis.to_string(),
            raw_specifier: None,
        };
        // NoLossEquivalent: SQLite {y.ts} == LiveGraph edge {y.ts}.
        let view = LiveImportView {
            edges: vec![edge("app/src/y.ts", "AstImport")],
            observations: vec![],
        };
        let s = imports_compare_sidecar(&["app/src/y.ts".to_string()], &view, Some(&met));
        assert_eq!(s["status"], "NoLossEquivalent");
        assert_eq!(s["matched"][0], "app/src/y.ts");
        assert_eq!(s["missing_in_livegraph"].as_array().unwrap().len(), 0);
        // NoLossLivegraphSuperset: LiveGraph has an extra alias edge SQLite lacks.
        let view2 = LiveImportView {
            edges: vec![
                edge("app/src/y.ts", "AstImport"),
                edge("app/src/z.ts", "AstImportTsconfigPathResolved"),
            ],
            observations: vec![],
        };
        let s2 = imports_compare_sidecar(&["app/src/y.ts".to_string()], &view2, Some(&met));
        assert_eq!(s2["status"], "NoLossLivegraphSuperset");
        assert_eq!(s2["extra_livegraph_edges"][0]["dst_file"], "app/src/z.ts");
        // Regression: SQLite has y.ts but the (precondition-met) LiveGraph lacks it -- a real loss.
        let empty = LiveImportView {
            edges: vec![],
            observations: vec![],
        };
        let s3 = imports_compare_sidecar(&["app/src/y.ts".to_string()], &empty, Some(&met));
        assert_eq!(s3["status"], "Regression");
        assert_eq!(s3["missing_in_livegraph"][0], "app/src/y.ts");
        // FallbackPreconditionUnmet: no resident TS partition -> fallback, NEVER a regression (the language gate).
        let s4 = imports_compare_sidecar(&["app/src/y.ts".to_string()], &empty, None);
        assert_eq!(s4["status"], "FallbackPreconditionUnmet");
        assert!(s4["precondition"].is_null());
    }

    #[test]
    fn aggregate_readiness_verdicts() {
        use repo_graph_livegraph::import_view::{
            FilePartitionStatus, ImportEdgeView, LiveImportView,
        };
        use repo_graph_storage::queries::BulkImportRow;
        use std::collections::BTreeMap;
        let row = |src: &str, tgt: &str, res: &str| BulkImportRow {
            source_file: src.to_string(),
            target_file: tgt.to_string(),
            kind: "FILE".to_string(),
            subtype: Some("SOURCE".to_string()),
            resolution: Some(res.to_string()),
        };
        let edge = |src: &str, dst: &str| ImportEdgeView {
            src_file: src.to_string(),
            dst_file: dst.to_string(),
            basis: "AstImport".to_string(),
            raw_specifier: None,
        };
        let met: BTreeMap<String, FilePartitionStatus> = [(
            "a.ts".to_string(),
            FilePartitionStatus {
                partition_id: "app".to_string(),
                resident: true,
                fresh: true,
                ts_primary: true,
            },
        )]
        .into_iter()
        .collect();
        let bulk = vec![row("a.ts", "b.ts", "static")];
        // GREEN: SQLite{b.ts} == LiveGraph edge{b.ts}, precondition met.
        let view = LiveImportView {
            edges: vec![edge("a.ts", "b.ts")],
            observations: vec![],
        };
        let r = aggregate_readiness("r1", "t", "snap", &bulk, &view, &met);
        assert_eq!(r["verdict"], "GREEN");
        assert_eq!(r["metrics"]["files_precondition_met"], 1);
        assert_eq!(r["metrics"]["files_regression"], 0);
        // RED regression: SQLite{b.ts} but LiveGraph empty, precondition met.
        let empty_view = LiveImportView {
            edges: vec![],
            observations: vec![],
        };
        let r2 = aggregate_readiness("r1", "t", "snap", &bulk, &empty_view, &met);
        assert_eq!(r2["verdict"], "RED");
        assert_eq!(r2["metrics"]["files_regression"], 1);
        assert_eq!(r2["metrics"]["missing_in_livegraph_total"], 1);
        // YELLOW fallback: SQLite{b.ts}, LiveGraph empty, precondition UNMET (no map entry).
        let no_precond: BTreeMap<String, FilePartitionStatus> = BTreeMap::new();
        let r3 = aggregate_readiness("r1", "t", "snap", &bulk, &empty_view, &no_precond);
        assert_eq!(r3["verdict"], "YELLOW");
        assert_eq!(r3["metrics"]["files_fallback_required"], 1);
        assert_eq!(r3["metrics"]["files_regression"], 0);
        // RED unknown: a FILE-target import that is non-external AND non-static.
        let bulk_unknown = vec![row("a.ts", "b.ts", "unresolved")];
        let r4 = aggregate_readiness("r1", "t", "snap", &bulk_unknown, &empty_view, &met);
        assert_eq!(r4["verdict"], "RED");
        assert_eq!(r4["metrics"]["unknown_total"], 1);
    }

    #[test]
    fn imports_auto_body_serves_livegraph_or_falls_back() {
        use repo_graph_livegraph::import_view::{
            FilePartitionStatus, ImportEdgeView, LiveImportView,
        };
        use repo_graph_storage::queries::ImportResult;
        let sqlite_row = |target: &str, resolution: &str| ImportResult {
            node_id: String::new(),
            symbol: target.to_string(),
            kind: "FILE".to_string(),
            subtype: Some("SOURCE".to_string()),
            file: target.to_string(),
            line: None,
            column: None,
            edge_type: Some("IMPORTS".to_string()),
            resolution: Some(resolution.to_string()),
            evidence: vec![],
            depth: 1,
        };
        let edge = |dst: &str, basis: &str| ImportEdgeView {
            src_file: "a.ts".to_string(),
            dst_file: dst.to_string(),
            basis: basis.to_string(),
            raw_specifier: None,
        };
        let met = FilePartitionStatus {
            partition_id: "app".to_string(),
            resident: true,
            fresh: true,
            ts_primary: true,
        };
        let sqlite = vec![sqlite_row("b.ts", "static")];
        // SERVE LIVEGRAPH: SQLite{b.ts} subset of LiveGraph{b.ts + c.ts (alias extra)}, precondition met.
        let view = LiveImportView {
            edges: vec![
                edge("b.ts", "AstImport"),
                edge("c.ts", "AstImportTsconfigPathResolved"),
            ],
            observations: vec![],
        };
        let r = imports_auto_body("a.ts", &sqlite, &view, Some(&met));
        assert_eq!(r["backend_used"], "livegraph");
        assert!(r["fallback_reason"].is_null());
        assert_eq!(r["count"], 2); // the extra alias edge IS served (D4)
        assert_eq!(r["imports"][1]["symbol"], "c.ts");
        assert_eq!(r["imports"][0]["kind"], "FILE");
        // FALLBACK precondition unmet (no resident TS partition).
        let r2 = imports_auto_body("a.ts", &sqlite, &view, None);
        assert_eq!(r2["backend_used"], "sqlite");
        assert_eq!(r2["fallback_reason"], "LiveGraphUnavailable");
        assert_eq!(r2["count"], 1);
        // FALLBACK regression: SQLite{b.ts} but LiveGraph lacks b.ts, precondition met.
        let view_missing = LiveImportView {
            edges: vec![edge("c.ts", "AstImport")],
            observations: vec![],
        };
        let r3 = imports_auto_body("a.ts", &sqlite, &view_missing, Some(&met));
        assert_eq!(r3["backend_used"], "sqlite");
        assert_eq!(r3["fallback_reason"], "LiveGraphImportRegression");
        // FALLBACK unknown: a SQLite FILE-target non-static row, precondition met.
        let sqlite_unknown = vec![sqlite_row("b.ts", "unresolved")];
        let r4 = imports_auto_body("a.ts", &sqlite_unknown, &view, Some(&met));
        assert_eq!(r4["backend_used"], "sqlite");
        assert_eq!(r4["fallback_reason"], "LiveGraphImportUnknown");
        // FALLBACK stale: precondition partition present but not Fresh.
        let stale = FilePartitionStatus {
            partition_id: "app".to_string(),
            resident: true,
            fresh: false,
            ts_primary: true,
        };
        let r5 = imports_auto_body("a.ts", &sqlite, &view, Some(&stale));
        assert_eq!(r5["fallback_reason"], "LiveGraphStale");
    }

    #[test]
    fn callers_auto_or_sqlite_lazy_served_eager_fallback() {
        use repo_graph_storage::queries::{CallerResult, ResolvedSymbol};
        let target = ResolvedSymbol {
            stable_key: "r:a#foo".to_string(),
            name: "foo".to_string(),
            qualified_name: None,
            kind: "SYMBOL".to_string(),
            subtype: None,
            file: None,
            line: None,
            column: None,
        };
        let sentinel = || -> Result<Vec<CallerResult>, StorageError> {
            Ok(vec![CallerResult {
                stable_key: "r:s#sql".to_string(),
                name: "sql".to_string(),
                qualified_name: None,
                kind: String::new(),
                subtype: None,
                file: None,
                line: None,
                column: None,
                edge_type: "CALLS".to_string(),
                resolution: "sqlite".to_string(),
            }])
        };
        // SERVED: Some(keys) -> LiveGraph; the PANICKING closure is NEVER called.
        let served = callers_auto_or_sqlite(
            &target,
            Some(vec!["r:b#bar".to_string()]),
            None,
            || -> Result<Vec<CallerResult>, StorageError> {
                panic!("SQLite read on the LiveGraph-served path")
            },
        )
        .unwrap();
        assert_eq!(served["backend_used"], "livegraph");
        assert!(served["fallback_reason"].is_null());
        assert_eq!(served["callers"][0]["stable_key"], "r:b#bar");
        // FALLBACK: None + reason -> the closure RUNS (its sentinel appears), backend=sqlite + reason.
        let fb = callers_auto_or_sqlite(
            &target,
            None,
            Some(FallbackReason::LiveGraphUnavailable),
            sentinel,
        )
        .unwrap();
        assert_eq!(fb["backend_used"], "sqlite");
        assert_eq!(fb["fallback_reason"], "LiveGraphUnavailable");
        assert_eq!(fb["callers"][0]["stable_key"], "r:s#sql");
        // SQLITE (Engine::Sqlite): None + no reason -> the closure RUNS, no fallback_reason.
        let sq = callers_auto_or_sqlite(&target, None, None, sentinel).unwrap();
        assert_eq!(sq["backend_used"], "sqlite");
        assert!(sq["fallback_reason"].is_null());
    }

    #[test]
    fn path_auto_or_sqlite_lazy_on_served_including_no_path_exact() {
        // SERVED no-path EXACT: Some((false, [])) -> LiveGraph; the PANICKING closure is NEVER called.
        let served = path_auto_or_sqlite(
            "r1",
            "snap",
            Some((false, vec![])),
            None,
            || -> Result<Value, StorageError> { panic!("SQLite read on the served no-path path") },
        )
        .unwrap();
        assert_eq!(served["backend_used"], "livegraph");
        assert_eq!(served["found"], false);
        // FALLBACK: None + reason -> the closure RUNS (its sentinel sqlite_value is stamped + served).
        let fb = path_auto_or_sqlite(
            "r1",
            "snap",
            None,
            Some(FallbackReason::LiveGraphPartial),
            || -> Result<Value, StorageError> {
                Ok(json!({"path": {"found": true}, "found": true}))
            },
        )
        .unwrap();
        assert_eq!(fb["backend_used"], "sqlite");
        assert_eq!(fb["fallback_reason"], "LiveGraphPartial");
        assert_eq!(fb["found"], true);
    }

    #[test]
    fn imports_fastpath_ladder() {
        use repo_graph_livegraph::import_view::{
            FilePartitionStatus, ImportEdgeView, LiveImportView,
        };
        use repo_graph_storage::queries::ImportResult;
        let met = FilePartitionStatus {
            partition_id: "app".to_string(),
            resident: true,
            fresh: true,
            ts_primary: true,
        };
        let view = LiveImportView {
            edges: vec![ImportEdgeView {
                src_file: "a.ts".to_string(),
                dst_file: "b.ts".to_string(),
                basis: "AstImport".to_string(),
                raw_specifier: None,
            }],
            observations: vec![],
        };
        let panic_sqlite = || -> Vec<ImportResult> { panic!("SQLite read on the GREEN fastpath") };
        let empty_sqlite = Vec::<ImportResult>::new;
        // GREEN cert + precondition met -> FASTPATH; the panicking find_imports + build are NEVER called.
        let g = imports_fastpath_or_compare(
            "a.ts",
            &view,
            Some(&met),
            ImportCertState::ValidGreen,
            panic_sqlite,
            || panic!("build on a valid cert"),
        );
        assert_eq!(g["backend_used"], "livegraph");
        assert_eq!(g["comparison"]["source"], "repo_no_loss_certificate");
        assert_eq!(g["count"], 1);
        // RED cert (ValidNotGreen) -> COMPARE-ON-CALL (find_imports runs -> per-call comparison, NOT the cert).
        let r = imports_fastpath_or_compare(
            "a.ts",
            &view,
            Some(&met),
            ImportCertState::ValidNotGreen,
            empty_sqlite,
            || panic!("build on a valid cert"),
        );
        assert!(r["comparison"]["sqlite_resolved_local"].is_number());
        assert!(r["comparison"]["source"].is_null());
        // STALE/MISSING + build -> GREEN -> FASTPATH (find_imports NEVER called).
        let s = imports_fastpath_or_compare(
            "a.ts",
            &view,
            Some(&met),
            ImportCertState::StaleOrMissing,
            panic_sqlite,
            || Some(true),
        );
        assert_eq!(s["comparison"]["source"], "repo_no_loss_certificate");
        // STALE/MISSING + build -> RED -> compare-on-call.
        let s2 = imports_fastpath_or_compare(
            "a.ts",
            &view,
            Some(&met),
            ImportCertState::StaleOrMissing,
            empty_sqlite,
            || Some(false),
        );
        assert!(s2["comparison"]["sqlite_resolved_local"].is_number());
        // BUILD FAILURE (None) -> compare-on-call (safe fallback, NOT the cert).
        let bf = imports_fastpath_or_compare(
            "a.ts",
            &view,
            Some(&met),
            ImportCertState::StaleOrMissing,
            empty_sqlite,
            || None,
        );
        assert!(bf["comparison"]["sqlite_resolved_local"].is_number());
        // NON-TS (precondition unmet) -> SQLite fallback regardless of the GREEN cert; build NEVER called.
        let nt = imports_fastpath_or_compare(
            "a.cpp",
            &view,
            None,
            ImportCertState::ValidGreen,
            empty_sqlite,
            || panic!("build on a non-TS file"),
        );
        assert_eq!(nt["backend_used"], "sqlite");
    }

    #[test]
    fn compare_keys_buckets_missing_and_extra() {
        let sqlite = vec!["a".to_string(), "b".to_string()];
        let lg = Some((AnswerClass::Exact, vec!["b".to_string(), "c".to_string()]));
        let r = compare_keys("sym", "callers", &sqlite, lg);
        assert_eq!(r.sqlite_count, 2);
        assert_eq!(r.livegraph_count, 2);
        assert_eq!(r.missing_in_livegraph, vec!["a".to_string()]); // in sqlite, not livegraph
        assert_eq!(r.extra_in_livegraph, vec!["c".to_string()]); // in livegraph, not sqlite
        assert!(!r.partition_unavailable);
        assert!(r.trust_class_mismatch.is_empty()); // Exact
    }

    #[test]
    fn compare_keys_partition_unavailable_when_no_livegraph() {
        let r = compare_keys("sym", "callers", &["a".to_string()], None);
        assert!(r.partition_unavailable);
        assert_eq!(r.livegraph_count, 0);
        assert_eq!(r.livegraph_class, "Unavailable");
    }

    #[test]
    fn compare_keys_trust_class_mismatch_when_not_exact() {
        let lg = Some((AnswerClass::Partial, vec!["a".to_string()]));
        let r = compare_keys("sym", "callers", &["a".to_string()], lg);
        assert!(!r.trust_class_mismatch.is_empty());
    }

    // PATH-LIVEGRAPH-DEFAULT-1: the `Auto` decision. Serve LiveGraph ONLY when Exact + Fresh + TS-only;
    // otherwise SQLite fallback with a labelled reason.

    /// A path node WITH display metadata (resolvable `file:line`).
    fn pnode(key: &str, line: u32) -> PathNodeDisplay {
        PathNodeDisplay {
            key: key.to_string(),
            location: Some((format!("{key}.ts"), line)),
        }
    }
    /// A path node WITHOUT display metadata (no recoverable range).
    fn pnode_no_loc(key: &str) -> PathNodeDisplay {
        PathNodeDisplay {
            key: key.to_string(),
            location: None,
        }
    }
    fn lg_path(
        class: AnswerClass,
        freshness: FreshnessState,
        ts_only: bool,
        found: bool,
        nodes: Vec<PathNodeDisplay>,
    ) -> Option<LgPathAuto> {
        Some(LgPathAuto {
            class,
            freshness,
            ts_only,
            found,
            nodes,
        })
    }

    #[test]
    fn path_auto_serves_livegraph_when_exact_fresh_ts() {
        // A found Exact/Fresh/TS path WITH display metadata is served from LiveGraph.
        let nodes = vec![pnode("a", 1), pnode("b", 2)];
        let found = path_auto_outcome(lg_path(
            AnswerClass::Exact,
            FreshnessState::Fresh,
            true,
            true,
            nodes.clone(),
        ));
        assert_eq!(found, Ok((true, nodes)));
        // D3: an EXACT no-path (traversal completeness proven) is ALSO served — no nodes to render, so
        // the display gate is vacuous.
        let no_path = path_auto_outcome(lg_path(
            AnswerClass::Exact,
            FreshnessState::Fresh,
            true,
            false,
            vec![],
        ));
        assert_eq!(no_path, Ok((false, vec![])));
    }

    #[test]
    fn path_auto_falls_back_on_partial() {
        // D3: a PARTIAL no-path (incomplete traversal) MUST fall back to SQLite, never an exact-empty.
        let r = path_auto_outcome(lg_path(
            AnswerClass::Partial,
            FreshnessState::Fresh,
            true,
            false,
            vec![],
        ));
        assert_eq!(r, Err(FallbackReason::LiveGraphPartial));
    }

    #[test]
    fn path_auto_falls_back_on_stale() {
        // Freshness is checked BEFORE class, so a Stale (even Exact) answer reports LiveGraphStale.
        let r = path_auto_outcome(lg_path(
            AnswerClass::Exact,
            FreshnessState::Stale,
            true,
            true,
            vec![pnode("a", 1), pnode("b", 2)],
        ));
        assert_eq!(r, Err(FallbackReason::LiveGraphStale));
    }

    #[test]
    fn path_auto_falls_back_on_unsupported_language() {
        // Exact + Fresh but NOT TS-only -> fall back (D2 TS-only scope).
        let r = path_auto_outcome(lg_path(
            AnswerClass::Exact,
            FreshnessState::Fresh,
            false,
            true,
            vec![pnode("a", 1), pnode("b", 2)],
        ));
        assert_eq!(r, Err(FallbackReason::LiveGraphUnsupportedLanguage));
    }

    #[test]
    fn path_auto_falls_back_on_missing_display_metadata() {
        // Exact + Fresh + TS, but a rendered node lacks file:line -> fall back rather than render `:0`
        // (PATH-LIVEGRAPH-DEFAULT-1 invariant; never serve a degraded default human path).
        let r = path_auto_outcome(lg_path(
            AnswerClass::Exact,
            FreshnessState::Fresh,
            true,
            true,
            vec![pnode("a", 1), pnode_no_loc("b")],
        ));
        assert_eq!(r, Err(FallbackReason::LiveGraphDisplayMetadataUnavailable));
    }

    #[test]
    fn path_auto_falls_back_on_unavailable() {
        // No LiveGraph for this repo -> LiveGraphUnavailable.
        let r = path_auto_outcome(None);
        assert_eq!(r, Err(FallbackReason::LiveGraphUnavailable));
    }
}
