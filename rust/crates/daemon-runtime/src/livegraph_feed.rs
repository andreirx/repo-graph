//! LIVEGRAPH-INTEGRATION-1B: dev-only preload of a SUPPLIED SCIP index into a repo's in-memory
//! LiveGraph. Decode + ingest + feed ONLY — the daemon does NOT run scip-typescript or do package
//! discovery / refresh orchestration (that is LIVEGRAPH-INTEGRATION-1C).

use repo_graph_daemon_transport::ProgressEmitter;
use repo_graph_ir::CanonicalKey;
use repo_graph_livegraph::{
    FileImportCyclesAnswer, LiveGraph, ModuleImportCyclesAnswer, ModuleStatRow,
};
use repo_graph_scip_ingest::{decode_index, ingest_partition};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::queries::{
    martin_metrics, CalleeResult, CallerResult, ModuleStatsResult, ResolvedSymbol,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::{AnswerClass, FreshnessState, Granularity, LanguageSupport};
use serde::Serialize;
use serde_json::{json, Value};

use repo_graph_agent::AgentSnapshot;

use crate::state::RepoState;

/// W-B-EPOCH-IMPL-1 (D-EP = EP-A): the request-scoped cross-store epoch.
///
/// A mixed-read handler (orient / explain / callers / callees) reads TWO independently-versioned
/// stores — the SQLite READY snapshot and the in-memory [`LiveGraph`] — and, before this slice, resolved
/// "latest" from each store SEPARATELY (orient resolved the snapshot twice; the LiveGraph was served at a
/// later instant with no epoch pin). `RequestEpoch` is the fix: each handler captures ONE pinned READY
/// snapshot + ONE green-validated LG-serve eligibility fingerprint right after `acquire_read`, then threads
/// the captured value so every read in the request resolves to the SAME epoch (`daemon-w-b-epoch-1.md` §5).
///
/// - `snapshot` — the pinned READY [`AgentSnapshot`], resolved ONCE. The request's ATOMIC SQLite identity;
///   every SQLite read uses [`RequestEpoch::snapshot_uid`]. Carried WHOLE (not just the uid) so the orient
///   use case keeps `aggregators::snapshot::aggregate(&snapshot)` without a second resolve, and so the
///   `OrientServeDecorator` (orient/explain) can serve it back from `get_latest_snapshot` (pinning explain's
///   whole request without an agent-crate change — `daemon-w-b-epoch-1.md` §5.2).
/// - `fingerprint` — the LG-serve eligibility witness, captured BUILD-THEN-PEEK (§6.4) via
///   `orient_serve::orient_serve_witness` (orient/explain — EC-M2 review-0 #1: `Some(fp)` iff AT LEAST ONE
///   of the three independent leaf decisions — the FOCUS-RESOLUTION ∧ CALLGRAPH bounded fold, cycle
///   VALUES, MODULE_SUMMARY — is GREEN at `fp`; which leaves actually serve rides beside it on the
///   witness) or `callgraph_cert::callgraph_cert_eligibility` (callers/callees — CALLGRAPH cert, where
///   `Some(fp)` = that one GREEN cert). Either way a `Some` fingerprint means a GREEN no-loss cert exists
///   at EXACTLY the resident fingerprint for `snapshot_uid` (the resident partitions are cert-proven
///   no-loss-equal to SQLite@`snapshot_uid` for the serving leaves); `None` = no green cert ⇒ eager
///   SQLite, no LiveGraph serve. At each serve site the resident fingerprint is re-validated against this
///   captured value under the data read guard (EV-A); on mismatch the leaf fails soft to the pinned SQLite
///   snapshot — never a cross-epoch mix.
///
/// Abstraction ledger (per the operating rule):
/// - **What:** a request-scoped `{ snapshot, fingerprint }` value captured once per request.
/// - **Concrete current users:** the orient / explain / callers / callees handlers (`dispatch.rs`), the
///   `OrientServeDecorator` (orient/explain serve gate + snapshot pin), and `callers_engine_response` /
///   `callees_engine_response` (the callers/callees Auto-arm serve gate).
/// - **Axis of variation:** per-request cross-store epoch pinning (one snapshot + one eligibility
///   fingerprint); nothing beyond that.
/// - **Rejected simpler:** carrying only `snapshot_uid` — non-buildable, it strands
///   `snapshot::aggregate(&snapshot)` in `orient/repo.rs`. Rejected fancier: a storage-port `pinned_epoch()`
///   method — an unearned boundary surface; `AgentSnapshot` already crosses the port, so this value
///   introduces NO new cross-boundary data shape.
/// - **Home:** this (`livegraph_feed`) module — the lowest of the three serve modules (orient_serve →
///   callgraph_cert → livegraph_feed; livegraph_feed depends on neither), beside `import_cert_fingerprint`
///   which computes the epoch's fingerprint. A dedicated `request_epoch` module was REJECTED (review-0 #3:
///   the packet forbids a new module boundary, and `RequestEpoch` does not need one); orient_serve /
///   callgraph_cert would each introduce a module cycle.
pub struct RequestEpoch {
    /// The pinned READY snapshot, resolved ONCE — the ATOMIC SQLite pin. Carried WHOLE so the orient use
    /// case keeps `snapshot::aggregate(&snapshot)` and the decorator can return it from
    /// `get_latest_snapshot` without re-resolving. Every SQLite read uses `snapshot.snapshot_uid`.
    pub snapshot: AgentSnapshot,
    /// The LG-serve eligibility witness (build-then-peek): `Some(fp)` = a GREEN no-loss cert validated
    /// `import_cert_fingerprint(partitions, snapshot.snapshot_uid)` at capture; `None` = no green cert ⇒
    /// eager SQLite, no LiveGraph serve.
    pub fingerprint: Option<String>,
}

impl RequestEpoch {
    /// The pinned SQLite identity — every SQLite read and the response stamp use THIS.
    pub fn snapshot_uid(&self) -> &str {
        &self.snapshot.snapshot_uid
    }
}

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
fn livegraph_callers_auto(
    repo_state: &RepoState,
    epoch: &RequestEpoch,
    target: &str,
) -> Option<LgAuto> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    // W-B-EPOCH-IMPL-1 (D-EV = EV-A): the resident LiveGraph must still be the captured green-validated
    // epoch (the CALLGRAPH-cert eligibility). The fingerprint compare is computed under the SAME read guard
    // that reads the envelope below, so the gate + the data read are atomic w.r.t. a swap. On a
    // swap/straddle (current_fp != captured) or a `None` eligibility, return `None` ⇒ the Auto arm serves
    // SQLite at the pinned snapshot (never a cross-epoch mix). This subsumes the existing per-call
    // Exact+Fresh+TS-only reduction below, which is kept (belt-and-suspenders).
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), epoch.snapshot_uid());
    if Some(&current_fp) != epoch.fingerprint.as_ref() {
        return None;
    }
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
fn livegraph_callees_auto(
    repo_state: &RepoState,
    epoch: &RequestEpoch,
    target: &str,
) -> Option<LgAuto> {
    let guard = repo_state.livegraph.read();
    let lg = guard.as_ref()?;
    // W-B-EPOCH-IMPL-1 (D-EV = EV-A): see `livegraph_callers_auto` — the same captured-epoch fingerprint
    // gate under the data read guard; mismatch/None ⇒ SQLite at the pinned snapshot.
    let current_fp = import_cert_fingerprint(&lg.live_partitions(), epoch.snapshot_uid());
    if Some(&current_fp) != epoch.fingerprint.as_ref() {
        return None;
    }
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

/// The LEGACY key→row builder (recon-design-1 §3.7-3/-4 defects: `edge_type: "CALLS"` hardcoded on
/// a KIND-BLIND key set; `""`/`0` placeholders standing in for unknown locations). RECON-M-R2
/// REPLACED it on the union path — `union_serve` builds rows with the kind-partitioned ledger
/// projection and null-not-zero locations. It survives HERE for exactly the byte-frozen legacy
/// paths: the flag-OFF GREEN-cert `Auto` serve and the explicit `--engine livegraph` 1B dev path,
/// whose served bytes are parity-mandated until the recorded default flip (do NOT "fix" the
/// placeholders here — that would change flag-off bytes).
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

/// Legacy key→row builder, callees side — see [`caller_results_from_keys`] (the same §3.7-3/-4
/// status: replaced on the union path, byte-frozen on the legacy paths).
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
    epoch: &RequestEpoch,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CallerResult>, StorageError>,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, StorageError> {
    match engine {
        // Explicit SQLite escape hatch: ALWAYS read (the closure is called).
        Engine::Sqlite => callers_auto_or_sqlite(target, None, None, sqlite_fetch),
        // DEFAULT (QUERY-AUTO-LAZY-SQLITE-1): LiveGraph-first; SQLite read LAZILY only on fallback.
        // W-B-EPOCH-IMPL-1: the Auto serve is gated on the captured CALLGRAPH-cert epoch (EV-A) inside
        // `livegraph_callers_auto`.
        Engine::Auto => {
            let (served, reason) = match auto_outcome(livegraph_callers_auto(
                repo_state,
                epoch,
                &target.stable_key,
            )) {
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
/// `pub(crate)`: RECON-M-R2 — `union_serve`'s fallback arms reuse THIS builder so a flag-ON
/// non-W-BOTH answer is byte-identical to today's fallback (one builder, no drift).
pub(crate) fn callers_auto_or_sqlite(
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
    epoch: &RequestEpoch,
    target: &ResolvedSymbol,
    sqlite_fetch: impl FnOnce() -> Result<Vec<CalleeResult>, StorageError>,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, StorageError> {
    match engine {
        // Explicit SQLite escape hatch: ALWAYS read (the closure is called).
        Engine::Sqlite => callees_auto_or_sqlite(target, None, None, sqlite_fetch),
        // DEFAULT (QUERY-AUTO-LAZY-SQLITE-1): LiveGraph-first; SQLite read LAZILY only on fallback.
        // W-B-EPOCH-IMPL-1: the Auto serve is gated on the captured CALLGRAPH-cert epoch (EV-A) inside
        // `livegraph_callees_auto`.
        Engine::Auto => {
            let (served, reason) = match auto_outcome(livegraph_callees_auto(
                repo_state,
                epoch,
                &target.stable_key,
            )) {
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
/// the served path. `pub(crate)`: RECON-M-R2 — reused by `union_serve`'s fallback arms (see the callers twin).
pub(crate) fn callees_auto_or_sqlite(
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

/// A LiveGraph path answer reduced for the explicit `--engine livegraph/compare` diagnostic serve
/// (PATH-LIVEGRAPH-DEFAULT-1). W-B-EPOCH-IMPL-2A removed the `ts_only` field with `path_auto_outcome`: the
/// default `Auto` no longer serves the LiveGraph path (it serves the pinned SQLite snapshot), and the explicit
/// LiveGraph arm surfaces `class`/`freshness` as diagnostics without gating on TS-only.
struct LgPathAuto {
    class: AnswerClass,
    freshness: FreshnessState,
    /// Whether a path was found (vs a no-path result).
    found: bool,
    /// Path nodes with display metadata (key + resolved `file:line`).
    nodes: Vec<PathNodeDisplay>,
}

/// LiveGraph path for `(from_key, to_key)` with a cooperative cancellation checkpoint
/// threaded into the BFS (DAEMON-CANCEL-1). `Ok(None)` = no LiveGraph for this repo
/// (fall back to SQLite); `Ok(Some(_))` = the LiveGraph answer reduced to the `Auto`
/// decision; `Err(StorageError::Cancelled)` = the peer disconnected mid-search (the BFS
/// bailed at its next checkpoint). Carries class + freshness + TS-only + found + per-node
/// display metadata. The `file:line` display metadata is resolved here (under
/// the read guard) via `node_location` and does NOT affect path()/trust semantics.
fn livegraph_path_cancellable(
    repo_state: &RepoState,
    from_key: &str,
    to_key: &str,
    // The `graph-algorithms` `CancelCheck` is exactly this bare type alias; we spell
    // it out so `daemon-runtime` needs no direct dependency on `graph-algorithms`
    // (it reaches the cancellable BFS through `livegraph`). It coerces to
    // `LiveGraph::path_cancellable`'s `CancelCheck` param at the call below.
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Option<LgPathAuto>, StorageError> {
    let guard = repo_state.livegraph.read();
    let lg = match guard.as_ref() {
        Some(lg) => lg,
        None => return Ok(None),
    };
    let env = lg
        .path_cancellable(from_key, to_key, cancel)
        .map_err(|_cancelled| StorageError::Cancelled)?;
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
    Ok(Some(LgPathAuto {
        class: env.class(),
        freshness: env.freshness(),
        found,
        nodes,
    }))
}

// W-B-EPOCH-IMPL-2A (§14 D-CC refined): the former `path_auto_outcome` (the Auto-arm LiveGraph path
// eligibility: Exact + Fresh + TS-only + display-metadata) is REMOVED — `path`'s default no longer serves the
// LiveGraph fastpath (`path_engine_response`'s `Engine::Auto` now serves the pinned SQLite snapshot). The LG
// path BFS itself (`livegraph_path_cancellable`) is preserved for explicit `--engine livegraph/compare`; a
// future CALLS∪IMPORTS path-parity cert re-introduces a (cert-based) eligibility for the deferred re-enable.

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
    // DAEMON-CANCEL-1: a cooperative checkpoint threaded into the LiveGraph BFS so a
    // peer disconnect cancels the search mid-flight. Consulted only on the
    // LiveGraph-serving arms (Auto/LiveGraph); `Compare` keeps the plain BFS (it is
    // a diagnostic that always reads SQLite) and the SQLite fallback is not
    // checkpointed here (that is the `sqlite3_interrupt` path, DAEMON-CANCEL-2).
    // Bare seam type (= `graph-algorithms` `CancelCheck`); see `livegraph_path_cancellable`.
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, StorageError> {
    match engine {
        // `--engine sqlite` FORCES SQLite (always reads).
        Engine::Sqlite => path_auto_or_sqlite(repo_uid, snapshot_uid, None, None, sqlite_fetch),
        // DEFAULT (W-B-EPOCH-IMPL-2A, §14 D-CC refined): serve the PINNED SQLite snapshot — NO LiveGraph
        // fastpath for `path`. There is no CALLS∪IMPORTS no-loss cert to license an LG path serve (the
        // callgraph cert is CALLS-only; `path` traverses CALLS ∪ IMPORTS), so a LiveGraph BFS could be as-of a
        // different epoch than the pinned `snapshot_uid` it is stamped with — the §1c false-freshness hazard.
        // Serving SQLite at `snapshot_uid` (the handler's captured epoch pin) makes the BFS genuinely as-of the
        // stamp. `path` still gets read-during-refresh via the pin (IMPL-3). A future CALLS∪IMPORTS path-parity
        // cert RE-ENABLES the LG fastpath here (a deferred speed optimization, NOT a correctness blocker); the
        // LG path BFS (`livegraph_path_cancellable`) is preserved for explicit `--engine livegraph/compare`.
        Engine::Auto => path_auto_or_sqlite(repo_uid, snapshot_uid, None, None, sqlite_fetch),
        // Explicit LiveGraph keeps trust_class/freshness as a diagnostic surface; SQLite LAZILY on fallback.
        Engine::LiveGraph => {
            match livegraph_path_cancellable(repo_state, from_key, to_key, cancel)? {
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
            }
        }
        // Compare: ALWAYS reads SQLite. The sqlite_found/sqlite_names extraction (compare-only) moves HERE.
        // DAEMON-CANCEL-1: the compare LiveGraph BFS is checkpointed too (review finding #2) — it runs the
        // SAME `LiveGraph::path` BFS, so a peer disconnect mid-search cancels it, not just the Auto/LiveGraph
        // arms. The SQLite side (`sqlite_fetch`, a recursive CTE) stays uncheckpointed — `sqlite3_interrupt`
        // is DAEMON-CANCEL-2.
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
            let lg = livegraph_path_cancellable(repo_state, from_key, to_key, cancel)?.map(|a| {
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

/// FIXTURE-POLLUTION-1 §2.3 — the honest asymmetry disclosure carried by the LiveGraph MODULE-cycle
/// serving paths: the default fastpath [`serve_cycles_fastpath`] and the explicit `--kind module-import`
/// route [`module_import_cycles_response`]. Both serve from the in-memory LiveGraph IR, which lacks the
/// stored `is_test` fact (test-only classification deferred to CYCLE-FACTS-2), so neither can split
/// test-only cycles out of the headline the way the SQLite route does. State it rather than pretend
/// uniformity; the `--engine sqlite` MODULE-cycle route IS the classified equivalent to point at.
/// (File-import cycles have no classified sqlite equivalent, so that route uses its own inline note.)
const LIVEGRAPH_MODULE_CYCLE_TEST_COMPOSITION_NOTE: &str =
    "test-only cycles not evaluated on this serving path (LiveGraph lacks the is_test fact); \
     run `rmap cycles --engine sqlite` to classify test-only cycles";

/// CYCLES-LIVEGRAPH-CLI-1: build the `--engine livegraph --kind file-import` cycles response. Calls the
/// headless `file_import_cycles()` and maps it into the cycles shape + trust metadata. NO SQLite fallback
/// (D7): the trust class/scope are surfaced; the answer never silently becomes the SQLite MODULE graph.
pub fn file_import_cycles_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    // DAEMON-CANCEL-1: checkpoint threaded into the file-import Tarjan so
    // `cycles --engine livegraph --kind file-import` cancels mid-flight. `Err` is
    // `StorageError::Cancelled` on a peer disconnect (this route has no SQLite peer).
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    let guard = repo_state.livegraph.read();
    let (class, freshness, missing, reasons, cycles, scope) = match guard.as_ref() {
        Some(lg) => {
            let env = lg
                .file_import_cycles_cancellable(cancel)
                .map_err(|_| repo_graph_storage::error::StorageError::Cancelled)?;
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
    // CYCLE-HONESTY-1 (§2.4, review-2 route-consistency): the type-only caveat basis is the SAME stored
    // per-language file facts (≥10% materiality) every cycles route reads — NOT `contributing_languages`.
    // Read AFTER the LiveGraph guard is dropped (no lock nesting). CLASSIFIED read -> a genuine error
    // PROPAGATES; when count == 0 the caveat is false regardless, but the read still surfaces honestly.
    let has_ts = {
        let conn = repo_state.storage().map_err(|e| {
            repo_graph_storage::error::StorageError::InvalidArgument(format!(
                "failed to open storage connection for the TS/JS caveat: {e}"
            ))
        })?;
        snapshot_has_material_ts_js(&conn, snapshot_uid)?
    };
    Ok(json!({
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
        "ts_type_only_caveat": has_ts && count > 0,
        // FIXTURE-POLLUTION-1 §2.3: the LiveGraph IR lacks the `is_test` fact (deferred to
        // CYCLE-FACTS-2), so this FILE-import serving path CANNOT classify test-only cycles.
        // State the asymmetry honestly rather than pretend uniformity. No `--engine sqlite`
        // hint here: that route serves MODULE cycles, not FILE cycles — pointing at it would
        // be a false claim of an equivalent classified answer.
        "test_composition_note":
            "test composition not evaluated on this serving path (the LiveGraph IR lacks the \
             is_test fact); FILE-import cycles are not classified test-only vs production",
    }))
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
    // DAEMON-CANCEL-1: checkpoint threaded into the module-import Tarjan so
    // `cycles --engine livegraph --kind module-import` cancels mid-flight. `Err` is
    // `StorageError::Cancelled` on a peer disconnect (this route has no SQLite peer).
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    let guard = repo_state.livegraph.read();
    let (class, freshness, missing, reasons, cycles, scope) = match guard.as_ref() {
        Some(lg) => {
            let env = lg
                .module_import_cycles_cancellable(cancel)
                .map_err(|_| repo_graph_storage::error::StorageError::Cancelled)?;
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
    // CYCLE-HONESTY-1 (§2.4, review-2 route-consistency): SAME stored per-language file facts (≥10%
    // materiality) as every cycles route — NOT `contributing_languages`. Read AFTER the guard is dropped
    // (no lock nesting). CLASSIFIED read -> a genuine error PROPAGATES.
    let has_ts = {
        let conn = repo_state.storage().map_err(|e| {
            repo_graph_storage::error::StorageError::InvalidArgument(format!(
                "failed to open storage connection for the TS/JS caveat: {e}"
            ))
        })?;
        snapshot_has_material_ts_js(&conn, snapshot_uid)?
    };
    Ok(json!({
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
        "ts_type_only_caveat": has_ts && count > 0,
        // FIXTURE-POLLUTION-1 §2.3: same asymmetry as the default fastpath — this MODULE-cycle
        // LiveGraph route lacks the `is_test` fact, so it cannot classify test-only cycles. The
        // `--engine sqlite` MODULE-cycle route is the classified equivalent to point at.
        "test_composition_note": LIVEGRAPH_MODULE_CYCLE_TEST_COMPOSITION_NOTE,
    }))
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
        .storage()
        .ok()
        .and_then(|conn| conn.distinct_file_languages(repo_uid).ok())
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
        .storage()
        .ok()
        .and_then(|conn| conn.find_imports(snapshot_uid, &file_stable_key).ok())
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

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D4) + W-B-EPOCH-IMPL-2A (EV-A): the PURE fastpath/compare ladder under
/// the captured request epoch. `epoch_eligible` is the EV-A serve gate — `true` iff the resident import-cert
/// fingerprint (computed under the SAME read guard that captured `view`) still equals the green-validated
/// `epoch.fingerprint` (built BUILD-THEN-PEEK by [`import_cert_eligibility`] in the handler). precondition UNMET
/// -> the SQLite compare-on-call (via the find_imports closure) ; precondition met AND `epoch_eligible` -> serve
/// LiveGraph WITHOUT calling find_imports (the cert proved the resident import edges no-loss-equal to
/// SQLite@`snapshot_uid`) ; NOT eligible (no green cert at capture, OR a swap/straddle since capture so the
/// resident fingerprint moved) -> compare-on-call (calls find_imports at the pinned snapshot). Pure (no
/// RepoState) -> a panicking find_imports closure proves the eligible fastpath skips SQLite.
fn imports_fastpath_or_compare(
    file_path: &str,
    view: &repo_graph_livegraph::import_view::LiveImportView,
    precond: Option<&repo_graph_livegraph::import_view::FilePartitionStatus>,
    epoch_eligible: bool,
    find_imports: impl FnOnce() -> Vec<repo_graph_storage::queries::ImportResult>,
) -> Value {
    if precond.is_some_and(|p| p.precondition_met()) && epoch_eligible {
        serve_import_fastpath(file_path, view)
    } else {
        // Non-TS / non-resident, OR the captured epoch no longer matches the resident fingerprint (EV-A
        // fail-soft) -> the proven compare-on-call (reads SQLite at the pin, verifies no-loss).
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

/// W-B-EPOCH-IMPL-2A (D-EP capture for `imports`; `daemon-w-b-epoch-1.md` §6.4): the IMPORT-cert LG-serve
/// eligibility WITNESS, captured BUILD-THEN-PEEK. The import-cert sibling of
/// [`crate::callgraph_cert::callgraph_cert_eligibility`] (callers/callees use the CALLGRAPH cert; `imports`
/// serves the LiveGraph import EDGES, whose no-loss proof is the IMPORT cert — a GREEN callgraph cert does NOT
/// license serving import edges). Returns `Some(current_fp)` iff a GREEN import cert exists at EXACTLY the
/// resident fingerprint for `snapshot_uid` — i.e. the resident partitions' import edges are cert-proven
/// no-loss-equal to SQLite@`snapshot_uid`, so they are substitutable for it; otherwise `None` ⇒ the request
/// serves the per-call compare-on-call at the pinned snapshot (the EV-A fail-soft).
///
/// **The build-then-peek pattern (reused by IMPL-2B for stats/cycles).** A naïve "compute current_fp, then
/// lazily build the cert" leaks a TOCTOU: the cert build re-locks the LiveGraph (parking_lot is non-reentrant),
/// so under a future W-B relax a publish could land between the fingerprint computation and the build, keying
/// the stored verdict at the pre-swap fingerprint while it was computed over post-swap partitions — a mislabel.
/// Build-then-peek closes it:
///   1. WARM — lazy-build the import cert at the current resident fingerprint if (and only if) it is
///      stale/missing (a valid cert is reused, preserving the zero-SQLite-read green fastpath).
///   2. PEEK — under ONE livegraph read guard (which excludes a concurrent swap, since a swap needs
///      `livegraph.write()`), recompute `current_fp` AND peek a GREEN import cert at EXACTLY `current_fp`.
///
/// So `Some(fp)` is the EXACT resident-and-validated state, or `None`. (Under the W-A coordinator this slice
/// ships, no swap can occur mid-request anyway; build-then-peek is the foundation IMPL-3's relax relies on —
/// this is exactly the closing of the "capture-view-then-lazy-cert-build straddle" the decision-review found.)
pub(crate) fn import_cert_eligibility(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Option<String> {
    // 1. WARM: lazy (re)build the import cert ONLY if stale/missing at the current resident fingerprint
    //    (build_and_store_import_cert reads SQLite once per fingerprint; the stale-check keeps a valid GREEN
    //    cert's serve zero-read). The read guard is dropped before the build so it can re-lock without deadlock.
    let warm_fp = {
        let guard = repo_state.livegraph.read();
        guard
            .as_ref()
            .map(|lg| import_cert_fingerprint(&lg.live_partitions(), snapshot_uid))
    };
    if let Some(fp) = warm_fp {
        let stale = !matches!(
            repo_state.import_cert.read().as_ref(),
            Some(c) if c.fingerprint == fp
        );
        if stale {
            let _ = build_and_store_import_cert(repo_state, repo_uid, snapshot_uid, Some(fp));
        }
    }
    // 2. PEEK under ONE read guard so "(GREEN import cert) at (this exact resident fingerprint)" is atomic
    //    w.r.t. any swap.
    let guard = repo_state.livegraph.read();
    let current_fp = import_cert_fingerprint(&guard.as_ref()?.live_partitions(), snapshot_uid);
    let cached = repo_state.import_cert.read();
    match cached.as_ref() {
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN" => Some(current_fp),
        _ => None,
    }
}

/// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1=C / D4) + W-B-EPOCH-IMPL-2A (EV-A): the AUTO (default)
/// `imports <file>` response under the captured request `epoch`. Serves the GREEN-cert FASTPATH (LiveGraph
/// WITHOUT SQLite) iff the file's precondition is met AND the resident import-cert fingerprint STILL equals the
/// captured green-validated `epoch.fingerprint` (the EV-A gate) ; else the proven compare-on-call (reads SQLite
/// at the pinned `epoch.snapshot_uid()`).
///
/// **TOCTOU closed.** `view` + the file's `precond` + the resident `current_fp` are captured under ONE read
/// guard, so the served view and the fingerprint it is validated by are the SAME resident partition set; the
/// gate then compares that `current_fp` against the PRE-validated `epoch.fingerprint` (built BUILD-THEN-PEEK by
/// [`import_cert_eligibility`] in the handler). A swap/straddle since capture moves `current_fp`, so the gate
/// fails and the request fails soft to SQLite at the pin — never a green-labelled serve of an unvalidated view.
/// No lazy cert build happens here anymore (it moved to the eligibility capture).
pub fn imports_auto_response(
    repo_state: &RepoState,
    repo_uid: &str,
    epoch: &RequestEpoch,
    file_path: &str,
) -> Value {
    let snapshot_uid = epoch.snapshot_uid();
    // SQLite-FREE: the file's view + precondition + the current import-cert fingerprint, ALL under ONE guard.
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
    // EV-A: serve the LiveGraph fastpath iff the resident fingerprint still equals the captured green-validated
    // eligibility witness; mismatch / None (a swap/straddle since capture, or no GREEN import cert) -> SQLite.
    let epoch_eligible = current_fp.is_some() && current_fp.as_ref() == epoch.fingerprint.as_ref();
    let file_stable_key = format!("{repo_uid}:{file_path}:FILE");
    imports_fastpath_or_compare(file_path, &view, precond.as_ref(), epoch_eligible, || {
        repo_state
            .storage()
            .ok()
            .and_then(|conn| conn.find_imports(snapshot_uid, &file_stable_key).ok())
            .unwrap_or_default()
    })
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
        .storage()
        .ok()
        .and_then(|conn| conn.all_imports(snapshot_uid).ok())
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
    /// EC-M2-LEAF-SERVE-1 (CYCLES-B): true iff the CANONICAL agent-value shapes agree — the
    /// repo-level SHORT-name cycle list (SQLite `CycleNode.name` vs LiveGraph member basename) AND
    /// the qualified cycle list, both canonicalized by the agent's `canonicalize_cycles`. Strictly
    /// stronger than the set compare; the extra strength covers the SHORT-name rendering the set
    /// compare never sees.
    values_exact: bool,
}

/// Compute the shared [`ModuleCycleCompareData`] — the SQLite MODULE cycles mapped to QUALIFIED module
/// paths vs the LiveGraph derived module cycles, compared by SET (reads SQLite once + the LiveGraph once) —
/// with a cooperative checkpoint threaded into BOTH Tarjan loops it runs: the SQLite SCC
/// (`find_cycles_cancellable`) and the LiveGraph module-cycle SCC (`module_import_cycles_cancellable`). So
/// BOTH the `--engine compare` surface AND the DEFAULT route's first-call-per-fingerprint cert build cancel
/// mid-flight (DAEMON-CANCEL-1). On a peer disconnect it returns
/// [`StorageError::Cancelled`](repo_graph_storage::error::StorageError::Cancelled); read-only ⇒ nothing to
/// roll back. There is no non-cancellable variant: the orient cert-build path reaches this through
/// [`build_and_store_cycles_cert`]'s never-breaking checkpoint, which is byte-identical (the `Cancelled` arm
/// is then unreachable).
fn module_cycle_compare_data_cancellable(
    repo_state: &RepoState,
    snapshot_uid: &str,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<ModuleCycleCompareData, repo_graph_storage::error::StorageError> {
    use repo_graph_livegraph::module_cycle_compare::compare_module_cycles;
    // D-S = S-A: one fresh per-operation connection for these reads.
    let conn = repo_state.storage().map_err(|e| {
        repo_graph_storage::error::StorageError::InvalidArgument(format!(
            "failed to open storage connection: {e}"
        ))
    })?;
    // Reborrow `cancel` (`&mut *cancel`) so it stays usable for the LiveGraph Tarjan below.
    let sqlite_cycles = conn.find_cycles_cancellable(snapshot_uid, "module", &mut *cancel)?;
    let qnames = conn.module_qualified_names(snapshot_uid)?;
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
                let env = lg
                    .module_import_cycles_cancellable(cancel)
                    .map_err(|_| repo_graph_storage::error::StorageError::Cancelled)?;
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
    // EC-M2-LEAF-SERVE-1 (CYCLES-B): compare the CANONICAL agent-VALUE shapes the decorator would
    // serve — the exact bytes-that-render check, computed from data already in hand (no extra
    // store read). Repo level: SQLite renders `CycleNode.name` (SHORT); the LiveGraph member is the
    // qualified dirname path, rendered via its basename. Path/module level: both render qualified.
    // Both shapes canonicalized by the SAME `canonicalize_cycles` the agent applies, so equality
    // here IS byte-equality of the served values (one canonicalization, zero drift).
    let values_exact = {
        use repo_graph_agent::ordering::canonicalize_cycles;
        use repo_graph_agent::AgentCycle;
        let mut sq_repo: Vec<AgentCycle> = sqlite_cycles
            .iter()
            .map(|c| AgentCycle {
                length: c.length,
                modules: c.nodes.iter().map(|n| n.name.clone()).collect(),
                // ORIENT-CYCLES-DISAGREE-1: canonical-shape compare only (cycle membership),
                // not a served value; test-composition is not part of the equivalence.
                test_composition: None,
            })
            .collect();
        let mut lg_repo: Vec<AgentCycle> = lg_cycles
            .iter()
            .map(|members| AgentCycle {
                length: members.len(),
                modules: members
                    .iter()
                    .map(|m| crate::cycle_output::module_basename(m).to_string())
                    .collect(),
                // ORIENT-CYCLES-DISAGREE-1: canonical-shape compare only (see above).
                test_composition: None,
            })
            .collect();
        let mut sq_qual: Vec<AgentCycle> = sqlite_qualified
            .iter()
            .zip(sqlite_cycles.iter())
            .map(|(quals, c)| AgentCycle {
                length: c.length,
                modules: quals.clone(),
                // ORIENT-CYCLES-DISAGREE-1: canonical-shape compare only (see above).
                test_composition: None,
            })
            .collect();
        let mut lg_qual: Vec<AgentCycle> = lg_cycles
            .iter()
            .map(|members| AgentCycle {
                length: members.len(),
                modules: members.clone(),
                // ORIENT-CYCLES-DISAGREE-1: canonical-shape compare only (see above).
                test_composition: None,
            })
            .collect();
        canonicalize_cycles(&mut sq_repo);
        canonicalize_cycles(&mut lg_repo);
        canonicalize_cycles(&mut sq_qual);
        canonicalize_cycles(&mut lg_qual);
        sq_repo == lg_repo && sq_qual == lg_qual
    };
    Ok(ModuleCycleCompareData {
        comparison,
        sqlite_cycles,
        sqlite_count,
        livegraph_count: lg_cycles.len(),
        livegraph_class,
        obs_by_module,
        lg_modules,
        values_exact,
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
    // DAEMON-CANCEL-1: checkpoint threaded into the compare's two Tarjan loops so
    // `cycles --engine compare` cancels mid-flight. The error type is now `StorageError`
    // (was `String`) so the handler can distinguish `Cancelled` from an internal error.
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    use repo_graph_livegraph::module_cycle_compare::classify_missing_module_cycle;
    // The SHARED comparison computation (identical basis to the fastpath cert -> no drift).
    let data = module_cycle_compare_data_cancellable(repo_state, snapshot_uid, cancel)?;
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
    // CYCLE-HONESTY-1 (§2.4): the repo-level type-only caveat, uniform with the other routes. The compare's
    // PRIMARY cycles (`data.sqlite_cycles`, raw `CycleResult`) carry NO edges, so the renderer draws
    // `members (unordered)` — the caveat is the only cycle-honesty output on this diagnostic surface.
    // CLASSIFIED read -> a genuine error propagates.
    let conn = repo_state.storage().map_err(|e| {
        repo_graph_storage::error::StorageError::InvalidArgument(format!(
            "failed to open storage connection: {e}"
        ))
    })?;
    let ts_type_only_caveat =
        snapshot_has_material_ts_js(&conn, snapshot_uid)? && data.sqlite_count > 0;
    let mut v = json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": data.sqlite_cycles,
        "count": data.sqlite_count,
        "backend_used": "sqlite",
        "kind": "module-import",
        "ts_type_only_caveat": ts_type_only_caveat,
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
    /// EC-M2-LEAF-SERVE-1 (CYCLES-B): the cycle-VALUES verdict — `GREEN` iff, ON TOP of the set
    /// compare above, the CANONICAL agent-value shapes (repo-level SHORT-name cycles AND the
    /// qualified-name cycles, both via `repo_graph_agent::ordering::canonicalize_cycles`) are
    /// byte-equal between the two stores. This is what licenses the `OrientServeDecorator` to serve
    /// orient/explain cycle VALUES from the LiveGraph: set equality alone does NOT prove the
    /// rendered short names agree (a SQLite MODULE node `name` that is not the basename of its
    /// `qualified_name` would diverge silently). `verdict` keeps its shipped set-based semantics —
    /// the cycles-command fastpath and the IMPORT_CYCLES corroboration label are UNCHANGED.
    pub values_verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// Serve the LiveGraph MODULE cycles in the CANONICAL, byte-identical output (`cycle_output`) WITHOUT reading
/// SQLite -- the GREEN-cert fastpath. The `cycles` array is byte-identical to the SQLite default's canonical
/// output (proven in CYCLES-OUTPUT-CONTRACT-1 on xpart + amodx). `backend_used=livegraph`, no fallback.
fn serve_cycles_fastpath(
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    lg_cycles: &[Vec<String>],
    // CYCLE-HONESTY-1 (§2.4, C1 repo-level + review-2): the repo shows MATERIALLY-present TS/JS — computed
    // by the caller (`cycles_auto_response`) from the SAME stored per-language file facts + ≥10% gate the
    // SQLite route reads, so the caveat is route-consistent (not `contributing_languages` guesswork). NO
    // `edges` are carried here: the LiveGraph dirname-aggregated module edges are not cert-proven equal to
    // SQLite's, so omitting the field is honest (operator ruling A1) and the renderer draws `members
    // (unordered)`.
    repo_has_ts_js: bool,
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
        "ts_type_only_caveat": repo_has_ts_js && count > 0,
        // FIXTURE-POLLUTION-1 §2.3: the LiveGraph IR lacks the `is_test` fact (deferred to
        // CYCLE-FACTS-2), so this serving path CANNOT classify test-only cycles. State the
        // asymmetry honestly rather than pretend uniformity — never a silent "no fixtures".
        // The SQLite route (`--engine sqlite`) carries the per-cycle classification.
        "test_composition_note": LIVEGRAPH_MODULE_CYCLE_TEST_COMPOSITION_NOTE,
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
    // DAEMON-CANCEL-1: cooperative checkpoint threaded into the SQLite SCC Tarjan
    // (`find_cycles_cancellable`) so the DEFAULT `cycles` fallback cancels mid-flight.
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    // D-S = S-A: one fresh per-operation connection for these reads.
    let conn = repo_state.storage().map_err(|e| {
        repo_graph_storage::error::StorageError::InvalidArgument(format!(
            "failed to open storage connection: {e}"
        ))
    })?;
    let sqlite_cycles = conn.find_cycles_cancellable(snapshot_uid, "module", cancel)?;
    let qualified = conn.module_qualified_names(snapshot_uid)?;
    // CYCLE-HONESTY-1 (§2.1): attach the REAL intra-SCC MODULE→MODULE IMPORTS edges so the renderer can draw
    // a verified walk. These are the SAME edges `find_cycles` loaded for its SCC pass; the renderer draws an
    // arrow ONLY for a pair present here.
    let module_edges = conn.module_import_edges(snapshot_uid)?;
    let mut cycles = crate::cycle_output::sqlite_module_cycles_json_with_edges(
        &sqlite_cycles,
        &qualified,
        &module_edges,
    );
    // FIXTURE-POLLUTION-1 §2.2/§2.3: classify test-only cycles (e.g. the xpart-monorepo
    // fixture) so the renderer demotes them below the real cycles. The SQLite route reaches
    // the stored `is_test` fact (the LiveGraph fastpath does not — §2.3 asymmetry).
    // Conservative aggregation: a cycle is test-only iff EVERY member module is wholly
    // test-owned; an unclassifiable member ⇒ unknown (not demoted). CLASSIFIED read → a
    // genuine error PROPAGATES.
    let tracked = conn.get_files_by_repo(repo_uid)?;
    let files: Vec<(&str, bool)> = tracked
        .iter()
        .map(|f| (f.path.as_str(), f.is_test))
        .collect();
    crate::cycle_output::label_test_only_cycles(&mut cycles, &files);
    let count = cycles.len();
    // ZEROSTATE-SCOPE-1 §2.3: on this SQLite MODULE-cycle route the member languages ARE reachable
    // (tracked-files `language` + `module_qualified_names`), so the type-only caveat gates on cycle
    // MEMBERSHIP, not the repo-level ≥10% gate — repo-graph (dominant Rust, one TS `tools/rgistr`
    // cycle below the repo share) now gets its caveat. Reuses the already-read `tracked`/`qualified`
    // (both already CLASSIFIED reads whose errors propagated above) — no new fallible read.
    let all_module_dirs: Vec<String> = qualified.values().cloned().collect();
    let member_dirs = crate::cycle_output::rendered_cycle_member_dirs(&cycles);
    let files_by_lang: Vec<(&str, Option<&str>)> = tracked
        .iter()
        .map(|f| (f.path.as_str(), f.language.as_deref()))
        .collect();
    let ts_type_only_caveat = crate::cycle_output::any_cycle_member_is_ts_js(
        &member_dirs,
        &all_module_dirs,
        &files_by_lang,
    ) && count > 0;
    Ok(json!({
        "repo_uid": repo_uid,
        "display_name": display_name,
        "snapshot_uid": snapshot_uid,
        "cycles": cycles,
        "count": count,
        "backend_used": "sqlite",
        "fallback_reason": fallback_reason.as_str(),
        "ts_type_only_caveat": ts_type_only_caveat,
    }))
}

/// CYCLE-HONESTY-1 (§2.4, ts-caveat-basis C1 REPO-level + review-2 route-consistency): true iff the
/// snapshot has MATERIALLY-present TS/JS — the repo-level type-only (`import type`) caveat basis. Reads the
/// stored per-language file counts (`query_file_count_by_language` via the `AgentStorageRead` port — the
/// SAME stored fact EVERY cycles route reads, so the caveat is route-consistent by construction) and
/// applies the SHARED ≥10%-of-code-files materiality gate ([`crate::reader_context::repo_has_material_ts_js`],
/// CONTRADICTION-SWEEP-1's `material_code_languages` — NOT a re-derived threshold). "Any TS/JS file present"
/// is deliberately NOT enough: a ~3.7% incidental JS (django) must not trip the caveat.
///
/// NOT name/extension classification — it reads the stored language fact. A genuine read error PROPAGATES
/// (the caller `?`s it) — a failed read is not evidence of "no material TS/JS", so it must never silently
/// become `false` (standing honesty rule 1). An EMPTY inventory (a legitimate no-rows result) is honest
/// `false`.
pub(crate) fn snapshot_has_material_ts_js(
    conn: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
) -> Result<bool, repo_graph_storage::error::StorageError> {
    // Cross-crate via the `AgentStorageRead` port (the pattern `deps list` uses); map its error into a
    // StorageError so the CLASSIFIED read still propagates rather than collapsing to a false "no TS/JS".
    let language_counts =
        repo_graph_agent::AgentStorageRead::query_file_count_by_language(conn, snapshot_uid)
            .map_err(|e| {
                repo_graph_storage::error::StorageError::InvalidArgument(format!(
                    "failed to read per-language file counts for the TS/JS type-only caveat: {e}"
                ))
            })?;
    Ok(crate::reader_context::repo_has_material_ts_js(
        &language_counts,
    ))
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (build): run the SHARED module-cycle compare -> verdict, STORE the cert
/// keyed by `fingerprint`, return `Some(is_green)` (or `None` if no fingerprint / a storage error -> the caller
/// falls back to SQLite). Reads SQLite ONCE per fingerprint via the SAME compare data the `--engine compare`
/// uses, so the GREEN verdict PROVABLY matches the compare (no drift -> no false GREEN).
///
/// This is the NON-CANCELLABLE cert build, retained for the ORIENT cert-build caller
/// (`orient_lg_decisions`), which is OUT of DAEMON-CANCEL-1's in-loop scope. It delegates to
/// [`build_and_store_cycles_cert_cancellable`] with a NEVER-breaking checkpoint, so its behavior is
/// byte-identical to the historical `module_cycle_compare_data(...).ok()?` form (the `Cancelled` arm cannot
/// fire; `.ok().flatten()` collapses both the no-fingerprint and storage-error cases to `None`). The DEFAULT
/// `cycles` handler uses the cancellable form DIRECTLY — review iteration 1 found this first-call-per-
/// fingerprint cert build was the last uncheckpointed Tarjan on that route.
pub(crate) fn build_and_store_cycles_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    build_and_store_cycles_cert_cancellable(repo_state, snapshot_uid, fingerprint, &mut || {
        std::ops::ControlFlow::Continue(())
    })
    .ok()
    .flatten()
}

/// [`build_and_store_cycles_cert`] with the cooperative checkpoint threaded into the SHARED compare data's
/// two Tarjan loops (via [`module_cycle_compare_data_cancellable`]: the SQLite SCC `find_cycles_cancellable`
/// plus the LiveGraph module-cycle SCC), so the DEFAULT `cycles` route's first-call-per-fingerprint cert
/// build cancels MID-FLIGHT on a peer disconnect (DAEMON-CANCEL-1, review iteration 1 — this build
/// previously ran its Tarjan to completion uncancellably). Returns:
///
/// - `Err(StorageError::Cancelled)` — the peer disconnected during the cert-build traversal (the handler
///   maps this to `ErrorCode::Cancelled`).
/// - `Ok(None)` — no fingerprint, OR a NON-cancel storage error (the caller then falls back to the
///   itself-cancellable SQLite serve, exactly as the historical `.ok()?` did). A non-cancel read failure is
///   deliberately NOT surfaced as a cancel — only a genuine client disconnect is (DAEMON-CANCEL-1
///   deliverable #2 discipline: never mislabel an internal failure as a cancellation).
/// - `Ok(Some(is_green))` — the verdict was computed and the cert stored.
pub(crate) fn build_and_store_cycles_cert_cancellable(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Option<bool>, repo_graph_storage::error::StorageError> {
    let Some(fingerprint) = fingerprint else {
        return Ok(None);
    };
    let data = match module_cycle_compare_data_cancellable(repo_state, snapshot_uid, cancel) {
        Ok(data) => data,
        Err(repo_graph_storage::error::StorageError::Cancelled) => {
            return Err(repo_graph_storage::error::StorageError::Cancelled)
        }
        // Non-cancel storage error: behave like the historical `.ok()?` — no cert stored, fall back to
        // SQLite. NOT a client cancel, so it is not surfaced as `Cancelled`.
        Err(_) => return Ok(None),
    };
    let is_green = data.comparison.is_exact();
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    // EC-M2-LEAF-SERVE-1 (CYCLES-B): the VALUES verdict requires BOTH the set compare AND the
    // canonical served-shape compare (`values_exact`); a set-equal repo whose SHORT-name rendering
    // diverges keeps `verdict=GREEN` (the shipped set semantics for the cycles-command fastpath +
    // the corroboration label) but `values_verdict=RED` (the decorator keeps serving SQLite).
    let values_verdict = if is_green && data.values_exact {
        "GREEN"
    } else {
        "RED"
    }
    .to_string();
    *repo_state.cycles_cert.write() = Some(CycleNoLossCert {
        verdict,
        values_verdict,
        fingerprint,
    });
    Ok(Some(is_green))
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1/D4) + W-B-EPOCH-IMPL-2B (EV-A): the PURE fastpath/SQLite ladder
/// under the captured request epoch. precondition UNMET (the LiveGraph module-cycle answer is not `Exact` --
/// non-resident / non-TS / degraded) -> SQLite (the labelled `precondition_reason`) ; precondition met AND
/// `epoch_eligible` -> serve LiveGraph WITHOUT `find_cycles` (the cycles cert proved the resident module
/// cycles no-loss-equal to SQLite@`snapshot_uid`) ; NOT eligible (no GREEN cycles cert at capture, OR a
/// swap/straddle since capture so the resident fingerprint moved) -> SQLite (`LiveGraphCycleDivergence`).
/// Pure (no I/O itself): a panicking `serve_sqlite` proves the eligible path skips SQLite.
///
/// `epoch_eligible` is the EV-A serve gate — `true` iff the resident cycles-cert fingerprint (computed under
/// the SAME read guard that captured `lg_cycles`) still equals the green-validated `epoch.fingerprint` (built
/// BUILD-THEN-PEEK by [`cycles_cert_eligibility`] in the handler). No lazy cert build happens here anymore (it
/// moved to the eligibility capture, closing the capture-LG-cycles-then-lazy-cert-build TOCTOU).
///
/// DAEMON-CANCEL-1: the SQLite-touching `serve_sqlite` branch RECEIVES the cooperative `cancel` checkpoint as a
/// parameter (so the SQLite SCC fallback cancels mid-Tarjan). The first-call-per-fingerprint cert build's
/// cancellation now lives in [`cycles_cert_eligibility`] (its WARM step threads the SAME checkpoint), so the
/// route's every Tarjan still cancels mid-flight.
fn cycles_fastpath_or_sqlite(
    precondition_met: bool,
    precondition_reason: FallbackReason,
    epoch_eligible: bool,
    serve_livegraph: impl FnOnce() -> Value,
    serve_sqlite: impl FnOnce(
        FallbackReason,
        &mut dyn FnMut() -> std::ops::ControlFlow<()>,
    ) -> Result<Value, repo_graph_storage::error::StorageError>,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    if !precondition_met {
        return serve_sqlite(precondition_reason, cancel);
    }
    if epoch_eligible {
        Ok(serve_livegraph())
    } else {
        serve_sqlite(FallbackReason::LiveGraphCycleDivergence, cancel)
    }
}

/// W-B-EPOCH-IMPL-2B (D-EP capture for `cycles`; `daemon-w-b-epoch-1.md` §6.4): the CYCLES-cert LG-serve
/// eligibility WITNESS, captured BUILD-THEN-PEEK. The cycles sibling of [`import_cert_eligibility`] /
/// [`crate::callgraph_cert::callgraph_cert_eligibility`] (callers/callees use the CALLGRAPH cert, `imports`
/// the IMPORT cert; `cycles` serves the LiveGraph MODULE-cycle SCC, whose no-loss proof is the CYCLES cert —
/// a GREEN callgraph/import cert does NOT license serving module cycles). Returns `Some(current_fp)` iff a
/// GREEN cycles cert exists at EXACTLY the resident fingerprint for `snapshot_uid` — i.e. the resident
/// partitions' module cycles are cert-proven no-loss-equal to SQLite@`snapshot_uid`, so they are
/// substitutable for it; otherwise `None` ⇒ the request serves the canonical SQLite answer at the pinned
/// snapshot (the EV-A fail-soft).
///
/// **Build-then-peek** (see [`import_cert_eligibility`] for the full rationale + the TOCTOU it closes):
///   1. WARM — lazy-build the cycles cert at the current resident fingerprint iff stale/missing (a valid cert
///      is reused, preserving the zero-`find_cycles` green fastpath).
///   2. PEEK — under ONE livegraph read guard (which excludes a concurrent swap), recompute `current_fp` AND
///      peek a GREEN cycles cert at EXACTLY `current_fp`.
///
/// So `Some(fp)` is the EXACT resident-and-validated state, or `None`.
///
/// DAEMON-CANCEL-1: `cancel` is threaded into the WARM (re)build (`build_and_store_cycles_cert_cancellable`)
/// so the first-call-per-fingerprint cert build still cancels mid-Tarjan on a peer disconnect —
/// `Err(StorageError::Cancelled)` propagates to the handler (mapped to `ErrorCode::Cancelled`). This is the
/// SAME Tarjan that previously cancelled inside the serve ladder; build-then-peek only moved WHERE it runs.
pub(crate) fn cycles_cert_eligibility(
    repo_state: &RepoState,
    snapshot_uid: &str,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Option<String>, repo_graph_storage::error::StorageError> {
    // 1. WARM: lazy (re)build the cycles cert ONLY if stale/missing at the current resident fingerprint (the
    //    stale-check keeps a valid cert's serve zero-read). The read guard is dropped before the build so it
    //    can re-lock without deadlock.
    let warm_fp = {
        let guard = repo_state.livegraph.read();
        guard
            .as_ref()
            .map(|lg| import_cert_fingerprint(&lg.live_partitions(), snapshot_uid))
    };
    if let Some(fp) = warm_fp {
        let stale = !matches!(
            repo_state.cycles_cert.read().as_ref(),
            Some(c) if c.fingerprint == fp
        );
        if stale {
            build_and_store_cycles_cert_cancellable(repo_state, snapshot_uid, Some(fp), cancel)?;
        }
    }
    // 2. PEEK under ONE read guard so "(GREEN cycles cert) at (this exact resident fingerprint)" is atomic
    //    w.r.t. any swap.
    let guard = repo_state.livegraph.read();
    let current_fp = match guard.as_ref() {
        Some(lg) => import_cert_fingerprint(&lg.live_partitions(), snapshot_uid),
        None => return Ok(None),
    };
    let cached = repo_state.cycles_cert.read();
    Ok(match cached.as_ref() {
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN" => Some(current_fp),
        _ => None,
    })
}

/// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (D1=A / D4) + W-B-EPOCH-IMPL-2B (EV-A): the AUTO (default) `cycles`
/// response under the captured request `epoch`. Serves the GREEN-cert FASTPATH (the LiveGraph module cycles
/// WITHOUT SQLite) iff the precondition is met AND the resident cycles-cert fingerprint STILL equals the
/// captured green-validated `epoch.fingerprint` (the EV-A gate) ; else the canonical SQLite answer at the
/// pinned `epoch.snapshot_uid()`. The answer-class + the LiveGraph cycles + the current fingerprint are
/// SQLite-FREE; SQLite is read ONLY on the fallback. The served output is byte-identical either way
/// (CYCLES-OUTPUT-CONTRACT-1).
///
/// **TOCTOU closed.** `lg_cycles` + the precondition + the resident `current_fp` are captured under ONE read
/// guard, so the served cycles and the fingerprint they are validated by are the SAME resident partition set;
/// the gate then compares that `current_fp` against the PRE-validated `epoch.fingerprint` (built
/// BUILD-THEN-PEEK by [`cycles_cert_eligibility`] in the handler). A swap/straddle since capture moves
/// `current_fp`, so the gate fails and the request fails soft to the pinned SQLite snapshot — never a
/// green-labelled serve of an unvalidated module-cycle set. No lazy cert build happens here anymore (it moved
/// to the eligibility capture).
///
/// DAEMON-CANCEL-1: `cancel` is threaded into BOTH the precondition LiveGraph module-cycle SCC
/// (`module_import_cycles_cancellable`) and the SQLite SCC fallback (`serve_cycles_sqlite` →
/// `find_cycles_cancellable`); the cert build's cancellation is in [`cycles_cert_eligibility`]. So a peer
/// disconnect during ANY phase still cancels mid-flight.
pub fn cycles_auto_response(
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    epoch: &RequestEpoch,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<Value, repo_graph_storage::error::StorageError> {
    let snapshot_uid = epoch.snapshot_uid();
    // SQLite-FREE: the module-cycle answer-class (the precondition), the LiveGraph cycles (the served answer),
    // and the current fingerprint -- all from a single LiveGraph read lock.
    let (precondition_met, precondition_reason, lg_cycles, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => {
                // The in-loop layer: the module-cycle Tarjan is checkpointed, so the
                // default route cancels mid-traversal (not at the handler boundary).
                let env = lg
                    .module_import_cycles_cancellable(cancel)
                    .map_err(|_| repo_graph_storage::error::StorageError::Cancelled)?;
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
    // CYCLE-HONESTY-1 (§2.4, review-2 route-consistency): the type-only caveat basis is the SAME stored
    // per-language file facts (≥10% materiality) on BOTH the fastpath and the SQLite fallback — computed
    // here from a fresh read connection so the fastpath cannot diverge from `contributing_languages`
    // guesswork. This is a cheap grouped language COUNT, NOT the `find_cycles` Tarjan the fastpath exists to
    // avoid; the served cycle SET stays SQLite-free. Operator ruling 2026-08-28 item 1 explicitly mandates
    // the LiveGraph route read the same stored facts. CLASSIFIED read -> a genuine error PROPAGATES.
    let repo_has_ts_js = {
        let conn = repo_state.storage().map_err(|e| {
            repo_graph_storage::error::StorageError::InvalidArgument(format!(
                "failed to open storage connection for the TS/JS caveat: {e}"
            ))
        })?;
        snapshot_has_material_ts_js(&conn, snapshot_uid)?
    };
    // EV-A: serve the LiveGraph fastpath iff the resident fingerprint still equals the captured green-validated
    // eligibility witness; mismatch / None (a swap/straddle since capture, or no GREEN cycles cert) -> SQLite.
    let epoch_eligible = current_fp.is_some() && current_fp.as_ref() == epoch.fingerprint.as_ref();
    cycles_fastpath_or_sqlite(
        precondition_met,
        precondition_reason,
        epoch_eligible,
        || {
            serve_cycles_fastpath(
                repo_uid,
                display_name,
                snapshot_uid,
                &lg_cycles,
                repo_has_ts_js,
            )
        },
        |reason, cancel| {
            serve_cycles_sqlite(
                repo_state,
                repo_uid,
                display_name,
                snapshot_uid,
                reason,
                cancel,
            )
        },
        cancel,
    )
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

/// A `stats` response, together with whether the client disconnected DURING its heavy SQL
/// aggregation (DAEMON-CANCEL-2). The dispatcher maps `Ready` → success, `Cancelled` →
/// `ErrorCode::Cancelled`, and a returned `Err` → `ErrorCode::InternalError`.
///
/// Every production `stats` path that runs `compute_module_stats` — the default `auto`
/// SQLite fallback, the cert-build/`--engine compare` divergence read, and the
/// `--engine sqlite` escape hatch — runs it on a worker thread under CANCEL-1's supervisor
/// (see [`cancellable_module_stats`]), so a peer-disconnect aborts the in-flight `SELECT`
/// via `sqlite3_interrupt` rather than letting it run to completion with no consumer.
pub enum StatsOutcome {
    /// The query produced its response body (served to a still-connected peer).
    Ready(Value),
    /// The peer disconnected mid-aggregation; the in-flight `SELECT` was interrupted.
    Cancelled,
}

/// Result of a cancellable [`cancellable_module_stats`] call: the module-stats rows, or the
/// client disconnected mid-aggregation (the `sqlite3_interrupt` actuator aborted the
/// in-flight `SELECT`).
pub(crate) enum SqlStats {
    Stats(Vec<ModuleStatsResult>),
    Cancelled,
}

/// Outcome of the cancellable stats-cert build ([`build_and_store_stats_cert`]). `NotGreen`
/// folds the prior `Some(false)` (built RED) and `None` (no fingerprint / storage error)
/// cases — both fall back to SQLite exactly as before; `Cancelled` is the new mid-build
/// disconnect path.
#[derive(Debug, PartialEq, Eq)]
enum CertBuild {
    Green,
    NotGreen,
    Cancelled,
}

/// Internal: the SQLite-vs-LiveGraph compare data, or the client disconnected during the
/// SQLite half's `compute_module_stats`.
enum CompareOutcome {
    Computed(StatsCompareData),
    Cancelled,
}

/// DAEMON-CANCEL-2: run `compute_module_stats` on a worker thread under CANCEL-1's
/// [`run_interruptible`](crate::cancel::run_interruptible) supervisor, cancellable via
/// `sqlite3_interrupt` on peer-disconnect. THE single chokepoint every production `stats`
/// SQL site funnels through, so "stats cancels mid-execution on disconnect" holds on the
/// DEFAULT `auto` path — not only the explicit `--engine sqlite` escape hatch (the
/// iteration-0 gap).
///
/// ## Connection / interrupt-handle ownership (B1 D-S = S-A, the slice's key design point)
///
/// `conn` is the caller's OWN per-operation `StorageConnection`, opened on the transport
/// thread (in the leaf, e.g. [`serve_stats_sqlite`] / [`stats_compare_data`]). We hoist its
/// interrupt handle out HERE, BEFORE moving the connection into the worker, and hand the
/// handle to the supervising (transport) thread as the `on_disconnect` actuator. So the
/// handle is obtained from the SAME connection the worker blocks inside, before the blocking
/// call — the slice's STOP-condition shape ("the worker opens its connection internally and
/// the handle can't be hoisted") does NOT arise: the connection is opened in the leaf and
/// passed in, never opened inside the worker. Firing the handle after the worker drops the
/// connection is a safe no-op (see `StorageInterruptHandle`). Connection-per-op ⇒ no reuse ⇒
/// a late interrupt cannot bleed into a later statement (there is none).
///
/// Read-only ⇒ a cancelled query has no partial state to roll back. A worker that vanishes
/// (panic / internal teardown while the peer was connected) is an INTERNAL failure
/// (`WorkerVanished`), surfaced as a storage error → `InternalError`, NEVER `Cancelled`
/// (CANCEL-1 deliverable #2).
pub(crate) fn cancellable_module_stats(
    emitter: &mut dyn ProgressEmitter,
    conn: StorageConnection,
    snapshot_uid: &str,
) -> Result<SqlStats, StorageError> {
    // Hoist the interrupt handle BEFORE the connection moves into the worker (S-A: this is
    // the leaf's own connection, so the handle is from the exact connection the worker uses).
    let interrupt = conn.interrupt_handle();
    let snap = snapshot_uid.to_string();
    match crate::cancel::run_interruptible(
        emitter,
        "computing_module_stats",
        // On peer-disconnect: `sqlite3_interrupt` the in-flight SELECT (an opaque SQL
        // statement has no Rust frame to poll the cooperative `CancelFlag`).
        move || interrupt.interrupt(),
        // The worker OWNS the connection. Its `CancelFlag` is intentionally unused: only the
        // interrupt handle can abort a single opaque statement mid-execution.
        move |_flag| conn.compute_module_stats(&snap),
    ) {
        crate::cancel::Supervised::Completed(Ok(stats)) => Ok(SqlStats::Stats(stats)),
        // A genuine SQL/storage failure while the peer stayed connected — never a cancel.
        crate::cancel::Supervised::Completed(Err(e)) => Err(e),
        crate::cancel::Supervised::Cancelled => Ok(SqlStats::Cancelled),
        // Worker panic / internal teardown: INTERNAL failure, classified as such — NEVER
        // masqueraded as a client cancel. Mapped to a storage error (→ `InternalError`).
        crate::cancel::Supervised::WorkerVanished => Err(StorageError::InvalidArgument(
            "stats worker vanished (internal failure during aggregation)".to_string(),
        )),
    }
}

/// Compute the shared [`StatsCompareData`] — the SQLite `compute_module_stats` answer vs the LiveGraph
/// `module_stats` answer (mapped to the same DTO), compared per-module by module identity then field-
/// exact (`ModuleStatsResult: PartialEq`; the floats are bit-identical by the shared `martin_metrics`).
/// Reads SQLite once + the LiveGraph once (one read lock). DAEMON-CANCEL-2: the SQLite half runs
/// through [`cancellable_module_stats`], so a peer-disconnect mid-`compute_module_stats` aborts the
/// `SELECT` and returns [`CompareOutcome::Cancelled`].
fn stats_compare_data(
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> Result<CompareOutcome, StorageError> {
    use std::collections::BTreeMap;
    // D-S = S-A: one fresh per-operation connection for this read.
    let conn = repo_state.storage().map_err(|e| {
        StorageError::InvalidArgument(format!("failed to open storage connection: {e}"))
    })?;
    let sqlite_stats = match cancellable_module_stats(emitter, conn, snapshot_uid)? {
        SqlStats::Stats(s) => s,
        SqlStats::Cancelled => return Ok(CompareOutcome::Cancelled),
    };
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
    Ok(CompareOutcome::Computed(StatsCompareData {
        livegraph_stats,
        sqlite_stats,
        livegraph_class,
        is_exact,
        missing_in_livegraph,
        extra_in_livegraph,
        field_mismatches,
    }))
}

/// STATS-LIVEGRAPH-IMPL-1 (build): run the SHARED field-exact stats compare -> verdict, STORE the cert
/// keyed by `fingerprint`, return a [`CertBuild`] (`Green`/`NotGreen`, or `Cancelled` if the peer
/// disconnected during the compare's `compute_module_stats`). Reads SQLite ONCE per fingerprint via the
/// SAME [`stats_compare_data`] the `--engine compare` uses, so the GREEN verdict PROVABLY matches the
/// compare (no drift -> no false GREEN). DAEMON-CANCEL-2: the SQLite read is cancellable, so a disconnect
/// mid-cert-build aborts the in-flight `SELECT` instead of running it to completion.
fn build_and_store_stats_cert(
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> CertBuild {
    // No fingerprint ⇒ cannot key a cert; treat as not-green (the prior `None` → `false`).
    let Some(fingerprint) = fingerprint else {
        return CertBuild::NotGreen;
    };
    let data = match stats_compare_data(emitter, repo_state, snapshot_uid) {
        Ok(CompareOutcome::Computed(d)) => d,
        Ok(CompareOutcome::Cancelled) => return CertBuild::Cancelled,
        // Storage error during the cert build ⇒ fall back to SQLite (the prior `.ok()?` → `None`
        // → `false`); the fallback serve re-runs and surfaces the same error if it persists.
        Err(_) => return CertBuild::NotGreen,
    };
    let is_green = data.is_exact;
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.stats_cert.write() = Some(StatsNoLossCert {
        verdict,
        fingerprint,
    });
    if is_green {
        CertBuild::Green
    } else {
        CertBuild::NotGreen
    }
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
/// path (the DEFAULT `auto` route when the LiveGraph is non-resident / non-`Exact` or the cert is RED).
/// DAEMON-CANCEL-2: reads SQLite (`compute_module_stats`) through [`cancellable_module_stats`], so a
/// peer-disconnect mid-aggregation aborts the in-flight `SELECT` ⇒ [`StatsOutcome::Cancelled`]. Distinct
/// from the forced `--engine sqlite` arm, which returns the UNCHANGED body (no `backend_used`) per D4.
fn serve_stats_sqlite(
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    fallback_reason: FallbackReason,
) -> Result<StatsOutcome, StorageError> {
    // D-S = S-A: one fresh per-operation connection for this read.
    let conn = repo_state.storage().map_err(|e| {
        StorageError::InvalidArgument(format!("failed to open storage connection: {e}"))
    })?;
    let stats = match cancellable_module_stats(emitter, conn, snapshot_uid)? {
        SqlStats::Stats(s) => s,
        SqlStats::Cancelled => return Ok(StatsOutcome::Cancelled),
    };
    Ok(StatsOutcome::Ready(json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "display_name": display_name,
        "stats": stats,
        "count": stats.len(),
        "backend_used": "sqlite",
        "fallback_reason": fallback_reason.as_str(),
    })))
}

/// STATS-LIVEGRAPH-IMPL-1 (D3) + W-B-EPOCH-IMPL-2B (EV-A): the fastpath/SQLite ladder under the captured
/// request epoch (mirrors `cycles_fastpath_or_sqlite`). precondition UNMET (the LiveGraph module-stats answer
/// is not `Exact` -- non-resident / non-TS / degraded) -> SQLite (the labelled `precondition_reason`) ;
/// precondition met AND `epoch_eligible` -> serve LiveGraph WITHOUT `compute_module_stats` (the stats cert
/// proved the resident module stats no-loss-equal to SQLite@`snapshot_uid`) ; NOT eligible (no GREEN stats
/// cert at capture, OR a swap/straddle since capture so the resident fingerprint moved) -> SQLite
/// (`LiveGraphStatsDivergence`). The decision stays pure: a panicking `serve_sqlite` proves the eligible
/// path skips SQLite. No lazy cert build happens here anymore (it moved to [`stats_cert_eligibility`],
/// closing the capture-LG-stats-then-lazy-cert-build TOCTOU). DAEMON-CANCEL-2: `serve_sqlite` runs
/// `compute_module_stats` under the supervisor, so it takes `emitter` and may report a mid-aggregation
/// disconnect, which the ladder forwards as [`StatsOutcome::Cancelled`].
fn stats_fastpath_or_sqlite(
    emitter: &mut dyn ProgressEmitter,
    precondition_met: bool,
    precondition_reason: FallbackReason,
    epoch_eligible: bool,
    serve_livegraph: impl FnOnce() -> Value,
    serve_sqlite: impl FnOnce(
        &mut dyn ProgressEmitter,
        FallbackReason,
    ) -> Result<StatsOutcome, StorageError>,
) -> Result<StatsOutcome, StorageError> {
    if !precondition_met {
        return serve_sqlite(emitter, precondition_reason);
    }
    if epoch_eligible {
        Ok(StatsOutcome::Ready(serve_livegraph()))
    } else {
        serve_sqlite(emitter, FallbackReason::LiveGraphStatsDivergence)
    }
}

/// W-B-EPOCH-IMPL-2B: outcome of [`stats_cert_eligibility`] — the build-then-peek witness, or the peer
/// disconnected during the WARM cert (re)build's `compute_module_stats`. A dedicated `Cancelled` variant
/// (rather than folding into `StorageError`) mirrors the stats module's existing cancel convention
/// ([`StatsOutcome`] / [`SqlStats`] / `CertBuild`): a cancelled aggregation is an interrupted SELECT, not a
/// storage error.
pub(crate) enum StatsEligibility {
    /// The build-then-peek eligibility witness: `Some(fp)` = a GREEN stats cert at EXACTLY the resident
    /// fingerprint (the resident module stats are cert-proven no-loss-equal to SQLite@`snapshot_uid`);
    /// `None` = no green cert ⇒ eager SQLite, no LiveGraph serve.
    Witness(Option<String>),
    /// The peer disconnected during the WARM cert (re)build (the in-flight `SELECT` was interrupted) — the
    /// handler maps this to `ErrorCode::Cancelled`.
    Cancelled,
}

/// W-B-EPOCH-IMPL-2B (D-EP capture for `stats`; `daemon-w-b-epoch-1.md` §6.4): the STATS-cert LG-serve
/// eligibility WITNESS, captured BUILD-THEN-PEEK. The stats sibling of [`import_cert_eligibility`] /
/// [`cycles_cert_eligibility`] (`stats` serves the LiveGraph module STATS, whose no-loss proof is the STATS
/// cert). Returns `StatsEligibility::Witness(Some(current_fp))` iff a GREEN stats cert exists at EXACTLY the
/// resident fingerprint for `snapshot_uid` — i.e. the resident partitions' module stats are cert-proven
/// no-loss-equal to SQLite@`snapshot_uid`, so they are substitutable for it; otherwise `Witness(None)` ⇒ the
/// request serves the SQLite answer at the pinned snapshot (the EV-A fail-soft).
///
/// **Build-then-peek** (see [`import_cert_eligibility`] for the full rationale + the TOCTOU it closes):
///   1. WARM — lazy-build the stats cert at the current resident fingerprint iff stale/missing (a valid cert
///      is reused, preserving the zero-`compute_module_stats` green fastpath).
///   2. PEEK — under ONE livegraph read guard (which excludes a concurrent swap), recompute `current_fp` AND
///      peek a GREEN stats cert at EXACTLY `current_fp`.
///
/// So the witness is the EXACT resident-and-validated state, or `None`.
///
/// DAEMON-CANCEL-2: the WARM (re)build runs `compute_module_stats` under the supervisor
/// ([`cancellable_module_stats`]), so a peer disconnect mid-aggregation returns [`StatsEligibility::Cancelled`]
/// (the in-flight `SELECT` is aborted) rather than a stale witness. This is the SAME aggregation that
/// previously cancelled inside the serve ladder; build-then-peek only moved WHERE it runs.
pub(crate) fn stats_cert_eligibility(
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> StatsEligibility {
    // 1. WARM: lazy (re)build the stats cert ONLY if stale/missing at the current resident fingerprint (the
    //    stale-check keeps a valid cert's serve zero-read). The read guard is dropped before the build so it
    //    can re-lock without deadlock.
    let warm_fp = {
        let guard = repo_state.livegraph.read();
        guard
            .as_ref()
            .map(|lg| import_cert_fingerprint(&lg.live_partitions(), snapshot_uid))
    };
    if let Some(fp) = warm_fp {
        let stale = !matches!(
            repo_state.stats_cert.read().as_ref(),
            Some(c) if c.fingerprint == fp
        );
        if stale
            && build_and_store_stats_cert(emitter, repo_state, snapshot_uid, Some(fp))
                == CertBuild::Cancelled
        {
            return StatsEligibility::Cancelled;
        }
    }
    // 2. PEEK under ONE read guard so "(GREEN stats cert) at (this exact resident fingerprint)" is atomic
    //    w.r.t. any swap.
    let guard = repo_state.livegraph.read();
    let current_fp = match guard.as_ref() {
        Some(lg) => import_cert_fingerprint(&lg.live_partitions(), snapshot_uid),
        None => return StatsEligibility::Witness(None),
    };
    let cached = repo_state.stats_cert.read();
    StatsEligibility::Witness(match cached.as_ref() {
        Some(c) if c.fingerprint == current_fp && c.verdict == "GREEN" => Some(current_fp),
        _ => None,
    })
}

/// STATS-LIVEGRAPH-IMPL-1 (D3) + W-B-EPOCH-IMPL-2B (EV-A): the AUTO (default) `stats` response under the
/// captured request `epoch` — the path `rmap stats` takes with no `--engine` flag. Serves the GREEN-cert
/// FASTPATH (the LiveGraph module stats WITHOUT SQLite) iff the precondition is met AND the resident
/// stats-cert fingerprint STILL equals the captured green-validated `epoch.fingerprint` (the EV-A gate) ;
/// else SQLite (the proven answer) at the pinned `epoch.snapshot_uid()`. The answer-class + the LiveGraph
/// stats + the current fingerprint are SQLite-FREE; SQLite is read ONLY on the fallback. The served human
/// output is byte-identical either way (the byte-preserving contract).
///
/// **TOCTOU closed.** `lg_stats` + the precondition + the resident `current_fp` are captured under ONE read
/// guard, so the served stats and the fingerprint they are validated by are the SAME resident partition set;
/// the gate then compares that `current_fp` against the PRE-validated `epoch.fingerprint` (built
/// BUILD-THEN-PEEK by [`stats_cert_eligibility`] in the handler). A swap/straddle since capture moves
/// `current_fp`, so the gate fails and the request fails soft to the pinned SQLite snapshot — never a
/// green-labelled serve of an unvalidated stat set. No lazy cert build happens here anymore (it moved to the
/// eligibility capture).
///
/// DAEMON-CANCEL-2: the SQLite fallback runs `compute_module_stats` under the supervisor, so a peer-disconnect
/// mid-aggregation returns [`StatsOutcome::Cancelled`] (the in-flight `SELECT` is aborted); the cert build's
/// cancellation is in [`stats_cert_eligibility`]. So the DEFAULT route still cancels mid-flight in every phase.
pub fn stats_auto_response(
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    epoch: &RequestEpoch,
) -> Result<StatsOutcome, StorageError> {
    let snapshot_uid = epoch.snapshot_uid();
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
    // EV-A: serve the LiveGraph fastpath iff the resident fingerprint still equals the captured green-validated
    // eligibility witness; mismatch / None (a swap/straddle since capture, or no GREEN stats cert) -> SQLite.
    let epoch_eligible = current_fp.is_some() && current_fp.as_ref() == epoch.fingerprint.as_ref();
    stats_fastpath_or_sqlite(
        emitter,
        precondition_met,
        precondition_reason,
        epoch_eligible,
        || serve_stats_fastpath(repo_uid, display_name, snapshot_uid, &lg_stats),
        |e, reason| serve_stats_sqlite(e, repo_state, repo_uid, display_name, snapshot_uid, reason),
    )
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
    emitter: &mut dyn ProgressEmitter,
    repo_state: &RepoState,
    repo_uid: &str,
    display_name: &str,
    snapshot_uid: &str,
    repo_root: &str,
) -> Result<StatsOutcome, StorageError> {
    // DAEMON-CANCEL-2: the SQLite half of the compare runs under the supervisor; a mid-aggregation
    // disconnect aborts the in-flight SELECT and returns Cancelled.
    let data = match stats_compare_data(emitter, repo_state, snapshot_uid)? {
        CompareOutcome::Computed(d) => d,
        CompareOutcome::Cancelled => return Ok(StatsOutcome::Cancelled),
    };
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
    Ok(StatsOutcome::Ready(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W-B-EPOCH-IMPL-1 headless coherence proofs (under W-A), on the SHARED faithful fixture
    /// (`callgraph_cert::test_fixture`: a resident LiveGraph byte-mirrored by SQLite, so a GREEN cert IS
    /// value parity). Covers the two eligibility witnesses (build-then-peek) and the EV-A serve-time gate at
    /// BOTH serve sites — the callers/callees Auto arm (this module) and the orient/explain decorator: a
    /// mid-request LiveGraph swap (the W-B race, simulated by `load_partition` bumping the partition epoch so
    /// the resident fingerprint moves) makes each leaf fail soft to the PINNED SQLite snapshot — never a
    /// cross-epoch mix. Relocated here from the removed `request_epoch` module (review-0 #3) — beside
    /// `RequestEpoch`'s new home and the callers/callees engine-response under test.
    mod wb_epoch_coherence {
        use super::NoEmit;
        use crate::callgraph_cert::{callgraph_cert_eligibility, test_fixture};
        use crate::livegraph_feed::{
            callees_engine_response, callers_engine_response, cycles_auto_response,
            cycles_cert_eligibility, file_import_cycles_response, import_cert_eligibility,
            import_cert_fingerprint, imports_auto_response, module_import_cycles_response,
            path_engine_response, stats_auto_response, stats_cert_eligibility, Engine,
            ImportNoLossCert, RequestEpoch, StatsEligibility, StatsNoLossCert, StatsOutcome,
        };
        use crate::orient_serve::{
            orient_bounded_cert_eligibility, orient_bounded_cert_is_green, OrientServeDecorator,
        };
        use repo_graph_agent::AgentStorageRead;
        use repo_graph_storage::error::StorageError;
        use repo_graph_storage::queries::ResolvedSymbol;
        use repo_graph_storage::StorageConnection;
        use repo_graph_trust_model::LanguageSupport;
        use serde_json::{json, Value};

        /// Capture the request epoch for the fixture, using the given eligibility witness.
        fn capture(
            state: &crate::state::RepoState,
            snapshot_uid: &str,
            fingerprint: Option<String>,
        ) -> RequestEpoch {
            let storage = state.storage().unwrap();
            let snapshot = AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
                .unwrap()
                .unwrap();
            assert_eq!(snapshot.snapshot_uid, snapshot_uid);
            RequestEpoch {
                snapshot,
                fingerprint,
            }
        }

        /// Simulate a refresh's LiveGraph swap: re-feed the partition, which bumps its epoch in place — so
        /// the resident `import_cert_fingerprint` changes (epochs are monotonic, fingerprints never recur).
        fn swap_livegraph(state: &crate::state::RepoState) {
            state.livegraph.write().as_mut().unwrap().load_partition(
                "p",
                test_fixture::build_ir(),
                LanguageSupport::TypeScriptPrimary,
            );
        }

        fn resolved(stable_key: &str, name: &str) -> ResolvedSymbol {
            ResolvedSymbol {
                stable_key: stable_key.to_string(),
                name: name.to_string(),
                qualified_name: None,
                kind: "SYMBOL".to_string(),
                subtype: None,
                file: None,
                line: None,
                column: None,
            }
        }

        // ── Eligibility witnesses (build-then-peek) ──────────────────────────────────────────────

        #[test]
        fn callgraph_eligibility_is_the_exact_resident_fingerprint_on_green() {
            let f = test_fixture::build_fixture(false);
            let fp = callgraph_cert_eligibility(&f.state, &f.snapshot_uid)
                .expect("green mirror -> Some eligibility");
            // The witness IS the exact resident-and-validated fingerprint (build-then-peek, §6.4).
            let current = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            assert_eq!(fp, current);
        }

        #[test]
        fn callgraph_eligibility_none_on_red_and_without_livegraph() {
            // RED: the SQLite mirror drops the CALLS edge -> the callgraph cert is RED -> no witness.
            let red = test_fixture::build_fixture(true);
            assert!(callgraph_cert_eligibility(&red.state, &red.snapshot_uid).is_none());
            // No resident LiveGraph -> no witness (eager SQLite).
            let f = test_fixture::build_fixture(false);
            *f.state.livegraph.write() = None;
            assert!(callgraph_cert_eligibility(&f.state, &f.snapshot_uid).is_none());
        }

        #[test]
        fn bounded_eligibility_some_iff_bounded_cert_green() {
            let f = test_fixture::build_fixture(false);
            assert_eq!(
                orient_bounded_cert_eligibility(&f.state, &f.snapshot_uid).is_some(),
                orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
                "eligibility.is_some() is the SAME serve decision as the prior bool gate"
            );
            *f.state.livegraph.write() = None;
            assert!(orient_bounded_cert_eligibility(&f.state, &f.snapshot_uid).is_none());
        }

        // ── EV-A at the callers/callees Auto arm ─────────────────────────────────────────────────

        #[test]
        fn ev_a_callers_serves_livegraph_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            let storage = f.state.storage().unwrap();
            let fingerprint = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            let target = resolved(&test_fixture::callee_key(), "calleeFn");
            let edge = ["CALLS"];

            // Steady state (no swap): the Auto serve hits the LiveGraph (transparency) — callerFn->calleeFn.
            let v = callers_engine_response(
                Engine::Auto,
                &f.state,
                &epoch,
                &target,
                || storage.find_direct_callers(epoch.snapshot_uid(), &target.stable_key, &edge),
                "calleeFn",
                "",
            )
            .unwrap();
            assert_eq!(v["backend_used"], "livegraph");
            assert_eq!(v["callers"][0]["stable_key"], test_fixture::caller_key());

            // EV-A: a mid-request LiveGraph swap moves the resident fingerprint; the captured epoch no longer
            // matches -> fail soft to SQLite AT THE PINNED snapshot (coherent — the same caller, never N+1).
            swap_livegraph(&f.state);
            let v2 = callers_engine_response(
                Engine::Auto,
                &f.state,
                &epoch,
                &target,
                || storage.find_direct_callers(epoch.snapshot_uid(), &target.stable_key, &edge),
                "calleeFn",
                "",
            )
            .unwrap();
            assert_eq!(v2["backend_used"], "sqlite");
            assert_eq!(v2["callers"][0]["stable_key"], test_fixture::caller_key());
        }

        #[test]
        fn ev_a_callees_serves_livegraph_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            let storage = f.state.storage().unwrap();
            let fingerprint = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            let target = resolved(&test_fixture::caller_key(), "callerFn");
            let edge = ["CALLS"];

            let v = callees_engine_response(
                Engine::Auto,
                &f.state,
                &epoch,
                &target,
                || storage.find_direct_callees(epoch.snapshot_uid(), &target.stable_key, &edge),
                "callerFn",
                "",
            )
            .unwrap();
            assert_eq!(v["backend_used"], "livegraph");
            assert_eq!(v["callees"][0]["stable_key"], test_fixture::callee_key());

            swap_livegraph(&f.state);
            let v2 = callees_engine_response(
                Engine::Auto,
                &f.state,
                &epoch,
                &target,
                || storage.find_direct_callees(epoch.snapshot_uid(), &target.stable_key, &edge),
                "callerFn",
                "",
            )
            .unwrap();
            assert_eq!(v2["backend_used"], "sqlite");
            assert_eq!(v2["callees"][0]["stable_key"], test_fixture::callee_key());
        }

        // ── EV-A at the orient/explain decorator ─────────────────────────────────────────────────

        #[test]
        fn ev_a_decorator_serves_livegraph_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            let storage: StorageConnection = f.state.storage().unwrap();
            let fingerprint = orient_bounded_cert_eligibility(&f.state, &f.snapshot_uid);
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            let callee = test_fixture::callee_key();

            // Make SQLite DIVERGE from the LiveGraph so the two serve sites are distinguishable: delete the
            // SQLite CALLS edge. SQLite callers(calleeFn) is now empty; the LiveGraph still has callerFn. A
            // SQLite-only mutation does NOT touch the LiveGraph fingerprint, so the captured epoch matches.
            storage.delete_edges_by_uids(&["ec0".to_string()]).unwrap();

            let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);

            // Green (epoch matches the resident fingerprint): SERVE the LiveGraph -> callerFn, DESPITE the now
            // empty SQLite. Proves the decorator serves LG on a matching epoch.
            let lg_rows = decorator
                .find_symbol_callers(epoch.snapshot_uid(), &callee)
                .unwrap();
            assert_eq!(lg_rows.len(), 1);
            assert_eq!(lg_rows[0].stable_key, test_fixture::caller_key());

            // EV-A: swap the LiveGraph (epoch no longer matches) -> the decorator fails soft to the PINNED
            // SQLite snapshot, which we mutated to empty. Proves the swap routes to SQLite, not the stale LG.
            swap_livegraph(&f.state);
            let sqlite_rows = decorator
                .find_symbol_callers(epoch.snapshot_uid(), &callee)
                .unwrap();
            assert!(
                sqlite_rows.is_empty(),
                "stale captured epoch -> delegate to the pinned SQLite snapshot (mutated to empty), \
                 NEVER serve the swapped LiveGraph"
            );
        }

        // ── W-B-EPOCH-IMPL-2A: `imports` build-then-peek eligibility + EV-A ───────────────────────

        /// The import-cert eligibility witness is BUILD-THEN-PEEK: `Some` ONLY when a GREEN import cert exists
        /// at EXACTLY the resident fingerprint, and the returned fingerprint IS that exact resident state.
        #[test]
        fn import_cert_eligibility_is_some_only_on_green_at_the_exact_resident_fingerprint() {
            let f = test_fixture::build_fixture(false);
            // The faithful fixture has NO import edges -> the import cert builds YELLOW (`met == 0`) -> the
            // warm builds it, the peek finds it non-GREEN -> no witness (eager SQLite).
            assert!(
                import_cert_eligibility(&f.state, test_fixture::REPO, &f.snapshot_uid).is_none(),
                "no import-bearing files -> YELLOW import cert -> no eligibility witness"
            );
            // Pre-store a GREEN import cert at EXACTLY the resident fingerprint (what the producer stores on a
            // no-loss repo). The warm sees a valid (non-stale) cert -> NO rebuild -> the peek hits -> Some(fp),
            // and the witness IS the exact resident-and-validated fingerprint.
            let resident_fp = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            *f.state.import_cert.write() = Some(ImportNoLossCert {
                verdict: "GREEN".to_string(),
                fingerprint: resident_fp.clone(),
            });
            let fp = import_cert_eligibility(&f.state, test_fixture::REPO, &f.snapshot_uid)
                .expect("GREEN import cert at the resident fingerprint -> Some eligibility");
            assert_eq!(
                fp, resident_fp,
                "the witness IS the exact resident-and-validated fingerprint"
            );

            // Honesty under a lazy rebuild straddle (§6.4): a swap bumps the partition epoch so the resident
            // fingerprint MOVES; the pre-stored GREEN cert is now at the OLD fp; the warm rebuilds at the NEW
            // fp (YELLOW, no imports) and the peek at the new fp finds no GREEN -> None. The stale GREEN
            // witness is NEVER returned (monotonic epochs: the old fp never recurs).
            swap_livegraph(&f.state);
            assert!(
                import_cert_eligibility(&f.state, test_fixture::REPO, &f.snapshot_uid).is_none(),
                "after a swap the witness is None, never the stale pre-swap fingerprint"
            );

            // No resident LiveGraph -> None (eager SQLite).
            *f.state.livegraph.write() = None;
            assert!(
                import_cert_eligibility(&f.state, test_fixture::REPO, &f.snapshot_uid).is_none()
            );
        }

        /// EV-A at the `imports` auto serve (TOCTOU closed): on a matching epoch the GREEN-cert FASTPATH
        /// serves the LiveGraph (zero SQLite read; `comparison.source` is the no-loss certificate); a
        /// mid-request swap moves the resident fingerprint so the captured epoch no longer matches -> the serve
        /// fails soft to the per-call compare-on-call at the PINNED snapshot (`comparison.sqlite_resolved_local`
        /// present; NO cert source) — never a cert-labelled serve of the unvalidated post-swap view.
        #[test]
        fn ev_a_imports_serves_cert_fastpath_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            // GREEN import cert at the resident fingerprint -> the eligibility witness is Some.
            let resident_fp = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            *f.state.import_cert.write() = Some(ImportNoLossCert {
                verdict: "GREEN".to_string(),
                fingerprint: resident_fp.clone(),
            });
            let fingerprint =
                import_cert_eligibility(&f.state, test_fixture::REPO, &f.snapshot_uid);
            assert!(fingerprint.is_some(), "GREEN import cert -> eligible");
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            // CALLER_PATH ("src/a.ts") is a resident TS file -> the precondition is met.
            let file = test_fixture::CALLER_PATH;

            // Steady state: epoch matches the resident fingerprint -> the cert FASTPATH (LiveGraph, the no-loss
            // certificate source).
            let v = imports_auto_response(&f.state, test_fixture::REPO, &epoch, file);
            assert_eq!(v["backend_used"], "livegraph");
            assert_eq!(v["comparison"]["source"], "repo_no_loss_certificate");

            // EV-A: a swap moves the resident fingerprint; the captured epoch no longer matches -> the serve
            // routes to the per-call compare-on-call at the pin (reads SQLite). (The empty-imports fixture
            // serves `livegraph` VACUOUSLY here — nothing to lose — so the observable EV-A signal is the
            // compare path's `sqlite_resolved_local`, NOT the cert `source`.)
            swap_livegraph(&f.state);
            let v2 = imports_auto_response(&f.state, test_fixture::REPO, &epoch, file);
            assert!(
                v2["comparison"]["sqlite_resolved_local"].is_number(),
                "stale epoch -> the per-call SQLite compare-on-call at the pin (not the cert fastpath)"
            );
            assert!(
                v2["comparison"]["source"].is_null(),
                "the post-swap serve is NEVER labelled with the no-loss certificate source"
            );
        }

        // ── W-B-EPOCH-IMPL-2A: `path` serves the pinned SQLite snapshot (§14 D-CC refined) ────────

        /// `path`'s default (`Engine::Auto`) serves the PINNED SQLite snapshot — NOT the LiveGraph BFS — even
        /// though the resident LiveGraph HAS the path. Closes the §1c false-freshness stamp (the served BFS is
        /// now genuinely as-of the stamped `snapshot_uid`). The explicit `--engine livegraph` diagnostic STILL
        /// serves the LiveGraph (the BFS machinery is preserved for the deferred CALLS∪IMPORTS re-enable).
        #[test]
        fn path_auto_serves_pinned_sqlite_not_livegraph() {
            let f = test_fixture::build_fixture(false);
            let from = test_fixture::caller_key();
            let to = test_fixture::callee_key();

            // Engine::Auto -> serve the pinned SQLite snapshot (the sentinel closure). The LiveGraph BFS is NOT
            // consulted, so the served path is the SQLite sentinel and the stamp is the pinned snapshot_uid.
            let mut cancel = || std::ops::ControlFlow::Continue(());
            let auto = path_engine_response(
                Engine::Auto,
                &f.state,
                &from,
                &to,
                test_fixture::REPO,
                &f.snapshot_uid,
                || -> Result<Value, StorageError> {
                    Ok(json!({
                        "repo_uid": test_fixture::REPO,
                        "snapshot_uid": f.snapshot_uid,
                        "path": { "found": true, "path": [{ "symbol": "from_sqlite" }] },
                        "found": true,
                    }))
                },
                "",
                &mut cancel,
            )
            .unwrap();
            assert_eq!(
                auto["backend_used"], "sqlite",
                "path Auto serves the pinned SQLite snapshot, NOT the LiveGraph BFS"
            );
            assert_eq!(
                auto["snapshot_uid"], f.snapshot_uid,
                "stamped with the pinned snapshot"
            );
            assert_eq!(
                auto["path"]["path"][0]["symbol"], "from_sqlite",
                "the SERVED path is the SQLite answer (the LiveGraph BFS was not consulted)"
            );

            // Contrast: the explicit --engine livegraph diagnostic STILL serves the LiveGraph BFS (preserved);
            // the resident LiveGraph has callerFn->calleeFn, so the SQLite closure is never read.
            let mut cancel2 = || std::ops::ControlFlow::Continue(());
            let lg = path_engine_response(
                Engine::LiveGraph,
                &f.state,
                &from,
                &to,
                test_fixture::REPO,
                &f.snapshot_uid,
                || -> Result<Value, StorageError> {
                    panic!("explicit --engine livegraph must not read SQLite when the LiveGraph serves")
                },
                "",
                &mut cancel2,
            )
            .unwrap();
            assert_eq!(lg["backend_used"], "livegraph");
            assert_eq!(lg["found"], true);
        }

        // ── W-B-EPOCH-IMPL-2B: `cycles` build-then-peek eligibility + EV-A ────────────────────────

        /// The cycles-cert eligibility witness is BUILD-THEN-PEEK: `Some` ONLY when a GREEN cycles cert
        /// exists at EXACTLY the resident fingerprint, and the returned fingerprint IS that exact resident
        /// state; after a swap it tracks the NEW resident state, never the stale captured fingerprint.
        #[test]
        fn cycles_cert_eligibility_is_the_exact_resident_fingerprint_on_green() {
            let f = test_fixture::build_fixture(false);
            let mut never = || std::ops::ControlFlow::Continue(());
            // The faithful fixture has 0 module-import cycles on BOTH sides -> the cycles cert builds GREEN;
            // the WARM builds it and the PEEK returns the EXACT resident fingerprint (build-then-peek, §6.4).
            let fp = cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                .expect("eligibility never errors here")
                .expect("GREEN cycles cert at the resident fingerprint -> Some eligibility");
            let resident_fp = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            assert_eq!(
                fp, resident_fp,
                "the witness IS the exact resident-and-validated fingerprint"
            );

            // Honesty under a swap (§6.4): a swap bumps the partition epoch so the resident fingerprint
            // MOVES; the witness is re-derived against the NEW resident state, NEVER the stale captured fp
            // (monotonic epochs: the old fp never recurs).
            swap_livegraph(&f.state);
            let after = cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                .expect("eligibility never errors here");
            assert_ne!(
                after.as_deref(),
                Some(resident_fp.as_str()),
                "after a swap the witness is never the stale pre-swap fingerprint"
            );

            // No resident LiveGraph -> None (eager SQLite).
            *f.state.livegraph.write() = None;
            assert!(
                cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                    .expect("eligibility never errors here")
                    .is_none()
            );
        }

        /// EV-A at the `cycles` auto serve (TOCTOU closed): on a matching epoch the GREEN-cert FASTPATH serves
        /// the LiveGraph module cycles (`backend_used=livegraph`); a mid-request swap moves the resident
        /// fingerprint so the captured epoch no longer matches -> the serve fails soft to the canonical SQLite
        /// answer AT THE PINNED snapshot (`backend_used=sqlite`, stamped with the pinned `snapshot_uid`).
        #[test]
        fn ev_a_cycles_serves_livegraph_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            let mut never = || std::ops::ControlFlow::Continue(());
            let fingerprint = cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                .expect("eligibility never errors here");
            assert!(
                fingerprint.is_some(),
                "faithful fixture -> GREEN cycles cert -> eligible"
            );
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);

            // Steady state (no swap): the epoch matches the resident fingerprint -> serve the LiveGraph cycles.
            let v = cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                .unwrap();
            assert_eq!(v["backend_used"], "livegraph");

            // EV-A: a mid-request swap moves the resident fingerprint; the captured epoch no longer matches ->
            // fail soft to the canonical SQLite answer AT THE PINNED snapshot (coherent at the pin).
            swap_livegraph(&f.state);
            let v2 = cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                .unwrap();
            assert_eq!(v2["backend_used"], "sqlite");
            assert_eq!(
                v2["snapshot_uid"], f.snapshot_uid,
                "the SQLite fallback is stamped with the PINNED snapshot"
            );
        }

        /// CYCLE-HONESTY-1 (§2.4, review-3 finding 1): the TS/JS type-only caveat is ROUTE-CONSISTENT on a
        /// RESIDENT LiveGraph. Every `cycles` route — the DEFAULT `auto` route SERVING the LiveGraph fastpath
        /// (`backend_used=livegraph`, NOT a silent SQLite fallback), the explicit `--engine livegraph --kind
        /// file-import` and `--kind module-import` routes, and the SQLite answer the `auto` route reaches
        /// after a swap — derives `ts_type_only_caveat` from the SAME stored per-language file facts under the
        /// SAME ≥10% materiality gate (`snapshot_has_material_ts_js` → `reader_context::repo_has_material_ts_js`).
        ///
        /// The faithful fixture is 100% TypeScript (three `.ts` files) WITH a real `src`↔`lib` cycle at BOTH
        /// FILE and MODULE granularity, so the caveat's TRUE path (material TS AND a rendered cycle) is
        /// observable on the LiveGraph fastpath — the path the hermetic dispatcher integration test
        /// (`tests/cycle_honesty_route_consistency.rs`) CANNOT reach, because it never preloads a LiveGraph so
        /// its `auto` always falls back to SQLite. `backend_used` is asserted on the LiveGraph routes so a
        /// route that silently served SQLite (which would make an "auto == sqlite" caveat comparison vacuous —
        /// exactly the review-3 gap) is caught here.
        #[test]
        fn cycles_caveat_route_consistent_on_resident_livegraph() {
            let f = test_fixture::build_fixture(false);
            let mut never = || std::ops::ControlFlow::Continue(());

            // DEFAULT (`auto`) route: a GREEN cycles cert at the resident fingerprint licenses the LiveGraph
            // fastpath, so this route genuinely SERVES the LiveGraph (asserted below), not a SQLite fallback.
            let fingerprint = cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                .expect("eligibility never errors here");
            assert!(
                fingerprint.is_some(),
                "faithful TS fixture -> GREEN cycles cert -> the auto route is fastpath-eligible"
            );
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            let auto =
                cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                    .unwrap();
            assert_eq!(
                auto["backend_used"], "livegraph",
                "GREEN cert -> the DEFAULT route SERVES the LiveGraph fastpath (not a silent SQLite fallback)"
            );
            assert!(
                auto["count"].as_u64().unwrap() >= 1,
                "the fixture carries a real src<->lib module cycle -> the fastpath renders one"
            );
            let auto_caveat = auto["ts_type_only_caveat"].as_bool().unwrap();
            assert!(
                auto_caveat,
                "material TS repo WITH a rendered cycle -> caveat TRUE on the LiveGraph fastpath \
                 (the TRUE path the hermetic integration test cannot reach)"
            );

            // Explicit LiveGraph routes: both read the SAME stored language facts for the caveat. The fixture
            // has the cycle at BOTH granularities, so each renders >=1 cycle and its caveat is likewise TRUE.
            let file_import = file_import_cycles_response(
                &f.state,
                test_fixture::REPO,
                "disp",
                &f.snapshot_uid,
                &mut never,
            )
            .unwrap();
            assert_eq!(file_import["backend_used"], "livegraph");
            assert!(file_import["count"].as_u64().unwrap() >= 1);
            let module_import = module_import_cycles_response(
                &f.state,
                test_fixture::REPO,
                "disp",
                &f.snapshot_uid,
                &mut never,
            )
            .unwrap();
            assert_eq!(module_import["backend_used"], "livegraph");
            assert!(module_import["count"].as_u64().unwrap() >= 1);

            // SQLite answer via the `auto` route after a mid-request swap moves the resident fingerprint: the
            // captured epoch no longer matches -> fail soft to the canonical SQLite answer at the pin.
            swap_livegraph(&f.state);
            let sqlite =
                cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                    .unwrap();
            assert_eq!(sqlite["backend_used"], "sqlite");
            assert!(sqlite["count"].as_u64().unwrap() >= 1);

            // The invariant: with a rendered cycle on every route the caveat is identically TRUE here. The
            // LiveGraph routes derive it from the repo-level ≥10% gate; the SQLite fallback derives it from
            // cycle MEMBERSHIP (ZEROSTATE-SCOPE-1 §2.3) — on this 100%-TypeScript fixture the two bases
            // coincide (every cycle member IS TypeScript, and the repo is materially TypeScript), so the
            // values still agree. A route reading a DIFFERENT basis (the review-2 `contributing_languages`
            // divergence) would break this equality.
            for (label, v) in [
                ("file-import", &file_import),
                ("module-import", &module_import),
                ("sqlite-fallback", &sqlite),
            ] {
                assert_eq!(
                    v["ts_type_only_caveat"].as_bool().unwrap(),
                    auto_caveat,
                    "the {label} route must derive the SAME caveat as the LiveGraph fastpath (same stored basis)"
                );
            }
        }

        /// FIXTURE-POLLUTION-1 §2.3 (review-3 finding 1): every LiveGraph cycle serving path
        /// carries the honest `test_composition_note` (it lacks the stored `is_test` fact, so it
        /// CANNOT classify test-only cycles), and the SQLite route — which DOES classify per cycle
        /// — does NOT (its per-cycle `test_composition` discriminant is the disclosure there). The
        /// file-import note deliberately omits the `--engine sqlite` hint: that route serves MODULE
        /// cycles, so pointing at it from the FILE route would be a false equivalence claim.
        #[test]
        fn cycles_livegraph_routes_disclose_test_composition_asymmetry() {
            let f = test_fixture::build_fixture(false);
            let mut never = || std::ops::ControlFlow::Continue(());

            // DEFAULT (`auto`) route serving the LiveGraph module-cycle fastpath.
            let fingerprint = cycles_cert_eligibility(&f.state, &f.snapshot_uid, &mut never)
                .expect("eligibility never errors here");
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);
            let auto =
                cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                    .unwrap();
            assert_eq!(auto["backend_used"], "livegraph");
            let auto_note = auto["test_composition_note"]
                .as_str()
                .expect("LiveGraph fastpath carries the §2.3 asymmetry note");
            assert!(
                auto_note.contains("not evaluated on this serving path"),
                "{auto_note}"
            );
            assert!(
                auto_note.contains("rmap cycles --engine sqlite"),
                "{auto_note}"
            );

            // Explicit `--kind file-import`: asymmetry stated, NO misleading sqlite hint.
            let file_import = file_import_cycles_response(
                &f.state,
                test_fixture::REPO,
                "disp",
                &f.snapshot_uid,
                &mut never,
            )
            .unwrap();
            let file_note = file_import["test_composition_note"]
                .as_str()
                .expect("FILE-import LiveGraph route carries the §2.3 asymmetry note");
            assert!(
                file_note.contains("not evaluated on this serving path"),
                "{file_note}"
            );
            assert!(
                !file_note.contains("--engine sqlite"),
                "FILE route must NOT claim a classified sqlite equivalent (it serves MODULE cycles): {file_note}"
            );

            // Explicit `--kind module-import`: asymmetry stated, sqlite hint accurate.
            let module_import = module_import_cycles_response(
                &f.state,
                test_fixture::REPO,
                "disp",
                &f.snapshot_uid,
                &mut never,
            )
            .unwrap();
            let module_note = module_import["test_composition_note"]
                .as_str()
                .expect("MODULE-import LiveGraph route carries the §2.3 asymmetry note");
            assert!(
                module_note.contains("not evaluated on this serving path"),
                "{module_note}"
            );
            assert!(
                module_note.contains("rmap cycles --engine sqlite"),
                "{module_note}"
            );

            // SQLite route (auto after a swap moves the resident fingerprint): it classifies per
            // cycle, so the top-level asymmetry note is ABSENT — never a false "not evaluated".
            swap_livegraph(&f.state);
            let sqlite =
                cycles_auto_response(&f.state, test_fixture::REPO, "disp", &epoch, &mut never)
                    .unwrap();
            assert_eq!(sqlite["backend_used"], "sqlite");
            assert!(
                sqlite["test_composition_note"].is_null(),
                "the classifying SQLite route carries no asymmetry note: {}",
                sqlite["test_composition_note"]
            );
        }

        // ── W-B-EPOCH-IMPL-2B: `stats` build-then-peek eligibility + EV-A ─────────────────────────

        /// The stats-cert eligibility witness is BUILD-THEN-PEEK: `Some` ONLY when a GREEN stats cert exists
        /// at EXACTLY the resident fingerprint, and the returned fingerprint IS that exact resident state;
        /// after a swap it tracks the NEW resident state, never the stale captured fingerprint. A pre-stored
        /// GREEN cert decouples this build-then-peek assertion from the SQLite/LiveGraph stats parity (which
        /// STATS-LIVEGRAPH-IMPL-1 tests separately) — the WARM reuses the valid cert (no worker).
        #[test]
        fn stats_cert_eligibility_is_the_exact_resident_fingerprint_on_green() {
            let f = test_fixture::build_fixture(false);
            let resident_fp = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            *f.state.stats_cert.write() = Some(StatsNoLossCert {
                verdict: "GREEN".to_string(),
                fingerprint: resident_fp.clone(),
            });
            let fp = match stats_cert_eligibility(&mut NoEmit, &f.state, &f.snapshot_uid) {
                StatsEligibility::Witness(fp) => fp,
                StatsEligibility::Cancelled => panic!("no disconnect -> never Cancelled"),
            }
            .expect("GREEN stats cert at the resident fingerprint -> Some eligibility");
            assert_eq!(
                fp, resident_fp,
                "the witness IS the exact resident-and-validated fingerprint"
            );

            // Honesty under a swap (§6.4): the WARM rebuilds against the NEW resident state and the PEEK
            // returns the new fp (or None), NEVER the stale captured fp.
            swap_livegraph(&f.state);
            let after = match stats_cert_eligibility(&mut NoEmit, &f.state, &f.snapshot_uid) {
                StatsEligibility::Witness(fp) => fp,
                StatsEligibility::Cancelled => panic!("no disconnect -> never Cancelled"),
            };
            assert_ne!(
                after.as_deref(),
                Some(resident_fp.as_str()),
                "after a swap the witness is never the stale pre-swap fingerprint"
            );

            // No resident LiveGraph -> Witness(None) (eager SQLite).
            *f.state.livegraph.write() = None;
            assert!(matches!(
                stats_cert_eligibility(&mut NoEmit, &f.state, &f.snapshot_uid),
                StatsEligibility::Witness(None)
            ));
        }

        /// EV-A at the `stats` auto serve (TOCTOU closed): on a matching epoch the GREEN-cert FASTPATH serves
        /// the LiveGraph module stats (`backend_used=livegraph`); a mid-request swap moves the resident
        /// fingerprint so the captured epoch no longer matches -> the serve fails soft to the SQLite answer AT
        /// THE PINNED snapshot (`backend_used=sqlite`, stamped with the pinned `snapshot_uid`).
        #[test]
        fn ev_a_stats_serves_livegraph_on_green_then_pinned_sqlite_after_swap() {
            let f = test_fixture::build_fixture(false);
            // Pre-store a GREEN stats cert at the resident fingerprint -> the eligibility witness is Some (no
            // worker; decouples the EV-A gate from SQLite/LiveGraph stats parity, tested separately).
            let resident_fp = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(&guard.as_ref().unwrap().live_partitions(), &f.snapshot_uid)
            };
            *f.state.stats_cert.write() = Some(StatsNoLossCert {
                verdict: "GREEN".to_string(),
                fingerprint: resident_fp.clone(),
            });
            let fingerprint = match stats_cert_eligibility(&mut NoEmit, &f.state, &f.snapshot_uid) {
                StatsEligibility::Witness(fp) => fp,
                StatsEligibility::Cancelled => panic!("no disconnect -> never Cancelled"),
            };
            assert!(fingerprint.is_some(), "GREEN stats cert -> eligible");
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);

            // Steady state (no swap): the epoch matches the resident fingerprint -> serve the LiveGraph stats.
            let v = match stats_auto_response(
                &mut NoEmit,
                &f.state,
                test_fixture::REPO,
                "disp",
                &epoch,
            ) {
                Ok(StatsOutcome::Ready(v)) => v,
                Ok(StatsOutcome::Cancelled) => panic!("no disconnect -> never Cancelled"),
                Err(e) => panic!("stats serve errored: {e}"),
            };
            assert_eq!(v["backend_used"], "livegraph");

            // EV-A: a mid-request swap moves the resident fingerprint; the captured epoch no longer matches ->
            // fail soft to the SQLite answer AT THE PINNED snapshot (the SQLite worker completes with NoEmit).
            swap_livegraph(&f.state);
            let v2 = match stats_auto_response(
                &mut NoEmit,
                &f.state,
                test_fixture::REPO,
                "disp",
                &epoch,
            ) {
                Ok(StatsOutcome::Ready(v)) => v,
                Ok(StatsOutcome::Cancelled) => panic!("no disconnect -> never Cancelled"),
                Err(e) => panic!("stats serve errored: {e}"),
            };
            assert_eq!(v2["backend_used"], "sqlite");
            assert_eq!(
                v2["snapshot_uid"], f.snapshot_uid,
                "the SQLite fallback is stamped with the PINNED snapshot"
            );
        }

        // ── W-B-EPOCH-IMPL-3: the §6 whole-request join coherence + read-during-refresh ──────────

        /// THE §6 whole-request join-coherence proof — the IMPL-3 acceptance (closes the cross-store
        /// split-brain the B1 review withdrew W-B over). A reader captures epoch N; a concurrent writer
        /// then PUBLISHES a REAL new SQLite READY snapshot N+1 (create + flip-ready, the prior snapshot
        /// retained — mirroring a real refresh) AND swaps the in-memory LiveGraph. Both
        /// independently-versioned stores move. Assert:
        ///   (a) the in-flight reader's WHOLE request stays coherent at N — its pinned `snapshot_uid` is
        ///       unchanged, the swapped LiveGraph no longer matches its captured fingerprint so the leaf
        ///       fails soft (EV-A) to SQLite@N, and the served caller is N's (never N+1, never the
        ///       swapped graph); AND
        ///   (b) a NEW request issued AFTER the publish sees N+1 (`get_latest_snapshot` returns N+1 and a
        ///       fresh epoch capture pins N+1).
        /// The EV-A sibling tests above only swap the LiveGraph and re-check the same pinned uid; this
        /// test actually publishes a new SQLite READY snapshot N+1 mid-request, proving CROSS-STORE
        /// N→N+1 coherence, not just a LiveGraph re-validation (the packet's explicit requirement).
        #[test]
        fn whole_request_join_coherence_across_real_sqlite_n_plus_1_publish() {
            let f = test_fixture::build_fixture(false);
            let storage = f.state.storage().unwrap();
            let snapshot_uid_n = f.snapshot_uid.clone();

            // The in-flight reader captures epoch N (pinned snapshot N + GREEN callgraph eligibility).
            let fingerprint = callgraph_cert_eligibility(&f.state, &snapshot_uid_n);
            assert!(fingerprint.is_some(), "GREEN fixture -> a witness at N");
            let epoch_n = capture(&f.state, &snapshot_uid_n, fingerprint);

            // ── A concurrent writer publishes epoch N+1 across BOTH stores ──────────────────────────
            // SQLite: push N's created_at into the past so `get_latest_snapshot` (ORDER BY created_at
            // DESC) deterministically returns N+1, then create + flip-ready a REAL new snapshot. The
            // prior READY snapshot N is retained (a real refresh prunes it only later), so BOTH rows
            // exist and the pinned reader can still read N by uid.
            storage
                .execute_raw(&format!(
                    "UPDATE snapshots SET created_at = '2000-01-01T00:00:00.000Z' \
                     WHERE snapshot_uid = '{snapshot_uid_n}'"
                ))
                .unwrap();
            let snap_n1 = storage
                .create_snapshot(&repo_graph_storage::types::CreateSnapshotInput {
                    repo_uid: test_fixture::REPO.to_string(),
                    kind: "full".to_string(),
                    basis_ref: None,
                    basis_commit: None,
                    parent_snapshot_uid: Some(snapshot_uid_n.clone()),
                    label: None,
                    toolchain_json: None,
                })
                .unwrap();
            storage
                .update_snapshot_status(&repo_graph_storage::types::UpdateSnapshotStatusInput {
                    snapshot_uid: snap_n1.snapshot_uid.clone(),
                    status: "ready".to_string(),
                    completed_at: None,
                })
                .unwrap();
            // LiveGraph: the refresh swaps the in-memory graph (epoch bumped in place).
            swap_livegraph(&f.state);

            // ── (a) the in-flight reader's whole request is STILL coherent at N ─────────────────────
            assert_eq!(
                epoch_n.snapshot_uid(),
                snapshot_uid_n,
                "the pinned SQLite identity never moves, even though N+1 was published"
            );
            // The swap moved the resident fingerprint off the captured witness -> EV-A must fail soft.
            let resident_fp_now = {
                let guard = f.state.livegraph.read();
                import_cert_fingerprint(
                    &guard.as_ref().unwrap().live_partitions(),
                    epoch_n.snapshot_uid(),
                )
            };
            assert_ne!(
                Some(&resident_fp_now),
                epoch_n.fingerprint.as_ref(),
                "the N+1 LiveGraph swap moved the resident fingerprint off the captured epoch"
            );
            let target = resolved(&test_fixture::callee_key(), "calleeFn");
            let edge = ["CALLS"];
            let in_flight = callers_engine_response(
                Engine::Auto,
                &f.state,
                &epoch_n,
                &target,
                || storage.find_direct_callers(epoch_n.snapshot_uid(), &target.stable_key, &edge),
                "calleeFn",
                "",
            )
            .unwrap();
            assert_eq!(
                in_flight["backend_used"], "sqlite",
                "EV-A: a swapped LiveGraph fails soft to the PINNED SQLite snapshot (N)"
            );
            assert_eq!(
                in_flight["callers"][0]["stable_key"],
                test_fixture::caller_key(),
                "the in-flight reader serves N's caller, never N+1 nor the swapped graph"
            );

            // ── (b) a NEW request issued AFTER the publish sees N+1 ─────────────────────────────────
            let latest_now = AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
                .unwrap()
                .unwrap();
            assert_eq!(
                latest_now.snapshot_uid, snap_n1.snapshot_uid,
                "a new request resolves 'latest' to the freshly-published N+1"
            );
            assert_ne!(
                latest_now.snapshot_uid, snapshot_uid_n,
                "the new request is NOT pinned to the in-flight reader's N"
            );
            let new_epoch = RequestEpoch {
                snapshot: latest_now,
                fingerprint: callgraph_cert_eligibility(&f.state, &snap_n1.snapshot_uid),
            };
            assert_eq!(
                new_epoch.snapshot_uid(),
                snap_n1.snapshot_uid,
                "a fresh capture pins N+1 (the new request is coherent at N+1, not N)"
            );
        }

        /// Read-during-refresh at the daemon (`RepoState`) level: with a refresh guard HELD on the repo's
        /// coordinator, a reader is ADMITTED (single thread — under W-A `acquire_read` returned `Blocked`
        /// and this would DEADLOCK) AND serves a coherent last-good answer against its captured epoch. The
        /// VISION's "orientation in milliseconds even during a background refresh". Previously this would
        /// block for the whole refresh.
        #[test]
        fn reader_admitted_and_coherent_during_refresh() {
            let f = test_fixture::build_fixture(false);
            let storage = f.state.storage().unwrap();
            let fingerprint = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
            let epoch = capture(&f.state, &f.snapshot_uid, fingerprint);

            // A refresh is in flight on THIS repo's coordinator.
            let _refresh = f.state.coordinator.acquire_refresh();
            // W-B: the reader is admitted alongside it — this returning at all on one thread IS the proof
            // (under W-A it blocked). The coordinator is now "a refresh + 1 reader": write-active AND one
            // admitted reader (== RefreshingWithReaders(1)).
            let _read = f.state.coordinator.acquire_read();
            let coord_state = f.state.coordinator.state();
            assert!(
                coord_state.is_write_active(),
                "the refresh is still in flight while the reader is admitted"
            );
            assert_eq!(
                coord_state.reader_count(),
                1,
                "exactly one reader admitted during the refresh (RefreshingWithReaders(1))"
            );

            // ...and it serves a coherent last-good answer at the captured epoch (LiveGraph fastpath; the
            // refresh has not swapped the graph yet).
            let target = resolved(&test_fixture::callee_key(), "calleeFn");
            let edge = ["CALLS"];
            let v = callers_engine_response(
                Engine::Auto,
                &f.state,
                &epoch,
                &target,
                || storage.find_direct_callers(epoch.snapshot_uid(), &target.stable_key, &edge),
                "calleeFn",
                "",
            )
            .unwrap();
            assert_eq!(v["backend_used"], "livegraph");
            assert_eq!(v["callers"][0]["stable_key"], test_fixture::caller_key());
        }
    }

    // ── CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 + W-B-EPOCH-IMPL-2B: the PURE fastpath/SQLite ladder ──
    // The ladder takes `epoch_eligible: bool` (the EV-A gate) + closures, so a PANICKING serve_sqlite proves
    // the eligible path serves LiveGraph WITHOUT touching SQLite. The cert (re)build moved OUT of the ladder
    // into `cycles_cert_eligibility` (build-then-peek), so the ladder no longer takes a build closure; the
    // build-path + build-cancellation cases now live in the eligibility tests (`wb_epoch_coherence`).

    #[test]
    fn cycles_fastpath_eligible_serves_livegraph_without_sqlite() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            true, // epoch_eligible
            || json!({"backend_used": "livegraph"}),
            |_r, _c| panic!("SQLite (find_cycles) must NOT be read when the epoch is eligible"),
            &mut || std::ops::ControlFlow::Continue(()),
        )
        .expect("eligible path serves LiveGraph and never errors");
        assert_eq!(out["backend_used"], "livegraph");
    }

    #[test]
    fn cycles_fastpath_not_eligible_falls_back_to_sqlite() {
        let out = cycles_fastpath_or_sqlite(
            true,
            FallbackReason::LiveGraphUnavailable,
            false, // epoch_eligible: a fingerprint mismatch / no GREEN cert -> EV-A fail-soft
            || panic!("must NOT serve LiveGraph when the epoch is not eligible"),
            |r, _c| Ok(json!({"backend_used": "sqlite", "reason": r.as_str()})),
            &mut || std::ops::ControlFlow::Continue(()),
        )
        .expect("not-eligible serves SQLite and never errors");
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphCycleDivergence");
    }

    #[test]
    fn cycles_fastpath_precondition_unmet_falls_back_with_reason() {
        let out = cycles_fastpath_or_sqlite(
            false,
            FallbackReason::LiveGraphPartial,
            true, // epoch_eligible (ignored when the precondition is unmet)
            || panic!("must NOT serve LiveGraph when the precondition is unmet"),
            |r, _c| Ok(json!({"backend_used": "sqlite", "reason": r.as_str()})),
            &mut || std::ops::ControlFlow::Continue(()),
        )
        .expect("precondition-unmet serves SQLite and never errors");
        assert_eq!(out["backend_used"], "sqlite");
        assert_eq!(out["reason"], "LiveGraphPartial");
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
            true, // repo_has_ts_js: caller-computed material-TS/JS signal (count>0 -> caveat true)
        );
        assert_eq!(out["backend_used"], "livegraph");
        assert!(out["fallback_reason"].is_null());
        assert_eq!(out["count"], 1);
        // CYCLE-HONESTY-1: the fastpath carries NO edges (LiveGraph route omits the field), so the renderer
        // renders `members (unordered)`; the repo-level TS caveat rides on the response.
        assert!(out["cycles"][0].get("edges").is_none());
        assert_eq!(out["ts_type_only_caveat"], true);
        let names: Vec<&str> = out["cycles"][0]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["qualified_name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a/y", "b/x"]);
    }

    // ── STATS-LIVEGRAPH-IMPL-1 + W-B-EPOCH-IMPL-2B: the stats fastpath/SQLite ladder ──
    // The ladder takes `epoch_eligible: bool` (the EV-A gate) + closures, so a PANICKING serve_sqlite proves
    // the eligible path serves LiveGraph WITHOUT touching SQLite. The cert (re)build moved OUT of the ladder
    // into `stats_cert_eligibility` (build-then-peek), so the ladder no longer takes a build closure; the
    // build-path + build-cancellation cases now live in the eligibility tests (`wb_epoch_coherence`). The
    // serve_sqlite closure still takes an emitter and may report a mid-aggregation disconnect
    // (`StatsOutcome::Cancelled`).

    use repo_graph_daemon_transport::{EmitError, ProgressDetail};

    /// A no-op emitter for the pure-ladder tests. The ladder forwards it to the SQL closures; the
    /// stub/panic closures here decide the outcome, so no real transport write happens.
    struct NoEmit;
    impl ProgressEmitter for NoEmit {
        fn emit(&mut self, _d: ProgressDetail) -> Result<(), EmitError> {
            Ok(())
        }
    }

    /// Extract the served body from a ladder outcome, or fail the test on Cancelled/Err.
    #[track_caller]
    fn ready_value(out: Result<StatsOutcome, StorageError>) -> Value {
        match out {
            Ok(StatsOutcome::Ready(v)) => v,
            Ok(StatsOutcome::Cancelled) => panic!("expected Ready, got Cancelled"),
            Err(e) => panic!("expected Ready, got Err: {e}"),
        }
    }

    #[test]
    fn stats_fastpath_eligible_serves_livegraph_without_sqlite() {
        let out = stats_fastpath_or_sqlite(
            &mut NoEmit,
            true,
            FallbackReason::LiveGraphUnavailable,
            true, // epoch_eligible
            || json!({"backend_used": "livegraph"}),
            |_e, _r| {
                panic!("SQLite (compute_module_stats) must NOT be read when the epoch is eligible")
            },
        );
        assert_eq!(ready_value(out)["backend_used"], "livegraph");
    }

    #[test]
    fn stats_fastpath_not_eligible_falls_back_to_sqlite() {
        let out = stats_fastpath_or_sqlite(
            &mut NoEmit,
            true,
            FallbackReason::LiveGraphUnavailable,
            false, // epoch_eligible: a fingerprint mismatch / no GREEN cert -> EV-A fail-soft
            || panic!("must NOT serve LiveGraph when the epoch is not eligible"),
            |_e, r| {
                Ok(StatsOutcome::Ready(
                    json!({"backend_used": "sqlite", "reason": r.as_str()}),
                ))
            },
        );
        let v = ready_value(out);
        assert_eq!(v["backend_used"], "sqlite");
        assert_eq!(v["reason"], "LiveGraphStatsDivergence");
    }

    #[test]
    fn stats_fastpath_precondition_unmet_falls_back_with_reason() {
        let out = stats_fastpath_or_sqlite(
            &mut NoEmit,
            false,
            FallbackReason::LiveGraphPartial,
            true, // epoch_eligible (ignored when the precondition is unmet)
            || panic!("must NOT serve LiveGraph when the precondition is unmet"),
            |_e, r| {
                Ok(StatsOutcome::Ready(
                    json!({"backend_used": "sqlite", "reason": r.as_str()}),
                ))
            },
        );
        let v = ready_value(out);
        assert_eq!(v["backend_used"], "sqlite");
        assert_eq!(v["reason"], "LiveGraphPartial");
    }

    // DAEMON-CANCEL-2: a peer-disconnect DURING the SQLite fallback (`serve_sqlite` returns Cancelled)
    // must propagate as StatsOutcome::Cancelled. (The cert-build cancellation moved to
    // `stats_cert_eligibility` — covered by the eligibility tests in `wb_epoch_coherence`.)
    #[test]
    fn stats_fastpath_sqlite_cancelled_propagates_cancelled() {
        let out = stats_fastpath_or_sqlite(
            &mut NoEmit,
            false, // precondition unmet ⇒ straight to serve_sqlite
            FallbackReason::LiveGraphPartial,
            true, // epoch_eligible (ignored when the precondition is unmet)
            || panic!("must NOT serve LiveGraph when the precondition is unmet"),
            |_e, _r| Ok(StatsOutcome::Cancelled),
        );
        assert!(matches!(out, Ok(StatsOutcome::Cancelled)));
    }

    #[test]
    fn serve_stats_fastpath_shape_is_sqlite_compatible_plus_backend_metadata() {
        let rows = vec![ModuleStatsResult {
            module: "src/a".to_string(),
            fan_in: 0,
            fan_out: 1,
            instability: Some(1.0),
            abstractness: 0.5,
            distance_from_main_sequence: Some(0.5),
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
    fn degenerate_module_stats_serialize_metrics_as_json_null_not_zero() {
        // HONEST-DEGRADATION-IMPL-1 (D1 ratified rider, honest-degradation-1.md §12): the
        // degenerate-`unknown` MUST reach JSON consumers as `null` (architecture Rule #6 at the DTO
        // layer), never a bare `0` that reads as a known value. A zero-degree module produced by the
        // shared helper carries `None` instability + distance; serialized, those are JSON `null`.
        //
        // CONTRACT BOUNDARY (the reviewer's degenerate-DTO question, resolved per the ratified text):
        // the `unknown` conversion is scoped to the metrics that are mathematically UNDEFINED at zero
        // degree — `instability` (the `0/0` the packet names as "the degenerate value the guard
        // converts") and the `distance` derived from it. The degree counts `fan_in`/`fan_out` are NOT
        // nulled: each is a genuine count of zero RESOLVED import edges (Rule #6 "empty = known-zero" —
        // a TRUE number, not the fabricated `0.0` the old `else` branch emitted for `0/0`), made honest
        // by the dependency-section caveat the renderer attaches when import-graph reliability != HIGH.
        // So below: I/D serialize as `null` AND fan_in/fan_out stay 0; abstractness stays a number.
        let rows = livegraph_module_stats_dto(&[ModuleStatRow {
            module: "src/core".to_string(),
            fan_in: 0,
            fan_out: 0,
            file_count: 12,
            symbol_count: 340,
            abstract_count: 3,
            type_count: 3,
        }]);
        assert_eq!(rows[0].instability, None);
        assert_eq!(rows[0].distance_from_main_sequence, None);
        let out = serve_stats_fastpath("repo", "disp", "snap", &rows);
        assert!(
            out["stats"][0]["instability"].is_null(),
            "degenerate instability must serialize as JSON null, got {}",
            out["stats"][0]["instability"]
        );
        assert!(
            out["stats"][0]["distance_from_main_sequence"].is_null(),
            "degenerate distance must serialize as JSON null, got {}",
            out["stats"][0]["distance_from_main_sequence"]
        );
        // The genuine zero-degree counts (Rule #6 "empty = known-zero") stay 0 — NOT nulled — for BOTH
        // fan_in and fan_out; the import-graph-independent abstractness stays a concrete number.
        assert_eq!(out["stats"][0]["fan_in"], 0);
        assert_eq!(out["stats"][0]["fan_out"], 0);
        assert_eq!(out["stats"][0]["abstractness"], 1.0);
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
        assert_eq!(dto[0].instability, Some(0.75));
        assert_eq!(dto[0].abstractness, 0.25);
        assert_eq!(dto[0].distance_from_main_sequence, Some(0.0));
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
        let panic_sqlite =
            || -> Vec<ImportResult> { panic!("SQLite read on the eligible fastpath") };
        let empty_sqlite = Vec::<ImportResult>::new;
        // W-B-EPOCH-IMPL-2A (EV-A): epoch_eligible + precondition met -> FASTPATH; the panicking find_imports
        // is NEVER called (proves zero SQLite read on the cert-proven serve).
        let g = imports_fastpath_or_compare("a.ts", &view, Some(&met), true, panic_sqlite);
        assert_eq!(g["backend_used"], "livegraph");
        assert_eq!(g["comparison"]["source"], "repo_no_loss_certificate");
        assert_eq!(g["count"], 1);
        // NOT eligible (no green cert at capture, OR a swap moved the resident fingerprint -> the EV-A
        // fail-soft) -> COMPARE-ON-CALL: find_imports runs, a per-call comparison (NOT the cert source).
        let r = imports_fastpath_or_compare("a.ts", &view, Some(&met), false, empty_sqlite);
        assert!(r["comparison"]["sqlite_resolved_local"].is_number());
        assert!(r["comparison"]["source"].is_null());
        // NON-TS (precondition unmet) -> SQLite compare-on-call regardless of eligibility.
        let nt = imports_fastpath_or_compare("a.cpp", &view, None, true, empty_sqlite);
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

    // W-B-EPOCH-IMPL-2A (§14 D-CC refined): the former `path_auto_outcome` unit tests
    // (`path_auto_serves_livegraph_when_*` / `path_auto_falls_back_on_*`) are REMOVED with the function —
    // `path`'s default no longer serves the LiveGraph fastpath. The replacement coherence proof (Engine::Auto
    // serves the PINNED SQLite snapshot even when the LiveGraph has a path; explicit `--engine livegraph` still
    // serves the LG) lives in `wb_epoch_coherence::path_auto_serves_pinned_sqlite_not_livegraph`.
}
