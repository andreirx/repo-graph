//! Core extractor implementation.

use std::collections::{BTreeMap, HashSet};

use repo_graph_classification::types::{
    ImportBinding, ImportKind, RuntimeBuiltinsSet, SourceLocation,
};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::types::{
    CallArgPayload, EdgeType, ExtractedEdge, ExtractedMetrics, ExtractedNode, ExtractionResult,
    NodeKind, NodeSubtype, Resolution, ResolvedCallsite, Visibility,
};
use tree_sitter::{Node, Parser};

use crate::builtins::python_runtime_builtins;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "python-core:0.2.0";

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

/// Built-in functions relevant for state-boundary analysis.
///
/// These are builtins that touch external resources (filesystem, etc.)
/// and should generate `ResolvedCallsite` facts even though they may
/// be in `BUILTIN_CALL_NOISE` (which suppresses generic CALLS edges).
///
/// SB-7C: For these builtins, we synthesize `resolved_module = "builtins"`
/// to satisfy the Form-A matcher contract. This is documented as a
/// deliberate builtin normalization rule, not literal import evidence.
const STATE_BOUNDARY_BUILTINS: &[&str] = &["open"];

/// Module imports that indicate state-boundary-relevant APIs.
///
/// When a call resolves to one of these modules, we attempt to generate
/// a `ResolvedCallsite` for state-boundary analysis.
const STATE_BOUNDARY_MODULES: &[&str] = &["sqlite3", "psycopg2", "mysql.connector", "pathlib"];

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
    /// PYTHON-RECEIVER-BINDING-1 (D-PRB-CARRIER-1): the lexical facts of the method whose body is
    /// being walked — set and restored by `extract_function` for a function directly in a class
    /// body (the only functions extracted with `current_class` set), `None` otherwise. Read by the
    /// self/cls call carrier to state whether its receiver is the method's first parameter.
    current_method: Option<MethodReceiverFacts>,
    /// Resolved callsite facts for state-boundary integration (SB-7C).
    resolved_callsites: Vec<ResolvedCallsite>,
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
            current_method: None,
            resolved_callsites: Vec::new(),
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
            resolved_callsites: ctx.resolved_callsites,
            import_observations: Vec::new(),
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
        "expression_statement" => extract_module_level_assignment(node, ctx),
        "annotated_assignment" => extract_annotated_assignment(node, ctx, None),
        _ => {}
    }
}

// -- Variable extraction ----------------------------------------------------

/// Extract module-level variable assignment.
///
/// Handles:
/// - `x = 1` (simple assignment)
/// - `x: int = 1` (annotated assignment)
///
/// Does NOT handle:
/// - `self.x = 1` (instance attribute)
/// - Multiple targets like `x = y = 1`
fn extract_module_level_assignment(node: &Node, ctx: &mut ExtractionCtx) {
    // expression_statement contains the actual assignment
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "assignment" {
            extract_assignment_variable(&child, ctx, None);
        }
    }
}

/// Extract a variable from an annotated assignment (`x: int = 1`).
///
/// `scope_prefix` is used for local variables (e.g., "func_name.") or None for module-level.
fn extract_annotated_assignment(node: &Node, ctx: &mut ExtractionCtx, scope_prefix: Option<&str>) {
    // annotated_assignment has `name` field (the identifier)
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };

    // Only handle simple identifiers
    if name_node.kind() != "identifier" {
        return;
    }

    let name = node_text(&name_node, ctx.source);

    // Skip instance/class attributes
    if name.starts_with("self.") || name.starts_with("cls.") {
        return;
    }

    let qualified_name = match scope_prefix {
        Some(prefix) => format!("{}.{}", prefix, name),
        None => name.to_string(),
    };

    let variable_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:VARIABLE",
            ctx.repo_uid, ctx.file_path, qualified_name
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Variable),
        name: name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Export
        }),
        doc_comment: None,
        metadata_json: None,
    };

    emit_node(variable_node, ctx);
}

/// Extract a variable from an assignment node.
///
/// `scope_prefix` is used for local variables (e.g., "func_name.") or None for module-level.
fn extract_assignment_variable(node: &Node, ctx: &mut ExtractionCtx, scope_prefix: Option<&str>) {
    // Get the left-hand side
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };

    // Only handle simple identifiers, not attribute access (self.x), subscripts, etc.
    if left.kind() != "identifier" {
        return;
    }

    let name = node_text(&left, ctx.source);

    // Skip if it looks like an instance attribute being assigned outside method context
    // (shouldn't happen at module level, but be safe)
    if name.starts_with("self.") || name.starts_with("cls.") {
        return;
    }

    let qualified_name = match scope_prefix {
        Some(prefix) => format!("{}.{}", prefix, name),
        None => name.to_string(),
    };

    let variable_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:VARIABLE",
            ctx.repo_uid, ctx.file_path, qualified_name
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Variable),
        name: name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Export
        }),
        doc_comment: None,
        metadata_json: None,
    };

    emit_node(variable_node, ctx);
}

/// Extract local variable assignments from a function body.
///
/// Walks the body looking for assignment statements, excluding:
/// - Instance attribute assignments (`self.x = ...`)
/// - Loop variables
/// - Comprehension variables
fn extract_local_variables(body: &Node, ctx: &mut ExtractionCtx, scope_prefix: &str) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "expression_statement" => {
                // Look for assignment inside
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "assignment" {
                        extract_local_assignment(&inner, ctx, scope_prefix);
                    }
                }
            }
            // Handle annotated assignments: `x: int = 1`
            "annotated_assignment" => {
                extract_annotated_assignment(&child, ctx, Some(scope_prefix));
            }
            // Recurse into blocks but skip nested functions/classes
            "if_statement" | "for_statement" | "while_statement" | "with_statement"
            | "try_statement" | "match_statement" => {
                extract_local_variables_recursive(&child, ctx, scope_prefix);
            }
            _ => {}
        }
    }
}

/// Recursively extract local variables from nested blocks.
fn extract_local_variables_recursive(node: &Node, ctx: &mut ExtractionCtx, scope_prefix: &str) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Skip nested function/class definitions
            "function_definition" | "class_definition" => continue,
            "expression_statement" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "assignment" {
                        extract_local_assignment(&inner, ctx, scope_prefix);
                    }
                }
            }
            // Handle annotated assignments: `x: int = 1`
            "annotated_assignment" => {
                extract_annotated_assignment(&child, ctx, Some(scope_prefix));
            }
            "block" => {
                extract_local_variables_recursive(&child, ctx, scope_prefix);
            }
            _ => {
                // Recurse into other node types
                extract_local_variables_recursive(&child, ctx, scope_prefix);
            }
        }
    }
}

/// Extract a local variable from an assignment, excluding instance attributes.
fn extract_local_assignment(node: &Node, ctx: &mut ExtractionCtx, scope_prefix: &str) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };

    // Skip attribute access (self.x, obj.attr)
    if left.kind() != "identifier" {
        return;
    }

    let name = node_text(&left, ctx.source);

    let qualified_name = format!("{}.{}", scope_prefix, name);

    let variable_node = ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: format!(
            "{}:{}#{}:SYMBOL:VARIABLE",
            ctx.repo_uid, ctx.file_path, qualified_name
        ),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Variable),
        name: name.to_string(),
        qualified_name: Some(qualified_name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Private), // Local variables are always private
        doc_comment: None,
        metadata_json: None,
    };

    emit_node(variable_node, ctx);
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
                // Python `import foo` is similar to TypeScript namespace import:
                // the module object itself is bound to the identifier.
                kind: ImportKind::Namespace,
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

                emit_from_import_binding(
                    ctx,
                    &full_specifier,
                    &identifier,
                    &imported_name,
                    is_relative,
                    location,
                );
            }
            "aliased_import" => {
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");

                if let Some(name) = name_node {
                    let imported_name = node_text(&name, ctx.source).to_string();
                    let identifier = alias_node
                        .map(|a| node_text(&a, ctx.source).to_string())
                        .unwrap_or_else(|| imported_name.clone());

                    emit_from_import_binding(
                        ctx,
                        &full_specifier,
                        &identifier,
                        &imported_name,
                        is_relative,
                        location,
                    );
                }
            }
            "wildcard_import" => {
                // `from x import *` - record the module import without individual bindings
                let target_key =
                    python_specifier_to_target_key(&full_specifier, ctx.file_path, ctx.repo_uid);
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
        // Python `from X import Y` is a named import
        kind: ImportKind::Named,
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
        // __init__ is a constructor, other methods are regular methods
        let subtype = if name == "__init__" {
            NodeSubtype::Constructor
        } else {
            NodeSubtype::Method
        };
        (subtype, format!("{}.{}", class_name, name))
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

    // Compute and store metrics
    if let Some(body) = node.child_by_field_name("body") {
        let cyclomatic = compute_cyclomatic_complexity(&body);
        let nesting = compute_max_nesting_depth(&body, 0);
        let param_count = count_parameters(node, ctx.source);
        let location = location_from_node(node);
        let function_length = (location.line_end - location.line_start + 1) as u32;

        ctx.metrics.insert(
            graph_node.stable_key.clone(),
            ExtractedMetrics {
                cyclomatic_complexity: cyclomatic,
                parameter_count: param_count,
                max_nesting_depth: nesting,
                function_length: Some(function_length),
                cognitive_complexity: None, // Deferred
            },
        );

        // Extract calls from function body. PYTHON-RECEIVER-BINDING-1: a method (a function
        // directly in a class body) records its receiver facts for the self/cls call carrier;
        // a module-level function records none (its calls carry no carrier).
        let method_facts = ctx
            .current_class
            .is_some()
            .then(|| method_receiver_facts(node, &body, ctx.source));
        let old_method = std::mem::replace(&mut ctx.current_method, method_facts);
        extract_calls_from_node(&body, ctx, &graph_node.node_uid);
        ctx.current_method = old_method;

        // Extract local variables from function body
        extract_local_variables(&body, ctx, &qualified_name);
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
        stable_key: format!("{}:{}#{}:SYMBOL:CLASS", ctx.repo_uid, ctx.file_path, name),
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

        // SB-7C: Also attempt to generate ResolvedCallsite for state-boundary analysis.
        // This runs even for builtins that are filtered from CALLS edges.
        if let Some(fn_node) = node.child_by_field_name("function") {
            // Only generate ResolvedCallsite for calls within functions/methods,
            // not top-level module calls. This matches the TS extractor contract:
            // enclosing_symbol_node_uid must be a SYMBOL node UID, not a FILE node UID.
            if caller_uid != ctx.file_node_uid {
                if let Some(resolved) = try_resolve_callsite_python(
                    node,
                    &fn_node,
                    ctx.source,
                    caller_uid,
                    &ctx.import_bindings,
                ) {
                    ctx.resolved_callsites.push(resolved);
                }
            }
        }
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

    // PYTHON-SELF-BINDING-1 (RG-REQ-005-L03/L02): stamp the self/cls call carrier so the
    // resolver's class-hierarchy walk has the one fact it needs — this is a `self.`/`cls.`
    // call inside class C. Pure binding EVIDENCE; the target key is NOT rewritten and every
    // other call (a chained attribute, a module-level self-call) carries nothing, byte-identical
    // to before.
    let metadata_json = self_call_carrier(
        &target_key,
        ctx.current_class.as_deref(),
        ctx.current_method.as_ref(),
        node,
        ctx.source,
    );

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
        metadata_json,
    });
}

/// The self/cls call carrier for the resolver's class-hierarchy binding stage.
///
/// Returns `{"selfCall":true,"enclosingClass":"C","receiverBinding":<b>}` ONLY when `target_key`
/// is exactly `self.<m>` / `cls.<m>` — a single dot, `<m>` a plain identifier — AND the call is
/// inside a class (`current_class == Some("C")`). A chained attribute (`self.extra.get` → three
/// parts), a receiverless/module call, or a self-call outside any class carries `None`. The
/// carrier is EVIDENCE for the resolver (RG-REQ-005-L02); the target key is never rewritten.
///
/// PYTHON-RECEIVER-BINDING-1 (D-PRB-CARRIER-1 = C): the class context alone does not prove what
/// the receiver identifier denotes, so the carrier states the receiver's LEXICAL binding:
/// `"parameter"` only when the receiver is the enclosing method's first positional parameter and
/// nothing rebinds it where the call is made ([`receiver_is_the_method_parameter`]); otherwise
/// `"unproven"` — the resolver then binds it through neither its class hierarchy nor a same-named
/// import alias.
fn self_call_carrier(
    target_key: &str,
    current_class: Option<&str>,
    method: Option<&MethodReceiverFacts>,
    call: &Node,
    source: &str,
) -> Option<String> {
    let class = current_class?;
    let (receiver, called) = target_key.split_once('.')?;
    if receiver != "self" && receiver != "cls" {
        return None;
    }
    // Exactly two parts: a chained attribute (`self.extra.get`) leaves a dot in `called`.
    if called.contains('.') || called.is_empty() {
        return None;
    }
    let binding = match method {
        Some(facts) if receiver_is_the_method_parameter(receiver, facts, call, source) => {
            RECEIVER_BINDING_PARAMETER
        }
        _ => RECEIVER_BINDING_UNPROVEN,
    };
    Some(
        serde_json::json!({
            "selfCall": true,
            "enclosingClass": class,
            "receiverBinding": binding,
        })
        .to_string(),
    )
}

// -- Receiver binding (PYTHON-RECEIVER-BINDING-1, D-PRB-CARRIER-1) ----------

/// `receiverBinding` value: the receiver is proven to be the enclosing method's first parameter.
const RECEIVER_BINDING_PARAMETER: &str = "parameter";
/// `receiverBinding` value: the extractor could not prove it.
const RECEIVER_BINDING_UNPROVEN: &str = "unproven";

/// The lexical facts of a method (a function directly in a class body) that decide whether a
/// `self`/`cls` receiver in its body denotes its first parameter.
#[derive(Debug, Clone)]
struct MethodReceiverFacts {
    /// The name of the first POSITIONAL parameter (`identifier`, `typed_parameter`,
    /// `default_parameter`, `typed_default_parameter`); `None` when the list is empty or starts
    /// with `*args`, `**kwargs`, a bare `*` or `/`.
    first_positional_param: Option<String>,
    /// The method is decorated `@staticmethod` (the identifier, or an attribute ending in it).
    is_staticmethod: bool,
    /// Every name the method's OWN scope binds other than by its parameters (its body outside
    /// nested function and class bodies), plus every name a nested function or class declares
    /// `nonlocal`.
    rebound_names: HashSet<String>,
}

/// Read the receiver facts of `function` (a `function_definition` directly in a class body,
/// possibly under a `decorated_definition`) whose body is `body`.
fn method_receiver_facts(function: &Node, body: &Node, source: &str) -> MethodReceiverFacts {
    let mut rebound_names = HashSet::new();
    collect_own_scope_bindings(body, source, &mut rebound_names);
    MethodReceiverFacts {
        first_positional_param: first_positional_parameter(function, source),
        is_staticmethod: is_decorated_staticmethod(function, source),
        rebound_names,
    }
}

/// The first positional parameter's name, or `None` (see [`MethodReceiverFacts`]).
fn first_positional_parameter(function: &Node, source: &str) -> Option<String> {
    let params = function.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    let first = params
        .named_children(&mut cursor)
        .find(|c| c.kind() != "comment")?;
    let name_node = match first.kind() {
        "identifier" => first,
        "typed_parameter" => first.named_child(0).filter(|n| n.kind() == "identifier")?,
        "default_parameter" | "typed_default_parameter" => first.child_by_field_name("name")?,
        _ => return None,
    };
    Some(node_text(&name_node, source).to_string())
}

/// Is `function` decorated `@staticmethod` (or `@<x>.staticmethod`)?
fn is_decorated_staticmethod(function: &Node, source: &str) -> bool {
    let Some(parent) = function.parent() else {
        return false;
    };
    if parent.kind() != "decorated_definition" {
        return false;
    }
    let mut cursor = parent.walk();
    let found = parent
        .children(&mut cursor)
        .filter(|c| c.kind() == "decorator")
        .any(|decorator| {
            let Some(expr) = decorator.named_child(0) else {
                return false;
            };
            match expr.kind() {
                "identifier" => node_text(&expr, source) == "staticmethod",
                "attribute" => expr
                    .child_by_field_name("attribute")
                    .is_some_and(|a| node_text(&a, source) == "staticmethod"),
                _ => false,
            }
        });
    found
}

/// Collect the names bound in a method's own scope (condition (c) of the rule): assignment,
/// augmented and annotated assignment targets; `for`, `with … as`, `except … as` targets; walrus
/// names; `del` targets; `global`/`nonlocal` names; `import` names; nested `def`/`class` names;
/// `match` captures. Nested function and class bodies are other scopes, except that a `nonlocal`
/// declared anywhere inside them rebinds the method's name. A lambda is its own scope. A
/// comprehension's `for` targets bind in the comprehension's scope, not the method's (a walrus
/// inside it still binds the method's).
fn collect_own_scope_bindings(node: &Node, source: &str, out: &mut HashSet<String>) {
    match node.kind() {
        "function_definition" | "class_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.insert(node_text(&name, source).to_string());
            }
            collect_nested_nonlocals(node, source, out);
            return;
        }
        "lambda" => return,
        "assignment" | "augmented_assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_target_names(&left, source, out);
            }
        }
        "for_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_target_names(&left, source, out);
            }
        }
        "as_pattern" => {
            // `with x as T` and `except E as T` (the alias is an `as_pattern_target`); a
            // `case … as name` capture carries the name directly.
            if let Some(alias) = node.child_by_field_name("alias") {
                collect_target_names(&alias, source, out);
            } else if let Some(last) = node.named_child(node.named_child_count().saturating_sub(1))
            {
                if last.kind() == "identifier" {
                    out.insert(node_text(&last, source).to_string());
                }
            }
        }
        "named_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.insert(node_text(&name, source).to_string());
            }
        }
        "delete_statement" => {
            let mut cursor = node.walk();
            for target in node.named_children(&mut cursor) {
                collect_target_names(&target, source, out);
            }
            return;
        }
        "global_statement" | "nonlocal_statement" => {
            let mut cursor = node.walk();
            for name in node.named_children(&mut cursor) {
                if name.kind() == "identifier" {
                    out.insert(node_text(&name, source).to_string());
                }
            }
            return;
        }
        "import_statement" | "import_from_statement" => {
            let mut cursor = node.walk();
            for (i, child) in node.children(&mut cursor).enumerate() {
                if node.field_name_for_child(i as u32) != Some("name") {
                    continue;
                }
                let bound = match child.kind() {
                    // `import a.b` binds `a`; `from m import a` binds `a`.
                    "dotted_name" => child.named_child(0),
                    "aliased_import" => child.child_by_field_name("alias"),
                    _ => None,
                };
                if let Some(b) = bound {
                    out.insert(node_text(&b, source).to_string());
                }
            }
            return;
        }
        "case_pattern" => {
            collect_match_captures(node, source, out);
        }
        "for_in_clause" => {
            // The comprehension's own targets are not the method's; its iterables may hold a
            // walrus that is.
            if let Some(right) = node.child_by_field_name("right") {
                collect_own_scope_bindings(&right, source, out);
            }
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_own_scope_bindings(&child, source, out);
    }
}

/// The names an assignment-like target binds: identifiers, through tuples, lists, parentheses
/// and starred targets — never the object of an attribute (`self.x = …`) or a subscript
/// (`self[0] = …`), which bind nothing.
fn collect_target_names(target: &Node, source: &str, out: &mut HashSet<String>) {
    match target.kind() {
        "identifier" => {
            out.insert(node_text(target, source).to_string());
        }
        "attribute" | "subscript" => {}
        _ => {
            let mut cursor = target.walk();
            for child in target.named_children(&mut cursor) {
                collect_target_names(&child, source, out);
            }
        }
    }
}

/// The capture names inside a `case` pattern: a single-identifier `dotted_name` in a pattern or
/// keyword-value position (`case [x]`, `case {"k": x}`, `case P(k=x)`), a star capture
/// (`case [*x]`) and an `as` capture. A dotted value pattern (`case self.X`) binds nothing.
fn collect_match_captures(node: &Node, source: &str, out: &mut HashSet<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name"
                if matches!(node.kind(), "case_pattern" | "keyword_pattern")
                    && child.named_child_count() == 1 =>
            {
                if let Some(id) = child.named_child(0) {
                    out.insert(node_text(&id, source).to_string());
                }
            }
            "identifier" if matches!(node.kind(), "splat_pattern" | "as_pattern") => {
                out.insert(node_text(&child, source).to_string());
            }
            "dotted_name" => {}
            _ => collect_match_captures(&child, source, out),
        }
    }
}

/// Every name a `nonlocal` statement inside a nested function or class declares (at any depth):
/// such a name rebinds the method's own variable.
fn collect_nested_nonlocals(node: &Node, source: &str, out: &mut HashSet<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "nonlocal_statement" {
            let mut names = child.walk();
            for name in child.named_children(&mut names) {
                if name.kind() == "identifier" {
                    out.insert(node_text(&name, source).to_string());
                }
            }
        } else {
            collect_nested_nonlocals(&child, source, out);
        }
    }
}

/// Condition (a)–(d) of D-PRB-CARRIER-1's rule as read lexically: the method is not a
/// `@staticmethod`; its first positional parameter is named `receiver`; its own scope does not
/// rebind that name; and no enclosing `lambda` parameter or comprehension `for` target binds it
/// where `call` is made.
fn receiver_is_the_method_parameter(
    receiver: &str,
    facts: &MethodReceiverFacts,
    call: &Node,
    source: &str,
) -> bool {
    !facts.is_staticmethod
        && facts.first_positional_param.as_deref() == Some(receiver)
        && !facts.rebound_names.contains(receiver)
        && !bound_by_an_enclosing_lambda_or_comprehension(receiver, call, source)
}

/// Does a `lambda` parameter, or the `for` target of a comprehension, bind `name` at `call`?
/// Walks up from the call to its enclosing function definition.
fn bound_by_an_enclosing_lambda_or_comprehension(name: &str, call: &Node, source: &str) -> bool {
    let mut child = *call;
    while let Some(parent) = child.parent() {
        match parent.kind() {
            "function_definition" | "class_definition" => return false,
            "lambda" => {
                let in_body = parent
                    .child_by_field_name("body")
                    .is_some_and(|b| b.id() == child.id());
                if in_body {
                    if let Some(params) = parent.child_by_field_name("parameters") {
                        let mut bound = HashSet::new();
                        collect_parameter_names(&params, source, &mut bound);
                        if bound.contains(name) {
                            return true;
                        }
                    }
                }
            }
            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => {
                // The first clause's iterable is evaluated in the enclosing scope.
                let mut cursor = parent.walk();
                let first_clause = parent
                    .named_children(&mut cursor)
                    .find(|c| c.kind() == "for_in_clause");
                let in_first_iterable = first_clause.is_some_and(|c| c.id() == child.id());
                if !in_first_iterable {
                    let mut bound = HashSet::new();
                    let mut cursor = parent.walk();
                    for clause in parent.named_children(&mut cursor) {
                        if clause.kind() == "for_in_clause" {
                            if let Some(left) = clause.child_by_field_name("left") {
                                collect_target_names(&left, source, &mut bound);
                            }
                        }
                    }
                    if bound.contains(name) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        child = parent;
    }
    false
}

/// The names a `lambda_parameters` list binds.
fn collect_parameter_names(params: &Node, source: &str, out: &mut HashSet<String>) {
    let mut cursor = params.walk();
    for p in params.named_children(&mut cursor) {
        let name = match p.kind() {
            "identifier" => Some(p),
            "default_parameter" | "typed_default_parameter" => p.child_by_field_name("name"),
            "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                p.named_child(0).filter(|n| n.kind() == "identifier")
            }
            _ => None,
        };
        if let Some(n) = name {
            out.insert(node_text(&n, source).to_string());
        }
    }
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

// -- State-boundary callsite resolution (SB-7C) -----------------------------

/// Attempt to generate a `ResolvedCallsite` for state-boundary analysis.
///
/// This handles:
/// 1. Builtin `open()` — synthesizes `resolved_module = "builtins"`
/// 2. Module-qualified calls like `sqlite3.connect()` — resolves via imports
/// 3. Named-imported calls like `connect()` from `from sqlite3 import connect`
///
/// Returns `None` if the call doesn't resolve to a state-boundary-relevant API
/// or if argument classification fails.
fn try_resolve_callsite_python(
    call_node: &Node,
    fn_node: &Node,
    source: &str,
    enclosing_symbol_node_uid: &str,
    import_bindings: &[ImportBinding],
) -> Option<ResolvedCallsite> {
    let (resolved_module, resolved_symbol) =
        resolve_callee_via_python(fn_node, source, import_bindings)?;

    // Only proceed if this is a state-boundary-relevant API.
    let is_sb_builtin = resolved_module == "builtins"
        && STATE_BOUNDARY_BUILTINS.contains(&resolved_symbol.as_str());
    let is_sb_module = STATE_BOUNDARY_MODULES
        .iter()
        .any(|m| resolved_module == *m || resolved_module.starts_with(&format!("{}.", m)));

    if !is_sb_builtin && !is_sb_module {
        return None;
    }

    let arg0_payload = classify_arg0_payload_python(call_node, source)?;
    let arg1_payload = classify_arg1_payload_python(call_node, source);

    Some(ResolvedCallsite {
        enclosing_symbol_node_uid: enclosing_symbol_node_uid.to_string(),
        resolved_module,
        resolved_symbol,
        arg0_payload,
        arg1_payload,
        source_location: location_from_node(call_node),
    })
}

/// Resolve a Python callee to a `(module, symbol)` pair.
///
/// Handles:
/// - Bare identifier (`open(...)`, `connect(...)`)
///   - If it's a state-boundary builtin → `("builtins", "open")`
///   - Otherwise, look up in import_bindings for `from X import Y` pattern
/// - Attribute expression (`sqlite3.connect(...)`, `os.open(...)`)
///   - Object must match an import binding's identifier
///   - Returns `(binding.specifier, attribute)`
fn resolve_callee_via_python(
    fn_node: &Node,
    source: &str,
    import_bindings: &[ImportBinding],
) -> Option<(String, String)> {
    match fn_node.kind() {
        "identifier" => {
            let ident = node_text(fn_node, source);

            // Check if it's a state-boundary builtin (synthesize module).
            if STATE_BOUNDARY_BUILTINS.contains(&ident) {
                return Some(("builtins".to_string(), ident.to_string()));
            }

            // Look up in import_bindings for `from X import Y` pattern.
            // These have `imported_name = Some(Y)`.
            let binding = import_bindings
                .iter()
                .find(|b| b.identifier == ident && b.imported_name.is_some())?;

            let symbol = binding
                .imported_name
                .clone()
                .expect("checked is_some() above");
            Some((binding.specifier.clone(), symbol))
        }
        "attribute" => {
            // Pattern: `module.func()` where `module` is an import binding.
            let object = fn_node.child_by_field_name("object")?;
            let attribute = fn_node.child_by_field_name("attribute")?;

            // Object must be a plain identifier (not a chain like `a.b.c()`).
            if object.kind() != "identifier" {
                return None;
            }

            let obj_text = node_text(&object, source);
            let attr_text = node_text(&attribute, source);

            // Look up import binding where identifier matches object.
            // Whole-module imports (`import sqlite3`) have `imported_name = None`.
            let binding = import_bindings
                .iter()
                .find(|b| b.identifier == obj_text && b.imported_name.is_none())?;

            Some((binding.specifier.clone(), attr_text.to_string()))
        }
        _ => None,
    }
}

/// Classify argument 0 of a Python call expression.
///
/// Supported patterns:
/// - String literal: `open("/etc/config")` → `StringLiteral { value: "/etc/config" }`
/// - os.environ access: `open(os.environ["PATH"])` → `EnvKeyRead { key_name: "PATH" }`
///
/// Returns `None` for unsupported patterns (variables, f-strings, etc.).
fn classify_arg0_payload_python(call_node: &Node, source: &str) -> Option<CallArgPayload> {
    classify_arg_at_index_python(call_node, source, 0)
}

/// Classify argument 1 of a Python call expression (e.g., mode for `open()`).
///
/// Returns `None` if arg1 is absent or not a supported pattern.
fn classify_arg1_payload_python(call_node: &Node, source: &str) -> Option<CallArgPayload> {
    classify_arg_at_index_python(call_node, source, 1)
}

/// Classify an argument at a specific index.
fn classify_arg_at_index_python(
    call_node: &Node,
    source: &str,
    index: usize,
) -> Option<CallArgPayload> {
    let args_node = call_node.child_by_field_name("arguments")?;

    // Iterate through named children to find the argument at the given index.
    let mut cursor = args_node.walk();
    let mut current_index = 0usize;
    let mut target_arg: Option<Node> = None;

    for child in args_node.children(&mut cursor) {
        if child.is_named() && child.kind() != "comment" {
            if current_index == index {
                target_arg = Some(child);
                break;
            }
            current_index += 1;
        }
    }

    let arg = target_arg?;
    classify_python_expression(&arg, source)
}

/// Classify a Python expression node as a `CallArgPayload`.
fn classify_python_expression(node: &Node, source: &str) -> Option<CallArgPayload> {
    match node.kind() {
        "string" => {
            // Python string literal. May have prefix (f, r, b, etc.).
            // Extract the actual string content.
            let literal_text = node_text(node, source);

            // Skip f-strings (they contain interpolation).
            if literal_text.starts_with('f') || literal_text.starts_with("rf") {
                return None;
            }

            // Strip quotes and optional prefix.
            let stripped = strip_python_string_quotes(literal_text);
            if stripped.is_empty() {
                return None;
            }

            Some(CallArgPayload::StringLiteral {
                value: stripped.to_string(),
            })
        }
        "subscript" => {
            // Pattern: `os.environ["KEY"]`
            let value = node.child_by_field_name("value")?;
            let subscript = node.child_by_field_name("subscript")?;

            // Check for `os.environ` pattern.
            if value.kind() == "attribute" {
                let obj = value.child_by_field_name("object")?;
                let attr = value.child_by_field_name("attribute")?;
                if node_text(&obj, source) == "os" && node_text(&attr, source) == "environ" {
                    // Extract the key from the subscript.
                    if subscript.kind() == "string" {
                        let key_literal = node_text(&subscript, source);
                        let key_name = strip_python_string_quotes(key_literal);
                        if !key_name.is_empty() {
                            return Some(CallArgPayload::EnvKeyRead {
                                key_name: key_name.to_string(),
                            });
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Strip quotes from a Python string literal.
///
/// Handles:
/// - Single quotes: `'text'`
/// - Double quotes: `"text"`
/// - Triple quotes: `'''text'''` or `"""text"""`
/// - Prefixed strings: `r"text"`, `b"text"`, etc.
fn strip_python_string_quotes(s: &str) -> &str {
    let s = s.trim();

    // Strip optional prefix (r, b, u, f, rf, br, etc.).
    let s = s.trim_start_matches(|c: char| c.is_ascii_alphabetic());

    // Strip triple quotes first.
    if s.starts_with("\"\"\"") && s.ends_with("\"\"\"") && s.len() >= 6 {
        return &s[3..s.len() - 3];
    }
    if s.starts_with("'''") && s.ends_with("'''") && s.len() >= 6 {
        return &s[3..s.len() - 3];
    }

    // Strip single quotes.
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
        && s.len() >= 2
    {
        return &s[1..s.len() - 1];
    }

    s
}

// -- Metrics computation ----------------------------------------------------

/// Compute cyclomatic complexity for a function body.
///
/// Counts decision points: if, elif, for, while, and, or, except, with, assert, match/case.
/// Base complexity is 1.
fn compute_cyclomatic_complexity(node: &Node) -> u32 {
    let mut complexity = 1u32; // Base complexity

    fn count_decisions(node: &Node, count: &mut u32) {
        match node.kind() {
            // Control flow
            "if_statement" | "elif_clause" | "for_statement" | "while_statement" => {
                *count += 1;
            }
            // Exception handling
            "except_clause" => {
                *count += 1;
            }
            // Context managers
            "with_statement" => {
                *count += 1;
            }
            // Assertions
            "assert_statement" => {
                *count += 1;
            }
            // Pattern matching (Python 3.10+)
            "case_clause" => {
                *count += 1;
            }
            // Boolean operators (short-circuit evaluation = decision point)
            "boolean_operator" => {
                *count += 1;
            }
            // Conditional expressions (ternary)
            "conditional_expression" => {
                *count += 1;
            }
            _ => {}
        }

        // Recurse into children, but skip nested function/class definitions
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "function_definition" && child.kind() != "class_definition" {
                count_decisions(&child, count);
            }
        }
    }

    count_decisions(node, &mut complexity);
    complexity
}

/// Compute maximum nesting depth in a function body.
///
/// Counts nesting levels for: if, for, while, with, try, match.
fn compute_max_nesting_depth(node: &Node, current_depth: u32) -> u32 {
    let mut max_depth = current_depth;

    let new_depth = match node.kind() {
        "if_statement" | "for_statement" | "while_statement" | "with_statement"
        | "try_statement" | "match_statement" => current_depth + 1,
        _ => current_depth,
    };

    if new_depth > max_depth {
        max_depth = new_depth;
    }

    // Recurse into children, skip nested function/class definitions
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "function_definition" && child.kind() != "class_definition" {
            let child_max = compute_max_nesting_depth(&child, new_depth);
            if child_max > max_depth {
                max_depth = child_max;
            }
        }
    }

    max_depth
}

/// Count parameters in a function definition.
fn count_parameters(node: &Node, source: &str) -> u32 {
    let Some(params) = node.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0u32;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "identifier"
            | "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern" => {
                // Get the parameter name (first identifier child or the node itself)
                let text = child
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, source))
                    .unwrap_or_else(|| node_text(&child, source));
                // Skip 'self' and 'cls' in methods
                if text != "self" && text != "cls" {
                    count += 1;
                }
            }
            _ => {}
        }
    }

    count
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
        let file_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::File)
            .unwrap();
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

    #[test]
    fn extracts_init_as_constructor() {
        let result = extract_test(
            r#"class MyClass:
    def __init__(self, value):
        self.value = value

    def get_value(self):
        return self.value
"#,
        );
        // __init__ should be Constructor
        let constructor = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Constructor))
            .unwrap();
        assert_eq!(constructor.name, "__init__");
        assert!(constructor
            .qualified_name
            .as_ref()
            .unwrap()
            .contains("MyClass.__init__"));

        // Other methods should still be Method
        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        assert_eq!(method.name, "get_value");
    }

    // -- Metrics extraction --

    #[test]
    fn extracts_cyclomatic_complexity_simple() {
        let result = extract_test(
            r#"def simple():
    return 42
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // Base complexity = 1, no decision points
        assert_eq!(metrics.cyclomatic_complexity, 1);
    }

    #[test]
    fn extracts_cyclomatic_complexity_with_if() {
        let result = extract_test(
            r#"def with_if(x):
    if x > 0:
        return 1
    else:
        return 0
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // Base = 1, if = 1 → total = 2
        assert_eq!(metrics.cyclomatic_complexity, 2);
    }

    #[test]
    fn extracts_cyclomatic_complexity_complex() {
        let result = extract_test(
            r#"def complex_func(x, y):
    if x > 0:
        if y > 0:
            return 1
        elif y < 0:
            return 2
    for i in range(10):
        if i % 2 == 0 and x > i:
            continue
    while x > 0:
        x -= 1
    return 0
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // Base=1, outer if=1, inner if=1, elif=1, for=1, if in for=1, and=1, while=1 → total=8
        assert_eq!(metrics.cyclomatic_complexity, 8);
    }

    #[test]
    fn extracts_nesting_depth() {
        let result = extract_test(
            r#"def nested():
    if True:
        for i in range(10):
            while i > 0:
                pass
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // if → for → while = depth 3
        assert_eq!(metrics.max_nesting_depth, 3);
    }

    #[test]
    fn extracts_parameter_count() {
        let result = extract_test(
            r#"def with_params(a, b, c=10, *args, **kwargs):
    pass
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // a, b, c, args, kwargs = 5 params
        assert_eq!(metrics.parameter_count, 5);
    }

    #[test]
    fn extracts_method_parameter_count_excludes_self() {
        let result = extract_test(
            r#"class Foo:
    def method(self, x, y):
        pass
"#,
        );
        let method = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Method))
            .unwrap();
        let metrics = result.metrics.get(&method.stable_key).unwrap();
        // x, y = 2 params (self excluded)
        assert_eq!(metrics.parameter_count, 2);
    }

    #[test]
    fn extracts_function_length() {
        let result = extract_test(
            r#"def multiline():
    x = 1
    y = 2
    z = 3
    return x + y + z
"#,
        );
        let func = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Function))
            .unwrap();
        let metrics = result.metrics.get(&func.stable_key).unwrap();
        // Lines 1-5 = 5 lines
        assert_eq!(metrics.function_length, Some(5));
    }

    // -- Variable extraction --

    #[test]
    fn extracts_module_level_variable() {
        let result = extract_test("CONFIG = 'production'");
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "CONFIG");
        assert_eq!(var.visibility, Some(Visibility::Export));
    }

    #[test]
    fn extracts_private_module_variable() {
        let result = extract_test("_internal_state = []");
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "_internal_state");
        assert_eq!(var.visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_local_variable_in_function() {
        let result = extract_test(
            r#"def process():
    count = 0
    return count
"#,
        );
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "count");
        // Qualified name includes function scope
        assert!(var
            .qualified_name
            .as_ref()
            .unwrap()
            .contains("process.count"));
        // Local variables are always private
        assert_eq!(var.visibility, Some(Visibility::Private));
    }

    #[test]
    fn does_not_extract_self_assignment_as_variable() {
        let result = extract_test(
            r#"class Foo:
    def __init__(self, x):
        self.x = x
"#,
        );
        // self.x is an instance attribute, not a variable
        let vars: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.subtype == Some(NodeSubtype::Variable))
            .collect();
        // Should have no VARIABLE nodes (self.x is attribute access, not identifier)
        assert!(vars.is_empty());
    }

    #[test]
    fn extracts_nested_local_variable() {
        let result = extract_test(
            r#"def outer():
    if True:
        inner_var = 42
"#,
        );
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "inner_var");
        assert!(var
            .qualified_name
            .as_ref()
            .unwrap()
            .contains("outer.inner_var"));
    }

    #[test]
    fn extracts_annotated_assignment_module_level() {
        let result = extract_test("count: int = 0");
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "count");
        assert_eq!(var.qualified_name.as_ref().unwrap(), "count");
        assert_eq!(var.visibility, Some(Visibility::Export));
    }

    #[test]
    fn extracts_annotated_assignment_in_function() {
        let result = extract_test(
            r#"def process():
    total: int = 0
    return total
"#,
        );
        let var = result
            .nodes
            .iter()
            .find(|n| n.subtype == Some(NodeSubtype::Variable))
            .unwrap();
        assert_eq!(var.name, "total");
        assert!(var
            .qualified_name
            .as_ref()
            .unwrap()
            .contains("process.total"));
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
        assert_eq!(
            python_specifier_to_target_key("os", "src/app.py", "r1"),
            "os"
        );
        assert_eq!(
            python_specifier_to_target_key("json", "src/app.py", "r1"),
            "json"
        );
    }

    #[test]
    fn target_key_non_relative_dotted() {
        // Dotted absolute imports become slash paths
        assert_eq!(
            python_specifier_to_target_key("foo.bar", "src/app.py", "r1"),
            "foo/bar"
        );
        assert_eq!(
            python_specifier_to_target_key("src.module.sub", "app.py", "r1"),
            "src/module/sub"
        );
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

    // -- Self/cls call carrier (PYTHON-SELF-BINDING-1) --

    fn find_call_edge<'a>(result: &'a ExtractionResult, target_key: &str) -> &'a ExtractedEdge {
        result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Calls && e.target_key == target_key)
            .unwrap_or_else(|| panic!("no CALLS edge for target_key {target_key}"))
    }

    #[test]
    fn self_call_in_a_class_carries_the_self_call_carrier() {
        let result = extract_test(
            r#"class C:
    def f(self):
        self.g()

    def g(self):
        pass
"#,
        );
        let edge = find_call_edge(&result, "self.g");
        let meta: serde_json::Value =
            serde_json::from_str(edge.metadata_json.as_ref().expect("carrier present")).unwrap();
        assert_eq!(meta["selfCall"], serde_json::json!(true));
        assert_eq!(meta["enclosingClass"], "C");
    }

    #[test]
    fn cls_call_in_a_class_carries_the_self_call_carrier() {
        let result = extract_test(
            r#"class C:
    @classmethod
    def make(cls):
        cls.build()

    @classmethod
    def build(cls):
        pass
"#,
        );
        let edge = find_call_edge(&result, "cls.build");
        let meta: serde_json::Value =
            serde_json::from_str(edge.metadata_json.as_ref().expect("carrier present")).unwrap();
        assert_eq!(meta["selfCall"], serde_json::json!(true));
        assert_eq!(meta["enclosingClass"], "C");
    }

    #[test]
    fn self_attribute_chain_call_carries_no_carrier() {
        // `self.extra.get()` is a 3-part key — an attribute chain, not a self-call the resolver
        // can bind by the class hierarchy. It carries nothing (byte-identical to before).
        let result = extract_test(
            r#"class C:
    def f(self):
        self.extra.get()
"#,
        );
        let edge = find_call_edge(&result, "self.extra.get");
        assert!(edge.metadata_json.is_none());
    }

    #[test]
    fn self_call_outside_a_class_carries_no_carrier() {
        // A `self.bar()` call at module level (no enclosing class) carries nothing.
        let result = extract_test(
            r#"def foo():
    self.bar()
"#,
        );
        let edge = find_call_edge(&result, "self.bar");
        assert!(edge.metadata_json.is_none());
    }

    // -- Receiver binding (PYTHON-RECEIVER-BINDING-1, D-PRB-CARRIER-1) --
    //
    // ONE RULE (F-PRB-LEXICAL-ORACLES): every test naming a `receiverBinding` value asserts BOTH
    // the carrier's value AND the real resolver's disposition of that same extraction.

    use repo_graph_indexer::resolver::{
        metadata_forward_decl, resolve_edges, superclasses_from_metadata, ResolutionResult,
        ResolverIndex, ResolverNode,
    };
    use std::collections::HashMap;

    /// Build a `ResolverIndex` from the extraction's nodes (name, qualified name, kind, subtype,
    /// file uid, forward-decl flag, `superclasses_from_metadata`) and run the real
    /// `repo_graph_indexer::resolver::resolve_edges` over its edges with the file's import
    /// bindings.
    fn resolve_through_the_indexer_resolver(result: &ExtractionResult) -> ResolutionResult {
        let wire = |v: serde_json::Value| v.as_str().map(|s| s.to_string());
        let mut index = ResolverIndex {
            nodes_by_stable_key: HashMap::new(),
            nodes_by_name: HashMap::new(),
            nodes_by_qualified_name: HashMap::new(),
            nodes_by_uid: HashMap::new(),
            node_uid_to_file_uid: HashMap::new(),
            file_resolution: HashMap::new(),
            per_file_include_resolution: HashMap::new(),
            stable_key_to_uid: HashMap::new(),
            file_to_module: HashMap::new(),
            include_resolver: None,
            rust_crate_roots: HashMap::new(),
            java_suffix_index: HashMap::new(),
            npm_workspace_packages: HashMap::new(),
            tsconfig_paths: Default::default(),
        };
        for n in &result.nodes {
            let node = ResolverNode {
                node_uid: n.node_uid.clone(),
                stable_key: n.stable_key.clone(),
                name: n.name.clone(),
                qualified_name: n.qualified_name.clone(),
                kind: wire(serde_json::to_value(n.kind).unwrap()).unwrap(),
                subtype: n
                    .subtype
                    .and_then(|st| wire(serde_json::to_value(st).unwrap())),
                file_uid: n.file_uid.clone(),
                forward_decl: metadata_forward_decl(n.metadata_json.as_deref()),
                superclasses: superclasses_from_metadata(n.metadata_json.as_deref()),
            };
            index
                .nodes_by_name
                .entry(node.name.clone())
                .or_default()
                .push(node.clone());
            if let Some(qn) = &node.qualified_name {
                index
                    .nodes_by_qualified_name
                    .entry(qn.clone())
                    .or_default()
                    .push(node.clone());
            }
            if let Some(f) = &node.file_uid {
                index
                    .node_uid_to_file_uid
                    .insert(node.node_uid.clone(), f.clone());
            }
            index
                .stable_key_to_uid
                .insert(node.stable_key.clone(), node.node_uid.clone());
            index
                .nodes_by_stable_key
                .insert(node.stable_key.clone(), node.clone());
            index.nodes_by_uid.insert(node.node_uid.clone(), node);
        }
        let bindings: HashMap<String, Vec<ImportBinding>> = [(
            "test:src/app.py".to_string(),
            result.import_bindings.clone(),
        )]
        .into_iter()
        .collect();
        resolve_edges(&result.edges, &index, Some(&bindings))
    }

    /// Every CALLS extraction edge with `target_key`, in source order.
    fn calls_with_key<'a>(
        result: &'a ExtractionResult,
        target_key: &str,
    ) -> Vec<&'a ExtractedEdge> {
        let mut edges: Vec<&ExtractedEdge> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls && e.target_key == target_key)
            .collect();
        edges.sort_by_key(|e| e.location.map(|l| (l.line_start, l.col_start)));
        edges
    }

    fn receiver_binding_of(edge: &ExtractedEdge) -> String {
        let meta: serde_json::Value =
            serde_json::from_str(edge.metadata_json.as_deref().expect("carrier present")).unwrap();
        assert_eq!(meta["selfCall"], serde_json::json!(true));
        meta["receiverBinding"]
            .as_str()
            .expect("receiverBinding is a string")
            .to_string()
    }

    fn node_uid_by_qn(result: &ExtractionResult, qn: &str) -> String {
        result
            .nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(qn) && n.kind == NodeKind::Symbol)
            .unwrap_or_else(|| panic!("no node {qn}"))
            .node_uid
            .clone()
    }

    /// The carrier call `edge` records `parameter` and the resolver binds it `static` to
    /// `target_qn`, with no unresolved row for it.
    fn assert_parameter(
        result: &ExtractionResult,
        resolved: &ResolutionResult,
        edge: &ExtractedEdge,
        target_qn: &str,
    ) {
        assert_eq!(receiver_binding_of(edge), "parameter");
        let target = node_uid_by_qn(result, target_qn);
        let hits: Vec<_> = resolved
            .resolved
            .iter()
            .filter(|r| r.edge_uid == edge.edge_uid)
            .collect();
        assert_eq!(hits.len(), 1, "one CALLS edge for the call");
        assert_eq!(hits[0].target_node_uid, target, "bound to {target_qn}");
        assert_eq!(hits[0].source_node_uid, edge.source_node_uid);
        assert_eq!(hits[0].resolution, Resolution::Static);
        assert!(
            !resolved
                .still_unresolved
                .iter()
                .any(|u| u.edge.edge_uid == edge.edge_uid),
            "no unresolved row for the call"
        );
    }

    /// The carrier call `edge` records `unproven`; the resolver leaves it unresolved with
    /// `self_call_receiver_unproven` and a sorted pool containing `candidate_qn`.
    fn assert_unproven(resolved: &ResolutionResult, edge: &ExtractedEdge, candidate_qn: &str) {
        assert_eq!(receiver_binding_of(edge), "unproven");
        assert!(
            !resolved
                .resolved
                .iter()
                .any(|r| r.edge_uid == edge.edge_uid),
            "no CALLS edge for the call"
        );
        let rows: Vec<_> = resolved
            .still_unresolved
            .iter()
            .filter(|u| u.edge.edge_uid == edge.edge_uid)
            .collect();
        assert_eq!(rows.len(), 1, "one unresolved row for the call");
        let meta: serde_json::Value =
            serde_json::from_str(rows[0].edge.metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["nameOnlyReason"], "self_call_receiver_unproven");
        let pool: Vec<String> = meta["nameOnlyCandidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let mut sorted = pool.clone();
        sorted.sort();
        assert_eq!(pool, sorted, "the pool is sorted");
        assert!(
            pool.iter().any(|c| c == candidate_qn),
            "pool {pool:?} holds {candidate_qn}"
        );
    }

    /// One `parameter` fixture: extract, resolve, and assert every `self.m()` carrier call.
    fn assert_every_self_m_is_parameter(source: &str) {
        let result = extract_test(source);
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.m");
        assert!(!calls.is_empty());
        for edge in calls {
            assert_parameter(&result, &resolved, edge, "C.m");
        }
    }

    /// One `unproven` fixture for a binding form: a method `f(self, other)` whose body holds
    /// `form` and then `return self.m()`.
    fn assert_binding_form_is_unproven(form: &str) {
        let body: String = form.lines().map(|l| format!("        {l}\n")).collect();
        let source = format!(
            "class C:\n    def m(self):\n        pass\n\n    def f(self, other):\n{body}        return self.m()\n"
        );
        let result = extract_test(&source);
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.m");
        assert_eq!(calls.len(), 1, "{source}");
        assert_unproven(&resolved, calls[0], "C.m");
    }

    #[test]
    fn self_call_on_the_first_parameter_records_receiver_binding_parameter_and_binds_to_the_class_method(
    ) {
        assert_every_self_m_is_parameter(
            "class C:\n    def m(self):\n        pass\n\n    def f(self):\n        return self.m()\n",
        );
    }

    #[test]
    fn cls_call_on_the_first_parameter_of_a_classmethod_records_receiver_binding_parameter_and_binds_to_the_class_method(
    ) {
        let result = extract_test(
            "class C:\n    @classmethod\n    def m(cls):\n        pass\n\n    @classmethod\n    def f(cls):\n        return cls.m()\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "cls.m");
        assert_eq!(calls.len(), 1);
        assert_parameter(&result, &resolved, calls[0], "C.m");
    }

    #[test]
    fn typed_or_defaulted_first_parameter_records_receiver_binding_parameter_and_binds_to_the_class_method(
    ) {
        let result = extract_test(
            "class C:\n    def m(self):\n        pass\n\n    def f(self: \"C\", x=None):\n        return self.m()\n\n    def g(self=None):\n        return self.m()\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.m");
        assert_eq!(calls.len(), 2);
        for edge in calls {
            assert_parameter(&result, &resolved, edge, "C.m");
        }
    }

    #[test]
    fn attribute_assignment_on_the_receiver_is_not_a_rebinding_and_binds_to_the_class_method() {
        assert_every_self_m_is_parameter(
            "class C:\n    def m(self):\n        pass\n\n    def f(self):\n        self.x = 1\n        self[0] = 2\n        return self.m()\n",
        );
    }

    #[test]
    fn assignment_inside_a_nested_function_is_not_a_rebinding_and_binds_to_the_class_method() {
        assert_every_self_m_is_parameter(
            "class C:\n    def m(self):\n        pass\n\n    def f(self):\n        def g():\n            self = 1\n            return self\n        return self.m()\n",
        );
    }

    #[test]
    fn self_call_in_a_staticmethod_records_receiver_binding_unproven_and_stays_unresolved() {
        for decorator in ["staticmethod", "builtins.staticmethod"] {
            let source = format!(
                "class C:\n    def m(self):\n        pass\n\n    @{decorator}\n    def f(self):\n        return self.m()\n"
            );
            let result = extract_test(&source);
            let resolved = resolve_through_the_indexer_resolver(&result);
            let calls = calls_with_key(&result, "self.m");
            assert_eq!(calls.len(), 1, "{decorator}");
            assert_unproven(&resolved, calls[0], "C.m");
        }
    }

    #[test]
    fn self_call_whose_receiver_is_not_the_first_parameter_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        // The test_base.py:190 shape (`cls = SimpleView; cls.as_view()` in a method taking
        // `self`), and a method whose first parameter is `self_`.
        let result = extract_test(
            "class C:\n    def m(self):\n        pass\n\n    def f(self):\n        cls = X\n        return cls.m()\n\n    def g(self_):\n        return self.m()\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let cls_calls = calls_with_key(&result, "cls.m");
        assert_eq!(cls_calls.len(), 1);
        assert_unproven(&resolved, cls_calls[0], "C.m");
        let self_calls = calls_with_key(&result, "self.m");
        assert_eq!(self_calls.len(), 1);
        assert_unproven(&resolved, self_calls[0], "C.m");
    }

    #[test]
    fn self_call_in_a_parameterless_method_records_receiver_binding_unproven_and_stays_unresolved()
    {
        // The review's executed counterexample: `self` is the module alias (Python returns 2.0).
        let result = extract_test(
            "import math as self\n\nclass C:\n    def sqrt(self, x):\n        return x\n\n    def caller():\n        return self.sqrt(4)\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.sqrt");
        assert_eq!(calls.len(), 1);
        assert_unproven(&resolved, calls[0], "C.sqrt");
    }

    #[test]
    fn receiver_rebound_by_plain_assignment_records_receiver_binding_unproven_and_stays_unresolved()
    {
        assert_binding_form_is_unproven("self = other");
    }

    #[test]
    fn receiver_rebound_by_augmented_assignment_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        assert_binding_form_is_unproven("self += other");
    }

    #[test]
    fn receiver_rebound_by_annotated_assignment_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        assert_binding_form_is_unproven("self: C = other");
    }

    #[test]
    fn receiver_rebound_by_a_for_target_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("for self in other:\n    pass");
    }

    #[test]
    fn receiver_rebound_by_a_with_target_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("with other as self:\n    pass");
    }

    #[test]
    fn receiver_rebound_by_an_except_target_records_receiver_binding_unproven_and_stays_unresolved()
    {
        assert_binding_form_is_unproven("try:\n    pass\nexcept Exception as self:\n    pass");
    }

    #[test]
    fn receiver_rebound_by_a_walrus_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("if (self := other):\n    pass");
    }

    #[test]
    fn receiver_deleted_by_del_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("del self");
    }

    #[test]
    fn receiver_declared_global_in_the_method_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        // CPython rejects `global` of a parameter name at compile time; tree-sitter parses it.
        // A form the language refuses is never `parameter`.
        assert_binding_form_is_unproven("global self");
    }

    #[test]
    fn receiver_declared_nonlocal_in_the_method_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        assert_binding_form_is_unproven("nonlocal self");
    }

    #[test]
    fn receiver_declared_nonlocal_in_a_nested_function_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        assert_binding_form_is_unproven("def g():\n    nonlocal self\n    self = other\ng()");
    }

    #[test]
    fn receiver_rebound_by_an_import_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("import os as self");
    }

    #[test]
    fn receiver_rebound_by_a_nested_def_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("def self():\n    pass");
    }

    #[test]
    fn receiver_rebound_by_a_nested_class_records_receiver_binding_unproven_and_stays_unresolved() {
        assert_binding_form_is_unproven("class self:\n    pass");
    }

    #[test]
    fn receiver_rebound_by_a_match_capture_records_receiver_binding_unproven_and_stays_unresolved()
    {
        assert_binding_form_is_unproven("match other:\n    case [self]:\n        pass");
    }

    #[test]
    fn self_call_inside_a_comprehension_that_binds_the_receiver_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        let result = extract_test(
            "class C:\n    def m(self):\n        pass\n\n    def f(self, other):\n        return [self.m() for self in other]\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.m");
        assert_eq!(calls.len(), 1);
        assert_unproven(&resolved, calls[0], "C.m");
    }

    #[test]
    fn self_call_inside_a_lambda_that_binds_the_receiver_records_receiver_binding_unproven_and_stays_unresolved(
    ) {
        // The call inside the lambda is `unproven`; a second carrier call outside that lambda in
        // the same method is `parameter` and binds to `C.m`.
        let result = extract_test(
            "class C:\n    def m(self):\n        pass\n\n    def f(self, other):\n        g = lambda self: self.m()\n        return self.m()\n",
        );
        let resolved = resolve_through_the_indexer_resolver(&result);
        let calls = calls_with_key(&result, "self.m");
        assert_eq!(calls.len(), 2);
        assert_unproven(&resolved, calls[0], "C.m");
        assert_parameter(&result, &resolved, calls[1], "C.m");
    }

    #[test]
    fn self_call_inside_a_nested_function_emits_no_call_edge() {
        // A call inside a nested function or class emits no CALLS extraction edge (unchanged):
        // no carrier and no disposition exist for it.
        let result = extract_test(
            "class C:\n    def m(self):\n        pass\n\n    def f(self):\n        def view(request):\n            self.m()\n        class Inner:\n            def h(self):\n                self.m()\n        return view\n",
        );
        assert!(calls_with_key(&result, "self.m").is_empty());
        let resolved = resolve_through_the_indexer_resolver(&result);
        assert!(!resolved
            .still_unresolved
            .iter()
            .any(|u| u.edge.target_key == "self.m"));
        assert!(result
            .edges
            .iter()
            .all(|e| e.extractor == "python-core:0.2.0"));
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

    // -- ResolvedCallsite extraction (SB-7C) --

    #[test]
    fn resolved_callsite_builtin_open_synthesizes_builtins_module() {
        let result = extract_test(
            r#"def load_config():
    f = open("/etc/config.json", "r")
    return f.read()
"#,
        );
        assert_eq!(result.resolved_callsites.len(), 1);
        let rc = &result.resolved_callsites[0];
        assert_eq!(rc.resolved_module, "builtins");
        assert_eq!(rc.resolved_symbol, "open");
        match &rc.arg0_payload {
            CallArgPayload::StringLiteral { value } => {
                assert_eq!(value, "/etc/config.json");
            }
            other => panic!("expected StringLiteral, got {:?}", other),
        }
        // arg1 should capture the mode
        match &rc.arg1_payload {
            Some(CallArgPayload::StringLiteral { value }) => {
                assert_eq!(value, "r");
            }
            other => panic!("expected Some(StringLiteral), got {:?}", other),
        }
    }

    #[test]
    fn resolved_callsite_sqlite3_connect_via_import() {
        let result = extract_test(
            r#"import sqlite3

def get_db():
    return sqlite3.connect("app.db")
"#,
        );
        assert_eq!(result.resolved_callsites.len(), 1);
        let rc = &result.resolved_callsites[0];
        assert_eq!(rc.resolved_module, "sqlite3");
        assert_eq!(rc.resolved_symbol, "connect");
        match &rc.arg0_payload {
            CallArgPayload::StringLiteral { value } => {
                assert_eq!(value, "app.db");
            }
            other => panic!("expected StringLiteral, got {:?}", other),
        }
    }

    #[test]
    fn resolved_callsite_from_import_pattern() {
        let result = extract_test(
            r#"from sqlite3 import connect

def get_db():
    return connect(":memory:")
"#,
        );
        assert_eq!(result.resolved_callsites.len(), 1);
        let rc = &result.resolved_callsites[0];
        assert_eq!(rc.resolved_module, "sqlite3");
        assert_eq!(rc.resolved_symbol, "connect");
        match &rc.arg0_payload {
            CallArgPayload::StringLiteral { value } => {
                assert_eq!(value, ":memory:");
            }
            other => panic!("expected StringLiteral, got {:?}", other),
        }
    }

    #[test]
    fn no_resolved_callsite_for_top_level_call() {
        // Top-level calls should NOT produce ResolvedCallsite per contract.
        let result = extract_test(
            r#"import sqlite3
db = sqlite3.connect("app.db")
"#,
        );
        assert!(
            result.resolved_callsites.is_empty(),
            "top-level calls should not produce ResolvedCallsite"
        );
    }

    #[test]
    fn no_resolved_callsite_for_non_state_boundary_api() {
        // json.dumps is not in STATE_BOUNDARY_MODULES.
        let result = extract_test(
            r#"import json

def serialize(data):
    return json.dumps(data)
"#,
        );
        assert!(
            result.resolved_callsites.is_empty(),
            "non-state-boundary APIs should not produce ResolvedCallsite"
        );
    }

    #[test]
    fn resolved_callsite_with_env_key_read_arg0() {
        let result = extract_test(
            r#"import os

def load_from_env():
    f = open(os.environ["CONFIG_PATH"], "r")
    return f.read()
"#,
        );
        assert_eq!(result.resolved_callsites.len(), 1);
        let rc = &result.resolved_callsites[0];
        match &rc.arg0_payload {
            CallArgPayload::EnvKeyRead { key_name } => {
                assert_eq!(key_name, "CONFIG_PATH");
            }
            other => panic!("expected EnvKeyRead, got {:?}", other),
        }
    }

    #[test]
    fn no_resolved_callsite_for_variable_arg0() {
        // open() with a variable argument should not produce ResolvedCallsite.
        let result = extract_test(
            r#"def load_file(path):
    f = open(path, "r")
    return f.read()
"#,
        );
        assert!(
            result.resolved_callsites.is_empty(),
            "variable arg0 should not produce ResolvedCallsite"
        );
    }

    #[test]
    fn strip_python_string_quotes_handles_variants() {
        // Single quotes
        assert_eq!(strip_python_string_quotes("'hello'"), "hello");
        // Double quotes
        assert_eq!(strip_python_string_quotes("\"world\""), "world");
        // Triple double quotes
        assert_eq!(
            strip_python_string_quotes("\"\"\"multi\nline\"\"\""),
            "multi\nline"
        );
        // Triple single quotes
        assert_eq!(strip_python_string_quotes("'''text'''"), "text");
        // Raw string prefix
        assert_eq!(
            strip_python_string_quotes("r\"raw\\nstring\""),
            "raw\\nstring"
        );
        // Bytes prefix
        assert_eq!(strip_python_string_quotes("b\"bytes\""), "bytes");
    }
}
