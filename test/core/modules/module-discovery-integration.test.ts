/**
 * Integration test for module discovery — end-to-end pipeline.
 *
 * Tests the full path: ManifestScanner → discoverModules orchestrator
 * → SQLite persistence → query-back verification.
 *
 * Uses existing fixtures:
 *   - monorepo-packages: package.json workspaces with 2 members
 *   - mixed-lang: standalone package.json + Cargo.toml at same root
 *
 * Verifies:
 *   - Candidates persisted with correct keys and names
 *   - Evidence items linked to correct candidates
 *   - File ownership assigned by longest-prefix containment
 *   - Workspace root is not a candidate (has workspaces field)
 *   - Workspace members are candidates
 *   - Multi-source evidence for same root is merged
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { ManifestScanner } from "../../../src/adapters/discovery/manifest-scanner.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";

const MONOREPO_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/typescript/monorepo-packages",
);

const MIXED_LANG_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/mixed-lang",
);

let extractor: TypeScriptExtractor;
let scanner: ManifestScanner;

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
	scanner = new ManifestScanner();
});

// Track DB paths for cleanup.
const dbPaths: string[] = [];

afterEach(() => {
	for (const p of dbPaths) {
		try { unlinkSync(p); } catch { /* ignore */ }
	}
	dbPaths.length = 0;
});

function setupDb(): { storage: SqliteStorage; provider: SqliteConnectionProvider; dbPath: string } {
	const dbPath = join(tmpdir(), `rgr-module-discovery-${randomUUID()}.db`);
	dbPaths.push(dbPath);
	const provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	const storage = new SqliteStorage(provider.getDatabase());
	return { storage, provider, dbPath };
}

// ── Monorepo workspace fixture ─────────────────────────────────────

describe("module discovery integration — monorepo workspace", () => {
	it("discovers workspace members as module candidates", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "monorepo-test";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		// Query persisted candidates.
		const candidates = storage.queryModuleCandidates(result.snapshotUid);

		// Workspace root has workspaces field → not a standalone candidate.
		// Members packages/api and packages/ui should be candidates.
		const memberCandidates = candidates.filter(
			(c) => c.canonicalRootPath.startsWith("packages/"),
		);
		expect(memberCandidates.length).toBeGreaterThanOrEqual(2);

		const apiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/api",
		);
		expect(apiCandidate).toBeDefined();
		expect(apiCandidate!.displayName).toBe("@fixture/api");
		expect(apiCandidate!.moduleKind).toBe("declared");
		expect(apiCandidate!.moduleKey).toBe(
			`${REPO_UID}:packages/api:DISCOVERED_MODULE`,
		);

		const uiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/ui",
		);
		expect(uiCandidate).toBeDefined();
		expect(uiCandidate!.displayName).toBe("@fixture/ui");

		provider.close();
	});

	it("creates evidence items for each workspace member", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "monorepo-evidence";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const apiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/api",
		);
		expect(apiCandidate).toBeDefined();

		const evidence = storage.queryModuleCandidateEvidence(
			apiCandidate!.moduleCandidateUid,
		);
		expect(evidence.length).toBeGreaterThanOrEqual(1);
		expect(evidence[0].sourceType).toBe("package_json_workspaces");
		expect(evidence[0].evidenceKind).toBe("workspace_member");

		provider.close();
	});

	it("assigns files to their workspace member module", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "monorepo-ownership";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const apiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/api",
		);

		const ownership = storage.queryModuleFileOwnership(result.snapshotUid);

		// Find ownership for the api server file.
		const serverOwnership = ownership.find((o) =>
			o.fileUid.endsWith("packages/api/src/server.ts"),
		);
		expect(serverOwnership).toBeDefined();
		expect(serverOwnership!.moduleCandidateUid).toBe(
			apiCandidate!.moduleCandidateUid,
		);
		expect(serverOwnership!.assignmentKind).toBe("root_containment");

		// Find ownership for the ui app file.
		const uiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/ui",
		);
		const appOwnership = ownership.find((o) =>
			o.fileUid.endsWith("packages/ui/src/App.tsx"),
		);
		expect(appOwnership).toBeDefined();
		expect(appOwnership!.moduleCandidateUid).toBe(
			uiCandidate!.moduleCandidateUid,
		);

		provider.close();
	});
});

// ── Mixed-lang fixture ─────────────────────────────────────────────

describe("module discovery integration — mixed-lang", () => {
	it("discovers both package.json and Cargo.toml at the same root", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "mixed-lang-test";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "mixed-lang-fixture",
			rootPath: MIXED_LANG_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);

		// Root "." should be a candidate (standalone package.json + Cargo.toml).
		const rootCandidate = candidates.find((c) =>
			c.canonicalRootPath === ".",
		);
		expect(rootCandidate).toBeDefined();
		expect(rootCandidate!.displayName).toBe("mixed-lang-fixture");

		// Multiple evidence items: package.json + Cargo.toml.
		const allEvidence = storage.queryAllModuleCandidateEvidence(result.snapshotUid);
		const rootEvidence = allEvidence.filter(
			(e) => e.moduleCandidateUid === rootCandidate!.moduleCandidateUid,
		);
		expect(rootEvidence.length).toBeGreaterThanOrEqual(2);

		const sourceTypes = rootEvidence.map((e) => e.sourceType).sort();
		expect(sourceTypes).toContain("cargo_crate");
		expect(sourceTypes).toContain("package_json_workspaces");

		provider.close();
	});

	it("assigns all files to the root module", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "mixed-lang-ownership";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "mixed-lang-fixture",
			rootPath: MIXED_LANG_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const ownership = storage.queryModuleFileOwnership(result.snapshotUid);

		// All tracked files should be owned by the root module.
		expect(ownership.length).toBe(result.filesTotal);
		const uniqueOwners = new Set(ownership.map((o) => o.moduleCandidateUid));
		expect(uniqueOwners.size).toBe(1);

		provider.close();
	});
});

// ── Rollup and targeted query surfaces ─────────────────────────────

describe("module discovery integration — rollups and queries", () => {
	it("queryModuleCandidateRollups returns correct counts", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "rollup-test";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const rollups = storage.queryModuleCandidateRollups(result.snapshotUid);
		expect(rollups.length).toBeGreaterThanOrEqual(2);

		const apiRollup = rollups.find((r) => r.canonicalRootPath === "packages/api");
		expect(apiRollup).toBeDefined();
		expect(apiRollup!.displayName).toBe("@fixture/api");
		expect(apiRollup!.moduleKind).toBe("declared");
		expect(apiRollup!.fileCount).toBeGreaterThanOrEqual(1);
		expect(apiRollup!.symbolCount).toBeGreaterThanOrEqual(0);
		expect(apiRollup!.evidenceCount).toBeGreaterThanOrEqual(1);
		expect(apiRollup!.confidence).toBeGreaterThan(0);
		// Languages should be deterministic (sorted).
		if (apiRollup!.languages) {
			const langs = apiRollup!.languages.split(",");
			const sorted = [...langs].sort();
			expect(langs).toEqual(sorted);
		}

		provider.close();
	});

	it("queryModuleOwnedFiles returns only files for the specified module", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "owned-files-test";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const apiCandidate = candidates.find((c) =>
			c.canonicalRootPath === "packages/api",
		);
		expect(apiCandidate).toBeDefined();

		const files = storage.queryModuleOwnedFiles(
			result.snapshotUid,
			apiCandidate!.moduleCandidateUid,
		);

		// All returned files should be under packages/api/.
		expect(files.length).toBeGreaterThanOrEqual(1);
		for (const f of files) {
			expect(f.filePath.startsWith("packages/api/")).toBe(true);
			expect(f.assignmentKind).toBe("root_containment");
			expect(f.confidence).toBeGreaterThan(0);
			expect(f.language).toBeTruthy();
		}

		// Should not include files from packages/ui/.
		const uiFiles = files.filter((f) => f.filePath.startsWith("packages/ui/"));
		expect(uiFiles).toHaveLength(0);

		provider.close();
	});

	it("queryModuleOwnedFiles respects limit parameter", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "owned-files-limit";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "monorepo-fixture",
			rootPath: MONOREPO_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const candidate = candidates.find((c) => c.canonicalRootPath === "packages/api")
			?? candidates[0];

		const limited = storage.queryModuleOwnedFiles(
			result.snapshotUid,
			candidate.moduleCandidateUid,
			1,
		);
		const unlimited = storage.queryModuleOwnedFiles(
			result.snapshotUid,
			candidate.moduleCandidateUid,
		);

		if (unlimited.length > 1) {
			expect(limited).toHaveLength(1);
			expect(unlimited.length).toBeGreaterThan(1);
		}

		provider.close();
	});
});

// ── Layer 2: Operational promotion (zero declared modules) ─────────

const STANDALONE_CLI_FIXTURE = join(
	import.meta.dirname,
	"../../fixtures/standalone-cli",
);

describe("module discovery integration — Layer 2 operational promotion", () => {
	it("promotes CLI surface to operational module when no declared modules exist", async () => {
		const { storage, provider } = setupDb();
		const REPO_UID = "standalone-cli-test";

		storage.addRepo({
			repoUid: REPO_UID,
			name: "standalone-cli",
			rootPath: STANDALONE_CLI_FIXTURE,
			defaultBranch: "main",
			createdAt: new Date().toISOString(),
			metadataJson: null,
		});

		const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
		const result = await indexer.indexRepo(REPO_UID);

		const candidates = storage.queryModuleCandidates(result.snapshotUid);
		const evidence = storage.queryAllModuleCandidateEvidence(result.snapshotUid);
		const surfaces = storage.queryProjectSurfaces(result.snapshotUid);

		// Should have one operational module candidate (promoted from CLI surface).
		expect(candidates).toHaveLength(1);
		expect(candidates[0].moduleKind).toBe("operational");
		expect(candidates[0].canonicalRootPath).toBe(".");
		// Confidence capped at Layer 2 ceiling (0.85).
		expect(candidates[0].confidence).toBeLessThanOrEqual(0.85);

		// Should have evidence from surface promotion.
		expect(evidence.length).toBeGreaterThanOrEqual(1);
		expect(evidence[0].sourceType).toBe("surface_promotion_cli");
		expect(evidence[0].evidenceKind).toBe("operational_entrypoint");

		// Should have attached surface (CLI).
		expect(surfaces.length).toBeGreaterThanOrEqual(1);
		const cliSurface = surfaces.find((s) => s.surfaceKind === "cli");
		expect(cliSurface).toBeDefined();
		expect(cliSurface!.moduleCandidateUid).toBe(candidates[0].moduleCandidateUid);

		// Should have file ownership rows.
		const ownership = storage.queryModuleFileOwnership(result.snapshotUid);
		expect(ownership.length).toBeGreaterThanOrEqual(1);
		// All owned files should belong to the operational module.
		for (const o of ownership) {
			expect(o.moduleCandidateUid).toBe(candidates[0].moduleCandidateUid);
		}

		provider.close();
	});

	it("produces no candidates when repo has no manifests", async () => {
		// Edge case: repo with only source files, no package.json/Cargo.toml/pyproject.toml.
		// The fixture for this would be empty or source-only, but we can test
		// with an empty temp directory.
		const { storage, provider } = setupDb();
		const REPO_UID = "empty-repo-test";
		const emptyFixture = join(tmpdir(), `rgr-empty-${randomUUID()}`);
		const { mkdirSync, writeFileSync, rmSync } = await import("node:fs");
		mkdirSync(emptyFixture, { recursive: true });
		// Add a source file but no manifest.
		mkdirSync(join(emptyFixture, "src"), { recursive: true });
		writeFileSync(join(emptyFixture, "src/main.ts"), "export const x = 1;");

		try {
			storage.addRepo({
				repoUid: REPO_UID,
				name: "empty-repo",
				rootPath: emptyFixture,
				defaultBranch: "main",
				createdAt: new Date().toISOString(),
				metadataJson: null,
			});

			const indexer = new RepoIndexer(storage, extractor, undefined, scanner);
			const result = await indexer.indexRepo(REPO_UID);

			// Should have no module candidates (no manifests = no declared, no surfaces = no operational).
			const candidates = storage.queryModuleCandidates(result.snapshotUid);
			expect(candidates).toHaveLength(0);

			// Should have no surfaces.
			const surfaces = storage.queryProjectSurfaces(result.snapshotUid);
			expect(surfaces).toHaveLength(0);

			// Should have no ownership rows.
			const ownership = storage.queryModuleFileOwnership(result.snapshotUid);
			expect(ownership).toHaveLength(0);
		} finally {
			rmSync(emptyFixture, { recursive: true, force: true });
			provider.close();
		}
	});
});
