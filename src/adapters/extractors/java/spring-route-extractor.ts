/**
 * Spring MVC/Web route fact extractor (PROTOTYPE).
 *
 * Scans Java source files for Spring route annotations and emits
 * normalized BoundaryProviderFact records. Handles route composition:
 *   - class-level @RequestMapping("/api/v2/products")
 *   - method-level @GetMapping("/{id}")
 *   → composed path: "/api/v2/products/{id}", method: GET
 *
 * Supported annotations:
 *   @RequestMapping, @GetMapping, @PostMapping, @PutMapping,
 *   @DeleteMapping, @PatchMapping
 *
 * Implementation: LINE-BASED REGEX scanning over raw source text.
 * This is NOT AST-backed. Known limitations:
 *   - Fragile on multi-line annotations
 *   - Does not handle array-valued paths (@GetMapping({"/a", "/b"}))
 *   - Does not handle composed/meta-annotations
 *   - Does not handle annotations with complex attribute expressions
 *
 * Maturity: PROTOTYPE. Sufficient to validate the boundary-fact model
 * on real repos. Should be rebuilt on the Java parse tree if this
 * boundary model proves out and becomes a mature adapter.
 *
 * Not wired into the indexer yet. Standalone helper called manually
 * or by future integration code.
 */

import type { BoundaryProviderFact } from "../../../core/classification/api-boundary.js";

/** HTTP method — adapter-specific, not a core type. */
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD" | "ANY";

/** Annotation name → HTTP method mapping. */
const ROUTE_ANNOTATIONS: Record<string, HttpMethod> = {
	"GetMapping": "GET",
	"PostMapping": "POST",
	"PutMapping": "PUT",
	"DeleteMapping": "DELETE",
	"PatchMapping": "PATCH",
	"RequestMapping": "ANY",
};


/**
 * Extract Spring route facts from a Java source file.
 *
 * @param source - Full source text of the .java file.
 * @param filePath - Repo-relative path.
 * @param repoUid - Repository UID.
 * @param symbols - Already-extracted symbol nodes from the Java extractor,
 *                  used to map handler methods to stable keys.
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
): BoundaryProviderFact[] {
	const facts: BoundaryProviderFact[] = [];
	const lines = source.split("\n");

	// Pass 1: find class-level @RequestMapping prefix.
	let classPrefix = "";
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i].trim();
		// Class-level @RequestMapping comes before the class declaration.
		const rmMatch = line.match(
			/@RequestMapping\s*\(\s*(?:value\s*=\s*)?["']([^"']*)["']/,
		);
		if (rmMatch) {
			// Check if the NEXT non-annotation, non-empty line is a class declaration.
			for (let j = i + 1; j < Math.min(i + 5, lines.length); j++) {
				const nextLine = lines[j].trim();
				if (
					nextLine.startsWith("public class ") ||
					nextLine.startsWith("public abstract class ") ||
					nextLine.match(/^(?:public\s+)?class\s+/)
				) {
					classPrefix = rmMatch[1];
					break;
				}
			}
		}
	}

	// Pass 2: find method-level route annotations.
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i].trim();

		for (const [annName, defaultMethod] of Object.entries(ROUTE_ANNOTATIONS)) {
			// Match @GetMapping("path") or @GetMapping(value = "path") or
			// @GetMapping(path = "path") or @GetMapping (no path = root).
			const patterns = [
				// @GetMapping("path")
				new RegExp(`@${annName}\\s*\\(\\s*(?:value\\s*=\\s*|path\\s*=\\s*)?["']([^"']*)["']`),
				// @GetMapping (no args — empty path)
				new RegExp(`@${annName}\\s*$`),
				// @GetMapping() — empty parens
				new RegExp(`@${annName}\\s*\\(\\s*\\)`),
			];

			let methodPath = "";
			let matched = false;

			for (const pattern of patterns) {
				const m = line.match(pattern);
				if (m) {
					methodPath = m[1] ?? "";
					matched = true;
					break;
				}
			}

			if (!matched) continue;
			// Skip class-level @RequestMapping — already captured as prefix.
			if (annName === "RequestMapping") {
				const isMethodLevel = isFollowedByMethodDecl(lines, i);
				if (!isMethodLevel) continue;
			}

			// Determine HTTP method.
			let method: HttpMethod = defaultMethod;
			if (annName === "RequestMapping") {
				// Check for method = RequestMethod.GET etc.
				const methodMatch = line.match(
					/method\s*=\s*(?:RequestMethod\.)?(\w+)/,
				);
				if (methodMatch) {
					const m = methodMatch[1].toUpperCase();
					if (["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"].includes(m)) {
						method = m as HttpMethod;
					}
				}
			}

			// Compose full path.
			const fullPath = composePath(classPrefix, methodPath);

			// Find the handler method — next method declaration after the annotation.
			const handlerSymbol = findHandlerSymbol(lines, i, symbols);

			facts.push({
				mechanism: "http",
				address: fullPath,
				operation: `${method} ${fullPath}`,
				handlerStableKey: handlerSymbol?.stableKey ?? `${repoUid}:${filePath}#unknown_handler:SYMBOL:METHOD`,
				sourceFile: filePath,
				lineStart: i + 1,
				framework: "spring-mvc",
				basis: "annotation",
				schemaRef: null,
				metadata: { httpMethod: method },
			});
		}
	}

	return facts;
}

function composePath(prefix: string, methodPath: string): string {
	// Normalize: ensure prefix starts with / and method path joins cleanly.
	let p = prefix;
	if (p && !p.startsWith("/")) p = `/${p}`;
	if (p.endsWith("/")) p = p.slice(0, -1);

	let m = methodPath;
	if (m && !m.startsWith("/")) m = `/${m}`;

	return `${p}${m}` || "/";
}

/**
 * Check if an annotation is followed by a METHOD declaration (not a
 * class declaration). Used to distinguish class-level @RequestMapping
 * from method-level @RequestMapping.
 */
function isFollowedByMethodDecl(lines: string[], annotationLine: number): boolean {
	for (let j = annotationLine + 1; j < Math.min(annotationLine + 5, lines.length); j++) {
		const l = lines[j].trim();
		// Class declaration — NOT a method.
		if (l.match(/^(?:public\s+)?(?:abstract\s+)?class\s+/)) return false;
		if (l.match(/^(?:public\s+)?interface\s+/)) return false;
		// Method declaration — yes.
		if (l.match(/^(?:public|private|protected)\s+\S+\s+\w+\s*\(/)) return true;
		if (l.startsWith("@")) continue;
		if (l === "") continue;
	}
	return false;
}

function findHandlerSymbol(
	lines: string[],
	annotationLine: number,
	symbols: Array<{ stableKey: string; name: string; qualifiedName: string; lineStart: number | null }>,
): { stableKey: string } | null {
	// Find the method name on the next non-annotation line.
	for (let j = annotationLine + 1; j < Math.min(annotationLine + 5, lines.length); j++) {
		const l = lines[j].trim();
		// Match method declaration: "public ReturnType methodName("
		const methodMatch = l.match(
			/(?:public|private|protected)\s+\S+\s+(\w+)\s*\(/,
		);
		if (methodMatch) {
			const methodName = methodMatch[1];
			// Find the symbol with this name closest to this line.
			const candidates = symbols.filter(
				(s) => s.name === methodName && s.lineStart !== null,
			);
			if (candidates.length === 1) return candidates[0];
			// Multiple candidates: pick closest by line.
			const targetLine = j + 1;
			const closest = candidates.reduce<typeof candidates[0] | null>(
				(best, c) => {
					if (!best) return c;
					const d = Math.abs((c.lineStart ?? 0) - targetLine);
					const bestD = Math.abs((best.lineStart ?? 0) - targetLine);
					return d < bestD ? c : best;
				},
				null,
			);
			return closest;
		}
		if (l.startsWith("@")) continue;
	}
	return null;
}
