//! ORIENT-LIVEGRAPH-IMPL: daemon-half LG-SERVED end-to-end proof (extracted from `orient_lg_decisions.rs`
//! per the structural guardrail, review-3 item 3 — production stays in the parent module).
//!
//! Builds an in-process daemon `RepoState` with a REAL LiveGraph (the committed `synthetic/index.scip`,
//! ingested producer-FREE — no scip-typescript at test time) plus a SQLite that MIRRORS the LiveGraph
//! caller/callee key sets, and proves orient's per-leaf decision resolves to `Livegraph` for ALL FOUR
//! LG-first signals — and that the assembled `CoherenceEnvelope<CoherentOrientResult>` carries the leaves
//! as `livegraph` (cycles) / `{livegraph, sqlite}` (callers/callees). `build_orient_envelope` is exercised
//! on the SERVED path (`serve_from_lg = true`) here — the bounded-cert RED fallback provenance is proven in
//! `orient_serve::tests`. `super` is `orient_lg_decisions`, so the re-exported decision functions resolve
//! exactly as when this module was inline.

use super::{
    orient_callees_outcome, orient_callers_outcome, orient_complexity_outcome,
    orient_cycles_outcome, ComplexityNoLossCert, OrientLgOutcome,
};
// `import_cert_fingerprint` + `CycleNoLossCert` stay in the shared feed module (review-7 pt2 refactor).
use crate::livegraph_feed::{import_cert_fingerprint, CycleNoLossCert};
use crate::state::RepoState;
use repo_graph_agent::{
    CalleesSummaryEvidence, CallersSummaryEvidence, Confidence, Focus, HighComplexityEvidence,
    ImportCyclesEvidence, OrientResult, Signal, SignalCode, SnapshotInfoEvidence, ORIENT_COMMAND,
    ORIENT_SCHEMA,
};
use repo_graph_coherence::Source;
use repo_graph_ir::EdgeType;
use repo_graph_livegraph::LiveGraph;
use repo_graph_livegraph_feed::feed_partition;
use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
use repo_graph_storage::types::{
    CreateSnapshotInput, FileVersion, GraphEdge, GraphNode, Repo, TrackedFile,
    UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::{
    AnswerClass, FreshnessState, Granularity, LanguageSupport, QueryCompleteness,
};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const REPO: &str = "repo_orient_e2e";

/// Ingest the committed synthetic SCIP fixture (producer-free; the SAME fixture `feed_real_index.rs`
/// uses). NOT run through scip-typescript — the `.scip` is committed.
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

/// Build a SQLite db carrying ONLY the call-graph the no-loss gate reads: a repo + a ready snapshot +
/// SYMBOL nodes + `CALLS` edges for `calls` (`(caller_key, callee_key)`). Returns `(db_path,
/// snapshot_uid)`. Uses ONLY the public storage write API.
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

    // Distinct node keys -> unique node_uids.
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

    // Distinct CALLS edges (source/target by node_uid).
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

/// A fully-wired fixture: a `RepoState` with the synthetic LiveGraph + a SQLite mirroring its
/// caller/callee key sets. `_dir` is held to keep the db file alive for the test's lifetime.
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

    // The single resident TS partition makes intra-partition callers/callees Exact (precondition for
    // the orient ladder to reach the no-loss gate).
    let callers_env = lg.callers(&dst, Granularity::CallerDetail);
    assert_eq!(
        callers_env.class(),
        AnswerClass::Exact,
        "callers(dst) Exact precondition over the resident synthetic partition"
    );
    let ks_callers: Vec<String> = callers_env
        .data()
        .expect("callers data")
        .caller_identities
        .iter()
        .map(|(_, k)| k.clone())
        .collect();
    let callees_env = lg.callees(&src, Granularity::CallerDetail);
    assert_eq!(
        callees_env.class(),
        AnswerClass::Exact,
        "callees(src) Exact precondition over the resident synthetic partition"
    );
    let ks_callees: Vec<String> = callees_env
        .data()
        .expect("callees data")
        .callee_identities
        .iter()
        .map(|(k, _)| k.clone())
        .collect();

    // SQLite mirrors BOTH key sets: (k -> dst) for callers, (src -> k) for callees.
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

/// Seed the cycles + complexity no-loss certs GREEN at the LIVE fingerprint (isolates the cert-gated
/// label wiring; the genuine compare is unit-tested elsewhere).
fn seed_certs_green(state: &RepoState, snapshot_uid: &str) {
    let fp = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph set");
        import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
    };
    *state.cycles_cert.write() = Some(CycleNoLossCert {
        verdict: "GREEN".to_string(),
        values_verdict: "GREEN".to_string(),
        fingerprint: fp.clone(),
    });
    *state.complexity_cert.write() = Some(ComplexityNoLossCert {
        verdict: "GREEN".to_string(),
        fingerprint: fp,
    });
}

/// Seed ONLY the complexity no-loss cert at the LIVE fingerprint with the given verdict — for the
/// cert-divergence label test (a RED cert at the matching fingerprint short-circuits the genuine compare,
/// isolating the "RED -> labelled SQLite fallback" wiring; the real GREEN/RED compare is unit-tested by
/// `complexity_compare_is_exact`).
fn seed_complexity_cert(state: &RepoState, snapshot_uid: &str, verdict: &str) {
    let fp = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph set");
        import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
    };
    *state.complexity_cert.write() = Some(ComplexityNoLossCert {
        verdict: verdict.to_string(),
        fingerprint: fp,
    });
}

/// A repo-focus HIGH_COMPLEXITY signal value (the agent-built SQLite evidence; the daemon decides only
/// its LABEL). Field values are cosmetic for the label tests.
fn high_complexity_signal() -> Signal {
    Signal::high_complexity(HighComplexityEvidence {
        high_complexity_count: 0,
        threshold: repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD,
        top_complex: Vec::new(),
    })
}

/// Insert a FILE with a STALE file-version for `snapshot_uid`, so `get_stale_files` returns non-empty —
/// the AUTHORITATIVE stale condition `build_orient_envelope` reads from storage (review-9 gap 1),
/// independent of which signals survived ranking/budget. Reopens the db the fixture already created.
fn insert_stale_file(db_path: &Path, repo_uid: &str, snapshot_uid: &str) {
    let mut conn = StorageConnection::open(db_path).expect("reopen storage");
    conn.upsert_files(&[TrackedFile {
        file_uid: "f_stale".to_string(),
        repo_uid: repo_uid.to_string(),
        path: "src/stale.ts".to_string(),
        language: Some("typescript".to_string()),
        is_test: false,
        is_generated: false,
        is_excluded: false,
    }])
    .expect("upsert files");
    conn.upsert_file_versions(&[FileVersion {
        snapshot_uid: snapshot_uid.to_string(),
        file_uid: "f_stale".to_string(),
        content_hash: "h".to_string(),
        ast_hash: None,
        extractor: None,
        parse_status: "stale".to_string(),
        size_bytes: None,
        line_count: None,
        indexed_at: "2026-01-01T00:00:00Z".to_string(),
    }])
    .expect("upsert file versions");
}

fn orient_result(
    repo: &str,
    snapshot_uid: &str,
    focus: Focus,
    signals: Vec<Signal>,
) -> OrientResult {
    OrientResult {
        schema: ORIENT_SCHEMA,
        command: ORIENT_COMMAND,
        repo: repo.to_string(),
        display_name: None,
        snapshot: snapshot_uid.to_string(),
        focus,
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

// ── The four per-leaf DAEMON DECISIONS resolve to Livegraph ──

#[test]
fn orient_callers_outcome_serves_livegraph_when_sqlite_matches() {
    let f = setup();
    assert!(f.ks_callers.contains(&f.src), "src is a caller of dst");
    match orient_callers_outcome(&f.state, &f.dst, &f.snapshot_uid) {
        OrientLgOutcome::Livegraph {
            class,
            contributing_languages,
            ..
        } => {
            assert_eq!(class, AnswerClass::Exact);
            assert!(contributing_languages.contains(&LanguageSupport::TypeScriptPrimary));
        }
        OrientLgOutcome::Fallback { reason } => {
            panic!("expected Livegraph callers, got fallback {reason:?}")
        }
    }
}

#[test]
fn orient_callees_outcome_serves_livegraph_when_sqlite_matches() {
    let f = setup();
    assert!(f.ks_callees.contains(&f.dst), "dst is a callee of src");
    match orient_callees_outcome(&f.state, &f.src, &f.snapshot_uid) {
        OrientLgOutcome::Livegraph { class, .. } => assert_eq!(class, AnswerClass::Exact),
        OrientLgOutcome::Fallback { reason } => {
            panic!("expected Livegraph callees, got fallback {reason:?}")
        }
    }
}

#[test]
fn orient_cycles_outcome_serves_livegraph_with_green_cert() {
    let f = setup();
    {
        let guard = f.state.livegraph.read();
        let lg = guard.as_ref().unwrap();
        assert_eq!(
            lg.module_import_cycles().class(),
            AnswerClass::Exact,
            "module cycles Exact precondition over the resident partition"
        );
    }
    seed_certs_green(&f.state, &f.snapshot_uid);
    match orient_cycles_outcome(&f.state, &f.snapshot_uid) {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { reason } => {
            panic!("expected Livegraph cycles, got fallback {reason:?}")
        }
    }
}

#[test]
fn orient_complexity_outcome_serves_livegraph_with_green_cert() {
    let f = setup();
    let threshold = repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD as u32;
    {
        let guard = f.state.livegraph.read();
        let lg = guard.as_ref().unwrap();
        assert_eq!(
            lg.high_complexity(threshold).class(),
            AnswerClass::Exact,
            "high_complexity Exact precondition over the resident partition"
        );
    }
    seed_certs_green(&f.state, &f.snapshot_uid);
    match orient_complexity_outcome(&f.state, &f.snapshot_uid) {
        OrientLgOutcome::Livegraph { .. } => {}
        OrientLgOutcome::Fallback { reason } => {
            panic!("expected Livegraph complexity, got fallback {reason:?}")
        }
    }
}

// ── The assembled CoherenceEnvelope carries the leaves with the right SOURCE set ──

#[test]
fn build_orient_envelope_symbol_focus_callers_leaf_is_multi_source() {
    let f = setup();
    let result = orient_result(
        REPO,
        &f.snapshot_uid,
        Focus::symbol(&f.dst, &f.dst, None),
        vec![Signal::callers_summary(CallersSummaryEvidence {
            count: f.ks_callers.len() as u64,
            top_modules: Vec::new(),
        })],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&f.state, REPO, result, true, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::CallersSummary)
        .expect("callers leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "assembled CALLERS_SUMMARY leaf is multi-source {{livegraph, sqlite}} (review-6 pt3)"
    );
    assert!(leaf.provenance.fallback_reason.is_none());

    // The serialized rmapd wire shape shows BOTH sources on the leaf.
    let json = serde_json::to_value(&env).unwrap();
    let leaf_json = json["value"]["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["value"]["code"] == "CALLERS_SUMMARY")
        .unwrap();
    let sources = leaf_json["provenance"]["source"].as_array().unwrap();
    assert!(sources.iter().any(|s| s == "livegraph"));
    assert!(sources.iter().any(|s| s == "sqlite"));
}

#[test]
fn build_orient_envelope_symbol_focus_callees_leaf_is_multi_source() {
    let f = setup();
    let result = orient_result(
        REPO,
        &f.snapshot_uid,
        Focus::symbol(&f.src, &f.src, None),
        vec![Signal::callees_summary(CalleesSummaryEvidence {
            count: f.ks_callees.len() as u64,
            top_modules: Vec::new(),
        })],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&f.state, REPO, result, true, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::CalleesSummary)
        .expect("callees leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite])
    );
}

#[test]
fn build_orient_envelope_repo_focus_cycles_leaf_is_livegraph() {
    let f = setup();
    seed_certs_green(&f.state, &f.snapshot_uid);
    let result = orient_result(
        REPO,
        &f.snapshot_uid,
        Focus::repo(),
        vec![Signal::import_cycles(ImportCyclesEvidence {
            cycle_count: 0,
            cycles: Vec::new(),
        })],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&f.state, REPO, result, true, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ImportCycles)
        .expect("cycles leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph]),
        "assembled IMPORT_CYCLES leaf is single-source livegraph (field-exact cert)"
    );
    // The root provenance union now includes livegraph (the LG-served path is reached end-to-end).
    assert!(env.provenance.source.contains(&Source::Livegraph));
}

#[test]
fn build_orient_envelope_emits_producer_unavailable_limit_without_livegraph() {
    // review-6 pt1 (E5), integration level: a repo-focus orient that emits an LG-first signal but has
    // NO populated LiveGraph -> the leaf falls back (LiveGraphUnavailable) AND the assembled envelope
    // gains the machine-discoverable PRODUCER_UNAVAILABLE limit (through the real build_orient_envelope).
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // `state.livegraph` is None (never preloaded).
    let result = orient_result(
        REPO,
        &snapshot_uid,
        Focus::repo(),
        vec![Signal::import_cycles(ImportCyclesEvidence {
            cycle_count: 1,
            cycles: Vec::new(),
        })],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&state, REPO, result, false, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ImportCycles)
        .expect("cycles leaf present");
    assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert_eq!(
        leaf.provenance.fallback_reason,
        Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable)
    );
    assert!(
        env.value
            .limits
            .iter()
            .any(|l| l.code == repo_graph_agent::LimitCode::ProducerUnavailable),
        "envelope gains PRODUCER_UNAVAILABLE when an LG-first leaf has no LiveGraph"
    );
}

// ── review-9 gap 2: HIGH_COMPLEXITY through build_orient_envelope (the reviewer's required coverage) ──

#[test]
fn build_orient_envelope_repo_focus_complexity_leaf_is_multi_source() {
    // Green LiveGraph path -> correct provenance WITHOUT a false single-source claim. With the complexity
    // no-loss cert GREEN at the live fingerprint and an Exact high_complexity answer, the assembled
    // HIGH_COMPLEXITY leaf is multi-source {livegraph, sqlite} (the cert corroborates the (key,
    // complexity) SET; the rendered top-N sample stays SQLite-built) — never single-source `livegraph`.
    let f = setup();
    seed_certs_green(&f.state, &f.snapshot_uid);
    let result = orient_result(
        REPO,
        &f.snapshot_uid,
        Focus::repo(),
        vec![high_complexity_signal()],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&f.state, REPO, result, true, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::HighComplexity)
        .expect("complexity leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "assembled HIGH_COMPLEXITY leaf is multi-source {{livegraph, sqlite}} (review-9 gap 2)"
    );
    assert!(leaf.provenance.fallback_reason.is_none());
    // The root provenance union reaches livegraph end-to-end.
    assert!(env.provenance.source.contains(&Source::Livegraph));

    // The serialized rmapd wire shape shows BOTH sources on the leaf — never a bare `livegraph` claim.
    let json = serde_json::to_value(&env).unwrap();
    let leaf_json = json["value"]["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["value"]["code"] == "HIGH_COMPLEXITY")
        .unwrap();
    let sources = leaf_json["provenance"]["source"].as_array().unwrap();
    assert!(sources.iter().any(|s| s == "livegraph"));
    assert!(sources.iter().any(|s| s == "sqlite"));
}

#[test]
fn build_orient_envelope_complexity_no_livegraph_falls_back_unavailable() {
    // No LiveGraph -> the SQLite primary, labelled LiveGraphUnavailable, PRODUCER_UNAVAILABLE surfaced.
    // NEVER a `livegraph` claim (the false-provenance risk review-9 flagged is impossible here).
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // `state.livegraph` is None (never preloaded).
    let result = orient_result(
        REPO,
        &snapshot_uid,
        Focus::repo(),
        vec![high_complexity_signal()],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&state, REPO, result, false, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::HighComplexity)
        .expect("complexity leaf present");
    assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!leaf.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        leaf.provenance.fallback_reason,
        Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable)
    );
    assert!(
        env.value
            .limits
            .iter()
            .any(|l| l.code == repo_graph_agent::LimitCode::ProducerUnavailable),
        "envelope gains PRODUCER_UNAVAILABLE when HIGH_COMPLEXITY has no LiveGraph"
    );
}

#[test]
fn build_orient_envelope_complexity_cert_divergence_falls_back() {
    // Cert divergence -> labelled SQLite fallback. The high_complexity answer is Exact, but the complexity
    // no-loss cert is RED at the live fingerprint (LG set != SQLite) -> the SQLite primary, labelled
    // LiveGraphComplexityDivergence. NEVER a `livegraph` claim.
    let f = setup();
    seed_complexity_cert(&f.state, &f.snapshot_uid, "RED");
    let result = orient_result(
        REPO,
        &f.snapshot_uid,
        Focus::repo(),
        vec![high_complexity_signal()],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&f.state, REPO, result, false, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::HighComplexity)
        .expect("complexity leaf present");
    assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!leaf.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        leaf.provenance.fallback_reason,
        Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphComplexityDivergence)
    );
}

// ── review-9 gap 1: the authoritative stale flag (storage, not the emitted signal list) ──

#[test]
fn build_orient_envelope_stale_index_marks_leaves_stale_without_trust_signal() {
    // review-9 gap 1 regression: `stale` is derived from `get_stale_files` (storage), NOT from the
    // presence of TRUST_STALE_SNAPSHOT in the (ranked + budget-truncated) emitted signals. Here the index
    // IS stale but the result carries NO TRUST_STALE_SNAPSHOT signal (simulating truncation / a focus that
    // omits it). The SQLite leaf + the root MUST still be Stale and SQLITE_SNAPSHOT_STALE must fire —
    // proving a missing trust signal can no longer mint a false Fresh/Exact.
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    insert_stale_file(&db_path, REPO, &snapshot_uid);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");

    let result = orient_result(
        REPO,
        &snapshot_uid,
        Focus::repo(),
        vec![Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: snapshot_uid.clone(),
            scope: "repo".to_string(),
            basis_commit: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })],
    );
    // Fixture precondition: the emitted signals do NOT contain the (unreliable) TRUST_STALE_SNAPSHOT proxy.
    assert!(
        !result
            .signals
            .iter()
            .any(|s| s.code() == SignalCode::TrustStaleSnapshot),
        "fixture: no TRUST_STALE_SNAPSHOT signal is emitted"
    );

    let env =
        crate::orient_coherence::build_orient_envelope(&state, REPO, result, false, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::SnapshotInfo)
        .expect("snapshot-info leaf present");
    assert_eq!(
            leaf.freshness,
            FreshnessState::Stale,
            "SQLite leaf is Stale from the storage-authoritative flag despite no TRUST_STALE_SNAPSHOT signal"
        );
    assert_eq!(
        env.freshness,
        FreshnessState::Stale,
        "root freshness is Stale"
    );
    assert_ne!(
        env.trust.class,
        AnswerClass::Exact,
        "root trust is never Exact over a stale index"
    );
    assert!(
        env.value
            .limits
            .iter()
            .any(|l| l.code == repo_graph_agent::LimitCode::SqliteSnapshotStale),
        "SQLITE_SNAPSHOT_STALE fires from the authoritative stale flag"
    );
}

#[test]
fn build_orient_envelope_fresh_index_keeps_leaves_fresh() {
    // The complement: a non-stale index (no stale file-versions) keeps the SQLite leaf + root Fresh and
    // does NOT fire SQLITE_SNAPSHOT_STALE — proving the authoritative read does not over-report staleness.
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    let result = orient_result(
        REPO,
        &snapshot_uid,
        Focus::repo(),
        vec![Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: snapshot_uid.clone(),
            scope: "repo".to_string(),
            basis_commit: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })],
    );
    let env =
        crate::orient_coherence::build_orient_envelope(&state, REPO, result, false, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::SnapshotInfo)
        .expect("snapshot-info leaf present");
    assert_eq!(leaf.freshness, FreshnessState::Fresh);
    assert_eq!(env.freshness, FreshnessState::Fresh);
    assert!(
        !env.value
            .limits
            .iter()
            .any(|l| l.code == repo_graph_agent::LimitCode::SqliteSnapshotStale),
        "SQLITE_SNAPSHOT_STALE must NOT fire on a fresh index"
    );
}

// ════════════════════════════════════════════════════════════════════════════════════════════════
// review-1 item 1: the callgraph LABEL path is CERT-GATED — ZERO per-call SQLite read on a GREEN
// repo-wide callgraph cert, so the FULL `build_orient_envelope` served path (not just the
// `orient_serve` decorator's VALUE serve) is zero-read for the callgraph leaf. Uses the SHARED faithful
// callgraph-cert fixture (a byte-faithful SQLite mirror -> the cert is GREEN by construction).
// ════════════════════════════════════════════════════════════════════════════════════════════════

#[test]
fn gate_callgraph_label_green_cert_skips_per_call_sqlite_read() {
    // A PANICKING per-call closure proves a cached GREEN cert NEVER reads SQLite for the callgraph leaf
    // label (the zero-read proof for the label path; mirrors the existing
    // `callgraph_no_loss_already_fallback_skips_sqlite_read` panicking-closure discipline).
    let f = crate::callgraph_cert::test_fixture::build_fixture(false);
    assert!(
            crate::callgraph_cert::callgraph_is_green(&f.state, &f.snapshot_uid),
            "faithful mirror -> callgraph cert GREEN (pre-built, as handle_orient's bounded precheck does)"
        );
    let ladder = super::OrientLgOutcome::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: vec![],
        contributing_languages: BTreeSet::from([LanguageSupport::TypeScriptPrimary]),
    };
    let out = super::gate_callgraph_label(
        &f.state,
        &f.snapshot_uid,
        ladder,
        BTreeSet::from(["x".to_string()]),
        || -> Result<BTreeSet<String>, ()> {
            panic!(
                "GREEN callgraph cert must NOT trigger a per-call find_symbol_callers/callees read"
            )
        },
    );
    assert!(
        matches!(out, super::OrientLgOutcome::Livegraph { .. }),
        "GREEN cert keeps the livegraph label with ZERO per-call SQLite read"
    );
}

#[test]
fn gate_callgraph_label_not_green_runs_per_symbol_compare() {
    // No LiveGraph -> callgraph_is_green = false (no fingerprint, no build, no read) -> the per-symbol
    // compare runs: matching keys keep `livegraph`, divergent keys -> a callgraph-divergence fallback.
    // Proves the shipped per-symbol label granularity is preserved when the cert is not green (no
    // regression vs the per-symbol `orient_callers_outcome` path explain reuses).
    let f = crate::callgraph_cert::test_fixture::build_fixture(false);
    *f.state.livegraph.write() = None;
    let ladder = || super::OrientLgOutcome::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: vec![],
        contributing_languages: BTreeSet::from([LanguageSupport::TypeScriptPrimary]),
    };
    let kept = super::gate_callgraph_label(
        &f.state,
        &f.snapshot_uid,
        ladder(),
        BTreeSet::from(["a".to_string()]),
        || Ok::<_, ()>(BTreeSet::from(["a".to_string()])),
    );
    assert!(matches!(kept, super::OrientLgOutcome::Livegraph { .. }));
    let diverged = super::gate_callgraph_label(
        &f.state,
        &f.snapshot_uid,
        ladder(),
        BTreeSet::from(["a".to_string()]),
        || Ok::<_, ()>(BTreeSet::from(["b".to_string()])),
    );
    assert!(matches!(
        diverged,
        super::OrientLgOutcome::Fallback {
            reason: super::FallbackReason::LiveGraphCallgraphDivergence
        }
    ));
}

#[test]
fn build_orient_envelope_symbol_focus_callgraph_leaf_livegraph_via_cert() {
    // The FULL `build_orient_envelope` served path (not just the decorator): on a GREEN bounded cert a
    // symbol-focus orient labels CALLERS_SUMMARY `livegraph` via the callgraph cert (the cert-gated
    // outcome), end-to-end, with no per-call SQLite callgraph read.
    let f = crate::callgraph_cert::test_fixture::build_fixture(false);
    assert!(
            crate::orient_serve::orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
            "bounded cert GREEN (focus-resolution AND callgraph) — handle_orient would take the served path"
        );
    let callee = crate::callgraph_cert::test_fixture::callee_key();
    let result = orient_result(
        crate::callgraph_cert::test_fixture::REPO,
        &f.snapshot_uid,
        Focus::symbol(&callee, &callee, None),
        vec![Signal::callers_summary(CallersSummaryEvidence {
            count: 1,
            top_modules: Vec::new(),
        })],
    );
    let env = crate::orient_coherence::build_orient_envelope(
        &f.state,
        crate::callgraph_cert::test_fixture::REPO,
        result,
        true,  // serve_from_lg: handle_orient SERVED (bounded cert GREEN, asserted above)
        false, // module_summary_served: not exercised by this fixture (pre-M-2 label path)
        false, // enrich_in_flight (ORIENT-FACT-COHERENCE-1: not exercised here)
    );
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::CallersSummary)
        .expect("callers leaf present");
    assert!(
        leaf.provenance.source.contains(&Source::Livegraph),
        "GREEN callgraph cert -> CALLERS_SUMMARY labelled livegraph through the full envelope path"
    );
    assert!(leaf.provenance.fallback_reason.is_none());
}

// ── EC-M2-LEAF-SERVE-1: the MODULE_SUMMARY leaf LABEL follows the ACTUAL M-2 serve ──

/// `module_summary_served == true` (dispatch: module-summary cert GREEN at the witness
/// fingerprint — review-0 #1: independent of the bounded fold — ∧ epoch still resident) → the
/// MODULE_SUMMARY leaf is the multi-source `{livegraph, sqlite}` treatment (counts from the
/// LiveGraph inventory; the module-DISCOVERY half stays SQLite-built).
/// `module_summary_served == false` → the decision is ABSENT and the leaf renders the pre-M-2
/// fixed `{sqlite}` leaf byte-identically (every fallback path, incl. a RED module-summary cert
/// under a GREEN bounded fold).
#[test]
fn build_orient_envelope_module_summary_leaf_follows_actual_serve() {
    let f = setup();
    let module_summary_signal = || {
        Signal::module_summary(repo_graph_agent::ModuleSummaryEvidence {
            file_count: 3,
            symbol_count: 2,
            languages: vec!["typescript".to_string()],
            discovered_module_count: None,
            module_kinds: None,
            top_modules: Vec::new(),
            package_groups: Vec::new(),
            root_manifest_limitation: None,
        })
    };

    // SERVED: the leaf is multi-source {livegraph, sqlite}.
    let served_env = crate::orient_coherence::build_orient_envelope(
        &f.state,
        REPO,
        orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::repo(),
            vec![module_summary_signal()],
        ),
        true,  // serve_from_lg
        true,  // module_summary_served
        false, // enrich_in_flight (ORIENT-FACT-COHERENCE-1: not exercised here)
    );
    let leaf = served_env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ModuleSummary)
        .expect("module summary leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served counts -> multi-source {{livegraph, sqlite}} (counts live, discovery half SQLite)"
    );
    assert!(leaf.provenance.fallback_reason.is_none());

    // NOT SERVED: the pre-M-2 fixed sqlite leaf, byte-identical (no fallback reason minted — an
    // unserved MODULE_SUMMARY is the proven SQLite primary, not a failed LiveGraph attempt).
    let unserved_env = crate::orient_coherence::build_orient_envelope(
        &f.state,
        REPO,
        orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::repo(),
            vec![module_summary_signal()],
        ),
        true,  // serve_from_lg (bounded GREEN — e.g. the module-summary cert alone was RED)
        false, // module_summary_served
        false, // enrich_in_flight (ORIENT-FACT-COHERENCE-1: not exercised here)
    );
    let leaf = unserved_env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ModuleSummary)
        .expect("module summary leaf present");
    assert_eq!(
        leaf.provenance.source,
        BTreeSet::from([Source::Sqlite]),
        "unserved counts stay the plain sqlite leaf (RED path byte-identical)"
    );
    assert!(leaf.provenance.fallback_reason.is_none());
}
