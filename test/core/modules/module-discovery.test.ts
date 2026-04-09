/**
 * Unit tests for the module discovery orchestrator.
 *
 * Pure logic — no filesystem, no storage. Tests cover:
 *   - candidate creation from discovered roots
 *   - evidence deduplication and merging
 *   - file ownership by longest-prefix match
 *   - overlapping candidate roots
 *   - confidence propagation
 *   - workspace root vs member semantics
 *   - unowned files (no matching candidate)
 */

import { describe, expect, it } from "vitest";
import { discoverModules } from "../../../src/core/modules/module-discovery.js";
import type { DiscoveredModuleRoot } from "../../../src/core/modules/manifest-detectors.js";
import type { TrackedFile } from "../../../src/core/model/index.js";

const REPO_UID = "test-repo";
const SNAPSHOT_UID = "snap-1";

function makeFile(path: string): TrackedFile {
	return {
		fileUid: `${REPO_UID}:${path}`,
		repoUid: REPO_UID,
		path,
		language: "typescript",
		isTest: false,
		isGenerated: false,
		isExcluded: false,
	};
}

function makeRoot(overrides: Partial<DiscoveredModuleRoot> & { rootPath: string }): DiscoveredModuleRoot {
	return {
		displayName: null,
		sourceType: "package_json_workspaces",
		sourcePath: "package.json",
		evidenceKind: "workspace_member",
		confidence: 0.95,
		payload: null,
		...overrides,
	};
}

// ── Basic candidate creation ───────────────────────────────────────

describe("discoverModules — candidates", () => {
	it("creates one candidate per unique root path", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api", displayName: "@app/api" }),
				makeRoot({ rootPath: "packages/web", displayName: "@app/web" }),
			],
			trackedFiles: [],
		});

		expect(result.candidates).toHaveLength(2);
		const paths = result.candidates.map((c) => c.canonicalRootPath).sort();
		expect(paths).toEqual(["packages/api", "packages/web"]);
	});

	it("generates stable module keys anchored by root path", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [makeRoot({ rootPath: "packages/api" })],
			trackedFiles: [],
		});

		expect(result.candidates[0].moduleKey).toBe(
			`${REPO_UID}:packages/api:DISCOVERED_MODULE`,
		);
	});

	it("sets moduleKind to declared", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [makeRoot({ rootPath: "lib" })],
			trackedFiles: [],
		});

		expect(result.candidates[0].moduleKind).toBe("declared");
	});

	it("returns empty results for no discovered roots", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [],
			trackedFiles: [makeFile("src/index.ts")],
		});

		expect(result.candidates).toHaveLength(0);
		expect(result.evidence).toHaveLength(0);
		expect(result.ownership).toHaveLength(0);
	});
});

// ── Evidence merging ───────────────────────────────────────────────

describe("discoverModules — evidence", () => {
	it("creates one evidence row per discovered root item", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api" }),
				makeRoot({ rootPath: "packages/web" }),
			],
			trackedFiles: [],
		});

		expect(result.evidence).toHaveLength(2);
	});

	it("merges multiple evidence items for the same root path", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({
					rootPath: "packages/api",
					sourceType: "package_json_workspaces",
					sourcePath: "package.json",
				}),
				makeRoot({
					rootPath: "packages/api",
					sourceType: "pnpm_workspace_yaml",
					sourcePath: "pnpm-workspace.yaml",
				}),
			],
			trackedFiles: [],
		});

		// One candidate, two evidence items.
		expect(result.candidates).toHaveLength(1);
		expect(result.evidence).toHaveLength(2);
		// Both evidence items reference the same candidate.
		const candidateUid = result.candidates[0].moduleCandidateUid;
		expect(result.evidence.every((e) => e.moduleCandidateUid === candidateUid)).toBe(true);
	});

	it("takes max confidence across evidence items", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "lib", confidence: 0.8 }),
				makeRoot({ rootPath: "lib", confidence: 0.95 }),
			],
			trackedFiles: [],
		});

		expect(result.candidates[0].confidence).toBe(0.95);
	});

	it("prefers first non-null display name", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "lib", displayName: null }),
				makeRoot({ rootPath: "lib", displayName: "my-lib" }),
			],
			trackedFiles: [],
		});

		expect(result.candidates[0].displayName).toBe("my-lib");
	});
});

// ── File ownership ─────────────────────────────────────────────────

describe("discoverModules — ownership", () => {
	it("assigns files to their containing module root", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api" }),
			],
			trackedFiles: [
				makeFile("packages/api/src/index.ts"),
				makeFile("packages/api/src/handler.ts"),
			],
		});

		expect(result.ownership).toHaveLength(2);
		const candidateUid = result.candidates[0].moduleCandidateUid;
		expect(result.ownership.every((o) => o.moduleCandidateUid === candidateUid)).toBe(true);
		expect(result.ownership.every((o) => o.assignmentKind === "root_containment")).toBe(true);
	});

	it("uses longest-prefix match for overlapping roots", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages", displayName: "root-packages" }),
				makeRoot({ rootPath: "packages/api", displayName: "api" }),
			],
			trackedFiles: [
				makeFile("packages/api/src/index.ts"),
				makeFile("packages/shared/util.ts"),
			],
		});

		expect(result.ownership).toHaveLength(2);

		const apiCandidate = result.candidates.find((c) => c.canonicalRootPath === "packages/api");
		const pkgCandidate = result.candidates.find((c) => c.canonicalRootPath === "packages");

		// api/src/index.ts should be owned by packages/api (more specific).
		const apiFile = result.ownership.find((o) =>
			o.fileUid === `${REPO_UID}:packages/api/src/index.ts`,
		);
		expect(apiFile!.moduleCandidateUid).toBe(apiCandidate!.moduleCandidateUid);

		// shared/util.ts should be owned by packages (less specific fallback).
		const sharedFile = result.ownership.find((o) =>
			o.fileUid === `${REPO_UID}:packages/shared/util.ts`,
		);
		expect(sharedFile!.moduleCandidateUid).toBe(pkgCandidate!.moduleCandidateUid);
	});

	it("leaves files unowned if no candidate root matches", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api" }),
			],
			trackedFiles: [
				makeFile("packages/api/src/index.ts"),
				makeFile("scripts/deploy.sh"),  // Not under any module root.
			],
		});

		expect(result.ownership).toHaveLength(1);
		expect(result.ownership[0].fileUid).toBe(`${REPO_UID}:packages/api/src/index.ts`);
	});

	it("handles root path of . (repo-root module)", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: ".", displayName: "root-pkg" }),
			],
			trackedFiles: [
				makeFile("src/index.ts"),
				makeFile("lib/util.ts"),
			],
		});

		// Root "." matches everything.
		expect(result.ownership).toHaveLength(2);
	});

	it("skips excluded files", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [makeRoot({ rootPath: "." })],
			trackedFiles: [
				makeFile("src/index.ts"),
				{ ...makeFile("generated/large.ts"), isExcluded: true },
			],
		});

		expect(result.ownership).toHaveLength(1);
	});

	it("does not match a file whose path starts with root but is not a child", () => {
		// "packages/api-docs/readme.md" should NOT match root "packages/api"
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api" }),
			],
			trackedFiles: [
				makeFile("packages/api-docs/readme.md"),
			],
		});

		// The file path starts with "packages/api" but is not under "packages/api/".
		expect(result.ownership).toHaveLength(0);
	});
});

// ── Confidence propagation ─────────────────────────────────────────

describe("discoverModules — confidence", () => {
	it("propagates candidate confidence to ownership rows", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "lib", confidence: 0.90 }),
			],
			trackedFiles: [makeFile("lib/index.ts")],
		});

		expect(result.ownership[0].confidence).toBe(0.90);
	});
});

// ── Root path normalization ────────────────────────────────────────

describe("discoverModules — normalization", () => {
	it("normalizes trailing slashes", () => {
		const result = discoverModules({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			discoveredRoots: [
				makeRoot({ rootPath: "packages/api/" }),
				makeRoot({ rootPath: "packages/api" }),
			],
			trackedFiles: [],
		});

		// Should merge into one candidate.
		expect(result.candidates).toHaveLength(1);
		expect(result.evidence).toHaveLength(2);
	});
});
