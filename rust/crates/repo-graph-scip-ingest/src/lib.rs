#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! SCIP ingestion adapter for repo-graph (INGEST-CORE-1).
//!
//! Adapter boundary: may depend on volatile producers (`scip`, `ts-extractor`),
//! mapping SCIP facts into the repo-graph-owned `repo-graph-ir` domain model.
//!
//! - Step 2 (decode de-risk): [`decode_index`] / [`summarize`].
//! - Step 3 (join probe + residual diagnosis): adopt the AST canonical key per
//!   in-partition declaration via name + span containment; measure the
//!   declaration-kind definition-join rate and classify each unmatched
//!   Term/Method by cause.

use std::collections::{BTreeMap, HashMap, HashSet};

use protobuf::Message;
use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::types::{
    EdgeType as TsEdgeType, ImportObservation as TsImportObservation, NodeKind,
};
use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType as IrEdgeType, IdentitySource, ImportEdgeMeta,
    ImportObservation as IrImportObservation, ImportResolution, IrEdge, IrNode, Partition,
    PartitionId, PartitionIr, PartitionKind, Provenance, SourceRange, TsconfigAliasConfig,
};
use repo_graph_ts_extractor::TsExtractor;
use scip::types::Index;

// ── Step 2: decode de-risk ────────────────────────────────────────

/// Decode a SCIP index from protobuf bytes (rust-protobuf via the `scip` crate).
pub fn decode_index(bytes: &[u8]) -> Result<Index, protobuf::Error> {
    Index::parse_from_bytes(bytes)
}

/// Summary counts for a decoded SCIP index (confirmed against the Node reader).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Number of documents.
    pub documents: usize,
    /// Total occurrences.
    pub occurrences: usize,
    /// Occurrences with the `Definition` role.
    pub definitions: usize,
    /// Occurrences without the `Definition` role.
    pub references: usize,
    /// External symbols.
    pub external_symbols: usize,
}

/// Summarize a decoded index.
pub fn summarize(index: &Index) -> Summary {
    let mut s = Summary {
        external_symbols: index.external_symbols.len(),
        ..Summary::default()
    };
    for doc in &index.documents {
        s.documents += 1;
        for occ in &doc.occurrences {
            s.occurrences += 1;
            if occ.symbol_roles & 0x1 != 0 {
                s.definitions += 1;
            } else {
                s.references += 1;
            }
        }
    }
    s
}

// ── Step 3: AST <-> SCIP definition-join probe + diagnosis ────────

/// A SCIP definition occurrence reduced for the join.
#[derive(Debug, Clone)]
pub struct ScipDef {
    /// Full SCIP symbol string (provenance).
    pub symbol: String,
    /// Terminal descriptor name (for the join).
    pub name: String,
    /// Terminal descriptor suffix label (Method / Type / Term / ...).
    pub kind: String,
    /// Enclosing descriptor suffix label (the context: Type / Namespace / Method / Root).
    pub enclosing_kind: String,
    /// Whether this is a SCIP `local` symbol.
    pub is_local: bool,
    /// Name-token start line, SCIP 0-based.
    pub start_line0: i32,
    /// Name-token start character, SCIP 0-based.
    pub start_char0: i32,
}

/// Extract definition occurrences (Definition role) from one SCIP document.
pub fn scip_definitions(doc: &scip::types::Document) -> Vec<ScipDef> {
    let mut out = Vec::new();
    for occ in &doc.occurrences {
        if occ.symbol_roles & 0x1 == 0 || occ.range.len() < 2 {
            continue;
        }
        let (name, kind, enclosing_kind) = descriptors_info(&occ.symbol);
        out.push(ScipDef {
            symbol: occ.symbol.clone(),
            name,
            kind,
            enclosing_kind,
            is_local: scip::symbol::is_local_symbol(&occ.symbol),
            start_line0: occ.range[0],
            start_char0: occ.range[1],
        });
    }
    out
}

/// Map each documented symbol string to its SCIP `SymbolInformation.kind` label
/// (diagnostic only — used to classify unmatched declarations for the follow-on).
pub fn symbol_kinds(doc: &scip::types::Document) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for si in &doc.symbols {
        let kind = match si.kind.enum_value() {
            Ok(k) => format!("{k:?}"),
            Err(_) => "UnknownKind".to_string(),
        };
        m.insert(si.symbol.clone(), kind);
    }
    m
}

/// (terminal name, terminal kind, enclosing kind) for a SCIP symbol.
fn descriptors_info(symbol: &str) -> (String, String, String) {
    if scip::symbol::is_local_symbol(symbol) {
        return (String::new(), "Local".to_string(), "Local".to_string());
    }
    let kind_of = |d: &scip::types::Descriptor| match d.suffix.enum_value() {
        Ok(s) => format!("{s:?}"),
        Err(_) => "Unknown".to_string(),
    };
    match scip::symbol::parse_symbol(symbol) {
        Ok(sym) => {
            let ds = &sym.descriptors;
            let (name, kind) = match ds.last() {
                Some(d) => (d.name.clone(), kind_of(d)),
                None => (String::new(), "NoDescriptor".to_string()),
            };
            let enclosing = if ds.len() >= 2 {
                kind_of(&ds[ds.len() - 2])
            } else {
                "Root".to_string()
            };
            (name, kind, enclosing)
        }
        Err(_) => (
            String::new(),
            "ParseErr".to_string(),
            "ParseErr".to_string(),
        ),
    }
}

/// An AST node reduced to identity + span. Span: lines 1-based, columns 0-based.
#[derive(Debug, Clone)]
pub struct AstNodeLite {
    /// Canonical stable key emitted by `ts-extractor`.
    pub stable_key: String,
    /// Symbol name.
    pub name: String,
    /// Cyclomatic complexity, if this node is a function/method (from ts-extractor).
    pub cyclomatic: Option<u32>,
    /// True when this is the file/module-scope source node (`NodeKind::File`). Such a
    /// node is a non-callable enclosing scope: any edge it sources is a
    /// `FileScopeReference`, never a `Calls`.
    pub is_file_scope: bool,
    /// Span start line (1-based).
    pub line_start: i64,
    /// Span start column (0-based).
    pub col_start: i64,
    /// Span end line (1-based).
    pub line_end: i64,
    /// Span end column (0-based).
    pub col_end: i64,
}

/// Run `ts-extractor` on one TypeScript source file; return its located nodes.
pub fn ast_nodes_for_source(source: &str, file_path: &str, repo_uid: &str) -> Vec<AstNodeLite> {
    let mut extractor = TsExtractor::new();
    if extractor.initialize().is_err() {
        return Vec::new();
    }
    let file_uid = format!("{repo_uid}:{file_path}");
    let result = match extractor.extract(source, file_path, &file_uid, repo_uid, "ingest-core-1") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let metrics = result.metrics;
    result
        .nodes
        .into_iter()
        .filter_map(|n| {
            let cyclomatic = metrics.get(&n.stable_key).map(|m| m.cyclomatic_complexity);
            let is_file_scope = matches!(n.kind, NodeKind::File);
            n.location.map(|loc| AstNodeLite {
                stable_key: n.stable_key,
                name: n.name,
                cyclomatic,
                is_file_scope,
                line_start: loc.line_start,
                col_start: loc.col_start,
                line_end: loc.line_end,
                col_end: loc.col_end,
            })
        })
        .collect()
}

/// Narrow, deterministic reconciliation of SCIP member-name markers to the bare
/// identifier `ts-extractor` uses for the same construct. Only the proven divergent
/// forms are normalized — `<constructor>`, `<get>X`, `<set>X` — observed in
/// scip-typescript output; every other name passes through unchanged. This is NOT
/// fuzzy matching: it is a fixed map over a closed set of compiler-emitted markers.
/// Span containment (in `find_match`) still disambiguates (e.g. a field `id` from a
/// getter `id`).
pub fn reconcile_scip_name(name: &str) -> &str {
    if name == "<constructor>" {
        "constructor"
    } else if let Some(rest) = name.strip_prefix("<get>") {
        rest
    } else if let Some(rest) = name.strip_prefix("<set>") {
        rest
    } else {
        name
    }
}

/// The AST node a global definition adopts: innermost (reconciled name + span
/// containment of the name-token start) match, or `None`.
pub fn find_match<'a>(d: &ScipDef, nodes: &'a [AstNodeLite]) -> Option<&'a AstNodeLite> {
    let name_line = d.start_line0 as i64 + 1; // SCIP 0-based -> ts 1-based
    let name_col = d.start_char0 as i64; // both 0-based
    let target = reconcile_scip_name(&d.name);
    nodes
        .iter()
        .filter(|n| !n.name.is_empty() && n.name == target)
        .filter(|n| contains_point(n, name_line, name_col))
        .min_by_key(|n| span_size(n))
}

/// True if a global definition adopts an AST node.
pub fn match_def(d: &ScipDef, nodes: &[AstNodeLite]) -> bool {
    find_match(d, nodes).is_some()
}

/// Classify an unmatched declaration definition into one of the five causes.
pub fn diagnose_unmatched(d: &ScipDef, file_path: &str, nodes: &[AstNodeLite]) -> &'static str {
    let name_line = d.start_line0 as i64 + 1;
    let name_col = d.start_char0 as i64;
    let same_name: Vec<&AstNodeLite> = nodes
        .iter()
        .filter(|n| !n.name.is_empty() && n.name == reconcile_scip_name(&d.name))
        .collect();
    if same_name
        .iter()
        .any(|n| contains_point(n, name_line, name_col))
    {
        return "join_bug"; // a same-name node contains the position -> join should have matched
    }
    // True coordinate/encoding mismatch: a same-name node spans the def's LINE
    // (plausibly the same symbol) but the point isn't contained -> columns differ.
    // A same-name node on a DIFFERENT line is a different declaration ts-extractor
    // didn't emit for this def, not a coordinate bug.
    if same_name
        .iter()
        .any(|n| name_line >= n.line_start && name_line <= n.line_end)
    {
        return "coordinate_path_mismatch";
    }
    if file_path.ends_with(".d.ts") {
        return "ambient_or_generated";
    }
    match d.enclosing_kind.as_str() {
        // Nested inside a function/method -> a local construct repo-graph doesn't model.
        "Method" => "scip_only_non_modeled",
        // Member of a Type, or module-level -> something ts-extractor arguably should emit.
        _ => "ts_extractor_coverage_gap",
    }
}

fn contains_point(n: &AstNodeLite, line: i64, col: i64) -> bool {
    let after_start = line > n.line_start || (line == n.line_start && col >= n.col_start);
    let before_end = line < n.line_end || (line == n.line_end && col <= n.col_end);
    after_start && before_end
}

fn span_size(n: &AstNodeLite) -> i64 {
    (n.line_end - n.line_start) * 100_000 + (n.col_end - n.col_start)
}

// ── Step 4: build PartitionIr nodes ───────────────────────────────

/// Output of building one partition's IR declaration nodes.
#[derive(Debug, Default)]
pub struct PartitionBuild {
    /// IR nodes: matched defs adopt the AST key; unmatched defs are labeled fallback.
    pub nodes: Vec<IrNode>,
    /// One value-fact kind (cyclomatic complexity), attached by canonical key.
    pub complexity: BTreeMap<String, u32>,
    /// Count of nodes whose identity was adopted from the AST.
    pub matched: usize,
    /// Count of matched defs recovered via narrow SCIP-name reconciliation
    /// (`<constructor>`/`<get>`/`<set>`) — constructor/getter recovery (subset of matched).
    pub reconciled: usize,
    /// Count of nodes whose identity was synthesized as labeled fallback.
    pub fallback: usize,
    /// Count of materialized file/module-scope (FILE) nodes (`AstFileScope`).
    pub file_scope: usize,
}

/// Build IR declaration nodes for one document. Matched defs adopt the AST canonical
/// key (and attach complexity); unmatched defs get a labeled `ScipSynthesizedFallback`
/// key synthesized from SCIP info. Locals and non-declaration kinds are skipped.
/// Additionally materializes the file/module-scope FILE node (`AstFileScope`) so
/// file-scope reference edges have a node-backed source (no dangling edge endpoints).
#[allow(clippy::too_many_arguments)]
pub fn build_partition_nodes(
    doc: &scip::types::Document,
    key_path: &str,
    ast_nodes: &[AstNodeLite],
    repo_uid: &str,
    partition_id: &str,
    indexer: &str,
    indexer_version: &str,
    build_inputs_hash: &str,
) -> PartitionBuild {
    let decl = ["Namespace", "Type", "Method", "Term"];
    let pid = PartitionId::new(partition_id);
    let mut out = PartitionBuild::default();

    for d in scip_definitions(doc) {
        if d.is_local || !decl.contains(&d.kind.as_str()) {
            continue;
        }
        let provenance = Provenance {
            indexer: indexer.to_string(),
            indexer_version: indexer_version.to_string(),
            scip_symbol_id: Some(d.symbol.clone()),
            build_inputs_hash: build_inputs_hash.to_string(),
        };
        match find_match(&d, ast_nodes) {
            Some(ast) => {
                if let Some(c) = ast.cyclomatic {
                    out.complexity.insert(ast.stable_key.clone(), c);
                }
                out.nodes.push(IrNode {
                    key: CanonicalKey::from_existing(ast.stable_key.clone()),
                    subtype: d.kind.clone(),
                    name: d.name.clone(),
                    range: Some(SourceRange {
                        file: key_path.to_string(),
                        start_line: ast.line_start.max(0) as u32,
                        start_col: ast.col_start.max(0) as u32,
                        end_line: ast.line_end.max(0) as u32,
                        end_col: ast.col_end.max(0) as u32,
                    }),
                    partition_id: pid.clone(),
                    identity_source: IdentitySource::AstAdopted,
                    provenance,
                });
                out.matched += 1;
                if d.name.starts_with('<') {
                    out.reconciled += 1;
                }
            }
            None => {
                // Labeled fallback: synthesize a canonical-format key from SCIP info. Uses the
                // REPO-RELATIVE key_path so it shares the producer's namespace (KEY-NAMESPACE-REPO-RELATIVE-1).
                let synth = format!("{repo_uid}:{key_path}#{}:SYMBOL:{}", d.name, d.kind);
                out.nodes.push(IrNode {
                    key: CanonicalKey::from_existing(synth),
                    subtype: d.kind.clone(),
                    name: d.name.clone(),
                    range: None,
                    partition_id: pid.clone(),
                    identity_source: IdentitySource::ScipSynthesizedFallback,
                    provenance,
                });
                out.fallback += 1;
            }
        }
    }

    // Materialize the file/module-scope node (ts-extractor FILE node) so file-scope
    // reference edges have a node-backed source (no dangling endpoints). It carries no
    // SCIP symbol; its identity is AST-structural.
    for n in ast_nodes.iter().filter(|n| n.is_file_scope) {
        out.nodes.push(IrNode {
            key: CanonicalKey::from_existing(n.stable_key.clone()),
            subtype: "FileScope".to_string(),
            name: n.name.clone(),
            range: Some(SourceRange {
                file: key_path.to_string(),
                start_line: n.line_start.max(0) as u32,
                start_col: n.col_start.max(0) as u32,
                end_line: n.line_end.max(0) as u32,
                end_col: n.col_end.max(0) as u32,
            }),
            partition_id: pid.clone(),
            identity_source: IdentitySource::AstFileScope,
            provenance: Provenance {
                indexer: indexer.to_string(),
                indexer_version: indexer_version.to_string(),
                scip_symbol_id: None,
                build_inputs_hash: build_inputs_hash.to_string(),
            },
        });
        out.file_scope += 1;
    }
    out
}

// ── Step 4b: derive Calls / References (strict) ───────────────────

/// A call-expression site: `(callee_name, (line_start, col_start, line_end, col_end))`.
/// Lines 1-based, columns 0-based.
type CallSite = (String, (i64, i64, i64, i64));

/// A raw module-import candidate from the `ts-extractor` (IMPORTS-MODULE-INGEST-1). The extractor emits
/// these only for relative + resolved imports; `resolved_path` is partition-relative and EXTENSIONLESS
/// (e.g. `src/shapes`). The target is resolved to a real file-scope node later (D4), or NOT CAPTURED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImport {
    /// The raw specifier as written (e.g. `"./shapes"`).
    pub raw_specifier: String,
    /// The extractor-resolved partition-relative path (extensionless).
    pub resolved_path: String,
}

/// AST facts for one file: located nodes (with complexity) + call-expression sites,
/// each carrying the callee name (last segment) and the call-expression range.
pub struct AstFacts {
    /// Located nodes.
    pub nodes: Vec<AstNodeLite>,
    /// Call-expression sites used to confirm `Calls` (callee name + range).
    pub call_sites: Vec<CallSite>,
    /// Relative + resolved module-import candidates (IMPORTS-MODULE-INGEST-1).
    pub imports: Vec<RawImport>,
    /// All producer import OBSERVATIONS (IMPORTS-EXTRACT-COMPLETENESS-1), carried through verbatim;
    /// ingest classifies them (with node-resolution) into IR observations.
    pub import_observations: Vec<TsImportObservation>,
}

/// Run ts-extractor on one file; return nodes + call-expression sites + relative import candidates.
pub fn ast_facts_for_source(source: &str, file_path: &str, repo_uid: &str) -> AstFacts {
    let mut extractor = TsExtractor::new();
    if extractor.initialize().is_err() {
        return AstFacts {
            nodes: Vec::new(),
            call_sites: Vec::new(),
            imports: Vec::new(),
            import_observations: Vec::new(),
        };
    }
    let file_uid = format!("{repo_uid}:{file_path}");
    let result = match extractor.extract(source, file_path, &file_uid, repo_uid, "ingest-core-1") {
        Ok(r) => r,
        Err(_) => {
            return AstFacts {
                nodes: Vec::new(),
                call_sites: Vec::new(),
                imports: Vec::new(),
                import_observations: Vec::new(),
            }
        }
    };
    let call_sites: Vec<CallSite> = result
        .edges
        .iter()
        .filter(|e| matches!(e.edge_type, TsEdgeType::Calls))
        .filter_map(|e| {
            e.location.as_ref().map(|loc| {
                (
                    name_from_target(&e.target_key).to_string(),
                    (loc.line_start, loc.col_start, loc.line_end, loc.col_end),
                )
            })
        })
        .collect();
    // IMPORTS-MODULE-INGEST-1: harvest relative+resolved import candidates from the SAME public
    // `result.edges` (the extractor emits import EDGES only for those; see slice doc D3). The resolved
    // path lives in `metadata_json.resolvedPath` (extensionless). Authority = ts-extractor, NOT SCIP.
    let imports: Vec<RawImport> = result
        .edges
        .iter()
        .filter(|e| matches!(e.edge_type, TsEdgeType::Imports))
        .filter_map(|e| raw_import_from_metadata(e.metadata_json.as_deref()))
        .collect();
    let metrics = result.metrics;
    let nodes = result
        .nodes
        .into_iter()
        .filter_map(|n| {
            let cyclomatic = metrics.get(&n.stable_key).map(|m| m.cyclomatic_complexity);
            let is_file_scope = matches!(n.kind, NodeKind::File);
            n.location.map(|loc| AstNodeLite {
                stable_key: n.stable_key,
                name: n.name,
                cyclomatic,
                is_file_scope,
                line_start: loc.line_start,
                col_start: loc.col_start,
                line_end: loc.line_end,
                col_end: loc.col_end,
            })
        })
        .collect();
    AstFacts {
        nodes,
        call_sites,
        imports,
        import_observations: result.import_observations,
    }
}

/// Parse a ts-extractor import edge's `metadata_json` (`{"rawPath":..,"resolvedPath":..}`) into a
/// [`RawImport`]. Returns `None` if the metadata is absent or malformed (the edge is then NOT CAPTURED).
fn raw_import_from_metadata(metadata_json: Option<&str>) -> Option<RawImport> {
    let v: serde_json::Value = serde_json::from_str(metadata_json?).ok()?;
    let raw_specifier = v.get("rawPath")?.as_str()?.to_string();
    let resolved_path = v.get("resolvedPath")?.as_str()?.to_string();
    Some(RawImport {
        raw_specifier,
        resolved_path,
    })
}

/// Extract a callee name from a ts-extractor edge target (a stable key
/// `...#NAME:SYMBOL:...`, or a possibly-dotted bare name).
fn name_from_target(target_key: &str) -> &str {
    if let Some(h) = target_key.find('#') {
        let after = &target_key[h + 1..];
        return after.split(':').next().unwrap_or(after);
    }
    target_key.rsplit('.').next().unwrap_or(target_key)
}

/// Caller resolution: innermost AST node whose span contains a point.
pub fn enclosing_node(nodes: &[AstNodeLite], line: i64, col: i64) -> Option<&AstNodeLite> {
    nodes
        .iter()
        .filter(|n| contains_point(n, line, col))
        .min_by_key(|n| span_size(n))
}

/// Caller resolution with graph closure: the innermost enclosing AST node whose key is
/// a materialized IR node (`node_keys`). Non-materialized enclosing nodes (a constructor
/// or getter that did not match a SCIP def, a `local` symbol, a destructuring binding)
/// are bubbled past, so the resolved source always exists as a node. The FILE node
/// always encloses and is always materialized, guaranteeing no dangling edge source.
pub fn enclosing_materialized_node<'a>(
    nodes: &'a [AstNodeLite],
    node_keys: &HashSet<String>,
    line: i64,
    col: i64,
) -> Option<&'a AstNodeLite> {
    nodes
        .iter()
        .filter(|n| node_keys.contains(n.stable_key.as_str()))
        .filter(|n| contains_point(n, line, col))
        .min_by_key(|n| span_size(n))
}

/// Strict call confirmation: a call-expression with the matching callee name covers
/// the point. Name match prevents promoting arguments inside a call to `Calls`.
fn is_call_at(call_sites: &[CallSite], name: &str, line: i64, col: i64) -> bool {
    call_sites.iter().any(|(cn, (ls, cs, le, ce))| {
        cn == name && {
            let after = line > *ls || (line == *ls && col >= *cs);
            let before = line < *le || (line == *le && col <= *ce);
            after && before
        }
    })
}

/// Strict-default edge counts surfaced by derivation. The three emitted-edge buckets
/// (`calls`, `references`, `file_scope_refs`) are disjoint and sum to the emitted set.
#[derive(Debug, Default, Clone)]
pub struct EdgeReport {
    /// All non-definition occurrences.
    pub total_refs: usize,
    /// Refs with an enclosing (caller) AST node.
    pub caller_resolved: usize,
    /// Refs whose referenced symbol is an in-partition definition.
    pub callee_resolved: usize,
    /// Refs with no enclosing caller node.
    pub no_caller: usize,
    /// Emitted edges that are declaration-level syntax-confirmed `Calls`
    /// (`SyntaxConfirmedCall`). The strict call graph.
    pub calls: usize,
    /// Emitted edges that are declaration-level non-call `References`
    /// (`DerivedReference`).
    pub references: usize,
    /// Emitted edges whose caller is a file/module-scope node (`FileScopeReference`).
    /// Always `References`, never `Calls`.
    pub file_scope_refs: usize,
    /// Emitted edges whose callee is a fallback node.
    pub fallback_target: usize,
}

impl EdgeReport {
    /// Accumulate another report.
    pub fn add(&mut self, o: &EdgeReport) {
        self.total_refs += o.total_refs;
        self.caller_resolved += o.caller_resolved;
        self.callee_resolved += o.callee_resolved;
        self.no_caller += o.no_caller;
        self.calls += o.calls;
        self.references += o.references;
        self.file_scope_refs += o.file_scope_refs;
        self.fallback_target += o.fallback_target;
    }
}

/// Derive edges for one document. `symbol_to_key` is the partition-wide map from a
/// SCIP symbol to (canonical key, is_fallback, name); `node_keys` is the partition-wide
/// set of materialized IR node keys, used to resolve the caller to the innermost
/// enclosing materialized node (graph closure: no dangling edge source). Emits `Calls`
/// only when a call-expression confirms it; otherwise `References`. Never infers a call
/// from the mere existence of a SCIP reference.
#[allow(clippy::too_many_arguments)]
pub fn derive_edges(
    doc: &scip::types::Document,
    facts: &AstFacts,
    symbol_to_key: &HashMap<String, (String, bool, String)>,
    node_keys: &HashSet<String>,
    indexer: &str,
    indexer_version: &str,
    build_inputs_hash: &str,
    edges_out: &mut Vec<IrEdge>,
) -> EdgeReport {
    let mut r = EdgeReport::default();
    for occ in &doc.occurrences {
        if occ.symbol_roles & 0x1 != 0 || occ.range.len() < 2 {
            continue; // skip definitions and malformed ranges
        }
        r.total_refs += 1;
        let line = occ.range[0] as i64 + 1; // SCIP 0-based -> ts 1-based
        let col = occ.range[1] as i64;
        let caller = enclosing_materialized_node(&facts.nodes, node_keys, line, col);
        match caller {
            Some(_) => r.caller_resolved += 1,
            None => r.no_caller += 1,
        }
        let callee = symbol_to_key.get(&occ.symbol);
        if callee.is_some() {
            r.callee_resolved += 1;
        }
        let (Some(caller), Some((callee_key, is_fb, callee_name))) = (caller, callee) else {
            continue;
        };
        let (edge_type, basis) = if caller.is_file_scope {
            // Strict rule (decision c): a file/module-scope caller is not callable. Its
            // edges are References with FileScopeReference basis and never enter the
            // strict call graph, even when a top-level call-expression covers the ref
            // (that is module-init execution, modeled later, not a callable edge).
            r.file_scope_refs += 1;
            (IrEdgeType::References, EdgeBasis::FileScopeReference)
        } else if is_call_at(&facts.call_sites, callee_name, line, col) {
            r.calls += 1;
            (IrEdgeType::Calls, EdgeBasis::SyntaxConfirmedCall)
        } else {
            r.references += 1;
            (IrEdgeType::References, EdgeBasis::DerivedReference)
        };
        if *is_fb {
            r.fallback_target += 1;
        }
        edges_out.push(IrEdge {
            src: CanonicalKey::from_existing(caller.stable_key.clone()),
            dst: CanonicalKey::from_existing(callee_key.clone()),
            edge_type,
            basis,
            provenance: Provenance {
                indexer: indexer.to_string(),
                indexer_version: indexer_version.to_string(),
                scip_symbol_id: Some(occ.symbol.clone()),
                build_inputs_hash: build_inputs_hash.to_string(),
            },
            // Call/reference edges carry no import metadata (IMPORTS-MODULE-INGEST-1); import edges
            // are joined separately from the ts-extractor in the ingest commit.
            import: None,
        });
    }
    r
}

// ── Partition ingestion entrypoint (the headless Test API surface) ─

/// Node-build counts surfaced by partition ingestion.
#[derive(Debug, Default, Clone)]
pub struct NodeCounts {
    /// Definitions whose identity was adopted from a matched AST node.
    pub matched: usize,
    /// Subset of `matched` recovered via narrow `<constructor>`/`<get>`/`<set>`
    /// reconciliation.
    pub reconciled: usize,
    /// Definitions with no AST match, synthesized as labeled fallback.
    pub fallback: usize,
    /// Materialized file/module-scope (FILE) nodes.
    pub file_scope: usize,
}

/// The result of ingesting one partition from a decoded SCIP index. This is the use-case
/// surface the 4c harness asserts against directly — headless, no GUI / DB / probe.
pub struct IngestOutcome {
    /// The assembled canonical IR (partition + nodes + edges).
    pub ir: PartitionIr,
    /// Strict edge-derivation report.
    pub edges_report: EdgeReport,
    /// Node-build counts.
    pub node_counts: NodeCounts,
    /// One value fact: cyclomatic complexity by canonical key.
    pub complexity: BTreeMap<String, u32>,
    /// Raw ts-extractor call-site count (the rmap upper bound for strict calls).
    pub ts_call_sites: usize,
    /// Documents whose source file could not be read.
    pub missing_source: usize,
    /// IMPORTS-MODULE-INGEST-1: relative+resolved import candidates that matched a real file-scope node
    /// in this partition and became `EdgeType::Imports` edges (node-connected FILE -> FILE).
    pub imports_resolved: usize,
    /// IMPORTS-MODULE-INGEST-1: import candidates NOT CAPTURED (target file not present in this partition
    /// — cross-partition / index-file / extension mismatch). NOT emitted as symbolic dangling edges.
    pub imports_not_captured: usize,
}

/// Strip a TypeScript source extension from a partition-relative path (longest match first), so a real
/// FILE node path (`src/shapes.ts`) can be compared to the extractor's extensionless resolved path
/// (`src/shapes`). Returns the input unchanged if it carries no known TS extension.
fn strip_ts_ext(path: &str) -> &str {
    for ext in [".d.ts", ".ts", ".tsx", ".mts", ".cts"] {
        if let Some(s) = path.strip_suffix(ext) {
            return s;
        }
    }
    path
}

/// Build the partition's FILE-node index: extensionless partition-relative path -> full FILE node key,
/// for every file-scope node. Shared by edge resolution AND observation classification (so they agree on
/// what "node-resolves in this partition" means).
fn build_file_index<'a>(
    repo_uid: &str,
    node_keys: &'a HashSet<String>,
) -> HashMap<&'a str, &'a str> {
    let prefix = format!("{repo_uid}:");
    let mut file_index: HashMap<&str, &str> = HashMap::new();
    for k in node_keys {
        if let Some(path) = k
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(":FILE"))
        {
            file_index.insert(strip_ts_ext(path), k.as_str());
        }
    }
    file_index
}

/// Classify producer import observations into IR observations (IMPORTS-EXTRACT-COMPLETENESS-1), WITHOUT
/// inference — each class follows from an observed fact + the partition's node-resolution:
///   dynamic              -> DynamicUnsupported
///   non-relative         -> PackageExternal
///   relative + resolves  -> StaticResolved   (the SAME node-resolution as the edge; also an edge exists)
///   relative + no resolve-> StaticUnresolved (target file not in this partition)
/// Modifiers (re-export / type-only / side-effect) are carried through. Observations are NEVER edges.
/// IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1: the PACKAGE NAME of a bare specifier (`@scope/pkg/sub` -> `@scope/pkg`;
/// `pkg/sub` -> `pkg`). Local copy -- scip-ingest does not depend on the import-resolver.
fn ingest_package_name_of(specifier: &str) -> String {
    let mut segs = specifier.split('/');
    let first = segs.next().unwrap_or(specifier);
    if first.starts_with('@') {
        match segs.next() {
            Some(second) => format!("{first}/{second}"),
            None => first.to_string(),
        }
    } else {
        first.to_string()
    }
}

/// The DefinitelyTyped package dir for a package name: `pkg` -> `@types/pkg`; `@scope/pkg` -> `@types/scope__pkg`.
fn types_package_dir(package_name: &str) -> String {
    match package_name.strip_prefix('@') {
        Some(rest) => format!("@types/{}", rest.replacen('/', "__", 1)),
        None => format!("@types/{package_name}"),
    }
}

/// IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1 (the trust hinge): does `package_name` resolve to a REAL external
/// install -- NOT a workspace package symlinked into node_modules? Checks the partition + repo-root
/// node_modules (and `@types/`) and CANONICALIZES: external iff the realpath has a `node_modules` path
/// segment OR is outside the canonical repo root. A realpath INSIDE the repo source tree (a workspace
/// symlink, e.g. node_modules/@amodx/shared -> packages/shared) is NOT external. CONSERVATIVE: any
/// canonicalization failure (incl. an ambiguous/symlinked repo root) -> `false` (blocks, never a false
/// external). Workspace-map precedence in the classifier is the primary guard; this is the second.
fn resolves_external_node_modules(root: &str, repo_root: &str, package_name: &str) -> bool {
    let canon_repo = match std::fs::canonicalize(repo_root) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let types = types_package_dir(package_name);
    for base in [root, repo_root] {
        for name in [package_name, types.as_str()] {
            let candidate = format!("{base}/node_modules/{name}");
            if let Ok(real) = std::fs::canonicalize(&candidate) {
                let in_node_modules = real.components().any(|c| c.as_os_str() == "node_modules");
                let outside_repo = !real.starts_with(&canon_repo);
                if in_node_modules || outside_repo {
                    return true;
                }
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn classify_import_observations(
    repo_uid: &str,
    source_file: &str,
    observations: &[TsImportObservation],
    node_keys: &HashSet<String>,
    root: &str,
    repo_root: &str,
    nm_memo: &mut std::collections::HashMap<String, bool>,
) -> Vec<IrImportObservation> {
    let file_index = build_file_index(repo_uid, node_keys);
    observations
        .iter()
        .map(|o| {
            let resolution = if o.is_dynamic {
                ImportResolution::DynamicUnsupported
            } else if !o.is_relative {
                ImportResolution::PackageExternal
            } else if o
                .resolved_path
                .as_deref()
                .map(|rp| file_index.contains_key(rp))
                .unwrap_or(false)
            {
                ImportResolution::StaticResolved
            } else {
                ImportResolution::StaticUnresolved
            };
            // IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1: positive external evidence for a non-relative specifier
            // (memoized per package; the FS resolution runs at the ingest boundary, NOT the classifier).
            let external_node_modules = if o.is_relative {
                false
            } else {
                let pkg = ingest_package_name_of(&o.raw_specifier);
                if let Some(&cached) = nm_memo.get(&pkg) {
                    cached
                } else {
                    let result = resolves_external_node_modules(root, repo_root, &pkg);
                    nm_memo.insert(pkg, result);
                    result
                }
            };
            IrImportObservation {
                source_file: source_file.to_string(),
                raw_specifier: o.raw_specifier.clone(),
                resolution,
                is_re_export: o.is_re_export,
                is_type_only: o.is_type_only,
                is_side_effect: o.is_side_effect,
                external_node_modules,
            }
        })
        .collect()
}

/// Resolve `ts-extractor` import candidates into node-connected FILE -> FILE import edges (D4).
///
/// BOTH endpoints must be existing file-scope (`:FILE`) nodes in THIS partition: `src` is the importing
/// file's FILE node; `dst` is found by matching the extractor's extensionless `resolved_path` against the
/// partition's FILE nodes (extension stripped). An import whose target file is not present in the
/// partition (cross-partition, missing, index-file, or extension mismatch) is **NOT CAPTURED** — never a
/// symbolic dangling edge (the resolution gap is a documented limitation, IMPORTS-XPART-RESOLUTION-1).
/// Returns `(edges, resolved_count, not_captured_count)`.
fn resolve_import_edges(
    repo_uid: &str,
    per_doc_imports: &[(String, Vec<RawImport>)],
    node_keys: &HashSet<String>,
    indexer: &str,
    indexer_version: &str,
    build_inputs_hash: &str,
) -> (Vec<IrEdge>, usize, usize) {
    let file_index = build_file_index(repo_uid, node_keys);

    let mut edges = Vec::new();
    let (mut resolved, mut not_captured) = (0usize, 0usize);
    for (source_file, imports) in per_doc_imports {
        let src_key = format!("{repo_uid}:{source_file}:FILE");
        let src_present = node_keys.contains(&src_key);
        for imp in imports {
            // Both src (importing file) and dst (target) must resolve to real file-scope nodes.
            match (src_present, file_index.get(imp.resolved_path.as_str())) {
                (true, Some(dst_key)) => {
                    edges.push(IrEdge {
                        src: CanonicalKey::from_existing(src_key.clone()),
                        dst: CanonicalKey::from_existing((*dst_key).to_string()),
                        edge_type: IrEdgeType::Imports,
                        basis: EdgeBasis::AstImport,
                        provenance: Provenance {
                            indexer: indexer.to_string(),
                            indexer_version: indexer_version.to_string(),
                            // AST-derived (ts-extractor import declaration), no SCIP symbol.
                            scip_symbol_id: None,
                            build_inputs_hash: build_inputs_hash.to_string(),
                        },
                        import: Some(ImportEdgeMeta {
                            raw_specifier: imp.raw_specifier.clone(),
                            resolved_path: imp.resolved_path.clone(),
                            resolution: ImportResolution::StaticResolved,
                        }),
                    });
                    resolved += 1;
                }
                _ => not_captured += 1,
            }
        }
    }
    (edges, resolved, not_captured)
}

/// Build the REPO-RELATIVE key path for a document (KEY-NAMESPACE-REPO-RELATIVE-1): qualify the
/// partition-relative SCIP document path with the partition's repo-relative prefix, so keys across
/// partitions share one collision-free namespace. POSIX-normalized; `..` is REJECTED (paths are not
/// allowed to escape). `partition_prefix` is `""` for a repo-root package (keys then == the doc path).
fn repo_relative_file_path(partition_prefix: &str, doc_relative: &str) -> Result<String, String> {
    let combined = if partition_prefix.is_empty() {
        doc_relative.to_string()
    } else {
        format!(
            "{}/{}",
            partition_prefix.trim_end_matches('/'),
            doc_relative
        )
    };
    let posix = combined.replace('\\', "/");
    let mut parts = Vec::new();
    for seg in posix.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "'..' not allowed in repo-relative key path: {posix}"
                ))
            }
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

/// IMPORTS-PACKAGE-RESOLUTION-1: read `{root}/package.json` -> (`name`, declared dependency NAMES). The
/// metadata captured at the INGEST boundary so the livegraph classifier stays IO-free. Best-effort: a
/// missing/malformed manifest -> `(None, empty)` (SAFE -- the partition contributes no workspace identity +
/// no external evidence, so its bare imports stay conservatively `PackageUnresolved`). dependencies +
/// devDependencies + peerDependencies keys are unioned (positive external evidence).
fn read_package_manifest(root: &str) -> (Option<String>, std::collections::BTreeSet<String>) {
    let text = match std::fs::read_to_string(format!("{root}/package.json")) {
        Ok(t) => t,
        Err(_) => return (None, std::collections::BTreeSet::new()),
    };
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return (None, std::collections::BTreeSet::new()),
    };
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut deps = std::collections::BTreeSet::new();
    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = json.get(key).and_then(|v| v.as_object()) {
            for dep_name in obj.keys() {
                deps.insert(dep_name.clone());
            }
        }
    }
    (name, deps)
}

/// IMPORTS-TSCONFIG-PATHS-1: read `{root}/tsconfig.json` (JSONC, via json5) -> the partition's path-alias
/// config (compilerOptions.baseUrl + paths). Captured at the INGEST boundary (resolver stays IO-free).
/// Best-effort: missing/malformed/no-paths -> `None` (SAFE -- the partition's `@/` imports stay blocking).
/// `base_url` defaults to `"."` when paths exist without an explicit baseUrl.
fn read_tsconfig_aliases(root: &str, partition_prefix: &str) -> Option<TsconfigAliasConfig> {
    let text = std::fs::read_to_string(format!("{root}/tsconfig.json")).ok()?;
    let json: serde_json::Value = json5::from_str(&text).ok()?;
    let compiler_options = json.get("compilerOptions")?;
    let paths_obj = compiler_options.get("paths").and_then(|v| v.as_object())?;
    let mut paths = std::collections::BTreeMap::new();
    for (pattern, targets) in paths_obj {
        if let Some(arr) = targets.as_array() {
            let target_list: Vec<String> = arr
                .iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect();
            if !target_list.is_empty() {
                paths.insert(pattern.clone(), target_list);
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    let base_url = compiler_options
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    Some(TsconfigAliasConfig {
        base_url,
        paths,
        partition_prefix: partition_prefix.to_string(),
    })
}

/// Ingest one TypeScript partition: decode-driven node build (AST-adopted / reconciled /
/// fallback / materialized FILE) plus strict edge derivation, assembled into a
/// `PartitionIr`. Source is read from `{root}/{relative_path}`. `partition_prefix` is the partition's
/// REPO-RELATIVE root (e.g. `"packages/a"`; `""` for a repo-root package); keys are built repo-relative
/// from it (KEY-NAMESPACE-REPO-RELATIVE-1). One buildable unit.
#[allow(clippy::too_many_arguments)]
pub fn ingest_partition(
    index: &Index,
    root: &str,
    repo_uid: &str,
    partition_id: &str,
    indexer: &str,
    indexer_version: &str,
    build_inputs_hash: &str,
    partition_prefix: &str,
) -> IngestOutcome {
    let (package_name, declared_dependencies) = read_package_manifest(root);
    let tsconfig_aliases = read_tsconfig_aliases(root, partition_prefix);
    let partition = Partition {
        id: PartitionId::new(partition_id),
        kind: PartitionKind::TsPackage,
        root: root.to_string(),
        indexer: indexer.to_string(),
        indexer_version: indexer_version.to_string(),
        build_inputs_hash: build_inputs_hash.to_string(),
        package_name,
        declared_dependencies,
        tsconfig_aliases,
    };
    let mut ir = PartitionIr::new(partition);
    let mut node_counts = NodeCounts::default();
    let mut complexity: BTreeMap<String, u32> = BTreeMap::new();
    let mut symbol_to_key: HashMap<String, (String, bool, String)> = HashMap::new();
    let mut node_keys: HashSet<String> = HashSet::new();
    let mut ts_call_sites = 0usize;
    let mut missing_source = 0usize;

    // Pass 1: per document, build nodes + the partition-wide symbol map and key set.
    let mut facts_by_doc: Vec<Option<AstFacts>> = Vec::with_capacity(index.documents.len());
    let mut doc_key_paths: Vec<String> = Vec::with_capacity(index.documents.len());
    for doc in &index.documents {
        // KEY-NAMESPACE-REPO-RELATIVE-1: keys derive from the REPO-RELATIVE path; the source is still
        // read from the partition-relative on-disk path. A malformed (`..`) path skips the doc.
        let key_path = match repo_relative_file_path(partition_prefix, &doc.relative_path) {
            Ok(p) => p,
            Err(_) => {
                missing_source += 1;
                doc_key_paths.push(String::new());
                facts_by_doc.push(None);
                continue;
            }
        };
        let facts = match std::fs::read_to_string(format!("{root}/{}", doc.relative_path)) {
            Ok(src) => Some(ast_facts_for_source(&src, &key_path, repo_uid)),
            Err(_) => {
                missing_source += 1;
                None
            }
        };
        if let Some(f) = &facts {
            ts_call_sites += f.call_sites.len();
            let b = build_partition_nodes(
                doc,
                &key_path,
                &f.nodes,
                repo_uid,
                partition_id,
                indexer,
                indexer_version,
                build_inputs_hash,
            );
            node_counts.matched += b.matched;
            node_counts.reconciled += b.reconciled;
            node_counts.fallback += b.fallback;
            node_counts.file_scope += b.file_scope;
            for (k, v) in &b.complexity {
                complexity.insert(k.clone(), *v);
            }
            for n in &b.nodes {
                node_keys.insert(n.key.as_str().to_string());
                if let Some(sym) = &n.provenance.scip_symbol_id {
                    let is_fb = n.identity_source == IdentitySource::ScipSynthesizedFallback;
                    symbol_to_key.insert(
                        sym.clone(),
                        (n.key.as_str().to_string(), is_fb, n.name.clone()),
                    );
                }
            }
            ir.nodes.extend(b.nodes);
        }
        doc_key_paths.push(key_path);
        facts_by_doc.push(facts);
    }

    // Pass 2: per document, derive edges into the partition IR.
    let mut edges_report = EdgeReport::default();
    for (doc, facts) in index.documents.iter().zip(facts_by_doc.iter()) {
        if let Some(f) = facts {
            edges_report.add(&derive_edges(
                doc,
                f,
                &symbol_to_key,
                &node_keys,
                indexer,
                indexer_version,
                build_inputs_hash,
                &mut ir.edges,
            ));
        }
    }

    // Pass 3 (IMPORTS-MODULE-INGEST-1): resolve AST import candidates to node-connected FILE -> FILE
    // import edges. Runs after pass 1 so EVERY partition file-scope node is known (a target may live in
    // a different document than the importer).
    let per_doc_imports: Vec<(String, Vec<RawImport>)> = facts_by_doc
        .iter()
        .zip(doc_key_paths.iter())
        .filter_map(|(facts, key_path)| {
            facts
                .as_ref()
                .map(|f| (key_path.clone(), f.imports.clone()))
        })
        .collect();
    let (import_edges, imports_resolved, imports_not_captured) = resolve_import_edges(
        repo_uid,
        &per_doc_imports,
        &node_keys,
        indexer,
        indexer_version,
        build_inputs_hash,
    );
    ir.edges.extend(import_edges);

    // IMPORTS-EXTRACT-COMPLETENESS-1 + IMPORTS-XPART-WIRING-1: classify each doc's producer import
    // observations into IR observations, stamping the importing file's repo-relative key_path as
    // `source_file` (needed for the cross-partition edge src). Same node-resolution as the edge pass.
    // IMPORTS-PACKAGE-EXTERNAL-EVIDENCE-1: the repo root (= partition root minus the repo-relative prefix) is
    // the canonicalization base for the node_modules realpath check; the memo dedups the FS check per package.
    let repo_root = if partition_prefix.is_empty() {
        root.to_string()
    } else {
        root.strip_suffix(partition_prefix)
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| root.to_string())
    };
    let mut nm_memo: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut ir_observations: Vec<IrImportObservation> = Vec::new();
    for (facts, key_path) in facts_by_doc.iter().zip(doc_key_paths.iter()) {
        if let Some(f) = facts {
            ir_observations.extend(classify_import_observations(
                repo_uid,
                key_path,
                &f.import_observations,
                &node_keys,
                root,
                &repo_root,
                &mut nm_memo,
            ));
        }
    }
    ir.import_observations = ir_observations;

    IngestOutcome {
        ir,
        edges_report,
        node_counts,
        complexity,
        ts_call_sites,
        missing_source,
        imports_resolved,
        imports_not_captured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_constructor_marker() {
        assert_eq!(reconcile_scip_name("<constructor>"), "constructor");
    }

    #[test]
    fn reconcile_getter_marker() {
        assert_eq!(reconcile_scip_name("<get>contentPath"), "contentPath");
        assert_eq!(reconcile_scip_name("<get>id"), "id");
    }

    #[test]
    fn reconcile_setter_marker() {
        assert_eq!(reconcile_scip_name("<set>value"), "value");
    }

    #[test]
    fn reconcile_plain_names_pass_through() {
        assert_eq!(reconcile_scip_name("load"), "load");
        assert_eq!(reconcile_scip_name("KnowledgeBase"), "KnowledgeBase");
    }

    #[test]
    fn reconcile_no_fuzzy_on_unknown_markers() {
        // Only the proven closed set is normalized; anything else is verbatim.
        assert_eq!(reconcile_scip_name(""), "");
        assert_eq!(reconcile_scip_name("<unknown>x"), "<unknown>x");
        assert_eq!(reconcile_scip_name("getter"), "getter");
    }

    // ── IMPORTS-MODULE-INGEST-1: node-resolved FILE -> FILE import edges (D4) ──

    #[test]
    fn resolve_import_edges_node_resolves_and_skips() {
        let node_keys: HashSet<String> = ["r:src/main.ts:FILE", "r:src/shapes.ts:FILE"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let per_doc = vec![(
            "src/main.ts".to_string(),
            vec![
                // Resolves: target src/shapes matches src/shapes.ts (extension stripped).
                RawImport {
                    raw_specifier: "./shapes".into(),
                    resolved_path: "src/shapes".into(),
                },
                // Target absent from the partition -> NOT CAPTURED (no symbolic dangling edge).
                RawImport {
                    raw_specifier: "./missing".into(),
                    resolved_path: "src/missing".into(),
                },
            ],
        )];
        let (edges, resolved, not_captured) =
            resolve_import_edges("r", &per_doc, &node_keys, "scip-typescript", "0", "h");
        assert_eq!(resolved, 1);
        assert_eq!(not_captured, 1);
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e.src.as_str(), "r:src/main.ts:FILE");
        assert_eq!(e.dst.as_str(), "r:src/shapes.ts:FILE"); // node-resolved
        assert_eq!(e.edge_type, IrEdgeType::Imports);
        assert_eq!(e.basis, EdgeBasis::AstImport);
        let meta = e.import.as_ref().expect("import meta present");
        assert_eq!(meta.raw_specifier, "./shapes");
        assert_eq!(meta.resolved_path, "src/shapes");
        assert_eq!(meta.resolution, ImportResolution::StaticResolved);
    }

    #[test]
    fn synthetic_fixture_import_edge_is_node_resolved() {
        // End-to-end on the committed real index.scip: src/main.ts has `import { Circle } from "./shapes"`.
        let root = format!("{}/tests/fixtures/synthetic", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(format!("{root}/index.scip")).expect("read index.scip");
        let index = decode_index(&bytes).expect("decode index.scip");
        let outcome = ingest_partition(
            &index,
            &root,
            "synthetic",
            "synthetic",
            "scip-typescript",
            "test",
            "h",
            "", // single-partition: repo-relative prefix is empty (keys byte-stable)
        );
        assert_eq!(
            outcome.imports_resolved, 1,
            "the single relative import resolves to a real file-scope node"
        );
        assert_eq!(outcome.imports_not_captured, 0);
        let imports: Vec<&IrEdge> = outcome
            .ir
            .edges
            .iter()
            .filter(|e| e.edge_type == IrEdgeType::Imports)
            .collect();
        assert_eq!(imports.len(), 1, "exactly one import edge");
        let e = imports[0];
        assert_eq!(e.src.as_str(), "synthetic:src/main.ts:FILE");
        assert_eq!(e.dst.as_str(), "synthetic:src/shapes.ts:FILE");
        assert_eq!(e.basis, EdgeBasis::AstImport);
        let meta = e.import.as_ref().expect("import meta present");
        assert_eq!(meta.raw_specifier, "./shapes");
        assert_eq!(meta.resolution, ImportResolution::StaticResolved);
    }

    // ── IMPORTS-EXTRACT-COMPLETENESS-1: observation classification ──

    fn ts_obs(
        spec: &str,
        resolved: Option<&str>,
        is_relative: bool,
        is_dynamic: bool,
    ) -> TsImportObservation {
        TsImportObservation {
            raw_specifier: spec.to_string(),
            resolved_path: resolved.map(|s| s.to_string()),
            is_relative,
            is_type_only: false,
            is_re_export: false,
            is_side_effect: false,
            is_dynamic,
            location: None,
        }
    }

    #[test]
    fn classify_import_observations_each_class() {
        let node_keys: HashSet<String> = ["r:src/main.ts:FILE", "r:src/shapes.ts:FILE"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut reexport_type = ts_obs("./missing", Some("src/missing"), true, false);
        reexport_type.is_re_export = true;
        reexport_type.is_type_only = true;
        let obs = vec![
            ts_obs("./shapes", Some("src/shapes"), true, false), // resolves -> StaticResolved
            reexport_type, // no resolve -> StaticUnresolved + mods
            ts_obs("react", None, false, false), // non-relative -> PackageExternal
            ts_obs("./z", None, true, true), // dynamic -> DynamicUnsupported
        ];
        let mut nm_memo = std::collections::HashMap::new();
        let ir_obs = classify_import_observations(
            "r",
            "packages/a/src/main.ts",
            &obs,
            &node_keys,
            "/tmp/nonexistent-root",
            "/tmp/nonexistent-root",
            &mut nm_memo,
        );
        assert_eq!(ir_obs.len(), 4);
        assert_eq!(
            ir_obs[0].source_file, "packages/a/src/main.ts",
            "source_file stamped"
        );
        assert_eq!(ir_obs[0].resolution, ImportResolution::StaticResolved);
        assert_eq!(ir_obs[1].resolution, ImportResolution::StaticUnresolved);
        assert!(ir_obs[1].is_re_export && ir_obs[1].is_type_only); // modifiers carried through
        assert_eq!(ir_obs[2].resolution, ImportResolution::PackageExternal);
        assert_eq!(ir_obs[3].resolution, ImportResolution::DynamicUnsupported);
    }

    #[test]
    fn synthetic_fixture_import_observation_static_resolved() {
        let root = format!("{}/tests/fixtures/synthetic", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(format!("{root}/index.scip")).expect("read index.scip");
        let index = decode_index(&bytes).expect("decode index.scip");
        let outcome = ingest_partition(
            &index,
            &root,
            "synthetic",
            "synthetic",
            "scip-typescript",
            "test",
            "h",
            "", // single-partition: repo-relative prefix is empty (keys byte-stable)
        );
        // The fixture's single relative import (`./shapes`) is observed AND classified StaticResolved
        // (it also produced an edge); no spurious extra observations.
        let resolved: Vec<_> = outcome
            .ir
            .import_observations
            .iter()
            .filter(|o| o.resolution == ImportResolution::StaticResolved)
            .collect();
        assert_eq!(resolved.len(), 1, "one StaticResolved observation");
        assert_eq!(resolved[0].raw_specifier, "./shapes");
    }

    // ── KEY-NAMESPACE-REPO-RELATIVE-1: repo-relative key namespace ──

    #[test]
    fn repo_relative_path_rejects_dotdot_and_normalizes() {
        assert!(repo_relative_file_path("packages/a", "../escape.ts").is_err());
        assert!(repo_relative_file_path("", "../escape.ts").is_err());
        // single-partition: empty prefix -> unchanged (byte-stable).
        assert_eq!(
            repo_relative_file_path("", "src/main.ts").unwrap(),
            "src/main.ts"
        );
        // qualified + POSIX-normalized (`.` dropped, trailing slash on prefix trimmed).
        assert_eq!(
            repo_relative_file_path("packages/a/", "./src/main.ts").unwrap(),
            "packages/a/src/main.ts"
        );
    }

    #[test]
    fn two_partitions_repo_relative_keys_are_distinct() {
        // The SAME source file ingested under two partition prefixes -> DISTINCT repo-relative keys (no
        // collision; the LiveGraph defines map would retain both).
        let root = format!("{}/tests/fixtures/synthetic", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(format!("{root}/index.scip")).expect("read index.scip");
        let index = decode_index(&bytes).expect("decode index.scip");
        let a = ingest_partition(
            &index,
            &root,
            "repo",
            "pkg-a",
            "scip-typescript",
            "t",
            "h",
            "packages/a",
        );
        let b = ingest_partition(
            &index,
            &root,
            "repo",
            "pkg-b",
            "scip-typescript",
            "t",
            "h",
            "packages/b",
        );
        let has = |o: &IngestOutcome, k: &str| o.ir.nodes.iter().any(|n| n.key.as_str() == k);
        assert!(
            has(&a, "repo:packages/a/src/main.ts:FILE"),
            "pkg-a repo-relative FILE key"
        );
        assert!(
            has(&b, "repo:packages/b/src/main.ts:FILE"),
            "pkg-b repo-relative FILE key"
        );
        let a_keys: HashSet<String> =
            a.ir.nodes
                .iter()
                .map(|n| n.key.as_str().to_string())
                .collect();
        let b_keys: HashSet<String> =
            b.ir.nodes
                .iter()
                .map(|n| n.key.as_str().to_string())
                .collect();
        assert!(
            a_keys.is_disjoint(&b_keys),
            "no cross-partition key collision (defines would not overwrite)"
        );
    }
}
