//! LIVEGRAPH-INTEGRATION-1B: dev-only preload of a SUPPLIED SCIP index into a repo's in-memory
//! LiveGraph. Decode + ingest + feed ONLY — the daemon does NOT run scip-typescript or do package
//! discovery / refresh orchestration (that is LIVEGRAPH-INTEGRATION-1C).

use repo_graph_ir::CanonicalKey;
use repo_graph_livegraph::{FileImportCyclesAnswer, LiveGraph};
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
    /// A rendered `path` node lacks display metadata (`file:line` not recoverable from the resident IR),
    /// so the DEFAULT (`Auto`) path falls back to SQLite rather than render `:0` (PATH-LIVEGRAPH-DEFAULT-1).
    /// Distinct from `RenderUnsupported`: the answer is trust-complete, only its presentation is.
    LiveGraphDisplayMetadataUnavailable,
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
            FallbackReason::LiveGraphDisplayMetadataUnavailable => {
                "LiveGraphDisplayMetadataUnavailable"
            }
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
    sqlite_value: Value,
    repo_root: &str,
) -> Value {
    let sqlite_found = sqlite_value
        .pointer("/path/found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // Compare by SYMBOL NAME: SQLite path node_ids are DB UUIDs while LiveGraph keys are stable keys —
    // different key spaces. Name-based comparison normalizes them so a representation difference is NOT
    // mistaken for a real path difference. (Approximation: a full cross-key-space alignment is a
    // follow-up; `DifferentPath` then reflects a genuine route difference, e.g. SQLite's IMPORTS edges.)
    let sqlite_names: Vec<String> = sqlite_value
        .pointer("/path/path")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("symbol").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut sqlite_response = sqlite_value;
    match engine {
        // `--engine sqlite` FORCES SQLite (the proven path), regardless of LiveGraph state.
        Engine::Sqlite => {
            sqlite_response["backend_used"] = json!("sqlite");
            sqlite_response["fallback_reason"] = Value::Null;
            sqlite_response
        }
        // DEFAULT (PATH-LIVEGRAPH-DEFAULT-1): serve LiveGraph only when Exact + Fresh + TS-only; else
        // labelled SQLite fallback. The D3 no-path rule is encoded by `path()`'s class (Exact = proven
        // complete found-OR-no-path; Partial = incomplete), so `path_auto_outcome` needs no special case.
        // Mirrors the callers/callees `Auto` arm: served LiveGraph carries backend_used only (the explicit
        // `--engine livegraph` branch keeps trust_class/freshness as a diagnostic surface).
        Engine::Auto => match path_auto_outcome(livegraph_path(repo_state, from_key, to_key)) {
            Ok((found, nodes)) => json!({
                "repo_uid": repo_uid,
                "snapshot_uid": snapshot_uid,
                "found": found,
                "path": livegraph_path_result(found, &nodes),
                "backend_used": "livegraph",
                "fallback_reason": Value::Null,
            }),
            Err(reason) => {
                sqlite_response["backend_used"] = json!("sqlite");
                sqlite_response["fallback_reason"] = json!(reason.as_str());
                sqlite_response
            }
        },
        Engine::LiveGraph => match livegraph_path(repo_state, from_key, to_key) {
            Some(a) => json!({
                "repo_uid": repo_uid,
                "snapshot_uid": snapshot_uid,
                "found": a.found,
                "path": livegraph_path_result(a.found, &a.nodes),
                "backend_used": "livegraph",
                "fallback_reason": Value::Null,
                "trust_class": format!("{:?}", a.class),
                "freshness": format!("{:?}", a.freshness),
            }),
            None => {
                sqlite_response["backend_used"] = json!("sqlite");
                sqlite_response["fallback_reason"] =
                    json!(FallbackReason::LiveGraphUnavailable.as_str());
                sqlite_response
            }
        },
        Engine::Compare => {
            // Normalize LiveGraph keys to names for an apples-to-apples comparison (see above).
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
            sqlite_response
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
    let (class, freshness, missing, reasons, cycles) = match guard.as_ref() {
        Some(lg) => {
            let env = lg.file_import_cycles();
            let cycles = env.data().map(file_import_cycles_json).unwrap_or_default();
            (
                format!("{:?}", env.class()),
                format!("{:?}", env.freshness()),
                env.missing_partitions().to_vec(),
                env.degradation_reasons()
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>(),
                cycles,
            )
        }
        // No LiveGraph for this repo (never preloaded/refreshed) -> Unavailable, NOT a SQLite fallback.
        None => (
            "Unavailable".to_string(),
            "Unavailable".to_string(),
            Vec::new(),
            vec!["LiveGraphUnavailable".to_string()],
            Vec::new(),
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
        "scope": "CapturedResolvedRelativeIntraPartition",
        "answer_class": class,
        "freshness": freshness,
        "missing_partitions": missing,
        "degradation_reasons": reasons,
    })
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
