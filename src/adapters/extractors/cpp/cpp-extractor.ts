/**
 * C/C++ language extractor — first slice (syntax-only).
 *
 * Parses .c, .h, .cpp, .hpp, .cc, .cxx files using web-tree-sitter.
 * Uses tree-sitter-c for .c/.h and tree-sitter-cpp for C++ files.
 *
 * Extracts:
 *   - FILE node (one per file)
 *   - SYMBOL nodes: functions, structs, classes, typedefs, enums,
 *     namespaces, methods, constructors
 *   - IMPORTS edges from #include directives
 *   - CALLS edges from function/method call expressions
 *   - ImportBinding records for #include directives
 *   - Basic cyclomatic complexity metrics
 *
 * Visibility: functions/types with `static` are PRIVATE.
 * Everything else is EXPORT (C/C++ default external linkage).
 *
 * NOT in this first slice:
 *   - compile_commands.json integration
 *   - Macro expansion or preprocessor evaluation
 *   - Template instantiation tracking
 *   - Overload resolution
 *   - Header/source ownership policy
 *   - Translation-unit-aware resolution
 *
 * Maturity: PROTOTYPE. Sufficient to index C/C++ files and produce
 * a navigable graph. Preprocessor-heavy code will have gaps.
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
import { C_CPP_RUNTIME_BUILTINS } from "./runtime-builtins.js";

const EXTRACTOR_NAME = "cpp-core:0.1.0";
const GRAMMARS_DIR = join(import.meta.dirname, "../../../../grammars");

const CPP_EXTENSIONS = new Set([".cpp", ".hpp", ".cc", ".cxx", ".hxx", ".h++"]);

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
	/** Track emitted stable keys to prevent duplicates from typedef + struct double-emission. */
	seenStableKeys: Set<string>;
}

export class CppExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["c", "cpp"];
	readonly runtimeBuiltins: RuntimeBuiltinsSet = C_CPP_RUNTIME_BUILTINS;

	private parser: Parser | null = null;
	private cLanguage: Language | null = null;
	private cppLanguage: Language | null = null;
	private parseCount = 0;
	/** Reset parser every N parses to prevent WASM heap exhaustion on large repos. */
	private static readonly PARSER_RESET_INTERVAL = 5000;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();
		const cWasm = join(GRAMMARS_DIR, "tree-sitter-c.wasm");
		const cppWasm = join(GRAMMARS_DIR, "tree-sitter-cpp.wasm");
		await readFile(cWasm);
		await readFile(cppWasm);
		this.cLanguage = await Language.load(cWasm);
		this.cppLanguage = await Language.load(cppWasm);
	}

	async extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult> {
		if (!this.parser || !this.cLanguage || !this.cppLanguage) {
			throw new Error("CppExtractor not initialized.");
		}

		// Periodic parser reset to prevent WASM heap exhaustion on large repos.
		// tree-sitter's WASM runtime accumulates internal state across parses.
		// After thousands of parses, it can exhaust the 4GB WASM linear memory limit.
		this.parseCount++;
		if (this.parseCount % CppExtractor.PARSER_RESET_INTERVAL === 0) {
			this.parser.delete();
			this.parser = new Parser();
		}

		const ext = filePath.slice(filePath.lastIndexOf("."));
		const isCpp = CPP_EXTENSIONS.has(ext);
		this.parser.setLanguage(isCpp ? this.cppLanguage : this.cLanguage);

		const tree = this.parser.parse(source);
		if (!tree) throw new Error(`Failed to parse ${filePath}`);

		try {

		const fileNodeUid = uuidv4();
		const ctx: ExtractionContext = {
			source, filePath, fileUid, fileNodeUid,
			repoUid, snapshotUid,
			nodes: [], edges: [],
			metrics: new Map(),
			importBindings: [],
			seenStableKeys: new Set(),
		};

		const isTest = filePath.includes("test");
		ctx.nodes.push({
			nodeUid: fileNodeUid, snapshotUid, repoUid,
			stableKey: `${repoUid}:${filePath}:FILE`,
			kind: NodeKind.FILE,
			subtype: isTest ? NodeSubtype.TEST_FILE : NodeSubtype.SOURCE,
			name: filePath.split("/").pop() ?? filePath,
			qualifiedName: filePath,
			fileUid, parentNodeUid: null,
			location: null, signature: null, visibility: null,
			docComment: null, metadataJson: null,
		});

		this.walkTopLevel(tree.rootNode, ctx, null);

		return {
			nodes: ctx.nodes,
			edges: ctx.edges,
			metrics: ctx.metrics,
			importBindings: ctx.importBindings,
		};

		} finally {
			// Release the Tree's WASM memory. Without this, each parsed
			// tree accumulates in the WASM linear memory until GC runs
			// (which may never happen fast enough for 63k files).
			tree.delete();
		}
	}

	// ── Top-level dispatch ─────────────────────────────────────────

	private walkTopLevel(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
	): void {
		for (const child of node.children) {
			switch (child.type) {
				case "preproc_include":
					this.processInclude(child, ctx);
					break;
				case "function_definition":
					this.processFunction(child, ctx, namespace, null);
					break;
				case "declaration":
					this.processDeclaration(child, ctx, namespace);
					break;
				case "type_definition":
					this.processTypedef(child, ctx, namespace);
					break;
				case "struct_specifier":
				case "enum_specifier":
					this.processTaggedType(child, ctx, namespace);
					break;
				// C++ specific
				case "namespace_definition":
					this.processNamespace(child, ctx, namespace);
					break;
				case "class_specifier":
					this.processClass(child, ctx, namespace);
					break;
				case "template_declaration":
					// Walk into template to find the underlying declaration.
					this.walkTopLevel(child, ctx, namespace);
					break;
				// Preprocessor blocks: recurse into their contents.
				// #ifndef/#ifdef/#if guards wrap real declarations.
				case "preproc_ifdef":
				case "preproc_if":
				case "preproc_else":
				case "preproc_elif":
					this.walkTopLevel(child, ctx, namespace);
					break;
				default:
					break;
			}
		}
	}

	// ── #include ──────────────────────────────────────────────────

	private processInclude(node: SyntaxNode, ctx: ExtractionContext): void {
		let specifier = "";
		let isSystem = false;

		for (const child of node.children) {
			if (child.type === "system_lib_string") {
				// <stdio.h> → strip angle brackets.
				specifier = child.text.slice(1, -1);
				isSystem = true;
			} else if (child.type === "string_literal") {
				const content = child.children.find((c) => c.type === "string_content");
				specifier = content?.text ?? child.text.replace(/"/g, "");
				isSystem = false;
			}
		}

		if (!specifier) return;

		// For quoted (local) includes, store rawPath with "./" prefix so
		// the classifier's startsWith(".") check recognizes it as a
		// relative/local import. System includes have no rawPath.
		const metadataJson = isSystem
			? null
			: JSON.stringify({ rawPath: `./${specifier}` });

		ctx.edges.push({
			edgeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid, sourceNodeUid: ctx.fileNodeUid,
			targetKey: specifier, type: EdgeType.IMPORTS,
			resolution: Resolution.STATIC, extractor: EXTRACTOR_NAME,
			location: locationFromNode(node), metadataJson,
		});

		// Import binding: the "identifier" is the header basename (sans extension).
		// In C/C++, quoted includes (#include "foo.h") are always local/project
		// includes. Angle-bracket includes (#include <foo.h>) are system includes.
		// isRelative=true for ALL quoted includes — this feeds the classifier's
		// internal-import path, which is the correct semantics for C/C++.
		const identifier = specifier.split("/").pop()?.replace(/\.\w+$/, "") ?? specifier;
		ctx.importBindings.push({
			identifier,
			specifier,
			isRelative: !isSystem,
			location: locationFromNode(node),
			isTypeOnly: false,
		});
	}

	// ── Functions ─────────────────────────────────────────────────

	private processFunction(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
		className: string | null,
	): void {
		const declNode = node.childForFieldName("declarator");
		if (!declNode) return;

		const { name, qualifiedPrefix } = extractDeclaratorName(declNode);
		if (!name) return;

		const isStatic = node.children.some(
			(c) => c.type === "storage_class_specifier" && c.text === "static",
		);

		// Determine if this is a method definition (ClassName::method).
		const effectiveClass = qualifiedPrefix ?? className;
		const isMethod = effectiveClass !== null;
		const isConstructor = isMethod && name === effectiveClass;

		const subtype = isConstructor
			? NodeSubtype.CONSTRUCTOR
			: isMethod ? NodeSubtype.METHOD : NodeSubtype.FUNCTION;

		const fullQualified = buildQualifiedName(namespace, effectiveClass, name);

		const funcUid = uuidv4();
		const stableKey = `${ctx.repoUid}:${ctx.filePath}#${fullQualified}:SYMBOL:${subtype}`;

		// Dedup: forward declarations + definitions can collide.
		if (ctx.seenStableKeys.has(stableKey)) return;
		ctx.seenStableKeys.add(stableKey);

		// Build signature.
		const params = declNode.childForFieldName("parameters");
		const returnType = node.childForFieldName("type");
		let sig = fullQualified;
		if (params) sig = `${fullQualified}${params.text}`;
		if (returnType) sig = `${returnType.text} ${sig}`;

		ctx.nodes.push({
			nodeUid: funcUid, snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey,
			kind: NodeKind.SYMBOL, subtype,
			name, qualifiedName: fullQualified,
			fileUid: ctx.fileUid,
			parentNodeUid: ctx.fileNodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility: isStatic ? Visibility.PRIVATE : Visibility.EXPORT,
			docComment: extractPrecedingComment(node),
			metadataJson: null,
		});

		// Extract calls from function body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCalls(body, funcUid, ctx);
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

	// ── Declarations (forward decls, variables) ───────────────────

	private processDeclaration(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
	): void {
		// Check for struct/enum specifiers inside declarations.
		for (const child of node.children) {
			if (child.type === "struct_specifier" || child.type === "enum_specifier") {
				this.processTaggedType(child, ctx, namespace);
			}
			if (child.type === "class_specifier") {
				this.processClass(child, ctx, namespace);
			}
		}
	}

	// ── Typedef ───────────────────────────────────────────────────

	private processTypedef(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
	): void {
		const declNode = node.childForFieldName("declarator");
		if (!declNode) return;
		const name = declNode.type === "type_identifier" ? declNode.text : null;
		if (!name) return;

		// Check for inner named struct/enum. If the typedef name matches
		// the inner tag name (e.g., `typedef struct Foo { } Foo;`), only
		// emit the struct — don't create a separate TYPE_ALIAS that
		// collides on stable key.
		let innerTagName: string | null = null;
		for (const child of node.children) {
			if (child.type === "struct_specifier" || child.type === "enum_specifier") {
				const tagName = child.childForFieldName("name");
				if (tagName) {
					innerTagName = tagName.text;
					this.processTaggedType(child, ctx, namespace);
				}
			}
		}

		// Only emit TYPE_ALIAS if the typedef name differs from the inner tag.
		if (innerTagName !== name) {
			const fullQualified = buildQualifiedName(namespace, null, name);
			const stableKey = `${ctx.repoUid}:${ctx.filePath}#${fullQualified}:SYMBOL:TYPE_ALIAS`;

			// Dedup: #ifdef branches can define the same typedef twice.
			if (ctx.seenStableKeys.has(stableKey)) return;
			ctx.seenStableKeys.add(stableKey);

			ctx.nodes.push({
				nodeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
				repoUid: ctx.repoUid,
				stableKey,
				kind: NodeKind.SYMBOL, subtype: NodeSubtype.TYPE_ALIAS,
				name, qualifiedName: fullQualified,
				fileUid: ctx.fileUid, parentNodeUid: ctx.fileNodeUid,
				location: locationFromNode(node),
				signature: null,
				visibility: Visibility.EXPORT,
				docComment: null, metadataJson: null,
			});
		}
	}

	// ── Struct / Enum ─────────────────────────────────────────────

	private processTaggedType(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const isEnum = node.type === "enum_specifier";
		const subtype = isEnum ? NodeSubtype.ENUM : NodeSubtype.CLASS;
		const fullQualified = buildQualifiedName(namespace, null, name);
		const stableKey = `${ctx.repoUid}:${ctx.filePath}#${fullQualified}:SYMBOL:${subtype}`;

		// Dedup: typedef + struct can emit the same node twice.
		if (ctx.seenStableKeys.has(stableKey)) return;
		ctx.seenStableKeys.add(stableKey);

		ctx.nodes.push({
			nodeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey,
			kind: NodeKind.SYMBOL, subtype,
			name, qualifiedName: fullQualified,
			fileUid: ctx.fileUid, parentNodeUid: ctx.fileNodeUid,
			location: locationFromNode(node),
			signature: null,
			visibility: Visibility.EXPORT,
			docComment: extractPrecedingComment(node),
			metadataJson: null,
		});
	}

	// ── Namespace (C++) ──────────────────────────────────────────

	private processNamespace(
		node: SyntaxNode,
		ctx: ExtractionContext,
		outerNamespace: string | null,
	): void {
		const nameNode = node.childForFieldName("name");
		const nsName = nameNode?.text ?? null;
		const fullNs = outerNamespace && nsName
			? `${outerNamespace}::${nsName}`
			: nsName;

		const body = node.childForFieldName("body");
		if (body) {
			this.walkTopLevel(body, ctx, fullNs);
		}
	}

	// ── Class (C++) ──────────────────────────────────────────────

	private processClass(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const fullQualified = buildQualifiedName(namespace, null, name);

		ctx.nodes.push({
			nodeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${fullQualified}:SYMBOL:CLASS`,
			kind: NodeKind.SYMBOL, subtype: NodeSubtype.CLASS,
			name, qualifiedName: fullQualified,
			fileUid: ctx.fileUid, parentNodeUid: ctx.fileNodeUid,
			location: locationFromNode(node),
			signature: null,
			visibility: Visibility.EXPORT,
			docComment: extractPrecedingComment(node),
			metadataJson: null,
		});

		// Process base classes as IMPLEMENTS edges.
		const baseList = node.children.find((c) => c.type === "base_class_clause");
		if (baseList) {
			for (const child of baseList.children) {
				if (child.type === "type_identifier" || child.type === "qualified_identifier") {
					ctx.edges.push({
						edgeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid,
						sourceNodeUid: ctx.fileNodeUid,
						targetKey: child.text,
						type: EdgeType.IMPLEMENTS,
						resolution: Resolution.STATIC,
						extractor: EXTRACTOR_NAME,
						location: locationFromNode(child),
						metadataJson: null,
					});
				}
			}
		}

		// Process members declared inside the class body.
		const body = node.childForFieldName("body");
		if (body) {
			for (const member of body.children) {
				if (member.type === "function_definition") {
					// Inline method definition with body.
					this.processFunction(member, ctx, namespace, name);
				} else if (
					member.type === "field_declaration" ||
					member.type === "declaration"
				) {
					// Method declaration (no body) or constructor declaration.
					// Check if it contains a function_declarator.
					this.processClassMemberDeclaration(member, ctx, namespace, name);
				}
			}
		}
	}

	// ── Class member declarations (no body) ─────────────────────

	/**
	 * Process a class member declaration that may be a method/constructor
	 * declaration without a body (e.g., `void run();` or `Engine(int size);`).
	 */
	private processClassMemberDeclaration(
		node: SyntaxNode,
		ctx: ExtractionContext,
		namespace: string | null,
		className: string,
	): void {
		// Look for a function_declarator child — that indicates a method.
		const funcDecl = node.children.find((c) => c.type === "function_declarator");
		if (!funcDecl) return; // Data member, not a method.

		const { name } = extractDeclaratorName(funcDecl);
		if (!name) return;

		const isConstructor = name === className;
		const subtype = isConstructor ? NodeSubtype.CONSTRUCTOR : NodeSubtype.METHOD;
		const fullQualified = buildQualifiedName(namespace, className, name);
		const stableKey = `${ctx.repoUid}:${ctx.filePath}#${fullQualified}:SYMBOL:${subtype}`;

		if (ctx.seenStableKeys.has(stableKey)) return;
		ctx.seenStableKeys.add(stableKey);

		const returnType = node.children.find(
			(c) => c.type === "primitive_type" || c.type === "type_identifier",
		);
		const params = funcDecl.childForFieldName("parameters");
		let sig = fullQualified;
		if (params) sig = `${fullQualified}${params.text}`;
		if (returnType) sig = `${returnType.text} ${sig}`;

		ctx.nodes.push({
			nodeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey,
			kind: NodeKind.SYMBOL, subtype,
			name, qualifiedName: fullQualified,
			fileUid: ctx.fileUid, parentNodeUid: ctx.fileNodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility: Visibility.EXPORT,
			docComment: null, metadataJson: null,
		});
	}

	// ── Call extraction ──────────────────────────────────────────

	private extractCalls(
		node: SyntaxNode,
		sourceNodeUid: string,
		ctx: ExtractionContext,
	): void {
		if (node.type === "call_expression") {
			const fn = node.childForFieldName("function");
			if (fn) {
				const callee = fn.text;
				if (callee && callee.length < 200) {
					ctx.edges.push({
						edgeUid: uuidv4(), snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid, sourceNodeUid,
						targetKey: callee, type: EdgeType.CALLS,
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
 * Extract the function/variable name from a declarator node.
 * Handles:
 *   - simple: identifier "main"
 *   - function: function_declarator → identifier "main"
 *   - scoped: qualified_identifier "Engine::run" → name="run", prefix="Engine"
 *   - pointer: pointer_declarator → function_declarator → ...
 */
function extractDeclaratorName(
	node: SyntaxNode,
): { name: string | null; qualifiedPrefix: string | null } {
	if (node.type === "identifier" || node.type === "field_identifier") {
		return { name: node.text, qualifiedPrefix: null };
	}
	if (node.type === "function_declarator") {
		const inner = node.childForFieldName("declarator");
		if (inner) return extractDeclaratorName(inner);
	}
	if (node.type === "qualified_identifier") {
		const scope = node.childForFieldName("scope");
		const name = node.childForFieldName("name");
		return {
			name: name?.text ?? null,
			qualifiedPrefix: scope?.text?.replace(/::$/, "") ?? null,
		};
	}
	if (node.type === "pointer_declarator" || node.type === "reference_declarator") {
		const inner = node.childForFieldName("declarator");
		if (inner) return extractDeclaratorName(inner);
	}
	if (node.type === "destructor_name") {
		return { name: node.text, qualifiedPrefix: null };
	}
	return { name: null, qualifiedPrefix: null };
}

function buildQualifiedName(
	namespace: string | null,
	className: string | null,
	name: string,
): string {
	const parts: string[] = [];
	if (namespace) parts.push(namespace);
	if (className) parts.push(className);
	parts.push(name);
	return parts.join("::");
}

function extractPrecedingComment(node: SyntaxNode): string | null {
	let prev = node.previousSibling;
	while (prev) {
		if (prev.type === "comment") {
			if (prev.text.startsWith("/*") || prev.text.startsWith("//")) {
				return prev.text;
			}
		}
		if (prev.type !== "comment") break;
		prev = prev.previousSibling;
	}
	return null;
}

function computeCyclomaticComplexity(body: SyntaxNode): number {
	let cc = 1;
	const walk = (node: SyntaxNode) => {
		switch (node.type) {
			case "if_statement":
			case "for_statement":
			case "while_statement":
			case "do_statement":
			case "case_statement":
			case "conditional_expression":
				cc++;
				break;
			case "&&":
			case "||":
				cc++;
				break;
		}
		for (const child of node.children) walk(child);
	};
	walk(body);
	return cc;
}

function countParameters(params: SyntaxNode | null): number {
	if (!params) return 0;
	let count = 0;
	for (const child of params.children) {
		if (child.type === "parameter_declaration") count++;
	}
	return count;
}

function computeMaxNesting(node: SyntaxNode, depth: number): number {
	let maxDepth = depth;
	const nestingTypes = new Set([
		"if_statement", "for_statement", "while_statement",
		"do_statement", "switch_statement",
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
