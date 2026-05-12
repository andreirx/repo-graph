//! Receiver expression locator using tree-sitter.
//!
//! This module extracts the receiver expression from call expressions
//! for semantic type resolution. It solves the
//! `calls_this_wildcard_method_needs_type_info` problem where the
//! cursor position points at the method name, but we need the type
//! of the receiver expression.
//!
//! # Problem
//!
//! For `this.field.method()`, tsserver quickinfo at `method` returns
//! information about the method, not the receiver `this.field`. We need
//! to find the receiver span and query its type separately.
//!
//! # Architecture
//!
//! - **Syntax localization**: tree-sitter (this module)
//! - **Semantic typing**: tsserver (client.rs)
//!
//! This split keeps syntax extraction deterministic and testable while
//! using tsserver only for what it's good at (semantic types).
//!
//! # Supported patterns
//!
//! - `obj.method()` → receiver is `obj`
//! - `this.field.method()` → receiver is `this.field`
//! - `a.b.c.method()` → receiver is `a.b.c`
//! - `obj?.method()` → receiver is `obj`
//! - `this?.field?.method()` → receiver is `this?.field`
//!
//! # Unsupported patterns (explicit failure)
//!
//! - `func()` — no receiver
//! - `(expr).method()` — parenthesized, complex to extract
//! - `arr[0].method()` — subscript receiver, tsserver may not resolve
//!
//! # Maturity
//!
//! PROTOTYPE. Covers common patterns, not exhaustive.

use tree_sitter::{Language, Parser, Point, Tree};

// ─────────────────────────────────────────────────────────────────────────────
// Public Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of receiver localization.
///
/// All coordinates are **0-based** to match `EligibleEdge` and
/// `TsServerSession::resolve_type()` contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverLocation {
    /// Successfully located the receiver expression.
    Found {
        /// 0-based line number of receiver start.
        line: u32,
        /// 0-based column of receiver start.
        column: u32,
        /// The receiver expression text (for diagnostics).
        text: String,
    },
    /// No receiver expression (e.g., plain function call).
    NoReceiver,
    /// Receiver pattern not supported (complex expression).
    UnsupportedPattern {
        /// Description of why localization failed.
        reason: String,
    },
    /// Parse error or position not found.
    ParseError { reason: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// Locator
// ─────────────────────────────────────────────────────────────────────────────

/// Locates receiver expressions in TypeScript/JavaScript code.
pub struct ReceiverLocator {
    parser: Parser,
}

impl ReceiverLocator {
    /// Create a new locator for TypeScript files.
    pub fn new_typescript() -> Result<Self, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
        parser
            .set_language(&Language::from(language))
            .map_err(|e| format!("failed to set TypeScript language: {}", e))?;
        Ok(Self { parser })
    }

    /// Create a new locator for TSX files.
    pub fn new_tsx() -> Result<Self, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TSX;
        parser
            .set_language(&Language::from(language))
            .map_err(|e| format!("failed to set TSX language: {}", e))?;
        Ok(Self { parser })
    }

    /// Locate the receiver expression for a call at the given position.
    ///
    /// `line` and `column` are **0-based** (matching `EligibleEdge` coordinates).
    /// The position should be at or near the method name in a call expression.
    ///
    /// Returns receiver location with **0-based** coordinates.
    pub fn locate_receiver(&mut self, source: &str, line: u32, column: u32) -> ReceiverLocation {
        // Parse the source
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => {
                return ReceiverLocation::ParseError {
                    reason: "tree-sitter parse returned None".to_string(),
                }
            }
        };

        // Input is already 0-based, tree-sitter uses 0-based
        let point = Point::new(line as usize, column as usize);

        locate_receiver_at_point(&tree, source, point)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core Algorithm
// ─────────────────────────────────────────────────────────────────────────────

fn locate_receiver_at_point(tree: &Tree, source: &str, point: Point) -> ReceiverLocation {
    let root = tree.root_node();

    // Find the deepest node at the given point
    let node = match root.descendant_for_point_range(point, point) {
        Some(n) => n,
        None => {
            return ReceiverLocation::ParseError {
                reason: "no node at position".to_string(),
            }
        }
    };

    // Walk up to find the call_expression
    let mut current = node;
    let call_expr = loop {
        if current.kind() == "call_expression" {
            break current;
        }
        match current.parent() {
            Some(p) => current = p,
            None => {
                return ReceiverLocation::ParseError {
                    reason: format!(
                        "no call_expression found above node '{}' at {}:{}",
                        node.kind(),
                        point.row + 1,
                        point.column + 1
                    ),
                }
            }
        }
    };

    // Get the callee (function being called)
    // In `obj.method()`, callee is `obj.method` (member_expression)
    // In `func()`, callee is `func` (identifier)
    let callee = match call_expr.child_by_field_name("function") {
        Some(c) => c,
        None => {
            return ReceiverLocation::ParseError {
                reason: "call_expression has no function child".to_string(),
            }
        }
    };

    extract_receiver_from_callee(callee, source)
}

fn extract_receiver_from_callee(callee: tree_sitter::Node, source: &str) -> ReceiverLocation {
    match callee.kind() {
        // `obj.method` or `this.field.method`
        "member_expression" => extract_receiver_from_member_expression(callee, source),

        // `obj?.method` (optional chaining)
        "optional_chain_expression" => {
            // The structure is: optional_chain_expression -> member_expression
            // Need to find the object inside
            extract_receiver_from_optional_chain(callee, source)
        }

        // Plain function call like `func()` — no receiver
        "identifier" | "super" => ReceiverLocation::NoReceiver,

        // Parenthesized expression like `(obj).method()`
        "parenthesized_expression" => ReceiverLocation::UnsupportedPattern {
            reason: "parenthesized_callee".to_string(),
        },

        // Subscript expression like `arr[0].method()` — complex
        "subscript_expression" => ReceiverLocation::UnsupportedPattern {
            reason: "subscript_callee".to_string(),
        },

        // Call expression like `getObj().method()` — would need to recurse
        "call_expression" => ReceiverLocation::UnsupportedPattern {
            reason: "call_expression_callee".to_string(),
        },

        // Await expression like `(await promise).method()`
        "await_expression" => ReceiverLocation::UnsupportedPattern {
            reason: "await_callee".to_string(),
        },

        // Unknown pattern
        other => ReceiverLocation::UnsupportedPattern {
            reason: format!("unknown_callee_kind:{}", other),
        },
    }
}

fn extract_receiver_from_member_expression(
    member_expr: tree_sitter::Node,
    source: &str,
) -> ReceiverLocation {
    // member_expression has 'object' and 'property' children
    // For `obj.method`, object is `obj`
    // For `this.field.method`, object is `this.field` (another member_expression)

    let object = match member_expr.child_by_field_name("object") {
        Some(o) => o,
        None => {
            return ReceiverLocation::ParseError {
                reason: "member_expression has no object".to_string(),
            }
        }
    };

    // Check if the object is something we can handle
    match object.kind() {
        // Simple cases: identifier, this, super
        "identifier" | "this" | "super" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Chained member expression: `a.b.c` -> object is `a.b`
        "member_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Optional chain: `obj?.field` as object
        "optional_chain_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Subscript: `arr[0]` as object — might work, try it
        "subscript_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Call expression as object: `getObj().method()`
        "call_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Parenthesized: `(obj).method`
        "parenthesized_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // New expression: `new Foo().method()`
        "new_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Non-null assertion: `obj!.method`
        "non_null_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Await expression: `(await p).method`
        "await_expression" => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: text.to_string(),
            }
        }

        // Unknown object type — try anyway
        other => {
            let start = object.start_position();
            let text = &source[object.byte_range()];
            // Log warning but still return the location
            ReceiverLocation::Found {
                line: start.row as u32,
                column: start.column as u32,
                text: format!("[{}] {}", other, text),
            }
        }
    }
}

fn extract_receiver_from_optional_chain(
    optional_chain: tree_sitter::Node,
    source: &str,
) -> ReceiverLocation {
    // optional_chain_expression wraps a member_expression for `obj?.method`
    // The structure varies; try to find the object

    // First child might be the object
    if let Some(first_child) = optional_chain.child(0) {
        if first_child.kind() == "member_expression" {
            return extract_receiver_from_member_expression(first_child, source);
        }

        // It might be the object directly
        let start = first_child.start_position();
        let text = &source[first_child.byte_range()];

        // Skip if it's the operator `?.`
        if text == "?." {
            // Try second child
            if let Some(second) = optional_chain.child(1) {
                if second.kind() == "member_expression" {
                    return extract_receiver_from_member_expression(second, source);
                }
            }
            return ReceiverLocation::UnsupportedPattern {
                reason: "optional_chain_structure".to_string(),
            };
        }

        return ReceiverLocation::Found {
            line: start.row as u32,
            column: start.column as u32,
            text: text.to_string(),
        };
    }

    ReceiverLocation::UnsupportedPattern {
        reason: "empty_optional_chain".to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn locate(source: &str, line: u32, column: u32) -> ReceiverLocation {
        let mut locator = ReceiverLocator::new_typescript().unwrap();
        locator.locate_receiver(source, line, column)
    }

    #[test]
    fn test_simple_method_call() {
        // obj.method()
        // 0123456789...
        //     ^ col 4 (0-based)
        let source = "obj.method()";
        let result = locate(source, 0, 4); // 0-based: line 0, col 4 at 'method'

        match result {
            ReceiverLocation::Found { text, line, column } => {
                assert_eq!(text, "obj");
                assert_eq!(line, 0);
                assert_eq!(column, 0);
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_this_field_method() {
        // this.field.method()
        // 0123456789012345678...
        //            ^ col 11 (0-based)
        let source = "this.field.method()";
        let result = locate(source, 0, 11); // 0-based: line 0, col 11 at 'method'

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "this.field");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_chained_receivers() {
        // a.b.c.method()
        // 01234567890123...
        //       ^ col 6 (0-based)
        let source = "a.b.c.method()";
        let result = locate(source, 0, 6); // 0-based: line 0, col 6 at 'method'

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "a.b.c");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_this_method() {
        // this.method()
        // 0123456789...
        //      ^ col 5 (0-based)
        let source = "this.method()";
        let result = locate(source, 0, 5); // 0-based: line 0, col 5 at 'method'

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "this");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_plain_function_call() {
        // func()
        let source = "func()";
        let result = locate(source, 0, 0); // 0-based

        assert!(matches!(result, ReceiverLocation::NoReceiver));
    }

    #[test]
    fn test_multiline() {
        // Line 0: ""
        // Line 1: "class Foo {"
        // Line 2: "    bar() {"
        // Line 3: "        this.field.method();"
        //         01234567890123456789012345678
        //                     ^ col 8 is 'this', col 19 is 'method'
        // Line 4: "    }"
        // Line 5: "}"
        let source = r#"
class Foo {
    bar() {
        this.field.method();
    }
}
"#;
        // 0-based: line 3, col 19 at 'method'
        let result = locate(source, 3, 19);

        match result {
            ReceiverLocation::Found { text, line, .. } => {
                assert_eq!(text, "this.field");
                assert_eq!(line, 3);
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_super_method() {
        // super.method()
        // 01234567890123...
        //       ^ col 6 (0-based)
        let source = "super.method()";
        let result = locate(source, 0, 6); // 0-based

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "super");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_new_expression_receiver() {
        // new Foo().method()
        // 0123456789012345678
        //           ^ col 10 (0-based)
        let source = "new Foo().method()";
        let result = locate(source, 0, 10); // 0-based

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "new Foo()");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_call_expression_receiver() {
        // getObj().method()
        // 01234567890123456
        //          ^ col 9 (0-based)
        let source = "getObj().method()";
        let result = locate(source, 0, 9); // 0-based

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "getObj()");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_subscript_receiver() {
        // arr[0].method()
        // 012345678901234
        //        ^ col 7 (0-based)
        let source = "arr[0].method()";
        let result = locate(source, 0, 7); // 0-based

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "arr[0]");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_non_null_assertion() {
        // obj!.method()
        // 0123456789012
        //      ^ col 5 (0-based)
        let source = "obj!.method()";
        let result = locate(source, 0, 5); // 0-based

        match result {
            ReceiverLocation::Found { text, .. } => {
                assert_eq!(text, "obj!");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_position_not_in_call() {
        let source = "const x = 1;";
        let result = locate(source, 0, 0); // 0-based

        assert!(matches!(result, ReceiverLocation::ParseError { .. }));
    }
}
