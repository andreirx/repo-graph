import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { v4 as uuidv4 } from "uuid";
import { Language, Parser, type Node as SyntaxNode } from "web-tree-sitter";
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

import { EXTRACTOR_VERSIONS } from "../../../version.js";
import { TS_JS_RUNTIME_BUILTINS } from "./runtime-builtins.js";

import { computeFunctionMetrics } from "./ast-metrics.js";

const EXTRACTOR_NAME = EXTRACTOR_VERSIONS.typescript;
const __dirname = dirname(fileURLToPath(import.meta.url));
const GRAMMARS_DIR = join(__dirname, "..", "..", "..", "..", "grammars");

export class TypeScriptExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["typescript", "tsx"];
	readonly runtimeBuiltins = TS_JS_RUNTIME_BUILTINS;

	private parser: Parser | null = null;
	private tsLanguage: Language | null = null;
	private tsxLanguage: Language | null = null;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();

		// Load grammar WASM files
		const tsWasmPath = join(GRAMMARS_DIR, "tree-sitter-typescript.wasm");
		const tsxWasmPath = join(GRAMMARS_DIR, "tree-sitter-tsx.wasm");

		// Verify files exist by trying to read them
		await readFile(tsWasmPath);
		await readFile(tsxWasmPath);

		this.tsLanguage = await Language.load(tsWasmPath);
		this.tsxLanguage = await Language.load(tsxWasmPath);
	}

	async extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult> {
		if (!this.parser || !this.tsLanguage || !this.tsxLanguage) {
			throw new Error("Extractor not initialized. Call initialize() first.");
		}

		// JSX-bearing extensions use the tsx grammar.
		// .jsx was previously parsed with the TypeScript grammar, which
		// fails on JSX syntax and can produce spurious top-level
		// declarations (e.g. inner function-scope consts misattributed
		// to module scope, causing stable_key collisions).
		const isTsx = filePath.endsWith(".tsx") || filePath.endsWith(".jsx");
		this.parser.setLanguage(isTsx ? this.tsxLanguage : this.tsLanguage);

		const tree = this.parser.parse(source);
		if (!tree) {
			throw new Error(`Failed to parse ${filePath}`);
		}
		const nodes: GraphNode[] = [];
		const edges: UnresolvedEdge[] = [];
		const metrics = new Map<string, ExtractedMetrics>();
		const importBindings: ImportBinding[] = [];

		// Emit the FILE node. This is the source of all IMPORTS edges
		// and the parent of top-level symbols in this file.
		const lineCount = source.split("\n").length;
		const fileNodeUid = uuidv4();
		nodes.push({
			nodeUid: fileNodeUid,
			snapshotUid,
			repoUid,
			stableKey: `${repoUid}:${filePath}:FILE`,
			kind: NodeKind.FILE,
			subtype: filePath.endsWith(".tsx")
				? NodeSubtype.SOURCE
				: NodeSubtype.SOURCE,
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
			exportedNames: new Set<string>(),
			metrics,
			importBindings,
			fileScopeBindings: new Map<string, string>(),
			classBindings: null,
			enclosingClassName: null,
			memberTypes: new Map<string, Map<string, string>>(),
		};

		// First pass: walk all top-level declarations to extract symbols
		for (const child of tree.rootNode.children) {
			this.visitTopLevel(child, ctx);
		}

		// Second pass: apply export visibility from `export { x, y }` lists
		// that were collected during visitTopLevel.
		if (ctx.exportedNames.size > 0) {
			for (const node of ctx.nodes) {
				if (
					node.kind === NodeKind.SYMBOL &&
					node.visibility !== Visibility.EXPORT &&
					ctx.exportedNames.has(node.name)
				) {
					node.visibility = Visibility.EXPORT;
				}
			}
		}

		tree.delete();
		return { nodes, edges, metrics, importBindings };
	}

	// ── Top-level visitor ────────────────────────────────────────────────

	private visitTopLevel(node: SyntaxNode, ctx: ExtractionContext): void {
		switch (node.type) {
			case "import_statement":
				this.extractImport(node, ctx);
				break;
			case "export_statement":
				this.extractExportStatement(node, ctx);
				break;
			case "function_declaration":
				this.extractFunction(node, ctx, this.isExported(node));
				break;
			case "class_declaration":
				this.extractClass(node, ctx, this.isExported(node));
				break;
			case "interface_declaration":
				this.extractInterface(node, ctx, this.isExported(node));
				break;
			case "type_alias_declaration":
				this.extractTypeAlias(node, ctx, this.isExported(node));
				break;
			case "enum_declaration":
				this.extractEnum(node, ctx, this.isExported(node));
				break;
			case "lexical_declaration":
				this.extractLexicalDeclaration(node, ctx, this.isExported(node));
				break;
			case "variable_declaration":
				this.extractLexicalDeclaration(node, ctx, this.isExported(node));
				break;
			case "expression_statement":
				this.extractCallsFromExpression(node, ctx);
				break;
		}
	}

	// ── Import extraction ────────────────────────────────────────────────

	private extractImport(node: SyntaxNode, ctx: ExtractionContext): void {
		const sourceNode = node.childForFieldName("source");
		if (!sourceNode) return;

		// Get the import path string value (strip quotes)
		const rawPath = sourceNode.text.replace(/^['"]|['"]$/g, "");

		const isRelative = rawPath.startsWith(".");
		const location = locationFromNode(node);

		// Statement-level `import type`. First-slice: detect via leading
		// tokens. Specifier-level `{ type X }` is not distinguished here.
		const isTypeOnly = /^import\s+type\s/.test(node.text);

		// Emit ImportBinding records for every identifier-bearing
		// binding in the import clause. Side-effect imports (no
		// identifier) are skipped by design.
		for (const child of node.children) {
			if (child.type !== "import_clause") continue;
			for (const localIdentifier of collectLocalIdentifiers(child)) {
				ctx.importBindings.push({
					identifier: localIdentifier,
					specifier: rawPath,
					isRelative,
					location,
					isTypeOnly,
				});
			}
		}

		// Graph edge emission: unchanged from prior behavior.
		// Only relative imports produce unresolved IMPORTS edges. This
		// preserves the existing trust posture: non-relative imports
		// remain side-channel facts only.
		if (!isRelative) return;

		// Resolve the import to a repo-relative path
		const resolvedPath = this.resolveImportPath(rawPath, ctx.filePath);
		if (!resolvedPath) return;

		// Target is the stable key of the imported file's FILE node.
		// The indexer will resolve this to the actual node_uid after
		// all files are extracted.
		const targetStableKey = `${ctx.repoUid}:${resolvedPath}:FILE`;

		ctx.edges.push({
			edgeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			sourceNodeUid: ctx.fileNodeUid, // this file's FILE node uid
			targetKey: targetStableKey, // resolved by indexer
			type: EdgeType.IMPORTS,
			resolution: Resolution.STATIC,
			extractor: EXTRACTOR_NAME,
			location,
			metadataJson: JSON.stringify({ rawPath, resolvedPath }),
		});
	}

	// ── Export statement ──────────────────────────────────────────────────

	private extractExportStatement(
		node: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		// `export default ...` or `export { ... }` or `export <declaration>`
		const declaration = node.childForFieldName("declaration");
		if (declaration) {
			switch (declaration.type) {
				case "function_declaration":
					this.extractFunction(declaration, ctx, true);
					break;
				case "class_declaration":
					this.extractClass(declaration, ctx, true);
					break;
				case "interface_declaration":
					this.extractInterface(declaration, ctx, true);
					break;
				case "type_alias_declaration":
					this.extractTypeAlias(declaration, ctx, true);
					break;
				case "enum_declaration":
					this.extractEnum(declaration, ctx, true);
					break;
				case "lexical_declaration":
					this.extractLexicalDeclaration(declaration, ctx, true);
					break;
			}
			return;
		}

		// Re-export: `export { X } from './Y'`
		const sourceNode = node.childForFieldName("source");
		if (sourceNode) {
			this.extractImport(node, ctx);
			return;
		}

		// Plain export list: `export { x, y }`
		// Collect the names so we can update visibility in a second pass
		// after all declarations have been extracted.
		for (const child of node.children) {
			if (child.type === "export_clause") {
				for (const specifier of child.children) {
					if (specifier.type === "export_specifier") {
						const nameNode = specifier.childForFieldName("name");
						if (nameNode) {
							ctx.exportedNames.add(nameNode.text);
						}
					}
				}
			}
		}
	}

	// ── Function extraction ──────────────────────────────────────────────

	private extractFunction(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const params = node.childForFieldName("parameters");
		const returnType = node.childForFieldName("return_type");
		const sig = params
			? `${name}${params.text}${returnType ? `: ${returnType.text}` : ""}`
			: name;

		const graphNode = this.makeSymbolNode(
			name,
			NodeSubtype.FUNCTION,
			exported ? Visibility.EXPORT : Visibility.PRIVATE,
			sig,
			node,
			ctx,
		);
		ctx.nodes.push(graphNode);

		// Extract calls within the function body.
		// Parameter bindings are the initial local scope.
		// var names are hoisted into a SEPARATE function-level map as
		// shadow-only entries; their typed bindings are installed at
		// statement position by walkBlockSequentially.
		// Keeping them separate prevents var shadows from contaminating
		// block-local binding copies (which would make them indistinguishable
		// from const/let TDZ shadows).
		const body = node.childForFieldName("body");
		if (body) {
			// Compute AST-derived metrics
			ctx.metrics.set(
				graphNode.stableKey,
				computeFunctionMetrics(body, params),
			);

			const paramBindings = this.collectParameterBindings(params);
			const varBindings = new Map<string, string | null>();
			this.collectVarBindings(body, varBindings);
			this.extractCallsFromNode(
				body,
				ctx,
				graphNode.nodeUid,
				paramBindings,
				varBindings,
			);
		}
	}

	// ── Class extraction ─────────────────────────────────────────────────

	private extractClass(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const classNode = this.makeSymbolNode(
			name,
			NodeSubtype.CLASS,
			exported ? Visibility.EXPORT : Visibility.PRIVATE,
			null,
			node,
			ctx,
		);
		ctx.nodes.push(classNode);

		// Extract IMPLEMENTS edges
		const heritage = this.findChildByType(node, "class_heritage");
		if (heritage) {
			this.extractImplements(heritage, classNode.nodeUid, ctx);
		}

		const body = node.childForFieldName("body");
		if (!body) return;

		// Collect member type annotations for 3-part chain resolution.
		this.collectMemberTypes(name, body, ctx);

		// Set class context before walking methods.
		// classBindings: resolve this.property.method() chains.
		// enclosingClassName: resolve this.method() to ClassName.method.
		ctx.classBindings = this.buildClassBindings(body);
		ctx.enclosingClassName = name;

		for (const member of body.children) {
			if (member.type === "method_definition") {
				this.extractMethod(member, classNode, ctx);
			} else if (member.type === "public_field_definition") {
				this.extractProperty(member, classNode, ctx);
			}
		}

		// Clear class context
		ctx.classBindings = null;
		ctx.enclosingClassName = null;
	}

	private extractMethod(
		node: SyntaxNode,
		parentClassNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const qualifiedName = `${parentClassNode.name}.${name}`;

		let subtype: NodeSubtype = NodeSubtype.METHOD;
		if (name === "constructor") subtype = NodeSubtype.CONSTRUCTOR;

		// Check for getter/setter
		const getterSetter = node.children.find(
			(c: SyntaxNode) => c.type === "get" || c.type === "set",
		);
		if (getterSetter?.type === "get") subtype = NodeSubtype.GETTER;
		if (getterSetter?.type === "set") subtype = NodeSubtype.SETTER;

		const visibility = this.getMethodVisibility(node);

		const params = node.childForFieldName("parameters");
		const sig = params ? `${name}${params.text}` : name;

		const methodNode: GraphNode = {
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${subtype}`,
			kind: NodeKind.SYMBOL,
			subtype,
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
		ctx.nodes.push(methodNode);

		// Extract calls within the method body.
		const body = node.childForFieldName("body");
		if (body) {
			// Compute AST-derived metrics
			ctx.metrics.set(
				methodNode.stableKey,
				computeFunctionMetrics(body, params),
			);

			const paramBindings = this.collectParameterBindings(params);
			const varBindings = new Map<string, string | null>();
			this.collectVarBindings(body, varBindings);
			this.extractCallsFromNode(
				body,
				ctx,
				methodNode.nodeUid,
				paramBindings,
				varBindings,
			);
		}
	}

	private extractProperty(
		node: SyntaxNode,
		parentClassNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const qualifiedName = `${parentClassNode.name}.${name}`;

		ctx.nodes.push({
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${NodeSubtype.PROPERTY}`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.PROPERTY,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentClassNode.nodeUid,
			location: locationFromNode(node),
			signature: null,
			visibility: this.getMethodVisibility(node),
			docComment: null,
			metadataJson: null,
		});
	}

	/**
	 * Build a type binding map for class-scope receivers.
	 * Maps property name → type name from:
	 *   1. Field declarations with type annotations: `private storage: StoragePort`
	 *   2. Constructor parameter properties: `constructor(private extractor: ExtractorPort)`
	 *
	 * Only captures simple type identifiers (not union types, generics, etc.).
	 * This is syntax-only: the type name is taken from the annotation, not resolved.
	 */
	private buildClassBindings(classBody: SyntaxNode): Map<string, string> {
		const bindings = new Map<string, string>();

		for (const member of classBody.children) {
			// 1. Field declarations: `private storage: StoragePort;`
			if (member.type === "public_field_definition") {
				const name = member.childForFieldName("name");
				const typeName = this.extractSimpleTypeName(member);
				if (name && typeName) {
					bindings.set(name.text, typeName);
				}
			}

			// 2. Constructor parameter properties:
			//    `constructor(private storage: StoragePort)`
			if (member.type === "method_definition") {
				const methodName = member.childForFieldName("name");
				if (methodName?.text !== "constructor") continue;

				const params = member.childForFieldName("parameters");
				if (!params) continue;

				for (const param of params.children) {
					if (param.type !== "required_parameter") continue;

					// Only parameter properties (those with accessibility modifiers
					// like private/protected/public/readonly) become this.X bindings
					const hasAccessibility = param.children.some(
						(c: SyntaxNode) =>
							c.type === "accessibility_modifier" || c.type === "readonly",
					);
					if (!hasAccessibility) continue;

					const paramName = param.childForFieldName("pattern");
					const typeName = this.extractSimpleTypeName(param);
					if (paramName && typeName) {
						bindings.set(paramName.text, typeName);
					}
				}
			}
		}

		return bindings;
	}

	/**
	 * Extract a simple type name from a node's type annotation.
	 * Returns the text of a `type_identifier` if the annotation is a plain
	 * type reference (e.g. `StoragePort`). Returns null for complex types
	 * (unions, intersections, generics, predefined types like `string`).
	 */
	private extractSimpleTypeName(node: SyntaxNode): string | null {
		const typeAnn = node.childForFieldName("type");
		if (!typeAnn) return null;

		// type_annotation contains ": TypeName"
		// Look for a direct type_identifier child
		for (const child of typeAnn.children) {
			if (child.type === "type_identifier") {
				return child.text;
			}
		}
		return null;
	}

	/**
	 * Collect property type annotations from an interface body or class body
	 * into the memberTypes map on the extraction context.
	 *
	 * For interfaces: scans property_signature nodes.
	 * For classes: scans public_field_definition nodes.
	 *
	 * Only records simple type identifiers (not unions, generics, etc.).
	 * Used for 3-part chain resolution: given `ctx: AppContext` and
	 * `interface AppContext { storage: StoragePort }`, resolves
	 * `ctx.storage.method()` to `StoragePort.method`.
	 */
	private collectMemberTypes(
		typeName: string,
		body: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		const members = new Map<string, string>();

		for (const member of body.children) {
			// Interface properties: property_signature
			// Class fields: public_field_definition
			if (
				member.type !== "property_signature" &&
				member.type !== "public_field_definition"
			) {
				continue;
			}

			const nameNode = member.childForFieldName("name");
			const memberType = this.extractSimpleTypeName(member);
			if (nameNode && memberType) {
				members.set(nameNode.text, memberType);
			}
		}

		if (members.size > 0) {
			ctx.memberTypes.set(typeName, members);
		}
	}

	private extractImplements(
		heritage: SyntaxNode,
		classNodeUid: string,
		ctx: ExtractionContext,
	): void {
		// class_heritage can contain `implements X, Y` and `extends Z`
		for (const child of heritage.children) {
			if (child.type === "implements_clause") {
				for (const typeNode of child.children) {
					if (
						typeNode.type === "type_identifier" ||
						typeNode.type === "identifier"
					) {
						ctx.edges.push({
							edgeUid: uuidv4(),
							snapshotUid: ctx.snapshotUid,
							repoUid: ctx.repoUid,
							sourceNodeUid: classNodeUid,
							targetKey: typeNode.text, // resolved by indexer
							type: EdgeType.IMPLEMENTS,
							resolution: Resolution.STATIC,
							extractor: EXTRACTOR_NAME,
							location: locationFromNode(typeNode),
							metadataJson: JSON.stringify({ targetName: typeNode.text }),
						});
					}
				}
			}
		}
	}

	// ── Interface extraction ─────────────────────────────────────────────

	private extractInterface(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const interfaceNode = this.makeSymbolNode(
			nameNode.text,
			NodeSubtype.INTERFACE,
			exported ? Visibility.EXPORT : Visibility.PRIVATE,
			null,
			node,
			ctx,
		);
		ctx.nodes.push(interfaceNode);

		// Extract interface members: method signatures and property signatures.
		// Skipped: index_signature, call_signature — structural typing features,
		// not named members that appear in call graphs.
		//
		// Overloaded methods (multiple signatures with the same name) are
		// merged into one node. From a call graph perspective, overloads are
		// one method — callers write `api.foo(x)`, not a specific overload.
		// The first signature is kept; subsequent overloads are skipped.
		const body = this.findChildByType(node, "interface_body");
		if (!body) return;

		// Collect member type annotations for 3-part chain resolution.
		// e.g. interface AppContext { storage: StoragePort } →
		// memberTypes["AppContext"]["storage"] = "StoragePort"
		this.collectMemberTypes(nameNode.text, body, ctx);

		const seenMembers = new Set<string>();
		for (const member of body.children) {
			if (member.type === "method_signature") {
				const nameNode = member.childForFieldName("name");
				if (!nameNode) continue;
				const key = `${nameNode.text}:method`;
				if (seenMembers.has(key)) continue; // skip overload
				seenMembers.add(key);
				this.extractInterfaceMethod(member, interfaceNode, ctx);
			} else if (member.type === "property_signature") {
				this.extractInterfaceProperty(member, interfaceNode, ctx);
			}
		}
	}

	private extractInterfaceMethod(
		node: SyntaxNode,
		parentInterfaceNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const qualifiedName = `${parentInterfaceNode.name}.${name}`;

		let subtype: NodeSubtype = NodeSubtype.METHOD;

		// Check for getter/setter (interfaces can declare get/set signatures)
		const getterSetter = node.children.find(
			(c: SyntaxNode) => c.type === "get" || c.type === "set",
		);
		if (getterSetter?.type === "get") subtype = NodeSubtype.GETTER;
		if (getterSetter?.type === "set") subtype = NodeSubtype.SETTER;

		const params = node.childForFieldName("parameters");
		const sig = params ? `${name}${params.text}` : name;

		ctx.nodes.push({
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${subtype}`,
			kind: NodeKind.SYMBOL,
			subtype,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentInterfaceNode.nodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility: Visibility.PUBLIC,
			docComment: this.extractDocComment(node),
			metadataJson: null,
		});
		// No call extraction — interface methods have no bodies.
	}

	private extractInterfaceProperty(
		node: SyntaxNode,
		parentInterfaceNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const qualifiedName = `${parentInterfaceNode.name}.${name}`;

		ctx.nodes.push({
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${NodeSubtype.PROPERTY}`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.PROPERTY,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentInterfaceNode.nodeUid,
			location: locationFromNode(node),
			signature: null,
			visibility: Visibility.PUBLIC,
			docComment: null,
			metadataJson: null,
		});
	}

	// ── Type alias extraction ────────────────────────────────────────────

	private extractTypeAlias(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		ctx.nodes.push(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.TYPE_ALIAS,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
		);
	}

	// ── Enum extraction ──────────────────────────────────────────────────

	private extractEnum(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		ctx.nodes.push(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.ENUM,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
		);
	}

	// ── Lexical declaration (const/let/var) ──────────────────────────────

	private extractLexicalDeclaration(
		node: SyntaxNode,
		ctx: ExtractionContext,
		exported: boolean,
	): void {
		// A lexical_declaration can have multiple declarators: const a = 1, b = 2
		for (const child of node.children) {
			if (child.type === "variable_declarator") {
				const nameNode = child.childForFieldName("name");
				if (!nameNode) continue;

				// Determine if it's a const (CONSTANT) or let/var (VARIABLE)
				const isConst = node.children.some(
					(c: SyntaxNode) => c.type === "const",
				);
				const name = nameNode.text;

				// Check if the value is an arrow function or function expression
				const value = child.childForFieldName("value");
				const isFunctionLike =
					value?.type === "arrow_function" ||
					value?.type === "function_expression" ||
					value?.type === "function";

				const subtype = isFunctionLike
					? NodeSubtype.FUNCTION
					: isConst
						? NodeSubtype.CONSTANT
						: NodeSubtype.VARIABLE;

				const graphNode = this.makeSymbolNode(
					name,
					subtype,
					exported ? Visibility.EXPORT : Visibility.PRIVATE,
					null,
					child,
					ctx,
				);
				ctx.nodes.push(graphNode);

				// Build file-scope type binding for this variable.
				// Priority: explicit type annotation > new ClassName() initializer.
				if (!isFunctionLike) {
					const typeName = this.extractSimpleTypeName(child);
					if (typeName) {
						ctx.fileScopeBindings.set(name, typeName);
					} else if (value?.type === "new_expression") {
						const ctor = value.childForFieldName("constructor");
						if (ctor) {
							ctx.fileScopeBindings.set(name, ctor.text);
						}
					}
				}

				// Extract calls from the initializer
				if (value) {
					this.extractCallsFromNode(value, ctx, graphNode.nodeUid);
				}
			}
		}
	}

	// ── Call extraction ──────────────────────────────────────────────────

	private extractCallsFromExpression(
		node: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		this.extractCallsFromNode(node, ctx, ctx.fileNodeUid);
	}

	/**
	 * @param localBindings - Current scope's read-only binding map.
	 * @param fnBindings - Mutable function-level binding map. var
	 *   declarations at any nesting depth write their typed binding
	 *   here when reached sequentially. Shared across all blocks in
	 *   the same function.
	 */
	private extractCallsFromNode(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
		localBindings?: ReadonlyMap<string, string | null>,
		fnBindings?: Map<string, string | null>,
	): void {
		// Scope boundary: statement_block → sequential walk with TDZ.
		if (node.type === "statement_block") {
			this.walkBlockSequentially(
				node,
				ctx,
				callerNodeUid,
				localBindings,
				fnBindings,
			);
			return;
		}

		// Scope boundary: for/for-in/for-of loops introduce a scope that
		// covers the initializer, condition, update, and body.
		if (node.type === "for_statement" || node.type === "for_in_statement") {
			this.walkLoopWithHeaderScope(
				node,
				ctx,
				callerNodeUid,
				localBindings,
				fnBindings,
			);
			return;
		}

		this.extractCallsFromSingleNode(
			node,
			ctx,
			callerNodeUid,
			localBindings,
			fnBindings,
		);

		// Recurse into children (for non-block nodes)
		for (const child of node.children) {
			if (this.isNewScopeNode(child)) continue;
			this.extractCallsFromNode(
				child,
				ctx,
				callerNodeUid,
				localBindings,
				fnBindings,
			);
		}
	}

	/**
	 * Handle for/for-in/for-of loops by extracting header bindings
	 * and passing them into all loop parts (condition, update, body).
	 *
	 * Loop initializer bindings:
	 *   for (const x = new Foo(); ...) → x binds to Foo (loop scope)
	 *   for (const x of items)         → x shadows outer (no type, null)
	 *   for (var x = new Foo(); ...)   → x typed binding at statement position,
	 *                                    also written to fnBindings (function-scoped)
	 */
	private walkLoopWithHeaderScope(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
		parentBindings?: ReadonlyMap<string, string | null>,
		fnBindings?: Map<string, string | null>,
	): void {
		const loopBindings = new Map<string, string | null>(parentBindings ?? []);

		if (node.type === "for_statement") {
			// for_statement: initializer is a lexical_declaration or variable_declaration.
			//
			// Ordering for correctness:
			//   1. Install shadow-only entries (TDZ for const/let, hoisted for var)
			//      so the initializer can't see the outer binding.
			//   2. Extract calls from the initializer with the shadow in place.
			//   3. Upgrade the shadow to a typed binding for condition/update/body.
			const initializer = node.childForFieldName("initializer");
			if (initializer) {
				// Step 1: shadow the names before extracting initializer calls
				this.prescanDeclarationNames(initializer, loopBindings);
				// Step 2: extract calls from the initializer
				this.extractCallsFromNode(
					initializer,
					ctx,
					callerNodeUid,
					loopBindings,
					fnBindings,
				);
				// Step 3: install typed bindings for use in condition/update/body
				if (initializer.type === "lexical_declaration") {
					this.accumulateDeclarationBindings(initializer, loopBindings);
				} else if (initializer.type === "variable_declaration") {
					this.accumulateDeclarationBindings(initializer, loopBindings);
					if (fnBindings) {
						this.accumulateDeclarationBindings(initializer, fnBindings);
					}
				}
			}

			// Extract calls from condition and update expressions
			for (const child of node.children) {
				const fieldName = this.getFieldName(node, child);
				if (fieldName === "condition" || fieldName === "increment") {
					this.extractCallsFromNode(
						child,
						ctx,
						callerNodeUid,
						loopBindings,
						fnBindings,
					);
				}
			}
		} else if (node.type === "for_in_statement") {
			// for_in_statement: loop variable is the "left" field
			const left = node.childForFieldName("left");
			if (left?.type === "identifier") {
				// Shadow-only: for (const item of items) — no type info available
				loopBindings.set(left.text, null);
			}

			// Extract calls from the iterable expression (right side)
			const right = node.childForFieldName("right");
			if (right) {
				this.extractCallsFromNode(
					right,
					ctx,
					callerNodeUid,
					loopBindings,
					fnBindings,
				);
			}
		}

		// Process the loop body with the loop-scoped bindings
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(
				body,
				ctx,
				callerNodeUid,
				loopBindings,
				fnBindings,
			);
		}
	}

	/**
	 * Get the field name for a child node within its parent.
	 * Returns null if the child is not a named field.
	 */
	private getFieldName(parent: SyntaxNode, child: SyntaxNode): string | null {
		for (let i = 0; i < parent.childCount; i++) {
			if (parent.child(i)?.id === child.id) {
				return parent.fieldNameForChild(i);
			}
		}
		return null;
	}

	/**
	 * Walk a statement_block's children in source order, accumulating
	 * bindings as declarations are encountered.
	 *
	 * Scope semantics modeled:
	 *
	 *   const/let (TDZ): Names shadow from the START of the block
	 *   (pre-scanned as null entries), but the typed binding is only
	 *   installed when the sequential walk reaches the declaration.
	 *   References before the declaration see null → no rewrite.
	 *
	 *   var (hoisted): Names are hoisted to function scope as shadow-only
	 *   entries by collectVarBindings(). The typed binding is installed
	 *   here when the sequential walk reaches the var declaration.
	 *
	 * Principle: shadow early, bind late.
	 */
	private walkBlockSequentially(
		block: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
		parentBindings?: ReadonlyMap<string, string | null>,
		fnBindings?: Map<string, string | null>,
	): void {
		const blockBindings = new Map<string, string | null>(parentBindings ?? []);

		// TDZ pre-scan: install shadow-only entries for all const/let
		// declarations in this block. This prevents earlier references
		// from resolving to an outer binding when an inner declaration
		// exists (temporal dead zone).
		this.prescanLexicalNames(block, blockBindings);

		for (const child of block.children) {
			if (this.isNewScopeNode(child)) continue;

			// const/let: extract calls from the initializer FIRST (the
			// binding is not yet usable inside its own initializer), then
			// upgrade the shadow entry to a typed binding for subsequent
			// statements.
			if (child.type === "lexical_declaration") {
				this.extractCallsFromNode(
					child,
					ctx,
					callerNodeUid,
					blockBindings,
					fnBindings,
				);
				this.accumulateDeclarationBindings(child, blockBindings);
				continue;
			}

			// var: same ordering — extract calls from initializer first,
			// then install typed binding in both block and function maps.
			if (child.type === "variable_declaration") {
				this.extractCallsFromNode(
					child,
					ctx,
					callerNodeUid,
					blockBindings,
					fnBindings,
				);
				this.accumulateDeclarationBindings(child, blockBindings);
				if (fnBindings) {
					this.accumulateDeclarationBindings(child, fnBindings);
				}
				continue;
			}

			this.extractCallsFromNode(
				child,
				ctx,
				callerNodeUid,
				blockBindings,
				fnBindings,
			);
		}
	}

	/**
	 * Pre-scan a block for const/let declaration names and install
	 * shadow-only (null) entries. This models the temporal dead zone:
	 * the name is "taken" from the start of the block, but the typed
	 * binding isn't available until the declaration statement.
	 */
	private prescanLexicalNames(
		block: SyntaxNode,
		bindings: Map<string, string | null>,
	): void {
		for (const child of block.children) {
			if (child.type !== "lexical_declaration") continue;
			for (const decl of child.children) {
				if (decl.type !== "variable_declarator") continue;
				const nameNode = decl.childForFieldName("name");
				if (nameNode) {
					bindings.set(nameNode.text, null);
				}
			}
		}
	}

	/**
	 * Install shadow-only (null) entries for all variable names in a
	 * declaration node (lexical_declaration or variable_declaration).
	 * Used to prevent initializer self-references from resolving.
	 */
	private prescanDeclarationNames(
		declNode: SyntaxNode,
		bindings: Map<string, string | null>,
	): void {
		for (const decl of declNode.children) {
			if (decl.type !== "variable_declarator") continue;
			const nameNode = decl.childForFieldName("name");
			if (nameNode) {
				bindings.set(nameNode.text, null);
			}
		}
	}

	/**
	 * Extract calls and new-expressions from a single AST node
	 * (without recursing into children).
	 */
	private extractCallsFromSingleNode(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
		localBindings?: ReadonlyMap<string, string | null>,
		fnBindings?: ReadonlyMap<string, string | null>,
	): void {
		if (node.type === "call_expression") {
			const fnNode = node.childForFieldName("function");
			if (fnNode) {
				const rawCalleeName = this.getCallTargetName(fnNode);
				if (rawCalleeName && !this.isBuiltinCall(rawCalleeName)) {
					const calleeName = this.resolveReceiverType(
						rawCalleeName,
						ctx,
						localBindings,
						fnBindings,
					);
					ctx.edges.push({
						edgeUid: uuidv4(),
						snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid,
						sourceNodeUid: callerNodeUid,
						targetKey: calleeName,
						type: EdgeType.CALLS,
						resolution: Resolution.STATIC,
						extractor: EXTRACTOR_NAME,
						location: locationFromNode(node),
						metadataJson: JSON.stringify({
							calleeName,
							...(calleeName !== rawCalleeName && {
								rawCalleeName,
							}),
						}),
					});
				}
			}
		}

		if (node.type === "new_expression") {
			const constructorNode = node.childForFieldName("constructor");
			if (constructorNode) {
				const className = constructorNode.text;
				ctx.edges.push({
					edgeUid: uuidv4(),
					snapshotUid: ctx.snapshotUid,
					repoUid: ctx.repoUid,
					sourceNodeUid: callerNodeUid,
					targetKey: className,
					type: EdgeType.INSTANTIATES,
					resolution: Resolution.STATIC,
					extractor: EXTRACTOR_NAME,
					location: locationFromNode(node),
					metadataJson: JSON.stringify({ className }),
				});
			}
		}
	}

	/**
	 * Add bindings from a lexical_declaration (const/let) to the
	 * given mutable map. Called during sequential block walking.
	 */
	private accumulateDeclarationBindings(
		declNode: SyntaxNode,
		bindings: Map<string, string | null>,
	): void {
		for (const decl of declNode.children) {
			if (decl.type !== "variable_declarator") continue;
			const nameNode = decl.childForFieldName("name");
			if (!nameNode) continue;

			const typeName = this.extractSimpleTypeName(decl);
			if (typeName) {
				bindings.set(nameNode.text, typeName);
				continue;
			}

			const value = decl.childForFieldName("value");
			if (value?.type === "new_expression") {
				const ctor = value.childForFieldName("constructor");
				if (ctor) {
					bindings.set(nameNode.text, ctor.text);
					continue;
				}
			}

			// Variable exists but has no resolvable type — shadow only
			bindings.set(nameNode.text, null);
		}
	}

	/**
	 * Recursively collect all `var` declaration NAMES in a function body
	 * and install them as shadow-only (null) entries.
	 *
	 * var is function-scoped in JS/TS: the name is hoisted to the
	 * function scope, but the initializer runs at statement position.
	 * Before the assignment, the variable is `undefined`, so we must
	 * not resolve calls through it. The typed binding is installed
	 * later by walkBlockSequentially when it reaches the var statement.
	 *
	 * Principle: shadow early, bind late.
	 */
	private collectVarBindings(
		node: SyntaxNode,
		bindings: Map<string, string | null>,
	): void {
		for (const child of node.children) {
			// Don't descend into nested functions — var doesn't hoist across them
			if (this.isNewScopeNode(child)) continue;

			if (child.type === "variable_declaration") {
				// Hoist names as shadow-only — the typed binding comes later
				for (const decl of child.children) {
					if (decl.type !== "variable_declarator") continue;
					const nameNode = decl.childForFieldName("name");
					if (nameNode) {
						bindings.set(nameNode.text, null);
					}
				}
			}

			// Recurse into nested blocks, if/for/while bodies, etc.
			this.collectVarBindings(child, bindings);
		}
	}

	private isNewScopeNode(node: SyntaxNode): boolean {
		return (
			node.type === "function_declaration" ||
			node.type === "class_declaration" ||
			node.type === "arrow_function" ||
			node.type === "function_expression" ||
			node.type === "method_definition"
		);
	}

	/**
	 * Collect parameter bindings from a formal_parameters node.
	 *
	 * Returns a map of parameter name → type name (or null if no simple
	 * type annotation). Entries with a type name enable receiver binding
	 * (e.g. `storage: StoragePort` → `storage.insert()` becomes
	 * `StoragePort.insert`). Entries with null shadow file-scope bindings
	 * without providing a positive binding.
	 */
	private collectParameterBindings(
		paramsNode: SyntaxNode | null,
	): Map<string, string | null> {
		const bindings = new Map<string, string | null>();
		if (!paramsNode) return bindings;
		for (const param of paramsNode.children) {
			if (
				param.type === "required_parameter" ||
				param.type === "optional_parameter"
			) {
				const pattern = param.childForFieldName("pattern");
				if (pattern) {
					const typeName = this.extractSimpleTypeName(param);
					bindings.set(pattern.text, typeName);
				}
			}
		}
		return bindings;
	}

	// ── Helpers ──────────────────────────────────────────────────────────

	private makeSymbolNode(
		name: string,
		subtype: NodeSubtype,
		visibility: Visibility,
		signature: string | null,
		node: SyntaxNode,
		ctx: ExtractionContext,
	): GraphNode {
		return {
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${name}:SYMBOL:${subtype}`,
			kind: NodeKind.SYMBOL,
			subtype,
			name,
			qualifiedName: name,
			fileUid: ctx.fileUid,
			parentNodeUid: null,
			location: locationFromNode(node),
			signature,
			visibility,
			docComment: this.extractDocComment(node),
			metadataJson: null,
		};
	}

	private getCallTargetName(fnNode: SyntaxNode): string | null {
		// Direct call: foo()
		if (fnNode.type === "identifier") {
			return fnNode.text;
		}
		// Member call: this.foo(), obj.bar(), a.b.c()
		if (fnNode.type === "member_expression") {
			const object = fnNode.childForFieldName("object");
			const property = fnNode.childForFieldName("property");
			if (object && property) {
				// For this.method() -> "ClassName.method" (best effort)
				// For obj.method() -> "obj.method"
				return `${object.text}.${property.text}`;
			}
		}
		return null;
	}

	/**
	 * Apply receiver type bindings to a raw call target name.
	 *
	 * Binding lookup order for variable.method():
	 *   1. Block-local bindings (const/let declarations in current block)
	 *   2. Function-level bindings (var declarations + parameters)
	 *   3. File-scope bindings (top-level const declarations)
	 *
	 * A null entry at any level means "name is taken but has no type" —
	 * it blocks all lower-priority lookups (shadow without rewrite).
	 *
	 * Class bindings (this.property.method) are separate and checked first.
	 */
	private resolveReceiverType(
		rawName: string,
		ctx: ExtractionContext,
		localBindings?: ReadonlyMap<string, string | null>,
		fnBindings?: ReadonlyMap<string, string | null>,
	): string {
		if (!rawName.includes(".")) return rawName;

		const parts = rawName.split(".");
		const method = parts[parts.length - 1];

		// Pattern 1: this.property.method() — class bindings, never shadowed.
		if (parts[0] === "this" && parts.length >= 3 && ctx.classBindings) {
			const propertyName = parts[1];
			const typeName = ctx.classBindings.get(propertyName);
			if (typeName) {
				return `${typeName}.${method}`;
			}
		}

		// Pattern 2: this.method() — rewrite to ClassName.method when
		// inside a class. This narrows the resolver's candidate pool from
		// "all nodes named method" to "the specific method on this class."
		// If the class doesn't define that method (e.g. inherited), the
		// qualified name won't match and the edge stays unresolved —
		// which is correct for syntax-only extraction.
		if (parts[0] === "this" && parts.length === 2) {
			if (ctx.enclosingClassName) {
				return `${ctx.enclosingClassName}.${method}`;
			}
			return rawName;
		}

		// Three-level variable type lookup: local → function → file.
		// A null entry at any level means "shadowed, stop looking."
		const resolveVarType = (varName: string): string | null | "shadowed" => {
			// Block-local bindings (const/let in current block)
			if (localBindings?.has(varName)) {
				const t = localBindings.get(varName);
				return t ?? "shadowed";
			}
			// Function-level bindings (var + params, shared across blocks)
			if (fnBindings?.has(varName)) {
				const t = fnBindings.get(varName);
				return t ?? "shadowed";
			}
			// File-scope bindings (top-level const)
			const fileType = ctx.fileScopeBindings.get(varName);
			if (fileType) return fileType;
			return null;
		};

		// Pattern 3: variable.method() — 2-part chain.
		if (parts.length === 2) {
			const result = resolveVarType(parts[0]);
			if (result === "shadowed") return rawName;
			if (result) return `${result}.${method}`;
			return rawName;
		}

		// Pattern 4: variable.property.method() — 3-part chain without `this`.
		if (parts.length >= 3 && parts[0] !== "this") {
			const result = resolveVarType(parts[0]);
			if (result === "shadowed") return rawName;
			if (result) {
				const propName = parts[1];
				const memberMap = ctx.memberTypes.get(result);
				if (memberMap) {
					const propType = memberMap.get(propName);
					if (propType) {
						return `${propType}.${method}`;
					}
				}
				return `${result}.${propName}.${method}`;
			}
		}

		return rawName;
	}

	private isBuiltinCall(name: string): boolean {
		// Filter out common JS/TS builtins that aren't user-defined symbols
		const builtins = new Set([
			"console.log",
			"console.error",
			"console.warn",
			"console.info",
			"console.debug",
			"JSON.parse",
			"JSON.stringify",
			"Object.keys",
			"Object.values",
			"Object.entries",
			"Object.assign",
			"Object.freeze",
			"Array.isArray",
			"Array.from",
			"Promise.resolve",
			"Promise.reject",
			"Promise.all",
			"Promise.allSettled",
			"Math.floor",
			"Math.ceil",
			"Math.round",
			"Math.max",
			"Math.min",
			"Math.abs",
			"parseInt",
			"parseFloat",
			"setTimeout",
			"setInterval",
			"clearTimeout",
			"clearInterval",
			"require",
		]);
		return builtins.has(name);
	}

	private isExported(node: SyntaxNode): boolean {
		return node.parent?.type === "export_statement" || false;
	}

	private getMethodVisibility(node: SyntaxNode): Visibility {
		for (const child of node.children) {
			if (child.type === "accessibility_modifier") {
				if (child.text === "private") return Visibility.PRIVATE;
				if (child.text === "protected") return Visibility.PROTECTED;
				if (child.text === "public") return Visibility.PUBLIC;
			}
		}
		return Visibility.PUBLIC; // TypeScript methods are public by default
	}

	private extractDocComment(node: SyntaxNode): string | null {
		// Tree-sitter places comments as siblings in the parent scope.
		// For declarations inside an export_statement, the comment is a
		// sibling of the export_statement, not the declaration itself.
		// So we check the node first, then fall back to its parent.
		return (
			this.findPrecedingDocComment(node) ??
			(node.parent ? this.findPrecedingDocComment(node.parent) : null)
		);
	}

	private findPrecedingDocComment(node: SyntaxNode): string | null {
		let prev = node.previousSibling;
		while (prev) {
			if (prev.type === "comment" && prev.text.startsWith("/**")) {
				return prev.text;
			}
			// Stop if we hit anything that isn't a comment
			if (prev.type !== "comment") {
				break;
			}
			prev = prev.previousSibling;
		}
		return null;
	}

	private findChildByType(node: SyntaxNode, type: string): SyntaxNode | null {
		for (const child of node.children) {
			if (child.type === type) return child;
		}
		return null;
	}

	private resolveImportPath(
		rawPath: string,
		currentFilePath: string,
	): string | null {
		const currentDir = dirname(currentFilePath);

		// Join and normalize: "./foo" from "src/bar/baz.ts" -> "src/bar/foo"
		const parts = [...currentDir.split("/"), ...rawPath.split("/")];
		const resolved: string[] = [];
		for (const part of parts) {
			if (part === "." || part === "") continue;
			if (part === "..") {
				resolved.pop();
			} else {
				resolved.push(part);
			}
		}
		let result = resolved.join("/");

		// Strip .js/.jsx extension — TS files commonly import with .js
		// but the actual source is .ts or .tsx. The extractor cannot know
		// which extension is correct without filesystem access, so we
		// normalize to extensionless and let the indexer resolve against
		// the actual file inventory (trying .ts, .tsx, /index.ts, /index.tsx).
		if (result.endsWith(".js")) {
			result = result.slice(0, -3);
		} else if (result.endsWith(".jsx")) {
			result = result.slice(0, -4);
		}

		return result;
	}
}

// ── Extraction context ─────────────────────────────────────────────────

interface ExtractionContext {
	source: string;
	filePath: string;
	fileUid: string;
	/** The node_uid of this file's FILE node — source of IMPORTS edges. */
	fileNodeUid: string;
	repoUid: string;
	snapshotUid: string;
	nodes: GraphNode[];
	edges: UnresolvedEdge[];
	/** Function-level metrics, keyed by stable_key. */
	metrics: Map<string, ExtractedMetrics>;
	/** Collected from `export { x, y }` lists — applied in a second pass. */
	exportedNames: Set<string>;
	/** Identifier-bearing import bindings observed in this file. */
	importBindings: ImportBinding[];

	// ── Receiver type bindings (v1.5) ─────────────────────────────────
	// These enable the extractor to emit typed target keys for member
	// calls (e.g. "StoragePort.insertNodes" instead of "this.storage.insertNodes").

	/**
	 * File-scope bindings: variable name → type name.
	 * Built from `const x: Type = ...` and `const x = new Type()`.
	 * Accumulated as top-level declarations are walked.
	 */
	fileScopeBindings: Map<string, string>;

	/**
	 * Class-scope bindings: property name → type name.
	 * Built from class field type annotations and constructor parameter
	 * properties. Set per-class during extractClass, null outside classes.
	 */
	classBindings: Map<string, string> | null;

	/**
	 * The name of the enclosing class, if currently inside a class body.
	 * Used to rewrite `this.method()` to `ClassName.method` so the
	 * resolver can match against the specific class's method node.
	 * Set per-class during extractClass, null outside classes.
	 */
	enclosingClassName: string | null;

	/**
	 * Member type map: type name → (member name → member type name).
	 * Built from interface property signatures and class field type
	 * annotations. Used for 3-part chain resolution:
	 *   ctx.storage.insertNodes() → AppContext.storage → StoragePort
	 * Only contains types defined in the current file.
	 */
	memberTypes: Map<string, Map<string, string>>;
}

// ── Utility ────────────────────────────────────────────────────────────

function locationFromNode(node: SyntaxNode): SourceLocation {
	return {
		lineStart: node.startPosition.row + 1, // tree-sitter is 0-based, we use 1-based
		colStart: node.startPosition.column,
		lineEnd: node.endPosition.row + 1,
		colEnd: node.endPosition.column,
	};
}

/**
 * Walk an `import_clause` AST node and return the local identifiers
 * each specifier binds.
 *
 * Handles:
 *   - default import:    `import X from "m"`                → ["X"]
 *   - named imports:     `import { a, b } from "m"`         → ["a", "b"]
 *   - renamed named:     `import { a as x } from "m"`       → ["x"]
 *   - namespace import:  `import * as ns from "m"`          → ["ns"]
 *   - default + named:   `import X, { a } from "m"`         → ["X", "a"]
 *
 * Does NOT introduce any binding for side-effect imports
 * (`import "m"`), which have no import_clause to walk.
 *
 * Order of returned identifiers follows their source-text order.
 */
function collectLocalIdentifiers(importClause: SyntaxNode): string[] {
	const identifiers: string[] = [];
	for (const child of importClause.children) {
		if (child.type === "identifier") {
			// Default import binding: `import X from "m"`
			identifiers.push(child.text);
		} else if (child.type === "namespace_import") {
			// `import * as ns from "m"` — the bound identifier is the
			// single `identifier` child of the namespace_import node.
			for (const n of child.children) {
				if (n.type === "identifier") {
					identifiers.push(n.text);
					break;
				}
			}
		} else if (child.type === "named_imports") {
			// `import { a, b as c } from "m"` — iterate import_specifier
			// children. When `alias` is present the LOCAL binding is the
			// alias, otherwise it's `name`.
			for (const spec of child.children) {
				if (spec.type !== "import_specifier") continue;
				const alias = spec.childForFieldName("alias");
				const name = spec.childForFieldName("name");
				const localName = alias?.text ?? name?.text;
				if (localName) identifiers.push(localName);
			}
		}
	}
	return identifiers;
}
