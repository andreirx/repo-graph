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
use repo_graph_indexer::types::{EdgeType as TsEdgeType, NodeKind};
use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType as IrEdgeType, IdentitySource, IrEdge, IrNode, Partition,
    PartitionId, PartitionIr, PartitionKind, Provenance, SourceRange,
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
                        file: doc.relative_path.clone(),
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
                // Labeled fallback: synthesize a canonical-format key from SCIP info.
                let synth = format!(
                    "{repo_uid}:{}#{}:SYMBOL:{}",
                    doc.relative_path, d.name, d.kind
                );
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
                file: doc.relative_path.clone(),
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

/// AST facts for one file: located nodes (with complexity) + call-expression sites,
/// each carrying the callee name (last segment) and the call-expression range.
pub struct AstFacts {
    /// Located nodes.
    pub nodes: Vec<AstNodeLite>,
    /// Call-expression sites used to confirm `Calls` (callee name + range).
    pub call_sites: Vec<CallSite>,
}

/// Run ts-extractor on one file; return nodes + call-expression sites.
pub fn ast_facts_for_source(source: &str, file_path: &str, repo_uid: &str) -> AstFacts {
    let mut extractor = TsExtractor::new();
    if extractor.initialize().is_err() {
        return AstFacts {
            nodes: Vec::new(),
            call_sites: Vec::new(),
        };
    }
    let file_uid = format!("{repo_uid}:{file_path}");
    let result = match extractor.extract(source, file_path, &file_uid, repo_uid, "ingest-core-1") {
        Ok(r) => r,
        Err(_) => {
            return AstFacts {
                nodes: Vec::new(),
                call_sites: Vec::new(),
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
    AstFacts { nodes, call_sites }
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
}

/// Ingest one TypeScript partition: decode-driven node build (AST-adopted / reconciled /
/// fallback / materialized FILE) plus strict edge derivation, assembled into a
/// `PartitionIr`. Source is read from `{root}/{relative_path}`. One buildable unit.
#[allow(clippy::too_many_arguments)]
pub fn ingest_partition(
    index: &Index,
    root: &str,
    repo_uid: &str,
    partition_id: &str,
    indexer: &str,
    indexer_version: &str,
    build_inputs_hash: &str,
) -> IngestOutcome {
    let partition = Partition {
        id: PartitionId::new(partition_id),
        kind: PartitionKind::TsPackage,
        root: root.to_string(),
        indexer: indexer.to_string(),
        indexer_version: indexer_version.to_string(),
        build_inputs_hash: build_inputs_hash.to_string(),
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
    for doc in &index.documents {
        let facts = match std::fs::read_to_string(format!("{root}/{}", doc.relative_path)) {
            Ok(src) => Some(ast_facts_for_source(&src, &doc.relative_path, repo_uid)),
            Err(_) => {
                missing_source += 1;
                None
            }
        };
        if let Some(f) = &facts {
            ts_call_sites += f.call_sites.len();
            let b = build_partition_nodes(
                doc,
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

    IngestOutcome {
        ir,
        edges_report,
        node_counts,
        complexity,
        ts_call_sites,
        missing_source,
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
}
