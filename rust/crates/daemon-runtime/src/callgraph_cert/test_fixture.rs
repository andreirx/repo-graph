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
    partition_with_id("p")
}

/// RECON-M-R2: [`partition`] with an explicit id — the pipeline-only fixture loads TWO partitions
/// (the `boundary` sub-class needs endpoints in DISTINCT compiler runs).
fn partition_with_id(id: &str) -> Partition {
    Partition {
        id: PartitionId::new(id),
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
    file_node_in(path, "p")
}

fn file_node_in(path: &str, pid: &str) -> IrNode {
    IrNode {
        key: CanonicalKey::from_existing(file_key(path)),
        subtype: "File".into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        range: None,
        partition_id: PartitionId::new(pid),
        identity_source: IdentitySource::AstFileScope,
        provenance: prov(),
        attributes: None,
    }
}

fn symbol_node(key: &str, name: &str, path: &str) -> IrNode {
    symbol_node_in(key, name, path, "p")
}

fn symbol_node_in(key: &str, name: &str, path: &str, pid: &str) -> IrNode {
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
        partition_id: PartitionId::new(pid),
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
    build_ir_with_calls(1)
}

/// RECON-M-R1: [`build_ir`] with a parameterized `caller -> callee` CALLS multiplicity (the S
/// witness's occurrence count for the pair) — the instance-fixture substrate (§3.3: the
/// `multiplicity` sub-classes are measured-empty at amodx scale, so they are FIXTURE-proven).
pub(crate) fn build_ir_with_calls(s_calls: usize) -> PartitionIr {
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(CALLEE_PATH));
    ir.nodes.push(file_node(LIB_PATH));
    ir.nodes
        .push(symbol_node(&caller_key(), "callerFn", CALLER_PATH));
    ir.nodes
        .push(symbol_node(&callee_key(), "calleeFn", CALLEE_PATH));
    for _ in 0..s_calls {
        ir.edges.push(IrEdge {
            src: CanonicalKey::from_existing(caller_key()),
            dst: CanonicalKey::from_existing(callee_key()),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        });
    }
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
    build_sqlite_mirror_with_calls(dir, if drop_calls { 0 } else { 1 })
}

/// RECON-M-R1: [`build_sqlite_mirror`] with a parameterized `caller -> callee` CALLS multiplicity
/// (the P witness's occurrence count for the pair — one `edges` row per instance, mirroring the
/// real un-DISTINCT pipeline shape).
pub(crate) fn build_sqlite_mirror_with_calls(
    dir: &Path,
    p_calls: usize,
) -> (std::path::PathBuf, String) {
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
    for i in 0..p_calls {
        edges.push(GraphEdge {
            edge_uid: format!("ec{i}"),
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

/// RECON-M-R1 (the §3.3 INSTANCE fixtures): the standard two-symbol pair with INDEPENDENT
/// per-witness occurrence counts — P holds `p_calls` CALLS rows, S holds `s_calls` strict-`Calls`
/// IR edges for the SAME `(callerFn, calleeFn)` pair. `(2, 1)` is the P-excess
/// (`syntactic`/`multiplicity`) fixture; `(1, 2)` the S-excess (`semantic`/`multiplicity`) one —
/// both measured-empty at amodx scale, hence fixture-proven (M-R1 gate).
pub(crate) fn build_multiplicity_fixture(p_calls: usize, s_calls: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror_with_calls(dir.path(), p_calls);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    let mut lg = LiveGraph::new();
    lg.load_partition(
        "p",
        build_ir_with_calls(s_calls),
        LanguageSupport::TypeScriptPrimary,
    );
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R1 (the R-RAT-4 COLLISION-GUARD fixture): the faithful two-symbol pair, EXCEPT the S
/// side's `calleeFn` node is a `ScipSynthesizedFallback` identity whose key BYTE-EQUALS the
/// pipeline's `calleeFn` key. The ingest cannot currently MINT this state — that impossibility IS
/// the contingent spelling disjointness §3.5 measures — so the fixture constructs the IR directly
/// at the guard's own layer (the established hand-built-`PartitionIr` pattern). Without the guard,
/// the byte-equal keys would silently merge and mint a false `both`.
pub(crate) fn build_collision_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror_with_calls(dir.path(), 1);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    let mut ir = build_ir_with_calls(1);
    for n in ir.nodes.iter_mut() {
        if n.key.as_str() == callee_key() {
            n.identity_source = IdentitySource::ScipSynthesizedFallback;
            // Fallback nodes carry no producer AST attributes (unknown, not zero — the IR rule).
            n.attributes = None;
        }
    }
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R2 keys for the PIPELINE-ONLY fixture (below).
pub(crate) fn other_key() -> String {
    format!("{REPO}:{CALLER_PATH}#otherFn:SYMBOL:FUNCTION")
}
pub(crate) const RUST_PATH: &str = "src/r.rs";
pub(crate) fn rust_fn_key() -> String {
    format!("{REPO}:{RUST_PATH}#rustFn:SYMBOL:FUNCTION")
}
pub(crate) fn rust_caller_key() -> String {
    format!("{REPO}:{RUST_PATH}#rustCaller:SYMBOL:FUNCTION")
}

/// RECON-M-R2 (the M-R2 gate's ADDED pipeline-only fixture): P rows ABSENT from S — the committed
/// fixture cannot produce this shape (spike §5.3: pipeline_only = 0); the amodx artifacts prove it
/// live and INFORM the fixture's two dual-measured sub-class shapes (recon-design-1 §3.1/§3.0b):
///
/// - **boundary**: `callerFn` (partition `p1`) CALLS `calleeFn` (partition `p2`) — both endpoints
///   known to S, partition sets DISJOINT (two compiler runs; amodx's dominant class, 11/13);
/// - **uncorroborated ×2**: `callerFn` CALLS `otherFn` (SAME partition `p1`; S measured, holds no
///   such call — amodx's misresolution-bearing class) AND `callerFn` CALLS `rustFn` (an endpoint
///   ABSENT from S entirely — the M-R1 precedence rule's "no two-compiler-runs story" arm);
/// - **unmeasured**: `rustCaller` CALLS `rustFn` — BOTH endpoints outside S (an uncovered-`.rs`
///   pair; zap-engine's 98.2% lesson at fixture scale): coverage, never divergence (§3.6), and
///   the R-1 mixed-repo scoping shape for serving (the answer falls back, byte-identical).
///
/// The S side holds NO call edges at all, so the ledger verdict is RED (divergent) — which also
/// makes this the M-R2 DIVERGENT-CAPTURE fixture class (flag-ON captures, flag-OFF does not).
pub(crate) fn build_pipeline_only_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("repo.db");
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

    // files: the two covered TS files + the uncovered Rust file (language honest per row).
    let tracked: Vec<TrackedFile> = [
        (CALLER_PATH, "typescript"),
        (CALLEE_PATH, "typescript"),
        (RUST_PATH, "rust"),
    ]
    .iter()
    .map(|(path, lang)| TrackedFile {
        file_uid: file_uid(path),
        repo_uid: REPO.into(),
        path: (*path).into(),
        language: Some((*lang).into()),
        is_test: false,
        is_generated: false,
        is_excluded: false,
    })
    .collect();
    conn.upsert_files(&tracked).expect("upsert files");

    // SYMBOL nodes with real files + locations (P rows must serve VERBATIM with their definition
    // locations beside the null-location S-minted rows — the §3.3a contrast).
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (uid, key, name, path) in [
        ("ns0", caller_key(), "callerFn", CALLER_PATH),
        ("ns1", callee_key(), "calleeFn", CALLEE_PATH),
        ("ns2", other_key(), "otherFn", CALLER_PATH),
        ("ns3", rust_fn_key(), "rustFn", RUST_PATH),
        ("ns4", rust_caller_key(), "rustCaller", RUST_PATH),
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

    // P CALLS: boundary + uncorroborated(same-partition) + uncorroborated(endpoint-absent) +
    // unmeasured(uncovered pair). One `edges` row per instance (the un-DISTINCT pipeline shape).
    let calls: Vec<GraphEdge> = [
        ("ec0", "ns0", "ns1"), // callerFn -> calleeFn  (boundary)
        ("ec1", "ns0", "ns2"), // callerFn -> otherFn   (uncorroborated, same partition)
        ("ec2", "ns0", "ns3"), // callerFn -> rustFn    (uncorroborated, endpoint absent from S)
        ("ec3", "ns4", "ns3"), // rustCaller -> rustFn  (unmeasured — both endpoints outside S)
    ]
    .iter()
    .map(|(uid, src, dst)| GraphEdge {
        edge_uid: (*uid).into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: REPO.into(),
        source_node_uid: (*src).into(),
        target_node_uid: (*dst).into(),
        edge_type: "CALLS".into(),
        resolution: "resolved".into(),
        extractor: "test".into(),
        location: None,
        metadata_json: None,
    })
    .collect();
    conn.insert_edges(&calls).expect("insert edges");

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    // S side: TWO partitions, NO call edges. p1 holds callerFn + otherFn; p2 holds calleeFn.
    // rustFn / rustCaller are absent from S entirely (the uncovered language).
    let mut ir1 = PartitionIr::new(partition_with_id("p1"));
    ir1.nodes.push(file_node_in(CALLER_PATH, "p1"));
    ir1.nodes
        .push(symbol_node_in(&caller_key(), "callerFn", CALLER_PATH, "p1"));
    ir1.nodes
        .push(symbol_node_in(&other_key(), "otherFn", CALLER_PATH, "p1"));
    let mut ir2 = PartitionIr::new(partition_with_id("p2"));
    ir2.nodes.push(file_node_in(CALLEE_PATH, "p2"));
    ir2.nodes
        .push(symbol_node_in(&callee_key(), "calleeFn", CALLEE_PATH, "p2"));
    let mut lg = LiveGraph::new();
    lg.load_partition("p1", ir1, LanguageSupport::TypeScriptPrimary);
    lg.load_partition("p2", ir2, LanguageSupport::TypeScriptPrimary);

    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R2 iteration 2: the third symbol of the per-symbol-unanswerability fixture (below).
pub(crate) fn clean_fn_key() -> String {
    format!("{REPO}:{CALLER_PATH}#cleanFn:SYMBOL:FUNCTION")
}

/// RECON-M-R2 iteration 2 (the review-1 §3.6 serving fix's fixture): an anchor in a
/// Fresh/resident ELIGIBLE TS partition whose OWN callers/callees projection is `Partial` —
/// per-symbol unanswerability INSIDE W-BOTH — with one pair measured from the OTHER endpoint's
/// projection and one pair neither projection measured:
///
/// - S partition `p` holds `callerFn` (AstAdopted), `calleeFn` (**`ScipSynthesizedFallback`
///   whose key byte-equals P's — the collision-guard shape**), `cleanFn` (AstAdopted, NO
///   outgoing S edges), and ONE S `Calls` edge `callerFn -> calleeFn` (WITHHELD by the guard).
/// - P holds `callerFn -> calleeFn` AND `cleanFn -> calleeFn`.
///
/// Ledger mechanics at the shared fingerprint: every projection touching the fallback-identity
/// `calleeFn` degrades to `Partial` (`ScipFallbackIdentity` — unanswerable, `lg_caller_rows`/
/// `lg_callee_rows` are Exact-only), so `(callerFn, calleeFn)` is measured by NEITHER projection
/// → `dual_measured: false` → its served row is UNMEASURED (no witness field). `cleanFn`'s
/// callees-projection is `Exact` (measured-empty: no S edges, touches no degraded node), so
/// `(cleanFn, calleeFn)` IS dual-measured with `s_calls == 0` → `syntactic`/uncorroborated.
/// `callers(calleeFn)` therefore serves a MIXED union answer {syntactic: 1, unmeasured: 1} whose
/// own projection is `Partial`; `callees(callerFn)` serves {unmeasured: 1} — both directions of
/// the review-1 required test. The withheld S instance can never serve: the ledger's
/// collision-excluded `s_calls` is the assembly's ONLY S source (the structural barrier — now the
/// SOLE barrier, since a per-symbol `Partial` no longer falls back).
pub(crate) fn build_partial_unanswerable_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("repo.db");
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

    let mut nodes: Vec<GraphNode> = Vec::new();
    for (uid, key, name, path) in [
        ("ns0", caller_key(), "callerFn", CALLER_PATH),
        ("ns1", callee_key(), "calleeFn", CALLEE_PATH),
        ("ns2", clean_fn_key(), "cleanFn", CALLER_PATH),
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

    let calls: Vec<GraphEdge> = [
        ("ec0", "ns0", "ns1"), // callerFn -> calleeFn (pair neither projection measures)
        ("ec1", "ns2", "ns1"), // cleanFn  -> calleeFn (pair measured from cleanFn's side)
    ]
    .iter()
    .map(|(uid, src, dst)| GraphEdge {
        edge_uid: (*uid).into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: REPO.into(),
        source_node_uid: (*src).into(),
        target_node_uid: (*dst).into(),
        edge_type: "CALLS".into(),
        resolution: "resolved".into(),
        extractor: "test".into(),
        location: None,
        metadata_json: None,
    })
    .collect();
    conn.insert_edges(&calls).expect("insert edges");

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    // S side: one Fresh TS partition; calleeFn is the colliding fallback identity; cleanFn has
    // no outgoing edges (its callees-projection stays Exact — the measured side).
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(CALLEE_PATH));
    ir.nodes
        .push(symbol_node(&caller_key(), "callerFn", CALLER_PATH));
    ir.nodes
        .push(symbol_node(&clean_fn_key(), "cleanFn", CALLER_PATH));
    let mut callee = symbol_node(&callee_key(), "calleeFn", CALLEE_PATH);
    callee.identity_source = IdentitySource::ScipSynthesizedFallback;
    // Fallback nodes carry no producer AST attributes (unknown, not zero — the IR rule).
    callee.attributes = None;
    ir.nodes.push(callee);
    ir.edges.push(IrEdge {
        src: CanonicalKey::from_existing(caller_key()),
        dst: CanonicalKey::from_existing(callee_key()),
        edge_type: EdgeType::Calls,
        basis: EdgeBasis::SyntaxConfirmedCall,
        provenance: prov(),
        import: None,
    });
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);

    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R2 iteration 3: the ghost symbols of the Unavailable-in-W-BOTH fixture (below) —
/// pipeline symbols in RESIDENT TS files that S's producer never emitted (absent from the xref).
pub(crate) fn ghost_fn_key() -> String {
    format!("{REPO}:{CALLEE_PATH}#ghostFn:SYMBOL:FUNCTION")
}
pub(crate) fn ghost_caller_key() -> String {
    format!("{REPO}:{CALLER_PATH}#ghostCaller:SYMBOL:FUNCTION")
}
pub(crate) fn ghost_target_key() -> String {
    format!("{REPO}:{CALLEE_PATH}#ghostTarget:SYMBOL:FUNCTION")
}

/// RECON-M-R2 iteration 3 (the review-2 fix's fixture): a P-only anchor in a Fresh, RESIDENT,
/// TS file that is ABSENT from the S xref — per-symbol `Unavailable` INSIDE W-BOTH (§3.6's
/// second unanswerable class; measured real at amodx scale: 128 `Unavailable` projections on a
/// fully-covered TS corpus). The anchor's own envelope carries NO regime evidence
/// (`FreshnessState::Unavailable`, empty languages), so serving eligibility comes from its
/// FILE's partition state (`LiveGraph::file_partition_status`) — the review-2 discriminator.
///
/// Shape (per direction, one dual-measured + one unmeasured pair):
/// - S partition `p` (Fresh, TS) holds FILE nodes for BOTH TS files plus `callerFn`/`calleeFn`
///   (AstAdopted), and NO call edges. `ghostFn`/`ghostCaller`/`ghostTarget` exist ONLY in P.
/// - P CALLS: `callerFn -> ghostFn` (measured from callerFn's Exact callees-projection →
///   `syntactic`/uncorroborated), `ghostCaller -> ghostFn` (NEITHER projection measures →
///   unmeasured), `ghostFn -> calleeFn` (measured from calleeFn's Exact callers-projection →
///   `syntactic`), `ghostFn -> ghostTarget` (neither → unmeasured).
///
/// `callers(ghostFn)` and `callees(ghostFn)` therefore each serve a MIXED union answer
/// {syntactic: 1, unmeasured: 1} on an anchor whose OWN class is `Unavailable` — the review-2
/// required test's both-direction substrate. Distinct builder (abstraction ledger): users are
/// the two direction assertions of the review-2 gate test; axis: `Unavailable`-class anchor
/// inside an eligible partition (vs the sibling fixture's `Partial`-class anchor); simpler
/// alternative rejected: extending `build_pipeline_only_fixture` — every ghost edge placement
/// would shift that ratified fixture's amodx-informed gate assertions.
pub(crate) fn build_unavailable_in_w_both_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("repo.db");
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

    let mut nodes: Vec<GraphNode> = Vec::new();
    for (uid, key, name, path) in [
        ("ns0", caller_key(), "callerFn", CALLER_PATH),
        ("ns1", callee_key(), "calleeFn", CALLEE_PATH),
        ("ns2", ghost_fn_key(), "ghostFn", CALLEE_PATH),
        ("ns3", ghost_caller_key(), "ghostCaller", CALLER_PATH),
        ("ns4", ghost_target_key(), "ghostTarget", CALLEE_PATH),
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

    let calls: Vec<GraphEdge> = [
        ("ec0", "ns0", "ns2"), // callerFn   -> ghostFn     (dual-measured via callerFn's side)
        ("ec1", "ns3", "ns2"), // ghostCaller -> ghostFn    (neither side measures — unmeasured)
        ("ec2", "ns2", "ns1"), // ghostFn    -> calleeFn    (dual-measured via calleeFn's side)
        ("ec3", "ns2", "ns4"), // ghostFn    -> ghostTarget (neither side measures — unmeasured)
    ]
    .iter()
    .map(|(uid, src, dst)| GraphEdge {
        edge_uid: (*uid).into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: REPO.into(),
        source_node_uid: (*src).into(),
        target_node_uid: (*dst).into(),
        edge_type: "CALLS".into(),
        resolution: "resolved".into(),
        extractor: "test".into(),
        location: None,
        metadata_json: None,
    })
    .collect();
    conn.insert_edges(&calls).expect("insert edges");

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    // S side: ONE Fresh TS partition holding both FILE nodes and the two S-known symbols only —
    // the ghosts are structurally absent from S's world, while their FILES are inside it.
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(CALLEE_PATH));
    ir.nodes
        .push(symbol_node(&caller_key(), "callerFn", CALLER_PATH));
    ir.nodes
        .push(symbol_node(&callee_key(), "calleeFn", CALLEE_PATH));
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);

    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

// ── RECON-M-R3b: the reference-tier fixture ─────────────────────────────────────────────────

pub(crate) const REFS_PATH: &str = "src/refs.ts";
pub(crate) const OUT_PATH: &str = "src/out.ts";
/// Distinct incoming referrers of `calleeFn` — > `REFERENCE_TIER_BUDGET` (25) so the tier
/// truncates with a NAMED count (the M-R3b gate's fixture-scale bound stands in for amodx's 456).
pub(crate) const REFERENCE_TIER_INCOMING: usize = 30;

pub(crate) fn ref_source_key(i: usize) -> String {
    format!("{REPO}:{REFS_PATH}#ref{i}:SYMBOL:FUNCTION")
}
pub(crate) fn ref_out_key(name: &str) -> String {
    format!("{REPO}:{OUT_PATH}#{name}:SYMBOL:FUNCTION")
}

/// An `EdgeType::References` IR edge (`src` references `dst`) — the SCIP semantic overlay's
/// non-`Calls` reference kind (basis `DerivedReference`); the M-R3b reference-tier substrate.
fn reference_edge(src: &str, dst: &str) -> IrEdge {
    IrEdge {
        src: CanonicalKey::from_existing(src.to_string()),
        dst: CanonicalKey::from_existing(dst.to_string()),
        edge_type: EdgeType::References,
        basis: EdgeBasis::DerivedReference,
        provenance: prov(),
        import: None,
    }
}

/// RECON-M-R3b: the reference-tier fixture. A faithful CALL mirror (so `callgraph_is_green` warms
/// a MEASURED ledger) whose S IR additionally carries `EdgeType::References` edges:
/// - `REFERENCE_TIER_INCOMING` (30) DISTINCT `ref{i}` symbols each referencing `calleeFn`
///   (INCOMING > the budget 25 → the tier truncates with a NAMED count);
/// - TWO outgoing references `calleeFn -> {outA, outB}` (the callees-direction population);
/// - a SELF-reference `calleeFn -> calleeFn` (EXCLUDED — the ledger's g2u convention);
/// - a COLLISION referrer: a `ScipSynthesizedFallback` node whose key BYTE-EQUALS the pipeline's
///   `callerFn` key, referencing `calleeFn` — WITHHELD by §3.5 guard 2 (so the incoming total
///   stays 30, not 31: proof the guard excludes it).
///
/// The `References` edges make the kind-BLIND callgraph cert RED (§3.4 — expected; the reference
/// tier is a W-BOTH read surface that renders on the MEASURED ledger regardless of verdict).
pub(crate) fn build_reference_tier_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror_with_calls(dir.path(), 1);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");

    let mut ir = build_ir_with_calls(1);
    ir.nodes.push(file_node(REFS_PATH));
    ir.nodes.push(file_node(OUT_PATH));
    // 30 distinct incoming referrers of calleeFn.
    for i in 0..REFERENCE_TIER_INCOMING {
        let key = ref_source_key(i);
        ir.nodes
            .push(symbol_node(&key, &format!("ref{i}"), REFS_PATH));
        ir.edges.push(reference_edge(&key, &callee_key()));
    }
    // Two outgoing references from calleeFn.
    for name in ["outA", "outB"] {
        let key = ref_out_key(name);
        ir.nodes.push(symbol_node(&key, name, OUT_PATH));
        ir.edges.push(reference_edge(&callee_key(), &key));
    }
    // Self-reference — excluded from both directions.
    ir.edges.push(reference_edge(&callee_key(), &callee_key()));
    // Collision referrer: a fallback identity whose key byte-equals the pipeline's callerFn key
    // (mixed sources under one key → the §3.5 guard-2 collision), referencing calleeFn.
    let mut collide = symbol_node(&caller_key(), "callerFn", CALLER_PATH);
    collide.identity_source = IdentitySource::ScipSynthesizedFallback;
    collide.attributes = None;
    ir.nodes.push(collide);
    ir.edges.push(reference_edge(&caller_key(), &callee_key()));

    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R3b (R-1 scoping): [`build_reference_tier_fixture`] plus a SECOND, STALE TS partition
/// `q` that ALSO references `calleeFn`. `q` is not W-BOTH-eligible (¬Fresh), so the ledger's
/// `eligible` set excludes it and the reference tier must NOT count its reference — the covered-
/// partition scoping the M-R3b gate's R-1 requires (a mixed repo surfaces only its covered part).
pub(crate) const STALE_PARTITION: &str = "q";
pub(crate) const STALE_PATH: &str = "q/stale.ts";
pub(crate) fn stale_ref_key() -> String {
    format!("{REPO}:{STALE_PATH}#staleRef:SYMBOL:FUNCTION")
}
pub(crate) fn build_reference_tier_mixed_fixture() -> Fixture {
    let f = build_reference_tier_fixture();
    {
        let mut guard = f.state.livegraph.write();
        let lg = guard.as_mut().unwrap();
        let mut ir = PartitionIr::new(partition_with_id(STALE_PARTITION));
        ir.nodes.push(file_node_in(STALE_PATH, STALE_PARTITION));
        ir.nodes.push(symbol_node_in(
            &stale_ref_key(),
            "staleRef",
            STALE_PATH,
            STALE_PARTITION,
        ));
        ir.edges
            .push(reference_edge(&stale_ref_key(), &callee_key()));
        lg.load_partition(STALE_PARTITION, ir, LanguageSupport::TypeScriptPrimary);
        lg.mark_stale(STALE_PARTITION);
    }
    f
}

/// RECON-M-R1 (the §3.5 guard-3 `identity_suspect` fixture): P resolves `callerFn`'s call to
/// `target` in `src/b.ts`; the compiler resolves a SAME-NAMED call from the SAME caller to a
/// DIFFERENT key (`target` in `lib/c.ts`) — the wrong/missed-adoption symptom signature. The P
/// pair classifies `syntactic` (S holds no call to P's key), the S pair `semantic`/`new_pair`,
/// and their (caller key, callee NAME) match under different callee keys must fire the detector.
pub(crate) fn build_suspect_fixture() -> Fixture {
    build_suspect_fixture_impl(None, false)
}

/// RECON-M-R4 (§5.5 review-1 #1): the suspect fixture PLUS a SECOND same-named `target` the
/// compiler resolved (`lib/d.ts#target`, a different key again) → TWO same-named candidates for
/// (callerFn, "target"). The §5.5 ambiguity guard REFUSES the contested signal (`contested`
/// empty — never a pick among ≥ 2), while `identity_suspect` STILL fires (its value is unchanged).
/// Proves "one syntactic target + two same-named semantic targets → no contested target selected".
pub(crate) fn build_contested_ambiguous_fixture() -> Fixture {
    let key2 = format!("{REPO}:lib/d.ts#target:SYMBOL:FUNCTION");
    build_suspect_fixture_impl(Some((&key2, "lib/d.ts")), false)
}

/// RECON-M-R4 (review-2 #2): the suspect shape where the compiler's competitor is a
/// `semantic`/`MULTIPLICITY` pair, not a `new_pair` — P ALSO resolved one call to
/// `lib/c.ts#target` (p = 1) while the compiler witnessed it TWICE (s = 2). The S-excess
/// instance is compiler-only evidence, so the pair MUST candidate in the Layer-2 index and the
/// contested join must see it (syntax `src/b.ts#target` vs compiler `lib/c.ts#target`).
pub(crate) fn build_contested_multiplicity_fixture() -> Fixture {
    build_suspect_fixture_impl(None, true)
}

/// RECON-M-R4 (review-2 #2): candidates SPANNING the sub-classes — `lib/c.ts#target` as
/// `semantic`/`multiplicity` (p = 1, s = 2) AND `lib/d.ts#target` as `semantic`/`new_pair`
/// (p = 0, s = 1), both named `target` in `callerFn` → the `(callerFn, "target")` lookup holds
/// TWO candidates → the §5.5 ambiguity guard refuses BOTH joins (contested empty; an unresolved
/// site of that head is counted ambiguous, never annotated).
pub(crate) fn build_layer2_cross_subclass_ambiguous_fixture() -> Fixture {
    let key2 = format!("{REPO}:lib/d.ts#target:SYMBOL:FUNCTION");
    build_suspect_fixture_impl(Some((&key2, "lib/d.ts")), true)
}

/// The suspect fixture, parameterized on an optional SECOND same-named semantic target
/// `(key, file)` — `None` = the single-candidate case; `Some` = a `new_pair` extra candidate —
/// and on `corroborate_first`: when true, P ALSO holds ONE call to the first semantic target
/// (`lib/c.ts#target`) while S holds TWO, turning that pair `semantic`/`multiplicity`
/// (s = 2 > p = 1) instead of `new_pair` (review-2 #1: both sub-classes are Layer-2 candidates).
/// Four concrete callers; axes = compiler-candidate multiplicity × first-candidate sub-class.
fn build_suspect_fixture_impl(
    second_semantic: Option<(&str, &str)>,
    corroborate_first: bool,
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    // P side: the standard mirror (callerFn -> calleeFn CALLS, calleeFn named "calleeFn"…) is not
    // name-matched here; build a custom mirror whose CALLS target is a symbol NAMED `target`.
    let db_path = dir.path().join("repo.db");
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
    let p_target_key = format!("{REPO}:{CALLEE_PATH}#target:SYMBOL:FUNCTION");
    let s_target_key = format!("{REPO}:{LIB_PATH}#target:SYMBOL:FUNCTION");
    let mut p_symbols: Vec<(String, String)> = vec![
        ("ns0".into(), caller_key()),
        ("ns1".into(), p_target_key.clone()),
    ];
    if corroborate_first {
        // The multiplicity variant: P resolved ONE call to the compiler's target too (p = 1).
        p_symbols.push(("ns2".into(), s_target_key.clone()));
    }
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (uid, key) in &p_symbols {
        let mut n = graph_node(uid, key, "SYMBOL");
        n.snapshot_uid = snapshot_uid.clone();
        n.name = if uid == "ns0" { "callerFn" } else { "target" }.into();
        nodes.push(n);
    }
    conn.insert_nodes(&nodes).expect("insert nodes");
    let p_calls_edge = |edge_uid: &str, target_node_uid: &str| GraphEdge {
        edge_uid: edge_uid.into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: REPO.into(),
        source_node_uid: "ns0".into(),
        target_node_uid: target_node_uid.into(),
        edge_type: "CALLS".into(),
        resolution: "resolved".into(),
        extractor: "test".into(),
        location: None,
        metadata_json: None,
    };
    let mut p_edges = vec![p_calls_edge("ec0", "ns1")];
    if corroborate_first {
        p_edges.push(p_calls_edge("ec1", "ns2"));
    }
    conn.insert_edges(&p_edges).expect("insert edges");
    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    // S side: callerFn calls a same-NAMED symbol under a DIFFERENT key (lib/c.ts#target).
    let mut ir = PartitionIr::new(partition());
    ir.nodes.push(file_node(CALLER_PATH));
    ir.nodes.push(file_node(LIB_PATH));
    ir.nodes
        .push(symbol_node(&caller_key(), "callerFn", CALLER_PATH));
    ir.nodes
        .push(symbol_node(&s_target_key, "target", LIB_PATH));
    ir.edges.push(calls_edge(&caller_key(), &s_target_key));
    if corroborate_first {
        // The S-EXCESS instance: the compiler witnessed the call TWICE (s = 2 > p = 1) — the
        // pair classifies `semantic`/`multiplicity`, and the excess is compiler-only evidence.
        ir.edges.push(calls_edge(&caller_key(), &s_target_key));
    }
    // The ambiguity variant: a SECOND same-named `target` the compiler resolved (new_pair — P
    // holds no call to it) → the (callerFn, "target") lookup now has TWO candidates.
    if let Some((key2, path2)) = second_semantic {
        ir.nodes.push(symbol_node(key2, "target", path2));
        ir.edges.push(calls_edge(&caller_key(), key2));
    }
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);

    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}

/// RECON-M-R4 (§5.5 case 1) key: a SCIP-only `cn` the pipeline leaves unresolved.
pub(crate) fn cn_key() -> String {
    format!("{REPO}:src/utils.ts#cn:SYMBOL:FUNCTION")
}
/// A SECOND same-named `cn` in a different file — the AMBIGUITY variant's extra candidate.
pub(crate) fn cn2_key() -> String {
    format!("{REPO}:lib/other.ts#cn:SYMBOL:FUNCTION")
}

/// An S strict-`Calls` IR edge `src_key -> dst_key` (`SyntaxConfirmedCall`).
fn calls_edge(src_key: &str, dst_key: &str) -> IrEdge {
    IrEdge {
        src: CanonicalKey::from_existing(src_key.to_string()),
        dst: CanonicalKey::from_existing(dst_key.to_string()),
        edge_type: EdgeType::Calls,
        basis: EdgeBasis::SyntaxConfirmedCall,
        provenance: prov(),
        import: None,
    }
}

/// RECON-M-R4 (§5.5 case 1, the LAYER-2 landing fixture): the faithful `callerFn -> calleeFn`
/// call (corroborated `both`) PLUS a SCIP-ONLY `callerFn -> cn` call the pipeline lacks — a
/// `semantic`/`new_pair` named `cn`, indexed by `(callerFn, "cn")`. A hand-built unresolved site
/// `(callerFn, "cn")` then joins to it → "likely resolves to cn". With `ambiguous`, a SECOND
/// same-named `cn` (another file) makes the `(callerFn, "cn")` lookup AMBIGUOUS → the join refuses.
pub(crate) fn build_layer2_fixture(ambiguous: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror(dir.path(), false);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    let mut ir = build_ir();
    ir.nodes.push(symbol_node(&cn_key(), "cn", "src/utils.ts"));
    ir.edges.push(calls_edge(&caller_key(), &cn_key()));
    if ambiguous {
        ir.nodes.push(symbol_node(&cn2_key(), "cn", "lib/other.ts"));
        ir.edges.push(calls_edge(&caller_key(), &cn2_key()));
    }
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, LanguageSupport::TypeScriptPrimary);
    *state.livegraph.write() = Some(lg);
    Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    }
}
