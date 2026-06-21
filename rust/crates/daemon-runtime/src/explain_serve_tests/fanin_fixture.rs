//! COHERENCE-LEAF-SERVE-IMPL-2: a DEDICATED high-fan-in faithful fixture for the strengthened explain
//! parity proof (DR-EXPLAIN-CALLER-ORDER resolution, ratified `2d6d00d`).
//!
//! It is SEPARATE from the shared single-caller `callgraph_cert::test_fixture` (which is FILES_OUT_OF_SCOPE
//! — consumed as-is) because the strengthened proof needs a SYMBOL with fan-in > the budget cap across >=2
//! modules, which the shared fixture cannot express without disturbing the sibling orient/callgraph tests
//! that hard-assert its single caller/callee.
//!
//! Like the shared fixture it builds a resident LiveGraph AND a byte-faithful SQLite mirror from the SAME
//! explicit facts, so a GREEN bounded cert (focus-resolution ∧ callgraph) IS the parity proof. The shape:
//! a `hub` SYMBOL with [`ALPHA_N`] callers in module `alpha` (high concentration) + [`BETA_N`] callers in
//! module `beta` (low concentration), total fan-in > the Medium cap (15). The relevance ranking
//! (`agent::explain::call_ranking`) sorts the FULL caller set the SAME way whether served from SQLite or
//! the LiveGraph, so the budget-truncated explain output is byte-identical — the test that would FAIL
//! without the ranking. The SQLite `CALLS` edges are inserted in caller order while the IR edges are pushed
//! in REVERSE, so the two stores' raw (pre-rank) caller orders differ — the ranking is what reconciles them.

use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, IrEdge, IrNode, IrVisibility, Partition,
    PartitionId, PartitionKind, Provenance, SourceRange, SymbolAttributes,
};
use repo_graph_livegraph::LiveGraph;
use repo_graph_storage::types::{
    CreateSnapshotInput, GraphEdge, GraphNode, Repo, SourceLocation, TrackedFile,
    UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::LanguageSupport;
use std::path::Path;

use crate::state::RepoState;

pub(super) const REPO: &str = "repo_explain_fanin";

/// Callers in module `alpha` (high concentration → ranked first). 12 > the Medium cap leaves room for the
/// truncation to bite cross-module.
pub(super) const ALPHA_N: usize = 12;
/// Callers in module `beta` (low concentration → ranked after alpha). 12 + 6 = 18 > the cap (15).
pub(super) const BETA_N: usize = 6;

const ALPHA_FILE: &str = "alpha/a.ts";
const BETA_FILE: &str = "beta/b.ts";
const HUB_FILE: &str = "hub/h.ts";

pub(super) fn hub_key() -> String {
    format!("{REPO}:{HUB_FILE}#hub:SYMBOL:FUNCTION")
}

fn alpha_name(i: usize) -> String {
    format!("a{i:02}")
}
fn beta_name(i: usize) -> String {
    format!("b{i:02}")
}
fn alpha_key(i: usize) -> String {
    format!("{REPO}:{ALPHA_FILE}#{}:SYMBOL:FUNCTION", alpha_name(i))
}
fn beta_key(i: usize) -> String {
    format!("{REPO}:{BETA_FILE}#{}:SYMBOL:FUNCTION", beta_name(i))
}

/// The caller (name, file, key) tuples in CANONICAL order (alpha a00.., then beta b00..). The SQLite
/// `CALLS` edges are inserted in this order; the IR edges in the reverse — so the stores' raw caller orders
/// differ and only the ranking reconciles them.
fn callers() -> Vec<(String, &'static str, String)> {
    let mut v = Vec::new();
    for i in 0..ALPHA_N {
        v.push((alpha_name(i), ALPHA_FILE, alpha_key(i)));
    }
    for i in 0..BETA_N {
        v.push((beta_name(i), BETA_FILE, beta_key(i)));
    }
    v
}

/// The EXPECTED ranked + Medium-truncated caller `stable_key`s: all [`ALPHA_N`] alpha callers (concentration
/// 12, name ASC) then the first `cap - ALPHA_N` beta callers (concentration 6). Drives the parity assertion.
pub(super) fn expected_ranked_caller_keys(cap: usize) -> Vec<String> {
    let mut v: Vec<String> = (0..ALPHA_N).map(alpha_key).collect();
    for i in 0..(cap - ALPHA_N) {
        v.push(beta_key(i));
    }
    v
}

fn file_uid(path: &str) -> String {
    format!("fuid::{path}")
}
fn file_key(path: &str) -> String {
    format!("{REPO}:{path}:FILE")
}
fn module_key(dir: &str) -> String {
    format!("{REPO}:{dir}:MODULE")
}
fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn prov() -> Provenance {
    Provenance {
        indexer: "scip-typescript".into(),
        indexer_version: "0.4.0".into(),
        scip_symbol_id: None,
        build_inputs_hash: "h".into(),
    }
}

fn partition() -> Partition {
    Partition {
        id: PartitionId::new("p"),
        kind: PartitionKind::TsPackage,
        root: ".".into(),
        indexer: "scip-typescript".into(),
        indexer_version: "0.4.0".into(),
        build_inputs_hash: "h".into(),
        package_name: None,
        declared_dependencies: std::collections::BTreeSet::new(),
        tsconfig_aliases: None,
    }
}

fn file_node(path: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(file_key(path)),
        subtype: "File".into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        range: None,
        partition_id: PartitionId::new("p"),
        identity_source: IdentitySource::AstFileScope,
        provenance: prov(),
        attributes: None,
    }
}

fn symbol_node(key: &str, name: &str, path: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(key.to_string()),
        subtype: "Term".into(),
        name: name.into(),
        range: Some(SourceRange {
            file: path.into(),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        }),
        partition_id: PartitionId::new("p"),
        identity_source: IdentitySource::AstAdopted,
        provenance: prov(),
        attributes: Some(SymbolAttributes {
            visibility: Some(IrVisibility::Export),
            is_top_level: true,
            symbol_kind: Some("FUNCTION".into()),
        }),
    }
}

/// The resident IR: 3 files, the hub + 18 caller symbols, one `caller -> hub` CALLS edge per caller. Edges
/// are pushed in REVERSE caller order to diverge from the SQLite raw order.
fn build_ir() -> repo_graph_ir::PartitionIr {
    let mut ir = repo_graph_ir::PartitionIr::new(partition());
    ir.nodes.push(file_node(ALPHA_FILE));
    ir.nodes.push(file_node(BETA_FILE));
    ir.nodes.push(file_node(HUB_FILE));
    ir.nodes.push(symbol_node(&hub_key(), "hub", HUB_FILE));
    for (name, file, key) in callers() {
        ir.nodes.push(symbol_node(&key, &name, file));
    }
    for (_, _, key) in callers().into_iter().rev() {
        ir.edges.push(IrEdge {
            src: CanonicalKey::from_existing(key),
            dst: CanonicalKey::from_existing(hub_key()),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        });
    }
    ir
}

fn build_livegraph() -> LiveGraph {
    let mut lg = LiveGraph::new();
    lg.load_partition("p", build_ir(), LanguageSupport::TypeScriptPrimary);
    lg
}

fn graph_node(uid: &str, stable_key: &str, kind: &str) -> GraphNode {
    GraphNode {
        node_uid: uid.into(),
        snapshot_uid: String::new(),
        repo_uid: REPO.into(),
        stable_key: stable_key.into(),
        kind: kind.into(),
        subtype: None,
        name: stable_key.into(),
        qualified_name: None,
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: None,
        doc_comment: None,
        metadata_json: None,
    }
}

fn owns_edge(snapshot_uid: &str, uid: &str, module_uid: &str, file_uid: &str) -> GraphEdge {
    GraphEdge {
        edge_uid: uid.into(),
        snapshot_uid: snapshot_uid.into(),
        repo_uid: REPO.into(),
        source_node_uid: module_uid.into(),
        target_node_uid: file_uid.into(),
        edge_type: "OWNS".into(),
        resolution: "resolved".into(),
        extractor: "test".into(),
        location: None,
        metadata_json: None,
    }
}

/// Build a SQLite db whose `nodes`/`files`/`edges` faithfully mirror [`build_ir`] (3 FILE nodes, 3 directory
/// MODULE nodes, the hub + 18 SYMBOLs, OWNS + 18 CALLS edges). `CALLS` edges are inserted in canonical
/// caller order (the REVERSE of the IR edge order). Returns `(db_path, snapshot_uid)`.
fn build_sqlite_mirror(dir: &Path) -> (std::path::PathBuf, String) {
    let db_path = dir.join("repo.db");
    let mut conn = StorageConnection::open(&db_path).expect("open storage");
    conn.add_repo(&Repo {
        repo_uid: REPO.into(),
        name: REPO.into(),
        root_path: ".".into(),
        default_branch: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        metadata_json: None,
    })
    .expect("add repo");
    let snap = conn
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: REPO.into(),
            kind: "full".into(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .expect("create snapshot");
    let snapshot_uid = snap.snapshot_uid;

    // files
    let tracked: Vec<TrackedFile> = [ALPHA_FILE, BETA_FILE, HUB_FILE]
        .iter()
        .map(|path| TrackedFile {
            file_uid: file_uid(path),
            repo_uid: REPO.into(),
            path: (*path).into(),
            language: Some("typescript".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        })
        .collect();
    conn.upsert_files(&tracked).expect("upsert files");

    let mut nodes: Vec<GraphNode> = Vec::new();
    // FILE nodes
    for (i, path) in [ALPHA_FILE, BETA_FILE, HUB_FILE].iter().enumerate() {
        let mut n = graph_node(&format!("nf{i}"), &file_key(path), "FILE");
        n.snapshot_uid = snapshot_uid.clone();
        n.file_uid = Some(file_uid(path));
        nodes.push(n);
    }
    // directory MODULE nodes (one per file's owning dir: alpha, beta, hub)
    for (i, path) in [ALPHA_FILE, BETA_FILE, HUB_FILE].iter().enumerate() {
        let d = dirname(path);
        let mut m = graph_node(&format!("nm{i}"), &module_key(d), "MODULE");
        m.snapshot_uid = snapshot_uid.clone();
        m.qualified_name = Some(d.into());
        nodes.push(m);
    }
    // SYMBOL nodes: hub + the 18 callers
    let mut symbols: Vec<(String, String, &'static str)> =
        vec![("hub".into(), hub_key(), HUB_FILE)];
    for (name, file, key) in callers() {
        symbols.push((name, key, file));
    }
    for (i, (name, key, path)) in symbols.iter().enumerate() {
        let mut n = graph_node(&format!("ns{i}"), key, "SYMBOL");
        n.snapshot_uid = snapshot_uid.clone();
        n.name = name.clone();
        n.qualified_name = Some(name.clone());
        n.subtype = Some("FUNCTION".into());
        n.file_uid = Some(file_uid(path));
        n.location = Some(SourceLocation {
            line_start: 1,
            col_start: 0,
            line_end: 1,
            col_end: 0,
        });
        nodes.push(n);
    }
    conn.insert_nodes(&nodes).expect("insert nodes");

    // edges: OWNS (each MODULE -> its FILE) + CALLS (each caller -> hub, canonical order)
    let mut edges: Vec<GraphEdge> = vec![
        owns_edge(&snapshot_uid, "eo0", "nm0", "nf0"),
        owns_edge(&snapshot_uid, "eo1", "nm1", "nf1"),
        owns_edge(&snapshot_uid, "eo2", "nm2", "nf2"),
    ];
    // hub is symbol index 0 -> node_uid "ns0"; caller i is node_uid "ns{i+1}".
    for (i, _) in callers().iter().enumerate() {
        edges.push(GraphEdge {
            edge_uid: format!("ec{i}"),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: REPO.into(),
            source_node_uid: format!("ns{}", i + 1),
            target_node_uid: "ns0".into(),
            edge_type: "CALLS".into(),
            resolution: "resolved".into(),
            extractor: "test".into(),
            location: None,
            metadata_json: None,
        });
    }
    conn.insert_edges(&edges).expect("insert edges");

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    (db_path, snapshot_uid)
}

pub(super) struct FaninFixture {
    pub _dir: tempfile::TempDir,
    pub state: RepoState,
    pub snapshot_uid: String,
}

pub(super) fn build() -> FaninFixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror(dir.path());
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_livegraph());
    FaninFixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}
