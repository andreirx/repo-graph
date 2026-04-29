//! BEHAVIORAL_MARKER extractor for C.
//!
//! Extracts behavioral markers from C source files indicating
//! policy-relevant control flow patterns.
//!
//! ## PF-2 Scope
//!
//! - RETRY_LOOP: loops with sleep/delay that retry operations
//! - RESUME_OFFSET: curl CURLOPT_RESUME_FROM* patterns
//!
//! See `docs/slices/pf-2-behavioral-marker.md` for full rules.

use crate::types::{BehavioralMarker, MarkerEvidence, MarkerKind};

/// Known sleep/delay function names (libc/POSIX only per PF-2 scope).
const SLEEP_FUNCTIONS: &[&str] = &[
    "sleep",     // POSIX: sleep(seconds)
    "usleep",    // POSIX: usleep(microseconds)
    "nanosleep", // POSIX: nanosleep(timespec)
    "Sleep",     // Windows: Sleep(milliseconds)
];

/// poll() is also a sleep-equivalent when used with timeout > 0.
const POLL_FUNCTION: &str = "poll";

/// Extract BEHAVIORAL_MARKER facts from a C file's AST.
///
/// # Arguments
/// * `tree` - Parsed tree-sitter tree
/// * `source` - Source file bytes
/// * `file_path` - Repo-relative file path
/// * `repo_uid` - Repository identifier
///
/// # Returns
/// Vector of BehavioralMarker structs found in the file.
pub fn extract_behavioral_markers(
    tree: &tree_sitter::Tree,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
) -> Vec<BehavioralMarker> {
    let mut results = Vec::new();
    let root = tree.root_node();

    // Walk top-level declarations
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                extract_markers_from_function(&child, source, file_path, repo_uid, &mut results);
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
    results: &mut Vec<BehavioralMarker>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                extract_markers_from_function(&child, source, file_path, repo_uid, results);
            }
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
                walk_preproc_for_functions(&child, source, file_path, repo_uid, results);
            }
            _ => {}
        }
    }
}

/// Extract markers from a single function definition.
fn extract_markers_from_function(
    func_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    repo_uid: &str,
    results: &mut Vec<BehavioralMarker>,
) {
    // Extract function name
    let declarator = match func_node.child_by_field_name("declarator") {
        Some(d) => d,
        None => return,
    };
    let function_name = match extract_function_name(&declarator, source) {
        Some(n) => n,
        None => return,
    };

    // Build symbol key
    let symbol_key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, function_name);

    // Get function body
    let body = match func_node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    // Extract RETRY_LOOP markers
    extract_retry_loops(
        &body,
        source,
        file_path,
        &function_name,
        &symbol_key,
        results,
    );

    // Extract RESUME_OFFSET markers
    extract_resume_offsets(
        &body,
        source,
        file_path,
        &function_name,
        &symbol_key,
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

// =============================================================================
// RETRY_LOOP extraction
// =============================================================================

/// Extract RETRY_LOOP markers from a function body.
///
/// A RETRY_LOOP is a loop (while/for/do-while) that contains a sleep/delay call.
fn extract_retry_loops(
    body: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
    results: &mut Vec<BehavioralMarker>,
) {
    find_loops_recursive(body, source, file_path, function_name, symbol_key, results);
}

/// Recursively find loop statements in a node tree.
fn find_loops_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
    results: &mut Vec<BehavioralMarker>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "while_statement" | "for_statement" | "do_statement" => {
                if let Some(marker) =
                    try_extract_retry_loop(&child, source, file_path, function_name, symbol_key)
                {
                    results.push(marker);
                }
                // Also check for nested loops inside this loop
                find_loops_recursive(&child, source, file_path, function_name, symbol_key, results);
            }
            // Recurse into other compound structures
            "compound_statement" | "if_statement" | "switch_statement" | "case_statement"
            | "preproc_if" | "preproc_ifdef" | "preproc_else" | "preproc_elif" => {
                find_loops_recursive(&child, source, file_path, function_name, symbol_key, results);
            }
            _ => {}
        }
    }
}

/// Try to extract a RETRY_LOOP marker from a loop statement.
///
/// A loop is a RETRY_LOOP candidate only if it has:
/// 1. A sleep/delay call in the body
/// 2. Result-dependent control (at least one of):
///    - Break/return on a success condition
///    - Loop condition references a result/status variable
///    - Counter variable with decrement/increment toward limit
fn try_extract_retry_loop(
    loop_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
) -> Option<BehavioralMarker> {
    let loop_kind = match loop_node.kind() {
        "while_statement" => "while",
        "for_statement" => "for",
        "do_statement" => "do_while",
        _ => return None,
    };

    // Find sleep call in loop body
    let (sleep_call, delay_ms) = find_sleep_in_loop(loop_node, source)?;

    // Extract max_attempts from loop condition (for-loops only)
    let max_attempts = if loop_kind == "for" {
        extract_max_attempts_from_for(loop_node, source)
    } else {
        None
    };

    // Extract break condition
    let break_condition = extract_break_condition(loop_node, source);

    // PF-2 requirement: must have result-dependent control evidence.
    // At least one of:
    // - Counter-based loop (max_attempts determinable)
    // - Break/return on success condition
    // - Loop condition references result/status variable (do-while or while)
    let has_counter_control = max_attempts.is_some();
    let has_break_condition = break_condition.is_some();
    let has_result_condition = has_result_dependent_condition(loop_node, source, loop_kind);

    if !has_counter_control && !has_break_condition && !has_result_condition {
        // Not a retry loop — just a delayed loop without retry semantics
        return None;
    }

    let line_start = (loop_node.start_position().row + 1) as u32;
    let line_end = (loop_node.end_position().row + 1) as u32;

    Some(BehavioralMarker {
        symbol_key: symbol_key.to_string(),
        function_name: function_name.to_string(),
        file_path: file_path.to_string(),
        line_start,
        line_end,
        kind: MarkerKind::RetryLoop,
        evidence: MarkerEvidence::RetryLoop {
            loop_kind: loop_kind.to_string(),
            sleep_call: Some(sleep_call),
            delay_ms,
            max_attempts,
            break_condition,
        },
    })
}

/// Check if a loop has a result-dependent condition.
///
/// For do-while and while loops, checks if the condition references
/// a result/status-like variable (contains "result", "status", "err",
/// "ret", "ok", "rc", etc.).
fn has_result_dependent_condition(
    loop_node: &tree_sitter::Node,
    source: &[u8],
    loop_kind: &str,
) -> bool {
    // For for-loops, counter control is checked separately via max_attempts
    if loop_kind == "for" {
        return false;
    }

    // Get the loop condition
    let condition = match loop_node.child_by_field_name("condition") {
        Some(c) => c,
        None => return false,
    };

    let cond_text = match condition.utf8_text(source) {
        Ok(t) => t.to_lowercase(),
        Err(_) => return false,
    };

    // Check for result/status-like identifiers in the condition
    const RESULT_PATTERNS: &[&str] = &[
        "result", "status", "error", "err", "ret", "rc", "ok", "success",
        "fail", "code", "state", "try", "attempt", "count",
    ];

    for pattern in RESULT_PATTERNS {
        if cond_text.contains(pattern) {
            return true;
        }
    }

    // Also check for comparisons to common success/failure constants
    const VALUE_PATTERNS: &[&str] = &[
        "== 0", "!= 0", "< 0", "> 0", "<= 0", ">= 0",
        "_ok", "_err", "_fail", "_success",
    ];

    for pattern in VALUE_PATTERNS {
        if cond_text.contains(pattern) {
            return true;
        }
    }

    false
}

/// Find a sleep/delay call in a loop body.
/// Returns (function_name, delay_ms) if found.
fn find_sleep_in_loop(loop_node: &tree_sitter::Node, source: &[u8]) -> Option<(String, Option<u64>)> {
    find_sleep_recursive(loop_node, source)
}

/// Recursively search for sleep calls.
fn find_sleep_recursive(node: &tree_sitter::Node, source: &[u8]) -> Option<(String, Option<u64>)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(result) = check_call_for_sleep(&child, source) {
                return Some(result);
            }
        }
        // Recurse
        if let Some(result) = find_sleep_recursive(&child, source) {
            return Some(result);
        }
    }
    None
}

/// Check if a call expression is a sleep/delay call.
fn check_call_for_sleep(call_node: &tree_sitter::Node, source: &[u8]) -> Option<(String, Option<u64>)> {
    let callee = call_node.child_by_field_name("function")?;
    let callee_name = callee.utf8_text(source).ok()?.trim().to_string();

    // Check for known sleep functions
    for &func in SLEEP_FUNCTIONS {
        if callee_name == func {
            let delay_ms = extract_sleep_delay_ms(call_node, source, func);
            return Some((callee_name, delay_ms));
        }
    }

    // Check for poll with timeout
    if callee_name == POLL_FUNCTION {
        let delay_ms = extract_poll_timeout_ms(call_node, source);
        if delay_ms.is_some() {
            return Some((callee_name, delay_ms));
        }
    }

    None
}

/// Extract delay in milliseconds from sleep call arguments.
fn extract_sleep_delay_ms(
    call_node: &tree_sitter::Node,
    source: &[u8],
    func_name: &str,
) -> Option<u64> {
    let args = call_node.child_by_field_name("arguments")?;
    let first_arg = get_first_argument(&args, source)?;

    // Try to parse as numeric literal
    let value: u64 = first_arg.parse().ok()?;

    // Convert to milliseconds based on function
    match func_name {
        "sleep" => Some(value * 1000),       // seconds -> ms
        "Sleep" => Some(value),               // already ms (Windows)
        "usleep" => Some(value / 1000),       // microseconds -> ms
        "nanosleep" => None,                  // requires parsing timespec struct
        _ => None,
    }
}

/// Extract poll timeout in milliseconds.
fn extract_poll_timeout_ms(call_node: &tree_sitter::Node, source: &[u8]) -> Option<u64> {
    // poll(fds, nfds, timeout) - timeout is 3rd argument in ms
    let args = call_node.child_by_field_name("arguments")?;
    let third_arg = get_nth_argument(&args, 2, source)?;

    // Try to parse as numeric literal
    let value: i64 = third_arg.parse().ok()?;
    if value > 0 {
        Some(value as u64)
    } else {
        None
    }
}

/// Get first argument from argument list as string.
fn get_first_argument(args: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    get_nth_argument(args, 0, source)
}

/// Get nth argument from argument list as string.
fn get_nth_argument(args: &tree_sitter::Node, n: usize, source: &[u8]) -> Option<String> {
    let mut cursor = args.walk();
    let mut count = 0;
    for child in args.children(&mut cursor) {
        // Skip punctuation
        if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
            continue;
        }
        if count == n {
            return Some(child.utf8_text(source).ok()?.trim().to_string());
        }
        count += 1;
    }
    None
}

/// Extract max_attempts from for-loop initializer/condition.
///
/// Handles patterns like:
/// - `for (int i = 0; i < 5; i++)` -> 5
/// - `for (int retries = 3; retries >= 0; retries--)` -> 4
fn extract_max_attempts_from_for(for_node: &tree_sitter::Node, source: &[u8]) -> Option<u32> {
    // Get initializer and condition
    let init = for_node.child_by_field_name("initializer")?;
    let condition = for_node.child_by_field_name("condition")?;

    // Try to find the loop variable and its initial value
    let (var_name, init_value) = extract_for_init_value(&init, source)?;

    // Try to parse the condition to find the limit
    extract_limit_from_condition(&condition, source, &var_name, init_value)
}

/// Extract variable name and initial value from for-loop initializer.
fn extract_for_init_value(init: &tree_sitter::Node, source: &[u8]) -> Option<(String, i64)> {
    // Handle declaration: `int i = 0` or `int retries = 3`
    if init.kind() == "declaration" {
        let mut cursor = init.walk();
        for child in init.children(&mut cursor) {
            if child.kind() == "init_declarator" {
                let declarator = child.child_by_field_name("declarator")?;
                let value = child.child_by_field_name("value")?;

                let var_name = declarator.utf8_text(source).ok()?.trim().to_string();
                let init_val: i64 = value.utf8_text(source).ok()?.trim().parse().ok()?;

                return Some((var_name, init_val));
            }
        }
    }

    None
}

/// Extract iteration count from for-loop condition.
fn extract_limit_from_condition(
    condition: &tree_sitter::Node,
    source: &[u8],
    var_name: &str,
    init_value: i64,
) -> Option<u32> {
    let cond_text = condition.utf8_text(source).ok()?.trim().to_string();

    // Pattern: `i < N` or `i <= N` (counting up)
    if cond_text.contains('<') {
        let parts: Vec<&str> = if cond_text.contains("<=") {
            cond_text.split("<=").collect()
        } else {
            cond_text.split('<').collect()
        };

        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();

            if left == var_name {
                let limit: i64 = right.parse().ok()?;
                let count = if cond_text.contains("<=") {
                    limit - init_value + 1
                } else {
                    limit - init_value
                };
                if count > 0 {
                    return Some(count as u32);
                }
            }
        }
    }

    // Pattern: `i >= 0` or `i > 0` (counting down)
    if cond_text.contains(">=") || (cond_text.contains('>') && !cond_text.contains(">=")) {
        let parts: Vec<&str> = if cond_text.contains(">=") {
            cond_text.split(">=").collect()
        } else {
            cond_text.split('>').collect()
        };

        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();

            if left == var_name {
                let limit: i64 = right.parse().ok()?;
                let count = if cond_text.contains(">=") {
                    init_value - limit + 1
                } else {
                    init_value - limit
                };
                if count > 0 {
                    return Some(count as u32);
                }
            }
        }
    }

    None
}

/// Extract break condition from loop.
fn extract_break_condition(loop_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // For do-while, get the while condition
    if loop_node.kind() == "do_statement" {
        if let Some(cond) = loop_node.child_by_field_name("condition") {
            let cond_text = cond.utf8_text(source).ok()?.trim().to_string();
            // Remove outer parentheses if present
            let cleaned = cond_text
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(&cond_text)
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }

    // For while/for, look for break with condition
    if let Some(break_cond) = find_break_condition_in_body(loop_node, source) {
        return Some(break_cond);
    }

    None
}

/// Find break condition from if-break pattern in loop body.
fn find_break_condition_in_body(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "if_statement" {
            // Check if this if leads to a break
            if contains_break(&child) {
                if let Some(cond) = child.child_by_field_name("condition") {
                    let cond_text = cond.utf8_text(source).ok()?.trim().to_string();
                    let cleaned = cond_text
                        .strip_prefix('(')
                        .and_then(|s| s.strip_suffix(')'))
                        .unwrap_or(&cond_text)
                        .trim()
                        .to_string();
                    if !cleaned.is_empty() {
                        return Some(cleaned);
                    }
                }
            }
        }
        // Recurse into compound statements
        if child.kind() == "compound_statement" {
            if let Some(cond) = find_break_condition_in_body(&child, source) {
                return Some(cond);
            }
        }
    }
    None
}

/// Check if a node contains a break statement.
fn contains_break(node: &tree_sitter::Node) -> bool {
    if node.kind() == "break_statement" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_break(&child) {
            return true;
        }
    }
    false
}

// =============================================================================
// RESUME_OFFSET extraction
// =============================================================================

/// Extract RESUME_OFFSET markers from a function body.
fn extract_resume_offsets(
    body: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
    results: &mut Vec<BehavioralMarker>,
) {
    find_curl_resume_recursive(body, source, file_path, function_name, symbol_key, results);
}

/// Recursively find curl_easy_setopt calls with CURLOPT_RESUME_FROM*.
fn find_curl_resume_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
    results: &mut Vec<BehavioralMarker>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(marker) =
                try_extract_resume_offset(&child, source, file_path, function_name, symbol_key)
            {
                results.push(marker);
            }
        }
        // Recurse
        find_curl_resume_recursive(&child, source, file_path, function_name, symbol_key, results);
    }
}

/// Try to extract a RESUME_OFFSET marker from a call expression.
fn try_extract_resume_offset(
    call_node: &tree_sitter::Node,
    source: &[u8],
    file_path: &str,
    function_name: &str,
    symbol_key: &str,
) -> Option<BehavioralMarker> {
    // Check if this is curl_easy_setopt
    let callee = call_node.child_by_field_name("function")?;
    let callee_name = callee.utf8_text(source).ok()?.trim();

    if callee_name != "curl_easy_setopt" {
        return None;
    }

    // Get arguments
    let args = call_node.child_by_field_name("arguments")?;

    // Check second argument for CURLOPT_RESUME_FROM*
    let second_arg = get_nth_argument(&args, 1, source)?;

    if second_arg != "CURLOPT_RESUME_FROM" && second_arg != "CURLOPT_RESUME_FROM_LARGE" {
        return None;
    }

    // Get third argument (offset source)
    let offset_source = get_nth_argument(&args, 2, source);

    // Get line range - find the statement containing this call
    let line_start = (call_node.start_position().row + 1) as u32;
    let line_end = (call_node.end_position().row + 1) as u32;

    Some(BehavioralMarker {
        symbol_key: symbol_key.to_string(),
        function_name: function_name.to_string(),
        file_path: file_path.to_string(),
        line_start,
        line_end,
        kind: MarkerKind::ResumeOffset,
        evidence: MarkerEvidence::ResumeOffset {
            api_call: "curl_easy_setopt".to_string(),
            option_name: Some(second_arg),
            offset_source,
        },
    })
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
    // RETRY_LOOP tests
    // =========================================================================

    #[test]
    fn test_for_loop_with_sleep_detected() {
        let source = r#"
void retry_connect(void) {
    for (int retries = 3; retries >= 0; retries--) {
        int result = try_connect();
        if (result > 0)
            break;
        sleep(1);
    }
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        let m = &markers[0];
        assert_eq!(m.function_name, "retry_connect");
        assert_eq!(m.kind, MarkerKind::RetryLoop);

        if let MarkerEvidence::RetryLoop {
            loop_kind,
            sleep_call,
            delay_ms,
            max_attempts,
            break_condition,
        } = &m.evidence
        {
            assert_eq!(loop_kind, "for");
            assert_eq!(sleep_call.as_deref(), Some("sleep"));
            assert_eq!(*delay_ms, Some(1000)); // 1 second = 1000ms
            assert_eq!(*max_attempts, Some(4)); // retries 3,2,1,0 = 4 iterations
            assert_eq!(break_condition.as_deref(), Some("result > 0"));
        } else {
            panic!("expected RetryLoop evidence");
        }
    }

    #[test]
    fn test_do_while_with_sleep_detected() {
        let source = r#"
int download_with_retry(void) {
    int result;
    int try_count = 0;
    do {
        result = perform_download();
        if (result != OK) {
            sleep(30);
        }
    } while (++try_count && (result != OK));
    return result;
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        let m = &markers[0];
        assert_eq!(m.kind, MarkerKind::RetryLoop);

        if let MarkerEvidence::RetryLoop {
            loop_kind,
            sleep_call,
            delay_ms,
            max_attempts,
            break_condition,
        } = &m.evidence
        {
            assert_eq!(loop_kind, "do_while");
            assert_eq!(sleep_call.as_deref(), Some("sleep"));
            assert_eq!(*delay_ms, Some(30000)); // 30 seconds
            assert_eq!(*max_attempts, None); // not determinable
            // do-while condition
            assert!(break_condition.as_ref().map_or(false, |c| c.contains("result != OK")));
        } else {
            panic!("expected RetryLoop evidence");
        }
    }

    #[test]
    fn test_while_with_usleep_detected() {
        let source = r#"
void poll_status(void) {
    while (1) {
        int status = check_status();
        if (status == DONE)
            break;
        usleep(100000); // 100ms
    }
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        if let MarkerEvidence::RetryLoop {
            loop_kind,
            sleep_call,
            delay_ms,
            ..
        } = &markers[0].evidence
        {
            assert_eq!(loop_kind, "while");
            assert_eq!(sleep_call.as_deref(), Some("usleep"));
            assert_eq!(*delay_ms, Some(100)); // 100000 usec = 100ms
        } else {
            panic!("expected RetryLoop evidence");
        }
    }

    #[test]
    fn test_loop_without_sleep_not_detected() {
        let source = r#"
void busy_wait(void) {
    while (1) {
        if (check_ready())
            break;
        // No sleep - busy wait
    }
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        // Loop without sleep should NOT be detected as RETRY_LOOP
        assert_eq!(markers.len(), 0);
    }

    #[test]
    fn test_dynamic_sleep_delay_is_none() {
        let source = r#"
void retry_with_config(config_t *cfg) {
    for (int i = 0; i < cfg->max_retries; i++) {
        if (try_operation())
            break;
        sleep(cfg->retry_delay);
    }
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        if let MarkerEvidence::RetryLoop {
            delay_ms,
            max_attempts,
            ..
        } = &markers[0].evidence
        {
            // Dynamic values should be None
            assert_eq!(*delay_ms, None);
            assert_eq!(*max_attempts, None);
        } else {
            panic!("expected RetryLoop evidence");
        }
    }

    // =========================================================================
    // RESUME_OFFSET tests
    // =========================================================================

    #[test]
    fn test_curl_resume_from_large_detected() {
        let source = r#"
int download_resume(CURL *curl, size_t offset) {
    curl_easy_setopt(curl, CURLOPT_RESUME_FROM_LARGE, offset);
    return curl_easy_perform(curl);
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        let m = &markers[0];
        assert_eq!(m.function_name, "download_resume");
        assert_eq!(m.kind, MarkerKind::ResumeOffset);

        if let MarkerEvidence::ResumeOffset {
            api_call,
            option_name,
            offset_source,
        } = &m.evidence
        {
            assert_eq!(api_call, "curl_easy_setopt");
            assert_eq!(option_name.as_deref(), Some("CURLOPT_RESUME_FROM_LARGE"));
            assert_eq!(offset_source.as_deref(), Some("offset"));
        } else {
            panic!("expected ResumeOffset evidence");
        }
    }

    #[test]
    fn test_curl_resume_from_detected() {
        let source = r#"
int download_small(CURL *curl, long offset) {
    curl_easy_setopt(curl, CURLOPT_RESUME_FROM, offset);
    return curl_easy_perform(curl);
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 1);
        if let MarkerEvidence::ResumeOffset {
            option_name, ..
        } = &markers[0].evidence
        {
            assert_eq!(option_name.as_deref(), Some("CURLOPT_RESUME_FROM"));
        } else {
            panic!("expected ResumeOffset evidence");
        }
    }

    #[test]
    fn test_other_curl_options_not_detected() {
        let source = r#"
void setup_curl(CURL *curl) {
    curl_easy_setopt(curl, CURLOPT_URL, "http://example.com");
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        // Other curl options should NOT be detected
        assert_eq!(markers.len(), 0);
    }

    // =========================================================================
    // Multiple markers tests
    // =========================================================================

    #[test]
    fn test_multiple_markers_in_function() {
        let source = r#"
int download_with_resume_retry(CURL *curl, size_t *offset) {
    int result;
    do {
        curl_easy_setopt(curl, CURLOPT_RESUME_FROM_LARGE, *offset);
        result = curl_easy_perform(curl);
        if (result != CURLE_OK) {
            sleep(5);
        }
    } while (result != CURLE_OK);
    return result;
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        // Should have both RETRY_LOOP and RESUME_OFFSET
        assert_eq!(markers.len(), 2);

        let kinds: Vec<_> = markers.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&MarkerKind::RetryLoop));
        assert!(kinds.contains(&MarkerKind::ResumeOffset));
    }

    #[test]
    fn test_nested_loops_both_detected() {
        let source = r#"
void nested_retry(void) {
    for (int i = 0; i < 3; i++) {
        sleep(1);
        for (int j = 0; j < 5; j++) {
            if (try_fast())
                break;
            usleep(10000);
        }
    }
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        // Both loops should be detected
        assert_eq!(markers.len(), 2);
    }

    // =========================================================================
    // False positive tests
    // =========================================================================

    #[test]
    fn test_init_function_no_markers() {
        let source = r#"
int curl_init(void) {
    CURL *curl = curl_easy_init();
    curl_easy_setopt(curl, CURLOPT_URL, "http://example.com");
    return 0;
}
"#;
        let tree = parse_c(source);
        let markers = extract_behavioral_markers(&tree, source.as_bytes(), "test.c", "test");

        assert_eq!(markers.len(), 0);
    }
}
