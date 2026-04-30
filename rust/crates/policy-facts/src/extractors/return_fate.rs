//! RETURN_FATE extractor for C.
//!
//! Extracts what happens to function return values at each call site.
//! Classification uses immediate AST context (no dataflow).
//!
//! ## PF-3 Scope
//!
//! - IGNORED: call result discarded
//! - CHECKED: result tested in condition
//! - PROPAGATED: result returned from caller
//! - TRANSFORMED: result passed to another function
//! - STORED: result assigned to variable
//!
//! See `docs/slices/pf-3-return-fate.md` for full rules.

use crate::types::{FateEvidence, FateKind, ReturnFate};

/// Known void functions (C stdlib/POSIX) - do not emit RETURN_FATE for these.
///
/// These functions have returns, but ignoring them is idiomatic C.
/// Focus is on status/result returns indicating success/failure.
const VOID_FUNCTIONS: &[&str] = &[
    // Memory management
    "free",
    // Process termination
    "abort",
    "exit",
    "_Exit",
    "quick_exit",
    // Variadic argument handling
    "va_start",
    "va_end",
    "va_copy",
    // I/O where ignoring return is common
    "perror",
    "puts",
    "putchar",
    "putc",
    "fputc",
    "fputs",
    // printf family (commonly ignored)
    "printf",
    "fprintf",
    "sprintf",
    "snprintf",
    "vprintf",
    "vfprintf",
    "vsprintf",
    "vsnprintf",
    // Memory operations (return value is input pointer)
    "memcpy",
    "memmove",
    "memset",
    // String operations (return value is input pointer)
    "strcpy",
    "strncpy",
    "strcat",
    "strncat",
    // Buffer setup (commonly ignored)
    "setbuf",
    "setvbuf",
];

use std::collections::HashMap;

/// A registry of functions defined in the same file.
/// Used for same-file callee_key resolution.
type FunctionRegistry = HashMap<String, String>; // name -> stable_key

/// Extract RETURN_FATE facts from a C file's AST.
///
/// # Arguments
/// * `tree` - Parsed tree-sitter tree
/// * `source` - Source file bytes
/// * `file_path` - Repo-relative file path
/// * `repo_uid` - Repository identifier
///
/// # Returns
/// Vector of ReturnFate structs found in the file.
///
/// # Callee Key Resolution
/// - Direct calls to functions defined in the same file: resolved to stable key
/// - Direct calls to external functions: callee_key = None
/// - Vtable/pointer calls (e.g., `server->method()`): callee_key = None
///
/// Cross-file resolution requires the symbol table (deferred to PF-3b).
pub fn extract_return_fates(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<ReturnFate> {
    let root = tree.root_node();

    // First pass: build registry of functions defined in this file.
    let mut function_registry = FunctionRegistry::new();
    build_function_registry(&root, source, file_path, repo_uid, &mut function_registry);

    // Second pass: extract fates with callee_key resolution.
    let mut results = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                extract_fates_from_function(&child, source, file_path, repo_uid, &function_registry, &mut results);
            }
            // Handle preprocessor blocks
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                walk_preproc_for_functions(&child, source, file_path, repo_uid, &function_registry, &mut results);
            }
            _ => {}
        }
    }

    results
}

/// Build a registry of function names to stable keys for this file.
fn build_function_registry(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
    registry: &mut FunctionRegistry,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    if let Some(name) = extract_function_name(&declarator, source) {
                        let key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, name);
                        registry.insert(name, key);
                    }
                }
            }
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                build_function_registry(&child, source, file_path, repo_uid, registry);
            }
            _ => {}
        }
    }
}

/// Recursively walk preprocessor blocks for function definitions.
fn walk_preproc_for_functions(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
    function_registry: &FunctionRegistry,
    results: &mut Vec<ReturnFate>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                extract_fates_from_function(&child, source, file_path, repo_uid, function_registry, results);
            }
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                walk_preproc_for_functions(&child, source, file_path, repo_uid, function_registry, results);
            }
            _ => {}
        }
    }
}

/// Extract fates from a single function definition.
fn extract_fates_from_function(
    func_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
    function_registry: &FunctionRegistry,
    results: &mut Vec<ReturnFate>,
) {
    // Extract function name (caller)
    let declarator = match func_node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };
    let caller_name = match extract_function_name(&declarator, source) {
        Some(n) => n,
        None => return,
    };

    // Build caller key
    let caller_key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, caller_name);

    // Get function body
    let body = match func_node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    // Find all call expressions in the body
    find_calls_recursive(
        &body,
        source,
        file_path,
        &caller_name,
        &caller_key,
        function_registry,
        results,
    );
}

/// Extract function name from declarator.
fn extract_function_name(declarator: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut current = *declarator;

    // Unwrap function_declarator and pointer_declarator wrappers
    loop {
        match current.kind() {
            "function_declarator" | "pointer_declarator" => {
                if let Some(inner) = current.child_by_field_name("declarator") {
                    current = inner;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    if current.kind() == "identifier" {
        return Some(current.utf8_text(source).ok()?.to_string());
    }

    // Search children for identifier
    let mut cursor = current.walk();
    for child in current.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child.utf8_text(source).ok()?.to_string());
        }
    }

    None
}

/// Recursively find call expressions in a node tree.
fn find_calls_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    caller_name: &str,
    caller_key: &str,
    function_registry: &FunctionRegistry,
    results: &mut Vec<ReturnFate>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(fate) = classify_call(&child, source, file_path, caller_name, caller_key, function_registry) {
                results.push(fate);
            }
        }
        // Recurse into all children
        find_calls_recursive(&child, source, file_path, caller_name, caller_key, function_registry, results);
    }
}

/// Classify a call expression's return value fate.
///
/// Returns None if the callee is a known void function.
fn classify_call(
    call_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    caller_name: &str,
    caller_key: &str,
    function_registry: &FunctionRegistry,
) -> Option<ReturnFate> {
    // Extract callee name and determine if it's a direct call
    let (callee_name, is_direct_call) = extract_callee_name_and_kind(call_node, source)?;

    // Skip known void functions
    if is_void_function(&callee_name) {
        return None;
    }

    let line = (call_node.start_position().row + 1) as u32;
    let column = call_node.start_position().column as u32;

    // Classify by walking up the AST
    let (fate, evidence) = classify_by_context(call_node, source);

    // Resolve callee_key:
    // - Direct calls to same-file functions: resolved from registry
    // - Direct calls to external functions: None (requires symbol table)
    // - Vtable/pointer calls: None (unresolvable without dataflow)
    let callee_key = if is_direct_call {
        function_registry.get(&callee_name).cloned()
    } else {
        None
    };

    Some(ReturnFate {
        callee_key,
        callee_name,
        caller_key: caller_key.to_string(),
        caller_name: caller_name.to_string(),
        file_path: file_path.to_string(),
        line,
        column,
        fate,
        evidence,
    })
}

/// Extract callee name from call expression (legacy, for external callers).
fn extract_callee_name(call_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    extract_callee_name_and_kind(call_node, source).map(|(name, _)| name)
}

/// Extract callee name and whether it's a direct call.
///
/// Returns (callee_name, is_direct_call):
/// - `is_direct_call = true`: simple identifier call like `func()`
/// - `is_direct_call = false`: vtable/pointer call like `ptr->method()` or `(*fp)()`
fn extract_callee_name_and_kind(call_node: &tree_sitter::Node, source: &[u8]) -> Option<(String, bool)> {
    let callee = call_node.child_by_field_name("function")?;

    match callee.kind() {
        // Direct function call: func()
        "identifier" => Some((callee.utf8_text(source).ok()?.to_string(), true)),
        // Method-like calls: ptr->method or obj.method (vtable, not direct)
        "field_expression" => {
            let field = callee.child_by_field_name("field")?;
            Some((field.utf8_text(source).ok()?.to_string(), false))
        }
        // Calls through pointers: (*func_ptr)() (not direct)
        "parenthesized_expression" | "pointer_expression" => {
            Some((callee.utf8_text(source).ok()?.to_string(), false))
        }
        // Other cases (e.g., subscript_expression for array of function pointers)
        _ => Some((callee.utf8_text(source).ok()?.to_string(), false)),
    }
}

/// Check if function is in the void function list.
fn is_void_function(name: &str) -> bool {
    VOID_FUNCTIONS.contains(&name)
}

/// Classify call by walking up the AST to find immediate context.
///
/// Precedence: assignment > condition > other contexts.
fn classify_by_context(call_node: &tree_sitter::Node, source: &[u8]) -> (FateKind, FateEvidence) {
    let mut current = *call_node;

    while let Some(parent) = current.parent() {
        match parent.kind() {
            // STORED: Assignment takes priority
            "assignment_expression" => {
                return classify_as_stored(&parent, &current, source);
            }

            // STORED: Variable initialization
            "init_declarator" => {
                return classify_as_stored_init(&parent, source);
            }

            // IGNORED: Standalone expression statement
            "expression_statement" => {
                // Check if call is the entire expression
                if is_direct_child(&parent, &current) || is_cast_to_void(&parent, &current, source)
                {
                    let explicit_void_cast = is_cast_to_void(&parent, &current, source);
                    return (
                        FateKind::Ignored,
                        FateEvidence::Ignored { explicit_void_cast },
                    );
                }
            }

            // IGNORED: Explicit void cast
            "cast_expression" => {
                if is_void_cast(&parent, source) {
                    return (
                        FateKind::Ignored,
                        FateEvidence::Ignored {
                            explicit_void_cast: true,
                        },
                    );
                }
            }

            // TRANSFORMED: Argument to another function call
            "argument_list" => {
                if let Some(outer_call) = parent.parent() {
                    if outer_call.kind() == "call_expression" {
                        return classify_as_transformed(&outer_call, &parent, call_node, source);
                    }
                }
            }

            // PROPAGATED: Return statement
            "return_statement" => {
                return classify_as_propagated(&parent, call_node, source);
            }

            // CHECKED: Condition contexts (only if not wrapped in assignment)
            "if_statement" | "while_statement" | "for_statement" | "switch_statement" => {
                // Check if call is in the condition field
                if let Some(cond) = parent.child_by_field_name("condition") {
                    if is_ancestor_of(&cond, call_node) && !has_assignment_ancestor(call_node, &cond)
                    {
                        return classify_as_checked(&parent, call_node, source);
                    }
                }
            }

            // CHECKED: Ternary condition
            "conditional_expression" => {
                if let Some(cond) = parent.child_by_field_name("condition") {
                    if is_ancestor_of(&cond, call_node) && !has_assignment_ancestor(call_node, &cond)
                    {
                        return (
                            FateKind::Checked,
                            FateEvidence::Checked {
                                check_kind: "ternary".to_string(),
                                operator: None,
                                compared_to: None,
                            },
                        );
                    }
                }
            }

            // CHECKED: Binary comparison where call is direct operand
            "binary_expression" => {
                if is_comparison_operator(&parent, source) {
                    if !has_assignment_ancestor(call_node, &parent) {
                        return classify_checked_from_binary(&parent, call_node, source);
                    }
                }
            }

            _ => {}
        }

        current = parent;
    }

    // Default: If we reach here without classification, it's likely ignored
    // (e.g., comma expression, complex macro, etc.)
    (
        FateKind::Ignored,
        FateEvidence::Ignored {
            explicit_void_cast: false,
        },
    )
}

/// Check if child is a direct child of parent (not nested deeper).
fn is_direct_child(parent: &tree_sitter::Node, child: &tree_sitter::Node) -> bool {
    let mut cursor = parent.walk();
    for c in parent.children(&mut cursor) {
        if c.id() == child.id() {
            return true;
        }
    }
    false
}

/// Check if this is a (void)expr cast pattern.
fn is_cast_to_void(
    expr_stmt: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut cursor = expr_stmt.walk();
    for child in expr_stmt.children(&mut cursor) {
        if child.kind() == "cast_expression" {
            if is_void_cast(&child, source) && is_ancestor_of(&child, call_node) {
                return true;
            }
        }
    }
    false
}

/// Check if cast expression is casting to void.
fn is_void_cast(cast_node: &tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(type_node) = cast_node.child_by_field_name("type") {
        if let Ok(text) = type_node.utf8_text(source) {
            return text.trim() == "void";
        }
    }
    false
}

/// Check if ancestor is an ancestor of node.
fn is_ancestor_of(ancestor: &tree_sitter::Node, node: &tree_sitter::Node) -> bool {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.id() == ancestor.id() {
            return true;
        }
        current = parent;
    }
    false
}

/// Check if there's an assignment expression between call_node and stop_node.
fn has_assignment_ancestor(call_node: &tree_sitter::Node, stop_node: &tree_sitter::Node) -> bool {
    let mut current = *call_node;
    while let Some(parent) = current.parent() {
        if parent.id() == stop_node.id() {
            return false;
        }
        if parent.kind() == "assignment_expression" {
            return true;
        }
        current = parent;
    }
    false
}

/// Classify as STORED from assignment expression.
fn classify_as_stored(
    assign_node: &tree_sitter::Node,
    _call_node: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    let variable_name = assign_node
        .child_by_field_name("left")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string());

    // Check if assignment is inside a condition (compound assignment in condition)
    let immediately_checked = is_in_condition_context(assign_node);

    (
        FateKind::Stored,
        FateEvidence::Stored {
            variable_name,
            immediately_checked,
        },
    )
}

/// Check if node is inside a condition context.
fn is_in_condition_context(node: &tree_sitter::Node) -> bool {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "if_statement" | "while_statement" | "for_statement" | "switch_statement" => {
                if let Some(cond) = parent.child_by_field_name("condition") {
                    if is_ancestor_of(&cond, node) {
                        return true;
                    }
                }
                return false;
            }
            "conditional_expression" => {
                if let Some(cond) = parent.child_by_field_name("condition") {
                    if is_ancestor_of(&cond, node) {
                        return true;
                    }
                }
                return false;
            }
            _ => {}
        }
        current = parent;
    }
    false
}

/// Classify as STORED from init declarator.
fn classify_as_stored_init(
    init_node: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    let variable_name = init_node
        .child_by_field_name("declarator")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string());

    (
        FateKind::Stored,
        FateEvidence::Stored {
            variable_name,
            immediately_checked: false,
        },
    )
}

/// Classify as TRANSFORMED (argument to another call).
fn classify_as_transformed(
    outer_call: &tree_sitter::Node,
    arg_list: &tree_sitter::Node,
    inner_call: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    let receiver_name = extract_callee_name(outer_call, source).unwrap_or_else(|| "?".to_string());

    // Find argument index
    let mut argument_index = 0u32;
    let mut cursor = arg_list.walk();
    for child in arg_list.children(&mut cursor) {
        // Skip punctuation
        if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
            continue;
        }
        if is_ancestor_of(&child, inner_call) || child.id() == inner_call.id() {
            break;
        }
        argument_index += 1;
    }

    (
        FateKind::Transformed,
        FateEvidence::Transformed {
            receiver_name,
            argument_index,
        },
    )
}

/// Classify as PROPAGATED from return statement.
fn classify_as_propagated(
    return_node: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    // Check if there's a wrapper call between return and our call
    let mut cursor = return_node.walk();
    for child in return_node.children(&mut cursor) {
        if child.kind() == "call_expression" && child.id() != call_node.id() {
            // There's another call - check if our call is nested in it
            if is_ancestor_of(&child, call_node) {
                let wrapper =
                    extract_callee_name(&child, source).unwrap_or_else(|| "?".to_string());
                return (
                    FateKind::Propagated,
                    FateEvidence::Propagated {
                        wrapped: true,
                        wrapper: Some(wrapper),
                    },
                );
            }
        }
    }

    // Direct return
    (
        FateKind::Propagated,
        FateEvidence::Propagated {
            wrapped: false,
            wrapper: None,
        },
    )
}

/// Check if binary expression is a comparison.
fn is_comparison_operator(binary_node: &tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(op) = binary_node.child_by_field_name("operator") {
        if let Ok(text) = op.utf8_text(source) {
            return matches!(text, "==" | "!=" | "<" | ">" | "<=" | ">=");
        }
    }
    false
}

/// Classify as CHECKED from binary expression.
fn classify_as_checked(
    parent: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    let check_kind = match parent.kind() {
        "if_statement" => "if",
        "while_statement" => "while",
        "for_statement" => "for",
        "switch_statement" => "switch",
        _ => "unknown",
    }
    .to_string();

    // Try to extract comparison details from condition
    if let Some(cond) = parent.child_by_field_name("condition") {
        if let Some((op, compared)) = extract_comparison_details(&cond, call_node, source) {
            return (
                FateKind::Checked,
                FateEvidence::Checked {
                    check_kind,
                    operator: Some(op),
                    compared_to: Some(compared),
                },
            );
        }
    }

    (
        FateKind::Checked,
        FateEvidence::Checked {
            check_kind,
            operator: None,
            compared_to: None,
        },
    )
}

/// Classify as CHECKED from direct binary comparison.
fn classify_checked_from_binary(
    binary_node: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> (FateKind, FateEvidence) {
    // Find enclosing statement type for check_kind
    let check_kind = find_enclosing_check_kind(binary_node);

    if let Some((op, compared)) = extract_comparison_from_binary(binary_node, call_node, source) {
        return (
            FateKind::Checked,
            FateEvidence::Checked {
                check_kind,
                operator: Some(op),
                compared_to: Some(compared),
            },
        );
    }

    (
        FateKind::Checked,
        FateEvidence::Checked {
            check_kind,
            operator: None,
            compared_to: None,
        },
    )
}

/// Find enclosing check kind (if/while/for/switch/ternary).
fn find_enclosing_check_kind(node: &tree_sitter::Node) -> String {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "if_statement" => return "if".to_string(),
            "while_statement" => return "while".to_string(),
            "for_statement" => return "for".to_string(),
            "switch_statement" => return "switch".to_string(),
            "conditional_expression" => return "ternary".to_string(),
            _ => {}
        }
        current = parent;
    }
    "expression".to_string()
}

/// Extract comparison operator and compared-to value from condition.
fn extract_comparison_details(
    cond: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, String)> {
    // Walk condition to find binary expression containing our call
    find_comparison_in_subtree(cond, call_node, source)
}

/// Find comparison in subtree containing call_node.
fn find_comparison_in_subtree(
    node: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, String)> {
    if node.kind() == "binary_expression" && is_comparison_operator(node, source) {
        if is_ancestor_of(node, call_node) {
            return extract_comparison_from_binary(node, call_node, source);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(result) = find_comparison_in_subtree(&child, call_node, source) {
            return Some(result);
        }
    }

    None
}

/// Extract comparison operator and compared-to value from binary expression.
fn extract_comparison_from_binary(
    binary_node: &tree_sitter::Node,
    call_node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, String)> {
    let op = binary_node.child_by_field_name("operator")?;
    let op_text = op.utf8_text(source).ok()?.to_string();

    let left = binary_node.child_by_field_name("left")?;
    let right = binary_node.child_by_field_name("right")?;

    // Determine which side has the call and which has the compared value
    let compared_node = if is_ancestor_of(&left, call_node) || left.id() == call_node.id() {
        right
    } else {
        left
    };

    let compared_text = compared_node.utf8_text(source).ok()?.trim().to_string();

    Some((op_text, compared_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_c(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        parser.parse(source, None).unwrap()
    }

    // =========================================================================
    // IGNORED tests
    // =========================================================================

    #[test]
    fn test_ignored_standalone_call() {
        let source = r#"
void test(void) {
    do_something();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].callee_name, "do_something");
        assert_eq!(fates[0].fate, FateKind::Ignored);

        if let FateEvidence::Ignored { explicit_void_cast } = &fates[0].evidence {
            assert!(!explicit_void_cast);
        } else {
            panic!("expected Ignored evidence");
        }
    }

    #[test]
    fn test_ignored_explicit_void_cast() {
        let source = r#"
void test(void) {
    (void)get_result();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Ignored);

        if let FateEvidence::Ignored { explicit_void_cast } = &fates[0].evidence {
            assert!(explicit_void_cast);
        } else {
            panic!("expected Ignored evidence");
        }
    }

    #[test]
    fn test_void_function_skipped() {
        let source = r#"
void test(void) {
    printf("hello");
    free(ptr);
    memcpy(dst, src, n);
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        // All these are void functions - should produce no fates
        assert_eq!(fates.len(), 0);
    }

    // =========================================================================
    // CHECKED tests
    // =========================================================================

    #[test]
    fn test_checked_if_condition() {
        let source = r#"
void test(void) {
    if (get_status() == OK) {
        // ...
    }
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Checked);

        if let FateEvidence::Checked {
            check_kind,
            operator,
            compared_to,
        } = &fates[0].evidence
        {
            assert_eq!(check_kind, "if");
            assert_eq!(operator.as_deref(), Some("=="));
            assert_eq!(compared_to.as_deref(), Some("OK"));
        } else {
            panic!("expected Checked evidence");
        }
    }

    #[test]
    fn test_checked_while_condition() {
        let source = r#"
void test(void) {
    while (read_byte() != EOF) {
        // ...
    }
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Checked);

        if let FateEvidence::Checked {
            check_kind,
            operator,
            compared_to,
        } = &fates[0].evidence
        {
            assert_eq!(check_kind, "while");
            assert_eq!(operator.as_deref(), Some("!="));
            assert_eq!(compared_to.as_deref(), Some("EOF"));
        } else {
            panic!("expected Checked evidence");
        }
    }

    // =========================================================================
    // PROPAGATED tests
    // =========================================================================

    #[test]
    fn test_propagated_direct() {
        let source = r#"
int test(void) {
    return get_value();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Propagated);

        if let FateEvidence::Propagated { wrapped, wrapper } = &fates[0].evidence {
            assert!(!wrapped);
            assert!(wrapper.is_none());
        } else {
            panic!("expected Propagated evidence");
        }
    }

    #[test]
    fn test_propagated_wrapped() {
        let source = r#"
int test(void) {
    return transform(get_value());
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        // Both transform and get_value are calls
        assert_eq!(fates.len(), 2);

        // Find get_value fate (inner call)
        let inner = fates.iter().find(|f| f.callee_name == "get_value").unwrap();
        assert_eq!(inner.fate, FateKind::Transformed); // It's an argument to transform

        // Find transform fate (outer call)
        let outer = fates.iter().find(|f| f.callee_name == "transform").unwrap();
        assert_eq!(outer.fate, FateKind::Propagated);
    }

    // =========================================================================
    // TRANSFORMED tests
    // =========================================================================

    #[test]
    fn test_transformed_as_argument() {
        let source = r#"
void test(void) {
    process(get_data());
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        // Two calls: process (IGNORED) and get_data (TRANSFORMED)
        assert_eq!(fates.len(), 2);

        let get_data = fates.iter().find(|f| f.callee_name == "get_data").unwrap();
        assert_eq!(get_data.fate, FateKind::Transformed);

        if let FateEvidence::Transformed {
            receiver_name,
            argument_index,
        } = &get_data.evidence
        {
            assert_eq!(receiver_name, "process");
            assert_eq!(*argument_index, 0);
        } else {
            panic!("expected Transformed evidence");
        }
    }

    #[test]
    fn test_transformed_second_argument() {
        let source = r#"
void test(void) {
    send(dest, get_data(), flags);
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        let get_data = fates.iter().find(|f| f.callee_name == "get_data").unwrap();
        assert_eq!(get_data.fate, FateKind::Transformed);

        if let FateEvidence::Transformed { argument_index, .. } = &get_data.evidence {
            assert_eq!(*argument_index, 1);
        } else {
            panic!("expected Transformed evidence");
        }
    }

    // =========================================================================
    // STORED tests
    // =========================================================================

    #[test]
    fn test_stored_simple_assignment() {
        let source = r#"
void test(void) {
    int result = get_value();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Stored);

        if let FateEvidence::Stored {
            variable_name,
            immediately_checked,
        } = &fates[0].evidence
        {
            assert_eq!(variable_name, "result");
            assert!(!immediately_checked);
        } else {
            panic!("expected Stored evidence");
        }
    }

    #[test]
    fn test_stored_reassignment() {
        let source = r#"
void test(void) {
    int x;
    x = get_value();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Stored);

        if let FateEvidence::Stored {
            variable_name,
            immediately_checked,
        } = &fates[0].evidence
        {
            assert_eq!(variable_name, "x");
            assert!(!immediately_checked);
        } else {
            panic!("expected Stored evidence");
        }
    }

    #[test]
    fn test_stored_compound_assignment_in_condition() {
        // Critical test: if ((result = func()) != OK)
        // Should be STORED with immediately_checked = true, NOT CHECKED
        let source = r#"
void test(void) {
    int result;
    if ((result = get_status()) != OK) {
        // handle error
    }
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].fate, FateKind::Stored);

        if let FateEvidence::Stored {
            variable_name,
            immediately_checked,
        } = &fates[0].evidence
        {
            assert_eq!(variable_name, "result");
            assert!(immediately_checked, "compound assignment in condition should have immediately_checked = true");
        } else {
            panic!("expected Stored evidence");
        }
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn test_method_call_through_pointer() {
        let source = r#"
void test(server_t *server) {
    server->install_update();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].callee_name, "install_update");
        assert_eq!(fates[0].callee_key, None); // Unresolved
        assert_eq!(fates[0].fate, FateKind::Ignored);
    }

    #[test]
    fn test_multiple_calls_same_line() {
        let source = r#"
void test(void) {
    int a = first(), b = second();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(fates.len(), 2);

        // Both should be STORED
        for fate in &fates {
            assert_eq!(fate.fate, FateKind::Stored);
        }

        // Should have different columns
        let columns: Vec<_> = fates.iter().map(|f| f.column).collect();
        assert!(
            columns[0] != columns[1],
            "same-line calls should have different columns"
        );
    }

    #[test]
    fn test_nested_call_in_condition() {
        // Inner call is TRANSFORMED, not CHECKED
        let source = r#"
void test(void) {
    if (validate(get_input()) == true) {
        // ...
    }
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "test");

        let get_input = fates.iter().find(|f| f.callee_name == "get_input").unwrap();
        assert_eq!(
            get_input.fate,
            FateKind::Transformed,
            "inner call in condition should be TRANSFORMED, not CHECKED"
        );
    }

    // =========================================================================
    // Callee key resolution tests
    // =========================================================================

    #[test]
    fn test_callee_key_resolved_for_same_file_function() {
        // helper() is defined in the same file, so callee_key should be resolved
        let source = r#"
int helper(void) { return 42; }

void test(void) {
    int x = helper();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "myrepo");

        let helper_call = fates.iter().find(|f| f.callee_name == "helper").unwrap();
        assert_eq!(
            helper_call.callee_key,
            Some("myrepo:test.c#helper:SYMBOL:FUNCTION".to_string()),
            "same-file function call should have resolved callee_key"
        );
    }

    #[test]
    fn test_callee_key_none_for_external_function() {
        // external_func() is NOT defined in this file, so callee_key should be None
        let source = r#"
void test(void) {
    int x = external_func();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "myrepo");

        let ext_call = fates.iter().find(|f| f.callee_name == "external_func").unwrap();
        assert_eq!(
            ext_call.callee_key,
            None,
            "external function call should have callee_key = None"
        );
    }

    #[test]
    fn test_callee_key_none_for_vtable_call() {
        // ptr->method() is a vtable call, callee_key should be None
        let source = r#"
void test(struct server *s) {
    s->do_action();
}
"#;
        let tree = parse_c(source);
        let fates = extract_return_fates(&tree, source.as_bytes(), "test.c", "myrepo");

        let vtable_call = fates.iter().find(|f| f.callee_name == "do_action").unwrap();
        assert_eq!(
            vtable_call.callee_key,
            None,
            "vtable call should have callee_key = None"
        );
    }
}
