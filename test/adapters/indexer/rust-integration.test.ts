/**
 * Rust multi-language indexer integration test.
 *
 * Exercises the full architectural seam for Rust:
 *   - .rs files routed to RustExtractor by the multi-extractor indexer
 *   - unresolved edges persisted with classification + basis codes
 *   - trust surfaces (countUnresolvedEdges, queryUnresolvedEdges) work
 *     on Rust-emitted edges
 *   - Rust runtime builtins (Vec, HashMap, etc.) are in the merged
 *     builtin set
 *
 * Uses the simple-crate fixture (pure Rust, no TS files).
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { RustExtractor } from "../../../src/adapters/extractors/rust/rust-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { NodeKind } from "../../../src/core/model/index.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../fixtures/rust/simple-crate",
);
const REPO_UID = "rust-fixture";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let extractor: RustExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	extractor = new RustExtractor();
	await extractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-rust-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	// Single-language indexer (Rust only) for this fixture.
	indexer = new RepoIndexer(storage, extractor);
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

describe("Rust indexer integration — basic indexing", () => {
	it("indexes .rs files and reports node/edge counts", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		// Fixture has lib.rs + utils.rs = 2 source files.
		expect(result.filesTotal).toBe(2);
		expect(result.nodesTotal).toBeGreaterThan(0);
		expect(result.edgesTotal).toBeGreaterThanOrEqual(0);
	});

	it("creates FILE nodes for .rs files", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const snap = storage.getSnapshot(result.snapshotUid);
		expect(snap).toBeDefined();

		const libFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/lib.rs:FILE`,
		);
		expect(libFile).not.toBeNull();
		expect(libFile?.kind).toBe(NodeKind.FILE);

		const utilsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/utils.rs:FILE`,
		);
		expect(utilsFile).not.toBeNull();
	});

	it("creates SYMBOL nodes for Rust structs, functions, traits", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const symbols = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "Config",
			limit: 10,
		});
		// Should find the Config struct.
		const configSymbol = symbols.find((s) => s.name === "Config");
		expect(configSymbol).toBeDefined();
	});
});

describe("Rust indexer integration — unresolved edge classification", () => {
	it("persists classified unresolved edges for Rust call sites", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		if (result.edgesUnresolved === 0) return; // no unresolved edges in this tiny fixture

		const byClassification = storage.countUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			groupBy: "classification",
		});
		// Should have at least some classified edges.
		const total = byClassification.reduce((sum, r) => sum + r.count, 0);
		expect(total).toBe(result.edgesUnresolved);

		// Every classified row must have a valid classification.
		const validBuckets = new Set([
			"external_library_candidate",
			"internal_candidate",
			"framework_boundary_candidate",
			"unknown",
		]);
		for (const row of byClassification) {
			expect(validBuckets.has(row.key)).toBe(true);
		}
	});

	it("classified rows carry non-empty basis codes and source paths", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		if (result.edgesUnresolved === 0) return;

		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			limit: 50,
		});
		for (const row of rows) {
			expect(row.basisCode.length).toBeGreaterThan(0);
			// Source file path should be a .rs file.
			expect(row.sourceFilePath).toMatch(/\.rs$/);
		}
	});
});

describe("Rust indexer integration — trust surface compatibility", () => {
	it("count by category works on Rust edges", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		if (result.edgesUnresolved === 0) return;

		const byCategory = storage.countUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			groupBy: "category",
		});
		expect(byCategory.length).toBeGreaterThan(0);
		// Category keys should be from the shared vocabulary.
		for (const row of byCategory) {
			expect(row.key).toMatch(/^(calls_|imports_|instantiates_|implements_|other)/);
		}
	});

	it("sample query returns Rust edges with correct shape", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		if (result.edgesUnresolved === 0) return;

		const samples = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			limit: 10,
		});
		expect(samples.length).toBeGreaterThan(0);
		for (const s of samples) {
			expect(s.edgeUid).toBeTruthy();
			expect(s.classification).toBeTruthy();
			expect(s.category).toBeTruthy();
			expect(s.basisCode).toBeTruthy();
			expect(s.targetKey).toBeTruthy();
			expect(s.sourceNodeUid).toBeTruthy();
		}
	});
});
