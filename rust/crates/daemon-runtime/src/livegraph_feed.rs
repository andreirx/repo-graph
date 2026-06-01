//! LIVEGRAPH-INTEGRATION-1B: dev-only preload of a SUPPLIED SCIP index into a repo's in-memory
//! LiveGraph. Decode + ingest + feed ONLY — the daemon does NOT run scip-typescript or do package
//! discovery / refresh orchestration (that is LIVEGRAPH-INTEGRATION-1C).

use repo_graph_livegraph::LiveGraph;
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use repo_graph_storage::queries::{CalleeResult, CallerResult, ResolvedSymbol};
use repo_graph_trust_model::{AnswerClass, Granularity, LanguageSupport};
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

/// Engine selector (S2). Default `Sqlite` (byte-compatible current behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Sqlite,
    LiveGraph,
    Compare,
}

impl Engine {
    /// Parse the `engine` param; anything other than `livegraph`/`compare` is `Sqlite` (default).
    pub fn parse(s: Option<&str>) -> Engine {
        match s {
            Some("livegraph") => Engine::LiveGraph,
            Some("compare") => Engine::Compare,
            _ => Engine::Sqlite,
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

/// Build the `callers` response for the selected engine. `Sqlite` (and the LiveGraph-miss fallback)
/// return the byte-compatible `{target, callers, count}`. `Compare` returns the SQLite answer plus a
/// `livegraph_compare` report + `livegraph_compare_sidecar` path.
pub fn callers_engine_response(
    engine: Engine,
    repo_state: &RepoState,
    target: &ResolvedSymbol,
    sqlite_callers: Vec<CallerResult>,
    symbol: &str,
    repo_root: &str,
) -> Value {
    match engine {
        Engine::Sqlite => {
            let count = sqlite_callers.len();
            json!({ "target": target, "callers": sqlite_callers, "count": count })
        }
        Engine::LiveGraph => match livegraph_caller_keys(repo_state, &target.stable_key) {
            Some((_class, keys)) => {
                let results = caller_results_from_keys(&keys);
                let count = results.len();
                json!({ "target": target, "callers": results, "count": count })
            }
            None => {
                let count = sqlite_callers.len();
                json!({ "target": target, "callers": sqlite_callers, "count": count })
            }
        },
        Engine::Compare => {
            let sqlite_keys: Vec<String> = sqlite_callers
                .iter()
                .map(|c| c.stable_key.clone())
                .collect();
            let lg = livegraph_caller_keys(repo_state, &target.stable_key);
            let report = compare_keys(symbol, "callers", &sqlite_keys, lg);
            let sidecar = write_compare_sidecar(repo_root, &report).ok();
            let count = sqlite_callers.len();
            let mut v = json!({ "target": target, "callers": sqlite_callers, "count": count });
            v["livegraph_compare"] = serde_json::to_value(&report).unwrap_or(Value::Null);
            if let Some(p) = sidecar {
                v["livegraph_compare_sidecar"] = json!(p);
            }
            v
        }
    }
}

/// Build the `callees` response for the selected engine (symmetric to [`callers_engine_response`]).
pub fn callees_engine_response(
    engine: Engine,
    repo_state: &RepoState,
    target: &ResolvedSymbol,
    sqlite_callees: Vec<CalleeResult>,
    symbol: &str,
    repo_root: &str,
) -> Value {
    match engine {
        Engine::Sqlite => {
            let count = sqlite_callees.len();
            json!({ "target": target, "callees": sqlite_callees, "count": count })
        }
        Engine::LiveGraph => match livegraph_callee_keys(repo_state, &target.stable_key) {
            Some((_class, keys)) => {
                let results = callee_results_from_keys(&keys);
                let count = results.len();
                json!({ "target": target, "callees": results, "count": count })
            }
            None => {
                let count = sqlite_callees.len();
                json!({ "target": target, "callees": sqlite_callees, "count": count })
            }
        },
        Engine::Compare => {
            let sqlite_keys: Vec<String> = sqlite_callees
                .iter()
                .map(|c| c.stable_key.clone())
                .collect();
            let lg = livegraph_callee_keys(repo_state, &target.stable_key);
            let report = compare_keys(symbol, "callees", &sqlite_keys, lg);
            let sidecar = write_compare_sidecar(repo_root, &report).ok();
            let count = sqlite_callees.len();
            let mut v = json!({ "target": target, "callees": sqlite_callees, "count": count });
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
