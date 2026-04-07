/**
 * Python extractor — indexer integration test.
 *
 * Exercises the full product seam:
 *   .py file → PythonExtractor → RepoIndexer → nodes/edges persisted
 *   → snapshot carries python-core provenance
 *
 * Uses the test/fixtures/python/simple fixture.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { PythonExtractor } from "../../../src/adapters/extractors/python/python-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { EdgeType, NodeKind, NodeSubtype } from "../../../src/core/model/index.js";

const FIXTURE_ROOT = join(import.meta.dirname, "../../fixtures/python/simple");
const REPO_UID = "python-test";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let pythonExtractor: PythonExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	pythonExtractor = new PythonExtractor();
	await pythonExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-python-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, [pythonExtractor]);
	storage.addRepo({
		repoUid: REPO_UID,
		name: REPO_UID,
		rootPath: FIXTURE_ROOT,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore
	}
});

describe("Python indexer integration", () => {
	it("indexes .py files and produces nodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		expect(result.filesTotal).toBe(2);
		expect(result.nodesTotal).toBeGreaterThan(0);
	});

	it("creates FILE nodes for Python files", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const mainFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:main.py:FILE`,
		);
		expect(mainFile).not.toBeNull();

		const utilsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:utils.py:FILE`,
		);
		expect(utilsFile).not.toBeNull();
	});

	it("extracts Python class symbols", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "UserService",
			limit: 5,
		});
		expect(symbols.some((s) => s.name === "UserService" && s.subtype === NodeSubtype.CLASS)).toBe(true);
	});

	it("extracts Python function symbols", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "process_items",
			limit: 5,
		});
		expect(symbols.some((s) => s.name === "process_items" && s.subtype === NodeSubtype.FUNCTION)).toBe(true);
	});

	it("extracts Python method symbols with qualified names", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "get_user",
			limit: 5,
		});
		const method = symbols.find((s) => s.name === "get_user");
		expect(method).toBeDefined();
		expect(method!.qualifiedName).toBe("UserService.get_user");
	});

	it("emits IMPORTS edges for Python imports", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const unresolved = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		});
		// main.py imports: os, typing, .utils
		const importEdges = unresolved.filter(
			(e) => e.sourceFilePath === "main.py",
		);
		expect(importEdges.length).toBeGreaterThan(0);
	});

	it("extracts top-level variable symbols", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "API_URL",
			limit: 5,
		});
		expect(symbols.some((s) => s.name === "API_URL" && s.subtype === NodeSubtype.VARIABLE)).toBe(true);
	});

	it("carries python-core provenance in snapshot toolchain", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const snapshot = storage.getSnapshot(result.snapshotUid);
		expect(snapshot).not.toBeNull();
		// The toolchain JSON should include python extractor version.
		// It's stored in the snapshot metadata.
		const toolchain = snapshot!.toolchainJson
			? JSON.parse(snapshot!.toolchainJson)
			: null;
		expect(toolchain).not.toBeNull();
		expect(toolchain.extractor_versions.python).toBe("python-core:0.1.0");
	});

	it("computes complexity metrics for Python functions", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const metrics = storage.queryFunctionMetrics({
			snapshotUid: result.snapshotUid,
			limit: 10,
		});
		// process_items has a for loop → CC > 1.
		const processMetric = metrics.find((m) => m.symbol.includes("process_items"));
		expect(processMetric).toBeDefined();
		expect(processMetric!.cyclomaticComplexity).toBeGreaterThanOrEqual(2);
	});
});
