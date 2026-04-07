/**
 * Spring MVC/Web route fact extractor (MATURE — AST-backed).
 *
 * Scans Java source files for Spring route annotations using the
 * tree-sitter-java parse tree and emits normalized BoundaryProviderFact
 * records. Handles route composition:
 *   - class-level @RequestMapping("/api/v2/products")
 *   - method-level @GetMapping("/{id}")
 *   → composed path: "/api/v2/products/{id}", method: GET
 *
 * Supported annotations:
 *   @RequestMapping, @GetMapping, @PostMapping, @PutMapping,
 *   @DeleteMapping, @PatchMapping
 *
 * Supported argument forms:
 *   - positional string:  @GetMapping("/path")
 *   - value attribute:    @GetMapping(value = "/path")
 *   - path attribute:     @GetMapping(path = "/path")
 *   - method attribute:   @RequestMapping(method = RequestMethod.GET)
 *   - multiline annotations (handled natively by AST)
 *   - empty or no-arg:    @GetMapping, @GetMapping()
 *
 * NOT supported:
 *   - Array-valued paths: @GetMapping({"/a", "/b"}) — emits only first
 *   - Custom composed annotations (@AdminOnly = @RequestMapping(...))
 *   - Spring WebFlux functional endpoints
 *
 * Maturity: MATURE. AST-backed, multiline-safe, attribute-aware.
 * Rebuilt from the PROTOTYPE regex version after boundary-fact model
 * proved out on glamCRM.
 *
 * Parser lifecycle:
 *   - When a pre-parsed rootNode is provided, uses it directly (no reparse).
 *   - When no rootNode is provided AND initSpringRouteParser() has been
 *     called, parses source text using the shared Java parser.
 *   - When no rootNode is provided AND the parser is NOT initialized,
 *     returns [] silently. Callers must ensure init before use.
 *
 * Known inefficiency: the indexer currently calls initSpringRouteParser()
 * and passes source text without a rootNode, causing each Java file to
 * be parsed twice (once by the Java extractor, once here). Passing the
 * Java extractor's parse tree through would eliminate this, but requires
 * extending ExtractorPort or ExtractionResult to expose the tree. That
 * is deferred until the double-parse cost becomes measurable.
 */

import { type Language, type Node as SyntaxNode, Parser } from "web-tree-sitter";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { BoundaryProviderFact } from "../../../core/classification/api-boundary.js";

/** HTTP method — adapter-specific, not a core type. */
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD" | "ANY";

/** Annotation name → default HTTP method mapping. */
const ROUTE_ANNOTATIONS: Record<string, HttpMethod> = {
	GetMapping: "GET",
	PostMapping: "POST",
	PutMapping: "PUT",
	DeleteMapping: "DELETE",
	PatchMapping: "PATCH",
	RequestMapping: "ANY",
};

const GRAMMARS_DIR = join(import.meta.dirname, "../../../../grammars");

// ── Standalone parser (lazy-init for non-indexer callers) ───────────

let sharedParser: Parser | null = null;
let sharedLanguage: Language | null = null;

async function ensureParser(): Promise<{ parser: Parser; language: Language }> {
	if (sharedParser && sharedLanguage) {
		return { parser: sharedParser, language: sharedLanguage };
	}
	await Parser.init();
	sharedParser = new Parser();
	const wasmPath = join(GRAMMARS_DIR, "tree-sitter-java.wasm");
	await readFile(wasmPath);
	const { Language: Lang } = await import("web-tree-sitter");
	sharedLanguage = await Lang.load(wasmPath);
	return { parser: sharedParser, language: sharedLanguage };
}

// ── Main extractor ──────────────────────────────────────────────────

/**
 * Extract Spring route facts from a Java source file.
 *
 * @param source - Full source text of the .java file.
 * @param filePath - Repo-relative path.
 * @param repoUid - Repository UID.
 * @param symbols - Already-extracted symbol nodes from the Java extractor,
 *   used to map handler methods to stable keys.
 * @param rootNode - Optional pre-parsed tree-sitter root node. When
 *   provided, avoids a redundant parse. When omitted, the source is
 *   parsed internally using a lazy-initialized Java parser.
 */
export function extractSpringRoutes(
	source: string,
	filePath: string,
	repoUid: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		qualifiedName: string;
		lineStart: number | null;
	}>,
	rootNode?: SyntaxNode,
): BoundaryProviderFact[] {
	// Quick gate: skip files that don't have any route annotation.
	if (!hasRouteAnnotation(source)) return [];

	// If no pre-parsed tree, parse synchronously using the shared parser.
	// The shared parser is initialized lazily on first use.
	let tree: SyntaxNode;
	if (rootNode) {
		tree = rootNode;
	} else {
		if (!sharedParser || !sharedLanguage) {
			// Cannot parse synchronously without init. Return empty.
			// The indexer path always provides rootNode or calls initParser first.
			return [];
		}
		sharedParser.setLanguage(sharedLanguage);
		const parsed = sharedParser.parse(source);
		if (!parsed) return [];
		tree = parsed.rootNode;
	}

	const facts: BoundaryProviderFact[] = [];

	// Walk class declarations at the top level.
	for (const child of tree.children) {
		if (child.type !== "class_declaration") continue;
		extractFromClass(child, "", filePath, repoUid, symbols, facts);
	}

	return facts;
}

/**
 * Initialize the standalone parser. Call before extractSpringRoutes()
 * if no rootNode will be provided. The indexer does not need this —
 * it passes the pre-parsed tree.
 */
export async function initSpringRouteParser(): Promise<void> {
	await ensureParser();
}

// ── AST extraction logic ────────────────────────────────────────────

/**
 * Extract routes from a class declaration.
 * Handles class-level @RequestMapping prefix and method-level annotations.
 */
function extractFromClass(
	classNode: SyntaxNode,
	_outerPrefix: string,
	filePath: string,
	repoUid: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		qualifiedName: string;
		lineStart: number | null;
	}>,
	facts: BoundaryProviderFact[],
): void {
	// Find class-level @RequestMapping prefix from modifiers.
	const classPrefix = extractClassPrefix(classNode);

	// Walk method declarations inside the class body.
	const body = classNode.childForFieldName("body");
	if (!body) return;

	for (const member of body.children) {
		if (member.type !== "method_declaration") continue;

		// Check if this method has a route annotation.
		const routeInfo = extractMethodRouteAnnotation(member);
		if (!routeInfo) continue;

		// Compose path.
		const fullPath = composePath(classPrefix, routeInfo.path);

		// Find handler symbol.
		const methodName = member.childForFieldName("name");
		const handlerSymbol = methodName
			? findSymbolByNameAndLine(
					methodName.text,
					methodName.startPosition.row + 1,
					symbols,
				)
			: null;

		facts.push({
			mechanism: "http",
			address: fullPath,
			operation: `${routeInfo.method} ${fullPath}`,
			handlerStableKey:
				handlerSymbol?.stableKey ??
				`${repoUid}:${filePath}#unknown_handler:SYMBOL:METHOD`,
			sourceFile: filePath,
			lineStart: routeInfo.annotationLine,
			framework: "spring-mvc",
			basis: "annotation",
			schemaRef: null,
			metadata: { httpMethod: routeInfo.method },
		});
	}
}

/**
 * Extract the class-level @RequestMapping path prefix.
 * Returns empty string if no class-level mapping exists.
 */
function extractClassPrefix(classNode: SyntaxNode): string {
	const modifiers = classNode.children.find((c) => c.type === "modifiers");
	if (!modifiers) return "";

	for (const child of modifiers.children) {
		if (child.type !== "annotation") continue;
		const nameNode = child.childForFieldName("name");
		if (!nameNode || nameNode.text !== "RequestMapping") continue;

		return extractPathFromAnnotation(child);
	}
	return "";
}

interface MethodRouteInfo {
	method: HttpMethod;
	path: string;
	annotationLine: number;
}

/**
 * Extract route information from a method declaration's annotations.
 * Returns null if no route annotation is found.
 */
function extractMethodRouteAnnotation(
	methodNode: SyntaxNode,
): MethodRouteInfo | null {
	const modifiers = methodNode.children.find((c) => c.type === "modifiers");
	if (!modifiers) return null;

	for (const child of modifiers.children) {
		// tree-sitter-java emits "annotation" for @X(...) and
		// "marker_annotation" for @X (no parens).
		if (child.type !== "annotation" && child.type !== "marker_annotation") continue;
		const nameNode = child.childForFieldName("name");
		if (!nameNode) continue;

		const annotationName = nameNode.text;
		const defaultMethod = ROUTE_ANNOTATIONS[annotationName];
		if (defaultMethod === undefined) continue;

		const path = child.type === "marker_annotation"
			? "" // No arguments — empty path.
			: extractPathFromAnnotation(child);
		let method = defaultMethod;

		// For @RequestMapping, check if method is specified.
		if (annotationName === "RequestMapping" && child.type === "annotation") {
			const explicitMethod = extractMethodFromAnnotation(child);
			if (explicitMethod) method = explicitMethod;
		}

		return {
			method,
			path,
			annotationLine: child.startPosition.row + 1,
		};
	}
	return null;
}

/**
 * Extract the path string from an annotation's arguments.
 *
 * Handles:
 *   @GetMapping("/path")                  → "/path"
 *   @GetMapping(value = "/path")          → "/path"
 *   @GetMapping(path = "/path")           → "/path"
 *   @GetMapping                           → ""
 *   @GetMapping()                         → ""
 */
function extractPathFromAnnotation(annotation: SyntaxNode): string {
	const args = annotation.childForFieldName("arguments");
	if (!args) return "";

	// Check for element_value_pair with name "value" or "path".
	for (const child of args.children) {
		if (child.type === "element_value_pair") {
			const pairName = child.children.find((c) => c.type === "identifier");
			if (pairName && (pairName.text === "value" || pairName.text === "path")) {
				return extractStringValue(child);
			}
		}
	}

	// Check for positional string_literal (no element_value_pair).
	for (const child of args.children) {
		if (child.type === "string_literal") {
			return extractStringFromLiteral(child);
		}
	}

	return "";
}

/**
 * Extract the HTTP method from a @RequestMapping annotation.
 * Looks for: method = RequestMethod.GET (or just GET).
 */
function extractMethodFromAnnotation(annotation: SyntaxNode): HttpMethod | null {
	const args = annotation.childForFieldName("arguments");
	if (!args) return null;

	for (const child of args.children) {
		if (child.type !== "element_value_pair") continue;
		const pairName = child.children.find((c) => c.type === "identifier");
		if (!pairName || pairName.text !== "method") continue;

		// Find field_access (RequestMethod.GET) or identifier (GET).
		for (const vc of child.children) {
			if (vc.type === "field_access") {
				// RequestMethod.DELETE → last identifier is the method.
				const parts = vc.children.filter((c) => c.type === "identifier");
				const methodId = parts[parts.length - 1];
				if (methodId) {
					return normalizeHttpMethod(methodId.text);
				}
			}
			if (vc.type === "identifier" && vc.text !== "method") {
				return normalizeHttpMethod(vc.text);
			}
		}
	}
	return null;
}

/**
 * Extract a string value from an element_value_pair node.
 * The string_literal is a child of the pair.
 */
function extractStringValue(pair: SyntaxNode): string {
	for (const child of pair.children) {
		if (child.type === "string_literal") {
			return extractStringFromLiteral(child);
		}
	}
	return "";
}

/**
 * Extract text content from a string_literal node.
 * string_literal → string_fragment.
 */
function extractStringFromLiteral(node: SyntaxNode): string {
	const fragment = node.children.find((c) => c.type === "string_fragment");
	return fragment?.text ?? "";
}

function normalizeHttpMethod(text: string): HttpMethod | null {
	const upper = text.toUpperCase();
	const valid: HttpMethod[] = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
	return valid.includes(upper as HttpMethod) ? (upper as HttpMethod) : null;
}

// ── Shared helpers ──────────────────────────────────────────────────

function composePath(prefix: string, methodPath: string): string {
	let p = prefix;
	if (p && !p.startsWith("/")) p = `/${p}`;
	if (p.endsWith("/")) p = p.slice(0, -1);

	let m = methodPath;
	if (m && !m.startsWith("/")) m = `/${m}`;

	return `${p}${m}` || "/";
}

function hasRouteAnnotation(source: string): boolean {
	return (
		source.includes("GetMapping") ||
		source.includes("PostMapping") ||
		source.includes("PutMapping") ||
		source.includes("DeleteMapping") ||
		source.includes("PatchMapping") ||
		source.includes("RequestMapping")
	);
}

function findSymbolByNameAndLine(
	name: string,
	line: number,
	symbols: Array<{
		stableKey: string;
		name: string;
		lineStart: number | null;
	}>,
): { stableKey: string } | null {
	// Exact name + closest line match.
	const candidates = symbols.filter((s) => s.name === name);
	if (candidates.length === 0) return null;
	if (candidates.length === 1) return candidates[0];

	return candidates.reduce<(typeof candidates)[0] | null>((best, c) => {
		if (!best) return c;
		const d = Math.abs((c.lineStart ?? 0) - line);
		const bestD = Math.abs((best.lineStart ?? 0) - line);
		return d < bestD ? c : best;
	}, null);
}
