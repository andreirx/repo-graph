/**
 * Express route fact extractor for TS/JS (PROTOTYPE).
 *
 * Scans TypeScript/JavaScript source files for Express route registrations
 * and emits normalized BoundaryProviderFact records.
 *
 * Supported patterns:
 *   - app.get("/api/v2/products", handler)
 *   - app.post("/api/v2/products", handler)
 *   - router.get("/api/v2/products/:id", handler)
 *   - app.put("/path", middleware, handler)
 *   - app.delete("/path", handler)
 *   - app.patch("/path", handler)
 *
 * Receiver provenance: only matches calls on a fixed set of identifiers
 * conventionally bound to Express application or router instances:
 * `app`, `router`, `server`. Does NOT derive receiver aliases from
 * imports or bindings — `const api = express.Router(); api.get(...)`
 * is NOT detected. Expanding the receiver set requires binding
 * analysis or import-following, which is out of scope for this
 * PROTOTYPE slice.
 *
 * Handler attribution: the enclosing symbol (nearest preceding function/
 * class) is recorded as the handler stable key.
 *
 * Consumes an optional StringBindingTable for resolving route path
 * constants (e.g. `const PREFIX = "/api/v2"; app.get(PREFIX + "/items", ...)`).
 *
 * Implementation: LINE-BASED REGEX scanning over raw source text.
 * NOT AST-backed. Known limitations:
 *   - Fragile on multi-line route registrations
 *   - Does not follow router composition across files
 *   - Does not handle app.route("/path").get().post()
 *   - Does not extract middleware-only registrations (app.use)
 *   - Does not track router mount prefixes (app.use("/api", router))
 *
 * Maturity: PROTOTYPE. Sufficient to extract fraktag's 47-route Express
 * API surface. Should be rebuilt on the TS parse tree for production.
 */

import type { BoundaryProviderFact } from "../../../core/classification/api-boundary.js";
import type { StringBindingTable } from "./file-local-string-resolver.js";

/** HTTP method type — adapter-specific. */
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD" | "ALL";

/** Express HTTP method names → canonical HTTP method. */
const EXPRESS_METHODS: Record<string, HttpMethod> = {
	get: "GET",
	post: "POST",
	put: "PUT",
	delete: "DELETE",
	patch: "PATCH",
	options: "OPTIONS",
	head: "HEAD",
	all: "ALL",
};

/**
 * Receiver names conventionally bound to Express app/router instances.
 * Only these receivers produce route facts. This prevents false positives
 * from cache.get(), map.get(), etc.
 */
const EXPRESS_RECEIVERS = new Set([
	"app", "router", "server",
]);

/**
 * Known base URL patterns to strip from extracted paths.
 * Same as in the HTTP client extractor.
 */
const BASE_URL_PATTERNS = [
	/\$\{[^}]*(?:API_URL|BASE_URL|SERVER_URL|BACKEND_URL|API_BASE)[^}]*\}/gi,
	/\$\{[^}]*(?:import\.meta\.env\.\w+)[^}]*\}/gi,
	/\$\{[^}]*(?:process\.env\.\w+)[^}]*\}/gi,
];

/**
 * Extract Express route provider facts from a TS/JS source file.
 *
 * @param source - Full source text of the file.
 * @param filePath - Repo-relative path.
 * @param repoUid - Repository UID.
 * @param symbols - Already-extracted symbols for handler attribution.
 * @param bindings - Optional file-local string bindings for constant resolution.
 */
export function extractExpressRoutes(
	source: string,
	filePath: string,
	_repoUid: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		lineStart: number | null;
	}>,
	bindings?: StringBindingTable,
): BoundaryProviderFact[] {
	const facts: BoundaryProviderFact[] = [];
	const lines = source.split("\n");

	// Check if this file imports express. If not, skip entirely.
	// This is a fast-path gate to avoid false positives on files
	// that happen to have app.get() but are not Express code.
	if (!hasExpressImport(source)) return facts;

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];

		// Pattern: receiver.METHOD(pathArg, ...)
		// Captures: [1] receiver, [2] method, [3] path argument
		const routeMatch = line.match(
			/(\w+)\.(get|post|put|delete|patch|options|head|all)\s*\(\s*([`"'][^,)]*|[A-Za-z_]\w*)/i,
		);
		if (!routeMatch) continue;

		const receiver = routeMatch[1];
		const methodName = routeMatch[2].toLowerCase();
		const rawPathArg = routeMatch[3];

		// Receiver provenance check.
		if (!EXPRESS_RECEIVERS.has(receiver)) continue;

		// Must be a known HTTP method.
		const httpMethod = EXPRESS_METHODS[methodName];
		if (!httpMethod) continue;

		// Resolve the path argument.
		const path = resolveRoutePath(rawPathArg, bindings);
		if (!path) continue;

		const handler = findEnclosingSymbol(i + 1, symbols);

		facts.push({
			mechanism: "http",
			operation: `${httpMethod} ${path}`,
			address: path,
			handlerStableKey: handler?.stableKey ?? `unknown:${filePath}:${i + 1}`,
			sourceFile: filePath,
			lineStart: i + 1,
			framework: "express",
			basis: "registration",
			schemaRef: null,
			metadata: {
				httpMethod,
				receiver,
				rawPath: rawPathArg.slice(0, 100),
			},
		});
	}

	return facts;
}

/**
 * Check if the source imports Express.
 * Looks for: import express, require('express'), import('express'),
 * or import from 'express'.
 */
function hasExpressImport(source: string): boolean {
	return (
		source.includes("'express'") ||
		source.includes('"express"') ||
		source.includes("from 'express'") ||
		source.includes('from "express"')
	);
}

/**
 * Resolve a route path argument to a normalized path string.
 *
 * Handles:
 *   - String literal: "/api/v2/products"
 *   - Template literal: `/api/v2/products/${id}`
 *   - Bare identifier: PREFIX (resolved via bindings)
 *   - Concatenation in the raw arg is NOT handled here
 *     (line-level regex captures the first argument only)
 *
 * Returns null if the path cannot be determined.
 */
function resolveRoutePath(
	rawArg: string,
	bindings?: StringBindingTable,
): string | null {
	let arg = rawArg.trim();

	// Bare identifier — resolve from binding table.
	if (/^[A-Za-z_]\w*$/.test(arg)) {
		if (!bindings) return null;
		const bound = bindings.get(arg);
		if (!bound || bound.confidence <= 0) return null;
		arg = `"${bound.value}"`;
	}

	// Remove surrounding quotes/backticks.
	if (arg.startsWith("`") || arg.startsWith("'") || arg.startsWith('"')) {
		arg = arg.slice(1);
	}
	// Remove trailing quote/comma/paren.
	arg = arg.replace(/[`'")\s,].*$/, "");

	// Strip env variable interpolations.
	for (const pattern of BASE_URL_PATTERNS) {
		arg = arg.replace(pattern, "");
	}

	// Resolve binding references in template literals.
	if (bindings && bindings.size > 0) {
		arg = arg.replace(/\$\{([A-Za-z_]\w*)\}/g, (match, name) => {
			const bound = bindings.get(name);
			if (bound && bound.confidence > 0) return bound.value;
			return match;
		});
	}

	// Normalize Express path parameters: :id → {id} for consistency
	// with the Spring convention used in matcher key normalization.
	arg = arg.replace(/:([A-Za-z_]\w*)/g, "{$1}");

	// Normalize remaining template interpolations to {param}.
	arg = arg.replace(/\$\{[^}]+\}/g, "{param}");

	// Must start with / to be a recognizable route path.
	if (!arg.startsWith("/")) return null;
	// Must have at least one path segment.
	if (arg === "/") return null;

	return arg;
}

function findEnclosingSymbol(
	lineNumber: number,
	symbols: Array<{ stableKey: string; name: string; lineStart: number | null }>,
): { stableKey: string } | null {
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
