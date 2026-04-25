//! AST-derived deterministic metrics for TypeScript functions.
//!
//! Mirror of `src/adapters/extractors/typescript/ast-metrics.ts`.
//! Pure computation over tree-sitter AST nodes.
//!
//! Phase A additions (quality-control-phase-a.md):
//! - `function_length`: line count of full function node span
//! - `cognitive_complexity`: Sonar-style weighted complexity

use repo_graph_indexer::types::ExtractedMetrics;

/// Compute metrics for a function/method.
///
/// `func_node` is the full function/method AST node (for function_length).
/// `body` is the `statement_block` AST node (for complexity metrics).
/// `params` is the `formal_parameters` AST node (optional).
pub fn compute_function_metrics(
	func_node: &tree_sitter::Node,
	body: &tree_sitter::Node,
	params: Option<&tree_sitter::Node>,
) -> ExtractedMetrics {
	// Cyclomatic complexity
	let mut cyclomatic: u32 = 1; // base complexity
	let mut max_depth: u32 = 0;
	walk_cyclomatic(body, 0, &mut cyclomatic, &mut max_depth);

	// Parameter count
	let parameter_count = count_parameters(params);

	// Function length: inclusive line span
	// tree-sitter uses 0-based rows; we use 1-based lines
	let line_start = func_node.start_position().row as u32 + 1;
	let line_end = func_node.end_position().row as u32 + 1;
	let function_length = line_end.saturating_sub(line_start) + 1;

	// Cognitive complexity
	let cognitive_complexity = compute_cognitive_complexity(body);

	ExtractedMetrics {
		cyclomatic_complexity: cyclomatic,
		parameter_count,
		max_nesting_depth: max_depth,
		function_length: Some(function_length),
		cognitive_complexity: Some(cognitive_complexity),
	}
}

fn walk_cyclomatic(
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
			walk_cyclomatic(&child, depth, complexity, max_depth);
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

// ── Cognitive complexity ─────────────────────────────────────────
//
// Sonar-style weighted complexity with nesting penalties.
// See quality-control-phase-a.md for counting rules.
//
// Deferred: recursion penalty, early-return penalty.

/// Compute cognitive complexity for a function body.
///
/// Sonar-style rules:
/// - Structural increments (+1): if, else, ternary, switch, loops, catch, logical ops
/// - Nesting penalty (+N): control structures inside other control structures
/// - else-if is a continuation, not deeper nesting
fn compute_cognitive_complexity(body: &tree_sitter::Node) -> u32 {
	let mut complexity: u32 = 0;
	walk_cognitive(body, 0, &mut complexity);
	complexity
}

fn walk_cognitive(node: &tree_sitter::Node, nesting: u32, complexity: &mut u32) {
	let kind = node.kind();

	// Determine if this node is a nesting increment
	let is_nesting_structure = is_cognitive_nesting_node(kind);

	// Special case: else-if is continuation, not deeper nesting
	// In tree-sitter: else_clause contains if_statement
	let is_else_if = kind == "if_statement"
		&& node.parent().map(|p| p.kind() == "else_clause").unwrap_or(false);

	// Structural increment for control flow
	if is_cognitive_increment_node(kind) {
		// +1 for the structure itself
		*complexity += 1;

		// +N nesting penalty (except else-if which stays at same level)
		if is_nesting_structure && !is_else_if {
			*complexity += nesting;
		}
	}

	// else clause: +1 structural increment only (no nesting penalty)
	// BUT: if this else_clause directly contains an if_statement (else-if),
	// don't count it — the if_statement will count as the "else if" construct.
	if kind == "else_clause" {
		// Check if any child is an if_statement (skip the "else" keyword)
		let is_else_if_container = (0..node.child_count())
			.filter_map(|i| node.child(i))
			.any(|c| c.kind() == "if_statement");
		if !is_else_if_container {
			*complexity += 1;
		}
	}

	// Logical operators: +1 each (no nesting penalty)
	if kind == "binary_expression" {
		for i in 0..node.child_count() {
			if let Some(child) = node.child(i) {
				if matches!(child.kind(), "&&" | "||" | "??") {
					*complexity += 1;
				}
			}
		}
	}

	// Calculate nesting depth for children
	let child_nesting = if is_nesting_structure && !is_else_if {
		nesting + 1
	} else {
		nesting
	};

	// Recurse into children (not into nested function/class bodies)
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if is_metrics_scope_boundary(child.kind()) {
				continue;
			}
			walk_cognitive(&child, child_nesting, complexity);
		}
	}
}

/// Nodes that get a structural increment (+1).
fn is_cognitive_increment_node(kind: &str) -> bool {
	matches!(
		kind,
		"if_statement"
			| "ternary_expression"
			| "switch_statement"
			| "for_statement"
			| "for_in_statement"
			| "while_statement"
			| "do_statement"
			| "catch_clause"
	)
}

/// Nodes that increase nesting depth for their children.
/// Note: else_clause is NOT a nesting node — it's a continuation of the
/// if_statement, so its children are at the same depth as the if body.
fn is_cognitive_nesting_node(kind: &str) -> bool {
	matches!(
		kind,
		"if_statement"
			| "ternary_expression"
			| "switch_statement"
			| "for_statement"
			| "for_in_statement"
			| "while_statement"
			| "do_statement"
			| "catch_clause"
	)
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
		compute_function_metrics(&func, &body, params.as_ref())
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

	// ── function_length tests ────────────────────────────────────

	#[test]
	fn function_length_single_line() {
		let m = metrics_for("function f() {}");
		assert_eq!(m.function_length, Some(1));
	}

	#[test]
	fn function_length_multiline() {
		let m = metrics_for(
			r#"function f() {
	const a = 1;
	const b = 2;
	return a + b;
}"#,
		);
		// 5 lines: signature + 3 body lines + closing brace
		assert_eq!(m.function_length, Some(5));
	}

	#[test]
	fn function_length_includes_signature() {
		// Signature spans multiple lines
		let m = metrics_for(
			r#"function f(
	a: number,
	b: number
): number {
	return a + b;
}"#,
		);
		// 6 lines total
		assert_eq!(m.function_length, Some(6));
	}

	// ── cognitive_complexity tests ───────────────────────────────

	#[test]
	fn cognitive_empty_function() {
		let m = metrics_for("function f() {}");
		assert_eq!(m.cognitive_complexity, Some(0));
	}

	#[test]
	fn cognitive_single_if() {
		let m = metrics_for("function f() { if (x) {} }");
		// +1 for if (no nesting penalty at depth 0)
		assert_eq!(m.cognitive_complexity, Some(1));
	}

	#[test]
	fn cognitive_nested_if() {
		let m = metrics_for("function f() { if (a) { if (b) {} } }");
		// +1 for outer if (depth 0)
		// +1 for inner if + 1 nesting penalty (depth 1)
		assert_eq!(m.cognitive_complexity, Some(3));
	}

	#[test]
	fn cognitive_else_adds_increment_and_penalty() {
		let m = metrics_for("function f() { if (a) {} else {} }");
		// +1 for if
		// +1 for else (+ 0 nesting penalty since else is at same depth)
		assert_eq!(m.cognitive_complexity, Some(2));
	}

	#[test]
	fn cognitive_else_if_is_continuation() {
		let m = metrics_for(
			"function f() { if (a) {} else if (b) {} else {} }",
		);
		// +1 for if
		// +1 for else-if (continuation, same depth)
		// +1 for else
		assert_eq!(m.cognitive_complexity, Some(3));
	}

	#[test]
	fn cognitive_logical_operators() {
		let m = metrics_for("function f() { if (a && b || c) {} }");
		// +1 for if
		// +1 for &&
		// +1 for ||
		assert_eq!(m.cognitive_complexity, Some(3));
	}

	#[test]
	fn cognitive_ternary_with_nesting() {
		let m = metrics_for("function f() { if (x) { return a ? 1 : 2; } }");
		// +1 for if
		// +1 for ternary + 1 nesting penalty (inside if)
		assert_eq!(m.cognitive_complexity, Some(3));
	}

	#[test]
	fn cognitive_loop_with_nested_if() {
		let m = metrics_for("function f() { for (;;) { if (x) {} } }");
		// +1 for for loop
		// +1 for if + 1 nesting penalty (inside for)
		assert_eq!(m.cognitive_complexity, Some(3));
	}

	#[test]
	fn cognitive_try_catch() {
		let m = metrics_for("function f() { try {} catch (e) {} }");
		// +1 for catch (try doesn't add complexity)
		assert_eq!(m.cognitive_complexity, Some(1));
	}

	#[test]
	fn cognitive_switch() {
		let m = metrics_for(
			"function f() { switch (x) { case 1: break; case 2: break; } }",
		);
		// +1 for switch (cases don't add in cognitive)
		assert_eq!(m.cognitive_complexity, Some(1));
	}

	#[test]
	fn cognitive_deeply_nested() {
		let m = metrics_for(
			"function f() { if (a) { if (b) { if (c) {} } } }",
		);
		// +1 for outer if
		// +1 + 1 for middle if (depth 1)
		// +1 + 2 for inner if (depth 2)
		assert_eq!(m.cognitive_complexity, Some(6));
	}

	#[test]
	fn cognitive_nested_function_boundary() {
		let m = metrics_for(
			"function f() { if (x) {} function inner() { if (y) {} } }",
		);
		// Only counts outer function's if; inner function is separate scope
		assert_eq!(m.cognitive_complexity, Some(1));
	}
}
