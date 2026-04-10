/**
 * Pure module seam rollup tests.
 *
 * Verifies aggregation policy in isolation — no storage, no indexer.
 * Covers:
 *   - env access kind precedence (required > optional > unknown)
 *   - env first-non-null default rule
 *   - env hasConflictingDefaults flag
 *   - env surfaceCount + evidenceCount aggregation
 *   - fs union by (target_path, mutation_kind)
 *   - fs destinationPaths union across surfaces
 *   - fs dynamic evidence summary (count, distinct files, by-kind)
 *   - sort order (D6)
 */

import { describe, expect, it } from "vitest";
import type {
	EnvAccessKind,
	SurfaceEnvDependency,
} from "../../../src/core/seams/env-dependency.js";
import type {
	MutationKind,
	SurfaceFsMutation,
	SurfaceFsMutationEvidence,
} from "../../../src/core/seams/fs-mutation.js";
import {
	aggregateEnvAccessKind,
	aggregateEnvAcrossSurfaces,
	aggregateFsAcrossSurfaces,
	summarizeFsDynamicEvidence,
} from "../../../src/core/seams/module-seam-rollup.js";

// ── helpers ────────────────────────────────────────────────────────

function makeEnvDep(overrides: Partial<SurfaceEnvDependency> & {
	envName: string;
	projectSurfaceUid: string;
}): SurfaceEnvDependency {
	return {
		surfaceEnvDependencyUid: `dep-${overrides.projectSurfaceUid}-${overrides.envName}`,
		snapshotUid: "snap-1",
		repoUid: "repo-1",
		projectSurfaceUid: overrides.projectSurfaceUid,
		envName: overrides.envName,
		accessKind: "unknown",
		defaultValue: null,
		confidence: 0.9,
		metadataJson: null,
		...overrides,
	};
}

function makeFsMutation(overrides: Partial<SurfaceFsMutation> & {
	targetPath: string;
	mutationKind: MutationKind;
	projectSurfaceUid: string;
}): SurfaceFsMutation {
	return {
		surfaceFsMutationUid: `fs-${overrides.projectSurfaceUid}-${overrides.targetPath}-${overrides.mutationKind}`,
		snapshotUid: "snap-1",
		repoUid: "repo-1",
		projectSurfaceUid: overrides.projectSurfaceUid,
		targetPath: overrides.targetPath,
		mutationKind: overrides.mutationKind,
		confidence: 0.9,
		metadataJson: null,
		...overrides,
	};
}

function makeFsEvidence(overrides: Partial<SurfaceFsMutationEvidence> & {
	sourceFilePath: string;
	mutationKind: MutationKind;
	dynamicPath: boolean;
}): SurfaceFsMutationEvidence {
	return {
		surfaceFsMutationEvidenceUid: `ev-${overrides.sourceFilePath}-${overrides.mutationKind}`,
		surfaceFsMutationUid: overrides.dynamicPath ? null : "fs-1",
		snapshotUid: "snap-1",
		repoUid: "repo-1",
		projectSurfaceUid: "surface-a",
		sourceFilePath: overrides.sourceFilePath,
		lineNumber: 1,
		mutationKind: overrides.mutationKind,
		mutationPattern: "fs_write_file",
		dynamicPath: overrides.dynamicPath,
		confidence: 0.85,
		metadataJson: null,
		...overrides,
	};
}

// ── env access kind precedence ─────────────────────────────────────

describe("aggregateEnvAccessKind", () => {
	it("returns required if any input is required", () => {
		expect(aggregateEnvAccessKind(["optional", "required", "unknown"])).toBe("required");
		expect(aggregateEnvAccessKind(["required"])).toBe("required");
	});

	it("returns optional if any input is optional and none required", () => {
		expect(aggregateEnvAccessKind(["unknown", "optional"])).toBe("optional");
		expect(aggregateEnvAccessKind(["optional", "optional"])).toBe("optional");
	});

	it("returns unknown only when all inputs are unknown", () => {
		expect(aggregateEnvAccessKind(["unknown", "unknown"])).toBe("unknown");
	});

	it("returns unknown for empty input", () => {
		expect(aggregateEnvAccessKind([])).toBe("unknown");
	});
});

// ── env aggregation ────────────────────────────────────────────────

describe("aggregateEnvAcrossSurfaces", () => {
	it("returns one row per env name across surfaces", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "DATABASE_URL",
					accessKind: "required",
				}),
				evidenceCount: 2,
			},
			{
				surfaceUid: "surface-b",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-b",
					envName: "DATABASE_URL",
					accessKind: "required",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "PORT",
					accessKind: "optional",
					defaultValue: "3000",
				}),
				evidenceCount: 1,
			},
		]);

		expect(out).toHaveLength(2);
		const dbUrl = out.find((r) => r.envName === "DATABASE_URL")!;
		expect(dbUrl.surfaceCount).toBe(2);
		expect(dbUrl.evidenceCount).toBe(3);
		expect(dbUrl.accessKind).toBe("required");
		expect(dbUrl.defaultValue).toBeNull();
		expect(dbUrl.hasConflictingDefaults).toBe(false);

		const port = out.find((r) => r.envName === "PORT")!;
		expect(port.surfaceCount).toBe(1);
		expect(port.evidenceCount).toBe(1);
		expect(port.accessKind).toBe("optional");
		expect(port.defaultValue).toBe("3000");
	});

	it("escalates access kind to required when any contribution is required", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "FEATURE_FLAG",
					accessKind: "optional",
					defaultValue: "false",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-b",
					envName: "FEATURE_FLAG",
					accessKind: "required",
				}),
				evidenceCount: 1,
			},
		]);

		expect(out).toHaveLength(1);
		expect(out[0].accessKind).toBe("required");
		// First non-null default is preserved even though escalated.
		expect(out[0].defaultValue).toBe("false");
		expect(out[0].hasConflictingDefaults).toBe(false);
	});

	it("flags hasConflictingDefaults when distinct non-null defaults exist", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "PORT",
					accessKind: "optional",
					defaultValue: "3000",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-b",
					envName: "PORT",
					accessKind: "optional",
					defaultValue: "8080",
				}),
				evidenceCount: 1,
			},
		]);

		expect(out).toHaveLength(1);
		expect(out[0].hasConflictingDefaults).toBe(true);
		// First non-null default wins for display.
		expect(out[0].defaultValue).toBe("3000");
	});

	it("does not flag conflicting defaults when one side is null", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "API_KEY",
					accessKind: "required",
					defaultValue: null,
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-b",
					envName: "API_KEY",
					accessKind: "optional",
					defaultValue: "dev-key",
				}),
				evidenceCount: 2,
			},
		]);

		expect(out[0].hasConflictingDefaults).toBe(false);
		expect(out[0].defaultValue).toBe("dev-key");
		expect(out[0].accessKind).toBe("required");
	});

	it("returns max confidence across contributions", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-a",
					envName: "X",
					confidence: 0.7,
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				dependency: makeEnvDep({
					projectSurfaceUid: "surface-b",
					envName: "X",
					confidence: 0.95,
				}),
				evidenceCount: 1,
			},
		]);
		expect(out[0].maxConfidence).toBe(0.95);
	});

	it("sorts output by env name ascending", () => {
		const out = aggregateEnvAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({ projectSurfaceUid: "surface-a", envName: "ZULU" }),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({ projectSurfaceUid: "surface-a", envName: "ALPHA" }),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				dependency: makeEnvDep({ projectSurfaceUid: "surface-a", envName: "MIKE" }),
				evidenceCount: 1,
			},
		]);
		expect(out.map((r) => r.envName)).toEqual(["ALPHA", "MIKE", "ZULU"]);
	});

	it("returns empty array on empty input", () => {
		expect(aggregateEnvAcrossSurfaces([])).toEqual([]);
	});
});

// ── fs aggregation ─────────────────────────────────────────────────

describe("aggregateFsAcrossSurfaces", () => {
	it("unions identical (path, kind) pairs across surfaces", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "logs/app.log",
					mutationKind: "write_file",
				}),
				evidenceCount: 3,
			},
			{
				surfaceUid: "surface-b",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-b",
					targetPath: "logs/app.log",
					mutationKind: "write_file",
				}),
				evidenceCount: 2,
			},
		]);

		expect(out).toHaveLength(1);
		expect(out[0].targetPath).toBe("logs/app.log");
		expect(out[0].mutationKind).toBe("write_file");
		expect(out[0].surfaceCount).toBe(2);
		expect(out[0].evidenceCount).toBe(5);
	});

	it("keeps same path with different kinds as separate rows", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "data/state.json",
					mutationKind: "write_file",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "data/state.json",
					mutationKind: "delete_path",
				}),
				evidenceCount: 1,
			},
		]);

		expect(out).toHaveLength(2);
		const kinds = out.map((r) => r.mutationKind).sort();
		expect(kinds).toEqual(["delete_path", "write_file"]);
	});

	it("unions destination paths from rename/copy metadata across surfaces", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "src.txt",
					mutationKind: "copy_path",
					metadataJson: JSON.stringify({ destinationPaths: ["backup1.txt"] }),
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-b",
					targetPath: "src.txt",
					mutationKind: "copy_path",
					metadataJson: JSON.stringify({ destinationPaths: ["backup2.txt", "backup3.txt"] }),
				}),
				evidenceCount: 1,
			},
		]);

		expect(out).toHaveLength(1);
		expect(out[0].destinationPaths).toEqual(["backup1.txt", "backup2.txt", "backup3.txt"]);
	});

	it("dedups duplicate destination paths in union", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "src.txt",
					mutationKind: "copy_path",
					metadataJson: JSON.stringify({ destinationPaths: ["dst.txt"] }),
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-b",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-b",
					targetPath: "src.txt",
					mutationKind: "copy_path",
					metadataJson: JSON.stringify({ destinationPaths: ["dst.txt"] }),
				}),
				evidenceCount: 1,
			},
		]);

		expect(out[0].destinationPaths).toEqual(["dst.txt"]);
	});

	it("ignores malformed metadata JSON without throwing", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "x",
					mutationKind: "rename_path",
					metadataJson: "{ not valid json",
				}),
				evidenceCount: 1,
			},
		]);
		expect(out).toHaveLength(1);
		expect(out[0].destinationPaths).toEqual([]);
	});

	it("sorts output by targetPath then mutationKind", () => {
		const out = aggregateFsAcrossSurfaces([
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "z.txt",
					mutationKind: "write_file",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "a.txt",
					mutationKind: "write_file",
				}),
				evidenceCount: 1,
			},
			{
				surfaceUid: "surface-a",
				mutation: makeFsMutation({
					projectSurfaceUid: "surface-a",
					targetPath: "a.txt",
					mutationKind: "delete_path",
				}),
				evidenceCount: 1,
			},
		]);
		expect(out.map((r) => `${r.targetPath}|${r.mutationKind}`)).toEqual([
			"a.txt|delete_path",
			"a.txt|write_file",
			"z.txt|write_file",
		]);
	});

	it("returns empty array on empty input", () => {
		expect(aggregateFsAcrossSurfaces([])).toEqual([]);
	});
});

// ── fs dynamic evidence summary ────────────────────────────────────

describe("summarizeFsDynamicEvidence", () => {
	it("counts only dynamic-path evidence", () => {
		const summary = summarizeFsDynamicEvidence([
			makeFsEvidence({ sourceFilePath: "a.ts", mutationKind: "write_file", dynamicPath: false }),
			makeFsEvidence({ sourceFilePath: "b.ts", mutationKind: "write_file", dynamicPath: true }),
			makeFsEvidence({ sourceFilePath: "c.ts", mutationKind: "delete_path", dynamicPath: true }),
		]);
		expect(summary.totalCount).toBe(2);
	});

	it("counts distinct source files among dynamic occurrences", () => {
		const summary = summarizeFsDynamicEvidence([
			makeFsEvidence({ sourceFilePath: "a.ts", mutationKind: "write_file", dynamicPath: true }),
			makeFsEvidence({ sourceFilePath: "a.ts", mutationKind: "delete_path", dynamicPath: true }),
			makeFsEvidence({ sourceFilePath: "b.ts", mutationKind: "write_file", dynamicPath: true }),
		]);
		expect(summary.totalCount).toBe(3);
		expect(summary.distinctFileCount).toBe(2);
	});

	it("groups by mutation kind", () => {
		const summary = summarizeFsDynamicEvidence([
			makeFsEvidence({ sourceFilePath: "a.ts", mutationKind: "write_file", dynamicPath: true }),
			makeFsEvidence({ sourceFilePath: "b.ts", mutationKind: "write_file", dynamicPath: true }),
			makeFsEvidence({ sourceFilePath: "c.ts", mutationKind: "delete_path", dynamicPath: true }),
		]);
		expect(summary.byKind.write_file).toBe(2);
		expect(summary.byKind.delete_path).toBe(1);
	});

	it("returns zero summary on empty input", () => {
		const summary = summarizeFsDynamicEvidence([]);
		expect(summary.totalCount).toBe(0);
		expect(summary.distinctFileCount).toBe(0);
		expect(summary.byKind).toEqual({});
	});

	it("returns zero summary when no evidence is dynamic", () => {
		const summary = summarizeFsDynamicEvidence([
			makeFsEvidence({ sourceFilePath: "a.ts", mutationKind: "write_file", dynamicPath: false }),
			makeFsEvidence({ sourceFilePath: "b.ts", mutationKind: "delete_path", dynamicPath: false }),
		]);
		expect(summary.totalCount).toBe(0);
		expect(summary.distinctFileCount).toBe(0);
	});
});
