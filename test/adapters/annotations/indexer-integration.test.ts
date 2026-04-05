/**
 * Annotations indexer integration test.
 *
 * Verifies the end-to-end path: temp repo with README + package.json
 * → repo.index → AnnotationsPort persists README and package-description
 * annotations attributed to the correct targets.
 *
 * Uses a minimal temp fixture (not simple-imports) to keep assertions
 * precise.
 */

import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteAnnotationsStorage } from "../../../src/adapters/annotations/sqlite-annotations-storage.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { AnnotationKind } from "../../../src/core/annotations/types.js";

let provider: SqliteConnectionProvider;
let storage: SqliteStorage;
let annotations: SqliteAnnotationsStorage;
let indexer: RepoIndexer;
let extractor: TypeScriptExtractor;
let dbPath: string;
let fixtureDir: string;
const REPO_UID = "annotations-integration-repo";

beforeEach(async () => {
	dbPath = join(tmpdir(), `rgr-ann-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	annotations = new SqliteAnnotationsStorage(provider.getDatabase());

	extractor = new TypeScriptExtractor();
	await extractor.initialize();
	indexer = new RepoIndexer(storage, extractor, annotations);

	// Build a minimal temp fixture
	fixtureDir = join(tmpdir(), `rgr-ann-fixture-${randomUUID()}`);
	mkdirSync(join(fixtureDir, "src", "core"), { recursive: true });
	mkdirSync(join(fixtureDir, "src", "adapters"), { recursive: true });

	// Repo-root package.json with description
	writeFileSync(
		join(fixtureDir, "package.json"),
		JSON.stringify(
			{
				name: "fixture-repo",
				version: "1.0.0",
				description: "Integration test fixture repository.",
			},
			null,
			2,
		),
	);
	// Repo-root README.md
	writeFileSync(
		join(fixtureDir, "README.md"),
		"# Fixture Repo\n\nTop-level README content.",
	);
	// Module-level README.md under src/core
	writeFileSync(
		join(fixtureDir, "src", "core", "README.md"),
		"# Core Module\n\nCore engine responsibility.",
	);
	// A TS file so the indexer creates MODULE nodes
	writeFileSync(
		join(fixtureDir, "src", "core", "index.ts"),
		"export const version = 1;\n",
	);
	writeFileSync(
		join(fixtureDir, "src", "adapters", "index.ts"),
		"export const adapter = true;\n",
	);

	// Register the repo
	storage.addRepo({
		repoUid: REPO_UID,
		name: "fixture",
		rootPath: fixtureDir,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});
});

afterEach(() => {
	provider.close();
	try {
		rmSync(fixtureDir, { recursive: true, force: true });
	} catch {}
	try {
		rmSync(dbPath, { force: true });
	} catch {}
});

describe("RepoIndexer → AnnotationsPort integration", () => {
	it("extracts repo-root README as repo-targeted annotation", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		expect(result.snapshotUid).toBeDefined();

		const repoAnnotations = annotations.getAnnotationsByTarget(
			result.snapshotUid,
			`${REPO_UID}:REPO`,
		);
		const readme = repoAnnotations.find(
			(a) => a.annotation_kind === AnnotationKind.MODULE_README,
		);
		expect(readme).toBeDefined();
		expect(readme!.content).toContain("# Fixture Repo");
		expect(readme!.source_file).toBe("README.md");
		expect(readme!.target_kind).toBe("repo");
		expect(readme!.language).toBe("markdown");
		expect(readme!.content_hash.startsWith("sha256:")).toBe(true);
	});

	it("extracts repo-root package.json description as repo-targeted annotation", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const repoAnnotations = annotations.getAnnotationsByTarget(
			result.snapshotUid,
			`${REPO_UID}:REPO`,
		);
		const pkg = repoAnnotations.find(
			(a) => a.annotation_kind === AnnotationKind.PACKAGE_DESCRIPTION,
		);
		expect(pkg).toBeDefined();
		expect(pkg!.content).toBe("Integration test fixture repository.");
		expect(pkg!.source_file).toBe("package.json");
		expect(pkg!.target_kind).toBe("repo");
		expect(pkg!.language).toBe("json");
	});

	it("extracts module-level README attributed to MODULE node", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const moduleAnnotations = annotations.getAnnotationsByTarget(
			result.snapshotUid,
			`${REPO_UID}:src/core:MODULE`,
		);
		const readme = moduleAnnotations.find(
			(a) => a.annotation_kind === AnnotationKind.MODULE_README,
		);
		expect(readme).toBeDefined();
		expect(readme!.content).toContain("Core engine responsibility");
		expect(readme!.source_file).toBe("src/core/README.md");
		expect(readme!.target_kind).toBe("module");
	});

	it("total annotation count matches extracted inputs", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const total = annotations.countAnnotationsBySnapshot(result.snapshotUid);
		// 3 expected: repo README + repo package description + module core README
		expect(total).toBe(3);
	});

	it("respects README.md preference over README.txt in same dir", async () => {
		// Add a README.txt alongside the existing README.md at repo root
		writeFileSync(join(fixtureDir, "README.txt"), "plain-text readme");

		const result = await indexer.indexRepo(REPO_UID);
		const repoAnnotations = annotations.getAnnotationsByTarget(
			result.snapshotUid,
			`${REPO_UID}:REPO`,
		);
		const readmes = repoAnnotations.filter(
			(a) => a.annotation_kind === AnnotationKind.MODULE_README,
		);
		expect(readmes).toHaveLength(1);
		// The .md file wins
		expect(readmes[0].source_file).toBe("README.md");
		expect(readmes[0].language).toBe("markdown");
	});

	it("works when AnnotationsPort is absent (skips extraction silently)", async () => {
		// Build an indexer WITHOUT the annotations port
		const silentIndexer = new RepoIndexer(storage, extractor);
		const result = await silentIndexer.indexRepo(REPO_UID);
		// No annotations written for this snapshot
		expect(annotations.countAnnotationsBySnapshot(result.snapshotUid)).toBe(0);
	});

	it("annotation_collisions_dropped is 0 when no collisions occur", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const diagnostics = storage.getSnapshotExtractionDiagnostics(
			result.snapshotUid,
		);
		expect(diagnostics).not.toBeNull();
		const parsed = JSON.parse(diagnostics!) as {
			annotation_collisions_dropped: number;
		};
		expect(parsed.annotation_collisions_dropped).toBe(0);
	});
});
