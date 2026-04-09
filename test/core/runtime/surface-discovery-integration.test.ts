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
