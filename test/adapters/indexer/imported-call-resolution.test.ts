/**
 * Imported free-function call resolution — integration test.
 *
 * Exercises the import-binding-assisted call resolution path.
 * The key test: when the same function name exists in multiple files,
 * the global name lookup is ambiguous. The import binding from the
 * calling file disambiguates by narrowing to the imported source file.
 *
 * Fixture:
 *   a.ts: exports greet() and farewell()
 *   b.ts: imports { greet, farewell } from "./a" and calls them
 *   c.ts: also exports greet() — creates ambiguity for global lookup
 *
 * Without import-binding resolution: greet() has 2 candidates, ambiguous, unresolved.
 * With import-binding resolution: b.ts imports from "./a", so greet in a.ts is picked.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { EdgeType, NodeKind } from "../../../src/core/model/index.js";

const FIXTURE_ROOT = join(import.meta.dirname, "../../fixtures/typescript/imported-calls");
const REPO_UID = "imported-calls-test";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let tsExtractor: TypeScriptExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	tsExtractor = new TypeScriptExtractor();
	await tsExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-imported-calls-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, [tsExtractor]);
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

describe("imported free-function call resolution", () => {
	it("resolves calls to imported functions as CALLS edges", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// Count resolved CALLS edges.
		const callCount = storage.countEdgesByType(result.snapshotUid, EdgeType.CALLS);
		// b.ts calls greet() and farewell() — both should resolve.
		// Also main() might have console.log calls (unresolved — no console node).
		expect(callCount).toBeGreaterThanOrEqual(2);
	});

	it("greet() in a.ts has a resolved caller from b.ts (import-binding disambiguation)", async () => {
		// This is the key test: greet() exists in BOTH a.ts and c.ts.
		// Global name lookup finds 2 candidates → ambiguous → would fail.
		// Import-binding resolution knows b.ts imports from "./a" →
		// disambiguates to a.ts's greet.
		const result = await indexer.indexRepo(REPO_UID);

		const callers = storage.findCallers({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:a.ts#greet:SYMBOL:FUNCTION`,
		});
		expect(callers.length).toBeGreaterThanOrEqual(1);
		expect(callers.some((c) => c.file === "b.ts")).toBe(true);
	});

	it("greet() in c.ts has NO callers (nobody imports from c.ts)", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const callers = storage.findCallers({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:c.ts#greet:SYMBOL:FUNCTION`,
		});
		expect(callers.length).toBe(0);
	});

	it("farewell() has a resolved caller from b.ts (unique, global resolves)", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const callers = storage.findCallers({
			snapshotUid: result.snapshotUid,
			stableKey: `${REPO_UID}:a.ts#farewell:SYMBOL:FUNCTION`,
		});
		expect(callers.length).toBeGreaterThanOrEqual(1);
	});

	it("greet() in a.ts is NOT dead (it has resolved callers via import binding)", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// a.ts's greet should NOT be dead — b.ts calls it.
		const greetDead = dead.find((d) => d.symbol === "greet" && d.file === "a.ts");
		expect(greetDead).toBeUndefined();
	});

	it("greet() in c.ts IS dead (no callers)", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		// c.ts's greet should be dead — nobody imports or calls it.
		const greetInC = dead.find((d) => d.symbol === "greet" && d.file === "c.ts");
		expect(greetInC).toBeDefined();
	});

	it("farewell() is NOT dead", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const dead = storage.findDeadNodes({
			snapshotUid: result.snapshotUid,
			kind: NodeKind.SYMBOL,
		});

		const farewellDead = dead.find((d) => d.symbol === "farewell");
		expect(farewellDead).toBeUndefined();
	});
});
