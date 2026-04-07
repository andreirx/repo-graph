/**
 * Mixed-language indexer integration test.
 *
 * Exercises the actual multi-extractor architectural seam with ALL
 * three languages: TypeScript, Rust, and Java.
 *   - .ts → TS extractor + package.json deps
 *   - .rs → Rust extractor + Cargo.toml deps
 *   - .java → Java extractor + build.gradle deps
 *   - NO cross-contamination between manifest types
 *   - merged builtins include all three language sets
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { JavaExtractor } from "../../../src/adapters/extractors/java/java-extractor.js";
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
let javaExtractor: JavaExtractor;
let indexer: RepoIndexer;
let dbPath: string;

beforeAll(async () => {
	tsExtractor = new TypeScriptExtractor();
	await tsExtractor.initialize();
	rustExtractor = new RustExtractor();
	await rustExtractor.initialize();
	javaExtractor = new JavaExtractor();
	await javaExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-mixed-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	// All three extractors registered — full multi-language seam.
	indexer = new RepoIndexer(storage, [tsExtractor, rustExtractor, javaExtractor]);
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
	it("indexes .ts, .rs, and .java files in the same repo", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		// Fixture: server.ts + engine.rs + App.java = 3 files.
		expect(result.filesTotal).toBe(3);
		expect(result.nodesTotal).toBeGreaterThan(0);

		const tsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/server.ts:FILE`,
		);
		expect(tsFile).not.toBeNull();

		const rsFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/engine.rs:FILE`,
		);
		expect(rsFile).not.toBeNull();

		const javaFile = storage.getNodeByStableKey(
			result.snapshotUid,
			`${REPO_UID}:src/App.java:FILE`,
		);
		expect(javaFile).not.toBeNull();
	});

	it("emits symbols from all three languages", async () => {
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

		// Java: should have "App" class from App.java.
		const javaSymbol = storage.resolveSymbol({
			snapshotUid: result.snapshotUid,
			query: "App",
			limit: 10,
		});
		// Filter to only Java-file symbols to avoid confusing with TS App.
		const javaApp = javaSymbol.find((s) =>
			s.stableKey.includes(".java#"),
		);
		expect(javaApp).toBeDefined();
	});
});

describe("mixed-language indexer — Java dependency isolation", () => {
	it("Java file gets Gradle deps, not package.json or Cargo deps", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const javaEdges = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath?.endsWith(".java"));

		// Java edges should exist.
		expect(javaEdges.length).toBeGreaterThan(0);

		// No Java edge should reference express (TS dep) or serde (Rust dep).
		for (const e of javaEdges) {
			expect(e.targetKey).not.toContain("express");
			expect(e.targetKey).not.toContain("serde");
		}
	});

	it("Java Spring import classifies as external via Gradle deps (prefix match)", async () => {
		const result = await indexer.indexRepo(REPO_UID);
		const javaEdges = storage.queryUnresolvedEdges({
			snapshotUid: result.snapshotUid,
		}).filter((r) => r.sourceFilePath?.endsWith(".java"));

		// The fixture imports org.springframework.web.bind.annotation.RestController.
		// build.gradle has org.springframework.boot:spring-boot-starter-web.
		// Gradle reader stores "org.springframework" as 2-segment prefix.
		// Classifier prefix-matches against "org.springframework" → external.
		//
		// This MUST produce an unresolved IMPORTS edge because there is no
		// local file to resolve it to. The assertion is unconditional.
		const springImport = javaEdges.find(
			(r) => r.category === "imports_file_not_found" &&
				r.targetKey.includes("springframework"),
		);
		expect(springImport).toBeDefined();
		expect(springImport!.classification).toBe("external_library_candidate");
		expect(springImport!.basisCode).toBe("specifier_matches_package_dependency");
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
