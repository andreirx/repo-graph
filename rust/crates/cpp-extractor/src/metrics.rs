//! Cyclomatic complexity metrics for C++ functions.
//!
//! Pinned complexity rules (from cpp-extractor-v1.md):
//!
//! | Construct           | Increment |
//! |---------------------|-----------|
//! | Base                | +1        |
//! | if                  | +1        |
//! | else if             | +1        |
//! | for (including range-for) | +1  |
//! | while               | +1        |
//! | do while            | +1        |
//! | case                | +1        |
//! | default             | +0        |
//! | switch              | +0        |
//! | &&                  | +1        |
//! | ||                  | +1        |
//! | ?:                  | +1        |
//! | catch               | +1        |
//! | throw               | +0        |

use repo_graph_indexer::types::ExtractedMetrics;

/// Compute cyclomatic complexity and other metrics for a C++ function body.
pub fn compute_function_metrics(
    body: &tree_sitter::Node,
    params: Option<&tree_sitter::Node>,
) -> ExtractedMetrics {
    let mut complexity: u32 = 1; // Base complexity
    let mut max_depth: u32 = 0;
    let mut current_depth: u32 = 0;

    fn walk(
        node: &tree_sitter::Node,
        complexity: &mut u32,
        max_depth: &mut u32,
        current_depth: &mut u32,
    ) {
        let dominated = match node.kind() {
            // Control flow that adds complexity
            "if_statement" => {
                *complexity += 1;
                true
            }
            // C++ has both regular for and range-based for
            "for_statement" | "for_range_loop" | "while_statement" | "do_statement" => {
                *complexity += 1;
                true
            }
            "case_statement" => {
                // In tree-sitter-cpp, both `case X:` and `default:` are case_statement.
                // `case X:` has a `value` field, `default:` does not.
                // Per design doc: case +1, default +0.
                if node.child_by_field_name("value").is_some() {
                    *complexity += 1;
                }
                false // case doesn't dominate its own scope
            }
            "goto_statement" => {
                *complexity += 1;
                false
            }
            // C++ exception handling
            "catch_clause" => {
                *complexity += 1;
                true
            }
            // throw does NOT add complexity per design doc
            // Logical operators
            "binary_expression" => {
                // Check for && or ||
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        let text = child.kind();
                        if text == "&&" || text == "||" {
                            *complexity += 1;
                        }
                    }
                }
                false
            }
            // Ternary operator
            "conditional_expression" => {
                *complexity += 1;
                false
            }
            _ => false,
        };

        if dominated {
            *current_depth += 1;
            if *current_depth > *max_depth {
                *max_depth = *current_depth;
            }
        }

        // Recurse into children (but not into nested functions or lambdas)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Don't recurse into nested function definitions or lambdas
            if child.kind() != "function_definition" && child.kind() != "lambda_expression" {
                walk(&child, complexity, max_depth, current_depth);
            }
        }

        if dominated {
            *current_depth -= 1;
        }
    }

    walk(body, &mut complexity, &mut max_depth, &mut current_depth);

    // Count parameters
    let param_count = params
        .map(|p| {
            let mut count = 0u32;
            let mut cursor = p.walk();
            for child in p.children(&mut cursor) {
                if child.kind() == "parameter_declaration" {
                    count += 1;
                }
            }
            count
        })
        .unwrap_or(0);

    ExtractedMetrics {
        cyclomatic_complexity: complexity,
        parameter_count: param_count,
        max_nesting_depth: max_depth,
        // Phase A: function_length and cognitive_complexity are TS/JS-only.
        // C++ support deferred to cross-language rollout.
        function_length: None,
        cognitive_complexity: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_cpp(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn get_function_body(tree: &tree_sitter::Tree) -> tree_sitter::Node {
        let root = tree.root_node();
        let func = root.child(0).unwrap();
        assert_eq!(func.kind(), "function_definition");
        func.child_by_field_name("body").unwrap()
    }

    #[test]
    fn empty_function_has_base_complexity() {
        let tree = parse_cpp("void foo() {}");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        assert_eq!(metrics.cyclomatic_complexity, 1);
        assert_eq!(metrics.max_nesting_depth, 0);
    }

    #[test]
    fn if_adds_complexity() {
        let tree = parse_cpp("void foo() { if (x) { y(); } }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        assert_eq!(metrics.cyclomatic_complexity, 2); // base + if
        assert_eq!(metrics.max_nesting_depth, 1);
    }

    #[test]
    fn range_for_adds_complexity() {
        let tree = parse_cpp("void foo() { for (auto& x : items) {} }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        assert_eq!(metrics.cyclomatic_complexity, 2); // base + range-for
        assert_eq!(metrics.max_nesting_depth, 1);
    }

    #[test]
    fn catch_adds_complexity() {
        let tree = parse_cpp("void foo() { try { x(); } catch (...) {} }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        // base(1) + catch(1) = 2 (try block doesn't add, catch does)
        assert_eq!(metrics.cyclomatic_complexity, 2);
    }

    #[test]
    fn multiple_catch_clauses() {
        let tree = parse_cpp(
            "void foo() { try { x(); } catch (int) {} catch (char*) {} catch (...) {} }",
        );
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        // base(1) + catch(1) + catch(1) + catch(1) = 4
        assert_eq!(metrics.cyclomatic_complexity, 4);
    }

    #[test]
    fn throw_does_not_add_complexity() {
        let tree = parse_cpp("void foo() { throw 42; }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        assert_eq!(metrics.cyclomatic_complexity, 1); // base only
    }

    #[test]
    fn logical_operators_add_complexity() {
        let tree = parse_cpp("void foo() { if (a && b || c) {} }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        // base(1) + if(1) + &&(1) + ||(1) = 4
        assert_eq!(metrics.cyclomatic_complexity, 4);
    }

    #[test]
    fn ternary_adds_complexity() {
        let tree = parse_cpp("void foo() { int x = a ? b : c; }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        assert_eq!(metrics.cyclomatic_complexity, 2); // base + ternary
    }

    #[test]
    fn case_adds_complexity_switch_does_not() {
        let tree =
            parse_cpp("void foo() { switch(x) { case 1: break; case 2: break; default: break; } }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        // base(1) + case(1) + case(1) = 3 (default doesn't count, switch doesn't count)
        assert_eq!(metrics.cyclomatic_complexity, 3);
    }

    #[test]
    fn parameter_count() {
        let tree = parse_cpp("void foo(int a, char* b, float c) {}");
        let root = tree.root_node();
        let func = root.child(0).unwrap();
        let body = func.child_by_field_name("body").unwrap();

        // Find parameter list
        let declarator = func.child_by_field_name("declarator").unwrap();
        let params = declarator.child_by_field_name("parameters");

        let metrics = compute_function_metrics(&body, params.as_ref());
        assert_eq!(metrics.parameter_count, 3);
    }

    #[test]
    fn nested_depth_with_try_catch() {
        let tree = parse_cpp("void foo() { try { if (a) { while (b) {} } } catch (...) {} }");
        let body = get_function_body(&tree);

        let metrics = compute_function_metrics(&body, None);
        // base(1) + if(1) + while(1) + catch(1) = 4
        assert_eq!(metrics.cyclomatic_complexity, 4);
        // try -> if -> while = depth 2 for control flow
        // catch also has depth 1
        assert_eq!(metrics.max_nesting_depth, 2);
    }
}
