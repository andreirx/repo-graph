//! LIVEGRAPH-INTEGRATION-1B: dev-only preload of a SUPPLIED SCIP index into a repo's in-memory
//! LiveGraph. Decode + ingest + feed ONLY — the daemon does NOT run scip-typescript or do package
//! discovery / refresh orchestration (that is LIVEGRAPH-INTEGRATION-1C).

use repo_graph_livegraph::LiveGraph;
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use repo_graph_storage::queries::{CalleeResult, CallerResult, ResolvedSymbol};
use repo_graph_trust_model::{AnswerClass, FreshnessState, Granularity, LanguageSupport};
use serde::Serialize;
use serde_json::{json, Value};

use crate::state::RepoState;

/// Decode a supplied `index.scip`, ingest it into a `PartitionIr` + complexity map, and feed both
/// into `lg` (epoch-stamped). Pure over the runtime (no daemon state) so it is unit-testable against
/// the committed fixture. Returns a summary `{partition_id, nodes, edges, value_facts, epoch}`.
pub fn preload_into(
    lg: &mut LiveGraph,
    repo_uid: &str,
    partition_id: &str,
    scip_path: &str,
    source_root: &str,
) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(scip_path).map_err(|e| format!("read scip '{scip_path}': {e}"))?;
    let index = decode_index(&bytes).map_err(|e| format!("decode scip '{scip_path}': {e}"))?;
    // The daemon DECODES + ingests a supplied index; it does NOT run the indexer (1C).
    let outcome = ingest_partition(
        &index,
        source_root,
        repo_uid,
        partition_id,
        "scip-typescript",
        "preload",
        "preload",
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
pub fn preload_partition(
    repo_state: &RepoState,
    repo_uid: &str,
    partition_id: &str,
    scip_path: &str,
    source_root: &str,
) -> Result<serde_json::Value, String> {
    let mut guard = repo_state.livegraph.write();
    let lg = guard.get_or_insert_with(LiveGraph::new);
    preload_into(lg, repo_uid, partition_id, scip_path, source_root)
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
    /// The LiveGraph engine errored (reserved; the callers/callees query path does not error today).
    LiveGraphError,
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
            FallbackReason::LiveGraphError => "LiveGraphError",
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

fn ts_only(langs: &std::collections::BTreeSet<LanguageSupport>) -> bool {
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
    sqlite_callers: Vec<CallerResult>,
    symbol: &str,
    repo_root: &str,
) -> Value {
    match engine {
        Engine::Sqlite => callers_value(target, sqlite_callers, "sqlite", None),
        Engine::Auto => {
            match auto_outcome(livegraph_callers_auto(repo_state, &target.stable_key)) {
                Ok(keys) => {
                    callers_value(target, caller_results_from_keys(&keys), "livegraph", None)
                }
                Err(reason) => callers_value(target, sqlite_callers, "sqlite", Some(reason)),
            }
        }
        Engine::LiveGraph => match livegraph_caller_keys(repo_state, &target.stable_key) {
            Some((_class, keys)) => {
                callers_value(target, caller_results_from_keys(&keys), "livegraph", None)
            }
            // Strict LiveGraph still falls back when the partition is unavailable (existing 1B
            // behavior; NOT a new strict-failure mode in this slice).
            None => callers_value(
                target,
                sqlite_callers,
                "sqlite",
                Some(FallbackReason::LiveGraphUnavailable),
            ),
        },
        Engine::Compare => {
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
            v
        }
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
    sqlite_callees: Vec<CalleeResult>,
    symbol: &str,
    repo_root: &str,
) -> Value {
    match engine {
        Engine::Sqlite => callees_value(target, sqlite_callees, "sqlite", None),
        Engine::Auto => {
            match auto_outcome(livegraph_callees_auto(repo_state, &target.stable_key)) {
                Ok(keys) => {
                    callees_value(target, callee_results_from_keys(&keys), "livegraph", None)
                }
                Err(reason) => callees_value(target, sqlite_callees, "sqlite", Some(reason)),
            }
        }
        Engine::LiveGraph => match livegraph_callee_keys(repo_state, &target.stable_key) {
            Some((_class, keys)) => {
                callees_value(target, callee_results_from_keys(&keys), "livegraph", None)
            }
            None => callees_value(
                target,
                sqlite_callees,
                "sqlite",
                Some(FallbackReason::LiveGraphUnavailable),
            ),
        },
        Engine::Compare => {
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
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            preload_into(&mut lg, "synthetic", "synthetic", &scip, &root).expect("preload");
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
}
