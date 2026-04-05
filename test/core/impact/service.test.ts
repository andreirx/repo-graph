/**
 * Change-impact service tests.
 *
 * Two layers:
 *   - resolveScope: pure function, tested in isolation
 *   - computeChangeImpact: orchestrator, tested with real SqliteStorage
 *     + a fake GitPort that returns fixed file lists
 *
 * The fake git is scoped to this test and does not pretend to exercise
 * the real git adapter. The git adapter has its own tests in
 * test/adapters/git/git-adapter.test.ts.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import {
	computeChangeImpact,
	ImpactScopeError,
	resolveScope,
	type ScopeRequest,
} from "../../../src/core/impact/service.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Resolution,
	SnapshotKind,
	Visibility,
} from "../../../src/core/model/index.js";
import type {
	FileChurnEntry,
	GitDiffScope,
	GitPort,
} from "../../../src/core/ports/git.js";

// ── resolveScope (pure function) ──────────────────────────────────

describe("resolveScope", () => {
	it("against_snapshot uses the basis commit", () => {
		const result = resolveScope({ kind: "against_snapshot" }, "abc123");
		expect(result.impactScope).toEqual({
			kind: "against_snapshot",
			basis_commit: "abc123",
		});
		expect(result.gitScope).toEqual({
			kind: "working_tree_vs_commit",
			commit: "abc123",
		});
	});

	it("against_snapshot throws when basis_commit is null", () => {
		expect(() => resolveScope({ kind: "against_snapshot" }, null)).toThrow(
			ImpactScopeError,
		);
	});

	it("staged produces staged git scope", () => {
		const result = resolveScope({ kind: "staged" }, "abc");
		expect(result.impactScope).toEqual({ kind: "staged" });
		expect(result.gitScope).toEqual({ kind: "staged" });
	});

	it("since_ref produces working_tree_vs_commit with the ref", () => {
		const result = resolveScope({ kind: "since_ref", ref: "main" }, null);
		expect(result.impactScope).toEqual({
			kind: "since_ref",
			ref: "main",
		});
		expect(result.gitScope).toEqual({
			kind: "working_tree_vs_commit",
			commit: "main",
		});
	});
});

// ── computeChangeImpact (with real storage + fake git) ────────────

/**
 * Fake GitPort that returns a fixed list of changed files regardless
 * of scope. Sufficient for testing the orchestrator's mapping logic.
 */
class FakeGitPort implements GitPort {
	constructor(private readonly changedFiles: string[]) {}

	async getCurrentCommit(): Promise<string | null> {
		return "fake-commit";
	}
	async isDirty(): Promise<boolean> {
		return false;
	}
	async getFileChurn(): Promise<FileChurnEntry[]> {
		return [];
	}
	async getChangedFiles(
		_repoPath: string,
		_scope: GitDiffScope,
	): Promise<string[]> {
		return this.changedFiles;
	}
}

const REPO_UID = "impact-repo";

describe("computeChangeImpact", () => {
	let storage: SqliteStorage;
	let provider: SqliteConnectionProvider;
	let dbPath: string;
	let snapshotUid: string;

	// Graph shape:
	//   src/core     owns  src/core/a.ts, src/core/b.ts
	//   src/adapters owns  src/adapters/c.ts
	//   src/cli      owns  src/cli/d.ts
	// Imports:
	//   src/adapters imports src/core
	//   src/cli      imports src/adapters
	// Reverse from src/core: src/adapters (d=1), src/cli (d=2)

	beforeEach(() => {
		dbPath = join(tmpdir(), `rgr-impact-${randomUUID()}.db`);
		provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		storage = new SqliteStorage(provider.getDatabase());
		storage.addRepo({
			repoUid: REPO_UID,
			name: "impact-repo",
			rootPath: "/tmp/impact-repo",
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "commit-hash-abc",
		});
		snapshotUid = snap.snapshotUid;

		// Files
		storage.upsertFiles([
			{
				fileUid: `${REPO_UID}:src/core/a.ts`,
				repoUid: REPO_UID,
				path: "src/core/a.ts",
				language: "typescript",
				isTest: false,
				isGenerated: false,
				isExcluded: false,
			},
			{
				fileUid: `${REPO_UID}:src/core/b.ts`,
				repoUid: REPO_UID,
				path: "src/core/b.ts",
				language: "typescript",
				isTest: false,
				isGenerated: false,
				isExcluded: false,
			},
			{
				fileUid: `${REPO_UID}:src/adapters/c.ts`,
				repoUid: REPO_UID,
				path: "src/adapters/c.ts",
				language: "typescript",
				isTest: false,
				isGenerated: false,
				isExcluded: false,
			},
			{
				fileUid: `${REPO_UID}:src/cli/d.ts`,
				repoUid: REPO_UID,
				path: "src/cli/d.ts",
				language: "typescript",
				isTest: false,
				isGenerated: false,
				isExcluded: false,
			},
		]);

		// Nodes
		const modCore = randomUUID();
		const modAdapters = randomUUID();
		const modCli = randomUUID();
		const fileA = randomUUID();
		const fileB = randomUUID();
		const fileC = randomUUID();
		const fileD = randomUUID();

		const mkMod = (uid: string, path: string) => ({
			nodeUid: uid,
			snapshotUid,
			repoUid: REPO_UID,
			stableKey: `${REPO_UID}:${path}:MODULE`,
			kind: NodeKind.MODULE,
			subtype: NodeSubtype.DIRECTORY,
			name: path,
			qualifiedName: path,
			fileUid: null,
			parentNodeUid: null,
			location: { lineStart: 0, colStart: 0, lineEnd: 0, colEnd: 0 },
			signature: null,
			visibility: Visibility.EXPORT,
			docComment: null,
			metadataJson: null,
		});
		const mkFile = (uid: string, path: string) => ({
			nodeUid: uid,
			snapshotUid,
			repoUid: REPO_UID,
			stableKey: `${REPO_UID}:${path}:FILE`,
			kind: NodeKind.FILE,
			subtype: NodeSubtype.SOURCE,
			name: path.split("/").pop() ?? path,
			qualifiedName: path,
			fileUid: `${REPO_UID}:${path}`,
			parentNodeUid: null,
			location: { lineStart: 0, colStart: 0, lineEnd: 0, colEnd: 0 },
			signature: null,
			visibility: Visibility.EXPORT,
			docComment: null,
			metadataJson: null,
		});

		storage.insertNodes([
			mkMod(modCore, "src/core"),
			mkMod(modAdapters, "src/adapters"),
			mkMod(modCli, "src/cli"),
			mkFile(fileA, "src/core/a.ts"),
			mkFile(fileB, "src/core/b.ts"),
			mkFile(fileC, "src/adapters/c.ts"),
			mkFile(fileD, "src/cli/d.ts"),
		]);

		const mkEdge = (
			src: string,
			tgt: string,
			type: EdgeType,
		) => ({
			edgeUid: randomUUID(),
			snapshotUid,
			repoUid: REPO_UID,
			sourceNodeUid: src,
			targetNodeUid: tgt,
			type,
			resolution: Resolution.STATIC,
			extractor: "test:0.0.1",
			location: null,
			metadataJson: null,
		});

		storage.insertEdges([
			// OWNS: module -> file
			mkEdge(modCore, fileA, EdgeType.OWNS),
			mkEdge(modCore, fileB, EdgeType.OWNS),
			mkEdge(modAdapters, fileC, EdgeType.OWNS),
			mkEdge(modCli, fileD, EdgeType.OWNS),
			// IMPORTS: module -> module
			mkEdge(modAdapters, modCore, EdgeType.IMPORTS),
			mkEdge(modCli, modAdapters, EdgeType.IMPORTS),
		]);
	});

	afterEach(() => {
		provider.close();
		try {
			unlinkSync(dbPath);
		} catch {}
	});

	it("maps a changed file to its owning module as a seed", async () => {
		const git = new FakeGitPort(["src/core/a.ts"]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});

		expect(result.seed_modules).toEqual([`${REPO_UID}:src/core:MODULE`]);
		expect(result.changed_files).toHaveLength(1);
		expect(result.changed_files[0]).toEqual({
			path: "src/core/a.ts",
			matched_to_index: true,
			owning_module: `${REPO_UID}:src/core:MODULE`,
			unmatched_reason: null,
		});
	});

	it("propagates to reverse importers via reverse MODULE IMPORTS", async () => {
		const git = new FakeGitPort(["src/core/a.ts"]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});

		// Expect: src/core as seed (d=0), src/adapters (d=1), src/cli (d=2)
		expect(result.impacted_modules).toEqual([
			{
				module: `${REPO_UID}:src/core:MODULE`,
				distance: 0,
				reason: "seed",
			},
			{
				module: `${REPO_UID}:src/adapters:MODULE`,
				distance: 1,
				reason: "reverse_import",
			},
			{
				module: `${REPO_UID}:src/cli:MODULE`,
				distance: 2,
				reason: "reverse_import",
			},
		]);
		expect(result.counts.max_distance).toBe(2);
	});

	it("marks unmatched files with unmatched_reason='not_in_snapshot'", async () => {
		const git = new FakeGitPort([
			"README.md",
			"new-file-not-indexed.ts",
			"src/core/a.ts",
		]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});

		const unmatched = result.changed_files.filter((f) => !f.matched_to_index);
		expect(unmatched).toHaveLength(2);
		for (const u of unmatched) {
			expect(u.unmatched_reason).toBe("not_in_snapshot");
			expect(u.owning_module).toBeNull();
		}
		expect(result.counts.changed_files_unmatched).toBe(2);
		expect(result.counts.changed_files_matched).toBe(1);
	});

	it("dedupes duplicate changed paths and sorts output", async () => {
		const git = new FakeGitPort([
			"src/core/b.ts",
			"src/core/a.ts",
			"src/core/a.ts", // duplicate
		]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});
		expect(result.changed_files.map((f) => f.path)).toEqual([
			"src/core/a.ts",
			"src/core/b.ts",
		]);
	});

	it("collapses multiple seeds in same module to one seed entry", async () => {
		const git = new FakeGitPort(["src/core/a.ts", "src/core/b.ts"]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});
		expect(result.seed_modules).toEqual([`${REPO_UID}:src/core:MODULE`]);
		expect(result.counts.seed_modules).toBe(1);
		expect(result.counts.changed_files_matched).toBe(2);
	});

	it("respects maxDepth cap", async () => {
		const git = new FakeGitPort(["src/core/a.ts"]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
			maxDepth: 1,
		});
		const modules = result.impacted_modules.map((m) => m.module);
		expect(modules).toContain(`${REPO_UID}:src/core:MODULE`); // seed
		expect(modules).toContain(`${REPO_UID}:src/adapters:MODULE`); // d=1
		expect(modules).not.toContain(`${REPO_UID}:src/cli:MODULE`); // would be d=2
	});

	it("returns trust metadata with standard caveats", async () => {
		const git = new FakeGitPort([]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});
		expect(result.trust.graph_basis).toBe("reverse_module_imports_only");
		expect(result.trust.calls_included).toBe(false);
		expect(result.trust.caveats.length).toBeGreaterThan(0);
		expect(
			result.trust.caveats.some((c) => c.includes("CALLS")),
		).toBe(true);
		expect(
			result.trust.caveats.some((c) => c.toLowerCase().includes("registry")),
		).toBe(true);
	});

	it("throws ImpactScopeError when against_snapshot has no basis_commit", async () => {
		const git = new FakeGitPort([]);
		await expect(
			computeChangeImpact({
				git,
				storage,
				repoUid: REPO_UID,
				repoPath: "/tmp/impact-repo",
				snapshotUid,
				snapshotBasisCommit: null,
				scopeRequest: { kind: "against_snapshot" },
			}),
		).rejects.toThrow(ImpactScopeError);
	});

	it("echoes scope in result output", async () => {
		const git = new FakeGitPort(["src/core/a.ts"]);
		const result = await computeChangeImpact({
			git,
			storage,
			repoUid: REPO_UID,
			repoPath: "/tmp/impact-repo",
			snapshotUid,
			snapshotBasisCommit: "commit-hash-abc",
			scopeRequest: { kind: "against_snapshot" },
		});
		expect(result.scope).toEqual({
			kind: "against_snapshot",
			basis_commit: "commit-hash-abc",
		});
		expect(result.snapshot_basis_commit).toBe("commit-hash-abc");
	});
});
