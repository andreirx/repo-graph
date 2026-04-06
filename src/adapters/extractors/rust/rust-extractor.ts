/**
 * Rust language extractor -- first slice.
 *
 * Parses .rs files using web-tree-sitter with the tree-sitter-rust
 * grammar. Extracts:
 *   - FILE nodes (one per file)
 *   - SYMBOL nodes for functions, structs, enums, traits, impl methods,
 *     constants, statics, and type aliases
 *   - IMPORTS edges from `use` declarations
 *   - CALLS edges from function/method call expressions
 *   - ImportBinding records for `use` items
 *
 * Visibility: items with `pub` (or `pub(crate)`, `pub(super)`, etc.)
 * are marked EXPORT; items without are PRIVATE.
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

import { RUST_RUNTIME_BUILTINS } from "./runtime-builtins.js";

const EXTRACTOR_NAME = "rust-core:0.1.0";
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
	 * emissions from #[cfg(...)] conditional compilation, where
	 * tree-sitter sees both variants in the source text but only
	 * one compiles. The first emission wins.
	 */
	emittedStableKeys: Set<string>;
}

// -- Extractor class ------------------------------------------------------

export class RustExtractor implements ExtractorPort {
	readonly name = EXTRACTOR_NAME;
	readonly languages = ["rust"];
	readonly runtimeBuiltins: RuntimeBuiltinsSet = RUST_RUNTIME_BUILTINS;

	private parser: Parser | null = null;
	private rustLanguage: Language | null = null;

	async initialize(): Promise<void> {
		await Parser.init();
		this.parser = new Parser();

		const rustWasmPath = join(GRAMMARS_DIR, "tree-sitter-rust.wasm");
		// Verify the grammar file exists.
		await readFile(rustWasmPath);
		this.rustLanguage = await Language.load(rustWasmPath);
	}

	async extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult> {
		if (!this.parser || !this.rustLanguage) {
			throw new Error("Extractor not initialized. Call initialize() first.");
		}

		this.parser.setLanguage(this.rustLanguage);
		const tree = this.parser.parse(source);
		if (!tree) {
			throw new Error(`Failed to parse ${filePath}`);
		}

		const nodes: GraphNode[] = [];
		const edges: UnresolvedEdge[] = [];
		const metrics = new Map<string, ExtractedMetrics>();
		const importBindings: ImportBinding[] = [];

		// FILE node
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

		// Walk top-level declarations.
		for (const child of tree.rootNode.children) {
			this.visitTopLevel(child, ctx);
		}

		tree.delete();
		return { nodes, edges, metrics, importBindings };
	}

	/**
	 * Emit a node, deduplicating by stable_key. Returns true if emitted,
	 * false if a node with the same key was already emitted (e.g. from
	 * a #[cfg(...)] conditional variant).
	 */
	private emitNode(node: GraphNode, ctx: ExtractionContext): boolean {
		if (ctx.emittedStableKeys.has(node.stableKey)) return false;
		ctx.emittedStableKeys.add(node.stableKey);
		ctx.nodes.push(node);
		return true;
	}

	// -- Top-level visitor --------------------------------------------------

	private visitTopLevel(node: SyntaxNode, ctx: ExtractionContext): void {
		switch (node.type) {
			case "use_declaration":
				this.extractUseDeclaration(node, ctx);
				break;
			case "function_item":
				this.extractFunction(node, ctx);
				break;
			case "struct_item":
				this.extractStruct(node, ctx);
				break;
			case "enum_item":
				this.extractEnum(node, ctx);
				break;
			case "trait_item":
				this.extractTrait(node, ctx);
				break;
			case "impl_item":
				this.extractImpl(node, ctx);
				break;
			case "const_item":
				this.extractConst(node, ctx);
				break;
			case "static_item":
				this.extractStatic(node, ctx);
				break;
			case "type_item":
				this.extractTypeAlias(node, ctx);
				break;
			case "mod_item":
				this.extractModItem(node, ctx);
				break;
		}
	}

	// -- Use declaration extraction -----------------------------------------

	/**
	 * Extract `use` declarations.
	 *
	 * Handles:
	 *   - Simple paths: `use std::collections::HashMap;`
	 *   - Use lists:    `use std::collections::{HashMap, HashSet};`
	 *   - Glob imports:  `use std::io::*;`
	 *   - Aliased:       `use std::collections::HashMap as Map;`
	 *   - Relative:      `use crate::module::Thing;`
	 *                    `use super::Thing;`
	 *                    `use self::sub::Thing;`
	 */
	private extractUseDeclaration(
		node: SyntaxNode,
		ctx: ExtractionContext,
	): void {
		const location = locationFromNode(node);
		// Collect all use bindings from this declaration.
		const bindings = this.collectUseBindings(node);

		for (const binding of bindings) {
			const isRelative =
				binding.specifier.startsWith("crate::") ||
				binding.specifier.startsWith("super::") ||
				binding.specifier.startsWith("self::");

			// ImportBinding side-channel record.
			ctx.importBindings.push({
				identifier: binding.identifier,
				specifier: binding.specifier,
				isRelative,
				location,
				isTypeOnly: false, // Rust `use` imports both types and values
			});

			// IMPORTS edge. Target key uses the module specifier path.
			// For relative imports (crate/super/self), convert :: to /
			// to build a path-like target key.
			const targetKey = isRelative
				? `${ctx.repoUid}:${this.rustModuleToPath(binding.specifier)}:FILE`
				: `${binding.specifier}`;

			ctx.edges.push({
				edgeUid: uuidv4(),
				snapshotUid: ctx.snapshotUid,
				repoUid: ctx.repoUid,
				sourceNodeUid: ctx.fileNodeUid,
				targetKey,
				type: EdgeType.IMPORTS,
				resolution: Resolution.STATIC,
				extractor: EXTRACTOR_NAME,
				location,
				metadataJson: JSON.stringify({
					specifier: binding.specifier,
					identifier: binding.identifier,
				}),
			});
		}
	}

	/**
	 * Collect identifier/specifier pairs from a use_declaration node.
	 *
	 * Walks the tree-sitter AST for the `use` statement and returns
	 * one entry per imported identifier.
	 */
	private collectUseBindings(
		node: SyntaxNode,
	): Array<{ identifier: string; specifier: string }> {
		const results: Array<{ identifier: string; specifier: string }> = [];
		// The child of use_declaration (after `use` keyword and optional
		// visibility) is a use_as_clause, scoped_use_list, use_wildcard,
		// or a scoped_identifier/identifier.
		this.walkUseTree(node, [], results);
		return results;
	}

	/**
	 * Recursively walk the use tree collecting bindings.
	 * `pathPrefix` accumulates path segments as we descend into
	 * scoped_use_list nodes.
	 */
	private walkUseTree(
		node: SyntaxNode,
		pathPrefix: string[],
		results: Array<{ identifier: string; specifier: string }>,
	): void {
		for (const child of node.children) {
			switch (child.type) {
				case "scoped_identifier": {
					// e.g., `std::collections::HashMap`
					// The full path is the text; the identifier is the last segment.
					const fullPath = child.text.replace(/::/g, "::");
					const segments = fullPath.split("::");
					const identifier = segments[segments.length - 1];
					const specifier = segments.slice(0, -1).join("::");
					results.push({ identifier, specifier });
					break;
				}
				case "identifier": {
					// A bare identifier at use level: `use HashMap;` (rare)
					// or the final identifier in a scoped use.
					// Only count if this is a direct child of use_declaration
					// (bare use) or use_list.
					if (node.type === "use_declaration" || node.type === "use_list") {
						const specifier = [...pathPrefix].join("::");
						results.push({
							identifier: child.text,
							specifier: specifier || child.text,
						});
					}
					break;
				}
				case "use_as_clause": {
					// `HashMap as Map` or `std::collections::HashMap as Map`
					const pathNode = child.childForFieldName("path");
					const aliasNode = child.childForFieldName("alias");
					if (pathNode && aliasNode) {
						const fullPath = pathNode.text;
						const segments = fullPath.split("::");
						const specifier = [...pathPrefix, ...segments.slice(0, -1)].join(
							"::",
						);
						results.push({
							identifier: aliasNode.text,
							specifier: specifier || segments.slice(0, -1).join("::"),
						});
					} else if (pathNode) {
						// No alias, just a path.
						const fullPath = pathNode.text;
						const segments = fullPath.split("::");
						const identifier = segments[segments.length - 1];
						const specifier = [...pathPrefix, ...segments.slice(0, -1)].join(
							"::",
						);
						results.push({ identifier, specifier });
					}
					break;
				}
				case "scoped_use_list": {
					// `std::collections::{HashMap, HashSet}`
					// The path before `::` is the prefix; the use_list has the items.
					const pathNode = child.childForFieldName("path");
					const listNode = child.childForFieldName("list");
					const newPrefix = pathNode
						? [...pathPrefix, ...pathNode.text.split("::")]
						: [...pathPrefix];
					if (listNode) {
						this.walkUseTree(listNode, newPrefix, results);
					}
					break;
				}
				case "use_list": {
					// Directly recurse into use_list items.
					this.walkUseTree(child, pathPrefix, results);
					break;
				}
				case "use_wildcard": {
					// `use std::io::*;` -- glob import, no specific identifier.
					// We still record the specifier for the IMPORTS edge.
					const pathNode = child.childForFieldName("path");
					if (pathNode) {
						results.push({
							identifier: "*",
							specifier: pathNode.text,
						});
					} else {
						const specifier = pathPrefix.join("::");
						if (specifier) {
							results.push({ identifier: "*", specifier });
						}
					}
					break;
				}
			}
		}
	}

	/**
	 * Convert a Rust module path to a file-system-like path for target
	 * key construction.
	 *
	 * `crate::module::sub` -> `src/module/sub`  (best effort)
	 * `super::thing`       -> kept as-is (cannot resolve without context)
	 * `self::sub`          -> kept as-is
	 *
	 * This is a heuristic. The indexer will do the real resolution.
	 */
	private rustModuleToPath(specifier: string): string {
		const parts = specifier.split("::");
		if (parts[0] === "crate") {
			// Replace `crate` with `src` as the conventional Rust layout.
			parts[0] = "src";
		}
		return parts.join("/");
	}

	// -- Function extraction ------------------------------------------------

	private extractFunction(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;

		const exported = this.hasPubVisibility(node);
		const params = node.childForFieldName("parameters");
		const returnType = node.childForFieldName("return_type");
		const sig = params
			? `fn ${name}${params.text}${returnType ? ` ${returnType.text}` : ""}`
			: `fn ${name}()`;

		const graphNode = this.makeSymbolNode(
			name,
			NodeSubtype.FUNCTION,
			exported ? Visibility.EXPORT : Visibility.PRIVATE,
			sig,
			node,
			ctx,
		);
		if (!this.emitNode(graphNode, ctx)) return;

		// Extract calls from function body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, graphNode.nodeUid);
		}
	}

	// -- Struct extraction --------------------------------------------------

	private extractStruct(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		this.emitNode(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.CLASS, // Closest mapping for structs
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
			ctx,
		);
	}

	// -- Enum extraction ----------------------------------------------------

	private extractEnum(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		this.emitNode(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.ENUM,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
			ctx,
		);
	}

	// -- Trait extraction ---------------------------------------------------

	private extractTrait(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		const traitNode = this.makeSymbolNode(
			nameNode.text,
			NodeSubtype.INTERFACE, // Closest mapping for traits
			exported ? Visibility.EXPORT : Visibility.PRIVATE,
			null,
			node,
			ctx,
		);
		this.emitNode(traitNode, ctx);

		// Extract trait method signatures (no bodies in most cases,
		// but default implementations have bodies).
		const body = node.childForFieldName("body");
		if (!body) return;

		for (const member of body.children) {
			if (member.type === "function_item") {
				this.extractTraitMethod(member, traitNode, ctx);
			}
			// function_signature_item covers method signatures without
			// a default body: `fn foo(&self);`
			if (member.type === "function_signature_item") {
				this.extractTraitMethod(member, traitNode, ctx);
			}
		}
	}

	private extractTraitMethod(
		node: SyntaxNode,
		parentTraitNode: GraphNode,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const qualifiedName = `${parentTraitNode.name}.${name}`;

		const params = node.childForFieldName("parameters");
		const sig = params ? `fn ${name}${params.text}` : `fn ${name}()`;

		this.emitNode({
			nodeUid: uuidv4(),
			snapshotUid: ctx.snapshotUid,
			repoUid: ctx.repoUid,
			stableKey: `${ctx.repoUid}:${ctx.filePath}#${qualifiedName}:SYMBOL:${NodeSubtype.METHOD}`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.METHOD,
			name,
			qualifiedName,
			fileUid: ctx.fileUid,
			parentNodeUid: parentTraitNode.nodeUid,
			location: locationFromNode(node),
			signature: sig,
			visibility: Visibility.PUBLIC, // Trait methods are public by nature
			docComment: this.extractDocComment(node),
			metadataJson: null,
		}, ctx);
		// No call extraction for trait method signatures (no bodies).
		// Default implementations could have bodies, but we skip call
		// extraction from trait defaults in the first slice.
	}

	// -- Impl block extraction ----------------------------------------------

	/**
	 * Extract an `impl` block.
	 *
	 * Handles both inherent impls (`impl Struct { ... }`) and trait
	 * impls (`impl Trait for Struct { ... }`).
	 *
	 * Methods inside the impl block become METHOD nodes parented to a
	 * synthetic or existing struct/trait node. For stable keys, we
	 * use `TypeName.method_name` as the qualified name (same pattern
	 * as class methods in the TS extractor).
	 */
	private extractImpl(node: SyntaxNode, ctx: ExtractionContext): void {
		// Determine the impl target type name.
		const typeNode = node.childForFieldName("type");
		if (!typeNode) return;
		const typeName = typeNode.text;

		// Check for trait impl: `impl Trait for Type`.
		const traitNode = node.childForFieldName("trait");
		const traitName = traitNode?.text ?? null;

		const body = node.childForFieldName("body");
		if (!body) return;

		for (const member of body.children) {
			if (member.type === "function_item") {
				this.extractImplMethod(member, typeName, traitName, ctx);
			}
		}
	}

	private extractImplMethod(
		node: SyntaxNode,
		typeName: string,
		traitName: string | null,
		ctx: ExtractionContext,
	): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;
		const name = nameNode.text;
		const qualifiedName = `${typeName}.${name}`;

		const exported = this.hasPubVisibility(node);
		const params = node.childForFieldName("parameters");
		const returnType = node.childForFieldName("return_type");
		const sig = params
			? `fn ${name}${params.text}${returnType ? ` ${returnType.text}` : ""}`
			: `fn ${name}()`;

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
			parentNodeUid: null, // No parent node for impl methods in first slice
			location: locationFromNode(node),
			signature: sig,
			visibility: exported ? Visibility.EXPORT : Visibility.PRIVATE,
			docComment: this.extractDocComment(node),
			metadataJson: traitName
				? JSON.stringify({ implTrait: traitName, implType: typeName })
				: JSON.stringify({ implType: typeName }),
		};
		// Dedup: if this method was already emitted (cfg variant), skip
		// all edges and call extraction that would reference its node UID.
		if (!this.emitNode(methodNode, ctx)) return;

		// If this is a trait impl, emit an IMPLEMENTS edge.
		if (traitName) {
			ctx.edges.push({
				edgeUid: uuidv4(),
				snapshotUid: ctx.snapshotUid,
				repoUid: ctx.repoUid,
				sourceNodeUid: methodNode.nodeUid,
				targetKey: traitName,
				type: EdgeType.IMPLEMENTS,
				resolution: Resolution.STATIC,
				extractor: EXTRACTOR_NAME,
				location: locationFromNode(node),
				metadataJson: JSON.stringify({
					traitName,
					typeName,
				}),
			});
		}

		// Extract calls from method body.
		const body = node.childForFieldName("body");
		if (body) {
			this.extractCallsFromNode(body, ctx, methodNode.nodeUid);
		}
	}

	// -- Const/static/type alias extraction ---------------------------------

	private extractConst(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		this.emitNode(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.CONSTANT,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
			ctx,
		);
	}

	private extractStatic(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		this.emitNode(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.VARIABLE,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
			ctx,
		);
	}

	private extractTypeAlias(node: SyntaxNode, ctx: ExtractionContext): void {
		const nameNode = node.childForFieldName("name");
		if (!nameNode) return;

		const exported = this.hasPubVisibility(node);
		this.emitNode(
			this.makeSymbolNode(
				nameNode.text,
				NodeSubtype.TYPE_ALIAS,
				exported ? Visibility.EXPORT : Visibility.PRIVATE,
				null,
				node,
				ctx,
			),
			ctx,
		);
	}

	// -- Mod item extraction ------------------------------------------------

	/**
	 * Extract inline module declarations: `mod name { ... }`.
	 *
	 * For inline modules with a body, we recurse into the body to
	 * extract nested declarations. For `mod name;` declarations
	 * (external file modules), we skip -- the file will be picked up
	 * separately by the indexer.
	 */
	private extractModItem(node: SyntaxNode, ctx: ExtractionContext): void {
		const body = node.childForFieldName("body");
		if (!body) return; // `mod foo;` -- external module, skip

		// Recurse into inline module body to extract nested items.
		for (const child of body.children) {
			this.visitTopLevel(child, ctx);
		}
	}

	// -- Call extraction ----------------------------------------------------

	/**
	 * Recursively walk a node tree extracting CALLS edges from
	 * call_expression nodes.
	 *
	 * First-slice simplifications:
	 *   - No scope/binding tracking (unlike the TS extractor).
	 *   - No receiver type resolution.
	 *   - Just raw callee text for the target key.
	 */
	private extractCallsFromNode(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		if (node.type === "call_expression") {
			this.emitCallEdge(node, ctx, callerNodeUid);
		}

		// Recurse into children. Skip nested function items (they are
		// their own scope and will be extracted separately if they
		// appear at the top level, or skipped if closures).
		for (const child of node.children) {
			if (child.type === "function_item") continue;
			if (child.type === "closure_expression") {
				// Walk closure bodies for calls attributed to the
				// enclosing function.
				this.extractCallsFromNode(child, ctx, callerNodeUid);
				continue;
			}
			this.extractCallsFromNode(child, ctx, callerNodeUid);
		}
	}

	/**
	 * Emit a CALLS edge for a call_expression node.
	 *
	 * Rust call expressions:
	 *   - Function call: `foo(args)` -> targetKey = "foo"
	 *   - Method call:   `self.method(args)` -> tree-sitter parses
	 *     this as call_expression with member access, but Rust actually
	 *     uses `method_call_expression` for `.method()` syntax.
	 *   - Associated function: `HashMap::new()` -> targetKey = "HashMap.new"
	 *   - Scoped path:   `module::func()` -> targetKey = "module.func"
	 *
	 * Note: tree-sitter-rust distinguishes `call_expression` (function
	 * calls and associated function calls) from method calls which
	 * are actually field_expression or similar. Method calls on self
	 * like `self.foo()` are represented differently.
	 *
	 * In tree-sitter-rust:
	 *   - `foo(args)` -> call_expression { function: identifier "foo", arguments }
	 *   - `Foo::bar(args)` -> call_expression { function: scoped_identifier, arguments }
	 *   - `x.method(args)` -> NOT a call_expression -- it is a separate node type
	 *     that we handle below.
	 */
	private emitCallEdge(
		node: SyntaxNode,
		ctx: ExtractionContext,
		callerNodeUid: string,
	): void {
		const fnNode = node.childForFieldName("function");
		if (!fnNode) return;

		const targetKey = this.getCallTargetName(fnNode);
		if (!targetKey) return;

		// Skip runtime builtins that are macro-like (println, format, etc.)
		// These are noise in the call graph.
		if (this.isRustMacroLikeBuiltin(targetKey)) return;

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
	 * Extract the call target name from a function node in a
	 * call_expression.
	 *
	 * Returns dot-notation for consistency with the TS extractor:
	 *   - `foo` -> "foo"
	 *   - `Foo::bar` -> "Foo.bar"
	 *   - `module::func` -> "module.func"
	 *   - `self.method` -> "self.method"
	 *   - `a::b::c` -> "a.b.c"
	 */
	private getCallTargetName(fnNode: SyntaxNode): string | null {
		if (fnNode.type === "identifier") {
			return fnNode.text;
		}
		if (fnNode.type === "scoped_identifier") {
			// `Foo::bar` or `std::collections::HashMap::new`
			// Convert :: to . for consistency.
			return fnNode.text.replace(/::/g, ".");
		}
		if (fnNode.type === "field_expression") {
			// `self.method` or `obj.field`
			const object = fnNode.childForFieldName("value");
			const field = fnNode.childForFieldName("field");
			if (object && field) {
				return `${object.text}.${field.text}`;
			}
		}
		return null;
	}

	/**
	 * Check if a target name is a Rust macro-like builtin that would
	 * produce noise in the call graph. These are typically invoked
	 * as macros (println!, format!, etc.) but tree-sitter may parse
	 * some macro invocations as call expressions when the `!` is
	 * part of a macro_invocation node rather than a call_expression.
	 *
	 * In practice, tree-sitter-rust parses `println!(...)` as a
	 * macro_invocation, not a call_expression. But we keep this guard
	 * for edge cases and future-proofing.
	 */
	private isRustMacroLikeBuiltin(name: string): boolean {
		const MACRO_BUILTINS = new Set([
			"println",
			"eprintln",
			"print",
			"eprint",
			"format",
			"write",
			"writeln",
			"todo",
			"unimplemented",
			"unreachable",
			"panic",
			"assert",
			"assert_eq",
			"assert_ne",
			"dbg",
			"vec",
			"cfg",
		]);
		return MACRO_BUILTINS.has(name);
	}

	// -- Helpers ------------------------------------------------------------

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

	/**
	 * Check whether a node has a `pub` visibility modifier.
	 * Matches `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`.
	 */
	private hasPubVisibility(node: SyntaxNode): boolean {
		for (const child of node.children) {
			if (child.type === "visibility_modifier") {
				return true;
			}
		}
		return false;
	}

	/**
	 * Extract a doc comment preceding a node.
	 *
	 * Rust doc comments:
	 *   - `///` line doc comments (tree-sitter type: "line_comment")
	 *   - `//!` inner doc comments
	 *   - Block doc comments are less common
	 *
	 * We collect consecutive `///` comments immediately preceding
	 * the declaration.
	 */
	private extractDocComment(node: SyntaxNode): string | null {
		const comments: string[] = [];
		let prev = node.previousSibling;

		while (prev) {
			if (prev.type === "line_comment" && prev.text.startsWith("///")) {
				comments.unshift(prev.text);
				prev = prev.previousSibling;
			} else if (prev.type === "block_comment" && prev.text.startsWith("/**")) {
				comments.unshift(prev.text);
				break;
			} else if (prev.type === "attribute_item") {
				// Skip attributes (#[...]) between doc comments and the declaration.
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

function locationFromNode(node: SyntaxNode): SourceLocation {
	return {
		lineStart: node.startPosition.row + 1, // tree-sitter is 0-based
		colStart: node.startPosition.column,
		lineEnd: node.endPosition.row + 1,
		colEnd: node.endPosition.column,
	};
}
