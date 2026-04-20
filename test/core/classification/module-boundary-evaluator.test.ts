/**
 * Unit tests for module boundary evaluator.
 *
 * Tests the pure policy core for discovered-module boundary evaluation:
 *   - Violation detection (forbidden edge exists)
 *   - Stale declaration detection (missing modules)
 *   - Mutual exclusion (stale declarations skip violation check)
 *   - Deterministic output ordering
 */

import { describe, expect, it } from "vitest";
import {
	type EvaluatableDiscoveredModuleBoundary,
	type EvaluatableModuleDependencyEdge,
	evaluateModuleBoundaries,
} from "../../../src/core/classification/module-boundary-evaluator.js";

// ── Test helpers ───────────────────────────────────────────────────

function makeBoundary(
	uid: string,
	source: string,
	target: string,
	reason?: string,
): EvaluatableDiscoveredModuleBoundary {
	return {
		declarationUid: uid,
		sourceCanonicalPath: source,
		targetCanonicalPath: target,
		reason,
	};
}

function makeEdge(
	source: string,
	target: string,
	importCount = 1,
	sourceFileCount = 1,
): EvaluatableModuleDependencyEdge {
	return {
		sourceCanonicalPath: source,
		targetCanonicalPath: target,
		importCount,
		sourceFileCount,
	};
}

function makeModuleIndex(paths: string[]): Map<string, string> {
	const index = new Map<string, string>();
	for (const path of paths) {
		index.set(path, `uid-${path.replace(/\//g, "-")}`);
	}
	return index;
}

// ── Violation detection ────────────────────────────────────────────

describe("evaluateModuleBoundaries — violations", () => {
	it("detects violation when forbidden edge exists", () => {
		const boundaries = [makeBoundary("decl-1", "packages/app", "packages/db")];
		const edges = [makeEdge("packages/app", "packages/db", 3, 2)];
		const moduleIndex = makeModuleIndex(["packages/app", "packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(1);
		expect(result.staleDeclarations).toHaveLength(0);
		expect(result.violations[0]).toEqual({
			declarationUid: "decl-1",
			sourceCanonicalPath: "packages/app",
			targetCanonicalPath: "packages/db",
			importCount: 3,
			sourceFileCount: 2,
			reason: undefined,
		});
	});

	it("includes reason in violation if present", () => {
		const boundaries = [
			makeBoundary("decl-1", "packages/app", "packages/db", "UI must not access DB"),
		];
		const edges = [makeEdge("packages/app", "packages/db")];
		const moduleIndex = makeModuleIndex(["packages/app", "packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations[0].reason).toBe("UI must not access DB");
	});

	it("reports no violation when forbidden edge does not exist", () => {
		const boundaries = [makeBoundary("decl-1", "packages/app", "packages/db")];
		const edges = [makeEdge("packages/app", "packages/core")]; // different target
		const moduleIndex = makeModuleIndex([
			"packages/app",
			"packages/db",
			"packages/core",
		]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(0);
		expect(result.staleDeclarations).toHaveLength(0);
	});

	it("handles multiple boundaries with mixed violations", () => {
		const boundaries = [
			makeBoundary("decl-1", "packages/app", "packages/db"),
			makeBoundary("decl-2", "packages/app", "packages/core"),
			makeBoundary("decl-3", "packages/api", "packages/db"),
		];
		const edges = [
			makeEdge("packages/app", "packages/db"), // violates decl-1
			// no edge from app to core — decl-2 is not violated
			makeEdge("packages/api", "packages/db"), // violates decl-3
		];
		const moduleIndex = makeModuleIndex([
			"packages/app",
			"packages/db",
			"packages/core",
			"packages/api",
		]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(2);
		expect(result.violations.map((v) => v.declarationUid).sort()).toEqual([
			"decl-1",
			"decl-3",
		]);
	});
});

// ── Stale declaration detection ────────────────────────────────────

describe("evaluateModuleBoundaries — stale declarations", () => {
	it("detects stale source module", () => {
		const boundaries = [makeBoundary("decl-1", "packages/removed", "packages/db")];
		const edges: EvaluatableModuleDependencyEdge[] = [];
		const moduleIndex = makeModuleIndex(["packages/db"]); // source missing

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.staleDeclarations).toHaveLength(1);
		expect(result.violations).toHaveLength(0);
		expect(result.staleDeclarations[0]).toEqual({
			declarationUid: "decl-1",
			staleSide: "source",
			missingPaths: ["packages/removed"],
		});
	});

	it("detects stale target module", () => {
		const boundaries = [makeBoundary("decl-1", "packages/app", "packages/removed")];
		const edges: EvaluatableModuleDependencyEdge[] = [];
		const moduleIndex = makeModuleIndex(["packages/app"]); // target missing

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.staleDeclarations).toHaveLength(1);
		expect(result.staleDeclarations[0]).toEqual({
			declarationUid: "decl-1",
			staleSide: "target",
			missingPaths: ["packages/removed"],
		});
	});

	it("detects both modules missing", () => {
		const boundaries = [
			makeBoundary("decl-1", "packages/old-app", "packages/old-db"),
		];
		const edges: EvaluatableModuleDependencyEdge[] = [];
		const moduleIndex = makeModuleIndex(["packages/current"]); // both missing

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.staleDeclarations).toHaveLength(1);
		expect(result.staleDeclarations[0]).toEqual({
			declarationUid: "decl-1",
			staleSide: "both",
			missingPaths: ["packages/old-app", "packages/old-db"],
		});
	});
});

// ── Mutual exclusion ───────────────────────────────────────────────

describe("evaluateModuleBoundaries — mutual exclusion", () => {
	it("stale declaration is not also reported as violation", () => {
		// Even if there's an edge matching the boundary pattern,
		// a stale declaration should NOT produce a violation.
		const boundaries = [makeBoundary("decl-1", "packages/removed", "packages/db")];
		// This edge would match the boundary if evaluated...
		const edges = [makeEdge("packages/removed", "packages/db")];
		// ...but source module is missing
		const moduleIndex = makeModuleIndex(["packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.staleDeclarations).toHaveLength(1);
		expect(result.violations).toHaveLength(0); // NOT a violation
	});

	it("handles mix of stale and live boundaries correctly", () => {
		const boundaries = [
			makeBoundary("decl-1", "packages/app", "packages/db"), // live, violated
			makeBoundary("decl-2", "packages/removed", "packages/db"), // stale source
			makeBoundary("decl-3", "packages/app", "packages/core"), // live, not violated
		];
		const edges = [
			makeEdge("packages/app", "packages/db"),
			makeEdge("packages/removed", "packages/db"), // edge exists but boundary is stale
		];
		const moduleIndex = makeModuleIndex([
			"packages/app",
			"packages/db",
			"packages/core",
		]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(1);
		expect(result.violations[0].declarationUid).toBe("decl-1");

		expect(result.staleDeclarations).toHaveLength(1);
		expect(result.staleDeclarations[0].declarationUid).toBe("decl-2");
	});
});

// ── Deterministic ordering ─────────────────────────────────────────

describe("evaluateModuleBoundaries — deterministic ordering", () => {
	it("sorts staleDeclarations by declarationUid", () => {
		const boundaries = [
			makeBoundary("decl-z", "packages/z", "packages/db"),
			makeBoundary("decl-a", "packages/a", "packages/db"),
			makeBoundary("decl-m", "packages/m", "packages/db"),
		];
		const edges: EvaluatableModuleDependencyEdge[] = [];
		const moduleIndex = makeModuleIndex(["packages/db"]); // all sources missing

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.staleDeclarations.map((s) => s.declarationUid)).toEqual([
			"decl-a",
			"decl-m",
			"decl-z",
		]);
	});

	it("sorts violations by (source, target, declarationUid)", () => {
		const boundaries = [
			makeBoundary("decl-3", "packages/b", "packages/x"),
			makeBoundary("decl-1", "packages/a", "packages/y"),
			makeBoundary("decl-2", "packages/a", "packages/x"),
			makeBoundary("decl-4", "packages/a", "packages/x"), // same path, different uid
		];
		const edges = [
			makeEdge("packages/a", "packages/x"),
			makeEdge("packages/a", "packages/y"),
			makeEdge("packages/b", "packages/x"),
		];
		const moduleIndex = makeModuleIndex([
			"packages/a",
			"packages/b",
			"packages/x",
			"packages/y",
		]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		// Expected order:
		// 1. packages/a → packages/x, decl-2
		// 2. packages/a → packages/x, decl-4
		// 3. packages/a → packages/y, decl-1
		// 4. packages/b → packages/x, decl-3
		expect(result.violations.map((v) => v.declarationUid)).toEqual([
			"decl-2",
			"decl-4",
			"decl-1",
			"decl-3",
		]);
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("evaluateModuleBoundaries — edge cases", () => {
	it("handles empty input", () => {
		const result = evaluateModuleBoundaries({
			boundaries: [],
			edges: [],
			moduleIndex: new Map(),
		});

		expect(result.violations).toHaveLength(0);
		expect(result.staleDeclarations).toHaveLength(0);
	});

	it("handles boundaries with no edges", () => {
		const boundaries = [makeBoundary("decl-1", "packages/app", "packages/db")];
		const edges: EvaluatableModuleDependencyEdge[] = [];
		const moduleIndex = makeModuleIndex(["packages/app", "packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(0);
		expect(result.staleDeclarations).toHaveLength(0);
	});

	it("handles edges with no boundaries", () => {
		const boundaries: EvaluatableDiscoveredModuleBoundary[] = [];
		const edges = [makeEdge("packages/app", "packages/db")];
		const moduleIndex = makeModuleIndex(["packages/app", "packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(0);
		expect(result.staleDeclarations).toHaveLength(0);
	});

	it("does not match reverse edge direction", () => {
		// Boundary: app must not depend on db
		// Edge: db depends on app (reverse)
		// Should NOT be a violation
		const boundaries = [makeBoundary("decl-1", "packages/app", "packages/db")];
		const edges = [makeEdge("packages/db", "packages/app")]; // reverse direction
		const moduleIndex = makeModuleIndex(["packages/app", "packages/db"]);

		const result = evaluateModuleBoundaries({ boundaries, edges, moduleIndex });

		expect(result.violations).toHaveLength(0);
	});
});
