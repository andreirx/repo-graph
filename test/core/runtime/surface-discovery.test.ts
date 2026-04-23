/**
 * Surface discovery unit tests.
 *
 * Tests the pure discoverSurfaces orchestrator logic:
 * - Module candidate linkage
 * - Identity computation
 * - Deduplication by stableSurfaceKey
 * - Evidence merging
 * - Confidence-based surface selection
 */

import { describe, it, expect } from "vitest";
import { discoverSurfaces } from "../../../src/core/runtime/surface-discovery.js";
import type { ModuleCandidate } from "../../../src/core/modules/module-candidate.js";
import type { DetectedSurface } from "../../../src/core/runtime/surface-detectors.js";

// ── Fixtures ───────────────────────────────────────────────────────

function makeModuleCandidate(rootPath: string, uid: string): ModuleCandidate {
	return {
		moduleCandidateUid: uid,
		snapshotUid: "snap-001",
		repoUid: "repo-001",
		moduleKey: rootPath,
		moduleKind: "npm_package",
		canonicalRootPath: rootPath,
		displayName: rootPath,
		confidence: 0.9,
		metadataJson: null,
	};
}

function makeDetectedSurface(
	rootPath: string,
	surfaceKind: "cli" | "library" | "backend_service",
	confidence: number,
	sourceType: "package_json_bin" | "package_json_main" | "framework_dependency",
	binName?: string,
): DetectedSurface {
	return {
		surfaceKind,
		displayName: binName ?? rootPath,
		rootPath,
		entrypointPath: `${rootPath}/index.js`,
		buildSystem: "typescript_tsc",
		runtimeKind: "node",
		confidence,
		evidence: [{
			sourceType,
			sourcePath: `${rootPath}/package.json`,
			evidenceKind: sourceType === "package_json_bin" ? "binary_entrypoint" : "main_export",
			confidence,
			payload: binName ? { binName } : null,
		}],
		metadata: null,
		sourceType,
		binName,
	};
}

// ── Tests ──────────────────────────────────────────────────────────

describe("discoverSurfaces", () => {
	const repoUid = "repo-001";
	const snapshotUid = "snap-001";

	it("links surfaces to module candidates by root path", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		const detectedSurfaces = [
			makeDetectedSurface("src", "library", 0.85, "package_json_main"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].moduleCandidateUid).toBe("mc-001");
		expect(result.unattachedSurfaces).toHaveLength(0);
	});

	it("collects unattached surfaces for Layer 2 promotion", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		const detectedSurfaces = [
			makeDetectedSurface("lib", "library", 0.85, "package_json_main"), // No matching module
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(0);
		expect(result.unattachedSurfaces).toHaveLength(1);
		expect(result.unattachedSurfaces[0].rootPath).toBe("lib");
	});

	it("computes identity fields (stableSurfaceKey, sourceSpecificId)", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "my-cli"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(1);
		const surface = result.surfaces[0];
		expect(surface.sourceType).toBe("package_json_bin");
		expect(surface.sourceSpecificId).toBe("my-cli:src/index.js");
		expect(surface.stableSurfaceKey).toBeDefined();
		expect(surface.stableSurfaceKey).toHaveLength(32); // 128-bit hex
	});

	it("deduplicates surfaces by stableSurfaceKey", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		// Two detections with identical identity (same binName, entrypoint)
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.85, "package_json_bin", "my-cli"),
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "my-cli"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		// Should deduplicate to one surface
		expect(result.surfaces).toHaveLength(1);
	});

	it("keeps highest-confidence surface when deduplicating", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		// First detection has lower confidence, second has higher
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.70, "package_json_bin", "my-cli"),
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "my-cli"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(1);
		expect(result.surfaces[0].confidence).toBe(0.95);
	});

	it("merges evidence from duplicate detections", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		// Two detections with same identity but different evidence payloads
		const surface1: DetectedSurface = {
			surfaceKind: "cli",
			displayName: "my-cli",
			rootPath: "src",
			entrypointPath: "src/index.js",
			buildSystem: "typescript_tsc",
			runtimeKind: "node",
			confidence: 0.85,
			evidence: [{
				sourceType: "package_json_bin",
				sourcePath: "src/package.json",
				evidenceKind: "binary_entrypoint",
				confidence: 0.85,
				payload: { binName: "my-cli", binPath: "./index.js" },
			}],
			metadata: null,
			sourceType: "package_json_bin",
			binName: "my-cli",
		};
		const surface2: DetectedSurface = {
			...surface1,
			confidence: 0.90,
			evidence: [{
				sourceType: "package_json_bin",
				sourcePath: "src/package.json",
				evidenceKind: "binary_entrypoint",
				confidence: 0.90,
				payload: { binName: "my-cli", binPath: "./dist/index.js" }, // Different payload → different evidence UID
			}],
		};

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces: [surface1, surface2],
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(1);
		// Evidence from both detections should be merged
		expect(result.evidence.length).toBe(2);
	});

	it("does not duplicate evidence with same UID", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		// Two identical detections with identical evidence → same evidence UID
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.85, "package_json_bin", "my-cli"),
			makeDetectedSurface("src", "cli", 0.90, "package_json_bin", "my-cli"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		expect(result.surfaces).toHaveLength(1);
		// Same evidence payload → same evidence UID → deduplicated to 1
		expect(result.evidence.length).toBe(1);
	});

	it("does not deduplicate surfaces with different stableSurfaceKey", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		// Two CLI surfaces with different binNames → different identity
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "cli-a"),
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "cli-b"),
		];

		const result = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		// Different binName → different sourceSpecificId → different stableSurfaceKey
		expect(result.surfaces).toHaveLength(2);
	});

	it("computes deterministic UIDs for same inputs", () => {
		const moduleCandidates = [
			makeModuleCandidate("src", "mc-001"),
		];
		const detectedSurfaces = [
			makeDetectedSurface("src", "cli", 0.95, "package_json_bin", "my-cli"),
		];

		const result1 = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		const result2 = discoverSurfaces({
			repoUid,
			snapshotUid,
			detectedSurfaces,
			moduleCandidates,
		});

		// Same inputs → same UIDs
		expect(result1.surfaces[0].projectSurfaceUid).toBe(result2.surfaces[0].projectSurfaceUid);
		expect(result1.surfaces[0].stableSurfaceKey).toBe(result2.surfaces[0].stableSurfaceKey);
		expect(result1.evidence[0].projectSurfaceEvidenceUid).toBe(result2.evidence[0].projectSurfaceEvidenceUid);
	});
});
