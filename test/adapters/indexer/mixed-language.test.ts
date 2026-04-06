/**
 * Mixed-language indexer integration test.
 *
 * Exercises the actual multi-extractor architectural seam:
 *   - BOTH TypeScript and Rust extractors registered
 *   - .ts file routed to TS extractor, .rs file to Rust extractor
 *   - TS file gets package.json deps (express)
 *   - Rust file gets Cargo.toml deps (serde, wgpu)
 *   - NO cross-contamination: TS file does NOT see Cargo deps,
 *     Rust file does NOT see package.json deps
 *   - merged builtins include BOTH TS and Rust globals
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { RustExtractor } from "../../../src/adapters/extractors/rust/rust-extractor.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { NodeKind } from "../../../src/core/model/index.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../fixtures/mixed-lang",
);
const REPO_UID = "mixed-lang";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let tsExtractor: TypeScriptExtractor;
let rustExtractor: RustExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	tsExtractor = new TypeScriptExtractor();
	await tsExtractor.initialize();
	rustExtractor = new RustExtractor();
	await rustExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-mixed-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	// Multi-extractor indexer — the architectural seam under test.
	indexer = new RepoIndexer(storage, [tsExtractor, rustExtractor]);
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

describe("mixed-language indexer — routing", () => {
	it("indexes both .ts and .rs files in the same repo", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		expect(result.filesTotal).toBe(2);
		expect(result.nodesTotal).toBeGreaterThan(0);

		const tsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/server.ts:FILE`,
		);
		expect(tsFile).not.toBeNull();
		expect(tsFile?.kind).toBe(NodeKind.FILE);

		const rsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/engine.rs:FILE`,
		);
		expect(rsFile).not.toBeNull();
		expect(rsFile?.kind).toBe(NodeKind.FILE);
	});

	it("emits TS symbols from .ts file and Rust symbols from .rs file", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// TS: should have "start" function.
		const tsSymbol = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "start",
			limit: 5,
		});
		expect(tsSymbol.some((s) => s.name === "start")).toBe(true);

		// Rust: should have "GameState" struct.
		const rustSymbol = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "GameState",
			limit: 5,
		});
		expect(rustSymbol.some((s) => s.name === "GameState")).toBe(true);
	});
});

describe("mixed-language indexer — dependency isolation", () => {
	it("TS file classifies express as external (from package.json)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const tsEdges = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath === "src/server.ts");

		const expressEdge = tsEdges.find(
			(r) => r.targetKey === "express" &&
				r.classification === "external_library_candidate",
		);
		expect(expressEdge).toBeDefined();
	});

	it("Rust file does NOT see express as a dependency", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rustEdges = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath === "src/engine.rs");

		// No Rust edge should be classified as external via express.
		const expressLeaks = rustEdges.filter(
			(r) => r.classification === "external_library_candidate" &&
				r.basisCode === "callee_matches_external_import" &&
				r.targetKey.includes("express"),
		);
		expect(expressLeaks).toEqual([]);
	});

	it("Rust file classifies Serialize.serialize as external via Cargo.toml deps", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rustEdges = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath === "src/engine.rs");

		// The fixture calls `Serialize::serialize(...)` which the extractor
		// emits as targetKey "Serialize.serialize". The import binding
		// is identifier=Serialize, specifier=serde. The Cargo.toml has
		// serde in [dependencies]. The classifier should:
		//   - find the binding for "Serialize"
		//   - match specifier "serde" against Cargo deps
		//   - classify as external_library_candidate
		const serializeCall = rustEdges.find(
			(r) => r.targetKey === "Serialize.serialize" ||
				r.targetKey.includes("Serialize"),
		);
		expect(serializeCall).toBeDefined();
		expect(serializeCall?.classification).toBe("external_library_candidate");
		expect(serializeCall?.basisCode).toBe("receiver_matches_external_import");

		// No Rust edge should reference express (TS dep isolation).
		for (const edge of rustEdges) {
			expect(edge.targetKey).not.toContain("express");
		}
	});
});
