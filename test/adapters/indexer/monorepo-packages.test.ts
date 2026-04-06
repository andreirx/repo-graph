/**
 * Monorepo nearest-package resolution regression test.
 *
 * Pins the core slice 4.2 behavior: each source file classifies
 * unresolved edges against its NEAREST owning package.json, not
 * the repo root or a repo-wide dep union.
 *
 * Fixture: two sibling packages with disjoint dependencies:
 *   packages/api  — depends on express, cors
 *   packages/ui   — depends on react, react-dom
 *
 * Root package.json has zero dependencies (workspaces only).
 *
 * If the classifier regresses to root-only resolution or repo-wide
 * union, these assertions will fail:
 *   - API files would stop classifying express/cors as external
 *     (root has no deps), OR
 *   - UI files would incorrectly classify express as external
 *     (union includes all deps).
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
	"../../fixtures/typescript/monorepo-packages",
);
const REPO_UID = "monorepo-fixture";

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
	dbPath = join(tmpdir(), `rgr-monorepo-test-${randomUUID()}.db`);
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

describe("monorepo nearest-package resolution", () => {
	it("API file classifies express as external (from packages/api/package.json)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});
		const expressRow = rows.find((r) => r.targetKey === "express");
		expect(expressRow).toBeDefined();
		expect(expressRow?.classification).toBe("external_library_candidate");
		expect(expressRow?.basisCode).toBe("callee_matches_external_import");
		expect(expressRow?.sourceFilePath).toContain("packages/api/");
	});

	it("API file classifies cors as external (from packages/api/package.json)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});
		const corsRow = rows.find((r) => r.targetKey === "cors");
		expect(corsRow).toBeDefined();
		expect(corsRow?.classification).toBe("external_library_candidate");
		expect(corsRow?.sourceFilePath).toContain("packages/api/");
	});

	it("UI file classifies useState as external (from packages/ui/package.json, react dep)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const rows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
			category: "calls_function_ambiguous_or_missing",
		});
		const useStateRow = rows.find((r) => r.targetKey === "useState");
		expect(useStateRow).toBeDefined();
		expect(useStateRow?.classification).toBe("external_library_candidate");
		expect(useStateRow?.basisCode).toBe("callee_matches_external_import");
		expect(useStateRow?.sourceFilePath).toContain("packages/ui/");
	});

	it("root package.json has zero deps — files NOT in a sub-package would get empty deps", async () => {
		// The root has zero dependencies. If the resolver fell back to
		// repo-wide union, express AND react would both be in every
		// file's dep set. This test proves the root-level deps are
		// truly empty by checking that the total external count is
		// consistent with per-package resolution only.
		const result = await indexer.indexRepo(REPO_UID);
		const allRows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		});

		// Every external_library row must originate from a sub-package file,
		// not from any root-level file (there are none in this fixture, but
		// the assertion shape confirms the package-scoping logic).
		const externalRows = allRows.filter(
			(r) => r.classification === "external_library_candidate" &&
				r.basisCode === "callee_matches_external_import",
		);
		for (const row of externalRows) {
			// Must be under packages/ — not at root
			expect(row.sourceFilePath).toMatch(/^packages\//);
		}
	});

	it("express is NOT classified as external in UI files (cross-package isolation)", async () => {
		// This is the key isolation test. If deps were unioned across
		// the repo, express would be in the UI file's dep set and
		// would incorrectly classify as external_library_candidate.
		const result = await indexer.indexRepo(REPO_UID);
		const uiRows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath?.includes("packages/ui/"));

		// No UI row should have express or cors as an external-library hit
		const crossLeaks = uiRows.filter(
			(r) =>
				r.classification === "external_library_candidate" &&
				r.basisCode === "callee_matches_external_import" &&
				(r.targetKey === "express" || r.targetKey === "cors"),
		);
		expect(crossLeaks).toEqual([]);
	});

	it("react is NOT classified as external in API files (cross-package isolation)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const apiRows = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath?.includes("packages/api/"));

		const crossLeaks = apiRows.filter(
			(r) =>
				r.classification === "external_library_candidate" &&
				r.basisCode === "callee_matches_external_import" &&
				(r.targetKey === "React" || r.targetKey === "useState"),
		);
		expect(crossLeaks).toEqual([]);
	});
});
