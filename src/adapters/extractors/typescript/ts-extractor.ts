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
	ExtractionResult,
	ExtractorPort,
	UnresolvedEdge,
} from "../../../core/ports/extractor.js";

const EXTRACTOR_NAME = "ts-core:0.2.0";
const __dirname = dirname(fileURLToPath(import.meta.url));
const GRAMMARS_DIR = join(__dirname, "..", "..", "..", "..", "grammars");

export class TypeScriptExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["typescript", "tsx"];

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

		const isTsx = filePath.endsWith(".tsx");
		this.parser.setLanguage(isTsx ? this.tsxLanguage : this.tsLanguage);

		const tree = this.parser.parse(source);
		if (!tree) {
			throw new Error(`Failed to parse ${filePath}`);
		}
		const nodes: GraphNode[] = [];
		const edges: UnresolvedEdge[] = [];

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
		return { nodes, edges };
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

		// Only track relative imports (project-internal dependencies)
		if (!rawPath.startsWith(".")) return;

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
			location: locationFromNode(node),
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

		// Extract calls within the function body
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, graphNode.nodeUid);
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

		// Extract methods
		const body = node.childForFieldName("body");
		if (!body) return;

		for (const member of body.children) {
			if (member.type === "method_definition") {
				this.extractMethod(member, classNode, ctx);
			} else if (member.type === "public_field_definition") {
				this.extractProperty(member, classNode, ctx);
			}
		}
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

		// Extract calls within the method body
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, methodNode.nodeUid);
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

		ctx.nodes.push(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.INTERFACE,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
		);
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

	private extractCallsFromNode(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		if (node.type === "call_expression") {
			const fnNode = node.childForFieldName("function");
			if (fnNode) {
				const calleeName = this.getCallTargetName(fnNode);
				if (calleeName && !this.isBuiltinCall(calleeName)) {
					ctx.edges.push({
						edgeUid: uuidv4(),
						snapshotUid: ctx.snapshotUid,
						repoUid: ctx.repoUid,
						sourceNodeUid: callerNodeUid,
						targetKey: calleeName, // resolved by indexer
						type: EdgeType.CALLS,
						resolution: Resolution.STATIC,
						extractor: EXTRACTOR_NAME,
						location: locationFromNode(node),
						metadataJson: JSON.stringify({ calleeName }),
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
					targetKey: className, // resolved by indexer
					type: EdgeType.INSTANTIATES,
					resolution: Resolution.STATIC,
					extractor: EXTRACTOR_NAME,
					location: locationFromNode(node),
					metadataJson: JSON.stringify({ className }),
				});
			}
		}

		// Recurse into children
		for (const child of node.children) {
			// Don't recurse into nested function/class declarations — they
			// get their own scope from visitTopLevel or extractMethod
			if (
				child.type === "function_declaration" ||
				child.type === "class_declaration" ||
				child.type === "arrow_function" ||
				child.type === "function_expression" ||
				child.type === "method_definition"
			) {
				continue;
			}
			this.extractCallsFromNode(child, ctx, callerNodeUid);
		}
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
	/** Collected from `export { x, y }` lists — applied in a second pass. */
	exportedNames: Set<string>;
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
