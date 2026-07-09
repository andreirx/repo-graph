//! FD-1B + FD-1B-EXT: React component and hook detection.
//!
//! Detects React component definitions and hook usage, emitting
//! Layer 3 inferences (`react_component`, `react_hook_usage`).
//!
//! Detection patterns:
//! - PascalCase functions returning JSX -> `react_component`
//! - Arrow functions with PascalCase names returning JSX -> `react_component`
//! - `React.FC<T>` typed functions -> `react_component`
//! - Built-in hooks (`useState`, `useEffect`, etc.) -> `react_hook_usage`
//! - Custom hooks (`use*` with lowercase second letter) -> `react_hook_usage`
//!
//! Detection gates:
//! - **Components:** File must import `react` AND have `.tsx`/`.jsx` extension
//! - **Hooks:** File must import `react` AND be any JS/TS family extension
//!   (`.ts`, `.tsx`, `.js`, `.jsx`, `.mts`, `.cts`, `.mjs`, `.cjs`)
//!
//! The hook gate is wider than the component gate because hook detection
//! does not require JSX syntax. This is per FD-1B-EXT.
//!
//! Limitations (first-cut scope):
//! - Does not detect class components (`extends React.Component`)
//! - Does not analyze component props
//! - Does not detect HOCs (Higher-Order Components)
//! - Does not build component hierarchy
//! - Does not detect components in `.ts`/`.js` files with JSX pragma

use repo_graph_indexer::jsts_extensions::{
    get_extension, is_jsts_extension, is_jsts_jsx_extension,
};
use repo_graph_indexer::orchestrator::FileInput;
use repo_graph_storage::types::InferenceInput;
use tree_sitter::{Node, Parser};

// ── Constants ─────────────────────────────────────────────────────────

/// Built-in React hooks.
const BUILTIN_HOOKS: &[&str] = &[
    "useState",
    "useEffect",
    "useContext",
    "useReducer",
    "useCallback",
    "useMemo",
    "useRef",
    "useImperativeHandle",
    "useLayoutEffect",
    "useDebugValue",
    "useDeferredValue",
    "useTransition",
    "useId",
    "useSyncExternalStore",
    "useInsertionEffect",
];

// ── Detection output ──────────────────────────────────────────────────

/// A detected React component.
#[derive(Debug, Clone)]
pub struct ReactComponentDetection {
    /// Component name (PascalCase).
    pub component_name: String,
    /// Detection style: "function", "arrow", "fc_typed".
    pub component_style: String,
    /// True if the function returns JSX.
    pub has_jsx_return: bool,
    /// Import specifier that satisfied the React gate (e.g., "react").
    pub import_gate: String,
    /// Source line (1-based).
    pub line_start: i64,
    /// Detection confidence (0.0 to 1.0).
    pub confidence: f64,
    /// Source file path (repo-relative).
    pub file_path: String,
}

/// A detected React hook usage.
#[derive(Debug, Clone)]
pub struct ReactHookDetection {
    /// Hook name (e.g., "useState", "useCustomHook").
    pub hook_name: String,
    /// Hook category: "builtin" or "custom".
    pub hook_category: String,
    /// Name of the component containing this hook call (if identifiable).
    pub caller_component: Option<String>,
    /// Source line (1-based).
    pub line_start: i64,
    /// Detection confidence (0.0 to 1.0).
    pub confidence: f64,
    /// Source file path (repo-relative).
    pub file_path: String,
}

// ── Public API ────────────────────────────────────────────────────────

/// Detect React components in TSX/JSX files.
///
/// Filters to TSX/JSX files, checks for React import, parses AST,
/// and extracts component definitions. Also reports the count of files skipped
/// for pathological AST nesting (PERSIST-RECURSION-1 item 2 — honest degradation).
pub fn detect_react_components(file_inputs: &[FileInput]) -> DetectedComponents {
    let mut components = Vec::new();
    let mut files_skipped_deep_nesting: Vec<String> = Vec::new();

    let mut parser = Parser::new();
    let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();

    for file in file_inputs {
        // Filter to TSX/JSX files only (narrow gate, FD-1B-EXT will widen).
        // Uses shared utility per FD-SUPPORT-EXT-JSTS.
        let ext = get_extension(&file.rel_path);
        if !is_jsts_jsx_extension(ext) {
            continue;
        }

        // Check for React import.
        let import_gate = match get_react_import(&file.content) {
            Some(gate) => gate,
            None => continue,
        };

        if parser.set_language(&tsx_language).is_err() {
            continue;
        }

        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue,
        };

        // PERSIST-RECURSION-1: skip pathologically deep files (honest degradation),
        // the same guard the compose-level postpasses apply. The walks below are
        // already iterative, so this is a resource bound, not the overflow fix.
        if crate::walk::tree_exceeds_depth(&tree.root_node(), crate::walk::MAX_POSTPASS_TREE_DEPTH)
        {
            files_skipped_deep_nesting.push(file.rel_path.clone());
            continue;
        }

        let file_components = detect_components_in_file(
            &tree.root_node(),
            file.content.as_bytes(),
            &file.rel_path,
            &import_gate,
        );
        components.extend(file_components);
    }

    DetectedComponents {
        components,
        files_skipped_deep_nesting,
    }
}

/// Output of [`detect_react_components`]: detected components plus the honest
/// degradation record — the relative PATHS of files skipped for pathological AST
/// nesting.
///
/// This is a path list, not a count, because React runs TWO passes over the same
/// files (components + hooks) under different gates. A single deep `.tsx` can be
/// skipped by both; the caller unions the two path lists so one file is reported
/// once, not twice (PERSIST-RECURSION-1, review-2 item 1).
#[derive(Debug, Default)]
pub struct DetectedComponents {
    pub components: Vec<ReactComponentDetection>,
    pub files_skipped_deep_nesting: Vec<String>,
}

/// Output of [`detect_react_hooks`]: detected hooks plus the honest degradation
/// record — the relative PATHS of files skipped for pathological AST nesting.
/// See [`DetectedComponents`] for why this is a path list, not a count.
#[derive(Debug, Default)]
pub struct DetectedHooks {
    pub hooks: Vec<ReactHookDetection>,
    pub files_skipped_deep_nesting: Vec<String>,
}

/// Detect React hook usage in JS/TS files.
///
/// Filters to all JS/TS family files (FD-1B-EXT widened gate), checks for
/// React import, parses AST, and extracts hook call sites.
///
/// Hook detection does not require JSX syntax, so it works for all JS/TS
/// extensions including `.ts`, `.js`, `.mts`, `.cts`, `.mjs`, `.cjs`.
pub fn detect_react_hooks(file_inputs: &[FileInput]) -> DetectedHooks {
    let mut hooks = Vec::new();
    let mut files_skipped_deep_nesting: Vec<String> = Vec::new();

    let mut parser = Parser::new();
    let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();

    for file in file_inputs {
        // Filter to all JS/TS family files (widened gate per FD-1B-EXT).
        // Hook detection does not require JSX, so we can include all JS/TS files.
        let ext = get_extension(&file.rel_path);
        if !is_jsts_extension(ext) {
            continue;
        }

        // Check for React import.
        if get_react_import(&file.content).is_none() {
            continue;
        }

        // Select grammar: TSX for .tsx/.jsx (JSX syntax), TS for others.
        let language = if is_jsts_jsx_extension(ext) {
            &tsx_language
        } else {
            &ts_language
        };

        if parser.set_language(language).is_err() {
            continue;
        }

        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue,
        };

        // PERSIST-RECURSION-1: skip pathologically deep files (honest degradation).
        if crate::walk::tree_exceeds_depth(&tree.root_node(), crate::walk::MAX_POSTPASS_TREE_DEPTH)
        {
            files_skipped_deep_nesting.push(file.rel_path.clone());
            continue;
        }

        let file_hooks =
            detect_hooks_in_file(&tree.root_node(), file.content.as_bytes(), &file.rel_path);
        hooks.extend(file_hooks);
    }

    DetectedHooks {
        hooks,
        files_skipped_deep_nesting,
    }
}

// ── Import detection ──────────────────────────────────────────────────

/// Check if source contains React import and return the specifier.
fn get_react_import(source: &str) -> Option<String> {
    // Check for various React import patterns.
    if source.contains("'react'") || source.contains("\"react\"") {
        return Some("react".to_string());
    }
    if source.contains("'@types/react'") || source.contains("\"@types/react\"") {
        return Some("@types/react".to_string());
    }
    // Check for require
    if source.contains("require('react')") || source.contains("require(\"react\")") {
        return Some("react".to_string());
    }
    None
}

// ── Component detection ───────────────────────────────────────────────

/// Detect component definitions in a file.
fn detect_components_in_file(
    root: &Node,
    source: &[u8],
    file_path: &str,
    import_gate: &str,
) -> Vec<ReactComponentDetection> {
    let mut components = Vec::new();
    collect_components(root, source, file_path, import_gate, &mut components);
    components
}

/// Recursively collect component definitions from AST nodes.
fn collect_components(
    node: &Node,
    source: &[u8],
    file_path: &str,
    import_gate: &str,
    components: &mut Vec<ReactComponentDetection>,
) {
    // PERSIST-RECURSION-1: iterative pre-order collect (was recursive on AST
    // depth → stack overflow at scale). Every node is checked in the same order.
    crate::walk::visit_preorder(*node, |node| {
        // Check for function declarations with PascalCase names.
        if node.kind() == "function_declaration" {
            if let Some(component) =
                try_extract_function_component(&node, source, file_path, import_gate)
            {
                components.push(component);
            }
        }

        // Check for variable declarations (arrow functions).
        if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
            if let Some(component) =
                try_extract_arrow_component(&node, source, file_path, import_gate)
            {
                components.push(component);
            }
        }
        std::ops::ControlFlow::Continue(())
    });
}

/// Try to extract a component from a function declaration.
fn try_extract_function_component(
    node: &Node,
    source: &[u8],
    file_path: &str,
    import_gate: &str,
) -> Option<ReactComponentDetection> {
    // Get function name.
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(&name_node, source)?;

    // Must be PascalCase (starts with uppercase).
    if !is_pascal_case(&name) {
        return None;
    }

    // Check if function body returns JSX.
    let body = node.child_by_field_name("body")?;
    let has_jsx = contains_jsx_return(&body, source);

    // Per spec: "PascalCase functions returning JSX". Must have JSX return.
    if !has_jsx {
        return None;
    }

    Some(ReactComponentDetection {
        component_name: name,
        component_style: "function".to_string(),
        has_jsx_return: true,
        import_gate: import_gate.to_string(),
        line_start: node.start_position().row as i64 + 1,
        confidence: 0.9,
        file_path: file_path.to_string(),
    })
}

/// Try to extract a component from an arrow function in a variable declaration.
fn try_extract_arrow_component(
    node: &Node,
    source: &[u8],
    file_path: &str,
    import_gate: &str,
) -> Option<ReactComponentDetection> {
    // Find the variable declarator.
    let declarator = find_child_of_kind(node, "variable_declarator")?;

    // Get variable name.
    let name_node = declarator.child_by_field_name("name")?;
    let name = node_text(&name_node, source)?;

    // Must be PascalCase.
    if !is_pascal_case(&name) {
        return None;
    }

    // Get the value (should be arrow function).
    let value = declarator.child_by_field_name("value")?;

    // Check for React.FC type annotation on the declarator or in the value.
    // Pattern 1: `const Foo: React.FC<Props> = () => ...` (type on declarator)
    // Pattern 2: `const Foo = (() => ...) as React.FC<Props>` (type in value)
    let has_fc_type =
        has_fc_type_annotation(&declarator, source) || has_fc_type_in_value(&value, source);

    // Check if value contains an arrow function.
    if !is_arrow_function_value(&value) {
        return None;
    }

    // Find arrow function body for JSX detection.
    let arrow_body = get_arrow_body(&value);

    let has_jsx = arrow_body
        .map(|b| contains_jsx_return(&b, source))
        .unwrap_or(false);

    // Must have JSX or FC type to be confident.
    if !has_jsx && !has_fc_type {
        return None;
    }

    let style = if has_fc_type { "fc_typed" } else { "arrow" };

    let confidence = if has_fc_type && has_jsx {
        0.95
    } else if has_jsx {
        0.9
    } else {
        0.8
    };

    Some(ReactComponentDetection {
        component_name: name,
        component_style: style.to_string(),
        has_jsx_return: has_jsx,
        import_gate: import_gate.to_string(),
        line_start: node.start_position().row as i64 + 1,
        confidence,
        file_path: file_path.to_string(),
    })
}

/// Check if the declarator has a React.FC type annotation.
/// Pattern: `const Foo: React.FC<Props> = ...`
fn has_fc_type_annotation(declarator: &Node, source: &[u8]) -> bool {
    // Look for type_annotation field on the declarator.
    // In tree-sitter-typescript, the declarator may have a "type" field.
    if let Some(type_ann) = declarator.child_by_field_name("type") {
        let type_text = node_text(&type_ann, source).unwrap_or_default();
        return type_text.contains("React.FC")
            || type_text.contains("React.FunctionComponent")
            || type_text.contains("FC<")
            || type_text.contains("FunctionComponent<");
    }

    // Also check for typed_identifier pattern (name with type annotation).
    let mut cursor = declarator.walk();
    for child in declarator.children(&mut cursor) {
        if child.kind() == "type_annotation" {
            let type_text = node_text(&child, source).unwrap_or_default();
            if type_text.contains("React.FC")
                || type_text.contains("React.FunctionComponent")
                || type_text.contains("FC<")
                || type_text.contains("FunctionComponent<")
            {
                return true;
            }
        }
    }

    false
}

/// Check if the value node contains React.FC (e.g., `as React.FC<Props>`).
fn has_fc_type_in_value(value: &Node, source: &[u8]) -> bool {
    let value_text = node_text(value, source).unwrap_or_default();
    value_text.contains("React.FC")
        || value_text.contains("React.FunctionComponent")
        || value_text.contains(" as FC<")
        || value_text.contains(" as FunctionComponent<")
}

/// Check if a value node is or contains an arrow function.
fn is_arrow_function_value(value: &Node) -> bool {
    // Direct arrow function.
    if value.kind() == "arrow_function" {
        return true;
    }

    // Check for as_expression wrapping arrow function.
    if value.kind() == "as_expression" {
        if let Some(expr) = value.child_by_field_name("expression") {
            if expr.kind() == "arrow_function" {
                return true;
            }
        }
    }

    // Check for parenthesized expression.
    if value.kind() == "parenthesized_expression" {
        let mut cursor = value.walk();
        for child in value.children(&mut cursor) {
            if child.kind() == "arrow_function" {
                return true;
            }
        }
    }

    false
}

/// Get the body of an arrow function within a value node.
fn get_arrow_body<'a>(value: &'a Node<'a>) -> Option<Node<'a>> {
    // Direct arrow function.
    if value.kind() == "arrow_function" {
        return value.child_by_field_name("body");
    }

    // Check for as_expression wrapping arrow function.
    if value.kind() == "as_expression" {
        if let Some(expr) = value.child_by_field_name("expression") {
            if expr.kind() == "arrow_function" {
                return expr.child_by_field_name("body");
            }
        }
    }

    // Check for parenthesized expression.
    if value.kind() == "parenthesized_expression" {
        let mut cursor = value.walk();
        for child in value.children(&mut cursor) {
            if child.kind() == "arrow_function" {
                return child.child_by_field_name("body");
            }
        }
    }

    None
}

/// Check if a function body contains a JSX return statement.
fn contains_jsx_return(body: &Node, source: &[u8]) -> bool {
    // If body is directly a JSX element (arrow function implicit return).
    if body.kind() == "jsx_element"
        || body.kind() == "jsx_self_closing_element"
        || body.kind() == "jsx_fragment"
    {
        return true;
    }

    // Check if body is a parenthesized JSX expression.
    if body.kind() == "parenthesized_expression" {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "jsx_element"
                || child.kind() == "jsx_self_closing_element"
                || child.kind() == "jsx_fragment"
            {
                return true;
            }
        }
    }

    // Search for return statements with JSX.
    contains_jsx_return_recursive(body, source)
}

fn contains_jsx_return_recursive(node: &Node, _source: &[u8]) -> bool {
    // PERSIST-RECURSION-1: iterative pre-order find-first (was recursive on AST
    // depth). The per-node JSX check on a `return_statement`'s immediate children
    // is bounded and unchanged; only the tree descent became iterative.
    let mut found = false;
    crate::walk::visit_preorder(*node, |node| {
        if node.kind() == "return_statement" {
            // Check if return value is JSX.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "jsx_element"
                    || child.kind() == "jsx_self_closing_element"
                    || child.kind() == "jsx_fragment"
                    || child.kind() == "parenthesized_expression"
                {
                    // For parenthesized, check nested.
                    if child.kind() == "parenthesized_expression" {
                        let mut inner = child.walk();
                        for inner_child in child.children(&mut inner) {
                            if inner_child.kind() == "jsx_element"
                                || inner_child.kind() == "jsx_self_closing_element"
                                || inner_child.kind() == "jsx_fragment"
                            {
                                found = true;
                                return std::ops::ControlFlow::Break(());
                            }
                        }
                    } else {
                        found = true;
                        return std::ops::ControlFlow::Break(());
                    }
                }
            }
        }
        std::ops::ControlFlow::Continue(())
    });
    found
}

// ── Hook detection ────────────────────────────────────────────────────

/// Detect hook calls in a file.
fn detect_hooks_in_file(root: &Node, source: &[u8], file_path: &str) -> Vec<ReactHookDetection> {
    let mut hooks = Vec::new();
    let mut enclosing_component: Option<String> = None;
    collect_hooks(
        root,
        source,
        file_path,
        &mut enclosing_component,
        &mut hooks,
    );
    hooks
}

/// Collect hook calls from AST nodes (PERSIST-RECURSION-1: iterative pre-order,
/// was recursive on AST depth → stack overflow at scale).
///
/// The enclosing-symbol context is preserved exactly as the recursive form did:
/// it is saved and restored ONLY across `function_declaration` / `arrow_function`
/// nodes (a `Restore` marker is scheduled for those, popping after the subtree),
/// while a `variable_declarator`'s set intentionally PERSISTS past its subtree
/// (no restore) — matching the original's conditional restore.
fn collect_hooks(
    node: &Node,
    source: &[u8],
    file_path: &str,
    enclosing_symbol: &mut Option<String>,
    hooks: &mut Vec<ReactHookDetection>,
) {
    enum HookWork<'a> {
        Visit(Node<'a>),
        Restore(Option<String>),
    }

    let mut stack: Vec<HookWork> = vec![HookWork::Visit(*node)];
    while let Some(work) = stack.pop() {
        let node = match work {
            HookWork::Restore(prev) => {
                *enclosing_symbol = prev;
                continue;
            }
            HookWork::Visit(n) => n,
        };

        if node.kind() == "function_declaration" {
            // Save + (maybe set), restore after the subtree.
            let restore_symbol = enclosing_symbol.clone();
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Some(name) = node_text(&name_node, source) {
                    // Track PascalCase (components) or use* (custom hooks).
                    if is_pascal_case(&name) || is_hook_name(&name) {
                        *enclosing_symbol = Some(name);
                    }
                }
            }
            stack.push(HookWork::Restore(restore_symbol));
        } else if node.kind() == "arrow_function" {
            // Save + restore after the subtree (no set — matches the recursive
            // form, which only restored on function_declaration / arrow_function).
            let restore_symbol = enclosing_symbol.clone();
            stack.push(HookWork::Restore(restore_symbol));
        } else if node.kind() == "variable_declarator" {
            // Arrow function with PascalCase / hook name: the set PERSISTS past
            // the subtree (no restore marker) — exactly as before.
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Some(name) = node_text(&name_node, source) {
                    if is_pascal_case(&name) || is_hook_name(&name) {
                        // Check if value is arrow function.
                        if let Some(value) = node.child_by_field_name("value") {
                            if value.kind() == "arrow_function" {
                                *enclosing_symbol = Some(name);
                            }
                        }
                    }
                }
            }
        } else if node.kind() == "call_expression" {
            // Check for hook call expressions.
            if let Some(hook) = try_extract_hook_call(&node, source, file_path, enclosing_symbol) {
                hooks.push(hook);
            }
        }

        // Recurse into all children, reverse-pushed for left-to-right pre-order.
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(HookWork::Visit(child));
        }
    }
}

/// Try to extract a hook call from a call expression.
fn try_extract_hook_call(
    node: &Node,
    source: &[u8],
    file_path: &str,
    enclosing_symbol: &Option<String>,
) -> Option<ReactHookDetection> {
    let callee = node.child_by_field_name("function")?;
    let hook_name = node_text(&callee, source)?;

    // Must start with "use".
    if !hook_name.starts_with("use") {
        return None;
    }

    // Verify it looks like a hook: "use" followed by uppercase letter.
    // (e.g., "useState", not "used" or "user")
    let chars: Vec<char> = hook_name.chars().collect();
    if chars.len() < 4 {
        return None;
    }
    if !chars[3].is_uppercase() {
        return None;
    }

    let is_builtin = BUILTIN_HOOKS.contains(&hook_name.as_str());
    let category = if is_builtin { "builtin" } else { "custom" };
    let confidence = if is_builtin { 0.9 } else { 0.8 };

    Some(ReactHookDetection {
        hook_name,
        hook_category: category.to_string(),
        caller_component: enclosing_symbol.clone(),
        line_start: node.start_position().row as i64 + 1,
        confidence,
        file_path: file_path.to_string(),
    })
}

/// Check if a name is a hook name (starts with "use" followed by uppercase).
fn is_hook_name(name: &str) -> bool {
    if !name.starts_with("use") {
        return false;
    }
    let chars: Vec<char> = name.chars().collect();
    chars.len() >= 4 && chars[3].is_uppercase()
}

// ── Inference conversion ──────────────────────────────────────────────

/// Convert component detections to inference inputs.
pub fn components_to_inferences(
    components: &[ReactComponentDetection],
    snapshot_uid: &str,
    repo_uid: &str,
) -> Vec<InferenceInput> {
    components
        .iter()
        .map(|c| {
            let inference_uid = format!("inf-react-comp-{}", uuid::Uuid::new_v4());
            let target_stable_key = format!(
                "{}:{}#{}:SYMBOL:FUNCTION",
                repo_uid, c.file_path, c.component_name
            );
            let value_json = serde_json::json!({
                "component_name": c.component_name,
                "component_style": c.component_style,
                "has_jsx_return": c.has_jsx_return,
                "import_gate": c.import_gate,
                "line_start": c.line_start,
            })
            .to_string();
            let basis_json = serde_json::json!({
                "rule": "react_component_detection",
                "style": c.component_style,
                "jsx_detected": c.has_jsx_return,
            })
            .to_string();

            InferenceInput {
                inference_uid,
                snapshot_uid: snapshot_uid.to_string(),
                repo_uid: repo_uid.to_string(),
                target_stable_key,
                kind: "react_component".to_string(),
                value_json,
                confidence: c.confidence,
                basis_json,
                extractor: "react-detector:0.1.0".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                provenance_json: Some(
                    serde_json::json!({
                        "source_file": c.file_path,
                        "line_start": c.line_start,
                    })
                    .to_string(),
                ),
            }
        })
        .collect()
}

/// Convert hook detections to inference inputs.
pub fn hooks_to_inferences(
    hooks: &[ReactHookDetection],
    snapshot_uid: &str,
    repo_uid: &str,
) -> Vec<InferenceInput> {
    hooks
        .iter()
        .map(|h| {
            let inference_uid = format!("inf-react-hook-{}", uuid::Uuid::new_v4());
            // Target is the caller component if known, otherwise the file.
            let target_stable_key = match &h.caller_component {
                Some(comp) => format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, h.file_path, comp),
                None => format!("{}:{}:FILE", repo_uid, h.file_path),
            };
            let value_json = serde_json::json!({
                "hook_name": h.hook_name,
                "hook_category": h.hook_category,
                "caller_component": h.caller_component,
                "line_start": h.line_start,
            })
            .to_string();
            let basis_json = serde_json::json!({
                "rule": "react_hook_detection",
                "category": h.hook_category,
            })
            .to_string();

            InferenceInput {
                inference_uid,
                snapshot_uid: snapshot_uid.to_string(),
                repo_uid: repo_uid.to_string(),
                target_stable_key,
                kind: "react_hook_usage".to_string(),
                value_json,
                confidence: h.confidence,
                basis_json,
                extractor: "react-detector:0.1.0".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                provenance_json: Some(
                    serde_json::json!({
                        "source_file": h.file_path,
                        "line_start": h.line_start,
                    })
                    .to_string(),
                ),
            }
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Check if a name is PascalCase (starts with uppercase).
fn is_pascal_case(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

/// Find first child of a specific kind.
#[allow(clippy::manual_find)]
fn find_child_of_kind<'a>(node: &'a Node, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Extract text content from an AST node.
fn node_text(node: &Node, source: &[u8]) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    if end > source.len() {
        return None;
    }
    std::str::from_utf8(&source[start..end])
        .ok()
        .map(|s| s.to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_detect_components(source: &str) -> Vec<ReactComponentDetection> {
        let mut parser = Parser::new();
        let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        parser.set_language(&tsx_language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        detect_components_in_file(&tree.root_node(), source.as_bytes(), "test.tsx", "react")
    }

    fn parse_and_detect_hooks(source: &str) -> Vec<ReactHookDetection> {
        let mut parser = Parser::new();
        let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        parser.set_language(&tsx_language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        detect_hooks_in_file(&tree.root_node(), source.as_bytes(), "test.tsx")
    }

    fn file_input(rel_path: &str, content: &str) -> FileInput {
        FileInput {
            rel_path: rel_path.to_string(),
            content: content.to_string(),
            content_hash: String::new(),
            size_bytes: content.len(),
            line_count: content.lines().count(),
            package_dependencies: None,
            tsconfig_aliases: None,
        }
    }

    /// PERSIST-RECURSION-1 item 2: the public detectors apply the per-file depth
    /// guard — a pathologically deep .tsx is skipped (no partial facts). Each pass
    /// reports the skipped file's PATH (not a bare count); the compose-level caller
    /// unions the two path lists so a file skipped by both is reported ONCE
    /// (review-2 item 1 — see `persist_react_inferences_dedups_deep_file` in
    /// `compose.rs`). A normal file is detected as before.
    #[test]
    fn detect_react_skips_pathologically_deep_files() {
        // A normal React file: detected, nothing skipped.
        let normal = file_input(
            "ok.tsx",
            "import React, { useState } from 'react';\nfunction Ok() {\n  useState(0);\n  return <div/>;\n}\n",
        );
        let comps = detect_react_components(std::slice::from_ref(&normal));
        let hooks = detect_react_hooks(std::slice::from_ref(&normal));
        assert_eq!(comps.components.len(), 1, "normal component detected");
        assert_eq!(hooks.hooks.len(), 1, "normal hook detected");
        assert!(comps.files_skipped_deep_nesting.is_empty());
        assert!(hooks.files_skipped_deep_nesting.is_empty());

        // A .tsx nested well past the guard: skipped by BOTH passes; each reports
        // the same path (the compose-level union dedups it to one file).
        let mut deep =
            String::from("import React, { useState } from 'react';\nfunction Deep() {\n");
        for _ in 0..(crate::walk::MAX_POSTPASS_TREE_DEPTH + 2_000) {
            deep.push('{');
        }
        deep.push_str(" useState(0); ");
        for _ in 0..(crate::walk::MAX_POSTPASS_TREE_DEPTH + 2_000) {
            deep.push('}');
        }
        deep.push_str("\n  return <div/>;\n}\n");
        let deep_file = file_input("deep.tsx", &deep);
        let comps = detect_react_components(std::slice::from_ref(&deep_file));
        let hooks = detect_react_hooks(std::slice::from_ref(&deep_file));
        assert_eq!(comps.components.len(), 0, "no partial component facts");
        assert_eq!(hooks.hooks.len(), 0, "no partial hook facts");
        assert_eq!(
            comps.files_skipped_deep_nesting,
            vec!["deep.tsx".to_string()]
        );
        assert_eq!(
            hooks.files_skipped_deep_nesting,
            vec!["deep.tsx".to_string()]
        );
    }

    /// PERSIST-RECURSION-1 regression: a deeply nested file must NOT overflow the
    /// React walks — `collect_hooks` (stateful enclosing-symbol tracking) and
    /// `collect_components` / `contains_jsx_return_recursive` (`visit_preorder`).
    /// Runs on the default test-thread stack, so a still-recursive walk aborts.
    #[test]
    fn deeply_nested_input_does_not_overflow() {
        let depth = 50_000;
        let mut source = String::from("function DeepComponent() {\n");
        for _ in 0..depth {
            source.push('{');
        }
        source.push_str(" useState(0); ");
        for _ in 0..depth {
            source.push('}');
        }
        source.push_str("\n  return null;\n}\n");

        // Reaching these assertions proves no stack overflow in either walk.
        let hooks = parse_and_detect_hooks(&source);
        let components = parse_and_detect_components(&source);
        assert!(hooks.len() <= 1);
        assert!(components.len() <= 1);
    }

    // ── Component tests ──────────────────────────────────────────────

    #[test]
    fn detects_function_component() {
        let source = r#"
import React from 'react';

function UserProfile() {
  return <div>Hello</div>;
}
"#;
        let components = parse_and_detect_components(source);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].component_name, "UserProfile");
        assert_eq!(components[0].component_style, "function");
        assert!(components[0].has_jsx_return);
    }

    #[test]
    fn detects_arrow_component() {
        let source = r#"
import React from 'react';

const Dashboard = () => {
  return <main>Dashboard</main>;
};
"#;
        let components = parse_and_detect_components(source);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].component_name, "Dashboard");
        assert_eq!(components[0].component_style, "arrow");
        assert!(components[0].has_jsx_return);
    }

    #[test]
    fn detects_fc_typed_component() {
        let source = r#"
import React from 'react';

const Card: React.FC<Props> = ({ title }) => {
  return <article>{title}</article>;
};
"#;
        let components = parse_and_detect_components(source);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].component_name, "Card");
        assert_eq!(components[0].component_style, "fc_typed");
        assert!(components[0].has_jsx_return);
    }

    #[test]
    fn ignores_lowercase_functions() {
        let source = r#"
import React from 'react';

function helper() {
  return <div>helper</div>;
}
"#;
        let components = parse_and_detect_components(source);
        assert!(components.is_empty());
    }

    #[test]
    fn detects_multiple_components() {
        let source = r#"
import React from 'react';

function Header() {
  return <header>Header</header>;
}

const Footer = () => <footer>Footer</footer>;

function Sidebar() {
  return <aside>Sidebar</aside>;
}
"#;
        let components = parse_and_detect_components(source);
        assert_eq!(components.len(), 3);
    }

    // ── Hook tests ───────────────────────────────────────────────────

    #[test]
    fn detects_usestate() {
        let source = r#"
import React, { useState } from 'react';

function Counter() {
  const [count, setCount] = useState(0);
  return <button>{count}</button>;
}
"#;
        let hooks = parse_and_detect_hooks(source);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].hook_name, "useState");
        assert_eq!(hooks[0].hook_category, "builtin");
        assert_eq!(hooks[0].caller_component, Some("Counter".to_string()));
    }

    #[test]
    fn detects_multiple_hooks() {
        let source = r#"
import React, { useState, useEffect, useCallback } from 'react';

function UserList() {
  const [users, setUsers] = useState([]);
  useEffect(() => {
    fetch('/api/users').then(r => r.json()).then(setUsers);
  }, []);
  const handleClick = useCallback(() => {}, []);
  return <ul>{users.map(u => <li key={u.id}>{u.name}</li>)}</ul>;
}
"#;
        let hooks = parse_and_detect_hooks(source);
        assert_eq!(hooks.len(), 3);
        let names: Vec<&str> = hooks.iter().map(|h| h.hook_name.as_str()).collect();
        assert!(names.contains(&"useState"));
        assert!(names.contains(&"useEffect"));
        assert!(names.contains(&"useCallback"));
    }

    #[test]
    fn detects_custom_hook() {
        let source = r#"
import React from 'react';

function useCustomData() {
  return { data: [] };
}

function MyComponent() {
  const data = useCustomData();
  return <div>{JSON.stringify(data)}</div>;
}
"#;
        let hooks = parse_and_detect_hooks(source);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].hook_name, "useCustomData");
        assert_eq!(hooks[0].hook_category, "custom");
    }

    #[test]
    fn ignores_non_hook_use_prefix() {
        let source = r#"
import React from 'react';

function user() {
  return null;
}

function used() {
  return null;
}
"#;
        let hooks = parse_and_detect_hooks(source);
        assert!(hooks.is_empty());
    }

    // ── Gate tests ───────────────────────────────────────────────────

    #[test]
    fn get_react_import_various_forms() {
        assert!(get_react_import("import React from 'react';").is_some());
        assert!(get_react_import("import React from \"react\";").is_some());
        assert!(get_react_import("import { useState } from 'react';").is_some());
        assert!(get_react_import("const React = require('react');").is_some());
        assert!(get_react_import("import express from 'express';").is_none());
    }
}
