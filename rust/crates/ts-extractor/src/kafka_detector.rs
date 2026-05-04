//! Kafka boundary interaction detector for TypeScript/JavaScript.
//!
//! Detects kafkajs patterns that indicate message broker boundary interactions.
//!
//! MB-2A shipped scope:
//!   - producer.send({ topic, messages }) - producer/publisher
//!   - consumer.subscribe({ topic }) - consumer subscription
//!   - consumer.subscribe({ topics: [...] }) - multi-topic subscription
//!
//! Scope guards (triple):
//!   1. File must have direct `kafkajs` import/require
//!   2. Receiver must be a tracked kafkajs factory result:
//!      - `const producer = kafka.producer()` → tracks `producer`
//!      - `const consumer = kafka.consumer({...})` → tracks `consumer`
//!   3. Call must have extractable topic evidence (topic or topics argument)
//!
//! Why receiver provenance is required:
//!   - `send`, `subscribe` are extremely generic method names
//!   - File-level import guard is insufficient — kafkajs may be imported
//!     for setup while other code uses `.send()` on unrelated objects
//!   - Topic-shaped arguments are not Kafka-specific either
//!   - Only factory-tracked receivers prove the call is actually Kafka
//!
//! Intentionally NOT detected (deferred):
//!   - `producer.sendBatch({ topicMessages })` — nested `topicMessages[].topic`
//!     extraction not implemented
//!   - `consumer.run({ eachMessage, eachBatch })` — no topic evidence,
//!     deferred for later correlation with subscribe()
//!   - Calls without extractable topic argument
//!   - Calls on receivers not assigned from `*.producer()` or `*.consumer()`
//!
//! Provenance tracking scope (deliberately narrow):
//!   - Same file only
//!   - Direct assignment only (`const x = kafka.producer()`)
//!   - Direct receiver identifier only (`x.send(...)`)
//!   - No interprocedural flow
//!   - No wrapper inference
//!   - No object property aliasing
//!
//! Out of scope:
//!   - Spring Kafka / Java kafka-clients
//!   - Partition/offset semantics
//!   - Consumer group reconstruction
//!   - Transactions / exactly-once
//!   - Schema registry / Avro/Protobuf payloads
//!
//! Contract: docs/slices/mb-2a-kafka-topic.md

use std::collections::HashSet;

use repo_graph_classification::types::SourceLocation;

/// Raw Kafka boundary call site extracted from TS/JS source.
///
/// This is the extractor-side DTO. The emitter converts this to
/// `BoundaryCallsite` from boundary-interaction-extractor.
#[derive(Debug, Clone)]
pub struct RawKafkaBoundaryCall {
    /// Method name (e.g., "send", "subscribe", "run").
    pub function_name: String,

    /// Source location of the call expression.
    pub location: SourceLocation,

    /// Enclosing function/method name (for stable key construction).
    /// Empty string for top-level calls.
    pub enclosing_function: String,

    /// Topic name literal if visible (from send/subscribe).
    pub topic: Option<String>,

    /// Multiple topics if visible (from subscribe({ topics: [...] })).
    pub topics: Option<Vec<String>>,

    /// Consumer group ID if visible.
    pub group_id: Option<String>,

    /// Callback mode for run() calls (eachMessage, eachBatch).
    pub callback_mode: Option<String>,
}

/// Kafka producer methods (provider direction).
/// Note: `send` is a generic name — receiver provenance + topic evidence required.
/// Note: `sendBatch` is intentionally excluded — its nested `topicMessages[].topic`
/// structure requires array extraction not yet implemented.
const KAFKA_PRODUCER_METHODS: &[&str] = &["send"];

/// Kafka consumer methods (consumer direction).
/// Note: `run` is intentionally excluded — it provides no topic evidence and
/// should only enrich subscribe() in future correlation work.
const KAFKA_CONSUMER_METHODS: &[&str] = &["subscribe"];

/// Check if a method name is a Kafka boundary candidate.
/// Note: This is a first-pass filter. Actual emission requires receiver provenance + topic evidence.
pub fn is_kafka_boundary_pattern(name: &str) -> bool {
    KAFKA_PRODUCER_METHODS.contains(&name) || KAFKA_CONSUMER_METHODS.contains(&name)
}

/// Tracked kafkajs factory variables in a file.
///
/// Built by scanning for assignments like:
/// - `const producer = kafka.producer()` → producer_vars
/// - `const consumer = kafka.consumer({...})` → consumer_vars
#[derive(Debug, Default)]
struct KafkaFactoryVars {
    /// Variables assigned from `*.producer()` calls
    producer_vars: HashSet<String>,
    /// Variables assigned from `*.consumer()` calls
    consumer_vars: HashSet<String>,
}

/// Extract kafkajs factory variable assignments from the AST.
///
/// Tracks same-file, direct assignments only:
/// - `const producer = kafka.producer()` → producer_vars
/// - `const consumer = kafka.consumer({...})` → consumer_vars
/// - `let x = kafkaClient.producer()` → producer_vars
///
/// Does NOT track:
/// - Cross-file receivers
/// - Wrapper functions returning producers/consumers
/// - Object property assignments
/// - Destructured assignments
fn extract_kafka_factory_vars(root: &tree_sitter::Node, src: &[u8]) -> KafkaFactoryVars {
    let mut vars = KafkaFactoryVars::default();
    extract_factory_vars_recursive(root, src, &mut vars);
    vars
}

fn extract_factory_vars_recursive(
    node: &tree_sitter::Node,
    src: &[u8],
    vars: &mut KafkaFactoryVars,
) {
    // Look for variable_declarator: const x = something.producer() or something.consumer()
    if node.kind() == "variable_declarator" {
        if let (Some(name_node), Some(value_node)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            // Only track simple identifier names (not destructuring patterns)
            if name_node.kind() == "identifier" {
                // Check if value is a call expression
                if value_node.kind() == "call_expression" {
                    if let Some(function) = value_node.child_by_field_name("function") {
                        // Check if it's a member expression: x.producer() or x.consumer()
                        if function.kind() == "member_expression" {
                            if let Some(property) = function.child_by_field_name("property") {
                                if let Ok(method_name) = property.utf8_text(src) {
                                    if let Ok(var_name) = name_node.utf8_text(src) {
                                        match method_name {
                                            "producer" => {
                                                vars.producer_vars.insert(var_name.to_string());
                                            }
                                            "consumer" => {
                                                vars.consumer_vars.insert(var_name.to_string());
                                            }
                                            _ => {}
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
            extract_factory_vars_recursive(&child, src, vars);
        }
    }
}

/// Check if the file has a direct kafkajs import/require.
///
/// Accepts:
/// - `import { Kafka } from 'kafkajs'`
/// - `import Kafka from 'kafkajs'`
/// - `import * as kafka from 'kafkajs'`
/// - `const { Kafka } = require('kafkajs')`
/// - `const Kafka = require('kafkajs')`
/// - `require('kafkajs')` (bare require)
///
/// Returns `true` if any kafkajs import is found.
pub fn has_kafkajs_import(root: &tree_sitter::Node, src: &[u8]) -> bool {
    has_kafkajs_import_recursive(root, src)
}

fn has_kafkajs_import_recursive(node: &tree_sitter::Node, src: &[u8]) -> bool {
    match node.kind() {
        // ESM import: import ... from 'kafkajs'
        "import_statement" => {
            if let Some(source) = node.child_by_field_name("source") {
                if let Ok(text) = source.utf8_text(src) {
                    let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
                    if trimmed == "kafkajs" {
                        return true;
                    }
                }
            }
        }

        // CommonJS require: require('kafkajs')
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
                                                if trimmed == "kafkajs" {
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
            if has_kafkajs_import_recursive(&child, src) {
                return true;
            }
        }
    }

    false
}

/// Extract Kafka boundary calls from a parsed TS/JS file.
///
/// `root` is the tree-sitter root node for the file.
/// `src` is the source text as bytes.
/// `_file_path` is the repo-relative path (for diagnostics).
///
/// Returns a list of detected Kafka boundary calls.
///
/// **Scope guards (triple):**
/// 1. File must have direct `kafkajs` import/require
/// 2. Receiver must be a tracked factory result (`*.producer()` or `*.consumer()`)
/// 3. Call must have extractable topic evidence
pub fn extract_kafka_boundary_calls(
    root: &tree_sitter::Node,
    src: &[u8],
    _file_path: &str,
) -> Vec<RawKafkaBoundaryCall> {
    // Guard 1: only extract if file has direct kafkajs import
    if !has_kafkajs_import(root, src) {
        return Vec::new();
    }

    // Guard 2 prep: extract tracked factory variables
    let factory_vars = extract_kafka_factory_vars(root, src);

    let mut results = Vec::new();
    let mut enclosing_function = String::new();

    extract_from_node(root, src, &mut enclosing_function, &factory_vars, &mut results);

    results
}

fn extract_from_node(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &mut String,
    factory_vars: &KafkaFactoryVars,
    results: &mut Vec<RawKafkaBoundaryCall>,
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
                    extract_from_node(&child, src, enclosing_function, factory_vars, results);
                }
            }

            // Restore enclosing function
            *enclosing_function = prev_enclosing;
            return; // Already recursed
        }

        // Detect `producer.send(...)`, `consumer.subscribe(...)`, etc.
        "call_expression" => {
            if let Some(call) = try_extract_kafka_call(node, src, enclosing_function, factory_vars) {
                results.push(call);
            }
        }

        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, src, enclosing_function, factory_vars, results);
        }
    }
}

/// Try to extract a Kafka boundary call from a `call_expression` node.
///
/// Handles:
/// - `producer.send({ topic, messages })`
/// - `consumer.subscribe({ topic })`
/// - `consumer.subscribe({ topics: [...] })`
///
/// **Guard 2 (receiver provenance):** Returns None if receiver is not a tracked
/// factory variable. The receiver must have been assigned from `*.producer()` or
/// `*.consumer()` in this file.
///
/// **Guard 3 (topic evidence):** Returns None if no topic/topics argument is
/// extractable. This prevents false positives from generic `.send()`/`.subscribe()`
/// calls on non-Kafka objects.
fn try_extract_kafka_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
    factory_vars: &KafkaFactoryVars,
) -> Option<RawKafkaBoundaryCall> {
    // call_expression structure: function arguments
    let function = node.child_by_field_name("function")?;

    // Only handle member expressions: receiver.method(...)
    if function.kind() != "member_expression" {
        return None;
    }

    let property = function.child_by_field_name("property")?;
    let method_name = property.utf8_text(src).ok()?;

    // Check if this is a Kafka boundary method
    if !is_kafka_boundary_pattern(method_name) {
        return None;
    }

    // **Guard 2: Receiver provenance check**
    // Extract receiver and verify it's a tracked factory variable
    let object = function.child_by_field_name("object")?;

    // Only track direct identifier receivers (not chained or computed)
    if object.kind() != "identifier" {
        return None;
    }
    let receiver_name = object.utf8_text(src).ok()?;

    // Check receiver against appropriate factory set based on method
    let is_producer_method = KAFKA_PRODUCER_METHODS.contains(&method_name);
    let is_consumer_method = KAFKA_CONSUMER_METHODS.contains(&method_name);

    if is_producer_method && !factory_vars.producer_vars.contains(receiver_name) {
        return None;
    }
    if is_consumer_method && !factory_vars.consumer_vars.contains(receiver_name) {
        return None;
    }

    // **Guard 3: Topic evidence**
    let arguments = node.child_by_field_name("arguments")?;
    let (topic, topics, group_id, callback_mode) =
        extract_kafka_arguments(method_name, &arguments, src);

    // Require extractable topic to emit
    let has_topic_evidence = topic.is_some() || topics.is_some();
    if !has_topic_evidence {
        return None;
    }

    let start = node.start_position();
    let end = node.end_position();

    Some(RawKafkaBoundaryCall {
        function_name: method_name.to_string(),
        location: SourceLocation {
            line_start: (start.row + 1) as i64,
            col_start: start.column as i64,
            line_end: (end.row + 1) as i64,
            col_end: end.column as i64,
        },
        enclosing_function: enclosing_function.to_string(),
        topic,
        topics,
        group_id,
        callback_mode,
    })
}

/// Extract topic/topics/groupId/callbackMode from Kafka call arguments.
fn extract_kafka_arguments(
    method_name: &str,
    arguments: &tree_sitter::Node,
    src: &[u8],
) -> (Option<String>, Option<Vec<String>>, Option<String>, Option<String>) {
    // Find the first object argument (most Kafka methods take { ... } as first arg)
    let object_arg = find_first_object_arg(arguments);

    match method_name {
        // send({ topic: 'x', messages: [...] })
        "send" | "sendBatch" => {
            let topic = object_arg
                .as_ref()
                .and_then(|obj| extract_object_property(obj, "topic", src));
            (topic, None, None, None)
        }

        // subscribe({ topic: 'x' }) or subscribe({ topics: ['x', 'y'] })
        "subscribe" => {
            let topic = object_arg
                .as_ref()
                .and_then(|obj| extract_object_property(obj, "topic", src));
            let topics = object_arg
                .as_ref()
                .and_then(|obj| extract_object_array_property(obj, "topics", src));
            (topic, topics, None, None)
        }

        // run({ eachMessage: ..., eachBatch: ... })
        "run" => {
            let callback_mode = object_arg.as_ref().and_then(|obj| {
                if has_object_property(obj, "eachMessage", src) {
                    Some("eachMessage".to_string())
                } else if has_object_property(obj, "eachBatch", src) {
                    Some("eachBatch".to_string())
                } else {
                    None
                }
            });
            (None, None, None, callback_mode)
        }

        _ => (None, None, None, None),
    }
}

/// Find the first object literal argument in an arguments node.
fn find_first_object_arg<'a>(arguments: &'a tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    for i in 0..arguments.child_count() {
        if let Some(arg) = arguments.child(i) {
            if arg.kind() == "object" {
                return Some(arg);
            }
        }
    }
    None
}

/// Extract a string property value from an object literal.
fn extract_object_property(
    object: &tree_sitter::Node,
    property_name: &str,
    src: &[u8],
) -> Option<String> {
    for i in 0..object.child_count() {
        if let Some(child) = object.child(i) {
            match child.kind() {
                // Regular property: { topic: 'value' }
                "pair" => {
                    if let Some(key) = child.child_by_field_name("key") {
                        if let Ok(key_text) = key.utf8_text(src) {
                            if key_text == property_name {
                                if let Some(value) = child.child_by_field_name("value") {
                                    return extract_string_value(&value, src);
                                }
                            }
                        }
                    }
                }
                // Shorthand property: { topic } (equivalent to { topic: topic })
                "shorthand_property_identifier" | "shorthand_property_identifier_pattern" => {
                    if let Ok(name) = child.utf8_text(src) {
                        if name == property_name {
                            // Shorthand means the value is the same identifier
                            return Some(format!("${{{}}}", name));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract an array of string values from an object property.
fn extract_object_array_property(
    object: &tree_sitter::Node,
    property_name: &str,
    src: &[u8],
) -> Option<Vec<String>> {
    for i in 0..object.child_count() {
        if let Some(pair) = object.child(i) {
            if pair.kind() == "pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    if let Ok(key_text) = key.utf8_text(src) {
                        if key_text == property_name {
                            if let Some(value) = pair.child_by_field_name("value") {
                                if value.kind() == "array" {
                                    return extract_string_array(&value, src);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if an object has a specific property (without extracting value).
fn has_object_property(object: &tree_sitter::Node, property_name: &str, src: &[u8]) -> bool {
    for i in 0..object.child_count() {
        if let Some(pair) = object.child(i) {
            if pair.kind() == "pair" {
                if let Some(key) = pair.child_by_field_name("key") {
                    if let Ok(key_text) = key.utf8_text(src) {
                        if key_text == property_name {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
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

/// Extract array of strings from an array node.
fn extract_string_array(array: &tree_sitter::Node, src: &[u8]) -> Option<Vec<String>> {
    let mut result = Vec::new();
    for i in 0..array.child_count() {
        if let Some(element) = array.child(i) {
            // Skip brackets and commas
            if element.kind() == "[" || element.kind() == "]" || element.kind() == "," {
                continue;
            }
            if let Some(value) = extract_string_value(&element, src) {
                result.push(value);
            }
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(source: &str) -> Vec<RawKafkaBoundaryCall> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        extract_kafka_boundary_calls(&tree.root_node(), source.as_bytes(), "test.ts")
    }

    fn parse_and_check_import(source: &str) -> bool {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        has_kafkajs_import(&tree.root_node(), source.as_bytes())
    }

    // ── Import guard tests ──────────────────────────────────────────────────

    #[test]
    fn import_guard_esm_named() {
        let source = r#"import { Kafka } from 'kafkajs';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_default() {
        let source = r#"import Kafka from 'kafkajs';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_esm_namespace() {
        let source = r#"import * as kafka from 'kafkajs';"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_commonjs_destructure() {
        let source = r#"const { Kafka } = require('kafkajs');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_commonjs_direct() {
        let source = r#"const Kafka = require('kafkajs');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_bare_require() {
        let source = r#"require('kafkajs');"#;
        assert!(parse_and_check_import(source));
    }

    #[test]
    fn import_guard_no_kafkajs() {
        let source = r#"import { Kafka } from 'some-other-lib';"#;
        assert!(!parse_and_check_import(source));
    }

    // ── Scope guard negative test (P1 regression) ───────────────────────────

    #[test]
    fn no_detection_without_kafkajs_import() {
        // Generic object with Kafka-like method names but NO kafkajs import.
        // This MUST return empty to prevent false positives.
        let source = r#"
            const producer = {
                send: (x) => console.log(x),
            };
            const consumer = {
                subscribe: (x) => console.log(x),
                run: (x) => console.log(x),
            };
            producer.send({ topic: 'test' });
            consumer.subscribe({ topic: 'test' });
            consumer.run({ eachMessage: () => {} });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "should NOT emit Kafka surfaces without kafkajs import; got {:?}",
            calls
        );
    }

    // ── Positive detection tests (with kafkajs import + factory + topic) ─────

    #[test]
    fn detect_producer_send() {
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            await producer.send({ topic: 'orders', messages: [{ value: 'msg' }] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "send");
        assert_eq!(calls[0].topic, Some("orders".to_string()));
    }

    #[test]
    fn detect_producer_send_with_variable_topic() {
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            const topic = 'orders';
            await producer.send({ topic, messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "send");
        // Variable reference captured with ${} notation
        assert_eq!(calls[0].topic, Some("${topic}".to_string()));
    }

    #[test]
    fn detect_consumer_subscribe_single_topic() {
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const consumer = kafka.consumer({ groupId: 'test-group' });
            await consumer.subscribe({ topic: 'orders' });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "subscribe");
        assert_eq!(calls[0].topic, Some("orders".to_string()));
    }

    #[test]
    fn detect_consumer_subscribe_multiple_topics() {
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const consumer = kafka.consumer({ groupId: 'test-group' });
            await consumer.subscribe({ topics: ['orders', 'payments'] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "subscribe");
        assert_eq!(
            calls[0].topics,
            Some(vec!["orders".to_string(), "payments".to_string()])
        );
    }

    // ── Negative tests: run() intentionally NOT detected ─────────────────────
    // run() provides no topic evidence and is excluded from detection.
    // These tests verify that run() does NOT produce surfaces.

    #[test]
    fn run_not_detected_even_with_import_and_factory() {
        // run() was intentionally removed from detection — it provides no
        // topic evidence. Future work may correlate it with subscribe().
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const consumer = kafka.consumer({ groupId: 'test' });
            await consumer.run({
                eachMessage: async ({ topic, partition, message }) => {
                    console.log(message.value);
                }
            });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "run() should NOT emit — no topic evidence; got {:?}",
            calls
        );
    }

    // ── Negative tests: topic evidence guard ────────────────────────────────

    #[test]
    fn send_without_topic_not_detected() {
        // send() without extractable topic argument should NOT emit.
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            await producer.send({ messages: [{ value: 'test' }] });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "send() without topic should NOT emit; got {:?}",
            calls
        );
    }

    #[test]
    fn subscribe_without_topic_not_detected() {
        // subscribe() without topic/topics argument should NOT emit.
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const consumer = kafka.consumer({ groupId: 'test' });
            await consumer.subscribe({ fromBeginning: true });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "subscribe() without topic/topics should NOT emit; got {:?}",
            calls
        );
    }

    // ── Negative tests: receiver provenance guard (P1 fix) ──────────────────

    #[test]
    fn plain_object_with_topic_not_detected() {
        // P1 regression test: a plain object assigned to `producer` with a
        // topic-shaped argument should NOT emit, because the receiver was
        // not assigned from `*.producer()`.
        let source = r#"
            import { Kafka } from 'kafkajs';
            const producer = { send: (x) => x };
            producer.send({ topic: 'fake' });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "plain object receiver should NOT emit even with topic; got {:?}",
            calls
        );
    }

    #[test]
    fn plain_object_consumer_with_topic_not_detected() {
        // P1 regression test: a plain object assigned to `consumer` should NOT emit.
        let source = r#"
            import { Kafka } from 'kafkajs';
            const consumer = { subscribe: (x) => x };
            consumer.subscribe({ topic: 'fake' });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "plain object receiver should NOT emit even with topic; got {:?}",
            calls
        );
    }

    #[test]
    fn untracked_receiver_not_detected() {
        // Receiver `bus` was never assigned from *.producer(), so not detected.
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            const bus = producer; // aliased — but we don't track this
            bus.send({ topic: 'fake' });
        "#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "aliased receiver should NOT emit (no alias tracking); got {:?}",
            calls
        );
    }

    // ── Positive tests continued ────────────────────────────────────────────

    #[test]
    fn detect_enclosing_function() {
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            async function publishOrder() {
                await producer.send({ topic: 'orders', messages: [] });
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].enclosing_function, "publishOrder");
    }

    #[test]
    fn detect_multiple_patterns() {
        // run() is NOT detected (no topic evidence)
        // Only subscribe and send with topic arguments from factory-tracked receivers emit
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            const consumer = kafka.consumer({ groupId: 'test' });
            await consumer.subscribe({ topic: 'orders' });
            await consumer.run({ eachMessage: async () => {} });
            await producer.send({ topic: 'notifications', messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2, "only subscribe and send should emit; got {:?}", calls);

        let names: Vec<_> = calls.iter().map(|c| c.function_name.as_str()).collect();
        assert!(names.contains(&"subscribe"));
        assert!(names.contains(&"send"));
        assert!(!names.contains(&"run"), "run should NOT be detected");
    }

    #[test]
    fn ignore_non_kafka_patterns_with_import() {
        // Even with kafkajs import and factory, non-Kafka method names are not detected
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            console.log("hello");
            arr.push(1);
            producer.connect();
        "#;
        let calls = parse_and_extract(source);
        assert!(calls.is_empty());
    }

    #[test]
    fn commonjs_require_enables_detection() {
        let source = r#"
            const { Kafka } = require('kafkajs');
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer = kafka.producer();
            producer.send({ topic: 'orders', messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "send");
    }

    // ── Factory variable extraction tests ───────────────────────────────────

    #[test]
    fn factory_vars_extracted_from_let() {
        // `let` declarations should also be tracked
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            let producer = kafka.producer();
            producer.send({ topic: 'orders', messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn factory_vars_different_kafka_instance_name() {
        // The Kafka instance can have any name
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafkaClient = new Kafka({ brokers: ['localhost:9092'] });
            const prod = kafkaClient.producer();
            prod.send({ topic: 'orders', messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "send");
    }

    #[test]
    fn factory_vars_multiple_producers() {
        // Multiple producer variables should all be tracked
        let source = r#"
            import { Kafka } from 'kafkajs';
            const kafka = new Kafka({ brokers: ['localhost:9092'] });
            const producer1 = kafka.producer();
            const producer2 = kafka.producer();
            producer1.send({ topic: 'orders', messages: [] });
            producer2.send({ topic: 'payments', messages: [] });
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2);
    }
}
