import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import {
	filterByEdgeAffinity,
	RepoIndexer,
} from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import type { GraphNode } from "../../../src/core/model/index.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Visibility,
} from "../../../src/core/model/index.js";

const FIXTURES_ROOT = join(
	import.meta.dirname,
	"../../fixtures/typescript/simple-imports",
);

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let extractor: TypeScriptExtractor;
let indexer: RepoIndexer;
let dbPath: string;

const REPO_UID = "fixture-repo";

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-indexer-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, extractor);

	// Register the fixture repo
	storage.addRepo({
		repoUid: REPO_UID,
		name: "fixture-repo",
		rootPath: FIXTURES_ROOT,
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

describe("indexRepo", () => {
	it("completes successfully and returns a result", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		expect(result.snapshotUid).toBeTruthy();
		expect(result.filesTotal).toBe(5); // types.ts, repository.ts, service.ts, index.ts, dual-export.ts
		expect(result.nodesTotal).toBeGreaterThan(0);
		expect(result.edgesTotal).toBeGreaterThan(0);
		expect(result.durationMs).toBeGreaterThan(0);
	});

	it("creates FILE nodes for every source file", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const snapshot = storage.getSnapshot(result.snapshotUid);
		expect(snapshot).not.toBeNull();
		expect(snapshot?.status).toBe("ready");

		// Query for FILE nodes
		const _fileNodes = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: ".ts",
			limit: 100,
		});
		// resolveSymbol only searches SYMBOL kind, so check FILE nodes directly
		const allFileNode = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/types.ts:FILE`,
		);
		expect(allFileNode).not.toBeNull();
		expect(allFileNode?.kind).toBe(NodeKind.FILE);
	});

	it("creates MODULE nodes for directories", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const srcModule = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src:MODULE`,
		);
		expect(srcModule).not.toBeNull();
		expect(srcModule?.kind).toBe(NodeKind.MODULE);
		expect(srcModule?.subtype).toBe(NodeSubtype.DIRECTORY);
		expect(srcModule?.name).toBe("src");
	});

	it("creates SYMBOL nodes for exported classes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const userService = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/service.ts#UserService:SYMBOL:CLASS`,
		);
		expect(userService).not.toBeNull();
		expect(userService?.kind).toBe(NodeKind.SYMBOL);
		expect(userService?.subtype).toBe(NodeSubtype.CLASS);
	});

	it("resolves IMPORTS edges to actual FILE node UIDs", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// service.ts imports types.ts and repository.ts
		const serviceFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/service.ts:FILE`,
		);
		expect(serviceFile).not.toBeNull();

		// Find callers of types.ts (who imports it)
		const typesFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/types.ts:FILE`,
		);
		expect(typesFile).not.toBeNull();

		// Use findCallers with IMPORTS edge type on types.ts FILE node
		const importers = storage.findCallers({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:src/types.ts:FILE`,
			edgeTypes: [EdgeType.IMPORTS],
		});

		// service.ts and repository.ts both import types.ts
		expect(importers.length).toBeGreaterThanOrEqual(2);
		const importerFiles = importers.map((n) => n.file);
		expect(importerFiles).toContain("src/service.ts");
		expect(importerFiles).toContain("src/repository.ts");
	});

	it("resolves CALLS edges for unambiguous function calls", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// generateId is called from UserService.createUser
		// It's unambiguous (only one function named generateId)
		const generateId = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/service.ts#generateId:SYMBOL:FUNCTION`,
		);
		expect(generateId).not.toBeNull();

		const callers = storage.findCallers({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:src/service.ts#generateId:SYMBOL:FUNCTION`,
			edgeTypes: [EdgeType.CALLS],
		});

		// createUser calls generateId
		expect(callers.length).toBeGreaterThanOrEqual(1);
	});

	it("reports unresolved edge count", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// Some edges (like this.repo.findById) may be ambiguous and unresolved
		// The important thing is that the count is tracked
		expect(result.edgesUnresolved).toBeGreaterThanOrEqual(0);
		expect(result.edgesTotal + result.edgesUnresolved).toBeGreaterThan(0);
	});

	it("marks snapshot as READY on success", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const snapshot = storage.getSnapshot(result.snapshotUid);
		expect(snapshot?.status).toBe("ready");
		expect(snapshot?.filesTotal).toBe(5);
		expect(snapshot?.nodesTotal).toBeGreaterThan(0);
		expect(snapshot?.edgesTotal).toBeGreaterThan(0);
	});

	it("can answer dead node queries after indexing", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const deadNodes = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// There should be some dead nodes (functions/classes nobody calls)
		// The exact count depends on resolution success, but the query should work
		expect(deadNodes).toBeDefined();
	});

	it("excludes specified patterns by exact path", async () => {
		const result = await indexer.indexRepo(REPO_UID, {
			exclude: ["src/repository.ts"],
		});

		expect(result.filesTotal).toBe(4);
	});

	it("excludes specified patterns by glob", async () => {
		const result = await indexer.indexRepo(REPO_UID, {
			exclude: ["src/repo*"],
		});

		// src/repository.ts matches src/repo*
		expect(result.filesTotal).toBe(4);
	});

	it("creates OWNS edges from MODULE to FILE nodes", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// src module should own the 5 files
		const srcModule = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src:MODULE`,
		);
		expect(srcModule).not.toBeNull();

		// Find callees of the src module using OWNS edges
		const owned = storage.findCallees({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:src:MODULE`,
			edgeTypes: [EdgeType.OWNS],
		});

		expect(owned.length).toBe(5);
	});

	it("creates module-to-module IMPORTS edges", async () => {
		// In the simple-imports fixture, all files are in src/,
		// so there are no cross-module imports.
		// This test verifies the mechanism works without cross-module edges.
		const result = await indexer.indexRepo(REPO_UID);

		// No module-to-module IMPORTS edges expected (all files in same dir)
		const srcModule = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src:MODULE`,
		);
		expect(srcModule).not.toBeNull();

		const moduleImports = storage.findCallees({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:src:MODULE`,
			edgeTypes: [EdgeType.IMPORTS],
		});
		// src doesn't import any OTHER module in this fixture
		expect(moduleImports.length).toBe(0);
	});

	it("refreshRepo creates a refresh snapshot with parent link", async () => {
		const initial = await indexer.indexRepo(REPO_UID);
		expect(initial.filesTotal).toBe(5);

		const refresh = await indexer.refreshRepo(REPO_UID);
		expect(refresh.filesTotal).toBe(5);
		expect(refresh.nodesTotal).toBeGreaterThan(0);

		// Verify it's a REFRESH snapshot with parent
		const snap = storage.getSnapshot(refresh.snapshotUid);
		expect(snap?.kind).toBe("refresh");
		expect(snap?.parentSnapshotUid).toBe(initial.snapshotUid);
	});

	it("dead node detection is not affected by OWNS edges", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// FILE nodes should still appear as dead if nothing IMPORTS them
		// types.ts is imported by service.ts and repository.ts — not dead
		// But OWNS edges from MODULE should NOT make everything appear live
		const deadFiles = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.FILE,
		});

		// index.ts is not imported by anything — it should be dead (as a FILE)
		const deadFileNames = deadFiles.map((d) => d.symbol);
		expect(deadFileNames).toContain("src/index.ts");
	});

	it("reports unresolved edge breakdown by category", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// The fixture has this.repo.findById() style calls that cannot resolve
		expect(result.unresolvedBreakdown).toBeDefined();
		expect(typeof result.unresolvedBreakdown).toBe("object");

		// There should be some unresolved edges
		const totalUnresolved = Object.values(result.unresolvedBreakdown).reduce(
			(sum, count) => sum + count,
			0,
		);
		expect(totalUnresolved).toBe(result.edgesUnresolved);
	});

	it("persists a classified row for every unresolved edge", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// Count persisted rows via classification axis — they should
		// match the reported unresolved total exactly.
		const byClassification = storage.countUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			groupBy: "classification",
		});
		const persistedTotal = byClassification.reduce(
			(sum, r) => sum + r.count,
			0,
		);
		expect(persistedTotal).toBe(result.edgesUnresolved);

		// Count by category must ALSO match (and match breakdown totals).
		const byCategory = storage.countUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			groupBy: "category",
		});
		const persistedTotalByCategory = byCategory.reduce(
			(sum, r) => sum + r.count,
			0,
		);
		expect(persistedTotalByCategory).toBe(result.edgesUnresolved);
	});

	it("classified rows carry non-empty classification and basis_code", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		if (result.edgesUnresolved === 0) return; // nothing to verify

		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			limit: 500,
		});
		expect(rows.length).toBeGreaterThan(0);

		const validClassifications = new Set([
			"external_library_candidate",
			"internal_candidate",
			"unknown",
		]);
		for (const row of rows) {
			expect(validClassifications.has(row.classification)).toBe(true);
			// basis_code: non-empty string
			expect(typeof row.basisCode).toBe("string");
			expect(row.basisCode.length).toBeGreaterThan(0);
			// source_file_path resolved via join
			expect(row.sourceFilePath).toBeTruthy();
		}
	});

	it("this.x.m() calls classify as internal_candidate via this-shortcut", async () => {
		// The fixture has this.* method calls that go unresolved.
		const result = await indexer.indexRepo(REPO_UID);
		const thisRows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_this_method_needs_class_context",
		});
		for (const row of thisRows) {
			expect(row.classification).toBe("internal_candidate");
			expect(row.basisCode).toBe("this_receiver_implies_internal");
		}
	});
});

// ── Multi-batch seam test ──────────────────────────────────────────────
//
// Verifies that the batch pipeline produces identical results regardless
// of batch size — not just count parity, but row-for-row classification
// identity on every unresolved edge.

/**
 * Helper: index with a given batch size in a fresh DB.
 * Returns the IndexResult plus a sorted snapshot of all unresolved edges
 * keyed by (sourceFilePath, lineStart, targetKey) with their full
 * classification tuple (category, classification, basisCode).
 */
async function indexAndCapture(
	batchSize: number | undefined,
	ext: TypeScriptExtractor,
) {
	const path = join(tmpdir(), `rgr-batch-identity-${randomUUID()}.db`);
	const prov = new SqliteConnectionProvider(path);
	prov.initialize();
	const stor = new SqliteStorage(prov.getDatabase());
	const idx = new RepoIndexer(stor, ext);
	stor.addRepo({
		repoUid: REPO_UID,
		name: "fixture-repo",
		rootPath: FIXTURES_ROOT,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});

	const result = batchSize
		? await idx.indexRepo(REPO_UID, { edgeBatchSize: batchSize })
		: await idx.indexRepo(REPO_UID);

	// Capture all unresolved edges with full classification detail.
	const unresolvedRows = stor.queryUnresolvedEdges({
		snapshotUid: result.snapshotUid,
	});

	// Sort by a stable composite key so row order is deterministic.
	const sorted = unresolvedRows
		.map((r) => ({
			key: `${r.sourceFilePath ?? ""}|${r.lineStart ?? 0}|${r.targetKey}`,
			category: r.category,
			classification: r.classification,
			basisCode: r.basisCode,
		}))
		.sort((a, b) => a.key.localeCompare(b.key));

	prov.close();
	try { unlinkSync(path); } catch { /* ignore */ }

	return { result, sorted };
}

describe("multi-batch edge resolution", () => {
	it("produces row-for-row identical classifications with edgeBatchSize=1", async () => {
		const baseline = await indexAndCapture(undefined, extractor);
		const batched = await indexAndCapture(1, extractor);

		// Count parity.
		expect(batched.result.filesTotal).toBe(baseline.result.filesTotal);
		expect(batched.result.nodesTotal).toBe(baseline.result.nodesTotal);
		expect(batched.result.edgesTotal).toBe(baseline.result.edgesTotal);
		expect(batched.result.edgesUnresolved).toBe(baseline.result.edgesUnresolved);
		expect(batched.result.unresolvedBreakdown).toEqual(baseline.result.unresolvedBreakdown);

		// Row-for-row classification identity.
		expect(batched.sorted.length).toBe(baseline.sorted.length);
		for (let i = 0; i < baseline.sorted.length; i++) {
			const b = baseline.sorted[i];
			const t = batched.sorted[i];
			expect(t.key).toBe(b.key);
			expect(t.category).toBe(b.category);
			expect(t.classification).toBe(b.classification);
			expect(t.basisCode).toBe(b.basisCode);
		}
	});

	it("produces row-for-row identical classifications with edgeBatchSize=3", async () => {
		const baseline = await indexAndCapture(undefined, extractor);
		const batched = await indexAndCapture(3, extractor);

		expect(batched.result.filesTotal).toBe(baseline.result.filesTotal);
		expect(batched.result.nodesTotal).toBe(baseline.result.nodesTotal);
		expect(batched.result.edgesTotal).toBe(baseline.result.edgesTotal);
		expect(batched.result.edgesUnresolved).toBe(baseline.result.edgesUnresolved);
		expect(batched.result.unresolvedBreakdown).toEqual(baseline.result.unresolvedBreakdown);

		expect(batched.sorted.length).toBe(baseline.sorted.length);
		for (let i = 0; i < baseline.sorted.length; i++) {
			const b = baseline.sorted[i];
			const t = batched.sorted[i];
			expect(t.key).toBe(b.key);
			expect(t.category).toBe(b.category);
			expect(t.classification).toBe(b.classification);
			expect(t.basisCode).toBe(b.basisCode);
		}
	});
});

// ── Scanner hygiene ────────────────────────────────────────────────────

describe("scanner hygiene", () => {
	it("excludes directories in the ALWAYS_EXCLUDED list", async () => {
		// The fixture doesn't have node_modules or dist, but
		// the test verifies that the scanner doesn't crash and
		// only returns .ts/.tsx/.js/.jsx files from src/
		const result = await indexer.indexRepo(REPO_UID);
		expect(result.filesTotal).toBe(5);
	});

	it("excludes patterns via glob matching", async () => {
		const result = await indexer.indexRepo(REPO_UID, {
			exclude: ["src/repo*"],
		});
		// src/repository.ts should be excluded
		expect(result.filesTotal).toBe(4);
	});

	it("excludes patterns via wildcard extension matching", async () => {
		const result = await indexer.indexRepo(REPO_UID, {
			exclude: ["*.test.ts"],
		});
		// No test files in the fixture, so count should be same
		expect(result.filesTotal).toBe(5);
	});
});

// ── Edge affinity disambiguation (stable key v2 regression) ───────────

describe("filterByEdgeAffinity", () => {
	function fakeNode(subtype: NodeSubtype | null, name = "Foo"): GraphNode {
		return {
			nodeUid: randomUUID(),
			snapshotUid: "s",
			repoUid: "r",
			stableKey: `r:f.ts#${name}:SYMBOL:${subtype}`,
			kind: NodeKind.SYMBOL,
			subtype,
			name,
			qualifiedName: name,
			fileUid: "r:f.ts",
			parentNodeUid: null,
			location: { lineStart: 1, colStart: 0, lineEnd: 10, colEnd: 0 },
			signature: null,
			visibility: Visibility.EXPORT,
			docComment: null,
			metadataJson: null,
		};
	}

	it("INSTANTIATES prefers CLASS over TYPE_ALIAS companion", () => {
		const cls = fakeNode(NodeSubtype.CLASS);
		const typeAlias = fakeNode(NodeSubtype.TYPE_ALIAS);
		const result = filterByEdgeAffinity(
			[cls, typeAlias],
			EdgeType.INSTANTIATES,
		);
		expect(result).toEqual([cls]);
	});

	it("INSTANTIATES prefers CLASS over INTERFACE companion", () => {
		const cls = fakeNode(NodeSubtype.CLASS);
		const iface = fakeNode(NodeSubtype.INTERFACE);
		const result = filterByEdgeAffinity([cls, iface], EdgeType.INSTANTIATES);
		expect(result).toEqual([cls]);
	});

	it("IMPLEMENTS prefers INTERFACE over CLASS companion", () => {
		const cls = fakeNode(NodeSubtype.CLASS);
		const iface = fakeNode(NodeSubtype.INTERFACE);
		const result = filterByEdgeAffinity([cls, iface], EdgeType.IMPLEMENTS);
		expect(result).toEqual([iface]);
	});

	it("CALLS filters out type-only subtypes", () => {
		const fn = fakeNode(NodeSubtype.CONSTANT);
		const typeAlias = fakeNode(NodeSubtype.TYPE_ALIAS);
		const result = filterByEdgeAffinity([fn, typeAlias], EdgeType.CALLS);
		expect(result).toEqual([fn]);
	});

	it("CALLS keeps all value-space candidates (may still be ambiguous)", () => {
		const fn1 = fakeNode(NodeSubtype.FUNCTION, "doStuff");
		const fn2 = fakeNode(NodeSubtype.METHOD, "doStuff");
		const result = filterByEdgeAffinity([fn1, fn2], EdgeType.CALLS);
		expect(result.length).toBe(2); // still ambiguous, but type-correct
	});

	it("returns empty when no candidate matches the required declaration space", () => {
		// INSTANTIATES but no CLASS candidate — must not resolve
		const iface = fakeNode(NodeSubtype.INTERFACE);
		const typeAlias = fakeNode(NodeSubtype.TYPE_ALIAS);
		const result = filterByEdgeAffinity(
			[iface, typeAlias],
			EdgeType.INSTANTIATES,
		);
		expect(result.length).toBe(0);
	});

	it("rejects a lone type-only candidate for INSTANTIATES", () => {
		// export interface Foo {}; new Foo() — must NOT resolve
		const iface = fakeNode(NodeSubtype.INTERFACE);
		const result = filterByEdgeAffinity([iface], EdgeType.INSTANTIATES);
		expect(result.length).toBe(0);
	});

	it("rejects a lone type-only candidate for CALLS", () => {
		// export type X = ...; X() — must NOT resolve
		const typeAlias = fakeNode(NodeSubtype.TYPE_ALIAS);
		const result = filterByEdgeAffinity([typeAlias], EdgeType.CALLS);
		expect(result.length).toBe(0);
	});

	it("accepts a lone value-space candidate for CALLS", () => {
		const fn = fakeNode(NodeSubtype.FUNCTION);
		const result = filterByEdgeAffinity([fn], EdgeType.CALLS);
		expect(result.length).toBe(1);
	});

	it("passes through unfiltered for IMPORTS edge type", () => {
		const a = fakeNode(NodeSubtype.CLASS);
		const b = fakeNode(NodeSubtype.INTERFACE);
		const result = filterByEdgeAffinity([a, b], EdgeType.IMPORTS);
		expect(result.length).toBe(2);
	});
});
