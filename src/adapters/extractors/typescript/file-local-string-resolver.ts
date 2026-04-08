/**
 * FileLocalStringResolver — file-local constant string propagation.
 *
 * Resolves string-valued `const` bindings within a single TS/JS file.
 * Not a general interpreter. Evaluates a narrow deterministic subset:
 *   - string literals: `"foo"`, `'bar'`
 *   - template literals: `` `${X}/path` ``
 *   - binary `+` concatenation: `A + "/suffix"`
 *   - references to previously-resolved same-file constants
 *   - environment variable placeholders: treated as opaque prefix,
 *     stripped from the resolved value (not truly resolved)
 *
 * Scope: same-file, top-level `const` only. No imports, no function
 * calls, no object property flow, no dynamic expressions.
 *
 * Consumers:
 *   - HTTP client boundary extractor (resolve base URL constants)
 *   - Future: Express route extraction, CLI modeling, config readers
 *
 * Maturity: PROTOTYPE. Sufficient to resolve the glamCRM base-URL
 * constant pattern that accounts for 11/16 unmatched consumer facts.
 *
 * Known limitations:
 *   - Only `const` declarations at program/module top level
 *   - No `let`/`var` (mutable — cannot propagate safely)
 *   - No destructuring: `const { BASE } = config`
 *   - No function return values: `const url = getBaseUrl()`
 *   - No cross-file resolution: `import { BASE } from "./config"`
 *   - No object property access: `const url = config.baseUrl`
 *   - Template literal nesting limited to one level
 *   - Cycle detection: bounded at 10 resolution steps per binding
 */

import { type Language, type Node as SyntaxNode, Parser } from "web-tree-sitter";
import { join } from "node:path";
import { readFile } from "node:fs/promises";

// ── Output types ────────────────────────────────────────────────────

/**
 * How the string value was determined.
 *
 *   literal      — direct string literal: `"/api/v2/products"`
 *   template     — template literal with all parts resolved
 *   concat       — binary `+` concatenation with all parts resolved
 *   binding_ref  — resolved entirely through another binding
 *   env_prefixed — contains an environment variable prefix that was
 *                  stripped (value is the suffix after the env var)
 */
export type ResolvedStringBasis =
	| "literal"
	| "template"
	| "concat"
	| "binding_ref"
	| "env_prefixed";

/**
 * A resolved string value from a file-local constant binding.
 */
export interface ResolvedStringValue {
	/** The resolved string value. */
	value: string;
	/** How the value was determined. */
	basis: ResolvedStringBasis;
	/**
	 * Confidence in the resolved value.
	 *   1.0  — fully literal, no dynamic parts
	 *   0.9  — resolved through binding references
	 *   0.8  — template/concat with some resolved parts
	 *   0.7  — env_prefixed (env var stripped, suffix retained)
	 *   0.0  — unresolvable (dynamic expression)
	 */
	confidence: number;
	/** Count of expression segments that could not be resolved. */
	unresolvedSegments: number;
}

/** A binding table: constant name → resolved value. */
export type StringBindingTable = Map<string, ResolvedStringValue>;

// ── Environment variable patterns ───────────────────────────────────

/**
 * Detect whether an AST node is an environment variable access.
 * Recognized patterns:
 *   - import.meta.env.VITE_API_URL
 *   - process.env.API_URL
 *   - import.meta.env.<anything>
 *   - process.env.<anything>
 */
function isEnvVarAccess(node: SyntaxNode): boolean {
	if (node.type !== "member_expression") return false;
	const text = node.text;
	return (
		text.startsWith("import.meta.env.") ||
		text.startsWith("process.env.")
	);
}

// ── Expression evaluator ────────────────────────────────────────────

/** Maximum resolution depth to prevent cycles. */
const MAX_RESOLVE_DEPTH = 10;

/**
 * Evaluate a string-like expression node against the current binding table.
 * Returns null if the expression cannot be resolved to a string.
 */
function evaluateStringExpr(
	node: SyntaxNode,
	bindings: StringBindingTable,
	depth: number,
): ResolvedStringValue | null {
	if (depth > MAX_RESOLVE_DEPTH) return null;

	switch (node.type) {
		// ── String literal: "foo" or 'bar' ────────────────────────
		case "string": {
			// Extract content between quotes.
			const fragment = node.children.find(
				(c) => c.type === "string_fragment",
			);
			return {
				value: fragment?.text ?? "",
				basis: "literal",
				confidence: 1.0,
				unresolvedSegments: 0,
			};
		}

		// ── Template literal: `${X}/path` ─────────────────────────
		case "template_string": {
			let value = "";
			let unresolvedSegments = 0;
			let hasEnvPrefix = false;
			let hasBindingRef = false;
			let minConfidence = 1.0;

			for (const child of node.children) {
				if (child.type === "string_fragment") {
					value += child.text;
				} else if (child.type === "template_substitution") {
					// The expression inside ${...}
					const expr = child.children.find(
						(c) =>
							c.type !== "${" &&
							c.type !== "}" &&
							c.type !== "$",
					);
					if (!expr) {
						unresolvedSegments++;
						continue;
					}

					if (isEnvVarAccess(expr)) {
						// Environment variable — strip it, mark as env_prefixed.
						hasEnvPrefix = true;
						minConfidence = Math.min(minConfidence, 0.7);
						continue;
					}

					// Try to resolve the expression.
					const resolved = evaluateStringExpr(expr, bindings, depth + 1);
					if (resolved) {
						value += resolved.value;
						hasBindingRef = true;
						minConfidence = Math.min(minConfidence, resolved.confidence);
						unresolvedSegments += resolved.unresolvedSegments;
					} else {
						unresolvedSegments++;
						minConfidence = 0;
					}
				}
				// Skip backtick nodes (` character).
			}

			const basis: ResolvedStringBasis = hasEnvPrefix
				? "env_prefixed"
				: hasBindingRef
					? "template"
					: "literal";

			return {
				value,
				basis,
				confidence:
					unresolvedSegments > 0 ? 0 : minConfidence,
				unresolvedSegments,
			};
		}

		// ── Identifier reference: BACKEND_URL ─────────────────────
		case "identifier": {
			const name = node.text;
			const bound = bindings.get(name);
			if (bound && bound.confidence > 0) {
				return {
					value: bound.value,
					basis: "binding_ref",
					confidence: Math.min(0.9, bound.confidence),
					unresolvedSegments: bound.unresolvedSegments,
				};
			}
			return null;
		}

		// ── Binary concatenation: A + "/suffix" ───────────────────
		case "binary_expression": {
			const operator = node.childForFieldName("operator");
			if (!operator || operator.text !== "+") return null;

			const left = node.childForFieldName("left");
			const right = node.childForFieldName("right");
			if (!left || !right) return null;

			const leftVal = evaluateStringExpr(left, bindings, depth + 1);
			const rightVal = evaluateStringExpr(right, bindings, depth + 1);

			if (!leftVal && !rightVal) return null;

			// If left is an env var access, treat it as env_prefixed.
			if (isEnvVarAccess(left) && rightVal) {
				return {
					value: rightVal.value,
					basis: "env_prefixed",
					confidence: Math.min(0.7, rightVal.confidence),
					unresolvedSegments: rightVal.unresolvedSegments,
				};
			}

			// If both sides resolved, concatenate.
			if (leftVal && rightVal) {
				return {
					value: leftVal.value + rightVal.value,
					basis: "concat",
					confidence: Math.min(leftVal.confidence, rightVal.confidence),
					unresolvedSegments:
						leftVal.unresolvedSegments + rightVal.unresolvedSegments,
				};
			}

			// Partial resolution: one side resolved, other didn't.
			return null;
		}

		// ── Member expression: import.meta.env.X, process.env.X ───
		case "member_expression": {
			if (isEnvVarAccess(node)) {
				// Return empty value — env vars are stripped, not resolved.
				return {
					value: "",
					basis: "env_prefixed",
					confidence: 0.7,
					unresolvedSegments: 0,
				};
			}
			return null;
		}

		default:
			return null;
	}
}

// ── Resolver ────────────────────────────────────────────────────────

/**
 * Resolve file-local string-valued `const` bindings from source text.
 *
 * Walks top-level `const` declarations in source order. Each binding's
 * initializer is evaluated against previously-resolved bindings, so
 * `const A = "x"; const B = A + "/y"` resolves B to "x/y".
 *
 * @param rootNode - The tree-sitter root node (program) of the parsed file.
 * @returns A binding table mapping constant names to resolved values.
 */
export function resolveFileLocalStrings(
	rootNode: SyntaxNode,
): StringBindingTable {
	const bindings: StringBindingTable = new Map();

	for (const child of rootNode.children) {
		// Find top-level `const` declarations.
		// Two shapes:
		//   - `const X = ...`          → child.type === "lexical_declaration"
		//   - `export const X = ...`   → child.type === "export_statement"
		//                                 containing a lexical_declaration
		let lexDecl: SyntaxNode | null = null;
		if (child.type === "lexical_declaration") {
			lexDecl = child;
		} else if (child.type === "export_statement") {
			lexDecl = child.children.find(
				(c) => c.type === "lexical_declaration",
			) ?? null;
		}
		if (!lexDecl) continue;

		// Check it's actually `const` (not `let`).
		const hasConst = lexDecl.children.some(
			(c) => c.type === "const",
		);
		if (!hasConst) continue;

		// Process each declarator in the declaration.
		for (const decl of lexDecl.children) {
			if (decl.type !== "variable_declarator") continue;

			const nameNode = decl.childForFieldName("name");
			const valueNode = decl.childForFieldName("value");
			if (!nameNode || !valueNode) continue;
			if (nameNode.type !== "identifier") continue;

			const name = nameNode.text;

			// Evaluate the initializer.
			const resolved = evaluateStringExpr(valueNode, bindings, 0);
			if (resolved && resolved.confidence > 0) {
				bindings.set(name, resolved);
			}
		}
	}

	return bindings;
}

// ── Standalone resolver class (for use outside the TS extractor) ────

const GRAMMARS_DIR = join(
	import.meta.dirname,
	"../../../../grammars",
);

/**
 * Standalone file-local string resolver.
 *
 * Manages its own tree-sitter parser lifecycle. Use when the caller
 * does not have access to the TS extractor's parser instance.
 *
 * For use within the TS extractor pipeline, prefer calling
 * `resolveFileLocalStrings(rootNode)` directly with the already-parsed tree.
 */
export class FileLocalStringResolver {
	private parser: Parser | null = null;
	private tsLanguage: Language | null = null;
	private tsxLanguage: Language | null = null;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();

		const tsWasmPath = join(GRAMMARS_DIR, "tree-sitter-typescript.wasm");
		const tsxWasmPath = join(GRAMMARS_DIR, "tree-sitter-tsx.wasm");

		await readFile(tsWasmPath);
		await readFile(tsxWasmPath);

		const { Language: Lang } = await import("web-tree-sitter");
		this.tsLanguage = await Lang.load(tsWasmPath);
		this.tsxLanguage = await Lang.load(tsxWasmPath);
	}

	/**
	 * Resolve string bindings from a TS/JS source file.
	 *
	 * @param source - Source text of the file.
	 * @param filePath - Repo-relative path (used to select TS vs TSX grammar).
	 * @returns Binding table, or empty map if parsing fails.
	 */
	resolve(source: string, filePath: string): StringBindingTable {
		if (!this.parser || !this.tsLanguage || !this.tsxLanguage) {
			throw new Error(
				"FileLocalStringResolver not initialized. Call initialize() first.",
			);
		}

		const isTsx =
			filePath.endsWith(".tsx") || filePath.endsWith(".jsx");
		this.parser.setLanguage(isTsx ? this.tsxLanguage : this.tsLanguage);

		const tree = this.parser.parse(source);
		if (!tree) return new Map();

		try {
			return resolveFileLocalStrings(tree.rootNode);
		} finally {
			tree.delete();
		}
	}
}
