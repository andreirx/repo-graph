/**
 * Framework-boundary post-pass — pure detection tests.
 */

import { describe, expect, it } from "vitest";
import { detectFrameworkBoundary } from "../../../src/core/classification/framework-boundary.js";
import { UnresolvedEdgeBasisCode, UnresolvedEdgeClassification } from "../../../src/core/diagnostics/unresolved-edge-classification.js";
import { UnresolvedEdgeCategory } from "../../../src/core/diagnostics/unresolved-edge-categories.js";
import type { ImportBinding } from "../../../src/core/ports/extractor.js";

function binding(identifier: string, specifier: string): ImportBinding {
	return { identifier, specifier, isRelative: specifier.startsWith("."), location: null, isTypeOnly: false };
}

const EXPRESS_IMPORT = [binding("express", "express")];
const NO_IMPORTS: ImportBinding[] = [];

describe("detectFrameworkBoundary — Express route registration", () => {
	it("app.get → framework_boundary_candidate / express_route_registration", () => {
		const result = detectFrameworkBoundary(
			"app.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		);
		expect(result).not.toBeNull();
		expect(result?.classification).toBe(UnresolvedEdgeClassification.FRAMEWORK_BOUNDARY_CANDIDATE);
		expect(result?.basisCode).toBe(UnresolvedEdgeBasisCode.EXPRESS_ROUTE_REGISTRATION);
	});

	it("router.post → framework_boundary_candidate / express_route_registration", () => {
		const result = detectFrameworkBoundary(
			"router.post",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		);
		expect(result?.basisCode).toBe(UnresolvedEdgeBasisCode.EXPRESS_ROUTE_REGISTRATION);
	});

	it("app.use → express_middleware_registration", () => {
		const result = detectFrameworkBoundary(
			"app.use",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		);
		expect(result?.basisCode).toBe(UnresolvedEdgeBasisCode.EXPRESS_MIDDLEWARE_REGISTRATION);
	});

	it("app.listen → express_route_registration (lifecycle)", () => {
		const result = detectFrameworkBoundary(
			"app.listen",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		);
		expect(result?.basisCode).toBe(UnresolvedEdgeBasisCode.EXPRESS_ROUTE_REGISTRATION);
	});

	for (const method of ["get", "post", "put", "delete", "patch", "options", "head", "all", "route"]) {
		it(`app.${method} → detected as route registration`, () => {
			const result = detectFrameworkBoundary(
				`app.${method}`,
				UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
				EXPRESS_IMPORT,
			);
			expect(result).not.toBeNull();
		});
	}
});

describe("detectFrameworkBoundary — non-matches", () => {
	it("returns null when no express import exists", () => {
		expect(detectFrameworkBoundary(
			"app.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			NO_IMPORTS,
		)).toBeNull();
	});

	it("returns null for non-route method on express-importing file", () => {
		expect(detectFrameworkBoundary(
			"app.render",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		)).toBeNull();
	});

	it("returns null for .get() on non-Express receiver in express-importing file (P1 regression)", () => {
		// cache.get(), map.get(), response.get() must NOT be reclassified
		// just because the file imports express.
		expect(detectFrameworkBoundary(
			"cache.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		)).toBeNull();
		expect(detectFrameworkBoundary(
			"map.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		)).toBeNull();
		expect(detectFrameworkBoundary(
			"response.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		)).toBeNull();
		expect(detectFrameworkBoundary(
			"db.use",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		)).toBeNull();
	});

	it("returns null for CALLS_FUNCTION category (not obj.method)", () => {
		expect(detectFrameworkBoundary(
			"express",
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			EXPRESS_IMPORT,
		)).toBeNull();
	});

	it("returns null for non-CALLS category", () => {
		expect(detectFrameworkBoundary(
			"app.get",
			UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			EXPRESS_IMPORT,
		)).toBeNull();
	});

	it("returns null for file importing react but not express", () => {
		expect(detectFrameworkBoundary(
			"app.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			[binding("React", "react")],
		)).toBeNull();
	});
});

describe("detectFrameworkBoundary — edge cases", () => {
	it("handles chained call: app.route('/x').get — detects first method", () => {
		const result = detectFrameworkBoundary(
			"app.route.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			EXPRESS_IMPORT,
		);
		// First method is "route" which is a registration method.
		expect(result?.basisCode).toBe(UnresolvedEdgeBasisCode.EXPRESS_ROUTE_REGISTRATION);
	});

	it("handles @types/express import", () => {
		const result = detectFrameworkBoundary(
			"app.get",
			UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
			[binding("express", "@types/express")],
		);
		expect(result).not.toBeNull();
	});
});
