/**
 * Env linkage orchestrator unit tests.
 *
 * Pure function — verifies linkage policy without indexer/storage.
 * Covers:
 *   - file → single surface mapping
 *   - file → multiple surfaces mapping (one detection → multiple deps)
 *   - dedup per surface/env_name
 *   - aggregate access kind (required > optional > unknown)
 *   - default value preservation
 *   - unlinked file dropping
 */

import { describe, expect, it } from "vitest";
import { linkEnvDependencies } from "../../../src/core/seams/env-linkage.js";
import type { DetectedEnvDependency } from "../../../src/core/seams/env-dependency.js";

function makeAccess(overrides: Partial<DetectedEnvDependency> & { varName: string; filePath: string }): DetectedEnvDependency {
	return {
		accessKind: "required",
		accessPattern: "process_env_dot",
		lineNumber: 1,
		defaultValue: null,
		confidence: 0.95,
		...overrides,
	};
}

describe("linkEnvDependencies", () => {
	it("links a single access to a single surface", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "PORT", filePath: "src/server.ts" }),
			],
			fileToSurfaces: new Map([["src/server.ts", ["surface-a"]]]),
		});

		expect(result.dependencies).toHaveLength(1);
		expect(result.dependencies[0].envName).toBe("PORT");
		expect(result.dependencies[0].projectSurfaceUid).toBe("surface-a");
		expect(result.evidence).toHaveLength(1);
		expect(result.unlinkedDropped).toBe(0);
	});

	it("links one access to multiple surfaces when file is shared", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "DB_URL", filePath: "src/shared.ts" }),
			],
			fileToSurfaces: new Map([["src/shared.ts", ["surface-a", "surface-b"]]]),
		});

		// One access, two surfaces → two dependency rows.
		expect(result.dependencies).toHaveLength(2);
		const surfaces = result.dependencies.map((d) => d.projectSurfaceUid).sort();
		expect(surfaces).toEqual(["surface-a", "surface-b"]);

		// Each dependency has its own evidence row.
		expect(result.evidence).toHaveLength(2);
	});

	it("dedups same env var across files within same surface", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "DB_URL", filePath: "src/a.ts", lineNumber: 5 }),
				makeAccess({ varName: "DB_URL", filePath: "src/b.ts", lineNumber: 10 }),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		// Two files, one surface → one dependency, two evidence.
		expect(result.dependencies).toHaveLength(1);
		expect(result.evidence).toHaveLength(2);
	});

	it("aggregates access kind: required wins over optional", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "X", filePath: "src/a.ts", accessKind: "optional" }),
				makeAccess({ varName: "X", filePath: "src/b.ts", accessKind: "required" }),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		expect(result.dependencies[0].accessKind).toBe("required");
	});

	it("aggregates access kind: optional wins over unknown", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "X", filePath: "src/a.ts", accessKind: "unknown" }),
				makeAccess({ varName: "X", filePath: "src/b.ts", accessKind: "optional" }),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		expect(result.dependencies[0].accessKind).toBe("optional");
	});

	it("preserves first non-null default value", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "X", filePath: "src/a.ts", defaultValue: null }),
				makeAccess({ varName: "X", filePath: "src/b.ts", defaultValue: "fallback" }),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		expect(result.dependencies[0].defaultValue).toBe("fallback");
	});

	it("drops accesses from files with no owning surface", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "ORPHAN", filePath: "src/orphan.ts" }),
			],
			fileToSurfaces: new Map(),
		});

		expect(result.dependencies).toHaveLength(0);
		expect(result.unlinkedDropped).toBe(1);
	});

	it("uses max confidence across occurrences", () => {
		const result = linkEnvDependencies({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedAccesses: [
				makeAccess({ varName: "X", filePath: "src/a.ts", confidence: 0.7 }),
				makeAccess({ varName: "X", filePath: "src/b.ts", confidence: 0.95 }),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		expect(result.dependencies[0].confidence).toBe(0.95);
	});
});
