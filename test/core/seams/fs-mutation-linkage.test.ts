/**
 * FS mutation linkage orchestrator unit tests.
 *
 * Pure function — verifies linkage policy without indexer/storage.
 * Covers:
 *   - identity dedup by (surface, path, kind)
 *   - same path + different kind → separate identity rows
 *   - dynamic-path occurrences create evidence only (no identity)
 *   - multi-surface linkage when file is shared
 *   - unlinked file dropping
 */

import { describe, expect, it } from "vitest";
import { linkFsMutations } from "../../../src/core/seams/fs-mutation-linkage.js";
import type { DetectedFsMutation } from "../../../src/core/seams/fs-mutation.js";

function makeMutation(overrides: Partial<DetectedFsMutation> & {
	filePath: string;
	mutationKind: DetectedFsMutation["mutationKind"];
}): DetectedFsMutation {
	return {
		lineNumber: 1,
		mutationPattern: "fs_write_file",
		targetPath: "logs/app.log",
		dynamicPath: false,
		confidence: 0.90,
		...overrides,
	};
}

describe("linkFsMutations", () => {
	it("creates identity + evidence row for a literal mutation", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/logger.ts",
					mutationKind: "write_file",
					targetPath: "logs/app.log",
				}),
			],
			fileToSurfaces: new Map([["src/logger.ts", ["surface-a"]]]),
		});

		expect(result.identities).toHaveLength(1);
		expect(result.identities[0].targetPath).toBe("logs/app.log");
		expect(result.identities[0].mutationKind).toBe("write_file");
		expect(result.identities[0].projectSurfaceUid).toBe("surface-a");
		expect(result.evidence).toHaveLength(1);
		expect(result.evidence[0].surfaceFsMutationUid).toBe(result.identities[0].surfaceFsMutationUid);
		expect(result.evidence[0].dynamicPath).toBe(false);
	});

	it("dedups same surface+path+kind across multiple files", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "write_file",
					targetPath: "logs/app.log",
					lineNumber: 5,
				}),
				makeMutation({
					filePath: "src/b.ts",
					mutationKind: "write_file",
					targetPath: "logs/app.log",
					lineNumber: 12,
				}),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		// One identity, two evidence.
		expect(result.identities).toHaveLength(1);
		expect(result.evidence).toHaveLength(2);
		const lines = result.evidence.map((e) => e.lineNumber).sort((a, b) => a - b);
		expect(lines).toEqual([5, 12]);
	});

	it("keeps same path + different kind as separate identity rows", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "write_file",
					targetPath: "data/cache.json",
				}),
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "delete_path",
					targetPath: "data/cache.json",
				}),
			],
			fileToSurfaces: new Map([["src/a.ts", ["surface-a"]]]),
		});

		expect(result.identities).toHaveLength(2);
		const kinds = result.identities.map((i) => i.mutationKind).sort();
		expect(kinds).toEqual(["delete_path", "write_file"]);
	});

	it("dynamic-path occurrences produce evidence only, no identity", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/dynamic.ts",
					mutationKind: "write_file",
					targetPath: null,
					dynamicPath: true,
				}),
			],
			fileToSurfaces: new Map([["src/dynamic.ts", ["surface-a"]]]),
		});

		expect(result.identities).toHaveLength(0);
		expect(result.evidence).toHaveLength(1);
		expect(result.evidence[0].surfaceFsMutationUid).toBeNull();
		expect(result.evidence[0].dynamicPath).toBe(true);
	});

	it("links shared file to multiple surfaces", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/shared.ts",
					mutationKind: "write_file",
					targetPath: "out.txt",
				}),
			],
			fileToSurfaces: new Map([["src/shared.ts", ["surface-a", "surface-b"]]]),
		});

		expect(result.identities).toHaveLength(2);
		const surfaces = result.identities.map((i) => i.projectSurfaceUid).sort();
		expect(surfaces).toEqual(["surface-a", "surface-b"]);
		expect(result.evidence).toHaveLength(2);
	});

	it("drops mutations from unowned files", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/orphan.ts",
					mutationKind: "write_file",
					targetPath: "out.txt",
				}),
			],
			fileToSurfaces: new Map(),
		});

		expect(result.identities).toHaveLength(0);
		expect(result.evidence).toHaveLength(0);
		expect(result.unlinkedDropped).toBe(1);
	});

	it("uses max confidence across occurrences", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "write_file",
					targetPath: "out.txt",
					confidence: 0.7,
				}),
				makeMutation({
					filePath: "src/b.ts",
					mutationKind: "write_file",
					targetPath: "out.txt",
					confidence: 0.95,
				}),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		expect(result.identities[0].confidence).toBe(0.95);
	});

	it("preserves destination path for rename in identity and evidence metadata", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/rename.ts",
					mutationKind: "rename_path",
					mutationPattern: "fs_rename",
					targetPath: "old.txt",
					destinationPath: "new.txt",
				}),
			],
			fileToSurfaces: new Map([["src/rename.ts", ["surface-a"]]]),
		});

		expect(result.identities).toHaveLength(1);
		expect(result.identities[0].targetPath).toBe("old.txt");
		expect(result.identities[0].metadataJson).toBeTruthy();
		const identityMeta = JSON.parse(result.identities[0].metadataJson!);
		expect(identityMeta.destinationPaths).toEqual(["new.txt"]);

		expect(result.evidence).toHaveLength(1);
		expect(result.evidence[0].metadataJson).toBeTruthy();
		const evidenceMeta = JSON.parse(result.evidence[0].metadataJson!);
		expect(evidenceMeta.destinationPath).toBe("new.txt");
	});

	it("aggregates multiple distinct destinations on identity metadata", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "copy_path",
					mutationPattern: "fs_copy_file",
					targetPath: "src.txt",
					destinationPath: "backup1.txt",
				}),
				makeMutation({
					filePath: "src/b.ts",
					mutationKind: "copy_path",
					mutationPattern: "fs_copy_file",
					targetPath: "src.txt",
					destinationPath: "backup2.txt",
				}),
			],
			fileToSurfaces: new Map([
				["src/a.ts", ["surface-a"]],
				["src/b.ts", ["surface-a"]],
			]),
		});

		// One identity (same surface, path, kind), two evidence.
		expect(result.identities).toHaveLength(1);
		const meta = JSON.parse(result.identities[0].metadataJson!);
		expect(meta.destinationPaths).toEqual(["backup1.txt", "backup2.txt"]);
	});

	it("dynamic and literal in same file produce one identity + two evidence", () => {
		const result = linkFsMutations({
			repoUid: "test-repo",
			snapshotUid: "snap-1",
			detectedMutations: [
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "write_file",
					targetPath: "out.txt",
				}),
				makeMutation({
					filePath: "src/a.ts",
					mutationKind: "write_file",
					targetPath: null,
					dynamicPath: true,
				}),
			],
			fileToSurfaces: new Map([["src/a.ts", ["surface-a"]]]),
		});

		expect(result.identities).toHaveLength(1);
		expect(result.evidence).toHaveLength(2);
		const dynamicEvidence = result.evidence.find((e) => e.dynamicPath);
		const literalEvidence = result.evidence.find((e) => !e.dynamicPath);
		expect(dynamicEvidence!.surfaceFsMutationUid).toBeNull();
		expect(literalEvidence!.surfaceFsMutationUid).toBe(result.identities[0].surfaceFsMutationUid);
	});
});
