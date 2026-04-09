/**
 * Python language extractor — first slice (syntax-only).
 *
 * Parses .py files using web-tree-sitter with tree-sitter-python.
 * Extracts:
 *   - FILE node (one per file)
 *   - SYMBOL nodes: functions, classes, methods (including decorated)
 *   - IMPORTS edges from `import` and `from...import` statements
 *   - CALLS edges from function/method call expressions
 *   - ImportBinding records for classifier
 *   - Basic cyclomatic complexity metrics
 *
 * Visibility: top-level names are EXPORT by default (Python module
 * exports everything not prefixed with _). Names starting with _
 * are PRIVATE.
 *
 * Two-pass extraction: a pre-scan determines the winning (last)
 * definition for each function/class name at every scope level
 * (module root and class bodies). Shadowed definitions are
 * suppressed — no node, no edges, no metrics emitted. This
 * matches Python runtime semantics where the last `def`/`class`
 * binding at a scope level shadows all earlier ones.
 *
 * NOT in this first slice:
 *   - Semantic enrichment (pyright/mypy)
 *   - Framework detectors (Django, Flask, FastAPI)
 *   - Decorator-based edge extraction
 *   - Comprehension/generator complexity
 *   - Type stub (.pyi) handling
 *   - __all__ export filtering
 *   - Diagnostic reporting for shadowed definitions (TECH-DEBT)
 *
 * Maturity: PROTOTYPE. Sufficient to index Python files and extract
 * symbols, imports, and calls. Stdlib imports are classifiable via
 * runtimeBuiltins.moduleSpecifiers. Third-party imports (pip packages)
 * lack dependency context until a pyproject.toml/requirements.txt
 * reader is implemented — they fall through to unknown/heuristic
 * classification.
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { v4 as uuidv4 } from "uuid";
import { Language, Parser, type Node as SyntaxNode } from "web-tree-sitter";
import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";
import type { GraphNode, SourceLocation } from "../../../core/model/index.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Resolution,
	Visibility,
} from "../../../core/model/index.js";
import type {
	ExtractedMetrics,
	ExtractionResult,
	ExtractorPort,
	ImportBinding,
	UnresolvedEdge,
} from "../../../core/ports/extractor.js";
import { PYTHON_RUNTIME_BUILTINS } from "./runtime-builtins.js";

const EXTRACTOR_NAME = "python-core:0.1.0";
const GRAMMARS_DIR = join(import.meta.dirname, "../../../../grammars");

// ── Extraction context ──────────────────────────────────────────────

interface ExtractionContext {
	source: string;
	filePath: string;
	fileUid: string;
	fileNodeUid: string;
	repoUid: string;
	snapshotUid: string;
	nodes: GraphNode[];
	edges: UnresolvedEdge[];
	metrics: Map<string, ExtractedMetrics>;
	importBindings: ImportBinding[];
	/** Track emitted variable stable keys to skip re-assignments. */
	seenVariableKeys: Set<string>;
}

// ── Extractor class ─────────────────────────────────────────────────

export class PythonExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["python"];
	readonly runtimeBuiltins: RuntimeBuiltinsSet = PYTHON_RUNTIME_BUILTINS;

	private parser: Parser | null = null;
	private language: Language | null = null;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();
		const wasmPath = join(GRAMMARS_DIR, "tree-sitter-python.wasm");
		await readFile(wasmPath);
		this.language = await Language.load(wasmPath);
	}

	async extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult> {
		if (!this.parser || !this.language) {
			throw new Error("PythonExtractor not initialized. Call initialize() first.");
		}

		this.parser.setLanguage(this.language);
		const tree = this.parser.parse(source);
		if (!tree) {
			throw new Error(`Failed to parse ${filePath}`);
		}

		try {

		const fileNodeUid = uuidv4();
		const ctx: ExtractionContext = {
			source,
			filePath,
			fileUid,
			fileNodeUid,
			repoUid,
			snapshotUid,
			nodes: [],
			edges: [],
			metrics: new Map(),
			importBindings: [],
			seenVariableKeys: new Set(),
		};

		ctx.nodes.push({
			nodeUid: fileNodeUid,
			snapshotUid,
			repoUid,
			stableKey: `${repoUid}:${filePath}:FILE`,
			kind: NodeKind.FILE,
			subtype: filePath.includes("test") ? NodeSubtype.TEST_FILE : NodeSubtype.SOURCE,
			name: filePath.split("/").pop() ?? filePath,
			qualifiedName: filePath,
			fileUid,
			parentNodeUid: null,
			location: null,
			signature: null,
			visibility: null,
			docComment: null,
			metadataJson: null,
		});

		// Two-pass extraction: pre-scan determines the winning (last)
		// definition for each name at module scope. Python allows
		// same-name redefinitions — only the last is callable at runtime.
		const winningDefs = this.scanWinningDefinitions(tree.rootNode.children);

		for (const child of tree.rootNode.children) {
			this.processTopLevel(child, ctx, null, winningDefs);
		}

		return {
			nodes: ctx.nodes,
			edges: ctx.edges,
			metrics: ctx.metrics,
			importBindings: ctx.importBindings,
		};

		} finally {
			tree.delete();
		}
	}

	// ── Two-pass definition scanning ──────────────────────────────

	/**
	 * Pre-scan a scope (module root or class body) to determine the
	 * winning definition for each function/class name. In Python, the
	 * last same-name `def`/`class` at the same scope level shadows all
	 * earlier ones — only the last is callable at runtime.
	 *
	 * Returns a Set of position keys ("row:col") for winning definitions.
	 * Non-winners are suppressed during emission (no node, no edges, no
	 * metrics). Shadowed definitions are silently dropped — a diagnostic
	 * channel for reporting them does not yet exist (see TECH-DEBT).
	 */
	private scanWinningDefinitions(children: SyntaxNode[]): Set<string> {
		const lastDefByName = new Map<string, string>();

		for (const child of children) {
			let defNode: SyntaxNode | null = null;

			if (child.type === "function_definition" || child.type === "class_definition") {
				defNode = child;
			} else if (child.type === "decorated_definition") {
				for (const inner of child.children) {
					if (inner.type === "function_definition" || inner.type === "class_definition") {
						defNode = inner;
						break;
					}
				}
			}

			if (defNode) {
				const nameNode = defNode.childForFieldName("name");
				if (nameNode) {
					const posKey = `${defNode.startPosition.row}:${defNode.startPosition.column}`;
					lastDefByName.set(nameNode.text, posKey);
				}
			}
		}

		return new Set(lastDefByName.values());
	}

	// ── Top-level dispatch ─────────────────────────────────────────

	private processTopLevel(
		node: SyntaxNode,
		ctx: ExtractionContext,
		parentClassNode: GraphNode | null,
		winningDefs: Set<string>,
	): void {
		switch (node.type) {
			case "import_statement":
				this.processImport(node, ctx);
				break;
			case "import_from_statement":
				this.processFromImport(node, ctx);
				break;
			case "class_definition":
				this.processClass(node, ctx, winningDefs);
				break;
			case "function_definition":
				this.processFunction(node, ctx, parentClassNode, winningDefs);
				break;
			case "decorated_definition":
				this.processDecorated(node, ctx, parentClassNode, winningDefs);
				break;
			case "expression_statement":
				this.processExpressionStatement(node, ctx, parentClassNode);
				break;
			default:
				break;
		}
	}

	// ── Imports ────────────────────────────────────────────────────

	private processImport(node: SyntaxNode, ctx: ExtractionContext): void {
		// `import os` or `import os.path`
		for (const child of node.children) {
			if (child.type === "dotted_name") {
				const specifier = child.text;
				const identifier = specifier.split(".")[0];
				this.emitImportEdge(ctx, specifier, node);
				ctx.importBindings.push({
					identifier,
					specifier,
					isRelative: false,
					location: locationFromNode(node),
					isTypeOnly: false,
				});
			} else if (child.type === "aliased_import") {
				const nameNode = child.childForFieldName("name");
				const aliasNode = child.childForFieldName("alias");
				if (nameNode) {
					const specifier = nameNode.text;
					const identifier = aliasNode?.text ?? specifier.split(".")[0];
					this.emitImportEdge(ctx, specifier, node);
					ctx.importBindings.push({
						identifier,
						specifier,
						isRelative: false,
						location: locationFromNode(node),
						isTypeOnly: false,
					});
				}
			}
		}
	}

	private processFromImport(node: SyntaxNode, ctx: ExtractionContext): void {
		// `from typing import List, Optional`
		// `from .utils import helper`
		let specifier = "";
		let isRelative = false;

		for (const child of node.children) {
			if (child.type === "dotted_name" && specifier === "") {
				specifier = child.text;
			} else if (child.type === "relative_import") {
				isRelative = true;
				const prefix = child.children.find((c) => c.type === "import_prefix");
				const name = child.children.find((c) => c.type === "dotted_name");
				specifier = (prefix?.text ?? ".") + (name?.text ?? "");
			}
		}

		if (!specifier) return;

		this.emitImportEdge(ctx, specifier, node);

		// Extract each imported name.
		let pastImportKeyword = false;
		for (const child of node.children) {
			if (child.type === "import") {
				pastImportKeyword = true;
				continue;
			}
			if (!pastImportKeyword) continue;

			if (child.type === "dotted_name") {
				const identifier = child.text;
				ctx.importBindings.push({
					identifier,
					specifier,
					isRelative,
					location: locationFromNode(node),
					isTypeOnly: false,
				});
			} else if (child.type === "aliased_import") {
				const nameNode = child.childForFieldName("name");
				const aliasNode = child.childForFieldName("alias");
				if (nameNode) {
					const identifier = aliasNode?.text ?? nameNode.text;
					ctx.importBindings.push({
						identifier,
						specifier,
						isRelative,
						location: locationFromNode(node),
						isTypeOnly: false,
					});
				}
			}
		}
	}

	private emitImportEdge(
		ctx: ExtractionContext,
		specifier: string,
		node: SyntaxNode,
	): void {
		ctx.edges.push({
			edgeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			sourceNodeUid: ctx.fileNodeUid,
			targetKey: specifier,
			type: EdgeType.IMPORTS,
			resolution: Resolution.STATIC,
			extractor: EXTRACTOR_NAME,
			location: locationFromNode(node),
			metadataJson: null,
		});
	}

	// ── Classes ───────────────────────────────────────────────────

	private processClass(
		node: SyntaxNode,
		ctx: ExtractionContext,
		winningDefs: Set<string>,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		// Skip shadowed class definitions — only the last same-name
		// class at this scope level is the winning definition.
		const posKey = `${node.startPosition.row}:${node.startPosition.column}`;
		if (!winningDefs.has(posKey)) return;

		const name = nameNode.text;

		const visibility = name.startsWith("_") ? Visibility.PRIVATE : Visibility.EXPORT;

		const classNodeUid = uuidv4();
		const classGraphNode: GraphNode = {
			nodeUid: classNodeUid,
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${name}:SYMBOL:CLASS`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.CLASS,
			name,
			qualifiedName: name,
			fileUid: ctx.fileUid,
			parentNodeUid: ctx.fileNodeUid,
			location: locationFromNode(node),
			signature: null,
			visibility,
			docComment: extractDocstring(node),
			metadataJson: null,
		};
		ctx.nodes.push(classGraphNode);

		// Process base classes as IMPLEMENTS edges.
		const argList = node.childForFieldName("superclasses");
		if (argList) {
			for (const arg of argList.children) {
				if (arg.type === "identifier" || arg.type === "attribute") {
					ctx.edges.push({
						edgeUid: uuidv4(),
						snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid,
						sourceNodeUid: classNodeUid,
						targetKey: arg.text,
						type: EdgeType.IMPLEMENTS,
						resolution: Resolution.STATIC,
						extractor: EXTRACTOR_NAME,
						location: locationFromNode(arg),
						metadataJson: null,
					});
				}
			}
		}

		// Process class body — pre-scan for method-level shadowing too.
		const body = node.childForFieldName("body");
		if (body) {
			const classWinningDefs = this.scanWinningDefinitions(body.children);
			for (const child of body.children) {
				this.processTopLevel(child, ctx, classGraphNode, classWinningDefs);
			}
		}
	}

	// ── Functions / Methods ───────────────────────────────────────

	private processFunction(
		node: SyntaxNode,
		ctx: ExtractionContext,
		parentClassNode: GraphNode | null,
		winningDefs: Set<string>,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		// Skip shadowed function definitions — only the last same-name
		// def at this scope level is callable at runtime.
		const posKey = `${node.startPosition.row}:${node.startPosition.column}`;
		if (!winningDefs.has(posKey)) return;

		const name = nameNode.text;

		const isMethod = parentClassNode !== null;
		const isConstructor = isMethod && name === "__init__";
		const visibility = name.startsWith("_") && !name.startsWith("__")
			? Visibility.PRIVATE
			: Visibility.EXPORT;

		const subtype = isConstructor
			? NodeSubtype.CONSTRUCTOR
			: isMethod
				? NodeSubtype.METHOD
				: NodeSubtype.FUNCTION;

		const qualifiedName = parentClassNode
			? `${parentClassNode.name}.${name}`
			: name;

		// Build signature from parameters.
		const params = node.childForFieldName("parameters");
		const returnType = node.childForFieldName("return_type");
		let signature = qualifiedName;
		if (params) {
			// Filter out 'self' and 'cls' from displayed signature.
			const paramTexts: string[] = [];
			for (const p of params.children) {
				if (p.type === "identifier" && (p.text === "self" || p.text === "cls")) continue;
				if (p.type === "typed_parameter" || p.type === "default_parameter" ||
					p.type === "typed_default_parameter" || p.type === "identifier") {
					paramTexts.push(p.text);
				}
			}
			signature = `${qualifiedName}(${paramTexts.join(", ")})`;
		}
		if (returnType) {
			signature += ` -> ${returnType.text}`;
		}

		const funcNodeUid = uuidv4();
		const stableKey = `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${subtype}`;

		ctx.nodes.push({
			nodeUid: funcNodeUid,
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey,
			kind: NodeKind.SYMBOL,
			subtype,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentClassNode?.nodeUid ?? ctx.fileNodeUid,
			location: locationFromNode(node),
			signature,
			visibility,
			docComment: extractDocstring(node),
			metadataJson: null,
		});

		// Extract call edges from the function body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCalls(body, funcNodeUid, ctx);
			// Compute metrics.
			const cc = computeCyclomaticComplexity(body);
			const paramCount = countParameters(params);
			const nesting = computeMaxNesting(body, 0);
			ctx.metrics.set(stableKey, {
				cyclomaticComplexity: cc,
				parameterCount: paramCount,
				maxNestingDepth: nesting,
			});
		}
	}

	// ── Decorated definitions ────────────────────────────────────

	private processDecorated(
		node: SyntaxNode,
		ctx: ExtractionContext,
		parentClassNode: GraphNode | null,
		winningDefs: Set<string>,
	): void {
		// A decorated_definition contains decorator(s) and then a
		// function_definition or class_definition. The inner definition
		// node is what gets checked against winningDefs (the scanner
		// unwraps decorated_definition the same way).
		for (const child of node.children) {
			if (child.type === "function_definition") {
				this.processFunction(child, ctx, parentClassNode, winningDefs);
			} else if (child.type === "class_definition") {
				this.processClass(child, ctx, winningDefs);
			}
		}
	}

	// ── Expression statements (top-level assignments) ────────────

	private processExpressionStatement(
		node: SyntaxNode,
		ctx: ExtractionContext,
		parentClassNode: GraphNode | null,
	): void {
		// Top-level assignments: `API_URL = "..."` or `MAX_RETRIES = 3`
		for (const child of node.children) {
			if (child.type !== "assignment") continue;
			const left = child.childForFieldName("left");
			if (!left || left.type !== "identifier") continue;

			const name = left.text;
			const visibility = name.startsWith("_")
				? Visibility.PRIVATE
				: Visibility.EXPORT;

			const qualifiedName = parentClassNode
				? `${parentClassNode.name}.${name}`
				: name;

			const stableKey = `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:VARIABLE`;
			// Skip re-assignments to the same variable name in the same file.
			// Python allows `result = expr1; result = expr2` — only emit the first.
			if (ctx.seenVariableKeys.has(stableKey)) continue;
			ctx.seenVariableKeys.add(stableKey);

			ctx.nodes.push({
				nodeUid: uuidv4(),
				snapshotUid: ctx.snapshotUid,
				repoUid: ctx.repoUid,
				stableKey,
				kind: NodeKind.SYMBOL,
				subtype: NodeSubtype.VARIABLE,
				name,
				qualifiedName,
				fileUid: ctx.fileUid,
				parentNodeUid: parentClassNode?.nodeUid ?? ctx.fileNodeUid,
				location: locationFromNode(node),
				signature: null,
				visibility,
				docComment: null,
				metadataJson: null,
			});
		}
	}

	// ── Call extraction ──────────────────────────────────────────

	private extractCalls(
		node: SyntaxNode,
		sourceNodeUid: string,
		ctx: ExtractionContext,
	): void {
		if (node.type === "call") {
			const fn = node.childForFieldName("function");
			if (fn) {
				const callee = extractCalleeName(fn);
				if (callee) {
					ctx.edges.push({
						edgeUid: uuidv4(),
						snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid,
						sourceNodeUid,
						targetKey: callee,
						type: EdgeType.CALLS,
						resolution: Resolution.STATIC,
						extractor: EXTRACTOR_NAME,
						location: locationFromNode(node),
						metadataJson: JSON.stringify({ rawCalleeName: callee }),
					});
				}
			}
		}
		for (const child of node.children) {
			this.extractCalls(child, sourceNodeUid, ctx);
		}
	}
}

// ── Helpers ─────────────────────────────────────────────────────────

function locationFromNode(node: SyntaxNode): SourceLocation {
	return {
		lineStart: node.startPosition.row + 1,
		colStart: node.startPosition.column,
		lineEnd: node.endPosition.row + 1,
		colEnd: node.endPosition.column,
	};
}

/**
 * Extract the callee name from a call expression's function node.
 * - `foo()` → "foo"
 * - `self.bar()` → "self.bar"
 * - `obj.method()` → "obj.method"
 * - `module.func()` → "module.func"
 */
function extractCalleeName(node: SyntaxNode): string | null {
	if (node.type === "identifier") {
		return node.text;
	}
	if (node.type === "attribute") {
		return node.text;
	}
	return null;
}

/**
 * Extract the docstring from a function/class definition.
 * Python convention: first statement in body is a string literal.
 */
function extractDocstring(defNode: SyntaxNode): string | null {
	const body = defNode.childForFieldName("body");
	if (!body) return null;
	const first = body.children[0];
	if (!first) return null;
	if (first.type === "expression_statement") {
		const expr = first.children[0];
		if (expr?.type === "string") {
			return expr.text;
		}
	}
	return null;
}

/**
 * Compute cyclomatic complexity of a function body.
 * Counts branching constructs: if, elif, for, while, except, and, or,
 * assert, with, conditional expression.
 */
function computeCyclomaticComplexity(body: SyntaxNode): number {
	let cc = 1; // Base complexity.
	const walk = (node: SyntaxNode) => {
		switch (node.type) {
			case "if_statement":
			case "elif_clause":
			case "for_statement":
			case "while_statement":
			case "except_clause":
			case "with_statement":
			case "assert_statement":
				cc++;
				break;
			case "conditional_expression":
				cc++;
				break;
			case "boolean_operator":
				// `and` / `or` each add a path.
				cc++;
				break;
		}
		for (const child of node.children) {
			walk(child);
		}
	};
	walk(body);
	return cc;
}

/**
 * Count non-self/cls parameters.
 */
function countParameters(params: SyntaxNode | null): number {
	if (!params) return 0;
	let count = 0;
	for (const child of params.children) {
		if (child.type === "identifier" && (child.text === "self" || child.text === "cls")) continue;
		if (child.type === "identifier" || child.type === "typed_parameter" ||
			child.type === "default_parameter" || child.type === "typed_default_parameter" ||
			child.type === "list_splat_pattern" || child.type === "dictionary_splat_pattern") {
			count++;
		}
	}
	return count;
}

/**
 * Compute maximum nesting depth in a function body.
 */
function computeMaxNesting(node: SyntaxNode, depth: number): number {
	let maxDepth = depth;
	const nestingTypes = new Set([
		"if_statement", "for_statement", "while_statement",
		"try_statement", "with_statement",
	]);
	for (const child of node.children) {
		if (nestingTypes.has(child.type)) {
			const childMax = computeMaxNesting(child, depth + 1);
			if (childMax > maxDepth) maxDepth = childMax;
		} else {
			const childMax = computeMaxNesting(child, depth);
			if (childMax > maxDepth) maxDepth = childMax;
		}
	}
	return maxDepth;
}
