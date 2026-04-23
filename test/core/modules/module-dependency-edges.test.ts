/**
 * Unit tests for module dependency edge derivation.
 *
 * Pure logic — no filesystem, no storage. Tests cover:
 *   - cross-module edge derivation from IMPORTS
 *   - intra-module exclusion
 *   - edge aggregation (importCount, sourceFileCount)
 *   - missing file/module handling
 *   - deterministic output ordering
 *
 * See docs/architecture/module-graph-contract.txt for the specification.
 */

import { describe, expect, it } from "vitest";
import {
	deriveModuleDependencyEdges,
	type FileOwnershipMapping,
	type ImportEdgeInput,
	type NodeFileMapping,
} from "../../../src/core/modules/module-dependency-edges.js";

// ── Test helpers ───────────────────────────────────────────────────

function makeEdge(
	sourceNodeUid: string,
	targetNodeUid: string,
): ImportEdgeInput {
	return { sourceNodeUid, targetNodeUid };
}

function makeNodeFile(
	nodeUid: string,
	fileUid: string | null,
): NodeFileMapping {
	return { nodeUid, fileUid };
}

function makeOwnership(
	fileUid: string,
	moduleCandidateUid: string,
): FileOwnershipMapping {
	return { fileUid, moduleCandidateUid };
}

// ── Basic cross-module derivation ──────────────────────────────────

describe("deriveModuleDependencyEdges — basic", () => {
	it("derives edge when source and target are in different modules", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-b")],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				makeOwnership("file-b", "module-core"),
			],
		});

		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].sourceModuleUid).toBe("module-api");
		expect(result.edges[0].targetModuleUid).toBe("module-core");
		expect(result.edges[0].importCount).toBe(1);
		expect(result.edges[0].sourceFileCount).toBe(1);
		expect(result.diagnostics.importsCrossModule).toBe(1);
	});

	it("excludes intra-module imports", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-b")],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-same"),
				makeOwnership("file-b", "module-same"),
			],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsIntraModule).toBe(1);
		expect(result.diagnostics.importsCrossModule).toBe(0);
	});

	it("returns empty edges when no imports exist", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [],
			nodeFiles: [],
			fileOwnership: [],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsEdgesTotal).toBe(0);
	});
});

// ── Edge aggregation ───────────────────────────────────────────────

describe("deriveModuleDependencyEdges — aggregation", () => {
	it("aggregates multiple imports between same module pair", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				makeEdge("node-a1", "node-b1"),
				makeEdge("node-a2", "node-b2"),
				makeEdge("node-a1", "node-b2"), // same source file, different target
			],
			nodeFiles: [
				makeNodeFile("node-a1", "file-a1"),
				makeNodeFile("node-a2", "file-a2"),
				makeNodeFile("node-b1", "file-b1"),
				makeNodeFile("node-b2", "file-b2"),
			],
			fileOwnership: [
				makeOwnership("file-a1", "module-api"),
				makeOwnership("file-a2", "module-api"),
				makeOwnership("file-b1", "module-core"),
				makeOwnership("file-b2", "module-core"),
			],
		});

		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].importCount).toBe(3);
		expect(result.edges[0].sourceFileCount).toBe(2); // file-a1, file-a2
	});

	it("keeps separate edges for different module pairs", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				makeEdge("node-a", "node-b"),
				makeEdge("node-a", "node-c"),
			],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-b"),
				makeNodeFile("node-c", "file-c"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				makeOwnership("file-b", "module-core"),
				makeOwnership("file-c", "module-utils"),
			],
		});

		expect(result.edges).toHaveLength(2);
		const targets = result.edges.map((e) => e.targetModuleUid).sort();
		expect(targets).toEqual(["module-core", "module-utils"]);
	});

	it("sorts edges by importCount descending", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				makeEdge("node-a", "node-b"),
				makeEdge("node-a", "node-c"),
				makeEdge("node-a2", "node-c"),
				makeEdge("node-a3", "node-c"),
			],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-a2", "file-a2"),
				makeNodeFile("node-a3", "file-a3"),
				makeNodeFile("node-b", "file-b"),
				makeNodeFile("node-c", "file-c"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				makeOwnership("file-a2", "module-api"),
				makeOwnership("file-a3", "module-api"),
				makeOwnership("file-b", "module-core"),
				makeOwnership("file-c", "module-utils"),
			],
		});

		expect(result.edges).toHaveLength(2);
		// module-api -> module-utils has 3 imports, should be first
		expect(result.edges[0].targetModuleUid).toBe("module-utils");
		expect(result.edges[0].importCount).toBe(3);
		// module-api -> module-core has 1 import, should be second
		expect(result.edges[1].targetModuleUid).toBe("module-core");
		expect(result.edges[1].importCount).toBe(1);
	});
});

// ── Missing data handling ──────────────────────────────────────────

describe("deriveModuleDependencyEdges — missing data", () => {
	it("tracks source node with no file", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-orphan", "node-b")],
			nodeFiles: [
				// node-orphan not in nodeFiles
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [makeOwnership("file-b", "module-core")],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsSourceNoFile).toBe(1);
	});

	it("tracks target node with no file", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-orphan")],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				// node-orphan not in nodeFiles
			],
			fileOwnership: [makeOwnership("file-a", "module-api")],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsTargetNoFile).toBe(1);
	});

	it("tracks source file with no module owner", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-b")],
			nodeFiles: [
				makeNodeFile("node-a", "file-unowned"),
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [
				// file-unowned not in ownership
				makeOwnership("file-b", "module-core"),
			],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsSourceNoModule).toBe(1);
	});

	it("tracks target file with no module owner", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-b")],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-unowned"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				// file-unowned not in ownership
			],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsTargetNoModule).toBe(1);
	});

	it("handles node with null fileUid", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [makeEdge("node-a", "node-b")],
			nodeFiles: [
				makeNodeFile("node-a", null), // explicit null
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [makeOwnership("file-b", "module-core")],
		});

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsSourceNoFile).toBe(1);
	});
});

// ── Diagnostics completeness ───────────────────────────────────────

describe("deriveModuleDependencyEdges — diagnostics", () => {
	it("reports complete diagnostics for mixed scenario", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				makeEdge("node-a", "node-b"), // cross-module
				makeEdge("node-c", "node-d"), // intra-module
				makeEdge("node-orphan", "node-b"), // source no file
				makeEdge("node-a", "node-orphan"), // target no file
				makeEdge("node-unowned", "node-b"), // source no module
			],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-b"),
				makeNodeFile("node-c", "file-c"),
				makeNodeFile("node-d", "file-d"),
				makeNodeFile("node-unowned", "file-unowned"),
				// node-orphan not mapped
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				makeOwnership("file-b", "module-core"),
				makeOwnership("file-c", "module-same"),
				makeOwnership("file-d", "module-same"),
				// file-unowned not owned
			],
		});

		expect(result.diagnostics.importsEdgesTotal).toBe(5);
		expect(result.diagnostics.importsCrossModule).toBe(1);
		expect(result.diagnostics.importsIntraModule).toBe(1);
		expect(result.diagnostics.importsSourceNoFile).toBe(1);
		expect(result.diagnostics.importsTargetNoFile).toBe(1);
		expect(result.diagnostics.importsSourceNoModule).toBe(1);
		expect(result.diagnostics.importsTargetNoModule).toBe(0);
	});
});

// ── Bidirectional edges ────────────────────────────────────────────

describe("deriveModuleDependencyEdges — bidirectional", () => {
	it("creates separate edges for A->B and B->A", () => {
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				makeEdge("node-a", "node-b"), // api imports core
				makeEdge("node-b", "node-a"), // core imports api
			],
			nodeFiles: [
				makeNodeFile("node-a", "file-a"),
				makeNodeFile("node-b", "file-b"),
			],
			fileOwnership: [
				makeOwnership("file-a", "module-api"),
				makeOwnership("file-b", "module-core"),
			],
		});

		expect(result.edges).toHaveLength(2);

		const apiToCore = result.edges.find(
			(e) =>
				e.sourceModuleUid === "module-api" &&
				e.targetModuleUid === "module-core",
		);
		const coreToApi = result.edges.find(
			(e) =>
				e.sourceModuleUid === "module-core" &&
				e.targetModuleUid === "module-api",
		);

		expect(apiToCore).toBeDefined();
		expect(coreToApi).toBeDefined();
	});
});

// ── Deterministic ordering ─────────────────────────────────────────

describe("deriveModuleDependencyEdges — deterministic ordering", () => {
	it("sorts by importCount DESC, then sourceModuleUid ASC, then targetModuleUid ASC", () => {
		// Create edges with equal importCount to test tie-breaking.
		// Input order is deliberately non-sorted to verify sorting.
		const result = deriveModuleDependencyEdges({
			importsEdges: [
				// module-z -> module-a (1 import)
				makeEdge("node-z1", "node-a1"),
				// module-a -> module-z (1 import)
				makeEdge("node-a2", "node-z2"),
				// module-m -> module-b (1 import)
				makeEdge("node-m1", "node-b1"),
				// module-a -> module-b (2 imports - should be first due to count)
				makeEdge("node-a3", "node-b2"),
				makeEdge("node-a4", "node-b3"),
			],
			nodeFiles: [
				makeNodeFile("node-z1", "file-z1"),
				makeNodeFile("node-a1", "file-a1"),
				makeNodeFile("node-a2", "file-a2"),
				makeNodeFile("node-z2", "file-z2"),
				makeNodeFile("node-m1", "file-m1"),
				makeNodeFile("node-b1", "file-b1"),
				makeNodeFile("node-a3", "file-a3"),
				makeNodeFile("node-b2", "file-b2"),
				makeNodeFile("node-a4", "file-a4"),
				makeNodeFile("node-b3", "file-b3"),
			],
			fileOwnership: [
				makeOwnership("file-z1", "module-z"),
				makeOwnership("file-a1", "module-a"),
				makeOwnership("file-a2", "module-a"),
				makeOwnership("file-z2", "module-z"),
				makeOwnership("file-m1", "module-m"),
				makeOwnership("file-b1", "module-b"),
				makeOwnership("file-a3", "module-a"),
				makeOwnership("file-b2", "module-b"),
				makeOwnership("file-a4", "module-a"),
				makeOwnership("file-b3", "module-b"),
			],
		});

		expect(result.edges).toHaveLength(4);

		// Expected order:
		// 1. module-a -> module-b (importCount=2)
		// 2. module-a -> module-z (importCount=1, source "a" < "m" < "z")
		// 3. module-m -> module-b (importCount=1, source "m")
		// 4. module-z -> module-a (importCount=1, source "z")

		expect(result.edges[0].sourceModuleUid).toBe("module-a");
		expect(result.edges[0].targetModuleUid).toBe("module-b");
		expect(result.edges[0].importCount).toBe(2);

		expect(result.edges[1].sourceModuleUid).toBe("module-a");
		expect(result.edges[1].targetModuleUid).toBe("module-z");
		expect(result.edges[1].importCount).toBe(1);

		expect(result.edges[2].sourceModuleUid).toBe("module-m");
		expect(result.edges[2].targetModuleUid).toBe("module-b");
		expect(result.edges[2].importCount).toBe(1);

		expect(result.edges[3].sourceModuleUid).toBe("module-z");
		expect(result.edges[3].targetModuleUid).toBe("module-a");
		expect(result.edges[3].importCount).toBe(1);
	});

	it("is stable when inputs arrive in different order", () => {
		// Same logical edges, different input order
		const inputA = {
			importsEdges: [
				makeEdge("n1", "n2"),
				makeEdge("n3", "n4"),
			],
			nodeFiles: [
				makeNodeFile("n1", "f1"),
				makeNodeFile("n2", "f2"),
				makeNodeFile("n3", "f3"),
				makeNodeFile("n4", "f4"),
			],
			fileOwnership: [
				makeOwnership("f1", "mod-b"),
				makeOwnership("f2", "mod-a"),
				makeOwnership("f3", "mod-c"),
				makeOwnership("f4", "mod-a"),
			],
		};

		const inputB = {
			importsEdges: [
				makeEdge("n3", "n4"), // swapped order
				makeEdge("n1", "n2"),
			],
			nodeFiles: [
				makeNodeFile("n3", "f3"), // swapped order
				makeNodeFile("n4", "f4"),
				makeNodeFile("n1", "f1"),
				makeNodeFile("n2", "f2"),
			],
			fileOwnership: [
				makeOwnership("f3", "mod-c"), // swapped order
				makeOwnership("f4", "mod-a"),
				makeOwnership("f1", "mod-b"),
				makeOwnership("f2", "mod-a"),
			],
		};

		const resultA = deriveModuleDependencyEdges(inputA);
		const resultB = deriveModuleDependencyEdges(inputB);

		// Both should produce identical output
		expect(resultA.edges).toHaveLength(2);
		expect(resultB.edges).toHaveLength(2);

		// Same order: mod-b->mod-a, then mod-c->mod-a (alphabetical source)
		expect(resultA.edges[0].sourceModuleUid).toBe(resultB.edges[0].sourceModuleUid);
		expect(resultA.edges[0].targetModuleUid).toBe(resultB.edges[0].targetModuleUid);
		expect(resultA.edges[1].sourceModuleUid).toBe(resultB.edges[1].sourceModuleUid);
		expect(resultA.edges[1].targetModuleUid).toBe(resultB.edges[1].targetModuleUid);
	});
});
