//! EXPLAIN-LIVEGRAPH-IMPL: daemon-half LG-SERVED end-to-end tests for `build_explain_envelope`.
//!
//! Split out of `explain_coherence.rs` to respect the >500-line structural guardrail. Proves explain's
//! callers/callees leaves are multi-source `{livegraph, sqlite}` when the LiveGraph serves, and a labelled
//! `{sqlite}` fallback when no LiveGraph is loaded (mirrors `orient_lg_decisions::orient_lg_served_e2e`).

use super::build_explain_envelope;
use crate::state::RepoState;
use repo_graph_agent::{
    Confidence, ExplainCalleeItem, ExplainCalleesEvidence, ExplainCallerItem,
    ExplainCallersEvidence, ExplainIdentityEvidence, Focus, OrientResult, Signal, SignalCode,
    EXPLAIN_COMMAND, ORIENT_SCHEMA,
};
use repo_graph_coherence::{CoherenceFallbackReason, Source};
use repo_graph_ir::EdgeType;
use repo_graph_livegraph::LiveGraph;
use repo_graph_livegraph_feed::feed_partition;
use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
use repo_graph_storage::types::{
    CreateSnapshotInput, GraphEdge, GraphNode, Repo, UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::{Granularity, LanguageSupport};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const REPO: &str = "repo_explain_e2e";

/// Ingest the committed synthetic SCIP fixture (producer-free; the SAME fixture orient's e2e uses).
fn synthetic_outcome() -> IngestOutcome {
    let root = format!(
        "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
        env!("CARGO_MANIFEST_DIR")
    );
    let scip = std::fs::read(format!("{root}/index.scip")).expect("read committed index.scip");
    let index = decode_index(&scip).expect("decode scip");
    ingest_partition(
        &index,
        &root,
        "synthetic",
        "synthetic",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    )
}

/// The endpoints of the first real `Calls` edge: `(caller_key, callee_key)`.
fn find_calls_edge(outcome: &IngestOutcome) -> (String, String) {
    outcome
        .ir
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::Calls)
        .map(|e| (e.src.as_str().to_string(), e.dst.as_str().to_string()))
        .expect("at least one Calls edge in the synthetic fixture")
}

/// Build a SQLite db carrying ONLY the call-graph the no-loss gate reads (SYMBOL nodes + `CALLS` edges).
fn build_db_with_calls(
    dir: &Path,
    repo_uid: &str,
    calls: &[(String, String)],
) -> (PathBuf, String) {
    let db_path = dir.join("repo.db");
    let mut conn = StorageConnection::open(&db_path).expect("open storage");
    conn.add_repo(&Repo {
        repo_uid: repo_uid.to_string(),
        name: repo_uid.to_string(),
        root_path: ".".to_string(),
        default_branch: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        metadata_json: None,
    })
    .expect("add repo");
    let snap = conn
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: repo_uid.to_string(),
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .expect("create snapshot");
    let snapshot_uid = snap.snapshot_uid;

    let mut uid_of: HashMap<String, String> = HashMap::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (a, b) in calls {
        for k in [a, b] {
            if !uid_of.contains_key(k) {
                let uid = format!("n{}", uid_of.len());
                uid_of.insert(k.clone(), uid.clone());
                nodes.push(GraphNode {
                    node_uid: uid,
                    snapshot_uid: snapshot_uid.clone(),
                    repo_uid: repo_uid.to_string(),
                    stable_key: k.clone(),
                    kind: "SYMBOL".to_string(),
                    subtype: Some("FUNCTION".to_string()),
                    name: k.clone(),
                    qualified_name: None,
                    file_uid: None,
                    parent_node_uid: None,
                    location: None,
                    signature: None,
                    visibility: None,
                    doc_comment: None,
                    metadata_json: None,
                });
            }
        }
    }
    if !nodes.is_empty() {
        conn.insert_nodes(&nodes).expect("insert nodes");
    }

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (a, b) in calls {
        if seen.insert((a.clone(), b.clone())) {
            edges.push(GraphEdge {
                edge_uid: format!("e{}", edges.len()),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: repo_uid.to_string(),
                source_node_uid: uid_of[a].clone(),
                target_node_uid: uid_of[b].clone(),
                edge_type: "CALLS".to_string(),
                resolution: "resolved".to_string(),
                extractor: "test".to_string(),
                location: None,
                metadata_json: None,
            });
        }
    }
    if !edges.is_empty() {
        conn.insert_edges(&edges).expect("insert edges");
    }

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".to_string(),
        completed_at: None,
    })
    .expect("ready snapshot");

    (db_path, snapshot_uid)
}

struct Fixture {
    _dir: tempfile::TempDir,
    state: RepoState,
    src: String,
    dst: String,
    ks_callers: Vec<String>,
    ks_callees: Vec<String>,
    snapshot_uid: String,
}

fn setup() -> Fixture {
    let outcome = synthetic_outcome();
    let (src, dst) = find_calls_edge(&outcome);
    let mut lg = LiveGraph::new();
    feed_partition(
        &mut lg,
        "synthetic",
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );

    let callers_env = lg.callers(&dst, Granularity::CallerDetail);
    let ks_callers: Vec<String> = callers_env
        .data()
        .expect("callers data")
        .caller_identities
        .iter()
        .map(|(_, k)| k.clone())
        .collect();
    let callees_env = lg.callees(&src, Granularity::CallerDetail);
    let ks_callees: Vec<String> = callees_env
        .data()
        .expect("callees data")
        .callee_identities
        .iter()
        .map(|(k, _)| k.clone())
        .collect();

    // SQLite mirrors BOTH key sets so the per-symbol no-loss key compare is GENUINELY GREEN.
    let mut calls: Vec<(String, String)> = ks_callers
        .iter()
        .map(|k| (k.clone(), dst.clone()))
        .collect();
    for k in &ks_callees {
        calls.push((src.clone(), k.clone()));
    }
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &calls);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(lg);

    Fixture {
        _dir: dir,
        state,
        src,
        dst,
        ks_callers,
        ks_callees,
        snapshot_uid,
    }
}

fn explain_symbol_result(
    snapshot_uid: &str,
    symbol_key: &str,
    signals: Vec<Signal>,
) -> OrientResult {
    OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: REPO.to_string(),
        display_name: Some(REPO.to_string()),
        snapshot: snapshot_uid.to_string(),
        focus: Focus::symbol(symbol_key, symbol_key, None),
        confidence: Confidence::High,
        documentation: None,
        signals,
        signals_truncated: None,
        signals_omitted_count: None,
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: false,
    }
}

#[test]
fn explain_callers_falls_back_to_sqlite_without_livegraph() {
    // No populated LiveGraph -> the proven SQLite primary, labelled LiveGraphUnavailable. NEVER a
    // `livegraph` claim, and the envelope gains the PRODUCER_UNAVAILABLE machine-degradation limit.
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // `state.livegraph` is None (never preloaded).
    let result = explain_symbol_result(
        &snapshot_uid,
        "r:src/a.ts:Foo.bar:SYMBOL",
        vec![Signal::explain_callers(ExplainCallersEvidence {
            count: 0,
            top_modules: Vec::new(),
            items: Vec::new(),
            items_truncated: None,
            items_omitted_count: None,
        })],
    );
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainCallers)
        .expect("callers leaf present");
    assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!leaf.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        leaf.provenance.fallback_reason,
        Some(CoherenceFallbackReason::LiveGraphUnavailable)
    );
    assert!(
        env.value
            .limits
            .iter()
            .any(|l| l.code == repo_graph_agent::LimitCode::ProducerUnavailable),
        "envelope gains PRODUCER_UNAVAILABLE when an LG-first leaf has no LiveGraph"
    );
}

/// EXPLAIN_IDENTITY at SYMBOL focus WITHOUT a LiveGraph is a COMMITTED LG-first attempt that could not serve
/// the live anchor → a LABELLED `{sqlite}` fallback (`LiveGraphUnavailable`), NOT an unlabelled collapse
/// (review-7: a failed attempt is never silently unlabelled — that would hide a real degradation as the
/// proven primary). The unlabelled `{sqlite}` identity is the file/path-focus listings case only (no symbol
/// anchor → no attempt), covered by the agent `identity_without_decision_collapses_to_sqlite` test. The
/// served `{livegraph, sqlite}` anchor case is `explain_identity_serves_live_anchor_from_livegraph` below.
#[test]
fn explain_identity_symbol_focus_without_livegraph_is_labelled_sqlite_fallback() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // `state.livegraph` is None (never preloaded) -> the symbol-focus identity attempt cannot reach a live
    // anchor -> serve_identity returns the labelled LiveGraphUnavailable fallback.
    let result = explain_symbol_result(
        &snapshot_uid,
        "r:src/a.ts:Foo.bar:SYMBOL",
        vec![Signal::explain_identity(ExplainIdentityEvidence {
            target_kind: "symbol".to_string(),
            path: Some("src/a.ts".to_string()),
            stable_key: Some("r:src/a.ts:Foo.bar:SYMBOL".to_string()),
            name: Some("bar".to_string()),
            subtype: Some("method".to_string()),
            line_start: Some(10),
            language: None,
            is_test: None,
            module_path: Some("src".to_string()),
            file_count: None,
            symbol_count: None,
        })],
    );
    let env = build_explain_envelope(&state, REPO, result, false, false);
    let id = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainIdentity)
        .expect("identity leaf present");
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Sqlite]),
        "no live anchor -> the proven SQLite identity primary is kept"
    );
    assert!(!id.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        id.provenance.fallback_reason,
        Some(CoherenceFallbackReason::LiveGraphUnavailable),
        "a failed symbol-focus LG-first attempt is a LABELLED {{sqlite}} fallback, never unlabelled"
    );
}

/// The current-state LiveGraph IR display name for `key` (the migrated surface + `node_display`).
fn live_name(state: &RepoState, key: &str) -> String {
    let guard = state.livegraph.read();
    let lg = guard.as_ref().expect("livegraph set");
    lg.node_display(&repo_graph_ir::CanonicalKey::from_existing(key))
        .map(|(n, _)| n)
        .expect("a resident key has a live IR name")
}

/// EXPLAIN_CALLERS: the served caller NAME is the current-state IR name (`node_display`), NOT the drifted
/// snapshot name — proving the value is REBUILT from the migrated `callers` surface, not a relabelled SQLite
/// result (review-4 #1/#4). The per-item module has no LiveGraph home -> stays SQLite (honest multi-source).
#[test]
fn explain_callers_serves_live_name_from_livegraph() {
    let f = setup();
    assert!(f.ks_callers.contains(&f.src), "src is a caller of dst");
    let expected = live_name(&f.state, &f.src);
    let result = explain_symbol_result(
        &f.snapshot_uid,
        &f.dst,
        vec![Signal::explain_callers(ExplainCallersEvidence {
            count: f.ks_callers.len() as u64,
            top_modules: Vec::new(),
            items: vec![ExplainCallerItem {
                stable_key: f.src.clone(),
                name: "DRIFTED_SQLITE_NAME".to_string(),
                module: Some("src".to_string()),
                file: None,
                line: None,
            }],
            items_truncated: None,
            items_omitted_count: None,
        })],
    );
    let env = build_explain_envelope(&f.state, REPO, result, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainCallers)
        .expect("callers leaf present");
    let ev = leaf
        .value
        .explain_callers_evidence()
        .expect("served callers evidence");
    let item = ev
        .items
        .iter()
        .find(|i| i.stable_key == f.src)
        .expect("the src caller item is rendered");
    assert_eq!(
        item.name, expected,
        "served caller NAME is the current-state LiveGraph IR name (LG-built, not relabelled SQLite)"
    );
    assert_ne!(item.name, "DRIFTED_SQLITE_NAME");
    assert_eq!(
        item.module.as_deref(),
        Some("src"),
        "the per-item module has no LiveGraph home -> stays SQLite"
    );
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served EXPLAIN_CALLERS leaf is multi-source {{livegraph, sqlite}}"
    );
    assert!(leaf.provenance.fallback_reason.is_none());
    // The root provenance union reaches livegraph end-to-end.
    assert!(env.provenance.source.contains(&Source::Livegraph));
    assert!(env.provenance.source.contains(&Source::Sqlite));
    // The serialized rmapd wire shape shows BOTH sources on the leaf.
    let json = serde_json::to_value(&env).unwrap();
    let leaf_json = json["value"]["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["value"]["code"] == "EXPLAIN_CALLERS")
        .unwrap();
    let sources = leaf_json["provenance"]["source"].as_array().unwrap();
    assert!(sources.iter().any(|s| s == "livegraph"));
    assert!(sources.iter().any(|s| s == "sqlite"));
}

/// EXPLAIN_CALLEES dual: the served callee NAME is the live IR name (the callee `dst` is defined in the
/// resident synthetic partition), proving the callees value is REBUILT from the migrated `callees` surface.
#[test]
fn explain_callees_serves_live_name_from_livegraph() {
    let f = setup();
    assert!(f.ks_callees.contains(&f.dst), "dst is a callee of src");
    let expected = live_name(&f.state, &f.dst);
    let result = explain_symbol_result(
        &f.snapshot_uid,
        &f.src,
        vec![Signal::explain_callees(ExplainCalleesEvidence {
            count: f.ks_callees.len() as u64,
            top_modules: Vec::new(),
            items: vec![ExplainCalleeItem {
                stable_key: f.dst.clone(),
                name: "DRIFTED_SQLITE_NAME".to_string(),
                module: Some("src".to_string()),
                file: None,
                line: None,
            }],
            items_truncated: None,
            items_omitted_count: None,
        })],
    );
    let env = build_explain_envelope(&f.state, REPO, result, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainCallees)
        .expect("callees leaf present");
    let ev = leaf
        .value
        .explain_callees_evidence()
        .expect("served callees evidence");
    let item = ev
        .items
        .iter()
        .find(|i| i.stable_key == f.dst)
        .expect("the dst callee item is rendered");
    assert_eq!(
        item.name, expected,
        "served callee NAME is the current-state LiveGraph IR name (LG-built)"
    );
    assert_ne!(item.name, "DRIFTED_SQLITE_NAME");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite])
    );
    assert!(leaf.provenance.fallback_reason.is_none());
}

// The LG-SERVED VALUE proofs for identity / imports / cycles (review-7 items 2+3) live in this sibling child
// module so neither test file exceeds the >500-line structural guardrail. It reuses this module's helpers
// (build_db_with_calls, explain_symbol_result, REPO, ...) via `use super::*` and adds a hand-built cyclic
// fixture for the deterministic import/cycle surfaces.
#[path = "explain_coherence_served_tests.rs"]
mod served_tests;
