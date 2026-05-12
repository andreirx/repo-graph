//! Lexical scope tracking for TypeScript/JavaScript AST analysis.
//!
//! This module provides scope-aware binding resolution for import shadowing detection.
//! It does NOT implement full name resolution — only enough to determine whether an
//! imported binding is shadowed at a specific call site.
//!
//! ## Scope model
//!
//! JavaScript/TypeScript has two scoping regimes:
//! - Function scope: `var` declarations, function declarations
//! - Block scope: `let`, `const` declarations
//!
//! For import shadowing detection, we track both because any declaration can shadow
//! an imported name.
//!
//! ## Usage
//!
//! ```ignore
//! let tree = ScopeTree::build(root, src);
//! let scope_idx = tree.scope_containing(call_site_byte_offset);
//! let shadowed = tree.is_shadowed_at("connect", scope_idx);
//! ```

use std::collections::HashSet;

/// A lexical scope in the source file.
#[derive(Debug, Default)]
pub struct LexicalScope {
    /// Names declared in this scope (functions, variables, parameters).
    declarations: HashSet<String>,
    /// Byte range [start, end) of this scope in the source.
    start_byte: usize,
    end_byte: usize,
    /// Parent scope index, None for module scope.
    parent: Option<usize>,
}

/// Scope tree for lexical binding resolution.
///
/// Index 0 is always the module (file) scope where imports live.
#[derive(Debug, Default)]
pub struct ScopeTree {
    scopes: Vec<LexicalScope>,
}

impl ScopeTree {
    /// Build a scope tree from a tree-sitter AST.
    ///
    /// Traverses the AST and collects:
    /// - Scope boundaries (functions, arrow functions, blocks)
    /// - Declarations within each scope (function declarations, variable declarations, parameters)
    pub fn build(root: &tree_sitter::Node, src: &[u8]) -> Self {
        let mut tree = ScopeTree::default();

        // Module scope (index 0) covers the entire file
        tree.scopes.push(LexicalScope {
            declarations: HashSet::new(),
            start_byte: root.start_byte(),
            end_byte: root.end_byte(),
            parent: None,
        });

        // Build scope tree recursively
        tree.build_recursive(root, src, 0);

        tree
    }

    /// Find the innermost scope containing a byte offset.
    ///
    /// Returns the scope index. Always returns at least 0 (module scope).
    pub fn scope_containing(&self, byte_offset: usize) -> usize {
        // Start from innermost scopes (highest indices) and work backward
        // This works because child scopes are added after parent scopes
        for (idx, scope) in self.scopes.iter().enumerate().rev() {
            if byte_offset >= scope.start_byte && byte_offset < scope.end_byte {
                return idx;
            }
        }
        // Fallback to module scope
        0
    }

    /// Check if a name is shadowed at a given scope.
    ///
    /// Returns true if any scope from `scope_idx` up to and including module
    /// scope (index 0) declares this name.
    ///
    /// In JavaScript/TypeScript, local declarations shadow imports even at
    /// module scope. For example:
    /// ```typescript
    /// import { connect } from 'nats';
    /// function connect() {}  // shadows the import
    /// ```
    ///
    /// The scope tree only tracks local declarations (function declarations,
    /// variable declarations, parameters), not imports. So any declaration
    /// found means the import is shadowed.
    pub fn is_shadowed_at(&self, name: &str, scope_idx: usize) -> bool {
        let mut current = scope_idx;

        loop {
            let scope = &self.scopes[current];

            // Check if this scope declares the name
            // ANY local declaration shadows the import, including at module scope
            if scope.declarations.contains(name) {
                return true;
            }

            // Move to parent scope
            match scope.parent {
                Some(parent_idx) => current = parent_idx,
                None => return false, // Reached module scope root, no shadow found
            }
        }
    }

    /// Build scope tree recursively.
    fn build_recursive(&mut self, node: &tree_sitter::Node, src: &[u8], current_scope: usize) {
        // Check if this node creates a new scope
        let new_scope = match node.kind() {
            // Function-like constructs create new scopes
            "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function" => {
                let scope_idx = self.scopes.len();
                self.scopes.push(LexicalScope {
                    declarations: HashSet::new(),
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                    parent: Some(current_scope),
                });

                // Extract parameters into the new scope
                self.extract_parameters(node, src, scope_idx);

                Some(scope_idx)
            }

            // Block statements create new scopes for let/const
            // But we track all declarations uniformly for simplicity
            "statement_block" => {
                // Only create new scope if not directly under a function
                // (function body blocks share scope with function)
                let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");

                if matches!(
                    parent_kind,
                    "function_declaration"
                        | "function"
                        | "arrow_function"
                        | "method_definition"
                        | "generator_function_declaration"
                        | "generator_function"
                ) {
                    // Function body — scope already created by function node
                    None
                } else {
                    // Standalone block (if, for, while, etc.)
                    let scope_idx = self.scopes.len();
                    self.scopes.push(LexicalScope {
                        declarations: HashSet::new(),
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        parent: Some(current_scope),
                    });
                    Some(scope_idx)
                }
            }

            _ => None,
        };

        let scope_for_children = new_scope.unwrap_or(current_scope);

        // Extract declarations in current scope
        self.extract_declarations(node, src, scope_for_children);

        // Recurse into children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.build_recursive(&child, src, scope_for_children);
            }
        }
    }

    /// Extract declarations from a node into the given scope.
    fn extract_declarations(&mut self, node: &tree_sitter::Node, src: &[u8], scope_idx: usize) {
        match node.kind() {
            // Function declarations
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(src) {
                        // Function declarations go in the PARENT scope (they're hoisted)
                        // But for shadowing purposes, we put them in current scope
                        // since the function name is visible in the enclosing scope
                        if let Some(parent_idx) = self.scopes[scope_idx].parent {
                            self.scopes[parent_idx]
                                .declarations
                                .insert(name.to_string());
                        } else {
                            // Module scope function
                            self.scopes[scope_idx].declarations.insert(name.to_string());
                        }
                    }
                }
            }

            // Variable declarations: const, let, var
            "lexical_declaration" | "variable_declaration" => {
                self.extract_variable_names(node, src, scope_idx);
            }

            _ => {}
        }
    }

    /// Extract variable names from a declaration node.
    ///
    /// Skips CommonJS require() patterns since they are import mechanisms,
    /// not local declarations that could shadow imports.
    fn extract_variable_names(&mut self, node: &tree_sitter::Node, src: &[u8], scope_idx: usize) {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "variable_declarator" {
                    // Skip require() patterns - they are imports, not shadows
                    if Self::is_require_call(&child, src) {
                        continue;
                    }
                    if let Some(name_node) = child.child_by_field_name("name") {
                        self.extract_binding_pattern(name_node, src, scope_idx);
                    }
                }
            }
        }
    }

    /// Check if a variable_declarator is a require() call pattern.
    fn is_require_call(declarator: &tree_sitter::Node, src: &[u8]) -> bool {
        if let Some(value) = declarator.child_by_field_name("value") {
            return Self::is_require_call_expression(&value, src);
        }
        false
    }

    /// Check if a node is a require() call expression.
    fn is_require_call_expression(node: &tree_sitter::Node, src: &[u8]) -> bool {
        if node.kind() != "call_expression" {
            return false;
        }
        if let Some(function) = node.child_by_field_name("function") {
            if function.kind() == "identifier" {
                if let Ok(name) = function.utf8_text(src) {
                    return name == "require";
                }
            }
        }
        false
    }

    /// Extract names from a binding pattern (handles destructuring).
    fn extract_binding_pattern(&mut self, node: tree_sitter::Node, src: &[u8], scope_idx: usize) {
        match node.kind() {
            "identifier" => {
                if let Ok(name) = node.utf8_text(src) {
                    self.scopes[scope_idx].declarations.insert(name.to_string());
                }
            }

            // Object destructuring: const { a, b } = ...
            "object_pattern" => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "shorthand_property_identifier_pattern" {
                            if let Ok(name) = child.utf8_text(src) {
                                self.scopes[scope_idx].declarations.insert(name.to_string());
                            }
                        } else if child.kind() == "pair_pattern" {
                            // const { a: renamed } = ... — extract 'renamed'
                            if let Some(value) = child.child_by_field_name("value") {
                                self.extract_binding_pattern(value, src, scope_idx);
                            }
                        }
                    }
                }
            }

            // Array destructuring: const [a, b] = ...
            "array_pattern" => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        self.extract_binding_pattern(child, src, scope_idx);
                    }
                }
            }

            _ => {}
        }
    }

    /// Extract parameter names from a function into its scope.
    fn extract_parameters(&mut self, node: &tree_sitter::Node, src: &[u8], scope_idx: usize) {
        if let Some(params) = node.child_by_field_name("parameters") {
            for i in 0..params.child_count() {
                if let Some(param) = params.child(i) {
                    match param.kind() {
                        "identifier" => {
                            if let Ok(name) = param.utf8_text(src) {
                                self.scopes[scope_idx].declarations.insert(name.to_string());
                            }
                        }
                        "required_parameter" | "optional_parameter" => {
                            if let Some(pattern) = param.child_by_field_name("pattern") {
                                self.extract_binding_pattern(pattern, src, scope_idx);
                            }
                        }
                        "rest_pattern" => {
                            // ...rest parameter
                            for j in 0..param.child_count() {
                                if let Some(child) = param.child(j) {
                                    if child.kind() == "identifier" {
                                        if let Ok(name) = child.utf8_text(src) {
                                            self.scopes[scope_idx]
                                                .declarations
                                                .insert(name.to_string());
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
    }

    /// Debug: print scope tree structure.
    #[allow(dead_code)]
    pub fn debug_print(&self, src: &[u8]) {
        for (idx, scope) in self.scopes.iter().enumerate() {
            let snippet_start = scope.start_byte;
            let snippet_end = (scope.start_byte + 40).min(scope.end_byte);
            let snippet = std::str::from_utf8(&src[snippet_start..snippet_end])
                .unwrap_or("<invalid utf8>")
                .replace('\n', "\\n");

            eprintln!(
                "Scope {}: parent={:?}, bytes={}..{}, decls={:?}, snippet=\"{}...\"",
                idx, scope.parent, scope.start_byte, scope.end_byte, scope.declarations, snippet
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(src: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    #[test]
    fn module_scope_contains_top_level_declarations() {
        let src = r#"
const connect = () => {};
function helper() {}
let x = 1;
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Module scope should have: connect, helper, x
        let module_scope = &scope_tree.scopes[0];
        assert!(module_scope.declarations.contains("connect"));
        assert!(module_scope.declarations.contains("helper"));
        assert!(module_scope.declarations.contains("x"));
    }

    #[test]
    fn function_creates_new_scope() {
        let src = r#"
function outer() {
    const inner = 1;
}
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Should have 2 scopes: module + outer function
        assert!(scope_tree.scopes.len() >= 2);

        // Module scope has 'outer'
        assert!(scope_tree.scopes[0].declarations.contains("outer"));

        // Function scope has 'inner'
        let fn_scope = scope_tree
            .scopes
            .iter()
            .find(|s| s.declarations.contains("inner"));
        assert!(fn_scope.is_some());
    }

    #[test]
    fn inner_scope_shadows_import() {
        let src = r#"
import { connect } from 'nats';

async function main() {
    const connect = async () => {};
    const nc = await connect();
}
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Find byte offset of `await connect()` call inside main
        let call_offset = src.find("await connect()").unwrap();
        let scope_idx = scope_tree.scope_containing(call_offset);

        // At this scope, 'connect' should be shadowed
        assert!(
            scope_tree.is_shadowed_at("connect", scope_idx),
            "connect should be shadowed inside main() where local const shadows import"
        );
    }

    #[test]
    fn top_level_not_shadowed_by_inner_function() {
        let src = r#"
import { connect } from 'nats';

const nc = await connect();

function helper() {
    function connect() {}
}
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Find byte offset of top-level `await connect()` call
        let call_offset = src.find("await connect()").unwrap();
        let scope_idx = scope_tree.scope_containing(call_offset);

        // At module scope, 'connect' should NOT be shadowed
        assert!(
            !scope_tree.is_shadowed_at("connect", scope_idx),
            "connect should not be shadowed at top level despite inner function shadow"
        );
    }

    #[test]
    fn arrow_function_creates_scope() {
        let src = r#"
const outer = () => {
    const inner = 1;
};
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Module scope has 'outer'
        assert!(scope_tree.scopes[0].declarations.contains("outer"));

        // Arrow function scope has 'inner'
        let arrow_scope = scope_tree
            .scopes
            .iter()
            .find(|s| s.declarations.contains("inner"));
        assert!(arrow_scope.is_some());
    }

    #[test]
    fn parameters_in_function_scope() {
        let src = r#"
function foo(connect, other) {
    return connect();
}
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Find byte offset inside function body
        let call_offset = src.find("return connect()").unwrap();
        let scope_idx = scope_tree.scope_containing(call_offset);

        // 'connect' is a parameter, so it shadows any import
        assert!(
            scope_tree.is_shadowed_at("connect", scope_idx),
            "parameter 'connect' should shadow import inside function"
        );
    }

    #[test]
    fn nested_scopes_shadow_correctly() {
        let src = r#"
import { connect } from 'nats';

async function main() {
    if (true) {
        const connect = () => {};
        const nc = await connect();
    }
    const nc2 = await connect();
}
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // Inside if block: shadowed
        let if_call_offset = src.find("const nc = await connect()").unwrap() + 15;
        let if_scope_idx = scope_tree.scope_containing(if_call_offset);
        assert!(
            scope_tree.is_shadowed_at("connect", if_scope_idx),
            "connect should be shadowed inside if block"
        );

        // Outside if block (nc2): not shadowed
        let outer_call_offset = src.find("const nc2 = await connect()").unwrap() + 16;
        let outer_scope_idx = scope_tree.scope_containing(outer_call_offset);
        assert!(
            !scope_tree.is_shadowed_at("connect", outer_scope_idx),
            "connect should not be shadowed outside if block"
        );
    }

    #[test]
    fn destructured_variable_tracked() {
        let src = r#"
const { connect, other } = someObject;
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        assert!(scope_tree.scopes[0].declarations.contains("connect"));
        assert!(scope_tree.scopes[0].declarations.contains("other"));
    }

    #[test]
    fn renamed_destructure_tracks_local_name() {
        let src = r#"
const { connect: localConnect } = someObject;
"#;
        let tree = parse_ts(src);
        let scope_tree = ScopeTree::build(&tree.root_node(), src.as_bytes());

        // The local name 'localConnect' should be tracked, not 'connect'
        assert!(scope_tree.scopes[0].declarations.contains("localConnect"));
        assert!(!scope_tree.scopes[0].declarations.contains("connect"));
    }
}
