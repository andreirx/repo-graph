/**
 * HTTP client request fact extractor for TS/JS (PROTOTYPE).
 *
 * Scans TypeScript/JavaScript source files for HTTP client calls
 * and emits normalized BoundaryConsumerFact records.
 *
 * Supported patterns:
 *   - axios.get(`${BASE}/api/v2/products`)
 *   - axios.post(`${BASE}/api/v2/products`, data)
 *   - fetch(`${BASE}/api/v2/products`)
 *   - fetch("/api/v2/products")
 *   - axios.get(`${BASE_URL}/subpath`) where BASE_URL is a file-local
 *     constant resolved via FileLocalStringResolver
 *
 * Implementation: LINE-BASED REGEX scanning over raw source text.
 * NOT AST-backed. Known limitations:
 *   - Fragile on multi-line URL arguments
 *   - Template literal interpolation for path params produces
 *     approximate path templates (${id} → {param})
 *   - Does not handle wrapper functions unless they match the
 *     direct pattern
 *
 * Binding resolution: when a StringBindingTable is provided, template
 * literal interpolations like `${BASE_URL}` are resolved against
 * file-local constants before path normalization. This recovers
 * the full API path when the route prefix is baked into a constant.
 *
 * Maturity: PROTOTYPE. Sufficient to validate the boundary-fact
 * model. Should be rebuilt on the TS parse tree if this model
 * proves out.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";
import type { StringBindingTable } from "./file-local-string-resolver.js";

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | null;

/**
 * Known base URL patterns to strip from extracted paths.
 * These are environment variable interpolations that represent
 * the server base URL and should not be part of the matchable path.
 */
const BASE_URL_PATTERNS = [
	/\$\{[^}]*(?:API_URL|BASE_URL|SERVER_URL|BACKEND_URL|API_BASE)[^}]*\}/gi,
	/\$\{[^}]*(?:import\.meta\.env\.\w+)[^}]*\}/gi,
	/\$\{[^}]*(?:process\.env\.\w+)[^}]*\}/gi,
];

/**
 * Extract HTTP client request facts from a TS/JS source file.
 *
 * @param bindings - Optional file-local string binding table from
 *   FileLocalStringResolver. When provided, template literal
 *   interpolations like `${BASE_URL}` are resolved against known
 *   constants before path normalization.
 */
export function extractHttpClientRequests(
	source: string,
	filePath: string,
	_repoUid: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		lineStart: number | null;
	}>,
	bindings?: StringBindingTable,
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];
	const lines = source.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];

		// Pattern 1a: axios.METHOD(`url`) or axios.METHOD("url") — quoted URL argument.
		// Pattern 1b: axios.METHOD(IDENTIFIER, ...) — bare identifier resolved via bindings.
		const axiosMatch = line.match(
			/axios\.(get|post|put|delete|patch)\s*\(\s*([`"'][^)]*)/i,
		);
		const axiosBareMatch = !axiosMatch
			? line.match(/axios\.(get|post|put|delete|patch)\s*\(\s*([A-Za-z_]\w*)/i)
			: null;
		const effectiveAxiosMatch = axiosMatch ?? axiosBareMatch;
		if (effectiveAxiosMatch) {
			const method = effectiveAxiosMatch[1].toUpperCase() as HttpMethod;
			let rawUrlArg = effectiveAxiosMatch[2];
			let urlArg: string;
			let isBareIdentifier = false;

			if (axiosBareMatch && bindings) {
				// Bare identifier: resolve directly from binding table.
				const bound = bindings.get(rawUrlArg);
				if (bound && bound.confidence > 0) {
					urlArg = bound.value;
					isBareIdentifier = true;
				} else {
					// Unresolvable bare identifier — skip.
					// Not a quoted URL, cannot extract path.
					urlArg = rawUrlArg;
				}
			} else {
				urlArg = resolveBindingsInUrlArg(rawUrlArg, bindings);
			}
			const path = extractPathFromUrlArg(urlArg);
			if (path) {
				const caller = findEnclosingSymbol(i + 1, symbols);
				const wasResolved = urlArg !== rawUrlArg;
				facts.push({
					mechanism: "http",
					operation: `${method} ${path}`,
					address: path,
					callerStableKey: caller?.stableKey ?? `unknown:${filePath}:${i + 1}`,
					sourceFile: filePath,
					lineStart: i + 1,
					basis: isBareIdentifier ? "template" : (rawUrlArg.includes("${") ? "template" : "literal"),
					confidence: isBareIdentifier ? 0.85 : computeConsumerConfidence("axios", rawUrlArg, wasResolved),
					schemaRef: null,
					metadata: {
						httpMethod: method,
						rawUrl: rawUrlArg.slice(0, 100),
						...(wasResolved || isBareIdentifier ? { resolvedUrl: urlArg.slice(0, 100) } : {}),
					},
				});
			}
			continue;
		}

		// Pattern 2: axios({ method: "GET", url: ... })
		// Deferred — less common in glamCRM-style code.

		// Pattern 3a: fetch(`url`) or fetch("url") — quoted URL argument.
		// Pattern 3b: fetch(IDENTIFIER, ...) — bare identifier resolved via bindings.
		const fetchMatch = line.match(
			/fetch\s*\(\s*([`"'][^)]*)/,
		);
		const fetchBareMatch = !fetchMatch
			? line.match(/fetch\s*\(\s*([A-Za-z_]\w*)\s*[,)]/i)
			: null;
		const effectiveFetchMatch = fetchMatch ?? fetchBareMatch;
		if (effectiveFetchMatch) {
			let rawUrlArg = effectiveFetchMatch[1];
			let urlArg: string;
			let isBareIdentifier = false;

			if (fetchBareMatch && bindings) {
				const bound = bindings.get(rawUrlArg);
				if (bound && bound.confidence > 0) {
					urlArg = bound.value;
					isBareIdentifier = true;
				} else {
					urlArg = rawUrlArg;
				}
			} else {
				urlArg = resolveBindingsInUrlArg(rawUrlArg, bindings);
			}
			const path = extractPathFromUrlArg(urlArg);
			if (path) {
				// Check for method in options object on same or next line.
				let method: HttpMethod = "GET";
				const methodInLine = line.match(
					/method\s*:\s*["'](\w+)["']/i,
				);
				if (methodInLine) {
					method = methodInLine[1].toUpperCase() as HttpMethod;
				} else if (i + 1 < lines.length) {
					const nextMethodMatch = lines[i + 1].match(
						/method\s*:\s*["'](\w+)["']/i,
					);
					if (nextMethodMatch) {
						method = nextMethodMatch[1].toUpperCase() as HttpMethod;
					}
				}

				const caller = findEnclosingSymbol(i + 1, symbols);
				const wasResolved = urlArg !== rawUrlArg;
				facts.push({
					mechanism: "http",
					operation: method ? `${method} ${path}` : path,
					address: path,
					callerStableKey: caller?.stableKey ?? `unknown:${filePath}:${i + 1}`,
					sourceFile: filePath,
					lineStart: i + 1,
					basis: isBareIdentifier ? "template" : (rawUrlArg.includes("${") ? "template" : "literal"),
					confidence: isBareIdentifier ? 0.75 : computeConsumerConfidence("fetch", rawUrlArg, wasResolved),
					schemaRef: null,
					metadata: {
						httpMethod: method,
						rawUrl: rawUrlArg.slice(0, 100),
						...(wasResolved || isBareIdentifier ? { resolvedUrl: urlArg.slice(0, 100) } : {}),
					},
				});
			}
		}
	}

	return facts;
}

/**
 * Resolve binding references in a URL argument string.
 *
 * Scans for `${IDENTIFIER}` patterns in the raw URL argument and
 * substitutes resolved values from the binding table. Only resolves
 * simple identifiers (no dot expressions, no nested templates).
 *
 * Falls back to the original string if no bindings are provided
 * or no binding matches.
 */
function resolveBindingsInUrlArg(
	urlArg: string,
	bindings?: StringBindingTable,
): string {
	if (!bindings || bindings.size === 0) return urlArg;

	// Match ${IDENTIFIER} patterns — simple identifiers only.
	return urlArg.replace(/\$\{([A-Za-z_]\w*)\}/g, (match, name) => {
		const bound = bindings.get(name);
		if (bound && bound.confidence > 0) {
			return bound.value;
		}
		return match; // Leave unresolved references as-is.
	});
}

/**
 * Compute consumer extraction confidence.
 *
 * Factors:
 *   - Client type: axios is more structured than fetch
 *   - URL type: literal > binding-resolved template > raw template
 *   - Binding resolution: resolved constants are slightly less confident
 *     than inline literals (one indirection level)
 */
function computeConsumerConfidence(
	clientType: "axios" | "fetch",
	rawUrlArg: string,
	wasResolved: boolean,
): number {
	const isTemplate = rawUrlArg.includes("${");

	if (clientType === "axios") {
		if (!isTemplate) return 0.95; // Literal URL, highest confidence.
		if (wasResolved) return 0.85; // Resolved through binding table.
		return 0.8; // Raw template, env vars stripped.
	}
	// fetch
	if (!isTemplate) return 0.9;
	if (wasResolved) return 0.75;
	return 0.7;
}

/**
 * Extract the API path from a URL argument string.
 * Strips base URL variables and normalizes path parameters.
 */
function extractPathFromUrlArg(urlArg: string): string | null {
	let url = urlArg.trim();
	// Remove surrounding quotes/backticks.
	if (url.startsWith("`") || url.startsWith("'") || url.startsWith('"')) {
		url = url.slice(1);
	}
	// Remove trailing quote/comma/paren.
	url = url.replace(/[`'")\s,].*$/, "");

	// Strip base URL patterns (environment variable interpolations).
	for (const pattern of BASE_URL_PATTERNS) {
		url = url.replace(pattern, "");
	}

	// Normalize template literal interpolations to path parameters.
	// ${projectId} → {param}, ${year}/${month} → {param}/{param}
	url = url.replace(/\$\{[^}]+\}/g, "{param}");

	// Must start with / to be a recognizable API path.
	if (!url.startsWith("/")) return null;
	// Must have at least one path segment.
	if (url === "/") return null;

	return url;
}

function findEnclosingSymbol(
	lineNumber: number,
	symbols: Array<{ stableKey: string; name: string; lineStart: number | null }>,
): { stableKey: string } | null {
	// Find the symbol whose lineStart is closest to and before this line.
	let best: { stableKey: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.lineStart === null) continue;
		if (s.lineStart <= lineNumber) {
			if (!best || s.lineStart > best.lineStart) {
				best = { stableKey: s.stableKey, lineStart: s.lineStart };
			}
		}
	}
	return best;
}
