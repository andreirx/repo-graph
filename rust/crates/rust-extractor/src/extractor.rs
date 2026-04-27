//! Core extractor implementation.

use std::collections::{BTreeMap, HashSet};

use repo_graph_classification::types::{ImportBinding, RuntimeBuiltinsSet, SourceLocation};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::types::{
    EdgeType, ExtractionResult, ExtractedEdge, ExtractedMetrics, ExtractedNode, NodeKind,
    NodeSubtype, Resolution, Visibility,
};
use tree_sitter::{Node, Parser};

use crate::builtins::rust_runtime_builtins;

/// Extractor name and version. Mirrors `EXTRACTOR_VERSIONS.rust`
/// from the TS side.
const EXTRACTOR_NAME: &str = "rust-core:0.2.0";

/// The language identifier this extractor handles.
const LANGUAGES: &[&str] = &["rust"];

/// Macro-like builtins that produce noise in the call graph.
/// These are typically invoked as macros (println!, format!, etc.)
/// but tree-sitter may parse some as identifiers.
const MACRO_BUILTINS: &[&str] = &[
    "println",
    "eprintln",
    "print",
    "eprint",
    "format",
    "write",
    "writeln",
    "todo",
    "unimplemented",
    "unreachable",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "dbg",
    "vec",
    "cfg",
];

/// Extraction context for a single file.
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
    /// Stable keys already emitted. Used for #[cfg] deduplication.
    emitted_stable_keys: HashSet<String>,
}

/// Concrete `ExtractorPort` adapter for Rust source files.
///
/// Uses native tree-sitter with compiled-in grammar from
/// `tree-sitter-rust`.
pub struct RustExtractor {
    languages: Vec<String>,
    builtins: RuntimeBuiltinsSet,
    parser: Option<Parser>,
    rust_language: tree_sitter::Language,
}

impl RustExtractor {
    /// Create a new extractor. Call `initialize()` before `extract()`.
    pub fn new() -> Self {
        Self {
            languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
            builtins: rust_runtime_builtins(),
            parser: None,
            rust_language: tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

impl Default for RustExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorPort for RustExtractor {
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
        let mut parser = Parser::new();
        parser
            .set_language(&self.rust_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set Rust grammar: {}", e),
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
        let _parser = self.parser.as_ref().ok_or_else(|| ExtractorError {
            message: "extractor not initialized -- call initialize() first".into(),
        })?;

        // Clone parser to avoid mutability issues (tree-sitter requires &mut).
        let mut parser_clone = Parser::new();
        parser_clone
            .set_language(&self.rust_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set grammar for {}: {}", file_path, e),
            })?;

        let tree = parser_clone
            .parse(source, None)
            .ok_or_else(|| ExtractorError {
                message: format!("tree-sitter returned null tree for {}", file_path),
            })?;

        let root = tree.root_node();

        // -- FILE node --
        // TS uses `source.split("\n").length` which counts trailing newline.
        // Mirror that behavior.
        let line_count = source.split('\n').count().max(1) as i64;
        let file_node_uid = uuid::Uuid::new_v4().to_string();
        let file_name = file_path.rsplit('/').next().unwrap_or(file_path);

        let file_node = ExtractedNode {
            node_uid: file_node_uid.clone(),
            snapshot_uid: snapshot_uid.into(),
            repo_uid: repo_uid.into(),
            stable_key: format!("{}:{}:FILE", repo_uid, file_path),
            kind: NodeKind::File,
            subtype: Some(NodeSubtype::Source),
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
        };

        let mut ctx = ExtractionCtx {
            file_path,
            file_uid,
            file_node_uid: &file_node_uid,
            repo_uid,
            snapshot_uid,
            nodes: vec![file_node],
            edges: Vec::new(),
            import_bindings: Vec::new(),
            metrics: BTreeMap::new(),
            emitted_stable_keys: HashSet::new(),
        };

        // -- Walk top-level statements --
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            visit_top_level(&child, source, &mut ctx);
        }

        Ok(ExtractionResult {
            nodes: ctx.nodes,
            edges: ctx.edges,
            metrics: ctx.metrics,
            import_bindings: ctx.import_bindings,
            resolved_callsites: Vec::new(), // Fork-1 posture: empty
        })
    }
}

// -- Top-level visitor ------------------------------------------------------

fn visit_top_level(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    match node.kind() {
        "use_declaration" => extract_use_declaration(node, source, ctx),
        "function_item" => extract_function(node, source, ctx),
        "struct_item" => extract_struct(node, source, ctx),
        "enum_item" => extract_enum(node, source, ctx),
        "trait_item" => extract_trait(node, source, ctx),
        "impl_item" => extract_impl(node, source, ctx),
        "const_item" => extract_const(node, source, ctx),
        "static_item" => extract_static(node, source, ctx),
        "type_item" => extract_type_alias(node, source, ctx),
        "mod_item" => extract_mod_item(node, source, ctx),
        _ => {}
    }
}

// -- Use declaration extraction ---------------------------------------------

fn extract_use_declaration(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let location = location_from_node(node);
    let bindings = collect_use_bindings(node, source);

    for binding in bindings {
        let is_relative = binding.specifier.starts_with("crate::")
            || binding.specifier.starts_with("super::")
            || binding.specifier.starts_with("self::");

        // ImportBinding record
        ctx.import_bindings.push(ImportBinding {
            identifier: binding.identifier.clone(),
            specifier: binding.specifier.clone(),
            is_relative,
            location: Some(location),
            is_type_only: false, // Rust `use` imports both types and values
            imported_name: Some(binding.imported_name.clone()), // Original name, not alias
        });

        // IMPORTS edge
        let target_key = if is_relative {
            format!(
                "{}:{}:FILE",
                ctx.repo_uid,
                rust_module_to_path(&binding.specifier)
            )
        } else {
            binding.specifier.clone()
        };

        ctx.edges.push(ExtractedEdge {
            edge_uid: uuid::Uuid::new_v4().to_string(),
            snapshot_uid: ctx.snapshot_uid.into(),
            repo_uid: ctx.repo_uid.into(),
            source_node_uid: ctx.file_node_uid.into(),
            target_key,
            edge_type: EdgeType::Imports,
            resolution: Resolution::Static,
            extractor: EXTRACTOR_NAME.into(),
            location: Some(location),
            metadata_json: Some(
                serde_json::json!({
                    "specifier": binding.specifier,
                    "identifier": binding.identifier
                })
                .to_string(),
            ),
        });
    }
}

/// A use binding: identifier, module specifier, and original imported name.
struct UseBinding {
    /// Local name after any aliasing (e.g., "Baz" in `use foo::Bar as Baz`).
    identifier: String,
    /// Module specifier (e.g., "foo" in `use foo::Bar as Baz`).
    specifier: String,
    /// Original exported name before aliasing (e.g., "Bar" in `use foo::Bar as Baz`).
    /// Same as `identifier` when no alias is used.
    imported_name: String,
}

/// Collect all identifier/specifier pairs from a use_declaration.
fn collect_use_bindings(node: &Node, source: &str) -> Vec<UseBinding> {
    let mut results = Vec::new();
    walk_use_tree(node, source, &[], &mut results);
    results
}

/// Recursively walk the use tree collecting bindings.
fn walk_use_tree(
    node: &Node,
    source: &str,
    path_prefix: &[&str],
    results: &mut Vec<UseBinding>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "scoped_identifier" => {
                // e.g., `std::collections::HashMap`
                let full_path = node_text(&child, source);
                let segments: Vec<&str> = full_path.split("::").collect();
                if let Some((last, rest)) = segments.split_last() {
                    results.push(UseBinding {
                        identifier: last.to_string(),
                        specifier: rest.join("::"),
                        imported_name: last.to_string(), // No alias
                    });
                }
            }
            "identifier" => {
                // Bare identifier at use level
                if node.kind() == "use_declaration" || node.kind() == "use_list" {
                    let specifier = path_prefix.join("::");
                    let identifier = node_text(&child, source);
                    results.push(UseBinding {
                        identifier: identifier.to_string(),
                        specifier: if specifier.is_empty() {
                            identifier.to_string()
                        } else {
                            specifier
                        },
                        imported_name: identifier.to_string(), // No alias
                    });
                }
            }
            "use_as_clause" => {
                let path_node = child.child_by_field_name("path");
                let alias_node = child.child_by_field_name("alias");
                if let (Some(path), Some(alias)) = (path_node, alias_node) {
                    // `use foo::Bar as Baz;` → identifier="Baz", imported_name="Bar"
                    let full_path = node_text(&path, source);
                    let segments: Vec<&str> = full_path.split("::").collect();
                    let mut all_segments: Vec<&str> = path_prefix.to_vec();
                    // Extract the original name (last segment of path before `as`).
                    let original_name = segments.last().map(|s| s.to_string()).unwrap_or_default();
                    if segments.len() > 1 {
                        all_segments.extend(&segments[..segments.len() - 1]);
                    }
                    results.push(UseBinding {
                        identifier: node_text(&alias, source).to_string(),
                        specifier: all_segments.join("::"),
                        imported_name: original_name, // Original exported name
                    });
                } else if let Some(path) = path_node {
                    let full_path = node_text(&path, source);
                    let segments: Vec<&str> = full_path.split("::").collect();
                    if let Some((last, rest)) = segments.split_last() {
                        let mut all_segments: Vec<&str> = path_prefix.to_vec();
                        all_segments.extend(rest);
                        results.push(UseBinding {
                            identifier: last.to_string(),
                            specifier: all_segments.join("::"),
                            imported_name: last.to_string(), // No alias
                        });
                    }
                }
            }
            "scoped_use_list" => {
                let path_node = child.child_by_field_name("path");
                let list_node = child.child_by_field_name("list");
                let mut new_prefix: Vec<&str> = path_prefix.to_vec();
                if let Some(path) = path_node {
                    let path_text = node_text(&path, source);
                    for segment in path_text.split("::") {
                        new_prefix.push(segment);
                    }
                }
                if let Some(list) = list_node {
                    // Need to leak the strings to satisfy lifetime requirements
                    let prefix_owned: Vec<String> = new_prefix.iter().map(|s| s.to_string()).collect();
                    let prefix_refs: Vec<&str> = prefix_owned.iter().map(|s| s.as_str()).collect();
                    walk_use_tree(&list, source, &prefix_refs, results);
                }
            }
            "use_list" => {
                walk_use_tree(&child, source, path_prefix, results);
            }
            "use_wildcard" => {
                // Wildcard imports (`use foo::*`) intentionally do NOT produce
                // ImportBinding records. The symbol set brought in by `*` is
                // indeterminate at parse time — we would have to fully resolve
                // the target module to enumerate its exports. For dependency
                // analysis, use the FILE-level dependency tracking from Cargo.toml.
                // This arm is explicitly empty to document the intentional skip.
            }
            _ => {}
        }
    }
}

/// Convert a Rust module path to a file-system-like path.
/// `crate::module::sub` -> `src/module/sub`
fn rust_module_to_path(specifier: &str) -> String {
    let parts: Vec<&str> = specifier.split("::").collect();
    if parts.first() == Some(&"crate") {
        let mut result = vec!["src"];
        result.extend(&parts[1..]);
        result.join("/")
    } else {
        parts.join("/")
    }
}

// -- Function extraction ----------------------------------------------------

fn extract_function(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let params = node.child_by_field_name("parameters");
    let return_type = node.child_by_field_name("return_type");

    let sig = match (params, return_type) {
        (Some(p), Some(r)) => {
            format!("fn {}{} -> {}", name, node_text(&p, source), node_text(&r, source))
        }
        (Some(p), None) => format!("fn {}{}", name, node_text(&p, source)),
        _ => format!("fn {}()", name),
    };

    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::Function,
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        Some(&sig),
        node,
        source,
        ctx,
    );

    if !emit_node(graph_node.clone(), ctx) {
        return;
    }

    // Extract calls from function body
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_node(&body, source, ctx, &graph_node.node_uid);
    }
}

// -- Struct extraction ------------------------------------------------------

fn extract_struct(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::Class, // Closest mapping for structs
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    emit_node(graph_node, ctx);
}

// -- Enum extraction --------------------------------------------------------

fn extract_enum(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::Enum,
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    emit_node(graph_node, ctx);
}

// -- Trait extraction -------------------------------------------------------

fn extract_trait(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let trait_node = make_symbol_node(
        &name,
        NodeSubtype::Interface, // Closest mapping for traits
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    if !emit_node(trait_node.clone(), ctx) {
        return;
    }

    // Extract trait method signatures
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "function_item" || member.kind() == "function_signature_item" {
            extract_trait_method(&member, source, &trait_node, ctx);
        }
    }
}

fn extract_trait_method(
    node: &Node,
    source: &str,
    parent_trait: &ExtractedNode,
    ctx: &mut ExtractionCtx,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);
    let qualified_name = format!("{}.{}", parent_trait.name, name);

    let params = node.child_by_field_name("parameters");
    let sig = match params {
        Some(p) => format!("fn {}{}", name, node_text(&p, source)),
        None => format!("fn {}()", name),
    };

    let method_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:{}",
            ctx.repo_uid,
            ctx.file_path,
            qualified_name,
            "METHOD"
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Method),
        name: name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: Some(parent_trait.node_uid.clone()),
        location: Some(location_from_node(node)),
        signature: Some(sig),
        visibility: Some(Visibility::Public), // Trait methods are public
        doc_comment: extract_doc_comment(node, source),
        metadata_json: None,
    };

    emit_node(method_node, ctx);
}

// -- Impl block extraction --------------------------------------------------

fn extract_impl(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(type_node) = node.child_by_field_name("type") else {
        return;
    };
    let type_name = node_text(&type_node, source);

    // Check for trait impl: `impl Trait for Type`
    let trait_node = node.child_by_field_name("trait");
    let trait_name = trait_node.map(|t| node_text(&t, source).to_string());

    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "function_item" {
            extract_impl_method(&member, source, &type_name, trait_name.as_deref(), ctx);
        }
    }
}

fn extract_impl_method(
    node: &Node,
    source: &str,
    type_name: &str,
    trait_name: Option<&str>,
    ctx: &mut ExtractionCtx,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);
    let qualified_name = format!("{}.{}", type_name, name);

    let exported = has_pub_visibility(node);
    let params = node.child_by_field_name("parameters");
    let return_type = node.child_by_field_name("return_type");

    let sig = match (params, return_type) {
        (Some(p), Some(r)) => {
            format!("fn {}{} -> {}", name, node_text(&p, source), node_text(&r, source))
        }
        (Some(p), None) => format!("fn {}{}", name, node_text(&p, source)),
        _ => format!("fn {}()", name),
    };

    let metadata = if let Some(trait_n) = trait_name {
        Some(
            serde_json::json!({
                "implTrait": trait_n,
                "implType": type_name
            })
            .to_string(),
        )
    } else {
        Some(
            serde_json::json!({
                "implType": type_name
            })
            .to_string(),
        )
    };

    let method_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:{}",
            ctx.repo_uid, ctx.file_path, qualified_name, "METHOD"
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Method),
        name: name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None, // No parent node for impl methods in v1
        location: Some(location_from_node(node)),
        signature: Some(sig),
        visibility: Some(if exported {
            Visibility::Export
        } else {
            Visibility::Private
        }),
        doc_comment: extract_doc_comment(node, source),
        metadata_json: metadata,
    };

    if !emit_node(method_node.clone(), ctx) {
        return;
    }

    // If trait impl, emit IMPLEMENTS edge
    if let Some(trait_n) = trait_name {
        ctx.edges.push(ExtractedEdge {
            edge_uid: uuid::Uuid::new_v4().to_string(),
            snapshot_uid: ctx.snapshot_uid.into(),
            repo_uid: ctx.repo_uid.into(),
            source_node_uid: method_node.node_uid.clone(),
            target_key: trait_n.to_string(),
            edge_type: EdgeType::Implements,
            resolution: Resolution::Static,
            extractor: EXTRACTOR_NAME.into(),
            location: Some(location_from_node(node)),
            metadata_json: Some(
                serde_json::json!({
                    "traitName": trait_n,
                    "typeName": type_name
                })
                .to_string(),
            ),
        });
    }

    // Extract calls from method body
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_node(&body, source, ctx, &method_node.node_uid);
    }
}

// -- Const/static/type alias extraction -------------------------------------

fn extract_const(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::Constant,
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    emit_node(graph_node, ctx);
}

fn extract_static(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::Variable,
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    emit_node(graph_node, ctx);
}

fn extract_type_alias(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, source);

    let exported = has_pub_visibility(node);
    let graph_node = make_symbol_node(
        &name,
        NodeSubtype::TypeAlias,
        if exported {
            Visibility::Export
        } else {
            Visibility::Private
        },
        None,
        node,
        source,
        ctx,
    );

    emit_node(graph_node, ctx);
}

// -- Mod item extraction ----------------------------------------------------

fn extract_mod_item(node: &Node, source: &str, ctx: &mut ExtractionCtx) {
    // Only handle inline modules with a body
    let Some(body) = node.child_by_field_name("body") else {
        return; // `mod foo;` -- external module, skip
    };

    // Recurse into inline module body
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        visit_top_level(&child, source, ctx);
    }
}

// -- Call extraction --------------------------------------------------------

fn extract_calls_from_node(node: &Node, source: &str, ctx: &mut ExtractionCtx, caller_uid: &str) {
    if node.kind() == "call_expression" {
        emit_call_edge(node, source, ctx, caller_uid);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Skip nested function items (they are their own scope)
        if child.kind() == "function_item" {
            continue;
        }
        // Walk closure bodies for calls attributed to the enclosing function
        extract_calls_from_node(&child, source, ctx, caller_uid);
    }
}

fn emit_call_edge(node: &Node, source: &str, ctx: &mut ExtractionCtx, caller_uid: &str) {
    let Some(fn_node) = node.child_by_field_name("function") else {
        return;
    };

    let Some(target_key) = get_call_target_name(&fn_node, source) else {
        return;
    };

    // Skip macro-like builtins
    if MACRO_BUILTINS.contains(&target_key.as_str()) {
        return;
    }

    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: caller_uid.into(),
        target_key,
        edge_type: EdgeType::Calls,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(location_from_node(node)),
        metadata_json: None,
    });
}

/// Extract call target name from a function node.
/// Returns dot-notation for consistency.
fn get_call_target_name(fn_node: &Node, source: &str) -> Option<String> {
    match fn_node.kind() {
        "identifier" => Some(node_text(fn_node, source).to_string()),
        "scoped_identifier" => {
            // `Foo::bar` or `std::collections::HashMap::new`
            // Convert :: to . for consistency
            Some(node_text(fn_node, source).replace("::", "."))
        }
        "field_expression" => {
            // `self.method` or `obj.field`
            let object = fn_node.child_by_field_name("value")?;
            let field = fn_node.child_by_field_name("field")?;
            Some(format!(
                "{}.{}",
                node_text(&object, source),
                node_text(&field, source)
            ))
        }
        _ => None,
    }
}

// -- Helpers ----------------------------------------------------------------

fn make_symbol_node(
    name: &str,
    subtype: NodeSubtype,
    visibility: Visibility,
    signature: Option<&str>,
    node: &Node,
    source: &str,
    ctx: &mut ExtractionCtx,
) -> ExtractedNode {
    let subtype_str = format!("{:?}", subtype).to_uppercase();
    ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:{}",
            ctx.repo_uid, ctx.file_path, name, subtype_str
        ),
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name: name.to_string(),
        qualified_name: Some(name.to_string()),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: signature.map(|s| s.to_string()),
        visibility: Some(visibility),
        doc_comment: extract_doc_comment(node, source),
        metadata_json: None,
    }
}

/// Emit a node, deduplicating by stable_key.
/// Returns true if emitted, false if duplicate.
fn emit_node(node: ExtractedNode, ctx: &mut ExtractionCtx) -> bool {
    if ctx.emitted_stable_keys.contains(&node.stable_key) {
        return false;
    }
    ctx.emitted_stable_keys.insert(node.stable_key.clone());
    ctx.nodes.push(node);
    true
}

/// Check whether a node has a `pub` visibility modifier.
fn has_pub_visibility(node: &Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            return true;
        }
    }
    false
}

/// Extract doc comment preceding a node.
fn extract_doc_comment(node: &Node, source: &str) -> Option<String> {
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "line_comment" => {
                let text = node_text(&sibling, source);
                if text.starts_with("///") {
                    comments.insert(0, text.to_string());
                    prev = sibling.prev_sibling();
                } else {
                    break;
                }
            }
            "block_comment" => {
                let text = node_text(&sibling, source);
                if text.starts_with("/**") {
                    comments.insert(0, text.to_string());
                }
                break;
            }
            "attribute_item" => {
                // Skip attributes between doc comments and declaration
                prev = sibling.prev_sibling();
            }
            _ => break,
        }
    }

    if comments.is_empty() {
        None
    } else {
        Some(comments.join("\n"))
    }
}

fn location_from_node(node: &Node) -> SourceLocation {
    SourceLocation {
        line_start: node.start_position().row as i64 + 1, // tree-sitter is 0-based
        col_start: node.start_position().column as i64,
        line_end: node.end_position().row as i64 + 1,
        col_end: node.end_position().column as i64,
    }
}

fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    let start = node.start_byte();
    let end = node.end_byte();
    &source[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_test(source: &str) -> ExtractionResult {
        let mut extractor = RustExtractor::new();
        extractor.initialize().unwrap();
        extractor
            .extract(source, "src/lib.rs", "test:src/lib.rs", "test", "snap-1")
            .unwrap()
    }

    // -- FILE node --

    #[test]
    fn extracts_file_node() {
        let result = extract_test("fn main() {}");
        assert!(result.nodes.iter().any(|n| n.kind == NodeKind::File));
        let file_node = result.nodes.iter().find(|n| n.kind == NodeKind::File).unwrap();
        assert_eq!(file_node.stable_key, "test:src/lib.rs:FILE");
        assert_eq!(file_node.name, "lib.rs");
    }

    // -- Function extraction --

    #[test]
    fn extracts_function() {
        let result = extract_test("fn hello() {}");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.name, "hello");
        assert_eq!(func.visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_pub_function() {
        let result = extract_test("pub fn hello() {}");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.visibility, Some(Visibility::Export));
    }

    // -- Struct extraction --

    #[test]
    fn extracts_struct() {
        let result = extract_test("pub struct Foo {}");
        let s = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        assert_eq!(s.name, "Foo");
        assert_eq!(s.visibility, Some(Visibility::Export));
    }

    // -- Enum extraction --

    #[test]
    fn extracts_enum() {
        let result = extract_test("enum Color { Red, Green, Blue }");
        let e = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Enum))
            .unwrap();
        assert_eq!(e.name, "Color");
    }

    // -- Trait extraction --

    #[test]
    fn extracts_trait() {
        let result = extract_test("trait Display { fn fmt(&self); }");
        let t = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Interface))
            .unwrap();
        assert_eq!(t.name, "Display");
    }

    // -- Impl extraction --

    #[test]
    fn extracts_impl_method() {
        let result = extract_test(
            r#"
struct Foo;
impl Foo {
    pub fn new() -> Self { Foo }
}
"#,
        );
        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        assert_eq!(method.name, "new");
        assert!(method.qualified_name.as_ref().unwrap().contains("Foo.new"));
    }

    // -- Use declaration extraction --

    #[test]
    fn extracts_use_imports_edge() {
        let result = extract_test("use std::collections::HashMap;");
        assert!(!result.edges.is_empty());
        let edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Imports).unwrap();
        assert_eq!(edge.target_key, "std::collections");
    }

    #[test]
    fn extracts_import_binding() {
        let result = extract_test("use std::collections::HashMap;");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        assert_eq!(binding.identifier, "HashMap");
        assert_eq!(binding.specifier, "std::collections");
    }

    // -- Call extraction --

    #[test]
    fn extracts_call_edge() {
        let result = extract_test(
            r#"
fn caller() {
    callee();
}
fn callee() {}
"#,
        );
        let call_edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Calls);
        assert!(call_edge.is_some());
        assert_eq!(call_edge.unwrap().target_key, "callee");
    }

    #[test]
    fn extracts_method_call() {
        let result = extract_test(
            r#"
fn foo() {
    self.bar();
}
"#,
        );
        let call_edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Calls);
        assert!(call_edge.is_some());
        assert_eq!(call_edge.unwrap().target_key, "self.bar");
    }

    #[test]
    fn extracts_scoped_call() {
        let result = extract_test(
            r#"
fn foo() {
    HashMap::new();
}
"#,
        );
        let call_edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Calls);
        assert!(call_edge.is_some());
        assert_eq!(call_edge.unwrap().target_key, "HashMap.new");
    }

    // -- Deduplication --

    #[test]
    fn deduplicates_cfg_variants() {
        let result = extract_test(
            r#"
#[cfg(feature = "a")]
fn foo() {}

#[cfg(not(feature = "a"))]
fn foo() {}
"#,
        );
        let fns: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::Function))
            .collect();
        assert_eq!(fns.len(), 1, "should deduplicate cfg variants");
    }

    // -- Const/static/type alias extraction --

    #[test]
    fn extracts_const() {
        let result = extract_test("pub const MAX_SIZE: usize = 1024;");
        let c = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constant))
            .unwrap();
        assert_eq!(c.name, "MAX_SIZE");
        assert_eq!(c.visibility, Some(Visibility::Export));
    }

    #[test]
    fn extracts_static() {
        let result = extract_test("static COUNTER: u32 = 0;");
        let s = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(s.name, "COUNTER");
        assert_eq!(s.visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_type_alias() {
        let result = extract_test("pub type Result<T> = std::result::Result<T, Error>;");
        let t = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::TypeAlias))
            .unwrap();
        assert_eq!(t.name, "Result");
        assert_eq!(t.visibility, Some(Visibility::Export));
    }

    // -- Doc comment extraction --

    #[test]
    fn extracts_doc_comment() {
        let result = extract_test(
            r#"
/// This is a doc comment.
/// Second line.
pub fn documented() {}
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("This is a doc comment"));
        assert!(doc.contains("Second line"));
    }

    // -- Visibility edge cases --

    #[test]
    fn trait_visibility_private() {
        let result = extract_test("trait Internal {}");
        let t = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Interface))
            .unwrap();
        assert_eq!(t.visibility, Some(Visibility::Private));
    }

    #[test]
    fn trait_visibility_public() {
        let result = extract_test("pub trait Public {}");
        let t = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Interface))
            .unwrap();
        assert_eq!(t.visibility, Some(Visibility::Export));
    }

    #[test]
    fn impl_method_visibility() {
        let result = extract_test(
            r#"
struct Foo;
impl Foo {
    fn private_method() {}
    pub fn public_method() {}
}
"#,
        );
        let methods: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::Method))
            .collect();
        assert_eq!(methods.len(), 2);

        let private = methods.iter().find(|m| m.name == "private_method").unwrap();
        assert_eq!(private.visibility, Some(Visibility::Private));

        let public = methods.iter().find(|m| m.name == "public_method").unwrap();
        assert_eq!(public.visibility, Some(Visibility::Export));
    }

    // -- Use declaration variants --

    #[test]
    fn extracts_use_with_alias() {
        let result = extract_test("use std::collections::HashMap as Map;");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        // P2 regression test: identifier is the local alias, imported_name is the original.
        assert_eq!(binding.identifier, "Map", "identifier should be the alias");
        assert_eq!(
            binding.imported_name,
            Some("HashMap".to_string()),
            "imported_name should be the original exported symbol"
        );
    }

    #[test]
    fn non_aliased_import_has_same_identifier_and_imported_name() {
        let result = extract_test("use std::collections::HashMap;");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        // Without alias, identifier and imported_name should match.
        assert_eq!(binding.identifier, "HashMap");
        assert_eq!(binding.imported_name, Some("HashMap".to_string()));
    }

    #[test]
    fn wildcard_use_no_binding() {
        // Wildcard imports (`use foo::*`) do NOT produce ImportBinding records.
        // Rationale: wildcard imports bring in an indeterminate set of symbols
        // at parse time, making individual bindings meaningless for static analysis.
        // This is intentional - the extractor does not pretend to know what `*` resolves to.
        // For dependency analysis, use the FILE-level dependency tracking from Cargo.toml.
        let result = extract_test("use std::collections::*;");
        assert!(
            result.import_bindings.is_empty(),
            "wildcard imports should not produce bindings"
        );
    }

    #[test]
    fn extracts_grouped_use() {
        let result = extract_test("use std::collections::{HashMap, HashSet};");
        assert_eq!(result.import_bindings.len(), 2);
        let names: Vec<_> = result.import_bindings.iter().map(|b| &b.identifier).collect();
        assert!(names.contains(&&"HashMap".to_string()));
        assert!(names.contains(&&"HashSet".to_string()));
    }

    #[test]
    fn extracts_crate_relative_import() {
        let result = extract_test("use crate::module::Type;");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        assert!(binding.is_relative);
        assert_eq!(binding.specifier, "crate::module");
    }

    // -- Trait impl extraction --

    #[test]
    fn extracts_trait_impl_with_implements_edge() {
        let result = extract_test(
            r#"
struct Foo;
impl Clone for Foo {
    fn clone(&self) -> Self { Foo }
}
"#,
        );
        // Should have IMPLEMENTS edge
        let impl_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Implements);
        assert!(impl_edge.is_some());
        let edge = impl_edge.unwrap();
        assert_eq!(edge.target_key, "Clone");
    }

    // -- Signature extraction --

    #[test]
    fn extracts_function_signature_with_return_type() {
        let result = extract_test("fn compute(x: i32, y: i32) -> i32 { x + y }");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let sig = func.signature.as_ref().unwrap();
        assert!(sig.contains("fn compute"));
        assert!(sig.contains("i32"));
        assert!(sig.contains("-> i32"));
    }
}
