//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: shared cert-test fixtures (split from `tests` per the 500-line
//! guardrail, review-1 pt5). Builds a resident LiveGraph and a faithful SQLite mirror from the SAME
//! explicit facts, so a GREEN cert proves the resolver's key-parse + module derivation reproduce
//! those facts and SQLite holds them too. The builders are parameterized over the file/symbol facts
//! and the partition language so the tests can construct the default fixture, a non-TS fixture, and
//! an ambiguity fixture from one place.

use repo_graph_ir::{
    CanonicalKey, IdentitySource, IrNode, IrVisibility, Partition, PartitionId, PartitionIr,
    PartitionKind, Provenance, SourceRange, SymbolAttributes,
};
use repo_graph_livegraph::LiveGraph;
use repo_graph_storage::types::{
    CreateSnapshotInput, GraphEdge, GraphNode, Repo, SourceLocation, TrackedFile,
    UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::LanguageSupport;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::livegraph_feed::import_cert_fingerprint;
use crate::state::RepoState;

pub(crate) const REPO: &str = "repo_focus_cert";

/// One symbol the fixture declares — shared by the IR builder and the SQLite mirror so the two are
/// built from the SAME explicit facts (no re-derivation).
pub(crate) struct Sym {
    pub path: String,
    /// the key's `#…` segment (= qualified_name; differs from `name` for a method).
    pub qname: String,
    pub name: String,
    pub kind: String,
    pub line: u32,
}

/// Ergonomic `Sym` constructor.
pub(crate) fn sym(path: &str, qname: &str, name: &str, kind: &str, line: u32) -> Sym {
    Sym {
        path: path.into(),
        qname: qname.into(),
        name: name.into(),
        kind: kind.into(),
        line,
    }
}

/// The default fixture file inventory: two nested files + a repo-root file.
pub(crate) fn default_files() -> Vec<String> {
    vec!["src/a.ts".into(), "src/util/b.ts".into(), "main.ts".into()]
}

/// The default fixture symbols: a duplicate name (`foo` in two files), a method (`Widget.render`),
/// and a repo-root symbol (`boot`).
pub(crate) fn default_symbols() -> Vec<Sym> {
    vec![
        sym("src/a.ts", "foo", "foo", "FUNCTION", 3),
        sym("src/a.ts", "Widget", "Widget", "CLASS", 10),
        sym("src/a.ts", "Widget.render", "render", "METHOD", 12),
        sym("src/util/b.ts", "foo", "foo", "FUNCTION", 1),
        sym("main.ts", "boot", "boot", "FUNCTION", 1),
    ]
}

/// SQLite-ONLY nodes to inject into the mirror so the parity cert sees identities the LiveGraph
/// lacks — the SQLite-extra RED cases (review-2 pt1). Each is written to `nodes` (+ a `files` row for
/// extra files) with NO LiveGraph counterpart, so the resolver MUST miss it -> the cert MUST go RED.
#[derive(Default)]
pub(crate) struct MirrorExtras {
    /// Extra FILE node paths present in SQLite only (a FILE the LiveGraph inventory does not carry).
    pub extra_files: Vec<String>,
    /// Extra directory-MODULE dirs present in SQLite only (a MODULE the LiveGraph cannot derive).
    pub extra_modules: Vec<String>,
    /// Extra SYMBOL nodes present in SQLite only (each in an EXISTING file so the file join holds).
    pub extra_symbols: Vec<Sym>,
}

fn prov() -> Provenance {
    Provenance {
        indexer: "scip-typescript".into(),
        indexer_version: "0.4.0".into(),
        scip_symbol_id: None,
        build_inputs_hash: "h".into(),
    }
}

pub(crate) fn file_key(path: &str) -> String {
    format!("{REPO}:{path}:FILE")
}
pub(crate) fn symbol_key(s: &Sym) -> String {
    format!("{REPO}:{}#{}:SYMBOL:{}", s.path, s.qname, s.kind)
}
pub(crate) fn module_key(dir: &str) -> String {
    format!("{REPO}:{dir}:MODULE")
}
fn file_uid(path: &str) -> String {
    format!("fuid::{path}")
}
fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// The ancestor-directory set over the file list (the SQLite directory-MODULE materializer walk).
pub(crate) fn module_dirs(files: &[String]) -> BTreeSet<String> {
    let mut dirs = BTreeSet::new();
    for f in files {
        let mut p = f.as_str();
        while let Some(pos) = p.rfind('/') {
            let dir = &p[..pos];
            if !dirs.insert(dir.to_string()) {
                break;
            }
            p = dir;
        }
    }
    dirs
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
        declared_dependencies: BTreeSet::new(),
        tsconfig_aliases: None,
    }
}

/// Build the resident PartitionIr from explicit file + symbol facts.
pub(crate) fn build_ir(files: &[String], symbols: &[Sym]) -> PartitionIr {
    let mut ir = PartitionIr::new(partition());
    for path in files {
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing(file_key(path)),
            subtype: "File".into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            range: None,
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::AstFileScope,
            provenance: prov(),
            attributes: None,
        });
    }
    for s in symbols {
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing(symbol_key(s)),
            subtype: "Term".into(),
            name: s.name.clone(),
            range: Some(SourceRange {
                file: s.path.clone(),
                start_line: s.line,
                start_col: 0,
                end_line: s.line,
                end_col: 0,
            }),
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::AstAdopted,
            provenance: prov(),
            attributes: Some(SymbolAttributes {
                visibility: Some(IrVisibility::Export),
                is_top_level: true,
                symbol_kind: Some(s.kind.clone()),
            }),
        });
    }
    ir
}

/// Build a resident LiveGraph from explicit facts + a partition language.
pub(crate) fn build_livegraph(
    files: &[String],
    symbols: &[Sym],
    language: LanguageSupport,
) -> LiveGraph {
    let mut lg = LiveGraph::new();
    lg.load_partition("p", build_ir(files, symbols), language);
    lg
}

/// The default resident LiveGraph (TS, default facts).
pub(crate) fn build_default_livegraph() -> LiveGraph {
    build_livegraph(
        &default_files(),
        &default_symbols(),
        LanguageSupport::TypeScriptPrimary,
    )
}

/// The default PartitionIr — for tests that RELOAD the partition to bump the epoch (and so the
/// fingerprint) to exercise cert invalidation.
pub(crate) fn build_default_ir() -> PartitionIr {
    build_ir(&default_files(), &default_symbols())
}

/// Build a resident LiveGraph whose `fallback_syms` are loaded as `ScipSynthesizedFallback` nodes
/// (NOT `AstAdopted`). The resolver matches AST-adopted symbols ONLY, so a fallback node is resident
/// yet UNRESOLVABLE — when SQLite carries the SAME key as a normal SYMBOL, `resolve_stable_key`
/// returns None on the LiveGraph but Some on SQLite -> the cert MUST go RED (review-2: the
/// fallback-symbol case; spec §7c L2). The fallback nodes are excluded from `focus_corpus` (it scans
/// AST-adopted only), so they bite parity solely via the SQLite-side enumeration.
pub(crate) fn build_livegraph_with_fallback(
    files: &[String],
    symbols: &[Sym],
    fallback_syms: &[Sym],
    language: LanguageSupport,
) -> LiveGraph {
    let mut ir = build_ir(files, symbols);
    for s in fallback_syms {
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing(symbol_key(s)),
            subtype: "Term".into(),
            name: s.name.clone(),
            range: Some(SourceRange {
                file: s.path.clone(),
                start_line: s.line,
                start_col: 0,
                end_line: s.line,
                end_col: 0,
            }),
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::ScipSynthesizedFallback,
            provenance: prov(),
            attributes: None,
        });
    }
    let mut lg = LiveGraph::new();
    lg.load_partition("p", ir, language);
    lg
}

fn node(uid: &str, stable_key: &str, kind: &str) -> GraphNode {
    GraphNode {
        node_uid: uid.into(),
        snapshot_uid: String::new(), // filled by caller
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

/// Build a SQLite db whose `nodes`/`files`/`edges` faithfully mirror the explicit facts (FILE +
/// SYMBOL + directory MODULE nodes, OWNS edges, files rows). `drop_symbol`, when set, omits that
/// symbol key (an LG-extra RED divergence). `extras` injects SQLite-ONLY nodes (FILE / MODULE /
/// SYMBOL the LiveGraph lacks) to force the SQLite-extra RED divergences (review-2). Returns
/// `(db_path, snapshot_uid)`.
pub(crate) fn build_sqlite_mirror_ex(
    dir: &Path,
    files: &[String],
    symbols: &[Sym],
    drop_symbol: Option<&str>,
    extras: &MirrorExtras,
) -> (PathBuf, String) {
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

    // files rows (the public upsert API; `connection()` is storage-private). Real files + any
    // SQLite-only extra files share the same row shape so `resolve_path_focus` can join them.
    let tracked: Vec<TrackedFile> = files
        .iter()
        .chain(extras.extra_files.iter())
        .map(|path| TrackedFile {
            file_uid: file_uid(path),
            repo_uid: REPO.into(),
            path: path.clone(),
            language: Some("typescript".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        })
        .collect();
    conn.upsert_files(&tracked).expect("upsert files");

    // FILE nodes.
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut node_uid_of: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (i, path) in files.iter().enumerate() {
        let uid = format!("nf{i}");
        let mut n = node(&uid, &file_key(path), "FILE");
        n.snapshot_uid = snapshot_uid.clone();
        n.file_uid = Some(file_uid(path));
        node_uid_of.insert(file_key(path), uid);
        nodes.push(n);
    }
    // directory MODULE nodes.
    for (i, d) in module_dirs(files).iter().enumerate() {
        let uid = format!("nm{i}");
        let mut n = node(&uid, &module_key(d), "MODULE");
        n.snapshot_uid = snapshot_uid.clone();
        n.qualified_name = Some(d.clone()); // resolve_path matches MODULE by qualified_name
        node_uid_of.insert(module_key(d), uid);
        nodes.push(n);
    }
    // SYMBOL nodes.
    for (i, s) in symbols.iter().enumerate() {
        let key = symbol_key(s);
        if drop_symbol == Some(key.as_str()) {
            continue;
        }
        let uid = format!("ns{i}");
        let mut n = node(&uid, &key, "SYMBOL");
        n.snapshot_uid = snapshot_uid.clone();
        n.name = s.name.clone();
        n.qualified_name = Some(s.qname.clone());
        n.subtype = Some(s.kind.clone());
        n.file_uid = Some(file_uid(&s.path));
        n.location = Some(SourceLocation {
            line_start: s.line as i64,
            col_start: 0,
            line_end: s.line as i64,
            col_end: 0,
        });
        nodes.push(n);
    }
    // SQLite-ONLY extras (review-2: the SQLite-extra RED cases). No LiveGraph counterpart, so the
    // resolver misses them -> the cert MUST go RED. Distinct uid prefixes avoid collisions.
    for (i, path) in extras.extra_files.iter().enumerate() {
        let uid = format!("exf{i}");
        let mut n = node(&uid, &file_key(path), "FILE");
        n.snapshot_uid = snapshot_uid.clone();
        n.file_uid = Some(file_uid(path));
        node_uid_of.insert(file_key(path), uid);
        nodes.push(n);
    }
    for (i, d) in extras.extra_modules.iter().enumerate() {
        let uid = format!("exm{i}");
        let mut n = node(&uid, &module_key(d), "MODULE");
        n.snapshot_uid = snapshot_uid.clone();
        n.qualified_name = Some(d.clone());
        node_uid_of.insert(module_key(d), uid);
        nodes.push(n);
    }
    for (i, s) in extras.extra_symbols.iter().enumerate() {
        let uid = format!("exs{i}");
        let mut n = node(&uid, &symbol_key(s), "SYMBOL");
        n.snapshot_uid = snapshot_uid.clone();
        n.name = s.name.clone();
        n.qualified_name = Some(s.qname.clone());
        n.subtype = Some(s.kind.clone());
        n.file_uid = Some(file_uid(&s.path));
        n.location = Some(SourceLocation {
            line_start: s.line as i64,
            col_start: 0,
            line_end: s.line as i64,
            col_end: 0,
        });
        nodes.push(n);
    }
    conn.insert_nodes(&nodes).expect("insert nodes");

    // OWNS edges: immediate-parent MODULE -> FILE (the get_symbol_context module join).
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (i, path) in files.iter().enumerate() {
        let d = dirname(path);
        if d.is_empty() {
            continue;
        }
        let (Some(m_uid), Some(f_uid)) = (
            node_uid_of.get(&module_key(d)),
            node_uid_of.get(&file_key(path)),
        ) else {
            continue;
        };
        edges.push(GraphEdge {
            edge_uid: format!("eo{i}"),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: REPO.into(),
            source_node_uid: m_uid.clone(),
            target_node_uid: f_uid.clone(),
            edge_type: "OWNS".into(),
            resolution: "resolved".into(),
            extractor: "test".into(),
            location: None,
            metadata_json: None,
        });
    }
    if !edges.is_empty() {
        conn.insert_edges(&edges).expect("insert edges");
    }

    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    (db_path, snapshot_uid)
}

/// The faithful SQLite mirror with no SQLite-only extras (the common case).
pub(crate) fn build_sqlite_mirror(
    dir: &Path,
    files: &[String],
    symbols: &[Sym],
    drop_symbol: Option<&str>,
) -> (PathBuf, String) {
    build_sqlite_mirror_ex(dir, files, symbols, drop_symbol, &MirrorExtras::default())
}

/// The default faithful SQLite mirror (default facts).
pub(crate) fn build_default_mirror(dir: &Path, drop_symbol: Option<&str>) -> (PathBuf, String) {
    build_sqlite_mirror(dir, &default_files(), &default_symbols(), drop_symbol)
}

/// The current SHARED SQLite-free fingerprint for the resident LiveGraph.
pub(crate) fn live_fp(state: &RepoState, snapshot_uid: &str) -> String {
    let guard = state.livegraph.read();
    let lg = guard.as_ref().expect("livegraph set");
    import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
}
