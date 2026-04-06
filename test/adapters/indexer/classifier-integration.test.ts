/**
 * End-to-end integration test for the unresolved-edge classifier.
 *
 * Uses a dedicated fixture (classifier-repo) whose source file
 * contains four calls, each designed to exercise one path through
 * the classifier:
 *
 *   debounce()        → EXTERNAL (lodash in package.json deps)
 *   aliased()         → INTERNAL (alias-matched via tsconfig paths)
 *   relatively()      → INTERNAL (relative-path import)
 *   mysteryFunction() → UNKNOWN   (no signal)
 *
 * This test validates the signals path end-to-end:
 *   - tsconfig.json is read and its paths become aliases
 *   - package.json is read and its deps flow into the classifier
 *   - importBindings are captured by the extractor
 *   - per-file signals reach the classifier correctly
 *   - classification results are persisted with the right basis codes
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

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../fixtures/typescript/classifier-repo",
);
const REPO_UID = "classifier-repo";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let extractor: TypeScriptExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-classifier-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
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

function findRowByTargetIdentifier(
	rows: ReturnType<typeof storage.queryUnresolvedEdges>,
	identifier: string,
) {
	return rows.find(
		(r) => r.targetKey === identifier || r.targetKey.startsWith(`${identifier}.`),
	);
}

describe("classifier integration — classifier-repo fixture", () => {
	it("classifies debounce() as external_library_candidate via lodash", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});

		const row = findRowByTargetIdentifier(rows, "debounce");
		expect(row).toBeDefined();
		expect(row?.classification).toBe("external_library_candidate");
		expect(row?.basisCode).toBe("callee_matches_external_import");
	});

	it("classifies aliased() as internal_candidate via project alias", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});

		const row = findRowByTargetIdentifier(rows, "aliased");
		expect(row).toBeDefined();
		expect(row?.classification).toBe("internal_candidate");
		expect(row?.basisCode).toBe("specifier_matches_project_alias");
	});

	it("classifies relatively() as internal_candidate via relative import", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});

		const row = findRowByTargetIdentifier(rows, "relatively");
		expect(row).toBeDefined();
		expect(row?.classification).toBe("internal_candidate");
		expect(row?.basisCode).toBe("callee_matches_internal_import");
	});

	it("classifies mysteryFunction() as unknown", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});

		const row = findRowByTargetIdentifier(rows, "mysteryFunction");
		expect(row).toBeDefined();
		expect(row?.classification).toBe("unknown");
		expect(row?.basisCode).toBe("no_supporting_signal");
	});

	it("unresolved IMPORTS edge for nonexistent relative path → internal / RELATIVE_IMPORT_TARGET_UNRESOLVED", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "imports_file_not_found",
		});

		// At least one row for './local-nonexistent'.
		expect(rows.length).toBeGreaterThanOrEqual(1);
		for (const row of rows) {
			expect(row.classification).toBe("internal_candidate");
			expect(row.basisCode).toBe("relative_import_target_unresolved");
		}
	});
});
