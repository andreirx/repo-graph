//! Java method complexity metrics.
//!
//! Computes cyclomatic complexity, parameter count, and max nesting
//! depth for Java methods.

use repo_graph_indexer::types::ExtractedMetrics;

/// Compute metrics for a method body node.
///
/// Returns `ExtractedMetrics` with:
/// - `cyclomatic_complexity`: decision point count + 1
/// - `parameter_count`: number of parameters
/// - `max_nesting_depth`: maximum control-flow nesting
///
/// # Arguments
/// - `method_node`: tree-sitter node for the method or constructor
/// - `source`: source text bytes
/// - `param_count`: pre-computed parameter count from declaration
pub fn compute_method_metrics(
    method_node: tree_sitter::Node,
    _source: &[u8],
    param_count: u32,
) -> ExtractedMetrics {
    // Find the method body
    let body = method_node
        .child_by_field_name("body")
        .or_else(|| method_node.child_by_field_name("constructor_body"));

    let (complexity, max_depth) = match body {
        Some(body_node) => compute_body_metrics(body_node),
        None => (1, 0), // Abstract method or interface method
    };

    ExtractedMetrics {
        cyclomatic_complexity: complexity,
        parameter_count: param_count,
        max_nesting_depth: max_depth,
    }
}

/// Compute complexity and max nesting for a method body.
fn compute_body_metrics(body: tree_sitter::Node) -> (u32, u32) {
    let mut complexity: u32 = 1; // Base complexity
    let mut max_depth: u32 = 0;

    fn walk(node: tree_sitter::Node, depth: u32, complexity: &mut u32, max_depth: &mut u32) {
        let kind = node.kind();

        // Decision points that add complexity
        let is_decision = matches!(
            kind,
            "if_statement"
                | "while_statement"
                | "for_statement"
                | "enhanced_for_statement"
                | "do_statement"
                | "catch_clause"
                | "switch_expression"
                | "ternary_expression"
                | "binary_expression" // && and || in conditions
        );

        // For binary_expression, only count && and ||
        if kind == "binary_expression" {
            // Check operator child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "&&" || child.kind() == "||" {
                    *complexity += 1;
                }
            }
        } else if is_decision {
            *complexity += 1;
        }

        // Case labels add complexity (each case is a branch)
        if kind == "switch_block_statement_group" || kind == "switch_rule" {
            *complexity += 1;
        }

        // Nesting depth tracking
        let nesting_kinds = [
            "if_statement",
            "while_statement",
            "for_statement",
            "enhanced_for_statement",
            "do_statement",
            "try_statement",
            "switch_expression",
            "lambda_expression",
            "class_body", // Anonymous inner class
        ];

        let new_depth = if nesting_kinds.contains(&kind) {
            depth + 1
        } else {
            depth
        };

        *max_depth = (*max_depth).max(new_depth);

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, new_depth, complexity, max_depth);
        }
    }

    walk(body, 0, &mut complexity, &mut max_depth);
    (complexity, max_depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_java(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn simple_method_has_complexity_1() {
        let source = r#"
            class Foo {
                void bar() {
                    System.out.println("hello");
                }
            }
        "#;
        let tree = parse_java(source);
        let root = tree.root_node();

        // Find the method_declaration
        let method = find_node_by_kind(root, "method_declaration").unwrap();
        let metrics = compute_method_metrics(method, source.as_bytes(), 0);

        assert_eq!(metrics.cyclomatic_complexity, 1);
        assert_eq!(metrics.parameter_count, 0);
        assert_eq!(metrics.max_nesting_depth, 0);
    }

    #[test]
    fn method_with_if_has_complexity_2() {
        let source = r#"
            class Foo {
                void bar(int x) {
                    if (x > 0) {
                        System.out.println("positive");
                    }
                }
            }
        "#;
        let tree = parse_java(source);
        let root = tree.root_node();
        let method = find_node_by_kind(root, "method_declaration").unwrap();
        let metrics = compute_method_metrics(method, source.as_bytes(), 1);

        assert_eq!(metrics.cyclomatic_complexity, 2);
        assert_eq!(metrics.parameter_count, 1);
        assert_eq!(metrics.max_nesting_depth, 1);
    }

    #[test]
    fn method_with_nested_loops() {
        let source = r#"
            class Foo {
                void bar() {
                    for (int i = 0; i < 10; i++) {
                        while (true) {
                            break;
                        }
                    }
                }
            }
        "#;
        let tree = parse_java(source);
        let root = tree.root_node();
        let method = find_node_by_kind(root, "method_declaration").unwrap();
        let metrics = compute_method_metrics(method, source.as_bytes(), 0);

        // for + while = 2 + base 1 = 3
        assert_eq!(metrics.cyclomatic_complexity, 3);
        assert_eq!(metrics.max_nesting_depth, 2);
    }

    fn find_node_by_kind<'a>(
        node: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_node_by_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }
}
