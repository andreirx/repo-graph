//! AMQP / RabbitMQ boundary interaction detector for TypeScript/JavaScript.
//!
//! Detects amqplib patterns that indicate message broker boundary interactions.
//!
//! MB-1A scope:
//!   - sendToQueue (direct queue publish)
//!   - publish (exchange publish)
//!   - consume (queue consumption)
//!   - assertQueue, assertExchange, bindQueue (declaration/setup)
//!
//! Scope guard:
//!   - ONLY emits hints when file has direct `amqplib` import/require
//!   - Generic `.publish()/.consume()/.assertQueue()` on non-amqplib objects
//!     are NOT detected (prevents false positives)
//!
//! Out of scope:
//!   - Spring AMQP / framework wrappers
//!   - Wrapper modules that re-export amqplib channels
//!   - Connection lifecycle details
//!   - Publisher confirms
//!   - DLQ / retry topology
//!
//! Contract: docs/slices/mb-1a-rabbitmq-amqp.md

use repo_graph_classification::types::SourceLocation;

/// Raw AMQP boundary call site extracted from TS/JS source.
///
/// This is the extractor-side DTO. The emitter converts this to
/// `BoundaryCallsite` from boundary-interaction-extractor.
#[derive(Debug, Clone)]
pub struct RawAmqpBoundaryCall {
    /// Method name (e.g., "sendToQueue", "consume", "assertQueue").
    pub function_name: String,

    /// Source location of the call expression.
    pub location: SourceLocation,

    /// Enclosing function/method name (for stable key construction).
    /// Empty string for top-level calls.
    pub enclosing_function: String,

    /// Queue name literal if visible.
    pub queue_name: Option<String>,

    /// Exchange name literal if visible.
    pub exchange_name: Option<String>,

    /// Routing key literal if visible.
    pub routing_key: Option<String>,
}

/// AMQP publisher methods (provider direction).
const AMQP_PUBLISHER_METHODS: &[&str] = &["sendToQueue", "publish"];

/// AMQP consumer methods (consumer direction).
const AMQP_CONSUMER_METHODS: &[&str] = &["consume"];

/// AMQP declaration/setup methods (bidirectional).
const AMQP_DECLARATION_METHODS: &[&str] = &["assertQueue", "assertExchange", "bindQueue"];

/// Check if a method name is an AMQP boundary candidate.
pub fn is_amqp_boundary_pattern(name: &str) -> bool {
    AMQP_PUBLISHER_METHODS.contains(&name)
        || AMQP_CONSUMER_METHODS.contains(&name)
        || AMQP_DECLARATION_METHODS.contains(&name)
}

/// Check if the file has a direct amqplib import/require.
///
/// Accepts:
/// - `import amqp from 'amqplib'`
/// - `import * as amqp from 'amqplib'`
/// - `import { ... } from 'amqplib'`
/// - `const amqp = require('amqplib')`
/// - `require('amqplib')` (bare require)
///
/// Returns `true` if any amqplib import is found.
pub fn has_amqplib_import(root: &tree_sitter::Node, src: &[u8]) -> bool {
    // PERSIST-RECURSION-1: iterative pre-order search (was recursive on AST
    // depth). The result is an order-insensitive boolean OR, so traversal order
    // is immaterial; the first `amqplib` import/require short-circuits. A
    // non-matching import/call still descends — a `require('amqplib')` can be
    // nested inside a larger expression — because the visitor visits every node.
    let mut found = false;
    crate::walk::visit_preorder(*root, |node| {
        match node.kind() {
            // ESM import: import ... from 'amqplib'
            "import_statement" => {
                if let Some(source) = node.child_by_field_name("source") {
                    if let Ok(text) = source.utf8_text(src) {
                        let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                        if trimmed == "amqplib" {
                            found = true;
                            return std::ops::ControlFlow::Break(());
                        }
                    }
                }
            }

            // CommonJS require: require('amqplib')
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "identifier" {
                        if let Ok(name) = function.utf8_text(src) {
                            if name == "require" {
                                if let Some(args) = node.child_by_field_name("arguments") {
                                    // Check first argument
                                    for i in 0..args.child_count() {
                                        if let Some(arg) = args.child(i) {
                                            if arg.kind() == "string" {
                                                if let Ok(text) = arg.utf8_text(src) {
                                                    let trimmed = text
                                                        .trim_matches(|c| c == '"' || c == '\'');
                                                    if trimmed == "amqplib" {
                                                        found = true;
                                                        return std::ops::ControlFlow::Break(());
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
        std::ops::ControlFlow::Continue(())
    });
    found
}

/// Extract AMQP boundary calls from a parsed TS/JS file.
///
/// `root` is the tree-sitter root node for the file.
/// `src` is the source text as bytes.
/// `_file_path` is the repo-relative path (for diagnostics).
///
/// Returns a list of detected AMQP boundary calls.
///
/// **Scope guard:** Returns empty if the file does not have a direct
/// `amqplib` import/require. This prevents false positives from generic
/// `.publish()/.consume()/.assertQueue()` methods on non-AMQP objects.
pub fn extract_amqp_boundary_calls(
    root: &tree_sitter::Node,
    src: &[u8],
    _file_path: &str,
) -> Vec<RawAmqpBoundaryCall> {
    // Scope guard: only extract if file has direct amqplib import
    if !has_amqplib_import(root, src) {
        return Vec::new();
    }

    // PERSIST-RECURSION-1: iterative pre-order walk with enclosing-function
    // tracking (was recursive on AST depth). Byte-for-byte identical output.
    let mut results = Vec::new();
    crate::walk::visit_with_enclosing(*root, src, |node, enclosing| {
        // Detect `channel.sendToQueue(...)`, `channel.publish(...)`, etc.
        if node.kind() == "call_expression" {
            if let Some(call) = try_extract_amqp_call(&node, src, enclosing) {
                results.push(call);
            }
        }
    });

    results
}

/// Try to extract an AMQP boundary call from a `call_expression` node.
///
/// Handles:
/// - `channel.sendToQueue(queue, msg)`
/// - `channel.publish(exchange, routingKey, msg)`
/// - `channel.consume(queue, handler, options)`
/// - `channel.assertQueue(queue, options)`
/// - `channel.assertExchange(exchange, type, options)`
/// - `channel.bindQueue(queue, exchange, routingKey)`
fn try_extract_amqp_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
) -> Option<RawAmqpBoundaryCall> {
    // call_expression structure: function arguments
    let function = node.child_by_field_name("function")?;

    // Only handle member expressions: receiver.method(...)
    if function.kind() != "member_expression" {
        return None;
    }

    let property = function.child_by_field_name("property")?;
    let method_name = property.utf8_text(src).ok()?;

    // Check if this is an AMQP boundary method
    if !is_amqp_boundary_pattern(method_name) {
        return None;
    }

    // Extract arguments based on method type
    let arguments = node.child_by_field_name("arguments")?;
    let (queue_name, exchange_name, routing_key) =
        extract_amqp_arguments(method_name, &arguments, src);

    let start = node.start_position();
    let end = node.end_position();

    Some(RawAmqpBoundaryCall {
        function_name: method_name.to_string(),
        location: SourceLocation {
            line_start: (start.row + 1) as i64,
            col_start: start.column as i64,
            line_end: (end.row + 1) as i64,
            col_end: end.column as i64,
        },
        enclosing_function: enclosing_function.to_string(),
        queue_name,
        exchange_name,
        routing_key,
    })
}

/// Extract queue/exchange/routing-key from AMQP call arguments.
fn extract_amqp_arguments(
    method_name: &str,
    arguments: &tree_sitter::Node,
    src: &[u8],
) -> (Option<String>, Option<String>, Option<String>) {
    let args = collect_arguments(arguments, src);

    match method_name {
        // sendToQueue(queue, msg, options?)
        "sendToQueue" => (args.first().cloned(), None, None),

        // publish(exchange, routingKey, msg, options?)
        "publish" => (None, args.first().cloned(), args.get(1).cloned()),

        // consume(queue, handler, options?)
        "consume" => (args.first().cloned(), None, None),

        // assertQueue(queue, options?)
        "assertQueue" => (args.first().cloned(), None, None),

        // assertExchange(exchange, type, options?)
        "assertExchange" => (None, args.first().cloned(), None),

        // bindQueue(queue, exchange, routingKey)
        "bindQueue" => (
            args.first().cloned(),
            args.get(1).cloned(),
            args.get(2).cloned(),
        ),

        _ => (None, None, None),
    }
}

/// Collect string literal arguments from an arguments node.
fn collect_arguments(arguments: &tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let mut result = Vec::new();

    for i in 0..arguments.child_count() {
        if let Some(arg) = arguments.child(i) {
            // Skip punctuation (parentheses, commas)
            if arg.kind() == "(" || arg.kind() == ")" || arg.kind() == "," {
                continue;
            }

            match arg.kind() {
                "string" | "template_string" => {
                    if let Ok(text) = arg.utf8_text(src) {
                        // Strip quotes
                        let trimmed = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                        result.push(trimmed.to_string());
                    }
                }
                "identifier" => {
                    // Variable reference - capture with ${} notation
                    if let Ok(name) = arg.utf8_text(src) {
                        result.push(format!("${{{}}}", name));
                    }
                }
                _ => {
                    // Non-literal, non-identifier - push empty placeholder
                    result.push(String::new());
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(source: &str) -> Vec<RawAmqpBoundaryCall> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        extract_amqp_boundary_calls(&tree.root_node(), source.as_bytes(), "test.ts")
    }

    fn parse_and_check_import(source: &str) -> bool {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        has_amqplib_import(&tree.root_node(), source.as_bytes())
    }

    // ── Import guard tests ──────────────────────────────────────────────────

    #[test]
    fn import_guard_esm_default() {
        let source = r#"import amqp from 'amqplib';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_namespace() {
        let source = r#"import * as amqp from 'amqplib';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_named() {
        let source = r#"import { connect } from 'amqplib';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_commonjs_require() {
        let source = r#"const amqp = require('amqplib');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_bare_require() {
        let source = r#"require('amqplib');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_no_amqplib() {
        let source = r#"import foo from 'some-other-lib';"#;
        assert!(!parse_and_check_import(source));
    }

    // ── Scope guard negative test (P1 regression) ───────────────────────────

    #[test]
    fn no_detection_without_amqplib_import() {
        // Generic object with AMQP-like method names but NO amqplib import.
        // This MUST return empty to prevent false positives.
        let source = r#"
            const bus = {
                publish: (x) => console.log(x),
                consume: (x) => console.log(x),
                assertQueue: (x) => console.log(x),
            };
            bus.publish('hello');
            bus.consume('world');
            bus.assertQueue('queue');
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "should NOT emit AMQP surfaces without amqplib import; got {:?}",
            calls
        );
    }

    // ── Positive detection tests (with amqplib import) ──────────────────────

    #[test]
    fn detect_send_to_queue() {
        let source = r#"
            import amqp from 'amqplib';
            channel.sendToQueue('hello', Buffer.from('msg'));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sendToQueue");
        assert_eq!(calls[0].queue_name, Some("hello".to_string()));
    }

    #[test]
    fn detect_publish_to_exchange() {
        let source = r#"
            import amqp from 'amqplib';
            channel.publish('logs', 'info', Buffer.from('msg'));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "publish");
        assert_eq!(calls[0].exchange_name, Some("logs".to_string()));
        assert_eq!(calls[0].routing_key, Some("info".to_string()));
    }

    #[test]
    fn detect_consume() {
        let source = r#"
            import amqp from 'amqplib';
            channel.consume('hello', function(msg) { console.log(msg); });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "consume");
        assert_eq!(calls[0].queue_name, Some("hello".to_string()));
    }

    #[test]
    fn detect_assert_queue() {
        let source = r#"
            import amqp from 'amqplib';
            await channel.assertQueue('hello', { durable: true });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "assertQueue");
        assert_eq!(calls[0].queue_name, Some("hello".to_string()));
    }

    #[test]
    fn detect_assert_exchange() {
        let source = r#"
            import amqp from 'amqplib';
            await channel.assertExchange('logs', 'fanout', { durable: false });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "assertExchange");
        assert_eq!(calls[0].exchange_name, Some("logs".to_string()));
    }

    #[test]
    fn detect_bind_queue() {
        let source = r#"
            import amqp from 'amqplib';
            channel.bindQueue('my-queue', 'logs', 'info');
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "bindQueue");
        assert_eq!(calls[0].queue_name, Some("my-queue".to_string()));
        assert_eq!(calls[0].exchange_name, Some("logs".to_string()));
        assert_eq!(calls[0].routing_key, Some("info".to_string()));
    }

    #[test]
    fn detect_enclosing_function() {
        let source = r#"
            import amqp from 'amqplib';
            async function publishMessage() {
                channel.sendToQueue('hello', Buffer.from('msg'));
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].enclosing_function, "publishMessage");
    }

    #[test]
    fn detect_multiple_patterns() {
        let source = r#"
            import amqp from 'amqplib';
            await channel.assertQueue('hello', { durable: true });
            channel.sendToQueue('hello', Buffer.from('msg'));
            channel.consume('hello', (msg) => console.log(msg));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 3);

        let names: Vec<_> = calls.iter().map(|c| c.function_name.as_str()).collect();
        assert!(names.contains(&"assertQueue"));
        assert!(names.contains(&"sendToQueue"));
        assert!(names.contains(&"consume"));
    }

    #[test]
    fn ignore_non_amqp_patterns_with_import() {
        // Even with amqplib import, non-AMQP method names are not detected
        let source = r#"
            import amqp from 'amqplib';
            console.log("hello");
            arr.push(1);
            channel.close();
        "#;
        let calls = parse_and_extract(source);
        assert!(calls.is_empty());
    }

    #[test]
    fn detect_variable_queue_name() {
        let source = r#"
            import amqp from 'amqplib';
            const queue = 'hello';
            channel.sendToQueue(queue, Buffer.from('msg'));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sendToQueue");
        // Variable reference is captured with ${} notation
        assert_eq!(calls[0].queue_name, Some("${queue}".to_string()));
    }

    #[test]
    fn commonjs_require_enables_detection() {
        let source = r#"
            const amqp = require('amqplib');
            channel.sendToQueue('hello', Buffer.from('msg'));
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sendToQueue");
    }
}
