/**
 * Java language extractor -- first slice.
 *
 * Parses .java files using web-tree-sitter with the tree-sitter-java
 * grammar. Extracts:
 *   - FILE nodes (one per file)
 *   - SYMBOL nodes for classes, interfaces, enums, methods, constructors,
 *     fields, and annotation type declarations
 *   - IMPORTS edges from `import` declarations
 *   - CALLS edges from method invocations and object creation expressions
 *   - IMPLEMENTS edges from `super_interfaces` clauses
 *   - ImportBinding records for import declarations
 *
 * Visibility: `public` -> EXPORT, `private` -> PRIVATE,
 * `protected` -> PROTECTED, package-private (no modifier) -> null.
 *
 * Does not compute complexity metrics in the first slice (returns
 * empty metrics map).
 */

import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
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

import { JAVA_RUNTIME_BUILTINS } from "./runtime-builtins.js";

const EXTRACTOR_NAME = "java-core:0.1.0";
const __dirname = dirname(fileURLToPath(import.meta.url));
const GRAMMARS_DIR = join(__dirname, "..", "..", "..", "..", "grammars");

// -- Extraction context ---------------------------------------------------

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
	/**
	 * Stable keys already emitted in this file. Used to deduplicate
	 * emissions when tree-sitter produces duplicate nodes (e.g.
	 * overloaded methods with the same name produce the same stable
	 * key under first-slice simplification). The first emission wins.
	 */
	emittedStableKeys: Set<string>;
}

// -- Extractor class ------------------------------------------------------

export class JavaExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["java"];
	readonly runtimeBuiltins: RuntimeBuiltinsSet = JAVA_RUNTIME_BUILTINS;

	private parser: Parser | null = null;
	private javaLanguage: Language | null = null;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();

		const javaWasmPath = join(GRAMMARS_DIR, "tree-sitter-java.wasm");
		// Verify the grammar file exists.
		await readFile(javaWasmPath);
		this.javaLanguage = await Language.load(javaWasmPath);
	}

	async extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult> {
		if (!this.parser || !this.javaLanguage) {
			throw new Error("Extractor not initialized. Call initialize() first.");
		}

		this.parser.setLanguage(this.javaLanguage);
		const tree = this.parser.parse(source);
		if (!tree) {
			throw new Error(`Failed to parse ${filePath}`);
		}

		try {

		const nodes: GraphNode[] = [];
		const edges: UnresolvedEdge[] = [];
		const metrics = new Map<string, ExtractedMetrics>();
		const importBindings: ImportBinding[] = [];

		const lineCount = source.split("\n").length;
		const fileNodeUid = uuidv4();
		nodes.push({
			nodeUid: fileNodeUid,
			snapshotUid,
			repoUid,
			stableKey: `${repoUid}:${filePath}:FILE`,
			kind: NodeKind.FILE,
			subtype: NodeSubtype.SOURCE,
			name: filePath.split("/").pop() ?? filePath,
			qualifiedName: filePath,
			fileUid,
			parentNodeUid: null,
			location: { lineStart: 1, colStart: 0, lineEnd: lineCount, colEnd: 0 },
			signature: null,
			visibility: null,
			docComment: null,
			metadataJson: null,
		});

		const ctx: ExtractionContext = {
			source,
			filePath,
			fileUid,
			fileNodeUid,
			repoUid,
			snapshotUid,
			nodes,
			edges,
			metrics,
			importBindings,
			emittedStableKeys: new Set<string>(),
		};

		for (const child of tree.rootNode.children) {
			this.visitTopLevel(child, ctx, null);
		}

		return { nodes, edges, metrics, importBindings };

		} finally {
			tree.delete();
		}
	}

	/**
	 * Emit a node, deduplicating by stable_key. Returns true if emitted,
	 * false if a node with the same key was already emitted.
	 */
	private emitNode(node: GraphNode, ctx: ExtractionContext): boolean {
		if (ctx.emittedStableKeys.has(node.stableKey)) return false;
		ctx.emittedStableKeys.add(node.stableKey);
		ctx.nodes.push(node);
		return true;
	}

	// -- Top-level visitor --------------------------------------------------

	private visitTopLevel(
		node: SyntaxNode,
		ctx: ExtractionContext,
		enclosingClassName: string | null,
	): void {
		switch (node.type) {
			case "import_declaration":
				this.extractImportDeclaration(node, ctx);
				break;
			case "class_declaration":
				this.extractClassLike(
					node,
					NodeSubtype.CLASS,
					ctx,
					enclosingClassName,
				);
				break;
			case "interface_declaration":
				this.extractClassLike(
					node,
					NodeSubtype.INTERFACE,
					ctx,
					enclosingClassName,
				);
				break;
			case "enum_declaration":
				this.extractClassLike(
					node,
					NodeSubtype.ENUM,
					ctx,
					enclosingClassName,
				);
				break;
			case "annotation_type_declaration":
				this.extractClassLike(
					node,
					NodeSubtype.INTERFACE,
					ctx,
					enclosingClassName,
				);
				break;
			case "package_declaration":
				// Package declarations are informational; no node emitted.
				break;
		}
	}

	// -- Import declaration extraction --------------------------------------

	/**
	 * Extract `import` declarations.
	 *
	 * Handles:
	 *   - Named imports:   `import java.util.HashMap;`
	 *   - Wildcard:        `import java.util.*;`
	 *   - Static named:    `import static org.junit.Assert.assertEquals;`
	 *   - Static wildcard: `import static org.junit.Assert.*;`
	 */
	private extractImportDeclaration(
		node: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		const location = locationFromNode(node);
		const text = node.text.trim();

		// Remove trailing semicolon.
		const stripped = text.replace(/;$/, "").trim();

		// Determine if static import.
		const isStatic = stripped.startsWith("import static ");
		const pathPart = isStatic
			? stripped.slice("import static ".length).trim()
			: stripped.slice("import ".length).trim();

		// Split into segments by "."
		const segments = pathPart.split(".");

		let identifier: string;
		let specifier: string;

		if (segments[segments.length - 1] === "*") {
			// Wildcard import: `java.util.*`
			identifier = "*";
			specifier = segments.slice(0, -1).join(".");
		} else {
			// Named import: last segment is the identifier.
			identifier = segments[segments.length - 1];
			specifier = segments.slice(0, -1).join(".");
		}

		// The full import path for the IMPORTS edge target key.
		const fullImportPath = pathPart;

		// All Java imports are absolute (no relative imports).
		ctx.importBindings.push({
			identifier,
			specifier,
			isRelative: false,
			location,
			isTypeOnly: false,
		});

		// IMPORTS edge. Target key is the full import path.
		ctx.edges.push({
			edgeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			sourceNodeUid: ctx.fileNodeUid,
			targetKey: fullImportPath,
			type: EdgeType.IMPORTS,
			resolution: Resolution.STATIC,
			extractor: EXTRACTOR_NAME,
			location,
			metadataJson: JSON.stringify({
				specifier,
				identifier,
				isStatic,
			}),
		});
	}

	// -- Class-like extraction (class, interface, enum, annotation type) ----

	/**
	 * Extract a class, interface, enum, or annotation type declaration.
	 *
	 * Also recurses into the class body to extract:
	 *   - Methods and constructors
	 *   - Fields
	 *   - Nested class-like declarations
	 *   - IMPLEMENTS edges from super_interfaces
	 */
	private extractClassLike(
		node: SyntaxNode,
		subtype: NodeSubtype,
		ctx: ExtractionContext,
		enclosingClassName: string | null,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const qualifiedName = enclosingClassName
			? `${enclosingClassName}.${name}`
			: name;

		const visibility = this.getVisibility(node);
		const docComment = this.extractDocComment(node);

		const classNode = this.makeSymbolNode(
			name,
			qualifiedName,
			subtype,
			visibility,
			null,
			node,
			ctx,
			docComment,
		);
		if (!this.emitNode(classNode, ctx)) return;

		// Extract IMPLEMENTS edges from super_interfaces.
		this.extractImplementsEdges(node, classNode, ctx);

		// Recurse into class body.
		const body = node.childForFieldName("body");
		if (!body) return;

		for (const member of body.children) {
			switch (member.type) {
				case "method_declaration":
					this.extractMethod(member, qualifiedName, classNode, ctx);
					break;
				case "constructor_declaration":
					this.extractConstructor(member, qualifiedName, classNode, ctx);
					break;
				case "field_declaration":
					this.extractField(member, qualifiedName, ctx);
					break;
				case "class_declaration":
					this.extractClassLike(
						member,
						NodeSubtype.CLASS,
						ctx,
						qualifiedName,
					);
					break;
				case "interface_declaration":
					this.extractClassLike(
						member,
						NodeSubtype.INTERFACE,
						ctx,
						qualifiedName,
					);
					break;
				case "enum_declaration":
					this.extractClassLike(
						member,
						NodeSubtype.ENUM,
						ctx,
						qualifiedName,
					);
					break;
				case "annotation_type_declaration":
					this.extractClassLike(
						member,
						NodeSubtype.INTERFACE,
						ctx,
						qualifiedName,
					);
					break;
			}
		}
	}

	// -- IMPLEMENTS edges ---------------------------------------------------

	/**
	 * Extract IMPLEMENTS edges from a class declaration's
	 * `super_interfaces` clause (e.g. `implements Serializable, Comparable`).
	 *
	 * Also handles `extends` for interfaces (interface extending another
	 * interface is semantically IMPLEMENTS in the graph).
	 */
	private extractImplementsEdges(
		node: SyntaxNode,
		classNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		for (const child of node.children) {
			if (child.type === "super_interfaces") {
				// super_interfaces contains a type_list.
				const typeList = child.childForFieldName("type") ?? child;
				this.extractTypeListAsImplements(typeList, classNode, node, ctx);
			}
			// Interface extending: `interface Foo extends Bar, Baz`
			if (
				child.type === "extends_interfaces" ||
				child.type === "extends_type_list"
			) {
				this.extractTypeListAsImplements(child, classNode, node, ctx);
			}
		}
	}

	/**
	 * Walk a type_list node and emit IMPLEMENTS edges for each type.
	 */
	private extractTypeListAsImplements(
		typeListNode: SyntaxNode,
		classNode: GraphNode,
		declNode: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		for (const child of typeListNode.children) {
			if (child.type === "type_identifier" || child.type === "generic_type") {
				const typeName =
					child.type === "generic_type"
						? (child.children[0]?.text ?? child.text)
						: child.text;

				ctx.edges.push({
					edgeUid: uuidv4(),
					snapshotUid: ctx.snapshotUid,
					repoUid: ctx.repoUid,
					sourceNodeUid: classNode.nodeUid,
					targetKey: typeName,
					type: EdgeType.IMPLEMENTS,
					resolution: Resolution.STATIC,
					extractor: EXTRACTOR_NAME,
					location: locationFromNode(declNode),
					metadataJson: JSON.stringify({
						interfaceName: typeName,
					}),
				});
			}
			// Recurse into type_list nodes.
			if (child.type === "type_list") {
				this.extractTypeListAsImplements(child, classNode, declNode, ctx);
			}
		}
	}

	// -- Method extraction --------------------------------------------------

	private extractMethod(
		node: SyntaxNode,
		enclosingClassName: string,
		parentClassNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		// Include parameter count in qualified name to disambiguate
		// Java overloads. Without this, Foo.bar(String) and Foo.bar(int)
		// would collide on the same stable_key.
		const params = node.childForFieldName("parameters");
		const paramSig = params ? parameterSignature(params) : "";
		const qualifiedName = `${enclosingClassName}.${name}(${paramSig})`;

		const visibility = this.getVisibility(node);
		const returnType = this.getReturnType(node);
		const sig = params
			? `${returnType ? `${returnType} ` : ""}${name}${params.text}`
			: `${returnType ? `${returnType} ` : ""}${name}()`;

		const methodNode: GraphNode = {
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${NodeSubtype.METHOD}`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.METHOD,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentClassNode.nodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility,
			docComment: this.extractDocComment(node),
			metadataJson: null,
		};
		if (!this.emitNode(methodNode, ctx)) return;

		// Extract calls from method body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, methodNode.nodeUid);
		}
	}

	// -- Constructor extraction ---------------------------------------------

	private extractConstructor(
		node: SyntaxNode,
		enclosingClassName: string,
		parentClassNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const params = node.childForFieldName("parameters");
		const paramSig = params ? parameterSignature(params) : "";
		const qualifiedName = `${enclosingClassName}.${name}(${paramSig})`;

		const visibility = this.getVisibility(node);
		const sig = params ? `${name}${params.text}` : `${name}()`;

		const ctorNode: GraphNode = {
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${NodeSubtype.CONSTRUCTOR}`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.CONSTRUCTOR,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentClassNode.nodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility,
			docComment: this.extractDocComment(node),
			metadataJson: null,
		};
		if (!this.emitNode(ctorNode, ctx)) return;

		// Extract calls from constructor body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, ctorNode.nodeUid);
		}
	}

	// -- Field extraction ---------------------------------------------------

	private extractField(
		node: SyntaxNode,
		enclosingClassName: string,
		ctx: ExtractionContext,
	): void {
		// A field_declaration may declare multiple variables:
		// `private int x, y;`
		// We extract each declarator as a separate PROPERTY node.
		const visibility = this.getVisibility(node);

		for (const child of node.children) {
			if (child.type === "variable_declarator") {
				const nameNode = child.childForFieldName("name");
				if (!nameNode) continue;
				const name = nameNode.text;
				const qualifiedName = `${enclosingClassName}.${name}`;

				this.emitNode(
					this.makeSymbolNode(
						name,
						qualifiedName,
						NodeSubtype.PROPERTY,
						visibility,
						null,
						child,
						ctx,
						null,
					),
					ctx,
				);
			}
		}
	}

	// -- Call extraction ----------------------------------------------------

	/**
	 * Recursively walk a node tree extracting CALLS edges from
	 * method_invocation and object_creation_expression nodes.
	 *
	 * First-slice simplifications:
	 *   - No scope/binding tracking.
	 *   - No receiver type resolution.
	 *   - Raw callee text for the target key.
	 */
	private extractCallsFromNode(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		if (node.type === "method_invocation") {
			this.emitMethodCallEdge(node, ctx, callerNodeUid);
		} else if (node.type === "object_creation_expression") {
			this.emitNewCallEdge(node, ctx, callerNodeUid);
		}

		// Recurse into children. Skip nested class declarations (they are
		// their own scope and will be extracted separately).
		for (const child of node.children) {
			if (child.type === "class_declaration") continue;
			if (child.type === "lambda_expression") {
				// Walk lambda bodies for calls attributed to the
				// enclosing method.
				this.extractCallsFromNode(child, ctx, callerNodeUid);
				continue;
			}
			this.extractCallsFromNode(child, ctx, callerNodeUid);
		}
	}

	/**
	 * Emit a CALLS edge for a method_invocation node.
	 *
	 * Java method invocations:
	 *   - Simple:    `foo(args)` -> targetKey = "foo"
	 *   - Qualified: `obj.method(args)` -> targetKey = "obj.method"
	 *   - Chained:   `a.b().c()` -> we extract the outermost call
	 */
	private emitMethodCallEdge(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const methodName = nameNode.text;

		const objectNode = node.childForFieldName("object");
		let targetKey: string;

		if (objectNode) {
			// Qualified call: `obj.method(...)` or `ClassName.method(...)`
			// Use the object text + method name for the target key.
			// For complex expressions (method chains), use just the text.
			targetKey = `${objectNode.text}.${methodName}`;
		} else {
			// Unqualified call: `method(...)`
			targetKey = methodName;
		}

		ctx.edges.push({
			edgeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			sourceNodeUid: callerNodeUid,
			targetKey,
			type: EdgeType.CALLS,
			resolution: Resolution.STATIC,
			extractor: EXTRACTOR_NAME,
			location: locationFromNode(node),
			metadataJson: null,
		});
	}

	/**
	 * Emit a CALLS edge for an object_creation_expression (`new Foo(...)`).
	 *
	 * Target key is the type name being instantiated.
	 */
	private emitNewCallEdge(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		const typeNode = node.childForFieldName("type");
		if (!typeNode) return;

		// For generic types like `new ArrayList<String>()`, extract just
		// the base type name.
		let typeName: string;
		if (typeNode.type === "generic_type") {
			typeName = typeNode.children[0]?.text ?? typeNode.text;
		} else {
			typeName = typeNode.text;
		}

		ctx.edges.push({
			edgeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			sourceNodeUid: callerNodeUid,
			targetKey: typeName,
			type: EdgeType.CALLS,
			resolution: Resolution.STATIC,
			extractor: EXTRACTOR_NAME,
			location: locationFromNode(node),
			metadataJson: JSON.stringify({ isConstructorCall: true }),
		});
	}

	// -- Helpers ------------------------------------------------------------

	private makeSymbolNode(
		name: string,
		qualifiedName: string,
		subtype: NodeSubtype,
		visibility: Visibility | null,
		signature: string | null,
		node: SyntaxNode,
		ctx: ExtractionContext,
		docComment: string | null,
	): GraphNode {
		return {
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${subtype}`,
			kind: NodeKind.SYMBOL,
			subtype,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: null,
			location: locationFromNode(node),
			signature,
			visibility,
			docComment,
			metadataJson: null,
		};
	}

	/**
	 * Determine the visibility of a declaration from its modifiers.
	 *
	 * Java visibility:
	 *   - `public`    -> Visibility.EXPORT
	 *   - `private`   -> Visibility.PRIVATE
	 *   - `protected` -> Visibility.PROTECTED
	 *   - (none)      -> null (package-private)
	 */
	private getVisibility(node: SyntaxNode): Visibility | null {
		for (const child of node.children) {
			if (child.type === "modifiers") {
				for (const mod of child.children) {
					if (mod.type === "public") return Visibility.EXPORT;
					if (mod.type === "private") return Visibility.PRIVATE;
					if (mod.type === "protected") return Visibility.PROTECTED;
					// tree-sitter-java also uses text matching for modifiers
					if (mod.text === "public") return Visibility.EXPORT;
					if (mod.text === "private") return Visibility.PRIVATE;
					if (mod.text === "protected") return Visibility.PROTECTED;
				}
			}
		}
		return null;
	}

	/**
	 * Extract the return type text from a method declaration.
	 * Returns null if not found.
	 */
	private getReturnType(node: SyntaxNode): string | null {
		const typeNode = node.childForFieldName("type");
		if (typeNode) return typeNode.text;
		return null;
	}

	/**
	 * Extract a Javadoc or block comment preceding a node.
	 *
	 * Java doc comments start with `/**`. We collect the block
	 * comment or consecutive line comments immediately preceding
	 * the declaration.
	 */
	private extractDocComment(node: SyntaxNode): string | null {
		const comments: string[] = [];
		let prev = node.previousSibling;

		while (prev) {
			if (prev.type === "block_comment" && prev.text.startsWith("/**")) {
				comments.unshift(prev.text);
				break;
			}
			if (prev.type === "line_comment") {
				comments.unshift(prev.text);
				prev = prev.previousSibling;
			} else if (prev.type === "marker_annotation" || prev.type === "annotation") {
				// Skip annotations between doc comments and the declaration.
				prev = prev.previousSibling;
			} else {
				break;
			}
		}

		if (comments.length === 0) return null;
		return comments.join("\n");
	}
}

// -- Utility ----------------------------------------------------------------

/**
 * Extract a parameter type signature from a `formal_parameters` node.
 * Returns a string like "String,int" for `(String key, int value)`.
 * Disambiguates Java overloads including same-arity cases:
 *   foo(String) and foo(Integer) → "foo(String)" vs "foo(Integer)".
 * Strips generic parameters for compactness.
 */
function parameterSignature(paramsNode: SyntaxNode): string {
	const types: string[] = [];
	for (const child of paramsNode.children) {
		if (
			child.type === "formal_parameter" ||
			child.type === "spread_parameter"
		) {
			const typeNode = child.childForFieldName("type");
			if (typeNode) {
				let typeName = typeNode.text;
				const angleIdx = typeName.indexOf("<");
				if (angleIdx > 0) typeName = typeName.slice(0, angleIdx);
				types.push(typeName);
			} else {
				types.push("?");
			}
		}
	}
	return types.join(",");
}

function locationFromNode(node: SyntaxNode): SourceLocation {
	return {
		lineStart: node.startPosition.row + 1, // tree-sitter is 0-based
		colStart: node.startPosition.column,
		lineEnd: node.endPosition.row + 1,
		colEnd: node.endPosition.column,
	};
}
