/**
 * .jsx parsing regression tests.
 *
 * Previously the extractor used the plain TypeScript grammar for .jsx
 * files, which could not parse JSX syntax and produced spurious
 * top-level declarations — in particular, inner function-scope
 * `const` declarations with the same name leaked to module scope
 * and collided on stable_key.
 *
 * These tests pin the behavior: .jsx files use the TSX grammar, and
 * nested same-named consts inside JSX callbacks do NOT produce
 * duplicate SYMBOL nodes.
 *
 * Discovered during smoke-run dogfooding on glamCRM.
 */

import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../../../src/adapters/extractors/typescript/ts-extractor.js";
import { SqliteConnectionProvider } from "../../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../../fixtures/typescript/jsx-nested-consts",
);
const HOMEPAGE_PATH = join(FIXTURE_ROOT, "src/HomePage.jsx");

let extractor: TypeScriptExtractor;
let provider: SqliteConnectionProvider;
let storage: SqliteStorage;
let dbPath: string;

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-jsx-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {}
});

describe(".jsx parsing with tsx grammar", () => {
	it("parses .jsx files without emitting duplicate same-named inner consts", async () => {
		const source = await readFile(HOMEPAGE_PATH, "utf-8");
		const snapshotUid = "test-snapshot";
		const repoUid = "test-repo";
		const fileNodeUid = randomUUID();

		const result = await extractor.extract(
			source,
			"src/HomePage.jsx",
			fileNodeUid,
			repoUid,
			snapshotUid,
		);

		// Collect stable_keys emitted for this file
		const stableKeys = result.nodes.map((n) => n.stableKey);
		const uniqueKeys = new Set(stableKeys);

		// No duplicates
		expect(stableKeys.length).toBe(uniqueKeys.size);

		// Top-level export is HomePage (the only real top-level decl)
		const topLevelNames = result.nodes
			.filter((n) => n.stableKey.includes("#"))
			.map((n) => n.name);
		expect(topLevelNames).toContain("HomePage");

		// None of the inner consts should have leaked to top-level
		// (there are 3 `currentYearMonth` consts, all function-scope)
		const yearMonthNodes = result.nodes.filter(
			(n) => n.name === "currentYearMonth",
		);
		expect(yearMonthNodes.length).toBeLessThanOrEqual(0);
	});

});
