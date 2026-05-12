//! NATS boundary interaction detector for TypeScript/JavaScript.
//!
//! Detects nats (official npm package) patterns that indicate message broker
//! boundary interactions.
//!
//! MB-3A shipped scope:
//!   - nc.publish(subject, data) — provider
//!   - nc.subscribe(subject, ...) — consumer
//!
//! Scope guards (triple):
//!   1. File must have direct `nats` import/require
//!   2. Receiver must be assigned from NATS `connect(...)` call (verified provenance)
//!   3. Call must have extractable subject argument (arg0)
//!
//! Why receiver provenance is required:
//!   - `publish`, `subscribe` are generic method names
//!   - File-level import guard is insufficient — nats may be imported
//!     for setup while other code uses `.publish()` on unrelated objects
//!   - Only connection-tracked receivers prove the call is actually NATS
//!
//! Provenance verification (P1 fix, scope-aware):
//!   The detector tracks which identifiers are actually imported from 'nats':
//!   - `import { connect } from 'nats'` → `connect` is a direct NATS identifier
//!   - `import * as nats from 'nats'` → `nats` is a NATS namespace
//!   - `import nats from 'nats'` → `nats` is a NATS namespace
//!   - `const { connect } = require('nats')` → `connect` is a direct NATS identifier
//!   - `const nats = require('nats')` → `nats` is a NATS namespace
//!
//!   Only calls that resolve to these bindings produce valid provenance:
//!   - `connect(...)` only if `connect` is in the direct import set AND not shadowed
//!   - `nats.connect(...)` only if `nats` is in the namespace set AND not shadowed
//!
//!   Scope-aware shadow detection:
//!   - Uses `lexical_scope::ScopeTree` to track declarations per lexical scope
//!   - At each `connect(...)` call site, checks if the binding is shadowed at that scope
//!   - Inner-scope shadows (const/function declarations) block detection correctly
//!   - Unrelated helper-scope shadows do NOT suppress valid top-level usage
//!
//!   Rejected (not tracked):
//!   - Local function/const that shadows the import at the call site
//!   - Unrelated `other.connect()` calls
//!   - Wrapper functions returning NATS connections
//!   - Cross-file connection flow
//!
//! Intentionally NOT detected (deferred):
//!   - `nc.request(subject, data)` — mixed semantics (outbound + reply handling),
//!     direction is muddy, deferred to MB-3B
//!   - Subject wildcard semantics
//!   - Queue groups
//!   - JetStream patterns
//!   - Cross-file connection tracking
//!
//! Provenance tracking scope (deliberately narrow):
//!   - Same file only
//!   - Direct assignment only (`const nc = await connect(...)`)
//!   - Direct receiver identifier only
//!   - No interprocedural flow
//!   - No wrapper inference
//!   - No alias tracking

use std::collections::HashSet;

use repo_graph_classification::types::SourceLocation;

use crate::lexical_scope::ScopeTree;

/// Raw NATS boundary call site extracted from TS/JS source.
#[derive(Debug, Clone)]
pub struct RawNatsBoundaryCall {
    /// Method name (e.g., "publish", "subscribe").
    pub function_name: String,

    /// Source location of the call expression.
    pub location: SourceLocation,

    /// Enclosing function/method name (for stable key construction).
    /// Empty string for top-level calls.
    pub enclosing_function: String,

    /// Subject string if extractable (arg0).
    pub subject: Option<String>,
}

/// NATS provider methods.
const NATS_PROVIDER_METHODS: &[&str] = &["publish"];

/// NATS consumer methods.
const NATS_CONSUMER_METHODS: &[&str] = &["subscribe"];

/// Check if a method name is a NATS boundary candidate.
pub fn is_nats_boundary_pattern(name: &str) -> bool {
    NATS_PROVIDER_METHODS.contains(&name) || NATS_CONSUMER_METHODS.contains(&name)
}

/// Check if the file has a direct nats import/require.
///
/// Accepts:
/// - `import { connect } from 'nats'`
/// - `import * as nats from 'nats'`
/// - `import nats from 'nats'`
/// - `const { connect } = require('nats')`
/// - `const nats = require('nats')`
/// - `require('nats')` (bare require)
pub fn has_nats_import(root: &tree_sitter::Node, src: &[u8]) -> bool {
    has_nats_import_recursive(root, src)
}

fn has_nats_import_recursive(node: &tree_sitter::Node, src: &[u8]) -> bool {
    match node.kind() {
        // ESM import: import ... from 'nats'
        "import_statement" => {
            if let Some(source) = node.child_by_field_name("source") {
                if let Ok(text) = source.utf8_text(src) {
                    let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                    if trimmed == "nats" {
                        return true;
                    }
                }
            }
        }

        // CommonJS require: require('nats')
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                if function.kind() == "identifier" {
                    if let Ok(name) = function.utf8_text(src) {
                        if name == "require" {
                            if let Some(args) = node.child_by_field_name("arguments") {
                                for i in 0..args.child_count() {
                                    if let Some(arg) = args.child(i) {
                                        if arg.kind() == "string" {
                                            if let Ok(text) = arg.utf8_text(src) {
                                                let trimmed =
                                                    text.trim_matches(|c| c == '"' || c == '\'');
                                                if trimmed == "nats" {
                                                    return true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if has_nats_import_recursive(&child, src) {
                return true;
            }
        }
    }

    false
}

// ── Import binding resolution ───────────────────────────────────────────────
//
// P1 fix: Track which identifiers are actually imported from 'nats', not just
// whether the file has any nats import. This prevents false positives from
// local functions named `connect()` or unrelated `other.connect()` calls.

/// Tracked NATS import bindings in a file.
///
/// Distinguishes between direct `connect` imports and namespace/module imports
/// to enable precise provenance verification.
#[derive(Debug, Default)]
struct NatsImportBindings {
    /// Direct identifiers imported from nats that represent the `connect` function.
    ///
    /// Examples:
    /// - `import { connect } from 'nats'` → "connect"
    /// - `import { connect as natsConnect } from 'nats'` → "natsConnect"
    /// - `const { connect } = require('nats')` → "connect"
    direct_connect_identifiers: HashSet<String>,

    /// Namespace/default/module identifiers representing the nats module.
    ///
    /// Examples:
    /// - `import * as nats from 'nats'` → "nats"
    /// - `import nats from 'nats'` → "nats"
    /// - `const nats = require('nats')` → "nats"
    namespace_identifiers: HashSet<String>,
}

impl NatsImportBindings {
    /// Check if this file has any NATS import bindings.
    fn has_bindings(&self) -> bool {
        !self.direct_connect_identifiers.is_empty() || !self.namespace_identifiers.is_empty()
    }
}

/// Extract NATS import bindings from the AST.
///
/// This replaces the simple `has_nats_import` check with precise binding tracking.
/// Shadowing is handled at call-site resolution via `ScopeTree`, not here.
fn extract_nats_import_bindings(root: &tree_sitter::Node, src: &[u8]) -> NatsImportBindings {
    let mut bindings = NatsImportBindings::default();
    extract_bindings_recursive(root, src, &mut bindings);
    bindings
}

fn extract_bindings_recursive(
    node: &tree_sitter::Node,
    src: &[u8],
    bindings: &mut NatsImportBindings,
) {
    match node.kind() {
        // ESM import: import ... from 'nats'
        "import_statement" => {
            if let Some(source) = node.child_by_field_name("source") {
                if let Ok(text) = source.utf8_text(src) {
                    let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                    if trimmed == "nats" {
                        // This is a NATS import — extract the bindings
                        extract_esm_import_bindings(node, src, bindings);
                    }
                }
            }
        }

        // CommonJS: const ... = require('nats')
        "lexical_declaration" | "variable_declaration" => {
            // Check if this is a require('nats') assignment
            for i in 0..node.child_count() {
                if let Some(declarator) = node.child(i) {
                    if declarator.kind() == "variable_declarator"
                        && is_nats_require_declarator(&declarator, src)
                    {
                        extract_commonjs_bindings(&declarator, src, bindings);
                    }
                }
            }
        }

        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_bindings_recursive(&child, src, bindings);
        }
    }
}

/// Extract bindings from ESM import statement.
///
/// Handles:
/// - `import { connect } from 'nats'` → direct_connect_identifiers
/// - `import { connect as nc } from 'nats'` → direct_connect_identifiers (as "nc")
/// - `import * as nats from 'nats'` → namespace_identifiers
/// - `import nats from 'nats'` → namespace_identifiers
fn extract_esm_import_bindings(
    import_node: &tree_sitter::Node,
    src: &[u8],
    bindings: &mut NatsImportBindings,
) {
    // Look for import_clause child
    for i in 0..import_node.child_count() {
        if let Some(child) = import_node.child(i) {
            match child.kind() {
                // Default import: import nats from 'nats'
                "identifier" => {
                    if let Ok(name) = child.utf8_text(src) {
                        bindings.namespace_identifiers.insert(name.to_string());
                    }
                }

                // import_clause contains the actual bindings
                "import_clause" => {
                    extract_from_import_clause(&child, src, bindings);
                }

                _ => {}
            }
        }
    }
}

/// Extract bindings from an import_clause node.
fn extract_from_import_clause(
    clause: &tree_sitter::Node,
    src: &[u8],
    bindings: &mut NatsImportBindings,
) {
    for i in 0..clause.child_count() {
        if let Some(child) = clause.child(i) {
            match child.kind() {
                // Default import identifier
                "identifier" => {
                    if let Ok(name) = child.utf8_text(src) {
                        bindings.namespace_identifiers.insert(name.to_string());
                    }
                }

                // Namespace import: import * as nats from 'nats'
                "namespace_import" => {
                    // The identifier is the second child (after "* as")
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(src) {
                            bindings.namespace_identifiers.insert(name.to_string());
                        }
                    } else {
                        // Fallback: look for identifier child
                        for j in 0..child.child_count() {
                            if let Some(grandchild) = child.child(j) {
                                if grandchild.kind() == "identifier" {
                                    if let Ok(name) = grandchild.utf8_text(src) {
                                        bindings.namespace_identifiers.insert(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }

                // Named imports: import { connect, StringCodec } from 'nats'
                "named_imports" => {
                    extract_named_imports(&child, src, bindings);
                }

                _ => {}
            }
        }
    }
}

/// Extract bindings from named_imports node.
///
/// Handles:
/// - `{ connect }` → track "connect"
/// - `{ connect as nc }` → track "nc" (the local alias)
fn extract_named_imports(
    named_imports: &tree_sitter::Node,
    src: &[u8],
    bindings: &mut NatsImportBindings,
) {
    for i in 0..named_imports.child_count() {
        if let Some(specifier) = named_imports.child(i) {
            if specifier.kind() == "import_specifier" {
                // import_specifier has name (original) and optionally alias
                let original_name = specifier
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(src).ok());

                let alias = specifier
                    .child_by_field_name("alias")
                    .and_then(|n| n.utf8_text(src).ok());

                // Only track if the original import is "connect"
                if original_name == Some("connect") {
                    // Use alias if present, otherwise use original name
                    let local_name = alias.unwrap_or("connect");
                    bindings
                        .direct_connect_identifiers
                        .insert(local_name.to_string());
                }
            }
        }
    }
}

/// Check if a variable_declarator is a require('nats') call.
fn is_nats_require_declarator(declarator: &tree_sitter::Node, src: &[u8]) -> bool {
    if let Some(value) = declarator.child_by_field_name("value") {
        return is_nats_require_call(&value, src);
    }
    false
}

/// Check if a node is a require('nats') call expression.
fn is_nats_require_call(node: &tree_sitter::Node, src: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }

    if let Some(function) = node.child_by_field_name("function") {
        if function.kind() == "identifier" {
            if let Ok(name) = function.utf8_text(src) {
                if name == "require" {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        for i in 0..args.child_count() {
                            if let Some(arg) = args.child(i) {
                                if arg.kind() == "string" {
                                    if let Ok(text) = arg.utf8_text(src) {
                                        let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                                        return trimmed == "nats";
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Extract bindings from CommonJS require pattern.
///
/// Handles:
/// - `const { connect } = require('nats')` → direct_connect_identifiers
/// - `const nats = require('nats')` → namespace_identifiers
fn extract_commonjs_bindings(
    declarator: &tree_sitter::Node,
    src: &[u8],
    bindings: &mut NatsImportBindings,
) {
    if let Some(name_node) = declarator.child_by_field_name("name") {
        match name_node.kind() {
            // Simple identifier: const nats = require('nats')
            "identifier" => {
                if let Ok(name) = name_node.utf8_text(src) {
                    bindings.namespace_identifiers.insert(name.to_string());
                }
            }

            // Destructuring: const { connect } = require('nats')
            "object_pattern" => {
                for i in 0..name_node.child_count() {
                    if let Some(prop) = name_node.child(i) {
                        // shorthand_property_identifier_pattern for { connect }
                        // pair_pattern for { connect: alias }
                        match prop.kind() {
                            "shorthand_property_identifier_pattern" => {
                                if let Ok(name) = prop.utf8_text(src) {
                                    if name == "connect" {
                                        bindings
                                            .direct_connect_identifiers
                                            .insert(name.to_string());
                                    }
                                }
                            }
                            "pair_pattern" => {
                                // { connect: alias }
                                let key = prop.child_by_field_name("key");
                                let value = prop.child_by_field_name("value");
                                if let (Some(k), Some(v)) = (key, value) {
                                    if let Ok(key_name) = k.utf8_text(src) {
                                        if key_name == "connect" {
                                            if let Ok(alias) = v.utf8_text(src) {
                                                bindings
                                                    .direct_connect_identifiers
                                                    .insert(alias.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

// ── Connection variable tracking ────────────────────────────────────────────

/// Tracked NATS connection variables in a file.
#[derive(Debug, Default)]
struct NatsConnectionVars {
    /// Variables assigned from NATS `connect(...)` calls (verified provenance)
    connection_vars: HashSet<String>,
}

/// Extract NATS connection variable assignments from the AST.
///
/// **P1 fix (scope-aware)**: Only tracks assignments where the `connect` call
/// resolves to a verified NATS import binding at that lexical scope, not any
/// arbitrary function named `connect` or a shadowing declaration.
///
/// Tracks same-file, direct assignments only:
/// - `const nc = await connect(...)` — only if `connect` is imported from nats
///   AND not shadowed at this scope
/// - `const nc = await nats.connect(...)` — only if `nats` is a nats module import
///   AND not shadowed at this scope
fn extract_nats_connection_vars(
    root: &tree_sitter::Node,
    src: &[u8],
    bindings: &NatsImportBindings,
    scope_tree: &ScopeTree,
) -> NatsConnectionVars {
    let mut vars = NatsConnectionVars::default();
    extract_connection_vars_recursive(root, src, bindings, scope_tree, &mut vars);
    vars
}

fn extract_connection_vars_recursive(
    node: &tree_sitter::Node,
    src: &[u8],
    bindings: &NatsImportBindings,
    scope_tree: &ScopeTree,
    vars: &mut NatsConnectionVars,
) {
    // Look for variable_declarator: const nc = connect(...) or const nc = await connect(...)
    if node.kind() == "variable_declarator" {
        if let (Some(name_node), Some(value_node)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            // Only track simple identifier names (not destructuring patterns)
            if name_node.kind() == "identifier" {
                // Check if value is a call to connect() or await connect()
                let call_node = if value_node.kind() == "await_expression" {
                    // await connect(...)
                    value_node.child(1) // Skip "await" keyword, get the expression
                } else if value_node.kind() == "call_expression" {
                    // connect(...)
                    Some(value_node)
                } else {
                    None
                };

                if let Some(call) = call_node {
                    if call.kind() == "call_expression" {
                        if let Some(function) = call.child_by_field_name("function") {
                            // Determine scope at this call site for shadow checking
                            let call_scope = scope_tree.scope_containing(call.start_byte());

                            // Check if it's a direct call to `connect`
                            if function.kind() == "identifier" {
                                if let Ok(fn_name) = function.utf8_text(src) {
                                    // CRITICAL: Verify this is the NATS-imported connect
                                    // AND it is not shadowed at this scope
                                    if bindings.direct_connect_identifiers.contains(fn_name)
                                        && !scope_tree.is_shadowed_at(fn_name, call_scope)
                                    {
                                        if let Ok(var_name) = name_node.utf8_text(src) {
                                            vars.connection_vars.insert(var_name.to_string());
                                        }
                                    }
                                }
                            }
                            // Check for nats.connect() pattern
                            else if function.kind() == "member_expression" {
                                if let (Some(object), Some(property)) = (
                                    function.child_by_field_name("object"),
                                    function.child_by_field_name("property"),
                                ) {
                                    if let (Ok(receiver_name), Ok(method_name)) =
                                        (object.utf8_text(src), property.utf8_text(src))
                                    {
                                        // CRITICAL: Verify receiver is a NATS namespace import
                                        // AND it is not shadowed at this scope
                                        if method_name == "connect"
                                            && bindings
                                                .namespace_identifiers
                                                .contains(receiver_name)
                                            && !scope_tree.is_shadowed_at(receiver_name, call_scope)
                                        {
                                            if let Ok(var_name) = name_node.utf8_text(src) {
                                                vars.connection_vars.insert(var_name.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_connection_vars_recursive(&child, src, bindings, scope_tree, vars);
        }
    }
}

/// Extract NATS boundary calls from a parsed TS/JS file.
///
/// **Scope guards (triple):**
/// 1. File must have direct `nats` import/require with tracked bindings
/// 2. Receiver must be a tracked connection variable (verified provenance)
/// 3. Call must have extractable subject evidence
///
/// **P1 fix (scope-aware)**: Uses lexical scope tree to verify `connect()` calls
/// actually resolve to the NATS import at the call site, not shadowing declarations.
pub fn extract_nats_boundary_calls(
    root: &tree_sitter::Node,
    src: &[u8],
    _file_path: &str,
) -> Vec<RawNatsBoundaryCall> {
    // Guard 1: extract import bindings — if none, no NATS patterns possible
    let bindings = extract_nats_import_bindings(root, src);
    if !bindings.has_bindings() {
        return Vec::new();
    }

    // Build lexical scope tree for shadow resolution
    let scope_tree = ScopeTree::build(root, src);

    // Guard 2 prep: extract tracked connection variables with scope-aware provenance
    let connection_vars = extract_nats_connection_vars(root, src, &bindings, &scope_tree);

    let mut results = Vec::new();
    let mut enclosing_function = String::new();

    extract_from_node(
        root,
        src,
        &mut enclosing_function,
        &connection_vars,
        &mut results,
    );

    results
}

fn extract_from_node(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &mut String,
    connection_vars: &NatsConnectionVars,
    results: &mut Vec<RawNatsBoundaryCall>,
) {
    match node.kind() {
        // Track enclosing function for stable key construction
        "function_declaration" | "method_definition" | "arrow_function" => {
            let prev_enclosing = enclosing_function.clone();

            // Try to extract function name
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(src) {
                    *enclosing_function = name.to_string();
                }
            }

            // Recurse into body
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    extract_from_node(&child, src, enclosing_function, connection_vars, results);
                }
            }

            // Restore enclosing function
            *enclosing_function = prev_enclosing;
            return; // Already recursed
        }

        // Detect `nc.publish(...)`, `nc.subscribe(...)`, etc.
        "call_expression" => {
            if let Some(call) =
                try_extract_nats_call(node, src, enclosing_function, connection_vars)
            {
                results.push(call);
            }
        }

        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, src, enclosing_function, connection_vars, results);
        }
    }
}

/// Try to extract a NATS boundary call from a `call_expression` node.
fn try_extract_nats_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
    connection_vars: &NatsConnectionVars,
) -> Option<RawNatsBoundaryCall> {
    // call_expression structure: function arguments
    let function = node.child_by_field_name("function")?;

    // Only handle member expressions: receiver.method(...)
    if function.kind() != "member_expression" {
        return None;
    }

    let property = function.child_by_field_name("property")?;
    let method_name = property.utf8_text(src).ok()?;

    // Check if this is a NATS boundary method
    if !is_nats_boundary_pattern(method_name) {
        return None;
    }

    // **Guard 2: Receiver provenance check**
    let object = function.child_by_field_name("object")?;

    // Only track direct identifier receivers
    if object.kind() != "identifier" {
        return None;
    }
    let receiver_name = object.utf8_text(src).ok()?;

    // Check receiver against connection vars
    if !connection_vars.connection_vars.contains(receiver_name) {
        return None;
    }

    // **Guard 3: Subject evidence**
    let arguments = node.child_by_field_name("arguments")?;
    let subject = extract_subject_arg(&arguments, src);

    // Require extractable subject to emit
    subject.as_ref()?;

    let start = node.start_position();
    let end = node.end_position();

    Some(RawNatsBoundaryCall {
        function_name: method_name.to_string(),
        location: SourceLocation {
            line_start: (start.row + 1) as i64,
            col_start: start.column as i64,
            line_end: (end.row + 1) as i64,
            col_end: end.column as i64,
        },
        enclosing_function: enclosing_function.to_string(),
        subject,
    })
}

/// Extract subject from the first argument.
fn extract_subject_arg(arguments: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    // Find the first non-punctuation child (the first argument)
    for i in 0..arguments.child_count() {
        if let Some(arg) = arguments.child(i) {
            // Skip parentheses and commas
            if arg.kind() == "(" || arg.kind() == ")" || arg.kind() == "," {
                continue;
            }

            return extract_string_value(&arg, src);
        }
    }
    None
}

/// Extract string value from a node (string literal or identifier).
fn extract_string_value(node: &tree_sitter::Node, src: &[u8]) -> Option<String> {
    match node.kind() {
        "string" | "template_string" => {
            if let Ok(text) = node.utf8_text(src) {
                let trimmed = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                return Some(trimmed.to_string());
            }
        }
        "identifier" => {
            if let Ok(name) = node.utf8_text(src) {
                return Some(format!("${{{}}}", name));
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(source: &str) -> Vec<RawNatsBoundaryCall> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        extract_nats_boundary_calls(&tree.root_node(), source.as_bytes(), "test.ts")
    }

    fn parse_and_check_import(source: &str) -> bool {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        has_nats_import(&tree.root_node(), source.as_bytes())
    }

    // ── Import guard tests ──────────────────────────────────────────────────

    #[test]
    fn import_guard_esm_named() {
        let source = r#"import { connect } from 'nats';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_namespace() {
        let source = r#"import * as nats from 'nats';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_default() {
        let source = r#"import nats from 'nats';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_commonjs_destructure() {
        let source = r#"const { connect } = require('nats');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_commonjs_direct() {
        let source = r#"const nats = require('nats');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_no_nats() {
        let source = r#"import { connect } from 'some-other-lib';"#;
        assert!(!parse_and_check_import(source));
    }

    // ── Scope guard negative test ───────────────────────────────────────────

    #[test]
    fn no_detection_without_nats_import() {
        let source = r#"
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
            nc.subscribe('orders.created');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "should NOT emit NATS surfaces without nats import; got {:?}",
            calls
        );
    }

    // ── Positive detection tests ────────────────────────────────────────────

    #[test]
    fn detect_publish() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', encoder.encode('data'));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "publish");
        assert_eq!(calls[0].subject, Some("orders.created".to_string()));
    }

    #[test]
    fn detect_subscribe() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            const sub = nc.subscribe('orders.created');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "subscribe");
        assert_eq!(calls[0].subject, Some("orders.created".to_string()));
    }

    #[test]
    fn detect_namespace_import_connect() {
        let source = r#"
            import * as nats from 'nats';
            const nc = await nats.connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "publish");
    }

    #[test]
    fn detect_multiple_patterns() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
            nc.subscribe('orders.updated');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2);

        let names: Vec<_> = calls.iter().map(|c| c.function_name.as_str()).collect();
        assert!(names.contains(&"publish"));
        assert!(names.contains(&"subscribe"));
    }

    #[test]
    fn detect_enclosing_function() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            async function publishOrder() {
                nc.publish('orders.created', 'data');
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].enclosing_function, "publishOrder");
    }

    #[test]
    fn detect_variable_subject() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            const subject = 'orders.created';
            nc.publish(subject, 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].subject, Some("${subject}".to_string()));
    }

    // ── Negative tests: receiver provenance guard ───────────────────────────

    #[test]
    fn plain_object_with_subject_not_detected() {
        // P1 regression: plain object with publish/subscribe should NOT emit
        let source = r#"
            import { connect } from 'nats';
            const nc = { publish: (x) => x, subscribe: (x) => x };
            nc.publish('orders.created', 'data');
            nc.subscribe('orders.created');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "plain object receiver should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn untracked_receiver_not_detected() {
        // Receiver `bus` was never assigned from connect(), so not detected
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            const bus = nc; // aliased — but we don't track this
            bus.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "aliased receiver should NOT emit; got {:?}",
            calls
        );
    }

    // ── P1 regression tests: local shadow and unrelated .connect() ──────────

    #[test]
    fn local_connect_function_not_tracked() {
        // P1 FIX: Local function named `connect` shadows the import
        // Should NOT produce tracked receiver
        let source = r#"
            import { connect } from 'nats';

            async function connect() {
                return { publish: () => {}, subscribe: () => {} };
            }

            const nc = await connect();
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "local shadow connect() should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn unrelated_member_connect_not_tracked() {
        // P1 FIX: Unrelated .connect() on non-NATS object
        // Should NOT produce tracked receiver
        let source = r#"
            import { connect } from 'nats';

            const redis = new RedisClient();
            const nc = await redis.connect();
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "unrelated redis.connect() should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn arbitrary_object_connect_not_tracked() {
        // P1 FIX: Arbitrary object with .connect() method
        // Should NOT produce tracked receiver
        let source = r#"
            import { connect, StringCodec } from 'nats';

            const other = { connect: async () => ({ publish: () => {} }) };
            const nc = await other.connect();
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "arbitrary other.connect() should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn nats_namespace_connect_still_works() {
        // P1 FIX: Ensure legit nats.connect() still works
        let source = r#"
            import * as nats from 'nats';
            const nc = await nats.connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "nats.connect() should still work; got {:?}",
            calls
        );
    }

    #[test]
    fn nats_default_import_connect_works() {
        // P1 FIX: Ensure default import nats.connect() works
        let source = r#"
            import nats from 'nats';
            const nc = await nats.connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "default import nats.connect() should work; got {:?}",
            calls
        );
    }

    #[test]
    fn named_import_connect_works() {
        // P1 FIX: Ensure named import connect() works
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "named import connect() should work; got {:?}",
            calls
        );
    }

    #[test]
    fn aliased_import_connect_works() {
        // P1 FIX: Ensure aliased import works
        let source = r#"
            import { connect as natsConnect } from 'nats';
            const nc = await natsConnect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "aliased import connect should work; got {:?}",
            calls
        );
    }

    #[test]
    fn commonjs_namespace_connect_works() {
        // P1 FIX: Ensure CommonJS module pattern works
        let source = r#"
            const nats = require('nats');
            const nc = await nats.connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "CommonJS nats.connect() should work; got {:?}",
            calls
        );
    }

    #[test]
    fn commonjs_destructure_connect_works() {
        // P1 FIX: Ensure CommonJS destructure works
        let source = r#"
            const { connect } = require('nats');
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "CommonJS destructured connect should work; got {:?}",
            calls
        );
    }

    // ── Negative tests: subject evidence guard ──────────────────────────────

    #[test]
    fn publish_without_subject_not_detected() {
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish();
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "publish without subject should NOT emit; got {:?}",
            calls
        );
    }

    // ── request() intentionally NOT detected ────────────────────────────────

    #[test]
    fn request_not_detected() {
        // request() is deferred to MB-3B — mixed semantics
        let source = r#"
            import { connect } from 'nats';
            const nc = await connect({ servers: ['localhost:4222'] });
            const response = await nc.request('service.time', '');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "request() should NOT emit (deferred); got {:?}",
            calls
        );
    }

    // ── CommonJS detection ──────────────────────────────────────────────────

    #[test]
    fn commonjs_require_enables_detection() {
        let source = r#"
            const { connect } = require('nats');
            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "publish");
    }

    // ── Multiple connections ────────────────────────────────────────────────

    #[test]
    fn multiple_connections_tracked() {
        let source = r#"
            import { connect } from 'nats';
            const nc1 = await connect({ servers: ['localhost:4222'] });
            const nc2 = await connect({ servers: ['localhost:4223'] });
            nc1.publish('orders.created', 'data');
            nc2.subscribe('orders.updated');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn detect_function_scoped_connection() {
        // This tests the fixture pattern where nc is declared inside a function
        let source = r#"
            import { connect } from 'nats';

            async function publishOrder(orderId: string, amount: number) {
                const nc = await connect({ servers: 'localhost:4222' });
                nc.publish('orders.created', JSON.stringify({ orderId, amount }));
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "expected 1 call from function-scoped connection; got {:?}",
            calls
        );
        assert_eq!(calls[0].function_name, "publish");
        assert_eq!(calls[0].subject, Some("orders.created".to_string()));
    }

    // ── P1 scope-aware regression tests ─────────────────────────────────────────

    #[test]
    fn p1_inner_scope_const_connect_shadow_not_detected() {
        // P1 REGRESSION: Inner-scope const shadowing import must not emit
        // The user identified this exact pattern as a false positive source.
        let source = r#"
            import { connect } from 'nats';

            async function main() {
                const connect = async () => {
                    return { publish: () => {}, subscribe: () => {} };
                };
                const nc = await connect();
                await nc.publish('orders.created', 'data');
            }
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "P1: inner-scope const connect shadow should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn p1_helper_scope_function_shadow_does_not_suppress_top_level() {
        // P1 REGRESSION: Helper-scoped function shadow must not suppress valid top-level
        // The user identified this pattern as a false negative source with scope-insensitive logic.
        let source = r#"
            import { connect } from 'nats';

            const nc = await connect({ servers: ['localhost:4222'] });
            nc.publish('orders.created', 'data');

            function helper() {
                function connect() {
                    return { notNats: true };
                }
                const local = connect();
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "P1: helper-scoped shadow should NOT suppress top-level; got {:?}",
            calls
        );
        assert_eq!(calls[0].function_name, "publish");
        assert_eq!(calls[0].subject, Some("orders.created".to_string()));
    }

    #[test]
    fn p1_nested_block_const_shadow_not_detected() {
        // P1 REGRESSION: Block-scoped const shadow in if-block must not emit
        let source = r#"
            import { connect } from 'nats';

            async function main() {
                if (process.env.TEST) {
                    const connect = async () => ({ publish: () => {} });
                    const nc = await connect();
                    await nc.publish('test.subject', 'data');
                }
            }
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "P1: block-scoped const connect shadow should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn p1_valid_inner_scope_connect_detected() {
        // P1 POSITIVE: Valid connect() inside function scope (no shadow) should work
        let source = r#"
            import { connect } from 'nats';

            async function main() {
                const nc = await connect({ servers: ['localhost:4222'] });
                await nc.publish('orders.created', 'data');
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(
            calls.len(),
            1,
            "P1: valid inner-scope connect should be detected; got {:?}",
            calls
        );
    }
}
