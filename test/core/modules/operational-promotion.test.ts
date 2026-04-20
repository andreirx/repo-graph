/**
 * Unit tests for the operational module promotion (Layer 2).
 *
 * Pure logic — no filesystem, no storage. Tests cover:
 *   - surface kind eligibility (cli, backend_service, web_app, worker)
 *   - evidence confidence threshold (>= 0.70)
 *   - aggregate confidence ceiling (0.85)
 *   - existing root collision detection
 *   - grouped surfaces at same root
 *   - evidence creation from promoting surfaces
 *
 * See docs/architecture/module-discovery-layers.txt for the contract.
 */

import { describe, expect, it } from "vitest";
import { promoteUnattachedSurfaces } from "../../../src/core/modules/operational-promotion.js";
import type { DetectedSurface, SurfaceEvidence } from "../../../src/core/runtime/surface-detectors.js";
import type { SurfaceKind } from "../../../src/core/runtime/project-surface.js";

const REPO_UID = "test-repo";
const SNAPSHOT_UID = "snap-1";

function makeEvidence(confidence: number): SurfaceEvidence {
	return {
		sourceType: "cli_manifest",
		sourcePath: "package.json",
		evidenceKind: "bin_entry",
		confidence,
		payload: null,
	};
}

function makeSurface(overrides: Partial<DetectedSurface> & { rootPath: string; surfaceKind: SurfaceKind }): DetectedSurface {
	return {
		displayName: null,
		entrypointPath: null,
		buildSystem: null,
		runtimeKind: "node",
		confidence: 0.80,
		metadata: null,
		evidence: [makeEvidence(0.80)],
		...overrides,
	};
}

// ── Surface kind eligibility ───────────────────────────────────────

describe("promoteUnattachedSurfaces — kind eligibility", () => {
	it("promotes cli surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "tools/cli", surfaceKind: "cli" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].moduleKind).toBe("operational");
		expect(result.diagnostics.surfacesPromoted).toBe(1);
	});

	it("promotes backend_service surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "services/api", surfaceKind: "backend_service" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].moduleKind).toBe("operational");
	});

	it("promotes web_app surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "apps/frontend", surfaceKind: "web_app" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].moduleKind).toBe("operational");
	});

	it("promotes worker surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "workers/batch", surfaceKind: "worker" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].moduleKind).toBe("operational");
	});

	it("does NOT promote library surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "libs/utils", surfaceKind: "library" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(0);
		expect(result.diagnostics.surfacesSkippedKind).toBe(1);
	});

	it("does NOT promote test_suite surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "test", surfaceKind: "test_suite" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(0);
		expect(result.diagnostics.surfacesSkippedKind).toBe(1);
	});
});

// ── Confidence threshold ───────────────────────────────────────────

describe("promoteUnattachedSurfaces — confidence threshold", () => {
	it("promotes surfaces with evidence confidence >= 0.70", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.70)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
	});

	it("does NOT promote surfaces with max evidence confidence < 0.70", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.69), makeEvidence(0.50)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(0);
		expect(result.diagnostics.surfacesSkippedConfidence).toBe(1);
	});

	it("uses max evidence confidence when multiple evidence items exist", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.50), makeEvidence(0.75)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
	});
});

// ── Confidence ceiling ─────────────────────────────────────────────

describe("promoteUnattachedSurfaces — confidence ceiling", () => {
	it("clamps module confidence to 0.85 (Layer 2 ceiling)", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.95)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].confidence).toBe(0.85);
	});

	it("preserves confidence below ceiling unchanged", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.75)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].confidence).toBe(0.75);
	});
});

// ── Existing root collision ────────────────────────────────────────

describe("promoteUnattachedSurfaces — existing root collision", () => {
	it("skips surfaces at roots that already exist as declared modules", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "packages/api", surfaceKind: "cli" }),
			],
			existingRoots: new Set(["packages/api"]),
		});

		expect(result.candidates).toHaveLength(0);
		expect(result.diagnostics.promotionSkippedExistingRoot).toBe(1);
	});

	it("promotes surfaces at roots not in existing set", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "tools/new-cli", surfaceKind: "cli" }),
			],
			existingRoots: new Set(["packages/api"]),
		});

		expect(result.candidates).toHaveLength(1);
	});
});

// ── Root grouping ──────────────────────────────────────────────────

describe("promoteUnattachedSurfaces — root grouping", () => {
	it("groups multiple surfaces at the same root into one candidate", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "apps/server", surfaceKind: "backend_service", displayName: "API" }),
				makeSurface({ rootPath: "apps/server", surfaceKind: "cli", displayName: "CLI" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates).toHaveLength(1);
		expect(result.candidates[0].canonicalRootPath).toBe("apps/server");
		// First display name is used
		expect(result.candidates[0].displayName).toBe("API");
	});

	it("creates separate evidence for each surface at the same root", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "apps/server", surfaceKind: "backend_service" }),
				makeSurface({ rootPath: "apps/server", surfaceKind: "cli" }),
			],
			existingRoots: new Set(),
		});

		expect(result.evidence).toHaveLength(2);
	});

	it("counts all surfaces as promoted in diagnostics", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "apps/server", surfaceKind: "backend_service" }),
				makeSurface({ rootPath: "apps/server", surfaceKind: "cli" }),
			],
			existingRoots: new Set(),
		});

		expect(result.diagnostics.surfacesPromoted).toBe(2);
	});

	it("uses max confidence across grouped surfaces", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "apps/server",
					surfaceKind: "backend_service",
					evidence: [makeEvidence(0.75)],
				}),
				makeSurface({
					rootPath: "apps/server",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.80)],
				}),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates[0].confidence).toBe(0.80);
	});
});

// ── Evidence structure ─────────────────────────────────────────────

describe("promoteUnattachedSurfaces — evidence structure", () => {
	it("creates evidence with correct source type for cli", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "tools/cli", surfaceKind: "cli" }),
			],
			existingRoots: new Set(),
		});

		expect(result.evidence[0].sourceType).toBe("surface_promotion_cli");
		expect(result.evidence[0].evidenceKind).toBe("operational_entrypoint");
	});

	it("creates evidence with correct source type for backend_service", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "services/api", surfaceKind: "backend_service" }),
			],
			existingRoots: new Set(),
		});

		expect(result.evidence[0].sourceType).toBe("surface_promotion_service");
	});

	it("creates evidence with correct source type for web_app", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "apps/frontend", surfaceKind: "web_app" }),
			],
			existingRoots: new Set(),
		});

		expect(result.evidence[0].sourceType).toBe("surface_promotion_web_app");
	});

	it("creates evidence with correct source type for worker", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "workers/batch", surfaceKind: "worker" }),
			],
			existingRoots: new Set(),
		});

		expect(result.evidence[0].sourceType).toBe("surface_promotion_worker");
	});

	it("includes original evidence in payload", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({
					rootPath: "tools/cli",
					surfaceKind: "cli",
					evidence: [makeEvidence(0.80)],
				}),
			],
			existingRoots: new Set(),
		});

		const payload = JSON.parse(result.evidence[0].payloadJson!);
		expect(payload.promoting_surface_kind).toBe("cli");
		expect(payload.original_evidence).toHaveLength(1);
		expect(payload.original_evidence[0].confidence).toBe(0.80);
	});
});

// ── Module key generation ──────────────────────────────────────────

describe("promoteUnattachedSurfaces — module key", () => {
	it("generates stable module key from repo UID and root path", () => {
		const result = promoteUnattachedSurfaces({
			repoUid: REPO_UID,
			snapshotUid: SNAPSHOT_UID,
			unattachedSurfaces: [
				makeSurface({ rootPath: "tools/cli", surfaceKind: "cli" }),
			],
			existingRoots: new Set(),
		});

		expect(result.candidates[0].moduleKey).toBe(`${REPO_UID}:tools/cli:DISCOVERED_MODULE`);
	});
});
