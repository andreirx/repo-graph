/**
 * AST-derived deterministic metrics for TypeScript functions.
 *
 * These are pure computations over tree-sitter AST nodes.
 * No symbol resolution, no type information, no external dependencies.
 * Every function body in → numeric metrics out.
 *
 * Counting rules are documented here and versioned via
 * EXTRACTION_SEMANTICS in src/version.ts.
 */

import type { Node as SyntaxNode } from "web-tree-sitter";

/**
 * Metrics computed for a single function/method body.
 */
export interface FunctionMetrics {
	/** Cyclomatic complexity. Base 1 + one per decision point. */
	cyclomaticComplexity: number;
	/** Number of formal parameters. */
	parameterCount: number;
	/** Deepest nesting of control flow structures. */
	maxNestingDepth: number;
}

// ── Decision point node types ─────────────────────────────────────────
// Each adds 1 to cyclomatic complexity.

/** Statement-level decision points. */
const DECISION_STATEMENTS = new Set([
	"if_statement",
	"for_statement",
	"for_in_statement",
	"while_statement",
	"do_statement",
	"catch_clause",
	"switch_case", // each case (not the switch itself)
]);

/** Expression-level decision points (short-circuit / conditional). */
const DECISION_EXPRESSIONS = new Set(["ternary_expression"]);

/** Binary operators that are decision points. */
const DECISION_OPERATORS = new Set(["&&", "||", "??"]);

// ── Nesting node types ────────────────────────────────────────────────
// Each increases nesting depth by 1 for its children.

const NESTING_NODES = new Set([
	"if_statement",
	// else_clause is NOT a nesting level — it's a branch at the same
	// level as the if. Including it would make every else-if chain
	// appear deeply nested.
	"for_statement",
	"for_in_statement",
	"while_statement",
	"do_statement",
	"switch_statement",
	"try_statement",
	"catch_clause",
]);

// ── Computation ───────────────────────────────────────────────────────

/**
 * Compute metrics for a function body.
 *
 * @param body - The statement_block AST node of the function body.
 * @param params - The formal_parameters AST node (may be null).
 */
export function computeFunctionMetrics(
	body: SyntaxNode,
	params: SyntaxNode | null,
): FunctionMetrics {
	let complexity = 1; // base complexity
	let maxDepth = 0;

	function walk(node: SyntaxNode, currentDepth: number): void {
		// Track nesting depth.
		// Special case: an if_statement directly inside an else_clause is
		// an "else if" continuation, not a deeper nesting level. In
		// tree-sitter, `else if (x)` is `else_clause -> if_statement`.
		// We skip the nesting increment for this specific parent/child pair.
		let depth = currentDepth;
		if (NESTING_NODES.has(node.type)) {
			const isElseIf =
				node.type === "if_statement" && node.parent?.type === "else_clause";
			if (!isElseIf) {
				depth = currentDepth + 1;
				if (depth > maxDepth) {
					maxDepth = depth;
				}
			}
		}

		// Count decision points
		if (DECISION_STATEMENTS.has(node.type)) {
			complexity++;
		}
		if (DECISION_EXPRESSIONS.has(node.type)) {
			complexity++;
		}
		// Binary expressions with short-circuit operators
		if (node.type === "binary_expression") {
			const op = node.children.find(
				(c) => c.type === "&&" || c.type === "||" || c.type === "??",
			);
			if (op && DECISION_OPERATORS.has(op.type)) {
				complexity++;
			}
		}

		// Recurse into children (but not into nested function bodies —
		// those get their own metrics)
		for (const child of node.children) {
			if (
				child.type === "function_declaration" ||
				child.type === "generator_function_declaration" ||
				child.type === "generator_function" ||
				child.type === "arrow_function" ||
				child.type === "function_expression" ||
				child.type === "class_declaration" ||
				child.type === "method_definition"
			) {
				continue;
			}
			walk(child, depth);
		}
	}

	walk(body, 0);

	// Parameter count
	let paramCount = 0;
	if (params) {
		for (const child of params.children) {
			if (
				child.type === "required_parameter" ||
				child.type === "optional_parameter" ||
				child.type === "rest_parameter"
			) {
				paramCount++;
			}
		}
	}

	return {
		cyclomaticComplexity: complexity,
		parameterCount: paramCount,
		maxNestingDepth: maxDepth,
	};
}
