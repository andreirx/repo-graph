/**
 * Project surface discovery integration tests.
 *
 * End-to-end: indexer → surface detectors → orchestrator → persistence.
 *
 * Uses existing fixtures:
 *   - simple-imports: package.json with name, no bin → library only
 *   - mixed-lang: package.json + Cargo.toml → library surfaces
 *
 * Verifies:
 *   - Surfaces persisted and linked to module candidates
 *   - Evidence items linked to surfaces
 *   - Surface kinds correct for detected manifests
 *   - Build system and runtime inference
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { ManifestScanner } from "../../../src/adapters/discovery/manifest-scanner.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";

const SIMPLE_IMPORTS_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/typescript/simple-imports",
);

const MIXED_LANG_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/mixed-lang",
);

const DOCKERFILE_APP_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/dockerfile-app",
);

const COMPOSE_MULTISERVICE_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/compose-multiservice",
);

const DOCKERFILE_PLUS_COMPOSE_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/dockerfile-plus-compose",
);

const SCRIPT_ONLY_PACKAGE_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/script-only-package",
);

const SCRIPT_ONLY_SERVICE_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/script-only-service",
);

let extractor: TypeScriptExtractor;
let scanner: ManifestScanner;

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
	scanner = new ManifestScanner();
});

const dbPaths: string[] = [];

afterEach(() => {
	for (const p of dbPaths) {
		try { unlinkSync(p); } catch { /* ignore */ }
	}
	dbPaths.length = 0;
});

function setupDb(): { storage: SqliteStorage; provider: SqliteConnectionProvider } {
	const dbPath = join(tmpdir(), `rgr-surface-${randomUUID()}.db`);
	dbPaths.push(dbPath);
	const provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	const storage = new SqliteStorage(provider.getDatabase());
	return { storage, provider };
}

// ── simple-imports fixture ─────────────────────────────────────────

describe("surface discovery integration — simple-imports", () => {
	it("detects no CLI surface (no bin field)", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-simple";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "simple-imports",
			rootPath: SIMPLE_IMPORTS_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// simple-imports has no bin field → no CLI surface.
		const cliSurfaces = surfaces.filter((s) => s.surfaceKind === "cli");
		expect(cliSurfaces).toHaveLength(0);

		provider.close();
	});

	it("surfaces are linked to module candidates", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-linked";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "simple-imports",
			rootPath: SIMPLE_IMPORTS_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Every surface should reference a valid module candidate.
		const candidateUids = new Set(candidates.map((c) => c.moduleCandidateUid));
		for (const s of surfaces) {
			expect(candidateUids.has(s.moduleCandidateUid)).toBe(true);
		}

		provider.close();
	});
});

// ── mixed-lang fixture ─────────────────────────────────────────────

describe("surface discovery integration — mixed-lang", () => {
	it("detects surfaces from multiple manifest types", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-mixed";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "mixed-lang",
			rootPath: MIXED_LANG_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// mixed-lang has package.json (with express dep) + Cargo.toml.
		// Should detect at least one surface.
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		// Check that surfaces have correct build systems.
		const buildSystems = surfaces.map((s) => s.buildSystem);
		// At least one from JS/TS ecosystem.
		const hasJsBuild = buildSystems.some((b) =>
			b === "typescript_tsc" || b === "typescript_bundler" || b === "unknown",
		);
		expect(hasJsBuild || buildSystems.includes("cargo")).toBe(true);

		provider.close();
	});

	it("persists evidence for each surface", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-evidence";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "mixed-lang",
			rootPath: MIXED_LANG_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const allEvidence = storage.queryAllProjectSurfaceEvidence(result.snapshotUid);

		// Each surface should have at least one evidence item.
		for (const s of surfaces) {
			const surfaceEvidence = allEvidence.filter(
				(e) => e.projectSurfaceUid === s.projectSurfaceUid,
			);
			expect(surfaceEvidence.length).toBeGreaterThanOrEqual(1);
		}

		provider.close();
	});

	it("detects backend_service from express dependency", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-express";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "mixed-lang",
			rootPath: MIXED_LANG_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const backend = surfaces.find((s) => s.surfaceKind === "backend_service");
		expect(backend).toBeDefined();
		expect(backend!.runtimeKind).toBe("node");

		provider.close();
	});
});

// ── dockerfile-app fixture ─────────────────────────────────────────

describe("surface discovery integration — dockerfile", () => {
	it("detects container surface from Dockerfile", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-dockerfile";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-app",
			rootPath: DOCKERFILE_APP_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should have a container surface from Dockerfile.
		const containerSurface = surfaces.find(
			(s) => s.sourceType === "dockerfile" && s.runtimeKind === "container",
		);
		expect(containerSurface).toBeDefined();
		expect(containerSurface!.surfaceKind).toBe("backend_service");

		provider.close();
	});

	it("persists correct identity fields for Dockerfile surface", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-docker-identity";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-app",
			rootPath: DOCKERFILE_APP_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const containerSurface = surfaces.find((s) => s.sourceType === "dockerfile");

		expect(containerSurface).toBeDefined();
		// sourceType must be "dockerfile"
		expect(containerSurface!.sourceType).toBe("dockerfile");
		// sourceSpecificId should be the Dockerfile path
		expect(containerSurface!.sourceSpecificId).toBe("Dockerfile");
		// stableSurfaceKey must be present and 32 chars (128 bits)
		expect(containerSurface!.stableSurfaceKey).toBeDefined();
		expect(containerSurface!.stableSurfaceKey).toHaveLength(32);

		provider.close();
	});

	it("produces deterministic identity on re-index", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-docker-deterministic";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-app",
			rootPath: DOCKERFILE_APP_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);

		// First index
		const result1 = await indexer.indexRepo(REPO_UID);
		const surfaces1 = storage.queryProjectSurfaces(result1.snapshotUid);
		const docker1 = surfaces1.find((s) => s.sourceType === "dockerfile");

		// Second index
		const result2 = await indexer.indexRepo(REPO_UID);
		const surfaces2 = storage.queryProjectSurfaces(result2.snapshotUid);
		const docker2 = surfaces2.find((s) => s.sourceType === "dockerfile");

		// Both exist
		expect(docker1).toBeDefined();
		expect(docker2).toBeDefined();

		// stableSurfaceKey must be identical across snapshots
		expect(docker2!.stableSurfaceKey).toBe(docker1!.stableSurfaceKey);

		// projectSurfaceUid differs (snapshot-scoped)
		expect(docker2!.projectSurfaceUid).not.toBe(docker1!.projectSurfaceUid);

		provider.close();
	});

	it("persists evidence with container_config kind", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-docker-evidence";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-app",
			rootPath: DOCKERFILE_APP_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const dockerSurface = surfaces.find((s) => s.sourceType === "dockerfile");
		expect(dockerSurface).toBeDefined();

		const allEvidence = storage.queryAllProjectSurfaceEvidence(result.snapshotUid);
		const dockerEvidence = allEvidence.filter(
			(e) => e.projectSurfaceUid === dockerSurface!.projectSurfaceUid,
		);

		expect(dockerEvidence.length).toBeGreaterThanOrEqual(1);
		expect(dockerEvidence[0].sourceType).toBe("dockerfile");
		expect(dockerEvidence[0].evidenceKind).toBe("container_config");
		expect(dockerEvidence[0].sourcePath).toBe("Dockerfile");

		// Evidence payload should contain base image info
		const payload = JSON.parse(dockerEvidence[0].payloadJson!);
		expect(payload.baseImage).toBe("node:20-alpine");
		expect(payload.baseRuntimeKind).toBe("node");

		provider.close();
	});
});

// ── compose-multiservice fixture ───────────────────────────────────

describe("surface discovery integration — docker-compose", () => {
	it("produces distinct surfaces for each compose service", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-compose-multi";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "compose-multiservice",
			rootPath: COMPOSE_MULTISERVICE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should have 3 compose service surfaces
		const composeSurfaces = surfaces.filter((s) => s.sourceType === "docker_compose");
		expect(composeSurfaces).toHaveLength(3);

		// Service names should be distinct
		const serviceNames = composeSurfaces
			.map((s) => JSON.parse(s.metadataJson!).serviceName)
			.sort();
		expect(serviceNames).toEqual(["api", "redis", "worker"]);

		provider.close();
	});

	it("each compose service has distinct stableSurfaceKey", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-compose-keys";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "compose-multiservice",
			rootPath: COMPOSE_MULTISERVICE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const composeSurfaces = surfaces.filter((s) => s.sourceType === "docker_compose");

		// All stableSurfaceKeys must be unique
		const keys = composeSurfaces.map((s) => s.stableSurfaceKey);
		expect(new Set(keys).size).toBe(keys.length);

		provider.close();
	});

	it("compose services have correct sourceSpecificId (serviceName)", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-compose-identity";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "compose-multiservice",
			rootPath: COMPOSE_MULTISERVICE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const apiSurface = surfaces.find(
			(s) => s.sourceType === "docker_compose" && s.displayName === "api",
		);

		expect(apiSurface).toBeDefined();
		expect(apiSurface!.sourceSpecificId).toBe("api");

		provider.close();
	});

	it("image-only compose service produces lower confidence surface", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-compose-image";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "compose-multiservice",
			rootPath: COMPOSE_MULTISERVICE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const redisSurface = surfaces.find(
			(s) => s.sourceType === "docker_compose" && s.displayName === "redis",
		);

		expect(redisSurface).toBeDefined();
		// Image-only should have lower confidence than build services
		expect(redisSurface!.confidence).toBeLessThan(0.85);

		provider.close();
	});
});

// ── dockerfile-plus-compose fixture ────────────────────────────────

describe("surface discovery integration — dockerfile plus compose", () => {
	it("Dockerfile and compose.yml at same root produce distinct surfaces", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-docker-compose";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-plus-compose",
			rootPath: DOCKERFILE_PLUS_COMPOSE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should have both dockerfile and compose surfaces
		const dockerfileSurface = surfaces.find((s) => s.sourceType === "dockerfile");
		const composeSurface = surfaces.find((s) => s.sourceType === "docker_compose");

		expect(dockerfileSurface).toBeDefined();
		expect(composeSurface).toBeDefined();

		// They should have different stableSurfaceKeys
		expect(dockerfileSurface!.stableSurfaceKey).not.toBe(composeSurface!.stableSurfaceKey);

		provider.close();
	});

	it("both surfaces have runtimeKind = container", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "surface-both-container";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "dockerfile-plus-compose",
			rootPath: DOCKERFILE_PLUS_COMPOSE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const containerSurfaces = surfaces.filter((s) => s.runtimeKind === "container");

		// Both dockerfile and compose surfaces should be containers
		expect(containerSurfaces.length).toBeGreaterThanOrEqual(2);

		provider.close();
	});
});

// ── script-only-package fixture ────────────────────────────────────

describe("surface discovery integration — script-only fallback", () => {
	it("script-only build package creates library surface via fallback", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "script-only-build";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "script-only-package",
			rootPath: SCRIPT_ONLY_PACKAGE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should detect exactly one script fallback surface
		const scriptSurfaces = surfaces.filter((s) => s.sourceType === "package_json_scripts");
		expect(scriptSurfaces).toHaveLength(1);

		const libSurface = scriptSurfaces[0];
		expect(libSurface.surfaceKind).toBe("library");
		expect(libSurface.confidence).toBe(0.50);
		expect(libSurface.runtimeKind).toBe("node");

		// Metadata should indicate fallback reason
		const meta = JSON.parse(libSurface.metadataJson!);
		expect(meta.fallbackReason).toBe("script_only_package");

		provider.close();
	});

	it("script-only service package creates backend_service surface via fallback", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "script-only-service";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "script-only-service",
			rootPath: SCRIPT_ONLY_SERVICE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should detect exactly one script fallback surface
		const scriptSurfaces = surfaces.filter((s) => s.sourceType === "package_json_scripts");
		expect(scriptSurfaces).toHaveLength(1);

		const serviceSurface = scriptSurfaces[0];
		expect(serviceSurface.surfaceKind).toBe("backend_service");
		expect(serviceSurface.confidence).toBe(0.55);
		expect(serviceSurface.runtimeKind).toBe("node");

		provider.close();
	});

	it("script fallback surface has valid identity fields", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "script-only-identity";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "script-only-package",
			rootPath: SCRIPT_ONLY_PACKAGE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const scriptSurface = surfaces.find((s) => s.sourceType === "package_json_scripts");

		expect(scriptSurface).toBeDefined();
		// sourceType must be package_json_scripts
		expect(scriptSurface!.sourceType).toBe("package_json_scripts");
		// sourceSpecificId should be the module root
		expect(scriptSurface!.sourceSpecificId).toBeDefined();
		// stableSurfaceKey must be present and 32 chars
		expect(scriptSurface!.stableSurfaceKey).toBeDefined();
		expect(scriptSurface!.stableSurfaceKey).toHaveLength(32);

		provider.close();
	});

	it("script fallback produces deterministic identity on re-index", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "script-only-deterministic";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "script-only-package",
			rootPath: SCRIPT_ONLY_PACKAGE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);

		// First index
		const result1 = await indexer.indexRepo(REPO_UID);
		const surfaces1 = storage.queryProjectSurfaces(result1.snapshotUid);
		const script1 = surfaces1.find((s) => s.sourceType === "package_json_scripts");

		// Second index
		const result2 = await indexer.indexRepo(REPO_UID);
		const surfaces2 = storage.queryProjectSurfaces(result2.snapshotUid);
		const script2 = surfaces2.find((s) => s.sourceType === "package_json_scripts");

		// Both exist
		expect(script1).toBeDefined();
		expect(script2).toBeDefined();

		// stableSurfaceKey must be identical across snapshots
		expect(script2!.stableSurfaceKey).toBe(script1!.stableSurfaceKey);

		provider.close();
	});

	it("script fallback surface has evidence items", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "script-only-evidence";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "script-only-package",
			rootPath: SCRIPT_ONLY_PACKAGE_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
		const scriptSurface = surfaces.find((s) => s.sourceType === "package_json_scripts");
		expect(scriptSurface).toBeDefined();

		const allEvidence = storage.queryAllProjectSurfaceEvidence(result.snapshotUid);
		const scriptEvidence = allEvidence.filter(
			(e) => e.projectSurfaceUid === scriptSurface!.projectSurfaceUid,
		);

		// Should have evidence for each classified script
		expect(scriptEvidence.length).toBeGreaterThanOrEqual(1);

		// Evidence should have script_command kind
		const scriptEv = scriptEvidence.find((e) => e.evidenceKind === "script_command");
		expect(scriptEv).toBeDefined();
		expect(scriptEv!.sourceType).toBe("package_json_scripts");

		provider.close();
	});
});
