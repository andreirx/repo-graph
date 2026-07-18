//! COHERENCE-LEAF-SERVE-IMPL-1: a SHARED faithful fixture — a resident LiveGraph AND a byte-faithful
//! SQLite mirror built from the SAME explicit facts (three files in two modules, a CALLS edge, a
//! `src` ↔ `lib` import cycle), so a GREEN focus-resolution + callgraph cert IS the byte/value parity
//! proof (the cert is a field-exact compare; GREEN == "the LiveGraph (b) leaves equal the SQLite
//! leaves"). Reused by `callgraph_cert::tests` and `orient_serve::tests` (`pub(crate)` so the
//! consumer's parity + no-eager-read proofs share one fixture — no drift between what the cert proved
//! and what the decorator serves).
//!
//! EC-M2-LEAF-SERVE-1 extended the mirror with the facts the M-2 leaves need, mirroring the REAL
//! writers' output shape:
//! - `file_versions` rows for every tracked file (the real index writes them; `compute_repo_summary`
//!   counts files/languages through the `file_versions` join, so the module-summary
//!   identity-reconciliation cert needs them on the SQLite side);
//! - a THIRD file `lib/c.ts` and a REAL two-module import cycle `src` ↔ `lib`, present on BOTH sides:
//!   LiveGraph as FILE-level `AstImport` IR edges (module cycles aggregate by dirname), SQLite as the
//!   indexer's shape [OBSERVED: orchestrator.rs create_module_* — MODULE nodes with `name` =
//!   directory basename + `qualified_name` = full dir path; FILE→FILE and MODULE→MODULE IMPORTS
//!   edges]. The MODULE `name` = basename fidelity is load-bearing for the cycle-VALUES cert (the
//!   repo-level cycle render uses `CycleNode.name`).

use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, ImportEdgeMeta, ImportResolution, IrEdge,
    IrNode, IrVisibility, Partition, PartitionId, PartitionIr, PartitionKind, Provenance,
    SourceRange, SymbolAttributes,
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
/// EC-M2-LEAF-SERVE-1: the second module's file — `src` ↔ `lib` is the fixture's REAL module
/// import cycle (both stores), exercising the cycle-VALUES serve with a non-empty answer.
pub(crate) const LIB_PATH: &str = "lib/c.ts";
pub(crate) const LIB_DIR: &str = "lib";

pub(crate) fn caller_key() -> String {
    format!("{REPO}:{CALLER_PATH}#callerFn:SYMBOL:FUNCTION")
}
pub(crate) fn callee_key() -> String {
    format!("{REPO}:{CALLEE_PATH}#calleeFn:SYMBOL:FUNCTION")
}
fn file_key(path: &str) -> String {
    format!("{REPO}:{path}:FILE")
}
fn module_key_for(dir: &str) -> String {
    format!("{REPO}:{dir}:MODULE")
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

/// A FILE-level `AstImport` IR edge (`src_path` imports `dst_path`) — the LiveGraph's import
/// substrate; `module_import_cycles` aggregates these to dirname modules before Tarjan.
fn import_edge(src_path: &str, dst_path: &str) -> IrEdge {
    IrEdge {
        src: CanonicalKey::from_existing(file_key(src_path)),
        dst: CanonicalKey::from_existing(file_key(dst_path)),
        edge_type: EdgeType::Imports,
        basis: EdgeBasis::AstImport,
        provenance: prov(),
        import: Some(ImportEdgeMeta {
            raw_specifier: format!("../{}", dst_path.trim_end_matches(".ts")),
            resolved_path: dst_path.to_string(),
            resolution: ImportResolution::StaticResolved,
        }),
    }
}

/// The resident IR: three files (modules `src` + `lib`), two symbols, one `caller -> callee` CALLS
/// edge, and the `src` ↔ `lib` FILE-level import cycle (EC-M2: aggregates to ONE module cycle).
pub(crate) fn build_ir() -> PartitionIr {
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(CALLEE_PATH));
    ir.nodes.push(file_node(LIB_PATH));
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
    ir.edges.push(import_edge(CALLER_PATH, LIB_PATH));
    ir.edges.push(import_edge(LIB_PATH, CALLER_PATH));
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

    // files + file_versions (EC-M2: the real index writes BOTH; `compute_repo_summary` and the
    // module-summary cert's `file_structural_rows` count through the `file_versions` join)
    let tracked: Vec<TrackedFile> = [CALLER_PATH, CALLEE_PATH, LIB_PATH]
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
    let versions: Vec<repo_graph_storage::types::FileVersion> =
        [CALLER_PATH, CALLEE_PATH, LIB_PATH]
            .iter()
            .map(|path| repo_graph_storage::types::FileVersion {
                snapshot_uid: snapshot_uid.clone(),
                file_uid: file_uid(path),
                content_hash: "deadbeef".into(),
                ast_hash: None,
                extractor: Some("test".into()),
                parse_status: "parsed".into(),
                size_bytes: Some(1),
                line_count: Some(1),
                indexed_at: "2026-01-01T00:00:00Z".into(),
            })
            .collect();
    conn.upsert_file_versions(&versions)
        .expect("upsert file versions");

    // nodes: FILE (a, b, c), MODULE (src, lib), SYMBOL (caller, callee). MODULE `name` is the
    // directory BASENAME + `qualified_name` the full dir path — the real writer's shape
    // [orchestrator.rs create_module_nodes] — load-bearing for the repo-level cycle render.
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (i, path) in [CALLER_PATH, CALLEE_PATH, LIB_PATH].iter().enumerate() {
        let mut n = graph_node(&format!("nf{i}"), &file_key(path), "FILE");
        n.snapshot_uid = snapshot_uid.clone();
        n.file_uid = Some(file_uid(path));
        nodes.push(n);
    }
    for (uid, dir) in [("nm0", MODULE_DIR), ("nm1", LIB_DIR)] {
        let mut module = graph_node(uid, &module_key_for(dir), "MODULE");
        module.snapshot_uid = snapshot_uid.clone();
        module.name = dir.rsplit('/').next().unwrap_or(dir).into();
        module.qualified_name = Some(dir.into());
        nodes.push(module);
    }
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

    // edges: OWNS (module -> each FILE), CALLS (caller -> callee), and the `src` ↔ `lib` import
    // cycle in the real writer's TWO granularities — FILE→FILE IMPORTS (resolved file imports)
    // AND MODULE→MODULE IMPORTS (`find_cycles(level=module)` walks ONLY MODULE-kind endpoints).
    let mut edges: Vec<GraphEdge> = vec![
        owns_edge(&snapshot_uid, "eo0", "nm0", "nf0"),
        owns_edge(&snapshot_uid, "eo1", "nm0", "nf1"),
        owns_edge(&snapshot_uid, "eo2", "nm1", "nf2"),
        imports_edge(&snapshot_uid, "ei0", "nf0", "nf2"),
        imports_edge(&snapshot_uid, "ei1", "nf2", "nf0"),
        imports_edge(&snapshot_uid, "em0", "nm0", "nm1"),
        imports_edge(&snapshot_uid, "em1", "nm1", "nm0"),
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

/// EC-M2-LEAF-SERVE-1: a resolved IMPORTS edge (FILE→FILE or MODULE→MODULE — the writer emits both
/// granularities from resolved file imports).
fn imports_edge(snapshot_uid: &str, uid: &str, source_uid: &str, target_uid: &str) -> GraphEdge {
    GraphEdge {
        edge_uid: uid.into(),
        snapshot_uid: snapshot_uid.into(),
        repo_uid: REPO.into(),
        source_node_uid: source_uid.into(),
        target_node_uid: target_uid.into(),
        edge_type: "IMPORTS".into(),
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
