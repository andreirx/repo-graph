//! Core extractor implementation.

use std::collections::{BTreeMap, HashSet};

use repo_graph_classification::types::{ImportBinding, RuntimeBuiltinsSet, SourceLocation};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::types::{
    EdgeType, ExtractionResult, ExtractedEdge, ExtractedMetrics, ExtractedNode, NodeKind,
    NodeSubtype, Resolution, Visibility,
};
use tree_sitter::{Node, Parser};

use crate::builtins::python_runtime_builtins;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "python-core:0.1.0";

/// The language identifier this extractor handles.
const LANGUAGES: &[&str] = &["python"];

/// Built-in functions that produce noise in the call graph.
/// These are typically logging, debugging, or assertion functions.
const BUILTIN_CALL_NOISE: &[&str] = &[
    "print",
    "len",
    "str",
    "int",
    "float",
    "bool",
    "list",
    "dict",
    "set",
    "tuple",
    "range",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sorted",
    "reversed",
    "type",
    "isinstance",
    "issubclass",
    "hasattr",
    "getattr",
    "setattr",
    "delattr",
    "repr",
    "id",
    "hash",
    "abs",
    "sum",
    "min",
    "max",
    "round",
    "open",
    "input",
    "iter",
    "next",
    "super",
    "vars",
    "dir",
    "globals",
    "locals",
];

/// Extraction context for a single file.
struct ExtractionCtx<'a> {
    file_path: &'a str,
    file_uid: &'a str,
    file_node_uid: &'a str,
    repo_uid: &'a str,
    snapshot_uid: &'a str,
    source: &'a str,
    nodes: Vec<ExtractedNode>,
    edges: Vec<ExtractedEdge>,
    import_bindings: Vec<ImportBinding>,
    metrics: BTreeMap<String, ExtractedMetrics>,
    /// Stable keys already emitted. Used for deduplication.
    emitted_stable_keys: HashSet<String>,
    /// Current class context for method qualified names.
    current_class: Option<String>,
}

/// Concrete `ExtractorPort` adapter for Python source files.
///
/// Uses native tree-sitter with compiled-in grammar from
/// `tree-sitter-python`.
pub struct PythonExtractor {
    languages: Vec<String>,
    builtins: RuntimeBuiltinsSet,
    parser: Option<Parser>,
    python_language: tree_sitter::Language,
}

impl PythonExtractor {
    /// Create a new extractor. Call `initialize()` before `extract()`.
    pub fn new() -> Self {
        Self {
            languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
            builtins: python_runtime_builtins(),
            parser: None,
            python_language: tree_sitter_python::LANGUAGE.into(),
        }
    }
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorPort for PythonExtractor {
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
            .set_language(&self.python_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set Python grammar: {}", e),
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
            .set_language(&self.python_language)
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
            source,
            nodes: vec![file_node],
            edges: Vec::new(),
            import_bindings: Vec::new(),
            metrics: BTreeMap::new(),
            emitted_stable_keys: HashSet::new(),
            current_class: None,
        };

        // -- Walk module body --
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            visit_top_level(&child, &mut ctx);
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

// -- Top-level visitor ------------------------------------------------------

fn visit_top_level(node: &Node, ctx: &mut ExtractionCtx) {
    match node.kind() {
        "import_statement" => extract_import_statement(node, ctx),
        "import_from_statement" => extract_import_from_statement(node, ctx),
        "function_definition" => extract_function(node, ctx),
        "class_definition" => extract_class(node, ctx),
        "decorated_definition" => extract_decorated_definition(node, ctx),
        _ => {}
    }
}

// -- Import statement extraction --------------------------------------------

/// Convert a Python import specifier to a resolver-compatible target key.
///
/// For **relative imports** (starting with `.`), returns a repo-scoped
/// stable key: `repo_uid:resolved_path:FILE`. This allows the resolver
/// to look up the file directly without needing source-file context.
///
/// For **non-relative imports** (stdlib, third-party, or absolute local),
/// returns a slash-style path. The resolver's Stage 4 will attempt to
/// construct `repo_uid:path:FILE` and look it up.
///
/// Examples (source file: `src/app.py`, repo_uid: `myrepo`):
/// - `.service` → `myrepo:src/service:FILE` (sibling module)
/// - `..utils` → `myrepo:utils:FILE` (parent's sibling)
/// - `os` → `os` (stdlib, won't resolve)
/// - `src.module` → `src/module` (absolute local, Stage 4 handles)
fn python_specifier_to_target_key(
    specifier: &str,
    source_file_path: &str,
    repo_uid: &str,
) -> String {
    if specifier.is_empty() {
        return specifier.to_string();
    }

    // Count leading dots for relative imports
    let dot_count = specifier.chars().take_while(|c| *c == '.').count();

    if dot_count == 0 {
        // Non-relative import: `import foo.bar` → `foo/bar`
        // Let resolver Stage 4 handle constructing stable key
        return specifier.replace('.', "/");
    }

    // Relative import: resolve against source file directory
    let source_dir = match source_file_path.rfind('/') {
        Some(pos) => &source_file_path[..pos],
        None => "", // Top-level file
    };

    let remainder = &specifier[dot_count..];
    let relative_path = if dot_count == 1 {
        // `.` = current directory
        if remainder.is_empty() {
            ".".to_string()
        } else {
            format!("./{}", remainder.replace('.', "/"))
        }
    } else {
        // `..` = 2 dots = 1 level up, `...` = 3 dots = 2 levels up
        let up_levels = dot_count - 1;
        let prefix: String = (0..up_levels).map(|_| "..").collect::<Vec<_>>().join("/");
        if remainder.is_empty() {
            prefix
        } else {
            format!("{}/{}", prefix, remainder.replace('.', "/"))
        }
    };

    // Resolve the relative path against source directory
    let resolved_path = resolve_relative_path(source_dir, &relative_path);

    // Return stable key format
    format!("{}:{}:FILE", repo_uid, resolved_path)
}

/// Resolve a relative path specifier against a source directory.
///
/// Mirror of `resolve_relative_path` from `resolver.rs`.
fn resolve_relative_path(source_dir: &str, specifier: &str) -> String {
    let mut parts: Vec<&str> = if source_dir.is_empty() {
        Vec::new()
    } else {
        source_dir.split('/').collect()
    };

    for seg in specifier.split('/') {
        if seg == "." {
            continue;
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }

    parts.join("/")
}

/// Extract `import x, y, z` statement.
///
/// Whole-module imports (`import os`, `import numpy as np`) set
/// `imported_name: None` because no specific symbol is imported —
/// the module itself becomes the binding.
fn extract_import_statement(node: &Node, ctx: &mut ExtractionCtx) {
    let location = location_from_node(node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "dotted_name" || child.kind() == "aliased_import" {
            let (specifier, identifier) = if child.kind() == "aliased_import" {
                // `import foo as bar`
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");
                match (name_node, alias_node) {
                    (Some(name), Some(alias)) => {
                        (node_text(&name, ctx.source), node_text(&alias, ctx.source))
                    }
                    (Some(name), None) => {
                        let text = node_text(&name, ctx.source);
                        (text, text)
                    }
                    _ => continue,
                }
            } else {
                // `import foo`
                let text = node_text(&child, ctx.source);
                (text, text)
            };

            let is_relative = specifier.starts_with('.');
            let target_key = python_specifier_to_target_key(specifier, ctx.file_path, ctx.repo_uid);

            ctx.import_bindings.push(ImportBinding {
                identifier: identifier.to_string(),
                specifier: specifier.to_string(),
                is_relative,
                location: Some(location),
                is_type_only: false,
                // P2 fix: whole-module imports have no specific imported symbol.
                // The module itself is the binding, not a symbol exported from it.
                imported_name: None,
            });

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
                        "specifier": specifier,
                        "identifier": identifier
                    })
                    .to_string(),
                ),
            });
        }
    }
}

/// Extract `from x import y, z` statement.
///
/// Tree structure for `from os import path`:
/// ```text
/// [import_from_statement] (module_name=os, name=path)
///   [from]
///   [dotted_name] "os"
///   [import]
///   [dotted_name] "path"
/// ```
///
/// For `from os import path, join`:
/// ```text
/// [import_from_statement] (module_name=os)
///   [from]
///   [dotted_name] "os"
///   [import]
///   [dotted_name] "path"  <- first name
///   [dotted_name] "join"  <- additional names
/// ```
fn extract_import_from_statement(node: &Node, ctx: &mut ExtractionCtx) {
    let location = location_from_node(node);

    // Get the module name using the field accessor
    let module_name_node = node.child_by_field_name("module_name");
    let module_name = module_name_node
        .map(|n| node_text(&n, ctx.source))
        .unwrap_or("");

    // Count leading dots for relative imports (e.g., `from . import x`)
    let mut leading_dots = String::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_prefix" => {
                // import_prefix contains the dots
                leading_dots.push_str(node_text(&child, ctx.source));
            }
            "from" | "import" => {
                // Skip keywords
            }
            "dotted_name" => {
                // We've reached the module name or imported names
                break;
            }
            _ => {}
        }
    }

    let full_specifier = if leading_dots.is_empty() {
        module_name.to_string()
    } else if module_name.is_empty() {
        // `from . import x` - just dots
        leading_dots.clone()
    } else {
        format!("{}{}", leading_dots, module_name)
    };

    let is_relative = !leading_dots.is_empty();

    // The `name` field points to the first imported name.
    // For multiple imports, we need to iterate through children after `import` keyword.
    let mut found_import_keyword = false;
    let mut cursor2 = node.walk();

    for child in node.children(&mut cursor2) {
        if child.kind() == "import" {
            found_import_keyword = true;
            continue;
        }

        if !found_import_keyword {
            continue;
        }

        // Process nodes after `import` keyword
        match child.kind() {
            "dotted_name" => {
                let imported_name = node_text(&child, ctx.source).to_string();
                let identifier = imported_name.clone();

                emit_from_import_binding(ctx, &full_specifier, &identifier, &imported_name, is_relative, location);
            }
            "aliased_import" => {
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");

                if let Some(name) = name_node {
                    let imported_name = node_text(&name, ctx.source).to_string();
                    let identifier = alias_node
                        .map(|a| node_text(&a, ctx.source).to_string())
                        .unwrap_or_else(|| imported_name.clone());

                    emit_from_import_binding(ctx, &full_specifier, &identifier, &imported_name, is_relative, location);
                }
            }
            "wildcard_import" => {
                // `from x import *` - record the module import without individual bindings
                let target_key = python_specifier_to_target_key(&full_specifier, ctx.file_path, ctx.repo_uid);
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
                            "specifier": full_specifier,
                            "wildcard": true
                        })
                        .to_string(),
                    ),
                });
            }
            _ => {}
        }
    }
}

/// Helper to emit an ImportBinding and IMPORTS edge for a from-import.
///
/// `from X import Y` imports a specific symbol Y from module X.
/// Unlike whole-module imports, `imported_name` is set to the symbol name.
fn emit_from_import_binding(
    ctx: &mut ExtractionCtx,
    specifier: &str,
    identifier: &str,
    imported_name: &str,
    is_relative: bool,
    location: SourceLocation,
) {
    // Convert specifier to target key (stable key for relative, path for non-relative)
    let target_key = python_specifier_to_target_key(specifier, ctx.file_path, ctx.repo_uid);

    ctx.import_bindings.push(ImportBinding {
        identifier: identifier.to_string(),
        specifier: specifier.to_string(),
        is_relative,
        location: Some(location),
        is_type_only: false,
        // `from X import Y` imports a specific symbol Y, so imported_name is set
        imported_name: Some(imported_name.to_string()),
    });

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
                "specifier": specifier,
                "identifier": identifier,
                "importedName": imported_name
            })
            .to_string(),
        ),
    });
}

// -- Decorated definition extraction ----------------------------------------

fn extract_decorated_definition(node: &Node, ctx: &mut ExtractionCtx) {
    // Find the actual definition (function or class) inside
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => extract_function(&child, ctx),
            "class_definition" => extract_class(&child, ctx),
            _ => {}
        }
    }
}

// -- Function extraction ----------------------------------------------------

fn extract_function(node: &Node, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, ctx.source);

    // Check if this is a method (inside a class)
    let (subtype, qualified_name) = if let Some(class_name) = &ctx.current_class {
        (NodeSubtype::Method, format!("{}.{}", class_name, name))
    } else {
        (NodeSubtype::Function, name.to_string())
    };

    let params = node.child_by_field_name("parameters");
    let return_type = node.child_by_field_name("return_type");

    let sig = match (params, return_type) {
        (Some(p), Some(r)) => {
            format!(
                "def {}{}  -> {}",
                name,
                node_text(&p, ctx.source),
                node_text(&r, ctx.source)
            )
        }
        (Some(p), None) => format!("def {}{}", name, node_text(&p, ctx.source)),
        _ => format!("def {}()", name),
    };

    // Determine visibility
    let visibility = if name.starts_with("__") && !name.ends_with("__") {
        Visibility::Private // Name mangling private
    } else if name.starts_with('_') {
        Visibility::Internal // Convention private
    } else {
        Visibility::Export // Public by default
    };

    let doc_comment = extract_docstring(node, ctx.source);

    let graph_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:{}",
            ctx.repo_uid,
            ctx.file_path,
            qualified_name,
            format!("{:?}", subtype).to_uppercase()
        ),
        kind: NodeKind::Symbol,
        subtype: Some(subtype),
        name: name.to_string(),
        qualified_name: Some(qualified_name.clone()),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: Some(sig),
        visibility: Some(visibility),
        doc_comment,
        metadata_json: None,
    };

    if !emit_node(graph_node.clone(), ctx) {
        return;
    }

    // Extract calls from function body
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_node(&body, ctx, &graph_node.node_uid);
    }
}

// -- Class extraction -------------------------------------------------------

fn extract_class(node: &Node, ctx: &mut ExtractionCtx) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(&name_node, ctx.source);

    let doc_comment = extract_docstring(node, ctx.source);

    // Check for base classes
    let superclass = node.child_by_field_name("superclasses").map(|sc| {
        let text = node_text(&sc, ctx.source);
        // Remove parentheses
        text.trim_start_matches('(')
            .trim_end_matches(')')
            .to_string()
    });

    let visibility = if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Export
    };

    let class_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:CLASS",
            ctx.repo_uid, ctx.file_path, name
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Class),
        name: name.to_string(),
        qualified_name: Some(name.to_string()),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(visibility),
        doc_comment,
        metadata_json: superclass.map(|sc| {
            serde_json::json!({
                "superclass": sc
            })
            .to_string()
        }),
    };

    if !emit_node(class_node.clone(), ctx) {
        return;
    }

    // Extract class body (methods)
    if let Some(body) = node.child_by_field_name("body") {
        let old_class = ctx.current_class.take();
        ctx.current_class = Some(name.to_string());

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "function_definition" => extract_function(&child, ctx),
                "decorated_definition" => extract_decorated_definition(&child, ctx),
                _ => {}
            }
        }

        ctx.current_class = old_class;
    }
}

// -- Call extraction --------------------------------------------------------

fn extract_calls_from_node(node: &Node, ctx: &mut ExtractionCtx, caller_uid: &str) {
    if node.kind() == "call" {
        emit_call_edge(node, ctx, caller_uid);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Skip nested function definitions (they are their own scope)
        if child.kind() == "function_definition" || child.kind() == "class_definition" {
            continue;
        }
        extract_calls_from_node(&child, ctx, caller_uid);
    }
}

fn emit_call_edge(node: &Node, ctx: &mut ExtractionCtx, caller_uid: &str) {
    let Some(fn_node) = node.child_by_field_name("function") else {
        return;
    };

    let Some(target_key) = get_call_target_name(&fn_node, ctx.source) else {
        return;
    };

    // Skip built-in noise
    if BUILTIN_CALL_NOISE.contains(&target_key.as_str()) {
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
fn get_call_target_name(fn_node: &Node, source: &str) -> Option<String> {
    match fn_node.kind() {
        "identifier" => Some(node_text(fn_node, source).to_string()),
        "attribute" => {
            // `obj.method` or `module.func`
            let object = fn_node.child_by_field_name("object")?;
            let attribute = fn_node.child_by_field_name("attribute")?;
            Some(format!(
                "{}.{}",
                node_text(&object, source),
                node_text(&attribute, source)
            ))
        }
        _ => None,
    }
}

// -- Helpers ----------------------------------------------------------------

/// Extract docstring from a function or class definition.
fn extract_docstring(node: &Node, source: &str) -> Option<String> {
    let body = node.child_by_field_name("body")?;

    // First statement in body might be a string (docstring)
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "expression_statement" => {
                // Check if it contains a string
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "string" {
                        let text = node_text(&inner, source);
                        // Strip quotes
                        let stripped = text
                            .trim_start_matches("\"\"\"")
                            .trim_start_matches("'''")
                            .trim_start_matches('"')
                            .trim_start_matches('\'')
                            .trim_end_matches("\"\"\"")
                            .trim_end_matches("'''")
                            .trim_end_matches('"')
                            .trim_end_matches('\'')
                            .trim();
                        if !stripped.is_empty() {
                            return Some(stripped.to_string());
                        }
                    }
                }
            }
            "comment" => {
                // Skip comments before docstring
                continue;
            }
            _ => break, // Non-string statement, no docstring
        }
    }
    None
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
        let mut extractor = PythonExtractor::new();
        extractor.initialize().unwrap();
        extractor
            .extract(source, "src/app.py", "test:src/app.py", "test", "snap-1")
            .unwrap()
    }

    // -- FILE node --

    #[test]
    fn extracts_file_node() {
        let result = extract_test("def main(): pass");
        assert!(result.nodes.iter().any(|n| n.kind == NodeKind::File));
        let file_node = result.nodes.iter().find(|n| n.kind == NodeKind::File).unwrap();
        assert_eq!(file_node.stable_key, "test:src/app.py:FILE");
        assert_eq!(file_node.name, "app.py");
    }

    // -- Function extraction --

    #[test]
    fn extracts_function() {
        let result = extract_test("def hello():\n    pass");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.name, "hello");
        assert_eq!(func.visibility, Some(Visibility::Export));
    }

    #[test]
    fn extracts_private_function() {
        let result = extract_test("def _internal():\n    pass");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.name, "_internal");
        assert_eq!(func.visibility, Some(Visibility::Internal));
    }

    #[test]
    fn extracts_dunder_private_function() {
        let result = extract_test("def __secret():\n    pass");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.name, "__secret");
        assert_eq!(func.visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_function_with_return_type() {
        let result = extract_test("def compute(x: int) -> int:\n    return x");
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let sig = func.signature.as_ref().unwrap();
        assert!(sig.contains("int"));
        assert!(sig.contains("->"));
    }

    // -- Class extraction --

    #[test]
    fn extracts_class() {
        let result = extract_test("class Foo:\n    pass");
        let cls = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        assert_eq!(cls.name, "Foo");
        assert_eq!(cls.visibility, Some(Visibility::Export));
    }

    #[test]
    fn extracts_class_method() {
        let result = extract_test(
            r#"class Foo:
    def bar(self):
        pass
"#,
        );
        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        assert_eq!(method.name, "bar");
        assert!(method.qualified_name.as_ref().unwrap().contains("Foo.bar"));
    }

    // -- Import extraction --

    #[test]
    fn extracts_import_statement() {
        let result = extract_test("import os");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        assert_eq!(binding.identifier, "os");
        assert_eq!(binding.specifier, "os");
        // P2 contract: whole-module imports have no specific imported symbol
        assert_eq!(binding.imported_name, None);
    }

    #[test]
    fn extracts_import_with_alias() {
        let result = extract_test("import numpy as np");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        assert_eq!(binding.identifier, "np");
        assert_eq!(binding.specifier, "numpy");
        // P2 contract: whole-module imports have no specific imported symbol
        assert_eq!(binding.imported_name, None);
    }

    #[test]
    fn extracts_from_import() {
        let result = extract_test("from os import path");
        assert!(!result.import_bindings.is_empty());
        let binding = &result.import_bindings[0];
        assert_eq!(binding.identifier, "path");
        assert_eq!(binding.specifier, "os");
        // `from X import Y` imports a specific symbol, so imported_name is set
        assert_eq!(binding.imported_name, Some("path".to_string()));
    }

    #[test]
    fn extracts_imports_edge() {
        let result = extract_test("import json");
        let edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Imports)
            .unwrap();
        assert_eq!(edge.target_key, "json");
    }

    #[test]
    fn extracts_relative_import_as_stable_key() {
        // Relative imports become repo-scoped stable keys
        // `.service` from `src/app.py` in repo `test` → `test:src/service:FILE`
        let result = extract_test("from .service import UserService");
        let edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Imports)
            .unwrap();
        assert_eq!(edge.target_key, "test:src/service:FILE");
    }

    #[test]
    fn extracts_parent_relative_import_as_stable_key() {
        // `..utils` from `src/app.py` → go up one level → `test:utils:FILE`
        let result = extract_test("from ..utils import helper");
        let edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Imports)
            .unwrap();
        assert_eq!(edge.target_key, "test:utils:FILE");
    }

    #[test]
    fn extracts_dotted_absolute_import_as_path() {
        // Non-relative dotted imports become slash paths (resolver Stage 4 handles)
        let result = extract_test("from src.module.submod import Thing");
        let edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Imports)
            .unwrap();
        assert_eq!(edge.target_key, "src/module/submod");
    }

    // -- specifier_to_target_key unit tests --

    #[test]
    fn target_key_non_relative_simple() {
        // Non-relative imports return slash paths (resolver Stage 4 handles them)
        assert_eq!(python_specifier_to_target_key("os", "src/app.py", "r1"), "os");
        assert_eq!(python_specifier_to_target_key("json", "src/app.py", "r1"), "json");
    }

    #[test]
    fn target_key_non_relative_dotted() {
        // Dotted absolute imports become slash paths
        assert_eq!(python_specifier_to_target_key("foo.bar", "src/app.py", "r1"), "foo/bar");
        assert_eq!(python_specifier_to_target_key("src.module.sub", "app.py", "r1"), "src/module/sub");
    }

    #[test]
    fn target_key_relative_one_dot() {
        // Relative imports become repo-scoped stable keys
        // `.service` from `src/app.py` → `r1:src/service:FILE`
        assert_eq!(
            python_specifier_to_target_key(".service", "src/app.py", "r1"),
            "r1:src/service:FILE"
        );
        // `.utils.helper` from `src/api/views.py` → `r1:src/api/utils/helper:FILE`
        assert_eq!(
            python_specifier_to_target_key(".utils.helper", "src/api/views.py", "r1"),
            "r1:src/api/utils/helper:FILE"
        );
    }

    #[test]
    fn target_key_relative_two_dots() {
        // `..service` from `src/api/views.py` → `r1:src/service:FILE`
        assert_eq!(
            python_specifier_to_target_key("..service", "src/api/views.py", "r1"),
            "r1:src/service:FILE"
        );
        // `..utils.helper` from `src/api/views.py` → `r1:src/utils/helper:FILE`
        assert_eq!(
            python_specifier_to_target_key("..utils.helper", "src/api/views.py", "r1"),
            "r1:src/utils/helper:FILE"
        );
    }

    #[test]
    fn target_key_relative_three_dots() {
        // `...deep` from `src/api/v1/views.py` → `r1:src/deep:FILE`
        assert_eq!(
            python_specifier_to_target_key("...deep", "src/api/v1/views.py", "r1"),
            "r1:src/deep:FILE"
        );
        // `...deep.mod` from `src/api/v1/views.py` → `r1:src/deep/mod:FILE`
        assert_eq!(
            python_specifier_to_target_key("...deep.mod", "src/api/v1/views.py", "r1"),
            "r1:src/deep/mod:FILE"
        );
    }

    #[test]
    fn target_key_just_dot() {
        // `from . import x` - imports from current package directory
        // From `src/api/views.py`, `.` refers to `src/api`
        assert_eq!(
            python_specifier_to_target_key(".", "src/api/views.py", "r1"),
            "r1:src/api:FILE"
        );
    }

    #[test]
    fn target_key_just_two_dots() {
        // `from .. import x` - imports from parent package
        // From `src/api/views.py`, `..` refers to `src`
        assert_eq!(
            python_specifier_to_target_key("..", "src/api/views.py", "r1"),
            "r1:src:FILE"
        );
    }

    #[test]
    fn target_key_top_level_file() {
        // From top-level file `app.py`, `.service` → `r1:service:FILE`
        assert_eq!(
            python_specifier_to_target_key(".service", "app.py", "r1"),
            "r1:service:FILE"
        );
    }

    // -- Call extraction --

    #[test]
    fn extracts_call_edge() {
        let result = extract_test(
            r#"def caller():
    callee()

def callee():
    pass
"#,
        );
        let call_edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Calls);
        assert!(call_edge.is_some());
        assert_eq!(call_edge.unwrap().target_key, "callee");
    }

    #[test]
    fn extracts_method_call() {
        let result = extract_test(
            r#"def foo():
    self.bar()
"#,
        );
        let call_edge = result.edges.iter().find(|e| e.edge_type == EdgeType::Calls);
        assert!(call_edge.is_some());
        assert_eq!(call_edge.unwrap().target_key, "self.bar");
    }

    #[test]
    fn extracts_module_function_call() {
        let result = extract_test(
            r#"import os

def foo():
    os.path.join("a", "b")
"#,
        );
        let call_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Calls && e.target_key.contains("join"));
        assert!(call_edge.is_some());
    }

    // -- Docstring extraction --

    #[test]
    fn extracts_docstring() {
        let result = extract_test(
            r#"def documented():
    """This is a docstring."""
    pass
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("This is a docstring"));
    }

    #[test]
    fn extracts_multiline_docstring() {
        let result = extract_test(
            r#"def documented():
    """
    This is a multiline docstring.
    Second line.
    """
    pass
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("multiline"));
    }

    // -- Decorated definitions --

    #[test]
    fn extracts_decorated_function() {
        let result = extract_test(
            r#"@decorator
def decorated():
    pass
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        assert_eq!(func.name, "decorated");
    }

    #[test]
    fn extracts_decorated_class() {
        let result = extract_test(
            r#"@dataclass
class Data:
    pass
"#,
        );
        let cls = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        assert_eq!(cls.name, "Data");
    }

    // -- Superclass extraction --

    #[test]
    fn extracts_class_with_superclass() {
        let result = extract_test("class Child(Parent):\n    pass");
        let cls = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Class))
            .unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(cls.metadata_json.as_ref().unwrap()).unwrap();
        assert_eq!(metadata["superclass"], "Parent");
    }
}
