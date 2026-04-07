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
 *
 * Implementation: LINE-BASED REGEX scanning over raw source text.
 * NOT AST-backed. Known limitations:
 *   - Fragile on multi-line URL arguments
 *   - Does not resolve variable-based URLs
 *   - Template literal interpolation for path params produces
 *     approximate path templates (${id} → {param})
 *   - Does not handle wrapper functions unless they match the
 *     direct pattern
 *
 * Maturity: PROTOTYPE. Sufficient to validate the boundary-fact
 * model. Should be rebuilt on the TS parse tree if this model
 * proves out.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";

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
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];
	const lines = source.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];

		// Pattern 1: axios.METHOD(url)
		const axiosMatch = line.match(
			/axios\.(get|post|put|delete|patch)\s*\(\s*([`"'][^)]*)/i,
		);
		if (axiosMatch) {
			const method = axiosMatch[1].toUpperCase() as HttpMethod;
			const urlArg = axiosMatch[2];
			const path = extractPathFromUrlArg(urlArg);
			if (path) {
				const caller = findEnclosingSymbol(i + 1, symbols);
				facts.push({
					mechanism: "http",
					operation: `${method} ${path}`,
					address: path,
					callerStableKey: caller?.stableKey ?? `unknown:${filePath}:${i + 1}`,
					sourceFile: filePath,
					lineStart: i + 1,
					basis: urlArg.includes("${") ? "template" : "literal",
					confidence: urlArg.includes("${") ? 0.8 : 0.95,
					schemaRef: null,
					metadata: { httpMethod: method, rawUrl: urlArg.slice(0, 100) },
				});
			}
			continue;
		}

		// Pattern 2: axios({ method: "GET", url: ... })
		// Deferred — less common in glamCRM-style code.

		// Pattern 3: fetch(url) — method defaults to GET unless
		// second arg has { method: "POST" }.
		const fetchMatch = line.match(
			/fetch\s*\(\s*([`"'][^)]*)/,
		);
		if (fetchMatch) {
			const urlArg = fetchMatch[1];
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
				facts.push({
					mechanism: "http",
					operation: method ? `${method} ${path}` : path,
					address: path,
					callerStableKey: caller?.stableKey ?? `unknown:${filePath}:${i + 1}`,
					sourceFile: filePath,
					lineStart: i + 1,
					basis: urlArg.includes("${") ? "template" : "literal",
					confidence: urlArg.includes("${") ? 0.7 : 0.9,
					schemaRef: null,
					metadata: { httpMethod: method, rawUrl: urlArg.slice(0, 100) },
				});
			}
		}
	}

	return facts;
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
