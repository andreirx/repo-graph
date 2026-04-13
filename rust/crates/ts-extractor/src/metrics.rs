//! AST-derived deterministic metrics for TypeScript functions.
//!
//! Mirror of `src/adapters/extractors/typescript/ast-metrics.ts`.
//! Pure computation over tree-sitter AST nodes.

use repo_graph_indexer::types::ExtractedMetrics;

/// Compute metrics for a function/method body.
///
/// `body` is the `statement_block` AST node.
/// `params` is the `formal_parameters` AST node (optional).
pub fn compute_function_metrics(
	body: &tree_sitter::Node,
	params: Option<&tree_sitter::Node>,
) -> ExtractedMetrics {
	let mut complexity: u32 = 1; // base complexity
	let mut max_depth: u32 = 0;

	walk(body, 0, &mut complexity, &mut max_depth);

	let parameter_count = count_parameters(params);

	ExtractedMetrics {
		cyclomatic_complexity: complexity,
		parameter_count,
		max_nesting_depth: max_depth,
	}
}

fn walk(
	node: &tree_sitter::Node,
	current_depth: u32,
	complexity: &mut u32,
	max_depth: &mut u32,
) {
	let mut depth = current_depth;

	// Track nesting depth.
	if is_nesting_node(node.kind()) {
		// Special case: if_statement inside else_clause is "else if"
		// continuation, not deeper nesting.
		let is_else_if = node.kind() == "if_statement"
			&& node.parent().map(|p| p.kind() == "else_clause").unwrap_or(false);
		if !is_else_if {
			depth = current_depth + 1;
			if depth > *max_depth {
				*max_depth = depth;
			}
		}
	}

	// Count decision points.
	if is_decision_statement(node.kind()) {
		*complexity += 1;
	}
	if node.kind() == "ternary_expression" {
		*complexity += 1;
	}
	// Binary expressions with short-circuit operators.
	if node.kind() == "binary_expression" {
		for i in 0..node.child_count() {
			if let Some(child) = node.child(i) {
				if matches!(child.kind(), "&&" | "||" | "??") {
					*complexity += 1;
					break;
				}
			}
		}
	}

	// Recurse into children (not into nested function/class bodies).
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if is_metrics_scope_boundary(child.kind()) {
				continue;
			}
			walk(&child, depth, complexity, max_depth);
		}
	}
}

fn is_decision_statement(kind: &str) -> bool {
	matches!(
		kind,
		"if_statement"
			| "for_statement"
			| "for_in_statement"
			| "while_statement"
			| "do_statement"
			| "catch_clause"
			| "switch_case"
	)
}

fn is_nesting_node(kind: &str) -> bool {
	matches!(
		kind,
		"if_statement"
			| "for_statement"
			| "for_in_statement"
			| "while_statement"
			| "do_statement"
			| "switch_statement"
			| "try_statement"
			| "catch_clause"
	)
}

fn is_metrics_scope_boundary(kind: &str) -> bool {
	matches!(
		kind,
		"function_declaration"
			| "generator_function_declaration"
			| "generator_function"
			| "arrow_function"
			| "function_expression"
			| "class_declaration"
			| "method_definition"
	)
}

fn count_parameters(params: Option<&tree_sitter::Node>) -> u32 {
	let params = match params {
		Some(p) => p,
		None => return 0,
	};
	let mut count = 0;
	for i in 0..params.child_count() {
		if let Some(child) = params.child(i) {
			if matches!(
				child.kind(),
				"required_parameter" | "optional_parameter" | "rest_parameter"
			) {
				count += 1;
			}
		}
	}
	count
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parse_and_get_body(source: &str) -> (tree_sitter::Tree, usize) {
		let mut parser = tree_sitter::Parser::new();
		let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
		parser.set_language(&lang).unwrap();
		let tree = parser.parse(source, None).unwrap();
		// Find the function_declaration's body (statement_block).
		let root = tree.root_node();
		let func = root.child(0).unwrap();
		assert_eq!(func.kind(), "function_declaration");
		let body_idx = (0..func.child_count())
			.find(|&i| func.child(i).unwrap().kind() == "statement_block")
			.unwrap();
		(tree, body_idx)
	}

	fn metrics_for(source: &str) -> ExtractedMetrics {
		let (tree, body_idx) = parse_and_get_body(source);
		let func = tree.root_node().child(0).unwrap();
		let body = func.child(body_idx).unwrap();
		let params = func.child_by_field_name("parameters");
		compute_function_metrics(&body, params.as_ref())
	}

	#[test]
	fn empty_function_has_base_complexity() {
		let m = metrics_for("function f() {}");
		assert_eq!(m.cyclomatic_complexity, 1);
		assert_eq!(m.parameter_count, 0);
		assert_eq!(m.max_nesting_depth, 0);
	}

	#[test]
	fn if_adds_complexity_and_depth() {
		let m = metrics_for("function f() { if (true) {} }");
		assert_eq!(m.cyclomatic_complexity, 2); // 1 + if
		assert_eq!(m.max_nesting_depth, 1);
	}

	#[test]
	fn else_if_does_not_add_nesting() {
		let m = metrics_for(
			"function f() { if (a) {} else if (b) {} else {} }",
		);
		assert_eq!(m.cyclomatic_complexity, 3); // 1 + if + else-if
		assert_eq!(m.max_nesting_depth, 1); // else-if stays at same level
	}

	#[test]
	fn nested_if_adds_depth() {
		let m = metrics_for(
			"function f() { if (a) { if (b) {} } }",
		);
		assert_eq!(m.cyclomatic_complexity, 3); // 1 + if + if
		assert_eq!(m.max_nesting_depth, 2);
	}

	#[test]
	fn logical_operators_add_complexity() {
		let m = metrics_for(
			"function f() { if (a && b || c) {} }",
		);
		// 1 (base) + 1 (if) + 1 (&&) + 1 (||) = 4
		assert_eq!(m.cyclomatic_complexity, 4);
	}

	#[test]
	fn for_while_do_catch_switch_case() {
		let m = metrics_for(
			r#"function f() {
				for (;;) {}
				while (true) {}
				do {} while (true);
				try {} catch (e) {}
				switch (x) { case 1: break; case 2: break; }
			}"#,
		);
		// 1 + for + while + do + catch + case + case = 7
		assert_eq!(m.cyclomatic_complexity, 7);
	}

	#[test]
	fn ternary_adds_complexity() {
		let m = metrics_for("function f() { return a ? 1 : 2; }");
		assert_eq!(m.cyclomatic_complexity, 2);
	}

	#[test]
	fn parameter_count() {
		let m = metrics_for("function f(a: string, b: number, c?: boolean) {}");
		assert_eq!(m.parameter_count, 3);
	}

	#[test]
	fn rest_parameter_counted() {
		let m = metrics_for("function f(...args: string[]) {}");
		assert_eq!(m.parameter_count, 1);
	}

	#[test]
	fn nested_function_not_counted() {
		let m = metrics_for(
			"function f() { function inner() { if (x) {} } }",
		);
		// inner's if should NOT contribute to f's metrics.
		assert_eq!(m.cyclomatic_complexity, 1);
		assert_eq!(m.max_nesting_depth, 0);
	}
}
