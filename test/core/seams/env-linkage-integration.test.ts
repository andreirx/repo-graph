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

	it("excludes test files from seam detection (production noise filter)", async () => {
		// Regression test for the test-file contamination defect:
		// the seam linkage layer must not attribute env accesses
		// from files under test/, tests/, __tests__, .test., or .spec.
		// to non-test surfaces. The seam contract describes runtime
		// behavior of operational surfaces, not test code.
		const fixtureDir = join(tmpdir(), `rgr-test-noise-${randomUUID()}`);
		mkdirSync(join(fixtureDir, "src"), { recursive: true });
		mkdirSync(join(fixtureDir, "test"), { recursive: true });
		mkdirSync(join(fixtureDir, "src/__tests__"), { recursive: true });

		writeFileSync(join(fixtureDir, "package.json"), JSON.stringify({
			name: "noise-test-app",
			bin: { "noise-test": "./dist/index.js" },
		}));

		// Production file: PROD_VAR must be detected.
		writeFileSync(join(fixtureDir, "src/main.ts"),
			`export const x = process.env.PROD_VAR;\n`,
		);
		// File under test/: TEST_VAR must NOT contribute.
		writeFileSync(join(fixtureDir, "test/run.ts"),
			`export const x = process.env.TEST_ONLY_VAR_A;\n`,
		);
		// File under src/__tests__/: must NOT contribute.
		writeFileSync(join(fixtureDir, "src/__tests__/main.test.ts"),
			`export const x = process.env.TEST_ONLY_VAR_B;\n`,
		);
		// File with .spec. infix: must NOT contribute.
		writeFileSync(join(fixtureDir, "src/main.spec.ts"),
			`export const x = process.env.TEST_ONLY_VAR_C;\n`,
		);

		const dbPath = join(tmpdir(), `rgr-test-noise-db-${randomUUID()}.db`);
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		const storage = new SqliteStorage(provider.getDatabase());
		const repoUid = `noise-test-${randomUUID().slice(0, 8)}`;
		storage.addRepo({
			repoUid,
			name: "noise-test",
			rootPath: fixtureDir,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});
		envs.push({ fixtureDir, dbPath, provider, storage, repoUid });

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(repoUid);

		const allDeps = storage.queryAllSurfaceEnvDependencies(result.snapshotUid);
		const names = new Set(allDeps.map((d) => d.envName));

		// Production var must be present.
		expect(names.has("PROD_VAR")).toBe(true);
		// Test-only vars must be absent.
		expect(names.has("TEST_ONLY_VAR_A")).toBe(false);
		expect(names.has("TEST_ONLY_VAR_B")).toBe(false);
		expect(names.has("TEST_ONLY_VAR_C")).toBe(false);
	});

	it("excludes env-access patterns inside comments (comment masking)", async () => {
		// Regression test for the comment-matching defect:
		// detectors run after a positional comment masker, so
		// process.env.X inside JSDoc, block comments, and line
		// comments must NOT produce env dependency rows. Real
		// production accesses on the same file must still detect.
		const fixtureDir = join(tmpdir(), `rgr-comment-${randomUUID()}`);
		mkdirSync(join(fixtureDir, "src"), { recursive: true });

		writeFileSync(join(fixtureDir, "package.json"), JSON.stringify({
			name: "comment-test-app",
			bin: { "comment-test": "./dist/index.js" },
		}));

		// File with comments containing env-access patterns + a real
		// production access. Only REAL_VAR should be detected.
		// Note: the comment masker does NOT mask string literals —
		// they are preserved because fs detectors rely on string
		// literal contents (e.g., fs.writeFile("real_path")). String-
		// embedded env access patterns are out of scope for this
		// regression test and are a separate, lower-severity class
		// of false positive.
		writeFileSync(join(fixtureDir, "src/main.ts"),
			[
				`/**`,
				` * Documentation example:`,
				` *   process.env.DOC_PHANTOM_A`,
				` *   const { DOC_PHANTOM_B } = process.env`,
				` */`,
				`// Inline comment: process.env.DOC_PHANTOM_C`,
				`/* Block comment: process.env.DOC_PHANTOM_D */`,
				`export const x = process.env.REAL_VAR;`,
				``,
			].join("\n"),
		);

		const dbPath = join(tmpdir(), `rgr-comment-db-${randomUUID()}.db`);
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		const storage = new SqliteStorage(provider.getDatabase());
		const repoUid = `comment-test-${randomUUID().slice(0, 8)}`;
		storage.addRepo({
			repoUid,
			name: "comment-test",
			rootPath: fixtureDir,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});
		envs.push({ fixtureDir, dbPath, provider, storage, repoUid });

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(repoUid);

		const allDeps = storage.queryAllSurfaceEnvDependencies(result.snapshotUid);
		const names = new Set(allDeps.map((d) => d.envName));

		// Real production access must be present.
		expect(names.has("REAL_VAR")).toBe(true);
		// Doc/comment-only phantoms must be absent.
		expect(names.has("DOC_PHANTOM_A")).toBe(false);
		expect(names.has("DOC_PHANTOM_B")).toBe(false);
		expect(names.has("DOC_PHANTOM_C")).toBe(false);
		expect(names.has("DOC_PHANTOM_D")).toBe(false);

		// Also assert that real detector line numbers are stable —
		// REAL_VAR is on line 8 of the file (1-indexed).
		const realDep = allDeps.find((d) => d.envName === "REAL_VAR");
		expect(realDep).toBeDefined();
		const realEvidence = storage.querySurfaceEnvEvidence(realDep!.surfaceEnvDependencyUid);
		expect(realEvidence.length).toBeGreaterThan(0);
		expect(realEvidence[0].lineNumber).toBe(8);
	});

	it("queries env evidence per surface across all dependencies", async () => {
		const env = setupFixtureWithEnvVars();

		const indexer = new RepoIndexer(env.storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(env.repoUid);

		const surfaces = env.storage.queryProjectSurfaces(result.snapshotUid);
		expect(surfaces.length).toBeGreaterThanOrEqual(1);
		const surface = surfaces[0];

		// Per-surface bulk evidence query (the new BySurface method).
		const surfaceEvidence = env.storage.querySurfaceEnvEvidenceBySurface(
			surface.projectSurfaceUid,
		);

		// Should return evidence from BOTH server.ts and config.ts since
		// the fixture has env accesses in both files and both belong to
		// the single CLI surface.
		expect(surfaceEvidence.length).toBeGreaterThanOrEqual(7);
		const files = new Set(surfaceEvidence.map((e) => e.sourceFilePath));
		expect(files.has("src/server.ts")).toBe(true);
		expect(files.has("src/config.ts")).toBe(true);

		// Cross-check: aggregate per-dependency querySurfaceEnvEvidence
		// must equal the bulk per-surface result.
		const surfaceDeps = env.storage.querySurfaceEnvDependencies(
			surface.projectSurfaceUid,
		);
		const aggregated: typeof surfaceEvidence = [];
		for (const d of surfaceDeps) {
			aggregated.push(...env.storage.querySurfaceEnvEvidence(d.surfaceEnvDependencyUid));
		}
		expect(surfaceEvidence.length).toBe(aggregated.length);

		// All returned evidence rows belong to dependencies of this surface.
		const surfaceDepUids = new Set(surfaceDeps.map((d) => d.surfaceEnvDependencyUid));
		for (const e of surfaceEvidence) {
			expect(surfaceDepUids.has(e.surfaceEnvDependencyUid)).toBe(true);
		}
	});
});
