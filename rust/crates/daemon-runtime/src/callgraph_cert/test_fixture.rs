//! COHERENCE-LEAF-SERVE-IMPL-1: a SHARED faithful fixture — a resident LiveGraph AND a byte-faithful
//! SQLite mirror built from the SAME explicit facts (two files in one module, a CALLS edge), so a GREEN
//! focus-resolution + callgraph cert IS the byte/value parity proof (the cert is a field-exact compare;
//! GREEN == "the LiveGraph (b) leaves equal the SQLite leaves"). Reused by `callgraph_cert::tests` and
//! `orient_serve::tests` (`pub(crate)` so the consumer's parity + no-eager-read proofs share one fixture
//! — no drift between what the cert proved and what the decorator serves).

use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, IrEdge, IrNode, IrVisibility, Partition,
    PartitionId, PartitionIr, PartitionKind, Provenance, SourceRange, SymbolAttributes,
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

pub(crate) const REPO: &str = "repo_callgraph_cert";
pub(crate) const CALLER_PATH: &str = "src/a.ts";
pub(crate) const CALLEE_PATH: &str = "src/b.ts";
pub(crate) const MODULE_DIR: &str = "src";

pub(crate) fn caller_key() -> String {
    format!("{REPO}:{CALLER_PATH}#callerFn:SYMBOL:FUNCTION")
}
pub(crate) fn callee_key() -> String {
    format!("{REPO}:{CALLEE_PATH}#calleeFn:SYMBOL:FUNCTION")
}
fn file_key(path: &str) -> String {
    format!("{REPO}:{path}:FILE")
}
fn module_key() -> String {
    format!("{REPO}:{MODULE_DIR}:MODULE")
}
fn file_uid(path: &str) -> String {
    format!("fuid::{path}")
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

/// The resident IR: two files (one module `src`), two symbols, one `caller -> callee` CALLS edge.
pub(crate) fn build_ir() -> PartitionIr {
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(CALLEE_PATH));
    ir.nodes
        .push(symbol_node(&caller_key(), "callerFn", CALLER_PATH));
    ir.nodes
        .push(symbol_node(&callee_key(), "calleeFn", CALLEE_PATH));
    ir.edges.push(IrEdge {
        src: CanonicalKey::from_existing(caller_key()),
        dst: CanonicalKey::from_existing(callee_key()),
        edge_type: EdgeType::Calls,
        basis: EdgeBasis::SyntaxConfirmedCall,
        provenance: prov(),
        import: None,
    });
    ir
}

pub(crate) fn build_livegraph() -> LiveGraph {
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

/// Build a SQLite db whose `nodes`/`files`/`edges` faithfully mirror [`build_ir`] (FILE + SYMBOL +
/// directory MODULE nodes, OWNS + CALLS edges, files rows). `drop_calls`, when true, omits the CALLS
/// edge (a divergence the callgraph cert MUST catch -> RED). Returns `(db_path, snapshot_uid)`.
pub(crate) fn build_sqlite_mirror(dir: &Path, drop_calls: bool) -> (std::path::PathBuf, String) {
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
    let tracked: Vec<TrackedFile> = [CALLER_PATH, CALLEE_PATH]
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

    // nodes: FILE (a, b), MODULE (src), SYMBOL (caller, callee)
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (i, path) in [CALLER_PATH, CALLEE_PATH].iter().enumerate() {
        let mut n = graph_node(&format!("nf{i}"), &file_key(path), "FILE");
        n.snapshot_uid = snapshot_uid.clone();
        n.file_uid = Some(file_uid(path));
        nodes.push(n);
    }
    let mut module = graph_node("nm0", &module_key(), "MODULE");
    module.snapshot_uid = snapshot_uid.clone();
    module.qualified_name = Some(MODULE_DIR.into());
    nodes.push(module);
    for (uid, key, name, path) in [
        ("ns0", caller_key(), "callerFn", CALLER_PATH),
        ("ns1", callee_key(), "calleeFn", CALLEE_PATH),
    ] {
        let mut n = graph_node(uid, &key, "SYMBOL");
        n.snapshot_uid = snapshot_uid.clone();
        n.name = name.into();
        n.qualified_name = Some(name.into());
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

    // edges: OWNS (module -> each FILE), CALLS (caller -> callee)
    let mut edges: Vec<GraphEdge> = vec![
        owns_edge(&snapshot_uid, "eo0", "nm0", "nf0"),
        owns_edge(&snapshot_uid, "eo1", "nm0", "nf1"),
    ];
    if !drop_calls {
        edges.push(GraphEdge {
            edge_uid: "ec0".into(),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: REPO.into(),
            source_node_uid: "ns0".into(),
            target_node_uid: "ns1".into(),
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

/// A fully-wired fixture: a `RepoState` whose SQLite faithfully mirrors the resident LiveGraph. `_dir`
/// is held to keep the db file alive for the test's lifetime. `drop_calls` omits the SQLite CALLS edge
/// to force a callgraph-cert RED.
pub(crate) struct Fixture {
    pub _dir: tempfile::TempDir,
    pub state: RepoState,
    pub snapshot_uid: String,
}

pub(crate) fn build_fixture(drop_calls: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror(dir.path(), drop_calls);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_livegraph());
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}
