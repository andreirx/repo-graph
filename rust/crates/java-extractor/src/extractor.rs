//! Core Java extractor implementation.
//!
//! Uses tree-sitter-java to parse Java source files and extract
//! structural information: symbols, edges, and metrics.
//!
//! Design doc: docs/design/java-rust-extractor-v1.md

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::types::{ImportBinding, RuntimeBuiltinsSet, SourceLocation};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::routing::is_test_file;
use repo_graph_indexer::types::{
    EdgeType, ExtractionResult, ExtractedEdge, ExtractedMetrics, ExtractedNode, NodeKind,
    NodeSubtype, Resolution, Visibility,
};

use crate::builtins::java_runtime_builtins;
use crate::metrics::compute_method_metrics;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "java-core:0.1.0";

/// Languages this extractor handles.
const LANGUAGES: &[&str] = &["java"];

/// Concrete `ExtractorPort` adapter for Java.
pub struct JavaExtractor {
    languages: Vec<String>,
    builtins: RuntimeBuiltinsSet,
    parser: Option<tree_sitter::Parser>,
    java_language: tree_sitter::Language,
}

impl JavaExtractor {
    /// Create a new extractor. Call `initialize()` before `extract()`.
    pub fn new() -> Self {
        Self {
            languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
            builtins: java_runtime_builtins(),
            parser: None,
            java_language: tree_sitter_java::LANGUAGE.into(),
        }
    }
}

impl Default for JavaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorPort for JavaExtractor {
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
            .set_language(&self.java_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set Java grammar: {}", e),
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
        // Verify initialization
        let _parser = self.parser.as_ref().ok_or_else(|| ExtractorError {
            message: "extractor not initialized — call initialize() first".into(),
        })?;

        // Clone parser for thread safety
        let mut parser_clone = tree_sitter::Parser::new();
        parser_clone
            .set_language(&self.java_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set Java grammar: {}", e),
            })?;

        let tree = parser_clone
            .parse(source, None)
            .ok_or_else(|| ExtractorError {
                message: format!("tree-sitter returned null tree for {}", file_path),
            })?;

        let root = tree.root_node();

        // Line count (mirror TS behavior)
        let line_count = source.split('\n').count().max(1) as i64;
        let file_node_uid = uuid::Uuid::new_v4().to_string();
        let file_name = file_path.rsplit('/').next().unwrap_or(file_path);

        let src = source.as_bytes();

        // Determine if test file
        let is_test = is_test_file(file_path);

        let mut ctx = ExtractionCtx {
            file_path,
            file_uid,
            file_node_uid: &file_node_uid,
            repo_uid,
            snapshot_uid,
            package_name: None,
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
        };

        // Walk the program node (root)
        // Top-level types have no enclosing type and are parented to the file
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "package_declaration" => {
                    ctx.package_name = extract_package_name(&child, src);
                }
                "import_declaration" => {
                    extract_import(&child, src, &mut ctx);
                }
                "class_declaration" => {
                    extract_class(&child, src, &mut ctx, None, &file_node_uid);
                }
                "interface_declaration" => {
                    extract_interface(&child, src, &mut ctx, None, &file_node_uid);
                }
                "enum_declaration" => {
                    extract_enum(&child, src, &mut ctx, None, &file_node_uid);
                }
                "record_declaration" => {
                    // Java 14+ records are treated as classes
                    extract_record(&child, src, &mut ctx, None, &file_node_uid);
                }
                "annotation_type_declaration" => {
                    // @interface declarations
                    extract_annotation_type(&child, src, &mut ctx, None, &file_node_uid);
                }
                _ => {}
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
    package_name: Option<String>,
    nodes: Vec<ExtractedNode>,
    edges: Vec<ExtractedEdge>,
    import_bindings: Vec<ImportBinding>,
    metrics: BTreeMap<String, ExtractedMetrics>,
    stable_key_counts: HashMap<String, u32>,
}

impl<'a> ExtractionCtx<'a> {
    /// Generate a stable_key with duplicate disambiguation.
    ///
    /// For members (methods, constructors, fields), `enclosing_type` should be
    /// the simple name of the containing class/interface/enum. For nested types,
    /// `enclosing_type` should be the outer type name chain (e.g., "Outer" for
    /// Outer.Inner, "Outer.Inner" for Outer.Inner.Deep).
    ///
    /// The symbol name in the key includes the enclosing context:
    /// - Top-level class: `#Foo:SYMBOL:CLASS`
    /// - Nested class: `#Outer.Inner:SYMBOL:CLASS`
    /// - Method in class: `#Foo.doWork:SYMBOL:METHOD`
    fn make_stable_key(
        &mut self,
        name: &str,
        subtype: &NodeSubtype,
        enclosing_type: Option<&str>,
    ) -> String {
        let subtype_str = serde_json::to_value(subtype)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", subtype));

        // Build the symbol identifier with enclosing context
        let symbol_id = match enclosing_type {
            Some(enclosing) => format!("{}.{}", enclosing, name),
            None => name.to_string(),
        };

        let base_key = format!(
            "{}:{}#{}:SYMBOL:{}",
            self.repo_uid, self.file_path, symbol_id, subtype_str
        );

        let count = self.stable_key_counts.entry(base_key.clone()).or_insert(0);
        *count += 1;

        if *count == 1 {
            base_key
        } else {
            format!("{}:dup{}", base_key, count)
        }
    }
}

// ── Package extraction ───────────────────────────────────────────

fn extract_package_name(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    // package_declaration has a scoped_identifier or identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
            return Some(node_text(&child, src));
        }
    }
    None
}

// ── Import extraction ────────────────────────────────────────────
//
// DOCUMENTED LIMITATION (design doc section 5):
// Java IMPORTS edges emit target keys in package-qualified form
// (e.g., "java.util.List", "com.example.Foo"). The current Rust indexer
// import resolver is file-path oriented and cannot resolve these to
// actual nodes. All Java IMPORTS edges will be stored in unresolved_edges
// with category ImportsFileNotFound. This is a structural v1 limitation,
// not a bug. The import EDGES exist and are useful for orientation
// (showing what a file depends on), but they don't resolve to nodes.
// Package-level resolution would require package nodes, which are out
// of scope for v1.

fn extract_import(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // import_declaration structure:
    //   "import" [static] scoped_identifier ["." "*"] ";"
    //   or "import" [static] scoped_identifier ";"

    let mut is_static = false;
    let mut import_path: Option<String> = None;
    let mut is_wildcard = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "static" => is_static = true,
            "scoped_identifier" | "identifier" => {
                import_path = Some(node_text(&child, src));
            }
            "asterisk" => is_wildcard = true,
            _ => {}
        }
    }

    let Some(path) = import_path else { return };

    // Build target key for the IMPORTS edge
    let target_key = if is_wildcard {
        format!("{}.*", path)
    } else {
        path.clone()
    };

    // Create IMPORTS edge from FILE to imported symbol/package
    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: ctx.file_node_uid.into(),
        target_key,
        edge_type: EdgeType::Imports,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(node_location(node)),
        metadata_json: if is_static {
            Some(r#"{"static":true}"#.into())
        } else {
            None
        },
    });

    // For non-wildcard imports, create ImportBinding for the classifier
    // Wildcard imports do NOT create ImportBindings per design doc
    if !is_wildcard {
        // Extract the simple name (last component of the path)
        let simple_name = path.rsplit('.').next().unwrap_or(&path).to_string();
        let specifier = path.rsplit_once('.').map(|(pkg, _)| pkg).unwrap_or("");

        // Java imports are never relative (no "." prefix like JS)
        // and never type-only (Java doesn't have that concept)
        ctx.import_bindings.push(ImportBinding {
            identifier: simple_name.clone(),
            specifier: specifier.to_string(),
            is_relative: false,
            location: Some(node_location(node)),
            is_type_only: false,
            imported_name: Some(simple_name),
        });
    }
}

// ── Class extraction ─────────────────────────────────────────────

/// Extract a class declaration.
///
/// `enclosing_type`: For nested classes, the enclosing type's name chain
/// (e.g., "Outer" or "Outer.Middle"). None for top-level classes.
///
/// `parent_node_uid`: UID of the parent node (file for top-level, enclosing
/// class for nested).
fn extract_class(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    enclosing_type: Option<&str>,
    parent_node_uid: &str,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);

    // For stable key, use enclosing type context
    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Class, enclosing_type);
    let node_uid = uuid::Uuid::new_v4().to_string();

    // Qualified name includes package and enclosing type chain
    let qualified_name = {
        let type_chain = match enclosing_type {
            Some(outer) => format!("{}.{}", outer, name),
            None => name.clone(),
        };
        match &ctx.package_name {
            Some(pkg) => format!("{}.{}", pkg, type_chain),
            None => type_chain,
        }
    };

    // This class's full type chain for nested members
    let this_type_chain = match enclosing_type {
        Some(outer) => format!("{}.{}", outer, name),
        None => name.clone(),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Class),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_node_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility,
        doc_comment,
        metadata_json: annotations,
    });

    // Extract superclass (extends)
    if let Some(superclass) = node.child_by_field_name("superclass") {
        extract_extends(&superclass, src, &node_uid, ctx);
    }

    // Extract interfaces (implements)
    if let Some(interfaces) = node.child_by_field_name("interfaces") {
        extract_implements(&interfaces, src, &node_uid, ctx);
    }

    // Extract class body members with this class as enclosing type
    if let Some(body) = node.child_by_field_name("body") {
        extract_class_body(&body, src, &node_uid, &this_type_chain, ctx);
    }
}

// ── Interface extraction ─────────────────────────────────────────

fn extract_interface(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    enclosing_type: Option<&str>,
    parent_node_uid: &str,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Interface, enclosing_type);
    let node_uid = uuid::Uuid::new_v4().to_string();

    let qualified_name = {
        let type_chain = match enclosing_type {
            Some(outer) => format!("{}.{}", outer, name),
            None => name.clone(),
        };
        match &ctx.package_name {
            Some(pkg) => format!("{}.{}", pkg, type_chain),
            None => type_chain,
        }
    };

    let this_type_chain = match enclosing_type {
        Some(outer) => format!("{}.{}", outer, name),
        None => name.clone(),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Interface),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_node_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility,
        doc_comment,
        metadata_json: annotations,
    });

    // Extract extended interfaces
    if let Some(extends_list) = node.child_by_field_name("extends_interfaces") {
        extract_implements(&extends_list, src, &node_uid, ctx);
    }

    // Extract interface body
    if let Some(body) = node.child_by_field_name("body") {
        extract_interface_body(&body, src, &node_uid, &this_type_chain, ctx);
    }
}

// ── Enum extraction ──────────────────────────────────────────────

fn extract_enum(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    enclosing_type: Option<&str>,
    parent_node_uid: &str,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Enum, enclosing_type);
    let node_uid = uuid::Uuid::new_v4().to_string();

    let qualified_name = {
        let type_chain = match enclosing_type {
            Some(outer) => format!("{}.{}", outer, name),
            None => name.clone(),
        };
        match &ctx.package_name {
            Some(pkg) => format!("{}.{}", pkg, type_chain),
            None => type_chain,
        }
    };

    let this_type_chain = match enclosing_type {
        Some(outer) => format!("{}.{}", outer, name),
        None => name.clone(),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Enum),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_node_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility,
        doc_comment,
        metadata_json: annotations,
    });

    // Extract enum body (constants and methods)
    if let Some(body) = node.child_by_field_name("body") {
        extract_enum_body(&body, src, &node_uid, &this_type_chain, ctx);
    }
}

// ── Record extraction (Java 14+) ─────────────────────────────────

fn extract_record(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    enclosing_type: Option<&str>,
    parent_node_uid: &str,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);

    // Records are treated as CLASS per design doc
    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Class, enclosing_type);
    let node_uid = uuid::Uuid::new_v4().to_string();

    let qualified_name = {
        let type_chain = match enclosing_type {
            Some(outer) => format!("{}.{}", outer, name),
            None => name.clone(),
        };
        match &ctx.package_name {
            Some(pkg) => format!("{}.{}", pkg, type_chain),
            None => type_chain,
        }
    };

    let this_type_chain = match enclosing_type {
        Some(outer) => format!("{}.{}", outer, name),
        None => name.clone(),
    };

    // Add record indicator to metadata
    let metadata = match annotations {
        Some(ann) => {
            // Merge with annotations
            let mut parsed: serde_json::Value = serde_json::from_str(&ann).unwrap_or_default();
            if let Some(obj) = parsed.as_object_mut() {
                obj.insert("isRecord".into(), serde_json::Value::Bool(true));
            }
            Some(serde_json::to_string(&parsed).unwrap_or(ann))
        }
        None => Some(r#"{"isRecord":true}"#.into()),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Class),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_node_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility,
        doc_comment,
        metadata_json: metadata,
    });

    // Extract record body if present
    if let Some(body) = node.child_by_field_name("body") {
        extract_class_body(&body, src, &node_uid, &this_type_chain, ctx);
    }
}

// ── Annotation type extraction (@interface) ──────────────────────

fn extract_annotation_type(
    node: &tree_sitter::Node,
    src: &[u8],
    ctx: &mut ExtractionCtx,
    enclosing_type: Option<&str>,
    parent_node_uid: &str,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let doc_comment = extract_doc_comment(node, src);

    // Annotation types are INTERFACE subtype per design doc
    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Interface, enclosing_type);
    let node_uid = uuid::Uuid::new_v4().to_string();

    let qualified_name = {
        let type_chain = match enclosing_type {
            Some(outer) => format!("{}.{}", outer, name),
            None => name.clone(),
        };
        match &ctx.package_name {
            Some(pkg) => format!("{}.{}", pkg, type_chain),
            None => type_chain,
        }
    };

    ctx.nodes.push(ExtractedNode {
        node_uid,
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Interface),
        name,
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_node_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility,
        doc_comment,
        metadata_json: Some(r#"{"isAnnotationType":true}"#.into()),
    });
}

// ── Class body extraction ────────────────────────────────────────

fn extract_class_body(
    body: &tree_sitter::Node,
    src: &[u8],
    class_node_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "method_declaration" => {
                extract_method(&child, src, class_node_uid, enclosing_type_chain, ctx);
            }
            "constructor_declaration" => {
                extract_constructor(&child, src, class_node_uid, enclosing_type_chain, ctx);
            }
            "field_declaration" => {
                extract_field(&child, src, class_node_uid, enclosing_type_chain, ctx);
            }
            "class_declaration" => {
                // Nested class: pass current type chain as enclosing
                extract_class(&child, src, ctx, Some(enclosing_type_chain), class_node_uid);
            }
            "interface_declaration" => {
                extract_interface(&child, src, ctx, Some(enclosing_type_chain), class_node_uid);
            }
            "enum_declaration" => {
                extract_enum(&child, src, ctx, Some(enclosing_type_chain), class_node_uid);
            }
            "record_declaration" => {
                // Nested record (Java 14+)
                extract_record(&child, src, ctx, Some(enclosing_type_chain), class_node_uid);
            }
            "annotation_type_declaration" => {
                // Nested @interface
                extract_annotation_type(&child, src, ctx, Some(enclosing_type_chain), class_node_uid);
            }
            _ => {}
        }
    }
}

// ── Interface body extraction ────────────────────────────────────

fn extract_interface_body(
    body: &tree_sitter::Node,
    src: &[u8],
    interface_node_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "method_declaration" => {
                extract_method(&child, src, interface_node_uid, enclosing_type_chain, ctx);
            }
            "constant_declaration" => {
                // Interface constants are fields
                extract_field(&child, src, interface_node_uid, enclosing_type_chain, ctx);
            }
            // Interfaces can have nested types
            "class_declaration" => {
                extract_class(&child, src, ctx, Some(enclosing_type_chain), interface_node_uid);
            }
            "interface_declaration" => {
                extract_interface(&child, src, ctx, Some(enclosing_type_chain), interface_node_uid);
            }
            "enum_declaration" => {
                extract_enum(&child, src, ctx, Some(enclosing_type_chain), interface_node_uid);
            }
            "record_declaration" => {
                extract_record(&child, src, ctx, Some(enclosing_type_chain), interface_node_uid);
            }
            "annotation_type_declaration" => {
                extract_annotation_type(&child, src, ctx, Some(enclosing_type_chain), interface_node_uid);
            }
            _ => {}
        }
    }
}

// ── Enum body extraction ─────────────────────────────────────────

fn extract_enum_body(
    body: &tree_sitter::Node,
    src: &[u8],
    enum_node_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "enum_constant" => {
                extract_enum_constant(&child, src, enum_node_uid, enclosing_type_chain, ctx);
            }
            "method_declaration" => {
                extract_method(&child, src, enum_node_uid, enclosing_type_chain, ctx);
            }
            "constructor_declaration" => {
                extract_constructor(&child, src, enum_node_uid, enclosing_type_chain, ctx);
            }
            "field_declaration" => {
                extract_field(&child, src, enum_node_uid, enclosing_type_chain, ctx);
            }
            // Enums can have nested types
            "class_declaration" => {
                extract_class(&child, src, ctx, Some(enclosing_type_chain), enum_node_uid);
            }
            "interface_declaration" => {
                extract_interface(&child, src, ctx, Some(enclosing_type_chain), enum_node_uid);
            }
            "enum_declaration" => {
                extract_enum(&child, src, ctx, Some(enclosing_type_chain), enum_node_uid);
            }
            "record_declaration" => {
                extract_record(&child, src, ctx, Some(enclosing_type_chain), enum_node_uid);
            }
            "annotation_type_declaration" => {
                extract_annotation_type(&child, src, ctx, Some(enclosing_type_chain), enum_node_uid);
            }
            _ => {}
        }
    }
}

// ── Method extraction ────────────────────────────────────────────

fn extract_method(
    node: &tree_sitter::Node,
    src: &[u8],
    parent_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);
    let signature = extract_method_signature(node, src);
    let param_count = count_parameters(node);

    // Use enclosing type for stable key to distinguish methods across types
    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Method, Some(enclosing_type_chain));
    let node_uid = uuid::Uuid::new_v4().to_string();

    // Qualified name: package.EnclosingType.methodName
    let qualified_name = match &ctx.package_name {
        Some(pkg) => format!("{}.{}.{}", pkg, enclosing_type_chain, name),
        None => format!("{}.{}", enclosing_type_chain, name),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: stable_key.clone(),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Method),
        name: name.clone(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_uid.into()),
        location: Some(node_location(node)),
        signature,
        visibility,
        doc_comment,
        metadata_json: annotations,
    });

    // Compute and store metrics
    let metrics = compute_method_metrics(*node, src, param_count);
    ctx.metrics.insert(stable_key, metrics);

    // Extract calls from method body
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_body(&body, src, &node_uid, ctx);
    }
}

// ── Constructor extraction ───────────────────────────────────────

fn extract_constructor(
    node: &tree_sitter::Node,
    src: &[u8],
    parent_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    // Constructor name is the simple class name (last component of chain)
    let simple_name = enclosing_type_chain
        .rsplit('.')
        .next()
        .unwrap_or(enclosing_type_chain);

    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);
    let doc_comment = extract_doc_comment(node, src);
    let signature = extract_constructor_signature(node, src);
    let param_count = count_parameters(node);

    // Constructor stable key uses the enclosing type chain directly, NOT appended.
    // For Foo, key is #Foo:SYMBOL:CONSTRUCTOR
    // For Outer.Inner, key is #Outer.Inner:SYMBOL:CONSTRUCTOR
    // This is because the constructor IS the type, not a member named after it.
    let stable_key = ctx.make_stable_key(enclosing_type_chain, &NodeSubtype::Constructor, None);
    let node_uid = uuid::Uuid::new_v4().to_string();

    // Qualified name includes package
    let qualified_name = match &ctx.package_name {
        Some(pkg) => format!("{}.{}.<init>", pkg, enclosing_type_chain),
        None => format!("{}.<init>", enclosing_type_chain),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: node_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: stable_key.clone(),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Constructor),
        name: simple_name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_uid.into()),
        location: Some(node_location(node)),
        signature,
        visibility,
        doc_comment,
        metadata_json: annotations,
    });

    // Compute and store metrics
    let metrics = compute_method_metrics(*node, src, param_count);
    ctx.metrics.insert(stable_key, metrics);

    // Extract calls from constructor body
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_body(&body, src, &node_uid, ctx);
    }
}

// ── Field extraction ─────────────────────────────────────────────

fn extract_field(
    node: &tree_sitter::Node,
    src: &[u8],
    parent_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    // field_declaration has one or more variable_declarator children
    let visibility = extract_visibility(node, src);
    let annotations = extract_annotations(node, src);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(&name_node, src);

                let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Property, Some(enclosing_type_chain));
                let node_uid = uuid::Uuid::new_v4().to_string();

                // Qualified name: package.EnclosingType.fieldName
                let qualified_name = match &ctx.package_name {
                    Some(pkg) => format!("{}.{}.{}", pkg, enclosing_type_chain, name),
                    None => format!("{}.{}", enclosing_type_chain, name),
                };

                ctx.nodes.push(ExtractedNode {
                    node_uid,
                    snapshot_uid: ctx.snapshot_uid.into(),
                    repo_uid: ctx.repo_uid.into(),
                    stable_key,
                    kind: NodeKind::Symbol,
                    subtype: Some(NodeSubtype::Property),
                    name,
                    qualified_name: Some(qualified_name),
                    file_uid: Some(ctx.file_uid.into()),
                    parent_node_uid: Some(parent_uid.into()),
                    location: Some(node_location(&child)),
                    signature: None,
                    visibility: visibility.clone(),
                    doc_comment: None,
                    metadata_json: annotations.clone(),
                });
            }
        }
    }
}

// ── Enum constant extraction ─────────────────────────────────────

fn extract_enum_constant(
    node: &tree_sitter::Node,
    src: &[u8],
    parent_uid: &str,
    enclosing_type_chain: &str,
    ctx: &mut ExtractionCtx,
) {
    let name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::EnumMember, Some(enclosing_type_chain));

    // Qualified name: package.EnumType.CONSTANT_NAME
    let qualified_name = match &ctx.package_name {
        Some(pkg) => format!("{}.{}.{}", pkg, enclosing_type_chain, name),
        None => format!("{}.{}", enclosing_type_chain, name),
    };

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::EnumMember),
        name,
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_uid.into()),
        location: Some(node_location(node)),
        signature: None,
        visibility: Some(Visibility::Public),
        doc_comment: None,
        metadata_json: None,
    });
}

// ── Extends/implements extraction ────────────────────────────────

fn extract_extends(
    superclass: &tree_sitter::Node,
    src: &[u8],
    class_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // superclass node contains: "extends" keyword + type_identifier/generic_type
    // Find the actual type node (skip the "extends" keyword)
    let mut cursor = superclass.walk();
    for child in superclass.children(&mut cursor) {
        if let Some(name) = extract_type_name(&child, src) {
            ctx.edges.push(ExtractedEdge {
                edge_uid: uuid::Uuid::new_v4().to_string(),
                snapshot_uid: ctx.snapshot_uid.into(),
                repo_uid: ctx.repo_uid.into(),
                source_node_uid: class_node_uid.into(),
                target_key: name,
                edge_type: EdgeType::Implements, // IMPLEMENTS for extends too (hierarchy edge)
                resolution: Resolution::Static,
                extractor: EXTRACTOR_NAME.into(),
                location: Some(node_location(superclass)),
                metadata_json: Some(r#"{"relation":"extends"}"#.into()),
            });
            break; // Only one superclass in Java
        }
    }
}

fn extract_implements(
    interfaces: &tree_sitter::Node,
    src: &[u8],
    class_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // interfaces is super_interfaces containing: "implements" keyword + type_list
    // Find the type_list and iterate its children
    let mut cursor = interfaces.walk();
    for child in interfaces.children(&mut cursor) {
        if child.kind() == "type_list" {
            // Iterate type_list children for individual types
            let mut type_cursor = child.walk();
            for type_child in child.children(&mut type_cursor) {
                if let Some(type_name) = extract_type_name(&type_child, src) {
                    ctx.edges.push(ExtractedEdge {
                        edge_uid: uuid::Uuid::new_v4().to_string(),
                        snapshot_uid: ctx.snapshot_uid.into(),
                        repo_uid: ctx.repo_uid.into(),
                        source_node_uid: class_node_uid.into(),
                        target_key: type_name,
                        edge_type: EdgeType::Implements,
                        resolution: Resolution::Static,
                        extractor: EXTRACTOR_NAME.into(),
                        location: Some(node_location(&type_child)),
                        metadata_json: None,
                    });
                }
            }
        } else if let Some(type_name) = extract_type_name(&child, src) {
            // Direct type (no type_list wrapper)
            ctx.edges.push(ExtractedEdge {
                edge_uid: uuid::Uuid::new_v4().to_string(),
                snapshot_uid: ctx.snapshot_uid.into(),
                repo_uid: ctx.repo_uid.into(),
                source_node_uid: class_node_uid.into(),
                target_key: type_name,
                edge_type: EdgeType::Implements,
                resolution: Resolution::Static,
                extractor: EXTRACTOR_NAME.into(),
                location: Some(node_location(&child)),
                metadata_json: None,
            });
        }
    }
}

// ── Call extraction ──────────────────────────────────────────────

fn extract_calls_from_body(
    body: &tree_sitter::Node,
    src: &[u8],
    enclosing_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    fn walk_for_calls(
        node: tree_sitter::Node,
        src: &[u8],
        enclosing_node_uid: &str,
        ctx: &mut ExtractionCtx,
    ) {
        match node.kind() {
            "method_invocation" => {
                extract_method_call(&node, src, enclosing_node_uid, ctx);
            }
            "object_creation_expression" => {
                extract_constructor_call(&node, src, enclosing_node_uid, ctx);
            }
            _ => {}
        }

        // Recurse
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_for_calls(child, src, enclosing_node_uid, ctx);
        }
    }

    walk_for_calls(*body, src, enclosing_node_uid, ctx);
}

fn extract_method_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // method_invocation: [object.]name(arguments)
    let method_name = match node.child_by_field_name("name") {
        Some(n) => node_text(&n, src),
        None => return,
    };

    // Check for receiver (object)
    // P2 fix: preserve receiver text for classifier provenance, don't replace with "?"
    let target_key = if let Some(obj) = node.child_by_field_name("object") {
        let obj_text = node_text(&obj, src);
        // Preserve the receiver text in all cases:
        // - "this.save()" → "this.save"
        // - "repo.find()" → "repo.find"
        // - "System.out.println()" → "System.out.println" (chained access)
        // - "Math.abs()" → "Math.abs"
        // The classifier can distinguish static-looking (uppercase) from instance calls
        format!("{}.{}", obj_text, method_name)
    } else {
        // Unqualified local call
        method_name.clone()
    };

    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: enclosing_node_uid.into(),
        target_key,
        edge_type: EdgeType::Calls,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(node_location(node)),
        metadata_json: None,
    });
}

fn extract_constructor_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // object_creation_expression: new type(arguments)
    let type_node = node.child_by_field_name("type");
    let type_name = type_node.and_then(|t| extract_type_name(&t, src));

    let Some(class_name) = type_name else { return };

    // INSTANTIATES edge targets the class name per design doc
    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: enclosing_node_uid.into(),
        target_key: class_name,
        edge_type: EdgeType::Instantiates,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(node_location(node)),
        metadata_json: None,
    });
}

// ── Helper functions ─────────────────────────────────────────────

fn node_text(node: &tree_sitter::Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

fn node_location(node: &tree_sitter::Node) -> SourceLocation {
    let start = node.start_position();
    let end = node.end_position();
    SourceLocation {
        line_start: (start.row + 1) as i64,
        col_start: start.column as i64,
        line_end: (end.row + 1) as i64,
        col_end: end.column as i64,
    }
}

fn extract_visibility(node: &tree_sitter::Node, _src: &[u8]) -> Option<Visibility> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mut mod_cursor = child.walk();
            for modifier in child.children(&mut mod_cursor) {
                match modifier.kind() {
                    "public" => return Some(Visibility::Public),
                    "private" => return Some(Visibility::Private),
                    "protected" => return Some(Visibility::Protected),
                    _ => {}
                }
            }
        }
    }
    // Package-private (default) maps to Internal
    Some(Visibility::Internal)
}

fn extract_annotations(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut annotations: Vec<serde_json::Value> = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mut mod_cursor = child.walk();
            for modifier in child.children(&mut mod_cursor) {
                if modifier.kind() == "annotation" || modifier.kind() == "marker_annotation" {
                    let ann = extract_single_annotation(&modifier, src);
                    annotations.push(ann);
                }
            }
        }
    }

    if annotations.is_empty() {
        None
    } else {
        let obj = serde_json::json!({ "annotations": annotations });
        Some(serde_json::to_string(&obj).unwrap_or_default())
    }
}

fn extract_single_annotation(node: &tree_sitter::Node, src: &[u8]) -> serde_json::Value {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, src))
        .unwrap_or_else(|| {
            // For marker_annotation without name field, the whole text minus @
            let text = node_text(node, src);
            text.trim_start_matches('@').to_string()
        });

    // Extract arguments if present
    let arguments = node.child_by_field_name("arguments").map(|args| {
        let text = node_text(&args, src);
        // Simple parse: just capture the text for now
        serde_json::Value::String(text)
    });

    match arguments {
        Some(args) => serde_json::json!({ "name": name, "arguments": args }),
        None => serde_json::json!({ "name": name }),
    }
}

fn extract_doc_comment(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    // Check for preceding block_comment that starts with /**
    if let Some(prev) = node.prev_sibling() {
        if prev.kind() == "block_comment" {
            let text = node_text(&prev, src);
            if text.starts_with("/**") {
                // Strip /** and */ and trim
                let doc = text
                    .trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .lines()
                    .map(|l| l.trim().trim_start_matches('*').trim())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();
                if !doc.is_empty() {
                    return Some(doc);
                }
            }
        }
    }
    None
}

fn extract_method_signature(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    // Build signature from parameters
    let params = node.child_by_field_name("parameters")?;
    let param_text = node_text(&params, src);
    Some(param_text)
}

fn extract_constructor_signature(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    let params = node.child_by_field_name("parameters")?;
    let param_text = node_text(&params, src);
    Some(param_text)
}

fn count_parameters(node: &tree_sitter::Node) -> u32 {
    let Some(params) = node.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() == "formal_parameter" || child.kind() == "spread_parameter" {
            count += 1;
        }
    }
    count
}

fn extract_type_name(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" | "identifier" => Some(node_text(node, src)),
        "generic_type" => {
            // Generic type: get the base type name
            node.child(0).map(|c| node_text(&c, src))
        }
        "scoped_type_identifier" => {
            // Qualified name: com.example.Foo
            Some(node_text(node, src))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_java(source: &str) -> ExtractionResult {
        let mut extractor = JavaExtractor::new();
        extractor.initialize().unwrap();
        extractor
            .extract(
                source,
                "Test.java",
                "test-repo:Test.java",
                "test-repo",
                "snap-1",
            )
            .unwrap()
    }

    #[test]
    fn extracts_file_node() {
        let result = extract_java("class Foo {}");
        assert_eq!(result.nodes.len(), 2); // FILE + CLASS
        assert_eq!(result.nodes[0].kind, NodeKind::File);
        assert_eq!(result.nodes[0].name, "Test.java");
    }

    #[test]
    fn extracts_class() {
        let result = extract_java("public class Foo {}");
        let class_node = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        assert_eq!(class_node.name, "Foo");
        assert_eq!(class_node.visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_interface() {
        let result = extract_java("public interface Bar {}");
        let iface = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Interface))
            .unwrap();
        assert_eq!(iface.name, "Bar");
    }

    #[test]
    fn extracts_enum() {
        let result = extract_java("enum Color { RED, GREEN, BLUE }");
        let enum_node = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Enum))
            .unwrap();
        assert_eq!(enum_node.name, "Color");

        // Enum constants
        let constants: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::EnumMember))
            .collect();
        assert_eq!(constants.len(), 3);
    }

    #[test]
    fn extracts_method() {
        let result = extract_java(
            r#"
            class Foo {
                public void doWork(int x) {}
            }
        "#,
        );
        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        assert_eq!(method.name, "doWork");
        assert!(method.signature.is_some());
    }

    #[test]
    fn extracts_constructor() {
        let result = extract_java(
            r#"
            class Foo {
                public Foo(String name) {}
            }
        "#,
        );
        let ctor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();
        assert_eq!(ctor.name, "Foo");
    }

    #[test]
    fn extracts_field() {
        let result = extract_java(
            r#"
            class Foo {
                private String name;
            }
        "#,
        );
        let field = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Property))
            .unwrap();
        assert_eq!(field.name, "name");
        assert_eq!(field.visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_import() {
        let result = extract_java("import java.util.List;");
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].edge_type, EdgeType::Imports);
        assert_eq!(result.edges[0].target_key, "java.util.List");
        assert_eq!(result.import_bindings.len(), 1);
        assert_eq!(result.import_bindings[0].identifier, "List");
    }

    #[test]
    fn extracts_wildcard_import() {
        let result = extract_java("import java.util.*;");
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].target_key, "java.util.*");
        // Wildcard imports do NOT create ImportBinding per design doc
        assert_eq!(result.import_bindings.len(), 0);
    }

    #[test]
    fn extracts_method_call() {
        let result = extract_java(
            r#"
            class Foo {
                void bar() {
                    System.out.println("hello");
                }
            }
        "#,
        );
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();
        assert!(!calls.is_empty());
    }

    #[test]
    fn extracts_constructor_call() {
        let result = extract_java(
            r#"
            class Foo {
                void bar() {
                    Object obj = new ArrayList();
                }
            }
        "#,
        );
        let instantiates: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Instantiates)
            .collect();
        assert_eq!(instantiates.len(), 1);
        assert_eq!(instantiates[0].target_key, "ArrayList");
    }

    #[test]
    fn extracts_implements() {
        let result = extract_java("class Foo implements Bar, Baz {}");
        let implements: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Implements)
            .collect();
        assert_eq!(implements.len(), 2);
    }

    #[test]
    fn extracts_extends() {
        let result = extract_java("class Foo extends Bar {}");
        let extends: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Implements)
            .collect();
        assert_eq!(extends.len(), 1);
        // Check metadata indicates extends
        assert!(extends[0]
            .metadata_json
            .as_ref()
            .unwrap()
            .contains("extends"));
    }

    #[test]
    fn extracts_annotations() {
        let result = extract_java(
            r#"
            @Override
            @Deprecated
            public class Foo {}
        "#,
        );
        let class_node = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        let metadata = class_node.metadata_json.as_ref().unwrap();
        assert!(metadata.contains("Override"));
        assert!(metadata.contains("Deprecated"));
    }

    #[test]
    fn handles_overloaded_methods() {
        let result = extract_java(
            r#"
            class Foo {
                void process(int x) {}
                void process(String s) {}
            }
        "#,
        );
        let methods: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::Method))
            .collect();
        assert_eq!(methods.len(), 2);
        // Second method should have :dup2 suffix
        let keys: Vec<_> = methods.iter().map(|m| &m.stable_key).collect();
        assert!(keys.iter().any(|k| k.contains(":dup2")));
    }

    #[test]
    fn computes_metrics() {
        let result = extract_java(
            r#"
            class Foo {
                void bar(int x) {
                    if (x > 0) {
                        System.out.println("positive");
                    }
                }
            }
        "#,
        );
        assert!(!result.metrics.is_empty());
        let metrics = result.metrics.values().next().unwrap();
        assert_eq!(metrics.cyclomatic_complexity, 2);
        assert_eq!(metrics.parameter_count, 1);
    }

    #[test]
    fn methods_in_different_classes_have_distinct_keys() {
        // P1 fix: methods with same name in different classes must NOT collide
        let result = extract_java(
            r#"
            class Foo {
                void doWork() {}
            }
            class Bar {
                void doWork() {}
            }
        "#,
        );
        let methods: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::Method))
            .collect();
        assert_eq!(methods.len(), 2);

        // Keys should include enclosing type, so no :dup2 needed
        let keys: Vec<_> = methods.iter().map(|m| &m.stable_key).collect();
        assert!(!keys.iter().any(|k| k.contains(":dup")));
        // One key should have Foo.doWork, the other Bar.doWork
        assert!(keys.iter().any(|k| k.contains("#Foo.doWork")));
        assert!(keys.iter().any(|k| k.contains("#Bar.doWork")));
    }

    #[test]
    fn nested_class_preserves_hierarchy() {
        // P1 fix: nested types must be parented to enclosing type, not file
        let result = extract_java(
            r#"
            class Outer {
                class Inner {
                    void work() {}
                }
            }
        "#,
        );

        let outer = result
            .nodes
            .iter()
            .find(|n| n.name == "Outer" && n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        let inner = result
            .nodes
            .iter()
            .find(|n| n.name == "Inner" && n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        let work = result
            .nodes
            .iter()
            .find(|n| n.name == "work" && n.subtype == Some(NodeSubtype::Method))
            .unwrap();

        // Inner should be parented to Outer
        assert_eq!(inner.parent_node_uid.as_ref().unwrap(), &outer.node_uid);

        // Inner's stable key should include Outer
        assert!(inner.stable_key.contains("#Outer.Inner"));

        // work's stable key should include Outer.Inner
        assert!(work.stable_key.contains("#Outer.Inner.work"));

        // work should be parented to Inner
        assert_eq!(work.parent_node_uid.as_ref().unwrap(), &inner.node_uid);
    }

    #[test]
    fn method_call_preserves_receiver() {
        // P2 fix: receiver text should be preserved in target key
        let result = extract_java(
            r#"
            class Foo {
                Repository repo;
                void bar() {
                    this.save();
                    repo.find();
                }
            }
        "#,
        );
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        // this.save() should preserve "this" receiver
        assert!(calls.iter().any(|c| c.target_key == "this.save"));
        // repo.find() should preserve "repo" receiver
        assert!(calls.iter().any(|c| c.target_key == "repo.find"));
    }

    #[test]
    fn constructor_stable_key_no_duplication() {
        // P1 fix: constructor key should be #Foo:SYMBOL:CONSTRUCTOR, not #Foo.Foo
        let result = extract_java(
            r#"
            class Foo {
                public Foo() {}
            }
        "#,
        );
        let ctor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();

        // Key should be #Foo:SYMBOL:CONSTRUCTOR, NOT #Foo.Foo:SYMBOL:CONSTRUCTOR
        assert!(ctor.stable_key.contains("#Foo:SYMBOL:CONSTRUCTOR"));
        assert!(!ctor.stable_key.contains("#Foo.Foo"));
    }

    #[test]
    fn nested_constructor_stable_key() {
        // For Outer.Inner, constructor key should be #Outer.Inner:SYMBOL:CONSTRUCTOR
        let result = extract_java(
            r#"
            class Outer {
                class Inner {
                    public Inner() {}
                }
            }
        "#,
        );
        let ctor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();

        // Key should be #Outer.Inner:SYMBOL:CONSTRUCTOR
        assert!(ctor.stable_key.contains("#Outer.Inner:SYMBOL:CONSTRUCTOR"));
        // Should NOT have #Outer.Inner.Inner
        assert!(!ctor.stable_key.contains("#Outer.Inner.Inner"));
    }

    #[test]
    fn members_have_qualified_names() {
        // P2 fix: members should have qualified_name populated
        let result = extract_java(
            r#"
            package com.example;
            class Foo {
                private String name;
                public Foo() {}
                void doWork() {}
            }
        "#,
        );

        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        assert_eq!(
            method.qualified_name.as_ref().unwrap(),
            "com.example.Foo.doWork"
        );

        let field = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Property))
            .unwrap();
        assert_eq!(
            field.qualified_name.as_ref().unwrap(),
            "com.example.Foo.name"
        );

        let ctor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();
        assert_eq!(
            ctor.qualified_name.as_ref().unwrap(),
            "com.example.Foo.<init>"
        );
    }

    #[test]
    fn nested_record_extracted() {
        // P2 fix: nested records should be extracted with proper hierarchy
        // Note: tree-sitter-java 0.23+ supports Java 14+ records
        let result = extract_java(
            r#"
            class Outer {
                record Inner(String name) {}
            }
        "#,
        );

        // Find nested record - MUST exist (tree-sitter-java 0.23+ supports records)
        let inner = result
            .nodes
            .iter()
            .find(|n| n.name == "Inner" && n.subtype == Some(NodeSubtype::Class))
            .expect("nested record Inner must be extracted");

        // Should be parented to Outer
        let outer = result
            .nodes
            .iter()
            .find(|n| n.name == "Outer" && n.subtype == Some(NodeSubtype::Class))
            .expect("enclosing class Outer must be extracted");
        assert_eq!(inner.parent_node_uid.as_ref().unwrap(), &outer.node_uid);

        // Stable key should include Outer
        assert!(inner.stable_key.contains("#Outer.Inner"));

        // Metadata should indicate isRecord
        assert!(inner.metadata_json.as_ref().unwrap().contains("isRecord"));
    }

    #[test]
    fn nested_annotation_type_extracted() {
        // P2 fix: nested @interface should be extracted with proper hierarchy
        let result = extract_java(
            r#"
            class Outer {
                @interface Marker {}
            }
        "#,
        );

        // Find nested annotation type - MUST exist
        let marker = result
            .nodes
            .iter()
            .find(|n| n.name == "Marker" && n.subtype == Some(NodeSubtype::Interface))
            .expect("nested annotation type Marker must be extracted");

        // Should be parented to Outer
        let outer = result
            .nodes
            .iter()
            .find(|n| n.name == "Outer" && n.subtype == Some(NodeSubtype::Class))
            .expect("enclosing class Outer must be extracted");
        assert_eq!(marker.parent_node_uid.as_ref().unwrap(), &outer.node_uid);

        // Stable key should include Outer
        assert!(marker.stable_key.contains("#Outer.Marker"));

        // Metadata should indicate isAnnotationType
        assert!(marker.metadata_json.as_ref().unwrap().contains("isAnnotationType"));
    }
}
