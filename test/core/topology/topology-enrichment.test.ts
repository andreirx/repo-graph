/**
 * Topology enrichment integration tests.
 *
 * Uses the mixed-lang fixture (has express dep → backend_service surface,
 * Cargo.toml → library surface) to verify that indexing produces
 * config root and entrypoint links for detected project surfaces.
 *
 * Assertions are unconditional — the mixed-lang fixture is known to
 * produce surfaces.
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
	const dbPath = join(tmpdir(), `rgr-topology-${randomUUID()}.db`);
	dbPaths.push(dbPath);
	const provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	const storage = new SqliteStorage(provider.getDatabase());
	return { storage, provider };
}

describe("topology enrichment — config roots", () => {
	it("persists config root links from evidence source paths", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "topo-config";

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
		// mixed-lang has express dep → backend_service. Unconditional.
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		const allConfigRoots = storage.queryAllSurfaceConfigRoots(result.snapshotUid);
		// At least one config root (package.json from the surface evidence).
		expect(allConfigRoots.length).toBeGreaterThanOrEqual(1);

		// Every config root references a valid surface.
		const surfaceUids = new Set(surfaces.map((s) => s.projectSurfaceUid));
		for (const cr of allConfigRoots) {
			expect(surfaceUids.has(cr.projectSurfaceUid)).toBe(true);
			expect(cr.configPath).toBeTruthy();
			expect(cr.configKind).toBeTruthy();
			expect(cr.confidence).toBeGreaterThan(0);
		}

		// package.json should be among the config roots.
		const pkgJsonRoots = allConfigRoots.filter((cr) => cr.configKind === "package_json");
		expect(pkgJsonRoots.length).toBeGreaterThanOrEqual(1);

		provider.close();
	});
});

describe("topology enrichment — entrypoints", () => {
	it("persists entrypoint links for surfaces", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "topo-entry";

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
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		const allEntrypoints = storage.queryAllSurfaceEntrypoints(result.snapshotUid);

		// Every entrypoint references a valid surface.
		const surfaceUids = new Set(surfaces.map((s) => s.projectSurfaceUid));
		for (const ep of allEntrypoints) {
			expect(surfaceUids.has(ep.projectSurfaceUid)).toBe(true);
			expect(ep.entrypointKind).toBeTruthy();
			expect(ep.confidence).toBeGreaterThan(0);
		}

		// Per-surface query should match the snapshot-wide results.
		for (const s of surfaces) {
			const surfaceEntrypoints = storage.querySurfaceEntrypoints(s.projectSurfaceUid);
			const fromAll = allEntrypoints.filter((e) => e.projectSurfaceUid === s.projectSurfaceUid);
			expect(surfaceEntrypoints.length).toBe(fromAll.length);
		}

		provider.close();
	});
});
