/**
 * Env dependency linkage integration tests.
 *
 * Creates a temp fixture with env var accesses, indexes it, and
 * verifies persisted env dependencies + evidence linked to surfaces.
 */

import { randomUUID } from "node:crypto";
import { cpSync, mkdirSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
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

function setupFixtureWithEnvVars(): TestEnv {
	const fixtureDir = join(tmpdir(), `rgr-env-${randomUUID()}`);
	mkdirSync(join(fixtureDir, "src"), { recursive: true });

	// package.json with name + bin → CLI surface.
	writeFileSync(join(fixtureDir, "package.json"), JSON.stringify({
		name: "env-test-app",
		bin: { "env-test": "./dist/index.js" },
		scripts: { build: "tsc" },
	}));

	// Source file with env var accesses.
	writeFileSync(join(fixtureDir, "src/server.ts"), `
import express from "express";

const port = process.env.PORT || "3000";
const dbUrl = process.env.DATABASE_URL;
const { NODE_ENV, SECRET_KEY } = process.env;
const debug = process.env["DEBUG"];

const app = express();
app.listen(port);
`);

	// Another file with some env access.
	writeFileSync(join(fixtureDir, "src/config.ts"), `
export const apiKey = process.env.API_KEY ?? "dev-key";
export const dbUrl = process.env.DATABASE_URL;
`);

	const dbPath = join(tmpdir(), `rgr-env-db-${randomUUID()}.db`);
	const provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	const storage = new SqliteStorage(provider.getDatabase());
	const repoUid = `env-test-${randomUUID().slice(0, 8)}`;

	storage.addRepo({
		repoUid,
		name: "env-test",
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

describe("env dependency linkage", () => {
	it("persists env dependencies linked to surfaces", async () => {
		const env = setupFixtureWithEnvVars();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const allDeps = env.storage.queryAllSurfaceEnvDependencies(result.snapshotUid);

		// Should detect PORT, DATABASE_URL, NODE_ENV, SECRET_KEY, DEBUG, API_KEY.
		expect(allDeps.length).toBeGreaterThanOrEqual(5);

		const varNames = allDeps.map((d) => d.envName).sort();
		expect(varNames).toContain("PORT");
		expect(varNames).toContain("DATABASE_URL");
		expect(varNames).toContain("NODE_ENV");
		expect(varNames).toContain("SECRET_KEY");
		expect(varNames).toContain("API_KEY");
	});

	it("links dependencies to the CLI surface", async () => {
		const env = setupFixtureWithEnvVars();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const surfaces = env.storage.queryProjectSurfaces(result.snapshotUid);
		expect(surfaces.length).toBeGreaterThanOrEqual(1);

		const allDeps = env.storage.queryAllSurfaceEnvDependencies(result.snapshotUid);
		const surfaceUids = new Set(surfaces.map((s) => s.projectSurfaceUid));

		// Every dependency should reference a valid surface.
		for (const d of allDeps) {
			expect(surfaceUids.has(d.projectSurfaceUid)).toBe(true);
		}
	});

	it("detects required vs optional access kinds", async () => {
		const env = setupFixtureWithEnvVars();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const allDeps = env.storage.queryAllSurfaceEnvDependencies(result.snapshotUid);

		// PORT has || "3000" → optional.
		const port = allDeps.find((d) => d.envName === "PORT");
		expect(port).toBeDefined();
		expect(port!.accessKind).toBe("optional");

		// DATABASE_URL is bare access (no fallback) → required.
		// But it appears in both files — one required, one required.
		const dbUrl = allDeps.find((d) => d.envName === "DATABASE_URL");
		expect(dbUrl).toBeDefined();
		expect(dbUrl!.accessKind).toBe("required");

		// API_KEY has ?? "dev-key" → optional.
		const apiKey = allDeps.find((d) => d.envName === "API_KEY");
		expect(apiKey).toBeDefined();
		expect(apiKey!.accessKind).toBe("optional");
		expect(apiKey!.defaultValue).toBe("dev-key");
	});

	it("deduplicates same env var across files into one dependency", async () => {
		const env = setupFixtureWithEnvVars();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const allDeps = env.storage.queryAllSurfaceEnvDependencies(result.snapshotUid);

		// DATABASE_URL appears in both server.ts and config.ts.
		// Should be one dependency row, not two.
		const dbUrlDeps = allDeps.filter((d) => d.envName === "DATABASE_URL");
		expect(dbUrlDeps).toHaveLength(1);

		// But should have two evidence rows.
		const evidence = env.storage.querySurfaceEnvEvidence(dbUrlDeps[0].surfaceEnvDependencyUid);
		expect(evidence).toHaveLength(2);
		const files = evidence.map((e) => e.sourceFilePath).sort();
		expect(files).toEqual(["src/config.ts", "src/server.ts"]);
	});
});
