//! Core C++ extractor implementation.
//!
//! Uses tree-sitter-cpp to parse C++ source files and extract structural
//! information: symbols, edges, and metrics.

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::types::{ImportBinding, RuntimeBuiltinsSet, SourceLocation};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::routing::is_test_file;
use repo_graph_indexer::types::{
    EdgeType, ExtractedEdge, ExtractedMetrics, ExtractedNode, ExtractionResult, NodeKind,
    NodeSubtype, Resolution, Visibility,
};

use crate::linkage::{
    extract_linkage_from_spec, FileLinkageStats, LanguageLinkage, LinkageMetadata,
};
use crate::metrics::compute_function_metrics;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "cpp-core:0.1.0";

/// Languages this extractor handles.
const LANGUAGES: &[&str] = &["cpp"];

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
        };

        // Walk top-level declarations
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            walk_top_level(&child, src, &mut ctx);
        }

        // Update file node metadata with linkage stats
        if ctx.file_linkage_stats.has_extern_c_declarations {
            if let Some(file_node) = ctx.nodes.first_mut() {
                file_node.metadata_json = ctx.file_linkage_stats.to_json();
            }
        }

        Ok(ExtractionResult {
            nodes: ctx.nodes,
            edges: ctx.edges,
            metrics: ctx.metrics,
            import_bindings: ctx.import_bindings,
            resolved_callsites: Vec::new(),
        })
    }
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
        "function_definition" => extract_function(node, src, ctx),
        "declaration" => extract_declaration(node, src, ctx),
        "class_specifier" => extract_class(node, src, ctx),
        "struct_specifier" => extract_struct(node, src, ctx),
        "enum_specifier" => extract_enum(node, src, ctx),
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
                specifier = text.trim_start_matches('<').trim_end_matches('>').to_string();
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
        .last()
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

// ── Class extraction ─────────────────────────────────────────────

fn extract_class(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    extract_class_like(node, src, ctx, NodeSubtype::Class);
}

fn extract_struct(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    extract_class_like(node, src, ctx, NodeSubtype::Struct);
}

fn extract_class_like(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    subtype: NodeSubtype,
) {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string());

    // Anonymous classes/structs
    let class_name = name.clone().unwrap_or_else(|| {
        if subtype == NodeSubtype::Class {
            "anon_class".to_string()
        } else {
            "anon_struct".to_string()
        }
    });

    let qualified_name = ctx.qualified_name(&class_name);
    let stable_key = ctx.make_stable_key(&qualified_name, &subtype);
    let linkage_meta = ctx.linkage_metadata();

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name: class_name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: linkage_meta.to_json(),
    });

    // Extract base classes as IMPLEMENTS edges
    extract_base_classes(node, src, ctx);

    // Process class body
    if let Some(body) = node.child_by_field_name("body") {
        let prev_class = ctx.current_class.take();
        ctx.current_class = Some(class_name);

        let mut current_visibility = if subtype == NodeSubtype::Class {
            Visibility::Private // C++ class default
        } else {
            Visibility::Export // C++ struct default is public
        };

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "access_specifier" => {
                    current_visibility = parse_access_specifier(&child, src);
                }
                "function_definition" => {
                    extract_method(node, &child, src, ctx, current_visibility);
                }
                "field_declaration" => {
                    // Check if it's a method declaration (has function_declarator)
                    if has_function_declarator(&child) {
                        extract_method_declaration(&child, src, ctx, current_visibility);
                    }
                }
                "declaration" => {
                    // Nested class/struct/enum
                    let mut decl_cursor = child.walk();
                    for decl_child in child.children(&mut decl_cursor) {
                        match decl_child.kind() {
                            "class_specifier" => extract_class(&decl_child, src, ctx),
                            "struct_specifier" => extract_struct(&decl_child, src, ctx),
                            "enum_specifier" => extract_enum(&decl_child, src, ctx),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        ctx.current_class = prev_class;
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

// ── Enum extraction ──────────────────────────────────────────────

fn extract_enum(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "anon_enum".to_string());

    let qualified_name = ctx.qualified_name(&name);
    let stable_key = ctx.make_stable_key(&qualified_name, &NodeSubtype::Enum);
    let linkage_meta = ctx.linkage_metadata();

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Enum),
        name,
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
    // Check for nested class/struct/enum
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_specifier" => extract_class(&child, src, ctx),
            "struct_specifier" => extract_struct(&child, src, ctx),
            "enum_specifier" => extract_enum(&child, src, ctx),
            _ => {}
        }
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
            let mut parts: Vec<&str> =
                ctx.current_namespace.iter().map(|s| s.as_str()).collect();
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
    let signature = params.map(|p| {
        format!(
            "{}{}",
            qualified_name,
            p.utf8_text(src).unwrap_or("()")
        )
    });

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
    let signature = params.map(|p| {
        format!(
            "{}{}",
            qualified_name,
            p.utf8_text(src).unwrap_or("()")
        )
    });

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
    let signature = params.map(|p| {
        format!(
            "{}{}",
            qualified_name,
            p.utf8_text(src).unwrap_or("()")
        )
    });

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
    fn walk(node: &tree_sitter::Node, src: &[u8], source_node_uid: &str, ctx: &mut ExtractionCtx) {
        if node.kind() == "call_expression" {
            extract_call(node, src, source_node_uid, ctx);
        }

        // Don't recurse into nested functions or lambdas
        if node.kind() == "function_definition" || node.kind() == "lambda_expression" {
            return;
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
        "identifier" => function.utf8_text(src).unwrap_or("").to_string(),
        "qualified_identifier" | "scoped_identifier" => {
            function.utf8_text(src).unwrap_or("").to_string()
        }
        "field_expression" => {
            // obj.method() or ptr->method()
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
        let result = extract_ok(
            &ext,
            "namespace ns { void foo() {} }\n",
            "src/main.cpp",
        );

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
        let result = extract_ok(
            &ext,
            "class C { void method() {} };\n",
            "src/main.cpp",
        );

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
        let result = extract_ok(
            &ext,
            r#"extern "C" { void c_func() {} }"#,
            "src/main.cpp",
        );

        let func = result.nodes.iter().find(|n| n.name == "c_func").unwrap();
        let meta = func.metadata_json.as_ref().unwrap();
        assert!(meta.contains("\"language_linkage\":\"c\""));
        assert!(meta.contains("\"declared_in_extern_c_block\":true"));
    }

    #[test]
    fn extern_c_single_declaration() {
        let mut ext = CppExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"extern "C" void c_func() {}"#,
            "src/main.cpp",
        );

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

        let methods: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.name == "method")
            .collect();

        // Should have both declaration and definition
        assert_eq!(methods.len(), 2);
        // Both should have qualified name C::method
        for m in methods {
            assert_eq!(m.qualified_name, Some("C::method".to_string()));
        }
    }
}
