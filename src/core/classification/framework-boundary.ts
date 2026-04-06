/**
 * Framework-boundary post-classification pass.
 *
 * Runs AFTER the generic classifier and may RECLASSIFY selected
 * unresolved edges from their generic bucket (typically `unknown`)
 * to `framework_boundary_candidate` when the edge matches a known
 * runtime-wiring / registration pattern.
 *
 * Separation from the generic classifier is intentional:
 *   - generic classifier: external / internal / unknown
 *   - framework pass: "this matches a known registration pattern"
 *
 * This keeps generic semantics and framework semantics as two
 * independent layers. Future plugin extraction will follow this
 * same boundary.
 *
 * Narrow semantic contract: only true runtime wiring / registration.
 * NOT ordinary framework API usage (React hooks, SDK calls, etc.).
 *
 * First-slice detectors:
 *   - Express route/middleware registration (app.get, router.post, app.use)
 */

import type { ImportBinding } from "../ports/extractor.js";
import {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../diagnostics/unresolved-edge-classification.js";
import { UnresolvedEdgeCategory } from "../diagnostics/unresolved-edge-categories.js";

// ── Express route/middleware detection ───────────────────────────────

/**
 * Express HTTP method names that constitute route registration.
 * app.get("/path", handler) is a route registration, not a data query.
 */
const EXPRESS_ROUTE_METHODS = new Set([
	"get", "post", "put", "delete", "patch",
	"options", "head", "all", "route",
]);

/** Express middleware registration method. */
const EXPRESS_MIDDLEWARE_METHODS = new Set(["use"]);

/** app.listen is also framework boundary (server startup). */
const EXPRESS_LIFECYCLE_METHODS = new Set(["listen"]);

/**
 * Specifiers that indicate an Express import.
 */
const EXPRESS_SPECIFIERS = new Set([
	"express",
	"@types/express",
]);

/**
 * Conventional receiver variable names for Express app/router instances.
 *
 * The detector requires BOTH an express import AND a conventional
 * receiver name. This prevents misclassification of arbitrary
 * `.get()/.use()` calls on non-Express objects (e.g. `cache.get()`,
 * `map.get()`) in files that happen to import express.
 *
 * Curation: conservative. Missing a non-conventional name (e.g.
 * `const api = express()`) produces a false NEGATIVE (stays unknown),
 * not a false positive. That is the safer failure mode.
 */
const EXPRESS_RECEIVER_NAMES = new Set([
	"app",
	"router",
	"server",
]);

/**
 * Check whether a file has an Express import binding.
 */
function fileHasExpressImport(importBindings: readonly ImportBinding[]): boolean {
	for (const binding of importBindings) {
		if (EXPRESS_SPECIFIERS.has(binding.specifier)) return true;
	}
	return false;
}

// ── Public interface ────────────────────────────────────────────────

export interface FrameworkReclassification {
	classification: typeof UnresolvedEdgeClassification.FRAMEWORK_BOUNDARY_CANDIDATE;
	basisCode: UnresolvedEdgeBasisCode;
}

/**
 * Attempt to reclassify a single unresolved edge as a framework
 * boundary observation. Returns a reclassification verdict if the
 * edge matches a known pattern, or null if no pattern applies.
 *
 * Pure function. Called per-edge in the post-classification pass.
 *
 * @param targetKey - The edge's target key (e.g. "app.get").
 * @param category - The extraction failure category.
 * @param importBindings - All import bindings in the source file.
 */
export function detectFrameworkBoundary(
	targetKey: string,
	category: string,
	importBindings: readonly ImportBinding[],
): FrameworkReclassification | null {
	// Only CALLS-family categories with a receiver.method() shape.
	if (category !== UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO) {
		return null;
	}

	// Extract receiver and method from targetKey.
	const dotIdx = targetKey.indexOf(".");
	if (dotIdx <= 0) return null;
	const receiver = targetKey.slice(0, dotIdx);
	const method = targetKey.slice(dotIdx + 1);
	// For chained calls like app.route("/x").get(...), take the
	// first method segment only.
	const firstMethod = method.includes(".") ? method.split(".")[0] : method;

	// Express detection requires THREE signals:
	//   (a) the file imports from "express"
	//   (b) the receiver name is a conventional Express app/router name
	//   (c) the method name is a known registration method
	//
	// The receiver check (b) prevents misclassification of arbitrary
	// .get()/.use() calls on non-Express objects (cache.get(),
	// map.get(), etc.) in files that happen to import express.
	if (
		fileHasExpressImport(importBindings) &&
		EXPRESS_RECEIVER_NAMES.has(receiver)
	) {
		if (EXPRESS_MIDDLEWARE_METHODS.has(firstMethod)) {
			return {
				classification: UnresolvedEdgeClassification.FRAMEWORK_BOUNDARY_CANDIDATE,
				basisCode: UnresolvedEdgeBasisCode.EXPRESS_MIDDLEWARE_REGISTRATION,
			};
		}
		if (EXPRESS_ROUTE_METHODS.has(firstMethod) || EXPRESS_LIFECYCLE_METHODS.has(firstMethod)) {
			return {
				classification: UnresolvedEdgeClassification.FRAMEWORK_BOUNDARY_CANDIDATE,
				basisCode: UnresolvedEdgeBasisCode.EXPRESS_ROUTE_REGISTRATION,
			};
		}
	}

	return null;
}
