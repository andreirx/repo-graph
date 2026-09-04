//! Core C++ extractor implementation.
//!
//! Uses tree-sitter-cpp to parse C++ source files and extract structural
//! information: symbols, edges, and metrics.

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::types::{
    ImportBinding, ImportKind, RuntimeBuiltinsSet, SourceLocation,
};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::routing::is_test_file;
use repo_graph_indexer::types::{CallArgPayload, ResolvedCallsite};
use repo_graph_indexer::types::{
    EdgeType, ExtractedEdge, ExtractedMetrics, ExtractedNode, ExtractionResult, NodeKind,
    NodeSubtype, Resolution, Visibility,
};

use crate::gtest_marker::detect_gtest_marker;
use crate::linkage::{
    extract_linkage_from_spec, FileLinkageStats, LanguageLinkage, LinkageMetadata,
};
use crate::metrics::compute_function_metrics;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "cpp-core:0.1.0";

/// Languages this extractor handles.
const LANGUAGES: &[&str] = &["cpp"];

// ── CPP-SB-1: State-boundary function detection ───────────────────
//
// C-style APIs (duplicated from C bindings for cpp language).
const SB_STDIO_FUNCTIONS: &[&str] = &["fopen"];
const SB_FCNTL_FUNCTIONS: &[&str] = &["open"];
const SB_SQLITE_FUNCTIONS: &[&str] = &["sqlite3_open", "sqlite3_open_v2"];

// C++ stream types that map to state-boundary symbols.
#[allow(dead_code)]
const STREAM_TYPES: &[&str] = &[
    "std::ifstream",
    "std::ofstream",
    "std::fstream",
    "ifstream",
    "ofstream",
    "fstream",
];

/// Stream type for local type map (D3 substrate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamType {
    Ifstream,
    Ofstream,
    Fstream,
}

impl StreamType {
    /// Parse from type specifier text.
    fn from_type_text(text: &str) -> Option<Self> {
        match text {
            "std::ifstream" | "ifstream" => Some(StreamType::Ifstream),
            "std::ofstream" | "ofstream" => Some(StreamType::Ofstream),
            "std::fstream" | "fstream" => Some(StreamType::Fstream),
            _ => None,
        }
    }

    /// Get constructor symbol for this stream type.
    fn constructor_symbol(&self) -> &'static str {
        match self {
            StreamType::Ifstream => "ifstream",
            StreamType::Ofstream => "ofstream",
            StreamType::Fstream => "fstream",
        }
    }

    /// Get .open() symbol for this stream type.
    fn open_symbol(&self) -> &'static str {
        match self {
            StreamType::Ifstream => "ifstream_open",
            StreamType::Ofstream => "ofstream_open",
            StreamType::Fstream => "fstream_open",
        }
    }

    /// Get direction-specific symbol when mode indicates read.
    #[allow(dead_code)]
    fn mode_read_symbol(&self) -> &'static str {
        match self {
            StreamType::Ifstream => "ifstream", // Already read
            StreamType::Ofstream => "ofstream", // Can't read from ofstream
            StreamType::Fstream => "fstream_read",
        }
    }

    /// Get direction-specific symbol when mode indicates write.
    #[allow(dead_code)]
    fn mode_write_symbol(&self) -> &'static str {
        match self {
            StreamType::Ifstream => "ifstream", // Can't write to ifstream
            StreamType::Ofstream => "ofstream", // Already write
            StreamType::Fstream => "fstream_write",
        }
    }
}

/// C++ runtime builtins (STL, etc.)
fn cpp_runtime_builtins() -> RuntimeBuiltinsSet {
    RuntimeBuiltinsSet {
        identifiers: vec![
            // STL containers
            "vector",
            "map",
            "unordered_map",
            "set",
            "unordered_set",
            "list",
            "deque",
            "array",
            "string",
            "wstring",
            // STL algorithms
            "sort",
            "find",
            "find_if",
            "copy",
            "transform",
            "accumulate",
            "for_each",
            "count",
            "count_if",
            "remove",
            "remove_if",
            "unique",
            "reverse",
            // STL utilities
            "make_pair",
            "make_tuple",
            "make_shared",
            "make_unique",
            "move",
            "forward",
            "swap",
            // I/O
            "cout",
            "cin",
            "cerr",
            "endl",
            "printf",
            "fprintf",
            "sprintf",
            "snprintf",
            // Memory
            "malloc",
            "calloc",
            "realloc",
            "free",
            "new",
            "delete",
            // Strings
            "strlen",
            "strcpy",
            "strncpy",
            "strcmp",
            "strcat",
            // Smart pointers
            "shared_ptr",
            "unique_ptr",
            "weak_ptr",
            // Exceptions
            "exception",
            "runtime_error",
            "logic_error",
            "invalid_argument",
            "out_of_range",
            // Assert
            "assert",
            "static_assert",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        // C++ standard library module specifiers
        module_specifiers: vec![
            "iostream",
            "fstream",
            "sstream",
            "string",
            "vector",
            "map",
            "set",
            "unordered_map",
            "unordered_set",
            "algorithm",
            "memory",
            "utility",
            "functional",
            "numeric",
            "cstdio",
            "cstdlib",
            "cstring",
            "cmath",
            "cassert",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

/// Concrete `ExtractorPort` adapter for C++.
pub struct CppExtractor {
    languages: Vec<String>,
    builtins: RuntimeBuiltinsSet,
    parser: Option<tree_sitter::Parser>,
    cpp_language: tree_sitter::Language,
}

impl CppExtractor {
    /// Create a new extractor. Call `initialize()` before `extract()`.
    pub fn new() -> Self {
        Self {
            languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
            builtins: cpp_runtime_builtins(),
            parser: None,
            cpp_language: tree_sitter_cpp::LANGUAGE.into(),
        }
    }
}

impl Default for CppExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorPort for CppExtractor {
    fn name(&self) -> &str {
        EXTRACTOR_NAME
    }

    fn languages(&self) -> &[String] {
        &self.languages
    }

    fn runtime_builtins(&self) -> &RuntimeBuiltinsSet {
        &self.builtins
    }

    fn initialize(&mut self) -> Result<(), ExtractorError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.cpp_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set C++ grammar: {}", e),
            })?;
        self.parser = Some(parser);
        Ok(())
    }

    fn extract(
        &self,
        source: &str,
        file_path: &str,
        file_uid: &str,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<ExtractionResult, ExtractorError> {
        // Verify initialization happened
        let _parser = self.parser.as_ref().ok_or_else(|| ExtractorError {
            message: "extractor not initialized — call initialize() first".into(),
        })?;

        // Clone parser for thread safety
        let mut parser_clone = tree_sitter::Parser::new();
        parser_clone
            .set_language(&self.cpp_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set C++ grammar: {}", e),
            })?;

        let tree = parser_clone
            .parse(source, None)
            .ok_or_else(|| ExtractorError {
                message: format!("tree-sitter returned null tree for {}", file_path),
            })?;

        let root = tree.root_node();
        let line_count = source.split('\n').count().max(1) as i64;
        let file_node_uid = uuid::Uuid::new_v4().to_string();
        let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
        let src = source.as_bytes();
        let is_test = is_test_file(file_path);

        let mut ctx = ExtractionCtx {
            file_path,
            file_uid,
            file_node_uid: &file_node_uid,
            repo_uid,
            snapshot_uid,
            nodes: vec![ExtractedNode {
                node_uid: file_node_uid.clone(),
                snapshot_uid: snapshot_uid.into(),
                repo_uid: repo_uid.into(),
                stable_key: format!("{}:{}:FILE", repo_uid, file_path),
                kind: NodeKind::File,
                subtype: Some(if is_test {
                    NodeSubtype::TestFile
                } else {
                    NodeSubtype::Source
                }),
                name: file_name.into(),
                qualified_name: Some(file_path.into()),
                file_uid: Some(file_uid.into()),
                parent_node_uid: None,
                location: Some(SourceLocation {
                    line_start: 1,
                    col_start: 0,
                    line_end: line_count,
                    col_end: 0,
                }),
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            }],
            edges: Vec::new(),
            import_bindings: Vec::new(),
            metrics: BTreeMap::new(),
            stable_key_counts: HashMap::new(),
            file_linkage_stats: FileLinkageStats::default(),
            current_namespace: Vec::new(),
            current_class: None,
            current_linkage: None,
            resolved_callsites: Vec::new(),
            local_stream_types: HashMap::new(),
        };

        // Walk top-level declarations
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            walk_top_level(&child, src, &mut ctx);
        }

        // Update file node metadata with linkage stats + the IS-TEST-CPP-1 gtest
        // marker. The FILE node has a single `metadata_json` field, so the two
        // facts are merged: a file that is both an `extern "C"` ABI boundary AND a
        // gtest test keeps both. `merge_file_metadata` returns the linkage blob
        // verbatim when there is no marker, so every non-test C++ file is
        // byte-identical to before this slice (extern-C files included).
        let gtest_marker = detect_gtest_marker(&root, src);
        if let Some(file_node) = ctx.nodes.first_mut() {
            file_node.metadata_json =
                merge_file_metadata(ctx.file_linkage_stats.to_json(), gtest_marker);
        }

        Ok(ExtractionResult {
            nodes: ctx.nodes,
            edges: ctx.edges,
            metrics: ctx.metrics,
            import_bindings: ctx.import_bindings,
            resolved_callsites: ctx.resolved_callsites,
            import_observations: Vec::new(),
        })
    }
}

/// IS-TEST-CPP-1: combine the (optional) `extern "C"` linkage metadata blob with
/// the structural gtest test marker into the FILE node's single `metadata_json`
/// field.
///
/// Byte-preserving for every non-marker file: with `gtest_marker == false` the
/// linkage blob is returned verbatim (`None` stays `None`), so extern-C and plain
/// C++ FILE nodes are unchanged by this slice. When the marker is present it is
/// recorded under `is_gtest_test = true`, merged into the linkage object when one
/// exists so no ABI-boundary fact is lost. (The linkage blob is our own valid-JSON
/// serialization; the `_` fallback only fires on an impossible parse failure, and
/// even then the load-bearing marker is still emitted.)
fn merge_file_metadata(linkage_json: Option<String>, gtest_marker: bool) -> Option<String> {
    if !gtest_marker {
        return linkage_json;
    }
    let mut obj = match linkage_json
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
    {
        Some(Ok(serde_json::Value::Object(map))) => map,
        _ => serde_json::Map::new(),
    };
    obj.insert("is_gtest_test".to_string(), serde_json::Value::Bool(true));
    serde_json::to_string(&serde_json::Value::Object(obj)).ok()
}

// ── Extraction context ───────────────────────────────────────────

struct ExtractionCtx<'a> {
    file_path: &'a str,
    file_uid: &'a str,
    file_node_uid: &'a str,
    repo_uid: &'a str,
    snapshot_uid: &'a str,
    nodes: Vec<ExtractedNode>,
    edges: Vec<ExtractedEdge>,
    import_bindings: Vec<ImportBinding>,
    metrics: BTreeMap<String, ExtractedMetrics>,
    stable_key_counts: HashMap<String, u32>,
    file_linkage_stats: FileLinkageStats,
    /// Current namespace stack (e.g., ["std", "chrono"])
    current_namespace: Vec<String>,
    /// Current class name if inside a class body
    current_class: Option<String>,
    /// Current linkage specification if inside extern "C" block
    current_linkage: Option<LanguageLinkage>,
    /// CPP-SB-1: ResolvedCallsite facts for state-boundary APIs.
    resolved_callsites: Vec<ResolvedCallsite>,
    /// CPP-SB-1 D3: Intra-function local type map for .open() resolution.
    /// Maps local variable identifier -> stream type. Cleared on function boundary.
    local_stream_types: HashMap<String, StreamType>,
}

impl<'a> ExtractionCtx<'a> {
    /// Build qualified name from namespace stack and optional class.
    fn qualified_name(&self, name: &str) -> String {
        let mut parts: Vec<&str> = self.current_namespace.iter().map(|s| s.as_str()).collect();
        if let Some(ref class) = self.current_class {
            parts.push(class);
        }
        parts.push(name);
        parts.join("::")
    }

    /// Generate a stable_key with duplicate disambiguation.
    fn make_stable_key(&mut self, qualified_name: &str, subtype: &NodeSubtype) -> String {
        let subtype_str = serde_json::to_value(subtype)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", subtype));

        let base_key = format!(
            "{}:{}#{}:SYMBOL:{}",
            self.repo_uid, self.file_path, qualified_name, subtype_str
        );

        let count = self.stable_key_counts.entry(base_key.clone()).or_insert(0);
        *count += 1;

        if *count == 1 {
            base_key
        } else {
            format!("{}:dup{}", base_key, count)
        }
    }

    /// Record linkage metadata for a symbol.
    fn linkage_metadata(&mut self) -> LinkageMetadata {
        let meta = LinkageMetadata::default().with_parent_linkage(self.current_linkage);
        if meta.is_c_abi_boundary() {
            self.file_linkage_stats.record_extern_c_symbol();
        }
        meta
    }
}

// ── Helper functions ─────────────────────────────────────────────

fn location_from_node(node: &tree_sitter::Node) -> SourceLocation {
    let start = node.start_position();
    let end = node.end_position();
    SourceLocation {
        line_start: (start.row + 1) as i64,
        col_start: start.column as i64,
        line_end: (end.row + 1) as i64,
        col_end: end.column as i64,
    }
}

/// Walk top-level declarations.
fn walk_top_level(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    match node.kind() {
        "preproc_include" => extract_include(node, src, ctx),
        // A `function_definition` / `declaration` is the shape tree-sitter-cpp
        // ERROR-recovers a macro-decorated type into (`class DLL_LINKAGE Name`,
        // `struct EXPORT Name : Base {}`). If the construct leads with a
        // class/struct/enum specifier fragment, it is a TYPE, not a function —
        // route it to the type path so the keyword drives the kind (a struct is
        // never `function`) and the real name is recovered from the header.
        "function_definition" => {
            if let Some((frag, subtype)) = leading_type_specifier(node) {
                extract_type(&frag, node, src, ctx, subtype);
            } else {
                extract_function(node, src, ctx)
            }
        }
        "declaration" => extract_declaration(node, src, ctx),
        "class_specifier" => extract_type(node, node, src, ctx, NodeSubtype::Class),
        "struct_specifier" => extract_type(node, node, src, ctx, NodeSubtype::Struct),
        "enum_specifier" => extract_type(node, node, src, ctx, NodeSubtype::Enum),
        "namespace_definition" => extract_namespace(node, src, ctx),
        "linkage_specification" => extract_linkage_spec(node, src, ctx),
        "template_declaration" => {
            // Walk into template to find the underlying declaration
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_top_level(&child, src, ctx);
            }
        }
        "type_definition" => extract_typedef(node, src, ctx),
        // Preprocessor blocks: recurse into contents
        "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_top_level(&child, src, ctx);
            }
        }
        _ => {}
    }
}

// ── Include extraction ───────────────────────────────────────────

fn extract_include(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let mut specifier = String::new();
    let mut is_system = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "system_lib_string" => {
                let text = child.utf8_text(src).unwrap_or("");
                specifier = text
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string();
                is_system = true;
            }
            "string_literal" => {
                let text = child.utf8_text(src).unwrap_or("");
                specifier = text.trim_matches('"').to_string();
                is_system = false;
            }
            _ => {}
        }
    }

    if specifier.is_empty() {
        return;
    }

    let metadata_json = if is_system {
        None
    } else {
        Some(serde_json::json!({ "rawPath": format!("./{}", specifier) }).to_string())
    };

    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: ctx.file_node_uid.into(),
        target_key: specifier.clone(),
        edge_type: EdgeType::Imports,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(location_from_node(node)),
        metadata_json,
    });

    let identifier = specifier
        .split('/')
        .next_back()
        .unwrap_or(&specifier)
        .trim_end_matches(".h")
        .trim_end_matches(".hpp")
        .trim_end_matches(".hxx")
        .to_string();

    ctx.import_bindings.push(ImportBinding {
        identifier,
        specifier,
        is_relative: !is_system,
        location: Some(location_from_node(node)),
        is_type_only: false,
        imported_name: None,
        // C++ #include brings in all declarations from the header,
        // similar to TypeScript namespace import semantics
        kind: ImportKind::Namespace,
    });
}

// ── Namespace extraction ─────────────────────────────────────────

fn extract_namespace(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string());

    // Push namespace onto stack
    if let Some(ref ns_name) = name {
        ctx.current_namespace.push(ns_name.clone());
    }

    // Process namespace body
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            walk_top_level(&child, src, ctx);
        }
    }

    // Pop namespace from stack
    if name.is_some() {
        ctx.current_namespace.pop();
    }
}

// ── Linkage specification extraction ─────────────────────────────

fn extract_linkage_spec(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let linkage = extract_linkage_from_spec(node, src);

    // Save and set current linkage
    let prev_linkage = ctx.current_linkage;
    ctx.current_linkage = linkage;

    // Process contents
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "declaration_list" => {
                // extern "C" { ... } block
                let mut block_cursor = child.walk();
                for block_child in child.children(&mut block_cursor) {
                    walk_top_level(&block_child, src, ctx);
                }
            }
            "function_definition" | "declaration" => {
                // extern "C" void func(); (single declaration)
                walk_top_level(&child, src, ctx);
            }
            _ => {}
        }
    }

    // Restore previous linkage
    ctx.current_linkage = prev_linkage;
}

// ── Type extraction (class / struct / enum) ──────────────────────
//
// CPP-SPAN-FIDELITY-1. One path for every class/struct/enum, whether the parse
// is clean (`class Foo {…}`) or ERROR-recovered from a macro-decorated form
// (`class DLL_LINKAGE Foo : Base {…}` → a `declaration`/`function_definition`
// wrapping a truncated specifier fragment). The keyword drives the kind; the
// name is the last identifier of the header (macros recorded, never the name);
// the span comes from balanced-brace recovery, never a tree ERROR extent; and
// definitions the parser swallowed into an over-extended body are recovered as
// siblings under their true scope.

/// The first `class_specifier`/`struct_specifier`/`enum_specifier` among a
/// construct's children, with the keyword-derived subtype. Presence of one under
/// a `declaration`/`function_definition` is the signal that the construct is a
/// TYPE (possibly macro-mangled), not a function or a plain declaration.
fn leading_type_specifier<'a>(
    node: &tree_sitter::Node<'a>,
) -> Option<(tree_sitter::Node<'a>, NodeSubtype)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_specifier" => return Some((child, NodeSubtype::Class)),
            "struct_specifier" => return Some((child, NodeSubtype::Struct)),
            "enum_specifier" => return Some((child, NodeSubtype::Enum)),
            _ => {}
        }
    }
    None
}

/// Emit a class/struct/enum node. `frag` is the specifier fragment (drives the
/// kind + header start); `construct` is the enclosing node whose byte span holds
/// the full header + body (`frag` itself when the parse is clean).
fn extract_type(
    frag: &tree_sitter::Node,
    construct: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    subtype: NodeSubtype,
) {
    let (resolved_name, macros) = type_name_and_macros(frag, construct, src);
    let type_name = resolved_name.unwrap_or_else(|| match subtype {
        NodeSubtype::Struct => "anon_struct".to_string(),
        NodeSubtype::Enum => "anon_enum".to_string(),
        _ => "anon_class".to_string(),
    });

    let qualified_name = ctx.qualified_name(&type_name);
    let stable_key = ctx.make_stable_key(&qualified_name, &subtype);
    let linkage_meta = ctx.linkage_metadata();

    // Span: balanced-brace recovery from source. `true_close` is the byte just
    // past the real closing `}` — the boundary between the body's own members
    // and any definitions the parser swallowed past it.
    let (location, true_close) = type_span_and_body_close(frag, construct, src);

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name: type_name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location,
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(construct, src),
        metadata_json: type_metadata_json(&linkage_meta, &macros),
    });

    // Base classes (IMPLEMENTS). Only recoverable when the base clause parsed as
    // a `base_class_clause` node; on a macro-mangled construct the base tokens
    // sit inside ERROR nodes and are not modeled (unchanged from before).
    extract_base_classes(construct, src, ctx);

    // enums carry no members we extract.
    if subtype == NodeSubtype::Enum {
        return;
    }

    // Process the body. The body node is the specifier's `body` field when clean,
    // else the sibling `compound_statement`/`initializer_list` the parser used
    // for the mangled form.
    if let Some(body) = type_body_node(frag, construct) {
        let prev_class = ctx.current_class.take();
        ctx.current_class = Some(type_name);

        let mut current_visibility = if subtype == NodeSubtype::Class {
            Visibility::Private // C++ class default
        } else {
            Visibility::Export // C++ struct default is public
        };

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            // Members past the real closing brace are not members — they are
            // sibling definitions the parser swallowed. Skip here; recover below.
            if let Some(close) = true_close {
                if child.start_byte() >= close {
                    continue;
                }
            }
            match child.kind() {
                "access_specifier" => {
                    current_visibility = parse_access_specifier(&child, src);
                }
                "function_definition" => {
                    extract_method(construct, &child, src, ctx, current_visibility);
                }
                "field_declaration" => {
                    if has_function_declarator(&child) {
                        extract_method_declaration(&child, src, ctx, current_visibility);
                    }
                }
                "declaration" => {
                    // Nested class/struct/enum defined directly in the body.
                    let mut decl_cursor = child.walk();
                    for decl_child in child.children(&mut decl_cursor) {
                        match decl_child.kind() {
                            "class_specifier" => {
                                extract_type(&decl_child, &decl_child, src, ctx, NodeSubtype::Class)
                            }
                            "struct_specifier" => extract_type(
                                &decl_child,
                                &decl_child,
                                src,
                                ctx,
                                NodeSubtype::Struct,
                            ),
                            "enum_specifier" => {
                                extract_type(&decl_child, &decl_child, src, ctx, NodeSubtype::Enum)
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        ctx.current_class = prev_class;

        // Recover definitions swallowed past the real closing brace. They live
        // deep inside `body` as well-formed specifier nodes (proven by the parse
        // dump); dispatch each at THIS enclosing scope (current_class already
        // restored), not as members of this type.
        if let Some(close) = true_close {
            if body.end_byte() > close {
                recover_swallowed_definitions(&body, close, src, ctx);
            }
        }
    }
}

/// Header name + macro tokens for a type. Scans source from the keyword to the
/// first `{` / `:` (base or enum-base) / `;`, collecting identifier tokens. The
/// LAST identifier is the name; the preceding ones are decoration macros
/// (`DLL_LINKAGE`, `LEVELDB_EXPORT`, …). `None` name → anonymous.
fn type_name_and_macros(
    frag: &tree_sitter::Node,
    construct: &tree_sitter::Node,
    src: &[u8],
) -> (Option<String>, Vec<String>) {
    let start = frag.start_byte();
    let end = construct.end_byte().min(src.len());
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut i = start;
    while i < end {
        // A decl may carry a comment (or, defensively, a string/char) between the
        // macro and the name; reuse the shared non-code skipper so a `{`/`:` inside
        // one never ends the header early.
        if let Some(next) = skip_noncode(src, i, end) {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            i = next;
            continue;
        }
        let c = src[i];
        if c.is_ascii_alphanumeric() || c == b'_' {
            cur.push(c as char);
            i += 1;
            continue;
        }
        if !cur.is_empty() {
            tokens.push(std::mem::take(&mut cur));
        }
        match c {
            b'{' | b';' => break,
            b':' => {
                // `::` keeps a qualified name together; a lone `:` opens the base
                // clause (class) / underlying-type (enum) → header ends.
                if i + 1 < end && src[i + 1] == b':' {
                    i += 2;
                } else {
                    break;
                }
            }
            _ => i += 1,
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }

    // Drop the leading keyword and structural words; what remains is
    // [macro… , Name].
    tokens.retain(|t| !matches!(t.as_str(), "class" | "struct" | "enum" | "final"));
    match tokens.pop() {
        Some(name) => (Some(name), tokens),
        None => (None, Vec::new()),
    }
}

/// The body node whose braces delimit a type's members: the specifier's `body`
/// field when the parse is clean, else the sibling block the parser attached to
/// the mangled construct.
fn type_body_node<'a>(
    frag: &tree_sitter::Node<'a>,
    construct: &tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    if let Some(body) = frag.child_by_field_name("body") {
        return Some(body);
    }
    let mut cursor = construct.walk();
    for child in construct.children(&mut cursor) {
        if matches!(
            child.kind(),
            "field_declaration_list"
                | "compound_statement"
                | "initializer_list"
                | "enumerator_list"
        ) {
            return Some(child);
        }
    }
    None
}

/// Compute the type's source span and the byte just past its real closing brace.
///
/// Honesty rule (spec §2.3): a span NEVER takes a tree ERROR/over-extended
/// extent. From the keyword we find the opening `{` and balance-match its `}` in
/// source (skipping strings/chars/comments). Balanced → `[keyword_line,
/// close_line]`. A `;` before any `{` → forward/opaque declaration → the single
/// declaration line. Unbalanced braces (genuinely unparseable) → NO span: the
/// declaration is emitted as a visible absence, never a guessed range.
fn type_span_and_body_close(
    frag: &tree_sitter::Node,
    construct: &tree_sitter::Node,
    src: &[u8],
) -> (Option<SourceLocation>, Option<usize>) {
    let start_byte = frag.start_byte();
    let start_line = (frag.start_position().row + 1) as i64;
    let start_col = frag.start_position().column as i64;
    let end = construct.end_byte().min(src.len());

    // Locate the body opener, or a `;` proving there is no body.
    match find_body_open_or_terminator(src, start_byte, end) {
        BodyProbe::ForwardDecl { semi } => {
            let line = line_of(src, semi) as i64;
            (
                Some(SourceLocation {
                    line_start: start_line,
                    col_start: start_col,
                    line_end: line,
                    col_end: 0,
                }),
                None,
            )
        }
        BodyProbe::Body { open } => match balanced_brace_end(src, open, src.len()) {
            Some(close_byte) => {
                let end_line = line_of(src, close_byte.saturating_sub(1)) as i64;
                (
                    Some(SourceLocation {
                        line_start: start_line,
                        col_start: start_col,
                        line_end: end_line,
                        col_end: 0,
                    }),
                    Some(close_byte),
                )
            }
            // Unbalanced → honest absence, no swallowing span.
            None => (None, None),
        },
        // No body and no terminator in range: a forward/opaque declaration whose
        // `;` sits just outside the specifier node (`class Foo;`). Its location is
        // known and tight — the fragment's own extent — so this is NOT the
        // unparseable case; emit that span rather than a false absence.
        BodyProbe::None => (
            Some(SourceLocation {
                line_start: start_line,
                col_start: start_col,
                line_end: (frag.end_position().row + 1) as i64,
                col_end: frag.end_position().column as i64,
            }),
            None,
        ),
    }
}

enum BodyProbe {
    /// A `{` opens the body at this byte.
    Body { open: usize },
    /// A `;` closed the declaration before any `{` (forward/opaque decl).
    ForwardDecl { semi: usize },
    /// Neither found within range.
    None,
}

/// Scan for the first `{` (body opener) or `;` (no body), skipping strings,
/// char literals, and comments so a brace inside them never counts.
fn find_body_open_or_terminator(src: &[u8], from: usize, end: usize) -> BodyProbe {
    let mut i = from;
    while i < end {
        if let Some(next) = skip_noncode(src, i, end) {
            i = next;
            continue;
        }
        match src[i] {
            b'{' => return BodyProbe::Body { open: i },
            b';' => return BodyProbe::ForwardDecl { semi: i },
            _ => i += 1,
        }
    }
    BodyProbe::None
}

/// Byte just past the `}` matching the `{` at `open`. Counts brace depth over
/// code only (strings/chars/comments skipped). `None` if depth never returns to
/// zero within `end` — the genuinely-unparseable case.
fn balanced_brace_end(src: &[u8], open: usize, end: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < end {
        if let Some(next) = skip_noncode(src, i, end) {
            i = next;
            continue;
        }
        match src[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// If `src[i]` begins a non-code region (line/block comment, string/char literal,
/// or C++ raw string literal), return the byte index just past it; else `None`.
///
/// Raw strings (`R"delim( ... )delim"`) are special-cased: their payload is
/// verbatim, so an embedded `"` or `}` must never be read as code — otherwise a
/// raw brace could falsely open or close a class span (reviewer repro:
/// `R"(text " } text)"`). See `skip_raw_string`.
fn skip_noncode(src: &[u8], i: usize, end: usize) -> Option<usize> {
    let c = src[i];
    if c == b'/' && i + 1 < end && src[i + 1] == b'/' {
        let mut j = i + 2;
        while j < end && src[j] != b'\n' {
            j += 1;
        }
        return Some(j);
    }
    if c == b'/' && i + 1 < end && src[i + 1] == b'*' {
        let mut j = i + 2;
        while j + 1 < end && !(src[j] == b'*' && src[j + 1] == b'/') {
            j += 1;
        }
        return Some((j + 2).min(end));
    }
    if c == b'"' {
        if let Some(after) = skip_raw_string(src, i, end) {
            return Some(after);
        }
    }
    if c == b'"' || c == b'\'' {
        let quote = c;
        let mut j = i + 1;
        while j < end {
            if src[j] == b'\\' {
                j += 2;
                continue;
            }
            if src[j] == quote {
                j += 1;
                break;
            }
            j += 1;
        }
        return Some(j);
    }
    None
}

/// If the `"` at `quote` opens a C++ raw string literal, return the byte just
/// past its closing `"`; else `None` (an ordinary string — the caller handles
/// it with the normal string rule).
///
/// Grammar: `(u8|u|U|L)? R "delim( ... )delim"`, where `delim` is the char
/// sequence between the opening `"` and the first `(` (a d-char sequence: no
/// `(`, `)`, `\`, or whitespace), and the literal ends at the first `)delim"`.
/// The `R` must sit at a token boundary — `fooR"x"` is the identifier `fooR`
/// followed by an ordinary string, not a raw string.
///
/// An unterminated raw payload consumes to `end`; the enclosing brace scan then
/// never balances and the class is emitted as a visible absence (honest — never
/// a guessed span), consistent with the §2.3 no-span fallback.
fn skip_raw_string(src: &[u8], quote: usize, end: usize) -> Option<usize> {
    // Require an `R` immediately before the quote.
    if quote == 0 || src[quote - 1] != b'R' {
        return None;
    }
    // Walk back over an optional encoding prefix (`u8` | `u` | `U` | `L`) to find
    // the token start, then require a boundary (start of buffer or a non-ident
    // byte) so an identifier ending in `R` is not misread as a raw prefix.
    let mut token_start = quote - 1; // the `R`
    if token_start >= 1 {
        match src[token_start - 1] {
            b'L' | b'u' | b'U' => token_start -= 1,
            b'8' if token_start >= 2 && src[token_start - 2] == b'u' => token_start -= 2,
            _ => {}
        }
    }
    let at_boundary = token_start == 0
        || !(src[token_start - 1].is_ascii_alphanumeric() || src[token_start - 1] == b'_');
    if !at_boundary {
        return None;
    }
    // Delimiter: bytes between the opening `"` and the first `(`.
    let delim_start = quote + 1;
    let mut p = delim_start;
    while p < end && src[p] != b'(' {
        // A d-char is not `)`, `\`, or whitespace; if one appears before `(`,
        // this is not a well-formed raw string — defer to the ordinary rule.
        if src[p] == b')' || src[p] == b'\\' || src[p].is_ascii_whitespace() {
            return None;
        }
        p += 1;
    }
    if p >= end {
        return None; // no opening `(` → not a raw string
    }
    let delim = &src[delim_start..p]; // possibly empty: R"( ... )"
                                      // Terminator: `)` + delim + `"`.
    let mut k = p + 1;
    while k < end {
        if src[k] == b')' {
            let after_paren = k + 1;
            if src.get(after_paren..after_paren + delim.len()) == Some(delim)
                && src.get(after_paren + delim.len()) == Some(&b'"')
            {
                return Some(after_paren + delim.len() + 1);
            }
        }
        k += 1;
    }
    // Unterminated raw payload runs to the end of the buffer.
    Some(end)
}

/// 1-based line number containing byte offset `pos`.
fn line_of(src: &[u8], pos: usize) -> usize {
    let upto = pos.min(src.len());
    1 + src[..upto].iter().filter(|&&b| b == b'\n').count()
}

/// Merge the linkage blob with the recorded macro decoration tokens into the
/// symbol's single `metadata_json`. Additive: with no macros and no linkage the
/// result is `None` (byte-identical to a plain type before this slice); macros
/// are recorded under `macro_tokens` alongside any linkage facts.
fn type_metadata_json(linkage: &LinkageMetadata, macros: &[String]) -> Option<String> {
    let linkage_json = linkage.to_json();
    if macros.is_empty() {
        return linkage_json;
    }
    let mut obj = match linkage_json
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
    {
        Some(Ok(serde_json::Value::Object(map))) => map,
        _ => serde_json::Map::new(),
    };
    obj.insert(
        "macro_tokens".to_string(),
        serde_json::Value::Array(
            macros
                .iter()
                .map(|m| serde_json::Value::String(m.clone()))
                .collect(),
        ),
    );
    serde_json::to_string(&serde_json::Value::Object(obj)).ok()
}

/// Recover definitions the parser swallowed into an over-extended body. They
/// survive as well-formed specifier nodes deep in `body`; find each whose start
/// is at/after the real closing brace and dispatch it at the current scope.
/// Does NOT descend into a dispatched node — `extract_type` handles its members
/// (and its own nested recovery) itself.
fn recover_swallowed_definitions(
    body: &tree_sitter::Node,
    true_close: usize,
    src: &[u8],
    ctx: &mut ExtractionCtx,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.end_byte() <= true_close {
            continue; // entirely within the real body — a genuine member
        }
        match child.kind() {
            "class_specifier" if child.start_byte() >= true_close => {
                extract_type(&child, &child, src, ctx, NodeSubtype::Class);
            }
            "struct_specifier" if child.start_byte() >= true_close => {
                extract_type(&child, &child, src, ctx, NodeSubtype::Struct);
            }
            "enum_specifier" if child.start_byte() >= true_close => {
                extract_type(&child, &child, src, ctx, NodeSubtype::Enum);
            }
            "function_definition" if child.start_byte() >= true_close => {
                if let Some((f, st)) = leading_type_specifier(&child) {
                    extract_type(&f, &child, src, ctx, st);
                } else {
                    extract_function(&child, src, ctx);
                }
            }
            "declaration" if child.start_byte() >= true_close => {
                extract_declaration(&child, src, ctx);
            }
            // A wrapper node (field_declaration_list, ERROR, …) straddling the
            // boundary: descend to reach the definitions nested inside it.
            _ => recover_swallowed_definitions(&child, true_close, src, ctx),
        }
    }
}

fn parse_access_specifier(node: &tree_sitter::Node, src: &[u8]) -> Visibility {
    let text = node.utf8_text(src).unwrap_or("");
    if text.contains("private") {
        Visibility::Private
    } else if text.contains("protected") {
        Visibility::Protected
    } else {
        Visibility::Export // public
    }
}

fn has_function_declarator(node: &tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declarator" {
            return true;
        }
    }
    false
}

// ── Base class extraction ────────────────────────────────────────

fn extract_base_classes(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "base_class_clause" {
            extract_base_clause(&child, src, ctx);
        }
    }
}

fn extract_base_clause(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // base_class_clause contains type specifiers for each base
        if child.kind() == "type_identifier"
            || child.kind() == "qualified_identifier"
            || child.kind() == "template_type"
        {
            let base_name = child.utf8_text(src).unwrap_or("").to_string();
            if !base_name.is_empty() {
                // Determine access specifier (look for preceding access_specifier)
                let access = find_base_access_specifier(node, &child, src);
                let is_virtual = is_virtual_base(node, &child);

                let metadata = serde_json::json!({
                    "access_specifier": match access {
                        Visibility::Private => "private",
                        Visibility::Protected => "protected",
                        _ => "public",
                    },
                    "is_virtual": is_virtual
                });

                ctx.edges.push(ExtractedEdge {
                    edge_uid: uuid::Uuid::new_v4().to_string(),
                    snapshot_uid: ctx.snapshot_uid.into(),
                    repo_uid: ctx.repo_uid.into(),
                    source_node_uid: ctx.file_node_uid.into(),
                    target_key: base_name,
                    edge_type: EdgeType::Implements,
                    resolution: Resolution::Static,
                    extractor: EXTRACTOR_NAME.into(),
                    location: Some(location_from_node(&child)),
                    metadata_json: Some(metadata.to_string()),
                });
            }
        }
    }
}

fn find_base_access_specifier(
    _clause: &tree_sitter::Node,
    _type_node: &tree_sitter::Node,
    _src: &[u8],
) -> Visibility {
    // TODO: Properly parse access specifier before the type
    // For now, default to public (most common)
    Visibility::Export
}

fn is_virtual_base(_clause: &tree_sitter::Node, _type_node: &tree_sitter::Node) -> bool {
    // TODO: Check for virtual keyword before the type
    false
}

// ── Typedef / Type alias extraction ──────────────────────────────

fn extract_typedef(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // Find the type_identifier being defined
    let name = find_typedef_name(node, src);
    if name.is_empty() {
        return;
    }

    let qualified_name = ctx.qualified_name(&name);
    let stable_key = ctx.make_stable_key(&qualified_name, &NodeSubtype::TypeAlias);
    let linkage_meta = ctx.linkage_metadata();

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::TypeAlias),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: linkage_meta.to_json(),
    });
}

fn find_typedef_name(node: &tree_sitter::Node, src: &[u8]) -> String {
    // Look for type_identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            return child.utf8_text(src).unwrap_or("").to_string();
        }
        if child.kind() == "type_definition" || child.kind() == "declaration" {
            let inner = find_typedef_name(&child, src);
            if !inner.is_empty() {
                return inner;
            }
        }
    }

    // Check declarator
    if let Some(declarator) = node.child_by_field_name("declarator") {
        return extract_declarator_name(&declarator, src);
    }

    String::new()
}

// ── Declaration extraction ───────────────────────────────────────

fn extract_declaration(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // A declaration that leads with a class/struct/enum specifier is a type
    // definition (`class Foo;`, `struct Point {} p;`) OR a macro-decorated type
    // ERROR-recovered into declaration shape (`class DLL_LINKAGE HeroClass : …`).
    // Route to the type path: `node` is the construct (carries the real name +
    // body as siblings of the fragment), `frag` is the specifier fragment.
    if let Some((frag, subtype)) = leading_type_specifier(node) {
        extract_type(&frag, node, src, ctx, subtype);
    }
}

// ── Function extraction ──────────────────────────────────────────

fn extract_function(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let declarator = match node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };

    let (name, qualified_prefix) = extract_function_name(&declarator, src);
    if name.is_empty() {
        return;
    }

    // Check for static
    let is_static = node.children(&mut node.walk()).any(|c| {
        c.kind() == "storage_class_specifier" && c.utf8_text(src).unwrap_or("") == "static"
    });

    // Determine if this is a method (has Class:: prefix or inside class)
    let effective_class = qualified_prefix.or_else(|| ctx.current_class.clone());
    let is_method = effective_class.is_some();

    // Determine subtype
    let (subtype, func_name) = if is_method {
        let class_name = effective_class.as_ref().unwrap();
        if name == *class_name {
            (NodeSubtype::Constructor, name.clone())
        } else if name.starts_with('~') && name[1..] == *class_name {
            (NodeSubtype::Destructor, name.clone())
        } else {
            (NodeSubtype::Method, name.clone())
        }
    } else {
        (NodeSubtype::Function, name.clone())
    };

    // Build qualified name
    let qualified_name = if let Some(ref class) = effective_class {
        if ctx.current_class.is_some() {
            // Already inside class, use context
            ctx.qualified_name(&func_name)
        } else {
            // Out-of-line definition with Class:: prefix
            let mut parts: Vec<&str> = ctx.current_namespace.iter().map(|s| s.as_str()).collect();
            parts.push(class);
            parts.push(&func_name);
            parts.join("::")
        }
    } else {
        ctx.qualified_name(&func_name)
    };

    let stable_key = ctx.make_stable_key(&qualified_name, &subtype);
    let func_uid = uuid::Uuid::new_v4().to_string();
    let linkage_meta = ctx.linkage_metadata();

    // Build signature
    let params = declarator.child_by_field_name("parameters");
    let signature =
        params.map(|p| format!("{}{}", qualified_name, p.utf8_text(src).unwrap_or("()")));

    let visibility = if is_static {
        Visibility::Private
    } else {
        Visibility::Export
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: func_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: stable_key.clone(),
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name: func_name,
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature,
        visibility: Some(visibility),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: linkage_meta.to_json(),
    });

    // Extract calls and compute metrics
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_body(&body, src, &func_uid, ctx);
        let metrics = compute_function_metrics(&body, params.as_ref());
        ctx.metrics.insert(stable_key, metrics);
    }
}

// ── Method extraction (inside class) ─────────────────────────────

fn extract_method(
    _class_node: &tree_sitter::Node,
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    visibility: Visibility,
) {
    let declarator = match node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };

    let (name, _) = extract_function_name(&declarator, src);
    if name.is_empty() {
        return;
    }

    let class_name = match &ctx.current_class {
        Some(c) => c.clone(),
        None => return,
    };

    // Determine subtype
    let subtype = if name == class_name {
        NodeSubtype::Constructor
    } else if name.starts_with('~') && name[1..] == class_name {
        NodeSubtype::Destructor
    } else {
        NodeSubtype::Method
    };

    let qualified_name = ctx.qualified_name(&name);
    let stable_key = ctx.make_stable_key(&qualified_name, &subtype);
    let func_uid = uuid::Uuid::new_v4().to_string();
    let linkage_meta = ctx.linkage_metadata();

    let params = declarator.child_by_field_name("parameters");
    let signature =
        params.map(|p| format!("{}{}", qualified_name, p.utf8_text(src).unwrap_or("()")));

    ctx.nodes.push(ExtractedNode {
        node_uid: func_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: stable_key.clone(),
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name,
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature,
        visibility: Some(visibility),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: linkage_meta.to_json(),
    });

    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_body(&body, src, &func_uid, ctx);
        let metrics = compute_function_metrics(&body, params.as_ref());
        ctx.metrics.insert(stable_key, metrics);
    }
}

fn extract_method_declaration(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    visibility: Visibility,
) {
    // Find function_declarator
    let declarator = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "function_declarator");

    let declarator = match declarator {
        Some(d) => d,
        None => return,
    };

    let (name, _) = extract_function_name(&declarator, src);
    if name.is_empty() {
        return;
    }

    let class_name = match &ctx.current_class {
        Some(c) => c.clone(),
        None => return,
    };

    let subtype = if name == class_name {
        NodeSubtype::Constructor
    } else if name.starts_with('~') && name[1..] == class_name {
        NodeSubtype::Destructor
    } else {
        NodeSubtype::Method
    };

    let qualified_name = ctx.qualified_name(&name);
    let stable_key = ctx.make_stable_key(&qualified_name, &subtype);
    let linkage_meta = ctx.linkage_metadata();

    let params = declarator.child_by_field_name("parameters");
    let signature =
        params.map(|p| format!("{}{}", qualified_name, p.utf8_text(src).unwrap_or("()")));

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name,
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature,
        visibility: Some(visibility),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: linkage_meta.to_json(),
    });

    // No body for declarations, so no calls or metrics
}

// ── Call extraction ──────────────────────────────────────────────

fn extract_calls_from_body(
    body: &tree_sitter::Node,
    src: &[u8],
    source_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // CPP-SB-1 D3: Clear local type map at function boundary.
    ctx.local_stream_types.clear();

    fn walk(node: &tree_sitter::Node, src: &[u8], source_node_uid: &str, ctx: &mut ExtractionCtx) {
        match node.kind() {
            "call_expression" => {
                extract_call(node, src, source_node_uid, ctx);
            }
            "declaration" => {
                // CPP-SB-1: Check for stream constructor with path.
                try_extract_stream_declaration(node, src, source_node_uid, ctx);
            }
            // Don't recurse into nested functions or lambdas
            "function_definition" | "lambda_expression" => {
                return;
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(&child, src, source_node_uid, ctx);
        }
    }

    walk(body, src, source_node_uid, ctx);
}

fn extract_call(
    node: &tree_sitter::Node,
    src: &[u8],
    source_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    let function = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };

    // Extract callee based on type
    let target_name = match function.kind() {
        "identifier" => {
            let fn_name = function.utf8_text(src).unwrap_or("").to_string();
            // CPP-SB-1: Check for C-style API state-boundary call.
            if let Some(callsite) = try_resolve_c_style_api(node, src, source_node_uid, &fn_name) {
                ctx.resolved_callsites.push(callsite);
            }
            fn_name
        }
        "qualified_identifier" | "scoped_identifier" => {
            function.utf8_text(src).unwrap_or("").to_string()
        }
        "field_expression" => {
            // obj.method() or ptr->method()
            // CPP-SB-1 D3: Check for stream .open() call.
            if let Some(callsite) =
                try_resolve_stream_open(node, &function, src, source_node_uid, ctx)
            {
                ctx.resolved_callsites.push(callsite);
            }

            if let Some(field) = function.child_by_field_name("field") {
                field.utf8_text(src).unwrap_or("").to_string()
            } else {
                return;
            }
        }
        "template_function" => {
            // func<T>() - extract the base function name
            if let Some(name) = function.child_by_field_name("name") {
                name.utf8_text(src).unwrap_or("").to_string()
            } else {
                return;
            }
        }
        _ => return, // Skip function pointers, etc.
    };

    if target_name.is_empty() {
        return;
    }

    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: source_node_uid.into(),
        target_key: target_name.clone(),
        edge_type: EdgeType::Calls,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(location_from_node(node)),
        metadata_json: Some(serde_json::json!({ "calleeName": target_name }).to_string()),
    });
}

// ── Helper: extract function name from declarator ────────────────

fn extract_function_name(declarator: &tree_sitter::Node, src: &[u8]) -> (String, Option<String>) {
    let mut current = *declarator;

    // Unwrap function_declarator, pointer_declarator, reference_declarator
    while matches!(
        current.kind(),
        "function_declarator" | "pointer_declarator" | "reference_declarator"
    ) {
        if let Some(inner) = current.child_by_field_name("declarator") {
            current = inner;
        } else {
            break;
        }
    }

    match current.kind() {
        "identifier" | "field_identifier" => {
            (current.utf8_text(src).unwrap_or("").to_string(), None)
        }
        "qualified_identifier" | "scoped_identifier" => {
            // Class::method or ns::func
            let scope = current.child_by_field_name("scope");
            let name = current.child_by_field_name("name");

            let name_str = name
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("")
                .to_string();
            let prefix = scope
                .and_then(|s| s.utf8_text(src).ok())
                .map(|s| s.trim_end_matches("::").to_string());

            (name_str, prefix)
        }
        "destructor_name" => (current.utf8_text(src).unwrap_or("").to_string(), None),
        _ => {
            // Try to find identifier child
            let mut cursor = current.walk();
            for child in current.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "field_identifier" {
                    return (child.utf8_text(src).unwrap_or("").to_string(), None);
                }
            }
            (String::new(), None)
        }
    }
}

fn extract_declarator_name(declarator: &tree_sitter::Node, src: &[u8]) -> String {
    if declarator.kind() == "type_identifier" || declarator.kind() == "identifier" {
        return declarator.utf8_text(src).unwrap_or("").to_string();
    }

    if let Some(inner) = declarator.child_by_field_name("declarator") {
        return extract_declarator_name(&inner, src);
    }

    let mut cursor = declarator.walk();
    for child in declarator.children(&mut cursor) {
        if child.kind() == "type_identifier" || child.kind() == "identifier" {
            return child.utf8_text(src).unwrap_or("").to_string();
        }
    }

    String::new()
}

// ── CPP-SB-1: State-boundary extraction ──────────────────────────

/// Try to resolve a C-style API call (fopen, open, sqlite3_open*) to a ResolvedCallsite.
fn try_resolve_c_style_api(
    call_node: &tree_sitter::Node,
    src: &[u8],
    enclosing_symbol_node_uid: &str,
    fn_name: &str,
) -> Option<ResolvedCallsite> {
    // Determine synthetic module and base symbol.
    let (resolved_module, base_symbol) = if SB_STDIO_FUNCTIONS.contains(&fn_name) {
        ("libc:stdio", fn_name)
    } else if SB_FCNTL_FUNCTIONS.contains(&fn_name) {
        ("libc:fcntl", fn_name)
    } else if SB_SQLITE_FUNCTIONS.contains(&fn_name) {
        ("sqlite3", fn_name)
    } else {
        return None;
    };

    // Get arguments node.
    let arguments = call_node.child_by_field_name("arguments")?;

    // Extract arg0 (path) — must be a string literal.
    let arg0_payload = extract_arg0_string_literal(&arguments, src)?;

    // Extract arg1 if present and needed for mode/flags.
    let arg1_payload = extract_arg1_payload(&arguments, src);

    // Determine direction-specific resolved_symbol.
    let resolved_symbol = match resolved_module {
        "libc:stdio" => {
            // fopen: use mode argument to determine direction.
            let mode = match &arg1_payload {
                Some(CallArgPayload::StringLiteral { value }) => value.as_str(),
                // Undetermined mode → empty → `unknown` (HONESTY-GATE-2 family 1).
                _ => "",
            };
            format!("{}_{}", base_symbol, normalize_fopen_mode(mode))
        }
        "libc:fcntl" => {
            // open: use flags argument to determine direction.
            let flags = match &arg1_payload {
                Some(CallArgPayload::StringLiteral { value }) => value.as_str(),
                // Dynamic/undetermined flags → empty → `unknown` (HONESTY-GATE-2 family 1).
                _ => "",
            };
            format!("{}_{}", base_symbol, normalize_open_flags(flags))
        }
        "sqlite3" => {
            // sqlite3_open*: always read_write.
            base_symbol.to_string()
        }
        _ => base_symbol.to_string(),
    };

    Some(ResolvedCallsite {
        enclosing_symbol_node_uid: enclosing_symbol_node_uid.to_string(),
        resolved_module: resolved_module.to_string(),
        resolved_symbol,
        arg0_payload,
        arg1_payload,
        source_location: location_from_node(call_node),
    })
}

/// CPP-SB-1 D3: Try to resolve a stream .open() call using the local type map.
fn try_resolve_stream_open(
    call_node: &tree_sitter::Node,
    function_node: &tree_sitter::Node,
    src: &[u8],
    enclosing_symbol_node_uid: &str,
    ctx: &ExtractionCtx,
) -> Option<ResolvedCallsite> {
    // Check if the field is "open".
    // HONESTY-GATE-2 (review-3): no `.ok()?` collapse on the structural read that
    // decides whether this is an access at all. An unreadable field name means we
    // cannot classify the call → emit no row (return None), never a guessed access.
    let field = function_node.child_by_field_name("field")?;
    let field_name = match field.utf8_text(src) {
        Ok(t) => t,
        Err(_) => return None,
    };
    if field_name != "open" {
        return None;
    }

    // Get the receiver (must be a simple identifier per D3 limits).
    let argument = function_node.child_by_field_name("argument")?;
    if argument.kind() != "identifier" {
        return None; // Not a simple identifier receiver (e.g., getStream().open())
    }
    // Same policy: an unreadable receiver name cannot be matched in the local type
    // map → no row, never a guessed stream type.
    let receiver_name = match argument.utf8_text(src) {
        Ok(t) => t,
        Err(_) => return None,
    };

    // Look up receiver in local type map.
    let stream_type = ctx.local_stream_types.get(receiver_name)?;

    // Get arguments node.
    let arguments = call_node.child_by_field_name("arguments")?;

    // Extract path argument (arg0).
    let arg0_payload = extract_arg0_string_literal(&arguments, src)?;

    // Extract mode argument (arg1) if present — for fstream mode parsing.
    let arg1_payload = extract_arg1_ios_mode(&arguments, src);

    // Determine resolved symbol based on stream type and mode.
    let resolved_symbol = match stream_type {
        StreamType::Ifstream => stream_type.open_symbol().to_string(),
        StreamType::Ofstream => stream_type.open_symbol().to_string(),
        StreamType::Fstream => {
            // Parse mode flags if present.
            if let Some(CallArgPayload::StringLiteral { value }) = arg1_payload.as_ref() {
                normalize_ios_mode_to_fstream_symbol(value)
            } else {
                // No explicit mode argument. C++ contract fixes the direction:
                // std::basic_fstream::open's openmode defaults to
                // ios_base::in | ios_base::out (read_write). Contract-fixed, so
                // classified — not a guess (operator steering: stdlib DEFAULTS may
                // stay classified where the API contract fixes direction).
                stream_type.open_symbol().to_string()
            }
        }
    };

    Some(ResolvedCallsite {
        enclosing_symbol_node_uid: enclosing_symbol_node_uid.to_string(),
        resolved_module: "std:fstream".to_string(),
        resolved_symbol,
        arg0_payload,
        arg1_payload,
        source_location: location_from_node(call_node),
    })
}

/// CPP-SB-1: Try to extract stream declaration and track in local type map.
/// If the declaration has a path argument, emit a ResolvedCallsite.
fn try_extract_stream_declaration(
    decl_node: &tree_sitter::Node,
    src: &[u8],
    enclosing_symbol_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // Look for type specifier that matches stream types.
    let type_text = extract_declaration_type(decl_node, src);
    let stream_type = match StreamType::from_type_text(&type_text) {
        Some(st) => st,
        None => return, // Not a stream type.
    };

    // Find init_declarator(s) to get variable name and potential path argument.
    let mut cursor = decl_node.walk();
    for child in decl_node.children(&mut cursor) {
        if child.kind() == "init_declarator" {
            if let Some((var_name, path_arg, mode_arg)) = extract_init_declarator_info(&child, src)
            {
                // Record in local type map (D3).
                ctx.local_stream_types.insert(var_name.clone(), stream_type);

                // If there's a path argument, emit ResolvedCallsite.
                if let Some(path) = path_arg {
                    let resolved_symbol =
                        if let Some(CallArgPayload::StringLiteral { value }) = mode_arg.as_ref() {
                            match stream_type {
                                StreamType::Fstream => normalize_ios_mode_to_fstream_symbol(value),
                                _ => stream_type.constructor_symbol().to_string(),
                            }
                        } else {
                            // No explicit mode argument. Contract-fixed default
                            // openmode per stream type (ifstream=in, ofstream=out,
                            // fstream=in|out) — classified, not guessed.
                            stream_type.constructor_symbol().to_string()
                        };

                    ctx.resolved_callsites.push(ResolvedCallsite {
                        enclosing_symbol_node_uid: enclosing_symbol_node_uid.to_string(),
                        resolved_module: "std:fstream".to_string(),
                        resolved_symbol,
                        arg0_payload: path,
                        arg1_payload: mode_arg,
                        source_location: location_from_node(&child),
                    });
                }
            }
        }
        // Also handle simple declarator without initializer (for type map tracking).
        else if child.kind() == "identifier" {
            let var_name = child.utf8_text(src).unwrap_or("").to_string();
            if !var_name.is_empty() {
                ctx.local_stream_types.insert(var_name, stream_type);
            }
        }
    }
}

/// Extract the type specifier text from a declaration.
fn extract_declaration_type(decl_node: &tree_sitter::Node, src: &[u8]) -> String {
    let mut cursor = decl_node.walk();
    for child in decl_node.children(&mut cursor) {
        match child.kind() {
            "qualified_identifier" | "scoped_identifier" | "type_identifier" => {
                return child.utf8_text(src).unwrap_or("").to_string();
            }
            "template_type" => {
                // e.g., std::basic_ifstream<char> — extract base type
                if let Some(name) = child.child_by_field_name("name") {
                    return name.utf8_text(src).unwrap_or("").to_string();
                }
            }
            _ => {}
        }
    }
    String::new()
}

/// Extract variable name and optional path/mode arguments from init_declarator.
/// Returns (var_name, path_payload, mode_payload).
fn extract_init_declarator_info(
    init_decl: &tree_sitter::Node,
    src: &[u8],
) -> Option<(String, Option<CallArgPayload>, Option<CallArgPayload>)> {
    let mut var_name = String::new();
    let mut path_payload = None;
    let mut mode_payload = None;

    let mut cursor = init_decl.walk();
    for child in init_decl.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                var_name = child.utf8_text(src).unwrap_or("").to_string();
            }
            "argument_list" => {
                // Constructor arguments: (path) or (path, mode)
                path_payload = extract_arg0_string_literal(&child, src);
                mode_payload = extract_arg1_ios_mode(&child, src);
            }
            "initializer_list" => {
                // Brace initialization: {path} or {path, mode}
                path_payload = extract_arg0_string_literal(&child, src);
                mode_payload = extract_arg1_ios_mode(&child, src);
            }
            _ => {}
        }
    }

    if var_name.is_empty() {
        return None;
    }

    Some((var_name, path_payload, mode_payload))
}

/// Extract arg0 as a string literal CallArgPayload.
fn extract_arg0_string_literal(
    arguments: &tree_sitter::Node,
    src: &[u8],
) -> Option<CallArgPayload> {
    let mut cursor = arguments.walk();
    let mut arg_index = 0;

    for child in arguments.children(&mut cursor) {
        // Skip punctuation.
        if matches!(child.kind(), "(" | ")" | "{" | "}" | "," | "comment") {
            continue;
        }

        if arg_index == 0 {
            if child.kind() == "string_literal" {
                // HONESTY-GATE-2 (review-2): no `.ok()?` collapse on a read
                // feeding a rendered resource row. An unreadable path literal
                // is NOT path evidence → emit no row (return None).
                let text = match child.utf8_text(src) {
                    Ok(t) => t,
                    Err(_) => return None,
                };
                let value = text.trim_matches('"').to_string();
                return Some(CallArgPayload::StringLiteral { value });
            }
            // Not a string literal → dynamic path, skip.
            return None;
        }
        arg_index += 1;
    }
    None
}

/// Extract arg1 for fopen/open — either string literal or identifier.
fn extract_arg1_payload(arguments: &tree_sitter::Node, src: &[u8]) -> Option<CallArgPayload> {
    let mut cursor = arguments.walk();
    let mut arg_index = 0;

    for child in arguments.children(&mut cursor) {
        if matches!(child.kind(), "(" | ")" | "," | "comment") {
            continue;
        }

        if arg_index == 1 {
            // arg1 CLASSIFIES the access direction (fopen mode / open flags).
            // HONESTY-GATE-2 (review-2): no `.ok()?` collapse — an unreadable
            // mode is undetermined, so return None; the caller maps a missing
            // arg1 to `unknown`, never a guessed direction.
            if child.kind() == "string_literal" {
                let text = match child.utf8_text(src) {
                    Ok(t) => t,
                    Err(_) => return None,
                };
                let value = text.trim_matches('"').to_string();
                return Some(CallArgPayload::StringLiteral { value });
            } else if child.kind() == "identifier" {
                // For open(), flags are identifiers like O_RDONLY.
                let value = match child.utf8_text(src) {
                    Ok(t) => t.to_string(),
                    Err(_) => return None,
                };
                return Some(CallArgPayload::StringLiteral { value });
            }
            return None;
        }
        arg_index += 1;
    }
    None
}

/// Extract arg1 for std::ios mode flags.
/// Captures the text of the mode expression for pattern matching.
fn extract_arg1_ios_mode(arguments: &tree_sitter::Node, src: &[u8]) -> Option<CallArgPayload> {
    let mut cursor = arguments.walk();
    let mut arg_index = 0;

    for child in arguments.children(&mut cursor) {
        if matches!(child.kind(), "(" | ")" | "{" | "}" | "," | "comment") {
            continue;
        }

        if arg_index == 1 {
            // Capture the entire expression text for mode parsing.
            // HONESTY-GATE-2 (review-2): no `.ok()?` collapse — an unreadable
            // mode expression is undetermined (return None), never silently
            // decoded.
            let text = match child.utf8_text(src) {
                Ok(t) => t.to_string(),
                Err(_) => return None,
            };
            return Some(CallArgPayload::StringLiteral { value: text });
        }
        arg_index += 1;
    }
    None
}

/// Normalize fopen mode string to direction suffix.
///
/// Recognizes ONLY well-formed C `fopen` mode strings: the FIRST character
/// must be a base mode (`r`/`w`/`a`) and every trailing character must be a
/// recognized mode flag (`+`, `b`, `x`, or the glibc extensions
/// `e`/`m`/`c`/`l`). Direction: `'+'` → `read_write`; else `r` → `read`,
/// `w`/`a` → `write`.
///
/// HONESTY-GATE-2 family 1: an undetermined mode (missing, dynamic, a
/// non-mode token such as `"q+"` or `"r_not_a_mode"`, or a malformed form
/// that REPEATS a mode-flag character such as `"r++"` / `"rbb"`) is
/// `unknown`, NOT a guessed direction. A `'+'` is honored only inside an
/// otherwise valid mode string — never as a bare or repeated substring
/// match (review-2).
fn normalize_fopen_mode(mode: &str) -> &'static str {
    let mut chars = mode.chars();
    let base = match chars.next() {
        Some(c @ ('r' | 'w' | 'a')) => c,
        _ => return "unknown",
    };
    // Each recognized trailing mode-flag character appears at most once in a
    // well-formed fopen mode; a repetition is malformed → `unknown`.
    let mut has_plus = false;
    let (mut seen_b, mut seen_x, mut seen_e, mut seen_m, mut seen_c, mut seen_l) =
        (false, false, false, false, false, false);
    for c in chars {
        let seen = match c {
            '+' => &mut has_plus,
            'b' => &mut seen_b,
            'x' => &mut seen_x,
            'e' => &mut seen_e,
            'm' => &mut seen_m,
            'c' => &mut seen_c,
            'l' => &mut seen_l,
            _ => return "unknown",
        };
        if *seen {
            return "unknown";
        }
        *seen = true;
    }
    if has_plus {
        "read_write"
    } else if base == 'r' {
        "read"
    } else {
        "write"
    }
}

/// Normalize an `open()` flags identifier to a direction suffix.
///
/// HONESTY-GATE-2 family 1: undetermined flags are `unknown`, NOT a
/// guessed `read_write` (the default that fabricated hadoop's phantom
/// writers). Matches the EXACT access-mode flag identifier only — a
/// substring match would misclassify a near-match such as
/// `O_RDONLY_ALIAS` (STANDING RULE 2: never classify from a name
/// fragment). `read_write` is emitted only for an explicit `O_RDWR`.
fn normalize_open_flags(flags: &str) -> &'static str {
    match flags {
        "O_RDONLY" => "read",
        "O_WRONLY" => "write",
        "O_RDWR" => "read_write",
        _ => "unknown",
    }
}

/// A recognized `std::ios` open-mode flag token, classified by the direction
/// it contributes. Fixed variant set, single operation (fold into a symbol) →
/// sum type + exhaustive match. Sole caller: `normalize_ios_mode_to_fstream_symbol`.
/// Rejected alternative: an `Option<bool>` tuple — it cannot distinguish a
/// recognized-but-direction-less modifier from an unrecognized token, which is
/// exactly the distinction the honesty invariant turns on.
enum IosFlag {
    /// `std::ios::in` — contributes read.
    In,
    /// `std::ios::out` / `app` / `trunc` — contributes write.
    Out,
    /// `std::ios::binary` / `ate` — recognized, carries NO direction.
    Modifier,
    /// Anything else: a dynamic variable, a typo, an unknown constant, or a
    /// near-name token (`std::ios::in_alias`). Makes the mode undetermined.
    Unrecognized,
}

/// Classify a single trimmed `std::ios` mode token by EXACT match.
///
/// HONESTY-GATE-2 (review-3): the previous `mode_text.contains("::in")` also
/// matched near-name tokens such as `std::ios::in_alias`. Exact matching on the
/// enumerated flag spellings rejects those.
fn classify_ios_flag(token: &str) -> IosFlag {
    match token {
        "std::ios::in" | "ios::in" | "std::ios_base::in" | "ios_base::in" => IosFlag::In,
        "std::ios::out"
        | "ios::out"
        | "std::ios_base::out"
        | "ios_base::out"
        | "std::ios::app"
        | "ios::app"
        | "std::ios_base::app"
        | "ios_base::app"
        | "std::ios::trunc"
        | "ios::trunc"
        | "std::ios_base::trunc"
        | "ios_base::trunc" => IosFlag::Out,
        "std::ios::binary"
        | "ios::binary"
        | "std::ios_base::binary"
        | "ios_base::binary"
        | "std::ios::ate"
        | "ios::ate"
        | "std::ios_base::ate"
        | "ios_base::ate" => IosFlag::Modifier,
        _ => IosFlag::Unrecognized,
    }
}

/// Normalize an explicit `std::ios` open-mode expression to an fstream direction
/// symbol. The expression is a `|`-separated list of flag tokens.
///
/// HONESTY-GATE-2 family 1 (review-3): a token that is not a recognized flag
/// (a dynamic variable, a typo, a near-name spelling) makes the whole mode
/// undetermined → `fstream_unknown` (binding direction `unknown`), NEVER a
/// guessed `read_write`. A mode composed ONLY of direction-less modifiers
/// (`binary`/`ate`) is likewise direction-undetermined → `fstream_unknown`.
/// This function is only reached when an EXPLICIT mode argument is present; the
/// no-argument default (contract-fixed `in|out`) is handled by the caller.
fn normalize_ios_mode_to_fstream_symbol(mode_text: &str) -> String {
    let mut has_in = false;
    let mut has_out = false;
    for raw in mode_text.split('|') {
        match classify_ios_flag(raw.trim()) {
            IosFlag::In => has_in = true,
            IosFlag::Out => has_out = true,
            IosFlag::Modifier => {}
            IosFlag::Unrecognized => return "fstream_unknown".to_string(),
        }
    }
    match (has_in, has_out) {
        (true, true) => "fstream".to_string(),           // read_write
        (true, false) => "fstream_read".to_string(),     // read
        (false, true) => "fstream_write".to_string(),    // write
        (false, false) => "fstream_unknown".to_string(), // modifiers only / empty
    }
}

// ── Doc comment extraction ───────────────────────────────────────

fn extract_doc_comment(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment" {
            let text = p.utf8_text(src).ok()?;
            if text.starts_with("/**") || text.starts_with("///") || text.starts_with("//!") {
                return Some(text.to_string());
            }
        }
        if p.kind() != "comment" {
            break;
        }
        prev = p.prev_sibling();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_ok(ext: &CppExtractor, source: &str, path: &str) -> ExtractionResult {
        ext.extract(source, path, &format!("r1:{}", path), "r1", "snap1")
            .expect("extraction should succeed")
    }

    #[test]
    fn file_node_has_correct_stable_key() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "int x = 1;\n", "src/main.cpp");

        assert!(!result.nodes.is_empty());
        let file_node = &result.nodes[0];
        assert_eq!(file_node.stable_key, "r1:src/main.cpp:FILE");
        assert_eq!(file_node.kind, NodeKind::File);
    }

    #[test]
    fn free_function_creates_symbol() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo(int x) { return; }\n", "src/main.cpp");

        let func = result.nodes.iter().find(|n| n.name == "foo").unwrap();
        assert_eq!(func.stable_key, "r1:src/main.cpp#foo:SYMBOL:FUNCTION");
        assert_eq!(func.subtype, Some(NodeSubtype::Function));
    }

    #[test]
    fn class_creates_symbol() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class MyClass { };\n", "src/main.cpp");

        let cls = result.nodes.iter().find(|n| n.name == "MyClass").unwrap();
        assert_eq!(cls.stable_key, "r1:src/main.cpp#MyClass:SYMBOL:CLASS");
        assert_eq!(cls.subtype, Some(NodeSubtype::Class));
    }

    #[test]
    fn namespace_qualified_name() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "namespace ns { void foo() {} }\n", "src/main.cpp");

        let func = result.nodes.iter().find(|n| n.name == "foo").unwrap();
        assert_eq!(func.qualified_name, Some("ns::foo".to_string()));
        assert!(func.stable_key.contains("ns::foo"));
    }

    #[test]
    fn nested_namespace() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            "namespace outer { namespace inner { void foo() {} } }\n",
            "src/main.cpp",
        );

        let func = result.nodes.iter().find(|n| n.name == "foo").unwrap();
        assert_eq!(func.qualified_name, Some("outer::inner::foo".to_string()));
    }

    #[test]
    fn method_in_class() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class C { void method() {} };\n", "src/main.cpp");

        let method = result.nodes.iter().find(|n| n.name == "method").unwrap();
        assert_eq!(method.qualified_name, Some("C::method".to_string()));
        assert_eq!(method.subtype, Some(NodeSubtype::Method));
    }

    #[test]
    fn constructor_detected() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class C { C() {} };\n", "src/main.cpp");

        let ctor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();
        assert_eq!(ctor.name, "C");
        assert_eq!(ctor.qualified_name, Some("C::C".to_string()));
    }

    #[test]
    fn destructor_detected() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class C { ~C() {} };\n", "src/main.cpp");

        let dtor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Destructor))
            .unwrap();
        assert_eq!(dtor.name, "~C");
    }

    // ── CPP-SPAN-FIDELITY-1: macro-decorated names, span fidelity, recovery ──

    /// Helper: the SYMBOL node with the given name (panics with the symbol list).
    fn sym<'a>(r: &'a ExtractionResult, name: &str) -> &'a ExtractedNode {
        r.nodes
            .iter()
            .find(|n| n.kind == NodeKind::Symbol && n.name == name)
            .unwrap_or_else(|| {
                let syms: Vec<_> = r
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Symbol)
                    .map(|n| format!("{} {:?}", n.name, n.subtype))
                    .collect();
                panic!("no symbol named {name}; symbols = {syms:#?}")
            })
    }

    #[test]
    fn macro_class_name_is_the_type_not_the_macro() {
        // §4(a): `class EXPORT_MACRO Foo {}` → name Foo, kind class, tight span,
        // macro recorded as metadata — never as the name.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class DLL_LINKAGE Foo {\n  int x;\n};\n", "src/a.cpp");

        assert!(
            !result.nodes.iter().any(|n| n.name == "DLL_LINKAGE"),
            "the export macro must never be a symbol name",
        );
        let foo = sym(&result, "Foo");
        assert_eq!(foo.subtype, Some(NodeSubtype::Class));
        let loc = foo.location.as_ref().expect("clean class has a span");
        assert_eq!((loc.line_start, loc.line_end), (1, 3));
        let meta = foo.metadata_json.as_deref().unwrap_or("");
        assert!(
            meta.contains("\"macro_tokens\":[\"DLL_LINKAGE\"]"),
            "macro recorded additively, got {meta:?}",
        );
    }

    #[test]
    fn macro_struct_with_base_is_struct_named_bar_not_function() {
        // §4(b): `struct API Bar : Base {}` → Bar/struct (never `function`,
        // which is what tree-sitter's function_definition mis-shape yields).
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            "struct API_EXPORT Bar : Base { int x; };\n",
            "src/b.cpp",
        );

        let bar = sym(&result, "Bar");
        assert_eq!(
            bar.subtype,
            Some(NodeSubtype::Struct),
            "a macro-decorated struct with a base must be STRUCT, never FUNCTION",
        );
        assert!(!result
            .nodes
            .iter()
            .any(|n| n.subtype == Some(NodeSubtype::Function)));
        let loc = bar.location.as_ref().unwrap();
        assert_eq!((loc.line_start, loc.line_end), (1, 1));
    }

    #[test]
    fn leveldb_export_db_class_not_function() {
        // The reported leveldb defect: `class LEVELDB_EXPORT DB { … }` landed as
        // SYMBOL:FUNCTION. It must be CLASS named DB.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let src = "class LEVELDB_EXPORT DB {\n public:\n  virtual ~DB();\n};\n";
        let result = extract_ok(&ext, src, "include/db.h");
        let db = sym(&result, "DB");
        assert_eq!(db.subtype, Some(NodeSubtype::Class));
    }

    #[test]
    fn preproc_confused_body_recovers_siblings_under_true_scope() {
        // §4(c): an anonymous-namespace class whose body is over-extended by a
        // preprocessor guard must not swallow the sibling classes/methods after
        // it. All three types are extracted with tight spans under the namespace.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let src = r#"
namespace {
class Limiter {
 public:
#if !defined(NDEBUG)
  int max_;
#endif
  bool Acquire() { return true; }
};

class Worker final : public Base {
 public:
  void Run() { work(); }
};

class Sink {
 public:
  void Flush() {}
};
}  // namespace
"#;
        let result = extract_ok(&ext, src, "src/anon.cpp");

        // All three classes recovered (not swallowed into Limiter).
        for name in ["Limiter", "Worker", "Sink"] {
            let n = sym(&result, name);
            assert_eq!(n.subtype, Some(NodeSubtype::Class), "{name} kind");
            assert!(n.location.is_some(), "{name} must carry a span");
        }
        // Limiter's span must NOT reach the later siblings.
        let limiter = sym(&result, "Limiter");
        let worker = sym(&result, "Worker");
        let lim_end = limiter.location.as_ref().unwrap().line_end;
        let wrk_start = worker.location.as_ref().unwrap().line_start;
        assert!(
            lim_end < wrk_start,
            "Limiter span {lim_end} must end before Worker starts {wrk_start} (no swallow)",
        );
        // Their methods are recovered under the right owners.
        assert_eq!(
            sym(&result, "Run").qualified_name.as_deref(),
            Some("Worker::Run")
        );
        assert_eq!(
            sym(&result, "Flush").qualified_name.as_deref(),
            Some("Sink::Flush")
        );
    }

    #[test]
    fn balanced_brace_end_is_the_honesty_primitive() {
        // §4(d): the span comes from balanced-brace recovery. When braces never
        // balance, the primitive returns None → `type_span_and_body_close` emits
        // NO span (visible absence), never a guessed/swallowing range. Strings,
        // char literals, and comments never contribute a brace.
        let bal = |s: &str| balanced_brace_end(s.as_bytes(), 0, s.len());
        assert_eq!(bal("{ a { } b }"), Some(11)); // balanced
        assert_eq!(bal("{ a { } b"), None); // one close missing → honest None
        assert_eq!(bal("{ \"}}}\" }"), Some(9)); // braces in a string ignored
        assert_eq!(bal("{ '}' }"), Some(7)); // brace in a char literal ignored
        assert_eq!(bal("{ /* } } */ }"), Some(13)); // braces in a comment ignored
        assert_eq!(bal("{ // }\n }"), Some(9)); // brace in a line comment ignored
                                                // Raw-string payload braces never count: R"(})" holds a `}` that must
                                                // not close the region; the real `}` after it does. Length 10.
        assert_eq!(bal("{ R\"(})\" }"), Some(10));
        // Custom delimiter: the payload's `)"` (wrong delim) does not terminate;
        // only `)x"` does, so the inner `}` stays inside the literal. Length 16.
        assert_eq!(bal("{ R\"x(} )\" )x\" }"), Some(16));
    }

    #[test]
    fn skip_raw_string_boundaries() {
        // The `R` must be at a token boundary: `fooR"x"` is identifier + string,
        // not a raw string, so its `"` obeys the ordinary rule.
        let s = b"fooR\"a } b\"";
        // quote index is 4 (after `fooR`); `R` is preceded by `o` (ident) → None.
        assert_eq!(skip_raw_string(s, 4, s.len()), None);
        // Bare `R"(...)"`: consumes the whole literal including inner `"` and `}`.
        let s = b"R\"(x \" } y)\"";
        assert_eq!(skip_raw_string(s, 1, s.len()), Some(s.len()));
        // Encoding-prefixed forms resolve their token start correctly.
        let s = b"u8R\"(z)\"";
        assert_eq!(skip_raw_string(s, 3, s.len()), Some(s.len()));
    }

    #[test]
    fn unparseable_class_yields_no_span_and_does_not_swallow_sibling() {
        // §4(d): a genuinely-unparseable class (its body braces never balance to
        // EOF) is emitted as a DECLARATION WITHOUT A SPAN — a visible absence,
        // never a guessed or swallowing range — and a following sibling
        // definition keeps its own tight span (is not absorbed).
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        // `Broken`'s body opens on line 1; the stray `{` on line 3 leaves the
        // brace depth net-open through EOF, so balanced-brace recovery finds no
        // honest close. `After` (line 5) is a clean, separate definition.
        let src = "struct Broken {\n  int x;\n{\n};\nstruct After { int y; };\n";
        let result = extract_ok(&ext, src, "src/broken.cpp");

        // Broken IS emitted (the declaration is not lost) …
        let broken = sym(&result, "Broken");
        // … but carries NO span: the honest no-span fallback, not a swallow.
        assert!(
            broken.location.is_none(),
            "unbalanced class must be emitted with no span, got {:?}",
            broken.location,
        );

        // The following sibling is extracted independently with its own span —
        // it was not absorbed into Broken.
        let after = sym(&result, "After");
        let loc = after.location.as_ref().expect("After has a real span");
        assert_eq!(
            (loc.line_start, loc.line_end),
            (5, 5),
            "After must keep its own tight span, not be swallowed",
        );

        // Invariant restated over every emitted symbol: none spans past EOF.
        let file_lines = src.lines().count() as i64;
        for n in result.nodes.iter().filter(|n| n.kind == NodeKind::Symbol) {
            if let Some(loc) = &n.location {
                assert!(
                    loc.line_end <= file_lines,
                    "no symbol may span past EOF: {} @ {:?}",
                    n.name,
                    loc,
                );
            }
        }
    }

    #[test]
    fn raw_string_payload_never_moves_a_class_span() {
        // §2.3 honesty: a C++ raw string in a member initializer holds a bare `"`
        // and an unbalanced `}`. If the source scanner treated it as an ordinary
        // string, the embedded `"` would end the string early and the `}` would
        // falsely CLOSE the class span two lines short. Raw-aware scanning must
        // ignore both, so the span ends at the real closing brace.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let src = "class C {\n  const char* s = R\"(a \" } b)\";\n  void m() {}\n};\n";
        let result = extract_ok(&ext, src, "src/raw.cpp");

        let c = sym(&result, "C");
        let loc = c.location.as_ref().expect("C has a real span");
        assert_eq!(
            (loc.line_start, loc.line_end),
            (1, 4),
            "span must end at the real closing brace, not the raw payload's `}}`",
        );
        // The member after the raw string is still inside the class (body not
        // truncated by a premature close).
        assert_eq!(sym(&result, "m").qualified_name.as_deref(), Some("C::m"));
    }

    #[test]
    fn wellformed_class_span_is_unchanged() {
        // Regression guard: a clean class's span is the keyword line to its
        // closing-brace line (byte-stable vs pre-slice behavior).
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class C {\n  void m() {}\n};\n", "src/c.cpp");
        let c = sym(&result, "C");
        let loc = c.location.as_ref().unwrap();
        assert_eq!((loc.line_start, loc.line_end), (1, 3));
        assert!(c.metadata_json.is_none(), "no macros → no metadata");
    }

    #[test]
    fn inheritance_creates_implements_edge() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "class D : public B { };\n", "src/main.cpp");

        let impl_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Implements)
            .collect();

        assert_eq!(impl_edges.len(), 1);
        assert_eq!(impl_edges[0].target_key, "B");
    }

    #[test]
    fn extern_c_block_detected() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, r#"extern "C" { void c_func() {} }"#, "src/main.cpp");

        let func = result.nodes.iter().find(|n| n.name == "c_func").unwrap();
        let meta = func.metadata_json.as_ref().unwrap();
        assert!(meta.contains("\"language_linkage\":\"c\""));
        assert!(meta.contains("\"declared_in_extern_c_block\":true"));
    }

    #[test]
    fn extern_c_single_declaration() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, r#"extern "C" void c_func() {}"#, "src/main.cpp");

        let func = result.nodes.iter().find(|n| n.name == "c_func").unwrap();
        let meta = func.metadata_json.as_ref().unwrap();
        assert!(meta.contains("\"language_linkage\":\"c\""));
    }

    #[test]
    fn file_has_linkage_stats() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"extern "C" { void f1() {} void f2() {} }"#,
            "src/main.cpp",
        );

        let file_node = &result.nodes[0];
        let meta = file_node.metadata_json.as_ref().unwrap();
        assert!(meta.contains("\"has_extern_c_declarations\":true"));
        assert!(meta.contains("\"extern_c_symbol_count\":2"));
    }

    #[test]
    fn call_extraction() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo() { bar(); }\n", "src/main.cpp");

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].target_key, "bar");
    }

    #[test]
    fn qualified_call() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo() { std::sort(); }\n", "src/main.cpp");

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].target_key, "std::sort");
    }

    #[test]
    fn method_call() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo() { obj.method(); }\n", "src/main.cpp");

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].target_key, "method");
    }

    #[test]
    fn include_creates_imports_edge() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "#include \"myheader.hpp\"\n", "src/main.cpp");

        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].edge_type, EdgeType::Imports);
        assert_eq!(result.edges[0].target_key, "myheader.hpp");
        assert!(result.import_bindings[0].is_relative);
    }

    #[test]
    fn system_include_not_relative() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "#include <iostream>\n", "src/main.cpp");

        assert!(!result.import_bindings[0].is_relative);
    }

    #[test]
    fn template_class_extracted() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            "template<typename T> class Container { };\n",
            "src/main.cpp",
        );

        let cls = result.nodes.iter().find(|n| n.name == "Container");
        assert!(cls.is_some());
    }

    #[test]
    fn out_of_line_method() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            "class C { void method(); };\nvoid C::method() {}\n",
            "src/main.cpp",
        );

        let methods: Vec<_> = result.nodes.iter().filter(|n| n.name == "method").collect();

        // Should have both declaration and definition
        assert_eq!(methods.len(), 2);
        // Both should have qualified name C::method
        for m in methods {
            assert_eq!(m.qualified_name, Some("C::method".to_string()));
        }
    }

    // ── CPP-SB-1: ResolvedCallsite tests ──────────────────────────

    #[test]
    fn fopen_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <cstdio>

void read_config() {
    FILE* f = fopen("/etc/config.txt", "r");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/reader.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "libc:stdio");
        assert_eq!(cs.resolved_symbol, "fopen_read");
        assert_eq!(
            cs.arg0_payload,
            CallArgPayload::StringLiteral {
                value: "/etc/config.txt".to_string()
            }
        );
    }

    #[test]
    fn fopen_write_mode_emits_correct_symbol() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
void write_log() {
    FILE* f = fopen("/var/log/app.log", "w");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/writer.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "fopen_write");
    }

    #[test]
    fn open_rdonly_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fcntl.h>

void read_device() {
    int fd = open("/dev/input0", O_RDONLY);
    if (fd >= 0) close(fd);
}
"#;
        let result = extract_ok(&ext, source, "src/device.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "libc:fcntl");
        assert_eq!(cs.resolved_symbol, "open_read");
    }

    #[test]
    fn open_dynamic_flags_emits_unknown_not_read_write() {
        // HONESTY-GATE-2 family 1: undetermined flags → open_unknown, NOT the
        // old guessed open_read_write.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fcntl.h>

void open_it(int fl) {
    int fd = open("/dev/thing", fl);
    if (fd >= 0) close(fd);
}
"#;
        let result = extract_ok(&ext, source, "src/dyn.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "open_unknown");
    }

    #[test]
    fn fopen_invalid_plus_mode_emits_unknown_not_read_write() {
        // HONESTY-GATE-2 family 1 (review-1): a literal that is NOT a valid
        // fopen mode but merely contains '+' (e.g. "q+") must NOT be guessed
        // as read_write via a substring match → fopen_unknown.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <cstdio>

void weird() {
    FILE* f = fopen("/etc/config.txt", "q+");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/weird.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol,
            "fopen_unknown"
        );
    }

    #[test]
    fn fopen_repeated_flag_mode_emits_unknown_not_read_write() {
        // HONESTY-GATE-2 family 1 (review-2): a malformed mode that merely
        // REPEATS a recognized flag character (e.g. "r++") is not a mode any
        // real fopen accepts. It must NOT collapse to read_write via a
        // has_plus latch — a repeated flag → fopen_unknown.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <cstdio>

void weird() {
    FILE* f = fopen("/etc/config.txt", "r++");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/weird.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol,
            "fopen_unknown"
        );
    }

    #[test]
    fn open_near_match_flag_identifier_emits_unknown_not_read() {
        // HONESTY-GATE-2 family 1 (review-1): a near-match identifier that
        // merely CONTAINS an O_* fragment (e.g. O_RDONLY_ALIAS) must NOT be
        // classified from that fragment (STANDING RULE 2). Exact-match only
        // → open_unknown.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fcntl.h>

void read_device() {
    int fd = open("/dev/input0", O_RDONLY_ALIAS);
    if (fd >= 0) close(fd);
}
"#;
        let result = extract_ok(&ext, source, "src/device.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "open_unknown");
    }

    #[test]
    fn ifstream_constructor_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>

void load_config() {
    std::ifstream config("/etc/app.ini");
}
"#;
        let result = extract_ok(&ext, source, "src/config.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "std:fstream");
        assert_eq!(cs.resolved_symbol, "ifstream");
        assert_eq!(
            cs.arg0_payload,
            CallArgPayload::StringLiteral {
                value: "/etc/app.ini".to_string()
            }
        );
    }

    #[test]
    fn ofstream_constructor_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>

void save_data() {
    std::ofstream output("/var/data/output.txt");
}
"#;
        let result = extract_ok(&ext, source, "src/output.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "std:fstream");
        assert_eq!(cs.resolved_symbol, "ofstream");
    }

    #[test]
    fn fstream_with_mode_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>

void read_binary() {
    std::fstream data("/data/file.bin", std::ios::in);
}
"#;
        let result = extract_ok(&ext, source, "src/binary.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "std:fstream");
        assert_eq!(cs.resolved_symbol, "fstream_read");
    }

    #[test]
    fn fstream_in_and_out_mode_emits_read_write() {
        // Sanity: a well-formed in|out explicit mode still classifies read_write.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>
void rw() {
    std::fstream data("/data/file.bin", std::ios::in | std::ios::out);
}
"#;
        let result = extract_ok(&ext, source, "src/rw.cpp");
        assert_eq!(result.resolved_callsites.len(), 1);
        // "fstream" symbol binds to direction read_write.
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "fstream");
    }

    #[test]
    fn fstream_dynamic_mode_emits_unknown_not_read_write() {
        // HONESTY-GATE-2 (review-3): an explicit mode that is a dynamic variable
        // (not a recognized std::ios flag) is undetermined → fstream_unknown,
        // never the old read_write default.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>
void open_dyn(std::ios::openmode m) {
    std::fstream data("/data/file.bin", m);
}
"#;
        let result = extract_ok(&ext, source, "src/dyn.cpp");
        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol, "fstream_unknown",
            "dynamic std::fstream mode must render unknown, not read_write"
        );
    }

    #[test]
    fn fstream_near_name_ios_token_emits_unknown_not_read() {
        // HONESTY-GATE-2 (review-3): a near-name token (`std::ios::in_alias`) must
        // NOT match `::in` by substring. Unrecognized token → fstream_unknown.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>
void open_near() {
    std::fstream data("/data/file.bin", std::ios::in_alias);
}
"#;
        let result = extract_ok(&ext, source, "src/near.cpp");
        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol, "fstream_unknown",
            "near-name ios token must not be read-classified"
        );
    }

    #[test]
    fn fstream_modifier_only_mode_emits_unknown() {
        // A mode composed only of direction-less modifiers (binary) is
        // direction-undetermined → fstream_unknown.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>
void open_bin() {
    std::fstream data("/data/file.bin", std::ios::binary);
}
"#;
        let result = extract_ok(&ext, source, "src/bin.cpp");
        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol, "fstream_unknown",
            "modifier-only mode carries no direction"
        );
    }

    #[test]
    fn literal_in_non_access_position_produces_no_callsite() {
        // HONESTY-GATE-2 spec §2.1 (review-3): a string literal that is NOT arg0
        // of the access call — here it is an argument to a path-join helper whose
        // result is passed to fopen — is NOT path evidence. arg0 is a call
        // expression, not a string literal → no resource callsite.
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <cstdio>
extern char* join(const char*, const char*);
void read_joined() {
    FILE* f = fopen(join("dir", "file.txt"), "r");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/joined.cpp");
        assert!(
            result.resolved_callsites.is_empty(),
            "literal inside a path-join argument is not access-position evidence: {:?}",
            result.resolved_callsites
        );
    }

    #[test]
    fn ifstream_open_with_local_type_map() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>

void load() {
    std::ifstream file;
    file.open("/etc/settings.conf");
}
"#;
        let result = extract_ok(&ext, source, "src/settings.cpp");

        // Should have one callsite from .open() (declaration without path doesn't emit)
        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "std:fstream");
        assert_eq!(cs.resolved_symbol, "ifstream_open");
        assert_eq!(
            cs.arg0_payload,
            CallArgPayload::StringLiteral {
                value: "/etc/settings.conf".to_string()
            }
        );
    }

    #[test]
    fn ofstream_open_with_local_type_map() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <fstream>

void save() {
    std::ofstream out;
    out.open("/var/log/output.log");
}
"#;
        let result = extract_ok(&ext, source, "src/log.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_symbol, "ofstream_open");
    }

    #[test]
    fn dynamic_path_produces_no_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
void read_file(const char* path) {
    FILE* f = fopen(path, "r");
    if (f) fclose(f);
}
"#;
        let result = extract_ok(&ext, source, "src/dynamic.cpp");

        assert!(
            result.resolved_callsites.is_empty(),
            "dynamic path should not produce ResolvedCallsite"
        );
    }

    #[test]
    fn cout_produces_no_state_boundary() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <iostream>

void log_message() {
    std::cout << "Hello, world!" << std::endl;
}
"#;
        let result = extract_ok(&ext, source, "src/logger.cpp");

        assert!(
            result.resolved_callsites.is_empty(),
            "cout should not produce state-boundary callsite"
        );
    }

    #[test]
    fn sqlite3_open_emits_resolved_callsite() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let source = r#"
#include <sqlite3.h>

int db_open() {
    sqlite3* db;
    int rc = sqlite3_open("app.db", &db);
    return rc;
}
"#;
        let result = extract_ok(&ext, source, "src/database.cpp");

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "sqlite3");
        assert_eq!(cs.resolved_symbol, "sqlite3_open");
    }
}
