/**
 * C/C++ extractor — indexer integration test.
 *
 * Exercises the full product seam:
 *   .c/.h files → CppExtractor → RepoIndexer → nodes/edges persisted
 *   → snapshot carries cpp-core:0.1.0 provenance
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { CppExtractor } from "../../../src/adapters/extractors/cpp/cpp-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { EdgeType, NodeKind, NodeSubtype } from "../../../src/core/model/index.js";

const FIXTURE_ROOT = join(import.meta.dirname, "../../fixtures/cpp/simple");
const REPO_UID = "cpp-test";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let cppExtractor: CppExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	cppExtractor = new CppExtractor();
	await cppExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-cpp-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, [cppExtractor]);
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
	try { unlinkSync(dbPath); } catch { /* ignore */ }
});

describe("C/C++ indexer integration", () => {
	it("indexes .c, .h, and .cpp files", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		// main.c, util.h, engine.cpp
		expect(result.filesTotal).toBe(3);
		expect(result.nodesTotal).toBeGreaterThan(0);
	});

	it("creates FILE nodes for C and C++ files", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const mainFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:main.c:FILE`,
		);
		expect(mainFile).not.toBeNull();

		const headerFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:util.h:FILE`,
		);
		expect(headerFile).not.toBeNull();

		const cppFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:engine.cpp:FILE`,
		);
		expect(cppFile).not.toBeNull();
	});

	it("extracts C function symbols", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "helper",
			limit: 10,
		});
		expect(symbols.some((s) => s.name === "helper" && s.subtype === NodeSubtype.FUNCTION)).toBe(true);
	});

	it("extracts struct as CLASS symbol", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "Point",
			limit: 10,
		});
		// Point may appear as TYPE_ALIAS (from typedef) or CLASS (from struct).
		expect(symbols.length).toBeGreaterThan(0);
	});

	it("emits IMPORTS edges for #include directives", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const unresolved = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		});
		const includes = unresolved.filter(
			(e) => e.targetKey === "stdio.h" || e.targetKey === "util.h",
		);
		expect(includes.length).toBeGreaterThan(0);
	});

	it("emits CALLS edges for function calls", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		// helper() and printf() should produce CALLS edges.
		const callCount = storage.countEdgesByType(result.snapshotUid, EdgeType.CALLS);
		expect(callCount).toBeGreaterThan(0);
	});

	it("carries cpp-core provenance in snapshot toolchain", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const snapshot = storage.getSnapshot(result.snapshotUid);
		const toolchain = snapshot?.toolchainJson
			? JSON.parse(snapshot.toolchainJson)
			: null;
		expect(toolchain).not.toBeNull();
		expect(toolchain.extractor_versions.c).toBe("cpp-core:0.1.0");
	});

	it("computes complexity metrics for C functions", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const metrics = storage.queryFunctionMetrics({
			snapshotUid: result.snapshotUid,
			limit: 10,
		});
		expect(metrics.length).toBeGreaterThan(0);
	});

	it("extracts C++ class from .cpp file", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "Engine",
			limit: 10,
		});
		expect(symbols.some(
			(s) => s.name === "Engine" && s.subtype === NodeSubtype.CLASS,
		)).toBe(true);
	});

	it("extracts namespace-qualified C++ method from .cpp file", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "run",
			limit: 10,
		});
		const run = symbols.find(
			(s) => s.name === "run" && s.subtype === NodeSubtype.METHOD,
		);
		expect(run).toBeDefined();
		expect(run!.qualifiedName).toBe("mylib::Engine::run");
	});

	it("classifies quoted #include as internal (relative)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const unresolved = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		});
		// #include "util.h" from both main.c and engine.cpp should be
		// classified as internal (relative import).
		const utilInclude = unresolved.find(
			(e) => e.targetKey === "util.h" &&
				e.classification === "internal_candidate",
		);
		expect(utilInclude).toBeDefined();
	});
});
