//! Boundary interaction detector for TypeScript/JavaScript.
//!
//! Detects SharedArrayBuffer and Atomics patterns that indicate shared-memory
//! boundary interactions between execution contexts.
//!
//! BI-1C scope:
//!   - SharedArrayBuffer allocation
//!   - Atomics.* synchronization primitives
//!
//! Explicitly out of scope (no SAB correlation possible without dataflow):
//!   - Worker creation (generic worker spawning)
//!   - postMessage calls (generic message passing)
//!
//! Contract: docs/slices/bi-1c-shared-array-buffer.md

use repo_graph_classification::types::SourceLocation;

/// Raw boundary call site extracted from TS/JS source.
///
/// This is the extractor-side DTO. The emitter converts this to
/// `BoundaryCallsite` from boundary-interaction-extractor.
#[derive(Debug, Clone)]
pub struct RawTsBoundaryCall {
    /// Function/constructor name (e.g., "SharedArrayBuffer", "Worker", "Atomics.wait").
    pub function_name: String,

    /// Source location of the call/new expression.
    pub location: SourceLocation,

    /// Enclosing function/method name (for stable key construction).
    /// Empty string for top-level calls.
    pub enclosing_function: String,

    /// Extracted argument (e.g., worker path for `new Worker("./worker.ts")`).
    pub extracted_argument: Option<String>,
}

/// SharedArrayBuffer constructor.
const SAB_CONSTRUCTORS: &[&str] = &["SharedArrayBuffer"];

/// Atomics methods that indicate SAB usage.
const ATOMICS_METHODS: &[&str] = &[
    "Atomics.wait",
    "Atomics.notify",
    "Atomics.load",
    "Atomics.store",
    "Atomics.add",
    "Atomics.sub",
    "Atomics.and",
    "Atomics.or",
    "Atomics.xor",
    "Atomics.compareExchange",
    "Atomics.exchange",
];

/// Check if a function/constructor name is a boundary candidate.
pub fn is_boundary_pattern(name: &str) -> bool {
    SAB_CONSTRUCTORS.contains(&name) || ATOMICS_METHODS.contains(&name)
}

/// Extract boundary calls from a parsed TS/JS file.
///
/// `root` is the tree-sitter root node for the file.
/// `src` is the source text as bytes.
/// `file_path` is the repo-relative path (for diagnostics).
///
/// Returns a list of detected boundary calls.
pub fn extract_ts_boundary_calls(
    root: &tree_sitter::Node,
    src: &[u8],
    _file_path: &str,
) -> Vec<RawTsBoundaryCall> {
    // PERSIST-RECURSION-1: iterative pre-order walk with enclosing-function
    // tracking (was recursive on AST depth → stack overflow at scale). Order and
    // emitted facts are byte-for-byte identical to the prior recursive form.
    let mut results = Vec::new();
    crate::walk::visit_with_enclosing(*root, src, |node, enclosing| match node.kind() {
        // Detect `new SharedArrayBuffer(...)` and `new Worker(...)`
        "new_expression" => {
            if let Some(call) = try_extract_new_expression(&node, src, enclosing) {
                results.push(call);
            }
        }
        // Detect `worker.postMessage(...)` and `Atomics.*(...)`
        "call_expression" => {
            if let Some(call) = try_extract_call_expression(&node, src, enclosing) {
                results.push(call);
            }
        }
        _ => {}
    });
    results
}

/// Try to extract a boundary call from a `new_expression` node.
///
/// Handles:
/// - `new SharedArrayBuffer(1024)`
fn try_extract_new_expression(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
) -> Option<RawTsBoundaryCall> {
    // new_expression structure: `new` constructor arguments
    // First named child is typically the constructor (identifier or member_expression)
    let constructor = node.child_by_field_name("constructor")?;

    let constructor_name = match constructor.kind() {
        "identifier" => constructor.utf8_text(src).ok()?,
        _ => return None,
    };

    // Check if it's SharedArrayBuffer (the only SAB-relevant constructor)
    if !SAB_CONSTRUCTORS.contains(&constructor_name) {
        return None;
    }

    let start = node.start_position();
    let end = node.end_position();

    Some(RawTsBoundaryCall {
        function_name: constructor_name.to_string(),
        location: SourceLocation {
            line_start: (start.row + 1) as i64,
            col_start: start.column as i64,
            line_end: (end.row + 1) as i64,
            col_end: end.column as i64,
        },
        enclosing_function: enclosing_function.to_string(),
        extracted_argument: None,
    })
}

/// Try to extract a boundary call from a `call_expression` node.
///
/// Handles:
/// - `Atomics.wait(...)`
/// - `Atomics.notify(...)`
/// - `Atomics.load/store/add/sub/and/or/xor/exchange/compareExchange(...)`
fn try_extract_call_expression(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
) -> Option<RawTsBoundaryCall> {
    // call_expression structure: function arguments
    let function = node.child_by_field_name("function")?;

    // Only handle member expressions for Atomics.* patterns
    let function_name = match function.kind() {
        "member_expression" => {
            let object = function.child_by_field_name("object")?;
            let property = function.child_by_field_name("property")?;

            let object_name = object.utf8_text(src).ok()?;
            let property_name = property.utf8_text(src).ok()?;

            // Only check for Atomics.* patterns
            if object_name == "Atomics" {
                let full_name = format!("Atomics.{}", property_name);
                if !ATOMICS_METHODS.contains(&full_name.as_str()) {
                    return None;
                }
                full_name
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let start = node.start_position();
    let end = node.end_position();

    Some(RawTsBoundaryCall {
        function_name,
        location: SourceLocation {
            line_start: (start.row + 1) as i64,
            col_start: start.column as i64,
            line_end: (end.row + 1) as i64,
            col_end: end.column as i64,
        },
        enclosing_function: enclosing_function.to_string(),
        extracted_argument: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(source: &str) -> Vec<RawTsBoundaryCall> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(source, None).expect("Failed to parse");
        extract_ts_boundary_calls(&tree.root_node(), source.as_bytes(), "test.ts")
    }

    /// PERSIST-RECURSION-1 regression: a pathologically deep TS file must NOT
    /// overflow the stack. Exercises the shared `visit_with_enclosing` walk (used
    /// by every detector's `extract_from_node`) on 50_000 nesting levels. Runs on
    /// the default test-thread stack, so a still-recursive walk would abort here.
    #[test]
    fn deeply_nested_input_does_not_overflow() {
        let depth = 50_000;
        let mut source = String::from("function deep() {\n");
        for _ in 0..depth {
            source.push('{');
        }
        source.push_str(" const sab = new SharedArrayBuffer(1024); ");
        for _ in 0..depth {
            source.push('}');
        }
        source.push_str("\n}\n");

        let calls = parse_and_extract(&source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "SharedArrayBuffer");
    }

    #[test]
    fn detect_shared_array_buffer_creation() {
        let source = r#"const sab = new SharedArrayBuffer(1024);"#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "SharedArrayBuffer");
    }

    #[test]
    fn worker_creation_not_detected_without_sab_correlation() {
        // Worker creation alone is not a SAB boundary — requires dataflow analysis
        // to prove SAB is actually passed. See Option A decision.
        let source = r#"const worker = new Worker("./worker.ts");"#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "Worker without SAB correlation should not emit SAB surface"
        );
    }

    #[test]
    fn post_message_not_detected_without_sab_correlation() {
        // postMessage alone is not a SAB boundary — requires dataflow analysis
        // to prove SAB is in the arguments. See Option A decision.
        let source = r#"worker.postMessage({ type: "ping" });"#;
        let calls = parse_and_extract(source);
        assert!(
            calls.is_empty(),
            "postMessage without SAB correlation should not emit SAB surface"
        );
    }

    #[test]
    fn detect_atomics_wait() {
        let source = r#"Atomics.wait(view, 0, 0);"#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "Atomics.wait");
    }

    #[test]
    fn detect_atomics_notify() {
        let source = r#"Atomics.notify(view, 0, 1);"#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "Atomics.notify");
    }

    #[test]
    fn detect_atomics_store_load() {
        let source = r#"
            Atomics.store(view, 0, 42);
            const value = Atomics.load(view, 0);
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function_name, "Atomics.store");
        assert_eq!(calls[1].function_name, "Atomics.load");
    }

    #[test]
    fn detect_atomics_compare_exchange() {
        let source = r#"Atomics.compareExchange(view, 0, 0, 1);"#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "Atomics.compareExchange");
    }

    #[test]
    fn detect_enclosing_function() {
        let source = r#"
            function setupSharedMemory() {
                const sab = new SharedArrayBuffer(1024);
                Atomics.store(new Int32Array(sab), 0, 0);
            }
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].enclosing_function, "setupSharedMemory");
        assert_eq!(calls[1].enclosing_function, "setupSharedMemory");
    }

    #[test]
    fn detect_multiple_sab_patterns_in_file() {
        // Only SAB allocation and Atomics.* are detected
        // Worker and postMessage are NOT detected (no SAB correlation)
        let source = r#"
            const sab = new SharedArrayBuffer(1024);
            Atomics.store(new Int32Array(sab), 0, 1);
            Atomics.notify(new Int32Array(sab), 0, 1);
            Atomics.wait(new Int32Array(sab), 0, 0);
        "#;
        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 4);

        let names: Vec<_> = calls.iter().map(|c| c.function_name.as_str()).collect();
        assert!(names.contains(&"SharedArrayBuffer"));
        assert!(names.contains(&"Atomics.store"));
        assert!(names.contains(&"Atomics.notify"));
        assert!(names.contains(&"Atomics.wait"));
    }

    #[test]
    fn ignore_non_boundary_patterns() {
        let source = r#"
            const arr = new Array(10);
            console.log("hello");
            Math.random();
        "#;
        let calls = parse_and_extract(source);
        assert!(calls.is_empty());
    }
}
