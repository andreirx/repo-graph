//! Core C extractor implementation.
//!
//! Uses tree-sitter-c to parse C source files and extract structural
//! information: symbols, edges, and metrics.

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::types::{
    ImportBinding, ImportKind, RuntimeBuiltinsSet, SourceLocation,
};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::routing::is_test_file;
use repo_graph_indexer::types::{
    CallArgPayload, EdgeType, ExtractedEdge, ExtractedMetrics, ExtractedNode, ExtractionResult,
    NodeKind, NodeSubtype, Resolution, ResolvedCallsite, Visibility,
};

use crate::metrics::compute_function_metrics;

/// Extractor name and version.
const EXTRACTOR_NAME: &str = "c-core:0.1.0";

/// Languages this extractor handles.
const LANGUAGES: &[&str] = &["c"];

// ── State-boundary constants (C-SB-1) ─────────────────────────────

/// State-boundary-relevant libc:stdio functions.
/// fopen uses synthetic module "libc:stdio".
const SB_STDIO_FUNCTIONS: &[&str] = &["fopen"];

/// State-boundary-relevant libc:fcntl functions.
/// open uses synthetic module "libc:fcntl".
const SB_FCNTL_FUNCTIONS: &[&str] = &["open"];

/// State-boundary-relevant SQLite functions.
/// sqlite3_open* uses synthetic module "sqlite3".
const SB_SQLITE_FUNCTIONS: &[&str] = &["sqlite3_open", "sqlite3_open_v2"];

/// C runtime builtins (libc functions, etc.)
fn c_runtime_builtins() -> RuntimeBuiltinsSet {
    RuntimeBuiltinsSet {
        identifiers: vec![
            // Standard I/O
            "printf", "fprintf", "sprintf", "snprintf", "scanf", "fscanf", "sscanf", "fopen",
            "fclose", "fread", "fwrite", "fflush", "fgets", "fputs", "puts", "gets", "fseek",
            "ftell", "rewind", "feof", "ferror", // Memory
            "malloc", "calloc", "realloc", "free", "memcpy", "memmove", "memset", "memcmp",
            // Strings
            "strlen", "strcpy", "strncpy", "strcat", "strncat", "strcmp", "strncmp", "strchr",
            "strrchr", "strstr", // stdlib
            "exit", "abort", "atexit", "atoi", "atol", "atof", "strtol", "strtod", "rand", "srand",
            "qsort", "bsearch", "abs", "labs", // assert
            "assert",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        // C has no module specifiers like "node:fs" - all includes are file paths
        module_specifiers: Vec::new(),
    }
}

/// Concrete `ExtractorPort` adapter for C.
pub struct CExtractor {
    languages: Vec<String>,
    builtins: RuntimeBuiltinsSet,
    parser: Option<tree_sitter::Parser>,
    c_language: tree_sitter::Language,
}

impl CExtractor {
    /// Create a new extractor. Call `initialize()` before `extract()`.
    pub fn new() -> Self {
        Self {
            languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
            builtins: c_runtime_builtins(),
            parser: None,
            c_language: tree_sitter_c::LANGUAGE.into(),
        }
    }
}

impl Default for CExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorPort for CExtractor {
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
            .set_language(&self.c_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set C grammar: {}", e),
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
        // Verify initialization happened (parser presence proves initialize() was called)
        let _parser = self.parser.as_ref().ok_or_else(|| ExtractorError {
            message: "extractor not initialized — call initialize() first".into(),
        })?;

        // Clone parser for thread safety (tree-sitter parsers are not Send)
        let mut parser_clone = tree_sitter::Parser::new();
        parser_clone
            .set_language(&self.c_language)
            .map_err(|e| ExtractorError {
                message: format!("failed to set C grammar: {}", e),
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

        // Determine if test file (use shared routing policy)
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
            resolved_callsites: Vec::new(),
        };

        // Walk top-level declarations
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "preproc_include" => {
                    extract_include(&child, src, &mut ctx);
                }
                "function_definition" => {
                    extract_function(&child, src, &mut ctx);
                }
                "struct_specifier" => {
                    extract_struct(&child, src, &mut ctx);
                }
                "enum_specifier" => {
                    extract_enum(&child, src, &mut ctx);
                }
                "type_definition" => {
                    extract_typedef(&child, src, &mut ctx);
                }
                "declaration" => {
                    // Skip function declarations (prototypes) per design decision.
                    // BUT: extract embedded struct/enum specifiers (anonymous types)
                    let mut decl_cursor = child.walk();
                    for decl_child in child.children(&mut decl_cursor) {
                        match decl_child.kind() {
                            "struct_specifier" => extract_struct(&decl_child, src, &mut ctx),
                            "enum_specifier" => extract_enum(&decl_child, src, &mut ctx),
                            _ => {}
                        }
                    }
                }
                // Preprocessor blocks: recurse into contents
                "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                    walk_preproc_block(&child, src, &mut ctx);
                }
                _ => {}
            }
        }

        Ok(ExtractionResult {
            nodes: ctx.nodes,
            edges: ctx.edges,
            metrics: ctx.metrics,
            import_bindings: ctx.import_bindings,
            resolved_callsites: ctx.resolved_callsites,
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
    /// Tracks usage count for stable_key disambiguation
    stable_key_counts: HashMap<String, u32>,
    /// C-SB-1: ResolvedCallsite facts for state-boundary integration.
    resolved_callsites: Vec<ResolvedCallsite>,
}

impl<'a> ExtractionCtx<'a> {
    /// Generate a stable_key with duplicate disambiguation.
    fn make_stable_key(&mut self, name: &str, subtype: &NodeSubtype) -> String {
        let subtype_str = serde_json::to_value(subtype)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", subtype));

        let base_key = format!(
            "{}:{}#{}:SYMBOL:{}",
            self.repo_uid, self.file_path, name, subtype_str
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

/// Walk preprocessor blocks to find nested declarations
fn walk_preproc_block(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "preproc_include" => extract_include(&child, src, ctx),
            "function_definition" => extract_function(&child, src, ctx),
            "struct_specifier" => extract_struct(&child, src, ctx),
            "enum_specifier" => extract_enum(&child, src, ctx),
            "type_definition" => extract_typedef(&child, src, ctx),
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                walk_preproc_block(&child, src, ctx);
            }
            _ => {}
        }
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
                // <stdio.h> → strip angle brackets
                let text = child.utf8_text(src).unwrap_or("");
                specifier = text
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string();
                is_system = true;
            }
            "string_literal" => {
                // "myheader.h" → strip quotes
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

    // Create IMPORTS edge
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

    // Import binding
    let identifier = specifier
        .split('/')
        .next_back()
        .unwrap_or(&specifier)
        .trim_end_matches(".h")
        .trim_end_matches(".c")
        .to_string();

    ctx.import_bindings.push(ImportBinding {
        identifier,
        specifier,
        is_relative: !is_system,
        location: Some(location_from_node(node)),
        is_type_only: false,
        imported_name: None,
        // C #include brings in all declarations from the header,
        // similar to TypeScript namespace import semantics
        kind: ImportKind::Namespace,
    });
}

// ── Function extraction ──────────────────────────────────────────

fn extract_function(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // Get function name from declarator
    let declarator = match node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };

    let name = extract_function_name(&declarator, src);
    if name.is_empty() {
        return;
    }

    // Check for static (private)
    let is_static = node.children(&mut node.walk()).any(|c| {
        c.kind() == "storage_class_specifier" && c.utf8_text(src).unwrap_or("") == "static"
    });

    let visibility = if is_static {
        Visibility::Private
    } else {
        Visibility::Export
    };

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Function);
    let func_uid = uuid::Uuid::new_v4().to_string();

    // Extract signature
    let params = declarator.child_by_field_name("parameters");
    let signature = params.map(|p| format!("{}{}", name, p.utf8_text(src).unwrap_or("()")));

    ctx.nodes.push(ExtractedNode {
        node_uid: func_uid.clone(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key: stable_key.clone(),
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Function),
        name: name.clone(),
        qualified_name: Some(name.clone()),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature,
        visibility: Some(visibility),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: None,
    });

    // Extract calls from function body and compute metrics
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_body(&body, src, &func_uid, ctx);

        let metrics = compute_function_metrics(&body, params.as_ref());
        ctx.metrics.insert(stable_key, metrics);
    }
}

/// Extract function name from a declarator node
fn extract_function_name(declarator: &tree_sitter::Node, src: &[u8]) -> String {
    // Handle function_declarator wrapping
    let mut current = *declarator;
    while current.kind() == "function_declarator" || current.kind() == "pointer_declarator" {
        if let Some(inner) = current.child_by_field_name("declarator") {
            current = inner;
        } else {
            break;
        }
    }

    if current.kind() == "identifier" {
        current.utf8_text(src).unwrap_or("").to_string()
    } else {
        // Try to find identifier child
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            if child.kind() == "identifier" {
                return child.utf8_text(src).unwrap_or("").to_string();
            }
        }
        String::new()
    }
}

// ── Call extraction ──────────────────────────────────────────────

fn extract_calls_from_body(
    body: &tree_sitter::Node,
    src: &[u8],
    source_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    fn walk_for_calls(
        node: &tree_sitter::Node,
        src: &[u8],
        source_node_uid: &str,
        ctx: &mut ExtractionCtx,
    ) {
        if node.kind() == "call_expression" {
            extract_call(node, src, source_node_uid, ctx);
        }

        // Don't recurse into nested function definitions
        if node.kind() == "function_definition" {
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_for_calls(&child, src, source_node_uid, ctx);
        }
    }

    walk_for_calls(body, src, source_node_uid, ctx);
}

fn extract_call(
    node: &tree_sitter::Node,
    src: &[u8],
    source_node_uid: &str,
    ctx: &mut ExtractionCtx,
) {
    // Get the function being called
    let function = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };

    // Only extract plain identifier calls (per design decision).
    // Function pointer calls (ptr(), (*fn)()), member-based calls
    // (obj->method()), and other non-identifier callees are excluded
    // entirely — not emitted as unresolved. Rationale: without type
    // resolution, we cannot determine the target; emitting an edge
    // with an unresolvable target adds noise without adding value.
    // This is documented in c-extractor-v1.md under "CALLS Scope".
    if function.kind() != "identifier" {
        return;
    }

    let target_name = function.utf8_text(src).unwrap_or("");
    if target_name.is_empty() {
        return;
    }

    ctx.edges.push(ExtractedEdge {
        edge_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        source_node_uid: source_node_uid.into(),
        target_key: target_name.into(),
        edge_type: EdgeType::Calls,
        resolution: Resolution::Static,
        extractor: EXTRACTOR_NAME.into(),
        location: Some(location_from_node(node)),
        metadata_json: Some(
            serde_json::json!({
                "calleeName": target_name
            })
            .to_string(),
        ),
    });

    // C-SB-1: Attempt to generate ResolvedCallsite for state-boundary analysis.
    if let Some(callsite) = try_resolve_callsite_c(node, src, source_node_uid, target_name) {
        ctx.resolved_callsites.push(callsite);
    }
}

// ── State-boundary callsite resolution (C-SB-1) ──────────────────

/// Attempt to generate a `ResolvedCallsite` for state-boundary analysis.
///
/// This handles:
/// 1. fopen(path, mode) → synthetic module "libc:stdio", direction-specific symbol
/// 2. open(path, flags) → synthetic module "libc:fcntl", direction-specific symbol
/// 3. sqlite3_open(path, &db) → synthetic module "sqlite3"
///
/// Returns `None` if:
/// - The function is not state-boundary-relevant
/// - arg0 is not a string literal (dynamic paths are out of scope)
fn try_resolve_callsite_c(
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
    let arg0_payload = extract_arg0_payload(&arguments, src)?;

    // Extract arg1 if present and needed for mode/flags.
    let arg1_payload = extract_arg1_payload(&arguments, src);

    // Determine direction-specific resolved_symbol.
    let resolved_symbol = match resolved_module {
        "libc:stdio" => {
            // fopen: use mode argument to determine direction.
            let mode = match &arg1_payload {
                Some(CallArgPayload::StringLiteral { value }) => value.as_str(),
                _ => "", // Default to read if no mode
            };
            format!("{}_{}", base_symbol, normalize_fopen_mode(mode))
        }
        "libc:fcntl" => {
            // open: use flags argument to determine direction.
            let flags = match &arg1_payload {
                Some(CallArgPayload::StringLiteral { value }) => value.as_str(),
                _ => "", // Default to read_write if flags are dynamic
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

/// Extract arg0 as a CallArgPayload. Returns None if not a string literal.
fn extract_arg0_payload(arguments: &tree_sitter::Node, src: &[u8]) -> Option<CallArgPayload> {
    let mut cursor = arguments.walk();
    let mut arg_index = 0;

    for child in arguments.children(&mut cursor) {
        // Skip punctuation: opening paren, commas, closing paren.
        if child.kind() == "("
            || child.kind() == ")"
            || child.kind() == ","
            || child.kind() == "comment"
        {
            continue;
        }

        if arg_index == 0 {
            // This is arg0.
            if child.kind() == "string_literal" {
                let text = child.utf8_text(src).ok()?;
                // Strip quotes. C string literals are "..."
                let value = text.trim_matches('"').to_string();
                return Some(CallArgPayload::StringLiteral { value });
            }
            // Not a string literal → dynamic path.
            return None;
        }
        arg_index += 1;
    }
    None
}

/// Extract arg1 as a CallArgPayload if present. For fopen this is the mode,
/// for open this might be the flags identifier.
fn extract_arg1_payload(arguments: &tree_sitter::Node, src: &[u8]) -> Option<CallArgPayload> {
    let mut cursor = arguments.walk();
    let mut arg_index = 0;
    for child in arguments.children(&mut cursor) {
        // Skip punctuation.
        if child.kind() == ","
            || child.kind() == "("
            || child.kind() == ")"
            || child.kind() == "comment"
        {
            continue;
        }

        if arg_index == 1 {
            // This is arg1.
            if child.kind() == "string_literal" {
                let text = child.utf8_text(src).ok()?;
                let value = text.trim_matches('"').to_string();
                return Some(CallArgPayload::StringLiteral { value });
            } else if child.kind() == "identifier" {
                // For open(), flags are often identifiers like O_RDONLY.
                // We capture the identifier name as a string literal for
                // pattern matching in normalize_open_flags.
                let value = child.utf8_text(src).ok()?.to_string();
                return Some(CallArgPayload::StringLiteral { value });
            }
            // Dynamic arg1 — return None.
            return None;
        }
        arg_index += 1;
    }
    None
}

/// Normalize fopen mode string to direction suffix.
///
/// Mode interpretation (matches Python adapter):
/// - `'r'`, `'rb'` → `read`
/// - `'w'`, `'wb'`, `'a'`, `'ab'`, `'x'`, `'xb'` → `write`
/// - Contains `'+'` → `read_write`
/// - Missing or unknown → `read` (C default)
fn normalize_fopen_mode(mode: &str) -> &'static str {
    // Strip 'b' (binary mode indicator).
    let mode_normalized: String = mode.chars().filter(|c| *c != 'b').collect();

    if mode_normalized.contains('+') {
        "read_write"
    } else if mode_normalized.starts_with('r') || mode_normalized.is_empty() {
        "read"
    } else if mode_normalized.starts_with('w')
        || mode_normalized.starts_with('a')
        || mode_normalized.starts_with('x')
    {
        "write"
    } else {
        "read" // Unknown mode, default to read.
    }
}

/// Normalize open() flags identifier to direction suffix.
///
/// Flag pattern matching:
/// - `O_RDONLY` → `read`
/// - `O_WRONLY` → `write`
/// - `O_RDWR` → `read_write`
/// - Unknown/dynamic → `read_write` (conservative default)
fn normalize_open_flags(flags: &str) -> &'static str {
    if flags.contains("O_RDONLY") {
        "read"
    } else if flags.contains("O_WRONLY") {
        "write"
    } else if flags.contains("O_RDWR") {
        "read_write"
    } else {
        // Unknown flags, conservative default.
        "read_write"
    }
}

// ── Struct extraction ────────────────────────────────────────────

fn extract_struct(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // Get struct name
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "anon_struct".to_string());

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Struct);

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Struct),
        name: name.clone(),
        qualified_name: Some(name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: None,
    });
}

// ── Enum extraction ──────────────────────────────────────────────

fn extract_enum(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // Get enum name
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "anon_enum".to_string());

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::Enum);

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::Enum),
        name: name.clone(),
        qualified_name: Some(name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: None,
    });
}

// ── Typedef extraction ───────────────────────────────────────────

fn extract_typedef(node: &tree_sitter::Node, src: &[u8], ctx: &mut ExtractionCtx) {
    // Find the type_identifier (the name being defined)
    let mut name = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            name = child.utf8_text(src).unwrap_or("").to_string();
            break;
        }
        // Handle more complex declarators
        if child.kind() == "type_definition" || child.kind() == "declaration" {
            if let Some(id) = find_type_identifier(&child, src) {
                name = id;
                break;
            }
        }
    }

    // Also check for declarator patterns
    if name.is_empty() {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            name = extract_declarator_name(&declarator, src);
        }
    }

    if name.is_empty() {
        return;
    }

    let stable_key = ctx.make_stable_key(&name, &NodeSubtype::TypeAlias);

    ctx.nodes.push(ExtractedNode {
        node_uid: uuid::Uuid::new_v4().to_string(),
        snapshot_uid: ctx.snapshot_uid.into(),
        repo_uid: ctx.repo_uid.into(),
        stable_key,
        kind: NodeKind::Symbol,
        subtype: Some(NodeSubtype::TypeAlias),
        name: name.clone(),
        qualified_name: Some(name),
        file_uid: Some(ctx.file_uid.into()),
        parent_node_uid: None,
        location: Some(location_from_node(node)),
        signature: None,
        visibility: Some(Visibility::Export),
        doc_comment: extract_doc_comment(node, src),
        metadata_json: None,
    });
}

fn find_type_identifier(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    if node.kind() == "type_identifier" {
        return Some(node.utf8_text(src).ok()?.to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = find_type_identifier(&child, src) {
            return Some(name);
        }
    }
    None
}

fn extract_declarator_name(declarator: &tree_sitter::Node, src: &[u8]) -> String {
    if declarator.kind() == "type_identifier" || declarator.kind() == "identifier" {
        return declarator.utf8_text(src).unwrap_or("").to_string();
    }

    // Handle pointer_declarator, array_declarator, etc.
    if let Some(inner) = declarator.child_by_field_name("declarator") {
        return extract_declarator_name(&inner, src);
    }

    // Search children
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
    // Look for preceding comment
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment" {
            let text = p.utf8_text(src).ok()?;
            // Only treat /** ... */ and /// as doc comments
            if text.starts_with("/**") || text.starts_with("///") {
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

    fn extract_ok(ext: &CExtractor, source: &str, path: &str) -> ExtractionResult {
        ext.extract(source, path, &format!("r1:{}", path), "r1", "snap1")
            .expect("extraction should succeed")
    }

    #[test]
    fn file_node_has_correct_stable_key() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "int x = 1;\n", "src/main.c");

        assert!(!result.nodes.is_empty());
        let file_node = &result.nodes[0];
        assert_eq!(file_node.stable_key, "r1:src/main.c:FILE");
        assert_eq!(file_node.kind, NodeKind::File);
    }

    #[test]
    fn function_definition_creates_symbol() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo(int x) { return; }\n", "src/main.c");

        let func = result.nodes.iter().find(|n| n.name == "foo").unwrap();
        assert_eq!(func.stable_key, "r1:src/main.c#foo:SYMBOL:FUNCTION");
        assert_eq!(func.kind, NodeKind::Symbol);
        assert_eq!(func.subtype, Some(NodeSubtype::Function));
        assert_eq!(func.visibility, Some(Visibility::Export));
    }

    #[test]
    fn static_function_is_private() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "static void helper() {}\n", "src/main.c");

        let func = result.nodes.iter().find(|n| n.name == "helper").unwrap();
        assert_eq!(func.visibility, Some(Visibility::Private));
    }

    #[test]
    fn function_declaration_does_not_create_symbol() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo(int x);\n", "src/main.c");

        // Only FILE node, no function symbol
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::File);
    }

    #[test]
    fn include_creates_imports_edge() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "#include \"myheader.h\"\n", "src/main.c");

        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].edge_type, EdgeType::Imports);
        assert_eq!(result.edges[0].target_key, "myheader.h");

        assert_eq!(result.import_bindings.len(), 1);
        assert!(result.import_bindings[0].is_relative);
    }

    #[test]
    fn system_include_is_not_relative() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "#include <stdio.h>\n", "src/main.c");

        assert_eq!(result.import_bindings.len(), 1);
        assert!(!result.import_bindings[0].is_relative);
    }

    #[test]
    fn function_call_creates_calls_edge() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo() { bar(); }\n", "src/main.c");

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].target_key, "bar");
    }

    #[test]
    fn function_pointer_call_excluded() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "void foo() { (*ptr)(); }\n", "src/main.c");

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();

        // Function pointer calls are excluded per design
        assert!(calls.is_empty());
    }

    #[test]
    fn struct_creates_symbol() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "struct Point { int x; int y; };\n", "src/main.c");

        let s = result.nodes.iter().find(|n| n.name == "Point").unwrap();
        assert_eq!(s.stable_key, "r1:src/main.c#Point:SYMBOL:STRUCT");
        assert_eq!(s.subtype, Some(NodeSubtype::Struct));
    }

    #[test]
    fn anonymous_struct_uses_anon_prefix() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "struct { int x; } var;\n", "src/main.c");

        let s = result.nodes.iter().find(|n| n.name == "anon_struct");
        assert!(s.is_some());
    }

    #[test]
    fn enum_creates_symbol() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "enum Status { OK, ERROR };\n", "src/main.c");

        let e = result.nodes.iter().find(|n| n.name == "Status").unwrap();
        assert_eq!(e.stable_key, "r1:src/main.c#Status:SYMBOL:ENUM");
        assert_eq!(e.subtype, Some(NodeSubtype::Enum));
    }

    #[test]
    fn typedef_creates_symbol() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, "typedef int MyInt;\n", "src/main.c");

        let t = result.nodes.iter().find(|n| n.name == "MyInt").unwrap();
        assert_eq!(t.stable_key, "r1:src/main.c#MyInt:SYMBOL:TYPE_ALIAS");
        assert_eq!(t.subtype, Some(NodeSubtype::TypeAlias));
    }

    #[test]
    fn duplicate_functions_get_disambiguated() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"
#ifdef FOO
static void helper() {}
#else
static void helper() {}
#endif
"#,
            "src/main.c",
        );

        let helpers: Vec<_> = result.nodes.iter().filter(|n| n.name == "helper").collect();

        assert_eq!(helpers.len(), 2);

        let keys: std::collections::HashSet<_> =
            helpers.iter().map(|n| n.stable_key.as_str()).collect();
        assert_eq!(keys.len(), 2); // All unique

        assert!(helpers
            .iter()
            .any(|n| n.stable_key.ends_with("#helper:SYMBOL:FUNCTION")));
        assert!(helpers
            .iter()
            .any(|n| n.stable_key.ends_with("#helper:SYMBOL:FUNCTION:dup2")));
    }

    #[test]
    fn complexity_metrics_populated() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            "void foo(int a, int b) { if (a) { while (b) {} } }\n",
            "src/main.c",
        );

        assert_eq!(result.metrics.len(), 1);
        let (key, metrics) = result.metrics.iter().next().unwrap();
        assert!(key.contains("foo"));
        assert_eq!(metrics.cyclomatic_complexity, 3); // base + if + while
        assert_eq!(metrics.parameter_count, 2);
        assert_eq!(metrics.max_nesting_depth, 2);
    }

    // -- ResolvedCallsite extraction tests (C-SB-1) --

    #[test]
    fn fopen_read_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { FILE* f = fopen("config.txt", "r"); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "libc:stdio");
        assert_eq!(cs.resolved_symbol, "fopen_read");
        assert!(
            matches!(&cs.arg0_payload, CallArgPayload::StringLiteral { value } if value == "config.txt")
        );
    }

    #[test]
    fn fopen_write_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { FILE* f = fopen("output.log", "w"); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "libc:stdio");
        assert_eq!(cs.resolved_symbol, "fopen_write");
    }

    #[test]
    fn fopen_append_emits_write() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { FILE* f = fopen("app.log", "a"); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "fopen_write");
    }

    #[test]
    fn fopen_read_write_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { FILE* f = fopen("data.bin", "r+"); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol,
            "fopen_read_write"
        );
    }

    #[test]
    fn fopen_binary_mode_normalized() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { FILE* f = fopen("data.bin", "rb"); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        // 'rb' normalizes to 'r' → fopen_read
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "fopen_read");
    }

    #[test]
    fn fopen_dynamic_path_skipped() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo(char* path) { FILE* f = fopen(path, "r"); }"#,
            "src/main.c",
        );

        // Dynamic path → no ResolvedCallsite
        assert!(result.resolved_callsites.is_empty());
        // But CALLS edge still emitted
        assert!(result
            .edges
            .iter()
            .any(|e| e.edge_type == EdgeType::Calls && e.target_key == "fopen"));
    }

    #[test]
    fn open_rdonly_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { int fd = open("data.bin", O_RDONLY); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "libc:fcntl");
        assert_eq!(cs.resolved_symbol, "open_read");
    }

    #[test]
    fn open_wronly_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { int fd = open("out.bin", O_WRONLY); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_symbol, "open_write");
    }

    #[test]
    fn open_rdwr_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { int fd = open("file.dat", O_RDWR); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol,
            "open_read_write"
        );
    }

    #[test]
    fn sqlite3_open_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { sqlite3* db; sqlite3_open("app.db", &db); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        let cs = &result.resolved_callsites[0];
        assert_eq!(cs.resolved_module, "sqlite3");
        assert_eq!(cs.resolved_symbol, "sqlite3_open");
        assert!(
            matches!(&cs.arg0_payload, CallArgPayload::StringLiteral { value } if value == "app.db")
        );
    }

    #[test]
    fn sqlite3_open_v2_emits_resolved_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(
            &ext,
            r#"void foo() { sqlite3* db; sqlite3_open_v2("app.db", &db, flags, 0); }"#,
            "src/main.c",
        );

        assert_eq!(result.resolved_callsites.len(), 1);
        assert_eq!(result.resolved_callsites[0].resolved_module, "sqlite3");
        assert_eq!(
            result.resolved_callsites[0].resolved_symbol,
            "sqlite3_open_v2"
        );
    }

    #[test]
    fn non_state_boundary_call_no_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        let result = extract_ok(&ext, r#"void foo() { printf("hello"); }"#, "src/main.c");

        // printf is not state-boundary-relevant
        assert!(result.resolved_callsites.is_empty());
    }

    #[test]
    fn top_level_fopen_no_callsite() {
        let mut ext = CExtractor::new();
        ext.initialize().unwrap();
        // Top-level calls (outside function) can't have enclosing_symbol_node_uid
        // because they're associated with FILE node. Currently these won't produce
        // ResolvedCallsite because extract_calls_from_body is only called for functions.
        let result = extract_ok(&ext, r#"FILE* g = fopen("global.txt", "r");"#, "src/main.c");

        // Top-level variable initializers are declarations, not call_expressions
        // in tree-sitter-c parse, so no callsite is extracted.
        assert!(result.resolved_callsites.is_empty());
    }
}
