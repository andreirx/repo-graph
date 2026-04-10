/**
 * Filesystem mutation integration tests.
 *
 * Creates a temp fixture with mutation calls, indexes it, and
 * verifies persisted identity + evidence rows linked to surfaces.
 */

import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { ManifestScanner } from "../../../src/adapters/discovery/manifest-scanner.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";

let extractor: TypeScriptExtractor;
let scanner: ManifestScanner;

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
	scanner = new ManifestScanner();
});

interface TestEnv {
	fixtureDir: string;
	dbPath: string;
	provider: SqliteConnectionProvider;
	storage: SqliteStorage;
	repoUid: string;
}

const envs: TestEnv[] = [];

function setupFixture(): TestEnv {
	const fixtureDir = join(tmpdir(), `rgr-fs-${randomUUID()}`);
	mkdirSync(join(fixtureDir, "src"), { recursive: true });

	writeFileSync(join(fixtureDir, "package.json"), JSON.stringify({
		name: "fs-test-app",
		bin: { "fs-test": "./dist/index.js" },
		scripts: { build: "tsc" },
	}));

	// File 1: writes app.log, deletes cache, mkdir uploads.
	writeFileSync(join(fixtureDir, "src/server.ts"), `
import * as fs from "node:fs";

fs.writeFileSync("logs/app.log", "boot");
fs.unlinkSync("data/cache.json");
fs.mkdirSync("uploads", { recursive: true });
`);

	// File 2: also writes app.log (dedup test) + dynamic path.
	writeFileSync(join(fixtureDir, "src/logger.ts"), `
import * as fs from "node:fs";

fs.writeFile("logs/app.log", entry, cb);
fs.writeFile(dynamicTarget, data, cb);
`);

	// File 3: same path with different mutation kind (separate identity).
	writeFileSync(join(fixtureDir, "src/cleanup.ts"), `
import * as fs from "node:fs";

fs.writeFileSync("data/state.json", state);
fs.unlinkSync("data/state.json");
`);

	// File 4: rename with destination capture.
	writeFileSync(join(fixtureDir, "src/atomic.ts"), `
import * as fs from "node:fs";

fs.renameSync("tmp/staging.txt", "data/final.txt");
`);

	const dbPath = join(tmpdir(), `rgr-fs-db-${randomUUID()}.db`);
	const provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	const storage = new SqliteStorage(provider.getDatabase());
	const repoUid = `fs-test-${randomUUID().slice(0, 8)}`;

	storage.addRepo({
		repoUid,
		name: "fs-test",
		rootPath: fixtureDir,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});

	const env = { fixtureDir, dbPath, provider, storage, repoUid };
	envs.push(env);
	return env;
}

afterEach(() => {
	for (const env of envs) {
		env.provider.close();
		try { unlinkSync(env.dbPath); } catch { /* ignore */ }
		try { rmSync(env.fixtureDir, { recursive: true, force: true }); } catch { /* ignore */ }
	}
	envs.length = 0;
});

describe("fs mutation linkage integration", () => {
	it("persists literal mutations as identity rows linked to surfaces", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const surfaces = env.storage.queryProjectSurfaces(result.snapshotUid);
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		const identities = env.storage.queryAllSurfaceFsMutations(result.snapshotUid);

		// Expected literal-path identities:
		//   logs/app.log + write_file
		//   data/cache.json + delete_path
		//   uploads + create_dir
		//   data/state.json + write_file
		//   data/state.json + delete_path
		expect(identities.length).toBeGreaterThanOrEqual(5);

		const targets = identities.map((i) => `${i.targetPath}|${i.mutationKind}`).sort();
		expect(targets).toContain("logs/app.log|write_file");
		expect(targets).toContain("data/cache.json|delete_path");
		expect(targets).toContain("uploads|create_dir");
		expect(targets).toContain("data/state.json|write_file");
		expect(targets).toContain("data/state.json|delete_path");

		// Every identity references a valid surface.
		const surfaceUids = new Set(surfaces.map((s) => s.projectSurfaceUid));
		for (const i of identities) {
			expect(surfaceUids.has(i.projectSurfaceUid)).toBe(true);
		}
	});

	it("dedups same path + same kind across multiple files", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const identities = env.storage.queryAllSurfaceFsMutations(result.snapshotUid);

		// logs/app.log + write_file appears in server.ts AND logger.ts.
		// Should be ONE identity row.
		const appLogWrites = identities.filter(
			(i) => i.targetPath === "logs/app.log" && i.mutationKind === "write_file",
		);
		expect(appLogWrites).toHaveLength(1);

		// But should have TWO evidence rows.
		const evidence = env.storage.querySurfaceFsMutationEvidence(
			appLogWrites[0].surfaceFsMutationUid,
		);
		expect(evidence).toHaveLength(2);
		const files = evidence.map((e) => e.sourceFilePath).sort();
		expect(files).toEqual(["src/logger.ts", "src/server.ts"]);
	});

	it("keeps same path + different kind as separate identity rows", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const identities = env.storage.queryAllSurfaceFsMutations(result.snapshotUid);

		// data/state.json has BOTH write_file and delete_path → 2 rows.
		const stateRows = identities.filter((i) => i.targetPath === "data/state.json");
		expect(stateRows).toHaveLength(2);
		const kinds = stateRows.map((r) => r.mutationKind).sort();
		expect(kinds).toEqual(["delete_path", "write_file"]);
	});

	it("dynamic-path occurrences produce evidence-only rows", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const allEvidence = env.storage.queryAllSurfaceFsMutationEvidence(result.snapshotUid);
		const dynamicEvidence = allEvidence.filter((e) => e.dynamicPath);

		// logger.ts has fs.writeFile(dynamicTarget, ...) → 1 dynamic evidence.
		expect(dynamicEvidence.length).toBeGreaterThanOrEqual(1);

		// Dynamic evidence should have null surfaceFsMutationUid.
		for (const e of dynamicEvidence) {
			expect(e.surfaceFsMutationUid).toBeNull();
		}
	});

	it("preserves rename destination in identity metadata", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const identities = env.storage.queryAllSurfaceFsMutations(result.snapshotUid);
		const renameRow = identities.find(
			(i) => i.targetPath === "tmp/staging.txt" && i.mutationKind === "rename_path",
		);
		expect(renameRow).toBeDefined();
		expect(renameRow!.metadataJson).toBeTruthy();
		const meta = JSON.parse(renameRow!.metadataJson!);
		expect(meta.destinationPaths).toEqual(["data/final.txt"]);
	});

	it("queries fs mutation evidence per surface (literal + dynamic)", async () => {
		const env = setupFixture();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const surfaces = env.storage.queryProjectSurfaces(result.snapshotUid);
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		// Find the surface that owns the fixture files (the CLI surface).
		const surface = surfaces[0];
		const evidence = env.storage.querySurfaceFsMutationEvidenceBySurface(
			surface.projectSurfaceUid,
		);

		// Should include both literal and dynamic evidence for this surface.
		expect(evidence.length).toBeGreaterThan(0);
		const hasDynamic = evidence.some((e) => e.dynamicPath);
		const hasLiteral = evidence.some((e) => !e.dynamicPath);
		expect(hasDynamic).toBe(true);
		expect(hasLiteral).toBe(true);

		// All evidence belongs to the queried surface.
		for (const e of evidence) {
			expect(e.projectSurfaceUid).toBe(surface.projectSurfaceUid);
		}
	});
});
