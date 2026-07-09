//! STATUS_MAPPING extractor for C.
//!
//! Extracts status/error translation functions from C source files.
//!
//! ## Extraction Rules
//!
//! A function is a STATUS_MAPPING candidate if:
//! 1. Return type is status/result-like (naming pattern)
//! 2. Body contains a switch statement on any qualifying parameter
//!    (supports direct parameter reference or pointer dereference like `*param`)
//! 3. Source discriminant type is either status-like or numeric scalar
//!
//! See `docs/slices/pf-1-status-mapping.md` for full rules.

use crate::types::{CaseMapping, StatusMapping};

/// Status/result-like type name suffixes.
/// Note: _t is a general C typedef convention, not specific to status types.
/// We use more specific suffixes to reduce false positives.
const STATUS_SUFFIXES: &[&str] = &[
    "_res_t",
    "_result_t",
    "_status_t",
    "_code_t",
    "_error_t",
    "_ret_t",
    "_res",
    "_result",
    "_status",
    "_code",
    "_error",
    "_ret",
    "_e", // enum suffix in some codebases
];

/// Status/result-like type name substrings (case-insensitive check).
const STATUS_SUBSTRINGS: &[&str] = &["result", "error", "status", "code"];

/// Function name patterns that boost mapping confidence (optional).
#[allow(dead_code)]
const MAPPING_NAME_PATTERNS: &[&str] = &["map", "convert", "translate", "to_", "_to_"];

/// Extract STATUS_MAPPING facts from a C file's AST.
///
/// # Arguments
/// * `tree` - Parsed tree-sitter tree
/// * `source` - Source file bytes
/// * `file_path` - Repo-relative file path
/// * `repo_uid` - Repository identifier
///
/// # Returns
/// Vector of StatusMapping structs found in the file.
pub fn extract_status_mappings(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<StatusMapping> {
    let mut results = Vec::new();
    let root = tree.root_node();

    // Walk top-level declarations
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(mapping) =
                    try_extract_status_mapping(&child, source, file_path, repo_uid)
                {
                    results.push(mapping);
                }
            }
            // Handle preprocessor blocks
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                walk_preproc_for_functions(&child, source, file_path, repo_uid, &mut results);
            }
            _ => {}
        }
    }

    results
}

/// Recursively walk preprocessor blocks for function definitions.
fn walk_preproc_for_functions(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
    results: &mut Vec<StatusMapping>,
) {
    // PERSIST-RECURSION-1: iterative descent through preprocessor blocks (was
    // recursive on preproc-nesting depth). Order and emitted mappings unchanged.
    super::walk::for_each_preproc_function(*node, |func_node| {
        if let Some(mapping) = try_extract_status_mapping(&func_node, source, file_path, repo_uid) {
            results.push(mapping);
        }
    });
}

/// Try to extract a StatusMapping from a function definition.
///
/// Returns None if the function does not match the STATUS_MAPPING pattern.
fn try_extract_status_mapping(
    func_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Option<StatusMapping> {
    // 1. Extract return type and check if status-like
    let return_type = extract_return_type(func_node, source)?;
    if !is_status_like_type(&return_type) {
        return None;
    }

    // 2. Extract function name
    let declarator = func_node.child_by_field_name("declarator")?;
    let function_name = extract_function_name(&declarator, source)?;

    // 3. Extract all parameters
    let params = extract_all_parameters(&declarator, source);
    if params.is_empty() {
        return None;
    }

    // 4. Find switch statement in body that switches on any parameter
    //    (including pointer dereferences like *param)
    let body = func_node.child_by_field_name("body")?;
    let (switch_node, matched_param_type) = find_switch_on_any_param(&body, &params, source)?;

    // 5. Extract case mappings from switch
    let (mappings, mut default_output) = extract_case_mappings(&switch_node, source);

    // 6. If no default inside switch, look for return statement after switch in function body
    if default_output.is_none() {
        default_output = find_fallback_return_after_switch(&body, &switch_node, source);
    }

    // PF-1 contract: a STATUS_MAPPING must have at least one explicit case-to-output mapping.
    // A fallback return alone (default_output without explicit mappings) indicates a
    // procedural switch dispatcher with side effects, not a pure status translation function.
    // Examples of false positives rejected by this rule:
    // - channel_post_method: switch(method) { case A: do_x(); break; ... } return result;
    // - disk_ioctl: switch(cmd) { case READ: read_disk(); break; ... } return err;
    if mappings.is_empty() {
        return None;
    }

    // 7. Build result
    let line_start = (func_node.start_position().row + 1) as u32;
    let line_end = (func_node.end_position().row + 1) as u32;

    // Use canonical stable key format with uppercase subtype
    let symbol_key = format!(
        "{}:{}#{}:SYMBOL:FUNCTION",
        repo_uid, file_path, function_name
    );

    Some(StatusMapping {
        symbol_key,
        function_name,
        file_path: file_path.to_string(),
        line_start,
        line_end,
        source_type: matched_param_type,
        target_type: return_type,
        mappings,
        default_output,
    })
}

/// Extract the return type from a function definition.
fn extract_return_type(func_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Return type is typically a type specifier before the declarator
    // Tree structure: function_definition -> [type_specifier|primitive_type|...] declarator body
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "primitive_type" | "sized_type_specifier" => {
                return Some(child.utf8_text(source).ok()?.trim().to_string());
            }
            // Handle struct/enum return types
            "struct_specifier" | "enum_specifier" => {
                // Extract the type name from struct/enum specifier
                let mut inner_cursor = child.walk();
                for inner_child in child.children(&mut inner_cursor) {
                    if inner_child.kind() == "type_identifier" {
                        return Some(inner_child.utf8_text(source).ok()?.trim().to_string());
                    }
                }
            }
            "function_declarator" | "declarator" | "pointer_declarator" => {
                // We've passed the return type, stop
                break;
            }
            _ => {}
        }
    }
    None
}

/// Check if a type name looks like a status/result type.
fn is_status_like_type(type_name: &str) -> bool {
    let lower = type_name.to_lowercase();

    // Check suffixes
    for suffix in STATUS_SUFFIXES {
        if type_name.ends_with(suffix) {
            return true;
        }
    }

    // Check substrings (case-insensitive)
    for substr in STATUS_SUBSTRINGS {
        if lower.contains(substr) {
            return true;
        }
    }

    false
}

/// Extract function name from declarator.
fn extract_function_name(declarator: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut current = *declarator;

    // Unwrap function_declarator and pointer_declarator wrappers
    while let "function_declarator" | "pointer_declarator" = current.kind() {
        if let Some(inner) = current.child_by_field_name("declarator") {
            current = inner;
        } else {
            break;
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

/// Parameter info: name, type, and whether it's a pointer.
#[derive(Debug, Clone)]
struct ParamInfo {
    name: String,
    type_name: String,
    is_pointer: bool,
}

/// Extract all parameters from function declarator.
fn extract_all_parameters(declarator: &tree_sitter::Node, source: &[u8]) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    extract_params_from_node(declarator, source, &mut params);
    params
}

/// Extract parameters from a declarator node.
///
/// PERSIST-RECURSION-1: iterative descent through nested `pointer_declarator`s
/// (was recursive on pointer nesting → could overflow on a pathologically deep
/// function-pointer declarator). A `function_declarator` yields its parameters
/// and is terminal (its subtree is not descended); a `pointer_declarator` is
/// descended. Reverse-pushing children preserves document order, so `params` is
/// byte-for-byte unchanged.
fn extract_params_from_node(node: &tree_sitter::Node, source: &[u8], params: &mut Vec<ParamInfo>) {
    let mut stack: Vec<tree_sitter::Node> = vec![*node];
    while let Some(n) = stack.pop() {
        // Direct function_declarator: extract its parameters; terminal.
        if n.kind() == "function_declarator" {
            if let Some(param_list) = n.child_by_field_name("parameters") {
                let mut cursor = param_list.walk();
                for child in param_list.children(&mut cursor) {
                    if child.kind() == "parameter_declaration" {
                        if let Some(info) = extract_param_info(&child, source) {
                            params.push(info);
                        }
                    }
                }
            }
            continue;
        }

        // Otherwise scan children: a function_declarator child is extracted (not
        // descended), a pointer_declarator child is descended — the exact
        // selective recursion of the prior form. Reverse-push preserves order.
        let mut cursor = n.walk();
        let children: Vec<tree_sitter::Node> = n.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            if matches!(child.kind(), "function_declarator" | "pointer_declarator") {
                stack.push(child);
            }
        }
    }
}

/// Extract parameter info (name, type, is_pointer) from parameter_declaration.
fn extract_param_info(param: &tree_sitter::Node, source: &[u8]) -> Option<ParamInfo> {
    let mut type_name = String::new();
    let mut param_name = String::new();
    let mut is_pointer = false;

    let mut cursor = param.walk();
    for child in param.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "primitive_type" | "sized_type_specifier" => {
                type_name = child.utf8_text(source).ok()?.trim().to_string();
            }
            "identifier" => {
                param_name = child.utf8_text(source).ok()?.to_string();
            }
            "pointer_declarator" => {
                is_pointer = true;
                // Extract identifier from pointer declarator
                if let Some(name) = find_identifier_in_declarator(&child, source) {
                    param_name = name;
                }
            }
            // Handle struct/enum parameter types
            "struct_specifier" | "enum_specifier" => {
                let mut inner_cursor = child.walk();
                for inner_child in child.children(&mut inner_cursor) {
                    if inner_child.kind() == "type_identifier" {
                        type_name = inner_child.utf8_text(source).ok()?.trim().to_string();
                    }
                }
            }
            _ => {}
        }
    }

    if param_name.is_empty() || type_name.is_empty() {
        return None;
    }

    Some(ParamInfo {
        name: param_name,
        type_name,
        is_pointer,
    })
}

/// Find identifier in a declarator node.
///
/// PERSIST-RECURSION-1: iterative pre-order find-first (was recursive on
/// declarator subtree depth). Returns the first `identifier` in pre-order — the
/// node itself, then children left-to-right — exactly as the recursive form.
/// (On valid source an identifier is always valid UTF-8; the recursive form's
/// `?` short-circuit and this one's skip-on-utf8-error can only differ on
/// invalid-UTF-8 input, which parsed source never contains.)
fn find_identifier_in_declarator(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut found: Option<String> = None;
    super::walk::visit_preorder(*node, |n| {
        if n.kind() == "identifier" {
            if let Ok(text) = n.utf8_text(source) {
                found = Some(text.to_string());
                return std::ops::ControlFlow::Break(());
            }
        }
        std::ops::ControlFlow::Continue(())
    });
    found
}

/// Find a switch statement that switches on any parameter (including pointer dereference).
/// Returns the switch node and the matched parameter's type.
fn find_switch_on_any_param<'a>(
    body: &'a tree_sitter::Node,
    params: &[ParamInfo],
    source: &[u8],
) -> Option<(tree_sitter::Node<'a>, String)> {
    // Look for top-level switch in compound_statement
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "switch_statement" {
            // Check if condition matches any parameter
            if let Some(condition) = child.child_by_field_name("condition") {
                if let Some(matched_type) =
                    switch_condition_matches_any_param(&condition, params, source)
                {
                    return Some((child, matched_type));
                }
            }
        }
    }
    None
}

/// Check if switch condition matches any parameter.
/// Handles:
/// - Direct identifier: switch (param)
/// - Pointer dereference: switch (*param)
/// - Cast expression: switch ((type)param) or switch ((type)*param)
///
/// Returns the matched parameter's underlying type if found.
fn switch_condition_matches_any_param(
    condition: &tree_sitter::Node,
    params: &[ParamInfo],
    source: &[u8],
) -> Option<String> {
    // The condition is typically a parenthesized_expression
    let inner: Option<tree_sitter::Node> = if condition.kind() == "parenthesized_expression" {
        // Get the inner expression
        let mut cursor = condition.walk();
        let mut found: Option<tree_sitter::Node> = None;
        for c in condition.children(&mut cursor) {
            if c.kind() != "(" && c.kind() != ")" {
                found = Some(c);
                break;
            }
        }
        found
    } else {
        Some(*condition)
    };

    let expr = inner?;

    // Try to extract the parameter name and whether it's dereferenced
    let (param_name, is_deref) = extract_discriminant_info(&expr, source)?;

    // Find matching parameter
    for param in params {
        if param.name == param_name {
            // If the parameter is a pointer and we're dereferencing it,
            // the discriminant type is the pointed-to type
            if is_deref && param.is_pointer {
                // For pointer params like `long *http_response_code`,
                // the discriminant type is `long`
                return Some(param.type_name.clone());
            } else if !is_deref && !param.is_pointer {
                // Direct non-pointer parameter
                return Some(param.type_name.clone());
            }
            // Also allow direct use of pointer types (unusual but valid)
            if !is_deref && param.is_pointer {
                return Some(format!("{} *", param.type_name));
            }
        }
    }

    None
}

/// Extract discriminant info from a switch condition expression.
/// Returns (parameter_name, is_dereferenced).
fn extract_discriminant_info(expr: &tree_sitter::Node, source: &[u8]) -> Option<(String, bool)> {
    // PERSIST-RECURSION-1: iterative unwrap of nested `cast_expression`s (was
    // tail-recursive on cast nesting → could overflow on `switch ((int)(int)…x)`).
    // The identifier / pointer_expression arms are terminal, exactly as before.
    let mut current = *expr;
    loop {
        match current.kind() {
            // Direct identifier: switch (param)
            "identifier" => {
                let name = current.utf8_text(source).ok()?.to_string();
                return Some((name, false));
            }
            // Pointer dereference: switch (*param)
            "pointer_expression" => {
                // Find the identifier being dereferenced
                let mut cursor = current.walk();
                for child in current.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = child.utf8_text(source).ok()?.to_string();
                        return Some((name, true));
                    }
                }
                return None;
            }
            // Cast expression: switch ((type)expr) — unwrap and continue.
            "cast_expression" => match current.child_by_field_name("value") {
                Some(value) => current = value,
                None => return None,
            },
            _ => return None,
        }
    }
}

/// Extract case mappings and default from a switch statement.
fn extract_case_mappings(
    switch_node: &tree_sitter::Node,
    source: &[u8],
) -> (Vec<CaseMapping>, Option<String>) {
    let mut mappings = Vec::new();
    let mut default_output = None;
    let mut current_inputs: Vec<String> = Vec::new();

    // Find the switch body (compound_statement)
    let body = match switch_node.child_by_field_name("body") {
        Some(b) => b,
        None => return (mappings, default_output),
    };

    // Walk the switch body, including preprocessor-wrapped nodes
    walk_switch_body_for_cases(
        &body,
        source,
        &mut current_inputs,
        &mut mappings,
        &mut default_output,
    );

    (mappings, default_output)
}

/// Recursively walk switch body nodes, handling preprocessor blocks.
///
/// Preprocessor conditionals like `#if LIBCURL_VERSION >= X` can wrap
/// case labels inside switch statements. We must walk into these nodes
/// to extract all case mappings correctly.
fn walk_switch_body_for_cases(
    node: &tree_sitter::Node,
    source: &[u8],
    current_inputs: &mut Vec<String>,
    mappings: &mut Vec<CaseMapping>,
    default_output: &mut Option<String>,
) {
    // PERSIST-RECURSION-1: iterative descent through preprocessor blocks (was
    // recursive on preproc-nesting depth). Case / default / return nodes are
    // acted on in document order and only preproc blocks are descended into, so
    // the accumulated case mappings and default output are byte-for-byte unchanged.
    let mut stack: Vec<tree_sitter::Node> = Vec::new();
    push_switch_frames(*node, &mut stack);
    while let Some(child) = stack.pop() {
        match child.kind() {
            "case_statement" => {
                // Check if this is actually a default case (starts with "default" keyword)
                if is_default_case(&child, source) {
                    // Handle as default
                    if let Some(return_expr) = find_return_in_case(&child, source) {
                        *default_output = Some(return_expr);
                    }
                } else {
                    // Regular case
                    if let Some(label) = extract_case_label(&child, source) {
                        current_inputs.push(label);
                    }

                    // Check if this case has a return (ends the fallthrough chain)
                    if let Some(return_expr) = find_return_in_case(&child, source) {
                        if !current_inputs.is_empty() {
                            mappings.push(CaseMapping {
                                inputs: current_inputs.clone(),
                                output: return_expr,
                            });
                            current_inputs.clear();
                        }
                    }
                }
            }
            "default" | "default_statement" => {
                // Handle default label - look for return in the following statements
                if let Some(return_expr) = find_return_in_default(&child, source) {
                    *default_output = Some(return_expr);
                }
            }
            "return_statement" => {
                // Return statement at this level ends the current case group
                if let Some(return_expr) = extract_return_expression(&child, source) {
                    if !current_inputs.is_empty() {
                        mappings.push(CaseMapping {
                            inputs: current_inputs.clone(),
                            output: return_expr,
                        });
                        current_inputs.clear();
                    } else if default_output.is_none() {
                        // Standalone return after all cases = default
                        *default_output = Some(return_expr);
                    }
                }
            }
            // Descend into preprocessor conditional blocks
            "preproc_if" | "preproc_ifdef" | "preproc_ifndef" | "preproc_else" | "preproc_elif" => {
                push_switch_frames(child, &mut stack);
            }
            _ => {}
        }
    }
}

/// Push a frame node's case / default / return / preprocessor children — the
/// only kinds `walk_switch_body_for_cases` acts on or descends into — in reverse
/// document order (so the left-most is popped first, preserving document order).
fn push_switch_frames<'a>(node: tree_sitter::Node<'a>, stack: &mut Vec<tree_sitter::Node<'a>>) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        match child.kind() {
            "case_statement" | "default" | "default_statement" | "return_statement"
            | "preproc_if" | "preproc_ifdef" | "preproc_ifndef" | "preproc_else"
            | "preproc_elif" => stack.push(child),
            _ => {}
        }
    }
}

/// Check if a case_statement is actually a default case.
fn is_default_case(case_node: &tree_sitter::Node, source: &[u8]) -> bool {
    // tree-sitter-c represents `default:` as a case_statement with first child "default"
    let mut cursor = case_node.walk();
    for child in case_node.children(&mut cursor) {
        if child.kind() == "default" {
            return true;
        }
        // Also check text content
        if let Ok(text) = child.utf8_text(source) {
            if text.trim() == "default" {
                return true;
            }
        }
    }
    false
}

/// Find a return statement after the switch in the function body.
/// This handles the pattern:
/// ```c
/// switch (x) { ... }
/// return DEFAULT_VALUE;  // fallback after switch
/// ```
fn find_fallback_return_after_switch(
    body: &tree_sitter::Node,
    switch_node: &tree_sitter::Node,
    source: &[u8],
) -> Option<String> {
    let switch_end_row = switch_node.end_position().row;

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        // Only look at statements after the switch
        if child.start_position().row > switch_end_row && child.kind() == "return_statement" {
            return extract_return_expression(&child, source);
        }
    }
    None
}

/// Extract the case label from a case_statement.
fn extract_case_label(case_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // case_statement has a "value" field containing the label
    if let Some(value) = case_node.child_by_field_name("value") {
        return Some(normalize_expression(value.utf8_text(source).ok()?.trim()));
    }

    // Fallback: look for first meaningful child
    let mut cursor = case_node.walk();
    for child in case_node.children(&mut cursor) {
        if child.kind() != "case" && child.kind() != ":" {
            let text = child.utf8_text(source).ok()?.trim();
            if !text.is_empty() && text != ":" {
                return Some(normalize_expression(text));
            }
        }
    }

    None
}

/// Find return statement within a case body.
fn find_return_in_case(case_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = case_node.walk();
    for child in case_node.children(&mut cursor) {
        if child.kind() == "return_statement" {
            return extract_return_expression(&child, source);
        }
        // Handle compound_statement inside case
        if child.kind() == "compound_statement" {
            if let Some(expr) = find_return_in_compound(&child, source) {
                return Some(expr);
            }
        }
    }
    None
}

/// Find return in compound statement.
fn find_return_in_compound(compound: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = compound.walk();
    for child in compound.children(&mut cursor) {
        if child.kind() == "return_statement" {
            return extract_return_expression(&child, source);
        }
    }
    None
}

/// Find return statement after default label.
fn find_return_in_default(default_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Default might contain the return directly or it might be a sibling
    let mut cursor = default_node.walk();
    for child in default_node.children(&mut cursor) {
        if child.kind() == "return_statement" {
            return extract_return_expression(&child, source);
        }
    }
    None
}

/// Extract the return expression from a return_statement.
fn extract_return_expression(return_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = return_node.walk();
    for child in return_node.children(&mut cursor) {
        // Skip "return" keyword and ";"
        if child.kind() != "return" && child.kind() != ";" {
            let text = child.utf8_text(source).ok()?.trim();
            if !text.is_empty() {
                return Some(normalize_expression(text));
            }
        }
    }
    None
}

/// Normalize an expression: trim whitespace, collapse internal whitespace.
fn normalize_expression(expr: &str) -> String {
    expr.split_whitespace().collect::<Vec<_>>().join(" ")
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

    #[test]
    fn test_enum_to_enum_mapping() {
        let source = r#"
typedef enum { A, B, C } input_code_t;
typedef enum { X, Y, Z } output_status_t;

output_status_t map_value(input_code_t val) {
    switch (val) {
    case A:
        return X;
    case B:
        return Y;
    case C:
        return Z;
    }
    return X;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.function_name, "map_value");
        assert_eq!(m.source_type, "input_code_t");
        assert_eq!(m.target_type, "output_status_t");
        assert_eq!(m.mappings.len(), 3);
        assert_eq!(m.mappings[0].inputs, vec!["A"]);
        assert_eq!(m.mappings[0].output, "X");
        assert_eq!(m.default_output, Some("X".to_string()));
    }

    #[test]
    fn test_fallthrough_case_grouping() {
        let source = r#"
result_t translate(code_t c) {
    switch (c) {
    case CODE_A:
    case CODE_B:
    case CODE_C:
        return RESULT_OK;
    case CODE_D:
        return RESULT_ERR;
    }
    return RESULT_UNKNOWN;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.mappings.len(), 2);

        // First mapping should have 3 inputs (fallthrough)
        assert_eq!(m.mappings[0].inputs, vec!["CODE_A", "CODE_B", "CODE_C"]);
        assert_eq!(m.mappings[0].output, "RESULT_OK");

        // Second mapping has single input
        assert_eq!(m.mappings[1].inputs, vec!["CODE_D"]);
        assert_eq!(m.mappings[1].output, "RESULT_ERR");

        assert_eq!(m.default_output, Some("RESULT_UNKNOWN".to_string()));
    }

    #[test]
    fn test_numeric_discriminant() {
        let source = r#"
error_code_t map_http_code(long code) {
    switch (code) {
    case 200:
        return ERR_OK;
    case 401:
    case 403:
        return ERR_AUTH;
    case 404:
        return ERR_NOT_FOUND;
    case 500:
        return ERR_SERVER;
    }
    return ERR_UNKNOWN;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.function_name, "map_http_code");
        assert_eq!(m.source_type, "long");
        assert_eq!(m.target_type, "error_code_t");

        assert_eq!(m.mappings.len(), 4);
        assert_eq!(m.mappings[0].inputs, vec!["200"]);
        assert_eq!(m.mappings[0].output, "ERR_OK");
        assert_eq!(m.mappings[1].inputs, vec!["401", "403"]);
        assert_eq!(m.mappings[1].output, "ERR_AUTH");
    }

    #[test]
    fn test_expression_return_preserved() {
        let source = r#"
status_t convert(input_t x) {
    switch (x) {
    case A:
        return (status_t)(FOO | BAR);
    case B:
        return STATUS_OK;
    }
    return STATUS_ERR;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.mappings.len(), 2);
        // Expression is preserved as normalized text
        assert_eq!(m.mappings[0].output, "(status_t)(FOO | BAR)");
        assert_eq!(m.mappings[1].output, "STATUS_OK");
    }

    #[test]
    fn test_non_status_function_ignored() {
        let source = r#"
int add(int a, int b) {
    return a + b;
}

void process(int x) {
    switch (x) {
    case 1:
        printf("one");
        break;
    }
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        // Neither function matches: add() has no switch, process() has void return
        assert_eq!(mappings.len(), 0);
    }

    #[test]
    fn test_default_arm_extraction() {
        let source = r#"
result_t translate(code_t c) {
    switch (c) {
    case CODE_A:
        return RESULT_A;
    default:
        return RESULT_DEFAULT;
    }
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.mappings.len(), 1);
        assert_eq!(m.default_output, Some("RESULT_DEFAULT".to_string()));
    }

    #[test]
    fn test_status_like_type_detection() {
        // Suffix patterns
        assert!(is_status_like_type("channel_op_res_t"));
        assert!(is_status_like_type("server_op_res_t"));
        assert!(is_status_like_type("error_code_t"));
        assert!(is_status_like_type("my_result"));
        assert!(is_status_like_type("SomeStatus"));

        // Substring patterns (case-insensitive)
        assert!(is_status_like_type("MyErrorType"));
        assert!(is_status_like_type("ReturnCode"));

        // Non-status types
        assert!(!is_status_like_type("int"));
        assert!(!is_status_like_type("long"));
        assert!(!is_status_like_type("MyStruct"));
        assert!(!is_status_like_type("buffer_t")); // _t suffix but not status-related... actually this will match
    }

    #[test]
    fn test_stable_key_format() {
        let source = r#"
status_t func(code_t c) {
    switch (c) {
    case A: return S;
    }
    return S;
}
"#;
        let tree = parse_c(source);
        let mappings =
            extract_status_mappings(&tree, source.as_bytes(), "path/to/file.c", "myrepo");

        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings[0].symbol_key,
            "myrepo:path/to/file.c#func:SYMBOL:FUNCTION"
        );
    }

    // ── Regression tests for false positive patterns ──────────────────

    /// Reject command dispatcher with side effects and trailing return.
    /// Pattern: switch(method) { case A: do_x(); break; ... } return result;
    /// This is NOT a STATUS_MAPPING because cases do not return values directly.
    #[test]
    fn test_command_dispatcher_rejected() {
        let source = r#"
channel_op_res_t channel_post_method(channel_method_t method, void *data) {
    channel_op_res_t result = CHANNEL_OK;
    switch (method) {
    case CHANNEL_POST:
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
        break;
    case CHANNEL_PUT:
        curl_easy_setopt(curl, CURLOPT_UPLOAD, 1L);
        break;
    case CHANNEL_GET:
        break;
    }
    return result;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        // Must be rejected: no explicit case-to-output mappings
        assert_eq!(
            mappings.len(),
            0,
            "command dispatcher should not be classified as STATUS_MAPPING"
        );
    }

    /// Reject ioctl-style dispatcher with trailing return.
    /// Pattern: switch(cmd) { case READ: do_read(); break; ... } return err;
    #[test]
    fn test_ioctl_dispatcher_rejected() {
        let source = r#"
disk_result_t disk_ioctl(disk_cmd_t cmd, void *buf) {
    disk_result_t err = DISK_OK;
    switch (cmd) {
    case DISK_READ:
        read_sectors(buf);
        break;
    case DISK_WRITE:
        write_sectors(buf);
        break;
    case DISK_SYNC:
        flush_cache();
        break;
    }
    return err;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(
            mappings.len(),
            0,
            "ioctl dispatcher should not be classified as STATUS_MAPPING"
        );
    }

    /// Reject handler dispatch with fallback return but no direct mappings.
    #[test]
    fn test_handler_dispatch_rejected() {
        let source = r#"
server_op_res_t handle_feedback(feedback_t fb_type, json_t *json_data) {
    server_op_res_t result = SERVER_OK;
    switch (fb_type) {
    case FB_PROGRESS:
        notify_progress(json_data);
        break;
    case FB_ERROR:
        log_error(json_data);
        result = SERVER_EERR;
        break;
    case FB_DONE:
        cleanup();
        break;
    }
    return result;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        // Even though result is modified in some cases, there are no direct
        // case-to-return mappings, so this is not a STATUS_MAPPING.
        assert_eq!(
            mappings.len(),
            0,
            "feedback handler should not be classified as STATUS_MAPPING"
        );
    }

    /// Accept true status mapping even when some cases have side effects.
    /// Key: at least one case directly returns a status value.
    #[test]
    fn test_mixed_side_effects_with_direct_returns_accepted() {
        let source = r#"
result_t process_code(code_t c) {
    switch (c) {
    case CODE_A:
        log_something();
        return RESULT_OK;
    case CODE_B:
        return RESULT_ERR;
    case CODE_C:
        do_cleanup();
        break;
    }
    return RESULT_UNKNOWN;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        // Accepted: has explicit case-to-output mappings (CODE_A->RESULT_OK, CODE_B->RESULT_ERR)
        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];
        assert_eq!(m.mappings.len(), 2);
        assert_eq!(m.mappings[0].inputs, vec!["CODE_A"]);
        assert_eq!(m.mappings[0].output, "RESULT_OK");
        assert_eq!(m.mappings[1].inputs, vec!["CODE_B"]);
        assert_eq!(m.mappings[1].output, "RESULT_ERR");
    }

    // ── Preprocessor handling tests ───────────────────────────────────

    /// Preprocessor-guarded case labels inside switch must be extracted correctly.
    /// Pattern from swupdate's channel_map_curl_error:
    /// ```c
    /// case CURLE_SSL_ISSUER_ERROR:
    /// #if LIBCURL_VERSION_NUM >= 0x072900
    /// case CURLE_SSL_INVALIDCERTSTATUS:
    /// #endif
    ///     return CHANNEL_EINIT;
    /// case CURLE_COULDNT_RESOLVE_HOST:
    ///     return CHANNEL_ENONET;
    /// ```
    /// The return inside the #if block terminates the first case group.
    #[test]
    fn test_preprocessor_guarded_cases_preserve_group_boundaries() {
        let source = r#"
channel_op_res_t channel_map_curl_error(CURLcode res) {
    switch (res) {
    case CURLE_SSL_ENGINE_NOTFOUND:
    case CURLE_SSL_ENGINE_SETFAILED:
    case CURLE_SSL_ISSUER_ERROR:
#if LIBCURL_VERSION_NUM >= 0x072900
    case CURLE_SSL_INVALIDCERTSTATUS:
#endif
        return CHANNEL_EINIT;
    case CURLE_COULDNT_RESOLVE_PROXY:
    case CURLE_COULDNT_RESOLVE_HOST:
        return CHANNEL_ENONET;
    case CURLE_OK:
        return CHANNEL_OK;
    default:
        return CHANNEL_EINIT;
    }
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];

        // Should have 3 distinct case groups, NOT merged together
        assert_eq!(
            m.mappings.len(),
            3,
            "expected 3 case groups, got {:?}",
            m.mappings
        );

        // First group: SSL init errors -> CHANNEL_EINIT
        // Note: preprocessor-guarded CURLE_SSL_INVALIDCERTSTATUS may or may not appear
        // depending on how tree-sitter parses #if blocks
        assert!(
            m.mappings[0]
                .inputs
                .contains(&"CURLE_SSL_ENGINE_NOTFOUND".to_string()),
            "first group should contain CURLE_SSL_ENGINE_NOTFOUND"
        );
        assert_eq!(m.mappings[0].output, "CHANNEL_EINIT");

        // Second group: network errors -> CHANNEL_ENONET
        assert!(
            m.mappings[1]
                .inputs
                .contains(&"CURLE_COULDNT_RESOLVE_HOST".to_string()),
            "second group should contain CURLE_COULDNT_RESOLVE_HOST"
        );
        assert_eq!(m.mappings[1].output, "CHANNEL_ENONET");

        // Third group: OK -> CHANNEL_OK
        assert_eq!(m.mappings[2].inputs, vec!["CURLE_OK"]);
        assert_eq!(m.mappings[2].output, "CHANNEL_OK");

        // Default should be captured
        assert_eq!(m.default_output, Some("CHANNEL_EINIT".to_string()));
    }

    /// Return statement inside preprocessor block correctly terminates case group.
    #[test]
    fn test_return_inside_preproc_terminates_group() {
        let source = r#"
error_code_t map_error(int code) {
    switch (code) {
    case 1:
    case 2:
#ifdef EXTENDED_ERRORS
    case 3:
        return ERR_EXTENDED;
#endif
    case 4:
        return ERR_BASIC;
    }
    return ERR_UNKNOWN;
}
"#;
        let tree = parse_c(source);
        let mappings = extract_status_mappings(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(mappings.len(), 1);
        let m = &mappings[0];

        // Two case groups expected:
        // 1. Cases 1, 2, [3] -> ERR_EXTENDED (from inside #ifdef)
        // 2. Case 4 -> ERR_BASIC
        // The #ifdef'd return terminates the first group
        assert_eq!(
            m.mappings.len(),
            2,
            "expected 2 groups, got {:?}",
            m.mappings
        );
        assert_eq!(m.mappings[0].output, "ERR_EXTENDED");
        assert_eq!(m.mappings[1].output, "ERR_BASIC");
    }
}
