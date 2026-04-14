/**
 * Cross-runtime DB interop test.
 *
 * Proves that a SQLite database written by the Rust indexer is
 * consumable by the existing TS product layer.
 *
 * Steps:
 *   1. Build and run the Rust `rgr-rust index` binary against a
 *      checked-in fixture repo → temp DB file
 *   2. Open that DB with the TS SqliteConnectionProvider
 *   3. Execute real read-side operations through the TS storage adapter
 *   4. Assert the results are structurally valid
 */

import { execSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../src/adapters/storage/sqlite/sqlite-storage.js";
import type {
	ModuleStats,
	NodeResult,
	PathResult,
	QueryResult,
} from "../../src/core/model/index.js";
import { EdgeType } from "../../src/core/model/index.js";
import {
	formatQueryResult,
	formatModuleStatsResult,
	formatNodeResult,
	formatPathResult as formatPathResultJson,
} from "../../src/cli/formatters/json.js";

const FIXTURE_REPO = join(
	__dirname,
	"..",
	"..",
	"test",
	"fixtures",
	"typescript",
	"classifier-repo",
);

const RUST_BINARY = join(
	__dirname,
	"..",
	"..",
	"rust",
	"target",
	"debug",
	"rgr-rust",
);

let tempDir: string;
let dbPath: string;
let provider: SqliteConnectionProvider;
let storage: SqliteStorage;

describe("Rust-written SQLite consumed by TS storage layer", () => {
	beforeAll(() => {
		// Ensure Rust binary is built.
		if (!existsSync(RUST_BINARY)) {
			throw new Error(
				`Rust binary not found at ${RUST_BINARY}. Run \`cargo build -p repo-graph-rgr\` first.`,
			);
		}

		// Create temp DB.
		tempDir = mkdtempSync(join(tmpdir(), "rgr-interop-"));
		dbPath = join(tempDir, "interop.db");

		// Run Rust indexer.
		const cmd = `"${RUST_BINARY}" index "${FIXTURE_REPO}" "${dbPath}"`;
		const output = execSync(cmd, { encoding: "utf-8", timeout: 30000 });
		expect(output).toBeDefined();
		expect(existsSync(dbPath)).toBe(true);
	});

	afterAll(() => {
		if (provider) {
			try {
				provider.close();
			} catch {
				// ignore
			}
		}
		if (tempDir && existsSync(tempDir)) {
			rmSync(tempDir, { recursive: true, force: true });
		}
	});

	it("opens the Rust-written DB without migration errors", () => {
		// The TS connection provider runs all migrations. Since the
		// Rust side already applied them, the TS side should detect
		// they exist and skip them.
		provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		storage = new SqliteStorage(provider.getDatabase());
	});

	it("getLatestSnapshot returns a READY snapshot", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();
		expect(snap!.status).toBe("ready");
		expect(snap!.kind).toBe("full");
	});

	it("queryAllNodes returns nodes with valid stable keys", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		const nodes = storage.queryAllNodes(snap!.snapshotUid);
		expect(nodes.length).toBeGreaterThan(0);

		// FILE node for src/index.ts should exist.
		const fileNode = nodes.find(
			(n) => n.stableKey === "classifier-repo:src/index.ts:FILE",
		);
		expect(fileNode).toBeDefined();
		expect(fileNode!.kind).toBe("FILE");

		// FUNCTION node for standalone should exist.
		const funcNode = nodes.find((n) =>
			n.stableKey.includes("#standalone:SYMBOL:FUNCTION"),
		);
		expect(funcNode).toBeDefined();
	});

	it("getRepo returns the Rust-registered repo", () => {
		const repo = storage.getRepo({ uid: "classifier-repo" });
		expect(repo).not.toBeNull();
		expect(repo!.name).toBe("classifier-repo");
	});

	it("countEdgesByType returns nonzero for unresolved calls", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		// The classifier-repo fixture has unresolved CALLS edges.
		// The unresolved_edges table should have rows.
		const unresolvedEdges = storage.countUnresolvedEdges({
			snapshotUid: snap!.snapshotUid,
			groupBy: "classification",
		});
		expect(unresolvedEdges.length).toBeGreaterThan(0);
	});

	// ── Rust-16: graph-query read-side interop ────────────────────

	it("findDeadNodes returns results from Rust-written DB", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		// The classifier-repo has exported symbols. Some should be dead
		// (no incoming reference edges). The key assertion is that the
		// TS query runs without error on a Rust-indexed DB.
		const dead = storage.findDeadNodes({
			snapshotUid: snap!.snapshotUid,
			kind: "SYMBOL",
		});
		// Structural validity: each result has required fields.
		for (const d of dead) {
			expect(typeof d.symbol).toBe("string");
			expect(typeof d.kind).toBe("string");
		}
	});

	it("findCycles returns results from Rust-written DB", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		// The classifier-repo may or may not have cycles, but the
		// query must run without error on a Rust-indexed DB.
		const cycles = storage.findCycles({
			snapshotUid: snap!.snapshotUid,
		});
		expect(Array.isArray(cycles)).toBe(true);
		for (const c of cycles) {
			expect(typeof c.cycleId).toBe("string");
			expect(typeof c.length).toBe("number");
			expect(Array.isArray(c.nodes)).toBe(true);
		}
	});

	it("computeModuleStats returns results from Rust-written DB", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		const stats = storage.computeModuleStats(snap!.snapshotUid);
		// The classifier-repo has at least one module with files.
		expect(stats.length).toBeGreaterThan(0);

		// Structural validity: each result has required metric fields.
		for (const s of stats) {
			expect(typeof s.path).toBe("string");
			expect(typeof s.fanIn).toBe("number");
			expect(typeof s.fanOut).toBe("number");
			expect(typeof s.instability).toBe("number");
			expect(typeof s.abstractness).toBe("number");
			expect(typeof s.distanceFromMainSequence).toBe("number");
			expect(typeof s.fileCount).toBe("number");
			expect(typeof s.symbolCount).toBe("number");
		}
	});

	// ── Rust-16: TS formatter boundary on Rust-written DB ────────

	it("TS formatQueryResult produces valid envelope from Rust-written stats", () => {
		const snap = storage.getLatestSnapshot("classifier-repo");
		expect(snap).not.toBeNull();

		const stats = storage.computeModuleStats(snap!.snapshotUid);
		expect(stats.length).toBeGreaterThan(0);

		// Build the same QueryResult the TS CLI graph.ts would build.
		const qr: QueryResult<ModuleStats> = {
			command: "graph stats",
			repo: "classifier-repo",
			snapshot: snap!.snapshotUid,
			snapshotScope: snap!.kind === "full" ? "full" : "incremental",
			basisCommit: snap!.basisCommit ?? null,
			results: stats,
			count: stats.length,
			stale: false,
		};

		// Run through the TS JSON formatter (the user-facing boundary).
		const json = formatQueryResult(qr, formatModuleStatsResult);
		const parsed = JSON.parse(json);

		// Assert the full TS envelope shape with snake_case field names.
		expect(parsed.command).toBe("graph stats");
		expect(typeof parsed.repo).toBe("string");
		expect(typeof parsed.snapshot).toBe("string");
		expect(["full", "incremental"]).toContain(parsed.snapshot_scope);
		expect(
			parsed.basis_commit === null || typeof parsed.basis_commit === "string",
		).toBe(true);
		expect(Array.isArray(parsed.results)).toBe(true);
		expect(typeof parsed.count).toBe("number");
		expect(typeof parsed.stale).toBe("boolean");

		// Assert per-result metric field names are snake_case.
		const first = parsed.results[0];
		expect(typeof first.module).toBe("string");
		expect(typeof first.fan_in).toBe("number");
		expect(typeof first.fan_out).toBe("number");
		expect(typeof first.instability).toBe("number");
		expect(typeof first.abstractness).toBe("number");
		expect(typeof first.distance_from_main_sequence).toBe("number");
		expect(typeof first.file_count).toBe("number");
		expect(typeof first.symbol_count).toBe("number");
	});
});

// ── Rust-20: graph-query interop on a richer fixture ─────────────
//
// The classifier-repo has only one file and no resolved call chains.
// This block builds an inline fixture with callers, callees, imports,
// path, and INSTANTIATES edges, indexes it with rgr-rust, then proves
// every shipped read-side surface through the TS storage + formatter
// boundary.

import { writeFileSync, mkdirSync } from "node:fs";

describe("Rust-20: graph-query interop on rich fixture", () => {
	let richTempDir: string;
	let richDbPath: string;
	let richProvider: SqliteConnectionProvider;
	let richStorage: SqliteStorage;
	let richSnapshotUid: string;

	beforeAll(() => {
		if (!existsSync(RUST_BINARY)) {
			throw new Error(
				`Rust binary not found at ${RUST_BINARY}. Run \`cargo build -p repo-graph-rgr\` first.`,
			);
		}

		// Build inline fixture on disk.
		richTempDir = mkdtempSync(join(tmpdir(), "rgr-interop-rich-"));
		const repoDir = join(richTempDir, "repo");
		mkdirSync(join(repoDir, "src"), { recursive: true });

		writeFileSync(
			join(repoDir, "package.json"),
			'{"dependencies":{}}',
		);
		// index.ts: imports server, calls serve(), instantiates Server
		writeFileSync(
			join(repoDir, "src/index.ts"),
			[
				'import { serve, Server } from "./server";',
				"export function main() { serve(); const s = new Server(); }",
				"",
			].join("\n"),
		);
		// server.ts: imports util, exports serve() which calls helper(), exports class Server
		writeFileSync(
			join(repoDir, "src/server.ts"),
			[
				'import { helper } from "./util";',
				"export function serve() { helper(); }",
				"export class Server {}",
				"",
			].join("\n"),
		);
		// util.ts: exports helper(), exports isolated() (unreferenced)
		writeFileSync(
			join(repoDir, "src/util.ts"),
			[
				"export function helper() {}",
				"export function isolated() {}",
				"",
			].join("\n"),
		);

		// Index with rgr-rust.
		richDbPath = join(richTempDir, "rich.db");
		const cmd = `"${RUST_BINARY}" index "${repoDir}" "${richDbPath}"`;
		execSync(cmd, { encoding: "utf-8", timeout: 30000 });
		expect(existsSync(richDbPath)).toBe(true);

		// Open with TS.
		richProvider = new SqliteConnectionProvider(richDbPath);
		richProvider.initialize();
		richStorage = new SqliteStorage(richProvider.getDatabase());

		const snap = richStorage.getLatestSnapshot("repo");
		expect(snap).not.toBeNull();
		richSnapshotUid = snap!.snapshotUid;
	});

	afterAll(() => {
		if (richProvider) {
			try {
				richProvider.close();
			} catch {
				// ignore
			}
		}
		if (richTempDir && existsSync(richTempDir)) {
			rmSync(richTempDir, { recursive: true, force: true });
		}
	});

	// ── callers ──────────────────────────────────────────────

	it("findCallers returns CALLS callers from Rust-written DB", () => {
		// serve is called by main.
		const serveNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "serve" && n.kind === "SYMBOL",
		);
		expect(serveNode).toBeDefined();

		const callers = richStorage.findCallers({
			snapshotUid: richSnapshotUid,
			stableKey: serveNode!.stableKey,
			maxDepth: 1,
			edgeTypes: [EdgeType.CALLS],
		});
		expect(callers.length).toBeGreaterThan(0);
		// At least one caller should reference index.ts.
		const fromIndex = callers.find((c) => c.file.includes("index.ts"));
		expect(fromIndex).toBeDefined();
	});

	it("findCallers with INSTANTIATES returns class instantiators", () => {
		const serverClass = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "Server" && n.kind === "SYMBOL",
		);
		expect(serverClass).toBeDefined();

		const callers = richStorage.findCallers({
			snapshotUid: richSnapshotUid,
			stableKey: serverClass!.stableKey,
			maxDepth: 1,
			edgeTypes: [EdgeType.INSTANTIATES],
		});
		expect(callers.length).toBeGreaterThan(0);
		expect(callers[0].edgeType).toBe("INSTANTIATES");
	});

	// ── callers formatter boundary ──────────────────────────

	it("TS formatQueryResult produces valid callers envelope from Rust-written DB", () => {
		const serveNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "serve" && n.kind === "SYMBOL",
		);
		expect(serveNode).toBeDefined();

		const results = richStorage.findCallers({
			snapshotUid: richSnapshotUid,
			stableKey: serveNode!.stableKey,
			maxDepth: 1,
		});

		const qr: QueryResult<NodeResult> = {
			command: "graph callers",
			repo: "repo",
			snapshot: richSnapshotUid,
			snapshotScope: "full",
			basisCommit: null,
			results,
			count: results.length,
			stale: false,
		};

		const json = formatQueryResult(qr, formatNodeResult);
		const parsed = JSON.parse(json);

		expect(parsed.command).toBe("graph callers");
		expect(Array.isArray(parsed.results)).toBe(true);
		if (parsed.results.length > 0) {
			const r = parsed.results[0];
			expect(typeof r.node_id).toBe("string");
			expect(typeof r.symbol).toBe("string");
			expect(typeof r.kind).toBe("string");
			expect(typeof r.edge_type).toBe("string");
			expect(typeof r.depth).toBe("number");
		}
	});

	// ── callees ──────────────────────────────────────────────

	it("findCallees returns CALLS callees from Rust-written DB", () => {
		const mainNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "main" && n.kind === "SYMBOL",
		);
		expect(mainNode).toBeDefined();

		const callees = richStorage.findCallees({
			snapshotUid: richSnapshotUid,
			stableKey: mainNode!.stableKey,
			maxDepth: 1,
			edgeTypes: [EdgeType.CALLS],
		});
		expect(callees.length).toBeGreaterThan(0);
		const serveCallee = callees.find((c) => c.symbol.includes("serve"));
		expect(serveCallee).toBeDefined();
	});

	it("findCallees with INSTANTIATES returns instantiated classes", () => {
		const mainNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "main" && n.kind === "SYMBOL",
		);
		expect(mainNode).toBeDefined();

		const callees = richStorage.findCallees({
			snapshotUid: richSnapshotUid,
			stableKey: mainNode!.stableKey,
			maxDepth: 1,
			edgeTypes: [EdgeType.INSTANTIATES],
		});
		expect(callees.length).toBeGreaterThan(0);
		expect(callees[0].edgeType).toBe("INSTANTIATES");
	});

	// ── imports ──────────────────────────────────────────────

	it("findImports returns IMPORTS results from Rust-written DB", () => {
		const indexFile = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.stableKey === "repo:src/index.ts:FILE",
		);
		expect(indexFile).toBeDefined();

		const imports = richStorage.findImports({
			snapshotUid: richSnapshotUid,
			stableKey: indexFile!.stableKey,
			maxDepth: 1,
		});
		expect(imports.length).toBeGreaterThan(0);
		const serverImport = imports.find((i) => i.file.includes("server.ts"));
		expect(serverImport).toBeDefined();
		expect(serverImport!.edgeType).toBe("IMPORTS");
	});

	// ── path ─────────────────────────────────────────────────

	it("findPath returns shortest path from Rust-written DB", () => {
		const mainNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "main" && n.kind === "SYMBOL",
		);
		const helperNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "helper" && n.kind === "SYMBOL",
		);
		expect(mainNode).toBeDefined();
		expect(helperNode).toBeDefined();

		const result = richStorage.findPath({
			snapshotUid: richSnapshotUid,
			fromStableKey: mainNode!.stableKey,
			toStableKey: helperNode!.stableKey,
		});
		expect(result.found).toBe(true);
		// Exact path: main --CALLS--> serve --CALLS--> helper (2 edges, 3 steps).
		expect(result.pathLength).toBe(2);
		expect(result.steps).toHaveLength(3);

		expect(result.steps[0].symbol).toContain("main");
		expect(result.steps[0].edgeType).toBe("");
		expect(result.steps[1].symbol).toContain("serve");
		expect(result.steps[1].edgeType).toBe("CALLS");
		expect(result.steps[2].symbol).toContain("helper");
		expect(result.steps[2].edgeType).toBe("CALLS");
	});

	it("findPath returns not-found for disconnected symbols", () => {
		const mainNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "main" && n.kind === "SYMBOL",
		);
		const isolatedNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "isolated" && n.kind === "SYMBOL",
		);
		expect(mainNode).toBeDefined();
		expect(isolatedNode).toBeDefined();

		const result = richStorage.findPath({
			snapshotUid: richSnapshotUid,
			fromStableKey: mainNode!.stableKey,
			toStableKey: isolatedNode!.stableKey,
		});
		expect(result.found).toBe(false);
		expect(result.pathLength).toBe(0);
		expect(result.steps).toHaveLength(0);
	});

	// ── path formatter boundary ─────────────────────────────

	it("TS formatQueryResult produces valid path envelope from Rust-written DB", () => {
		const mainNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "main" && n.kind === "SYMBOL",
		);
		const serveNode = richStorage.queryAllNodes(richSnapshotUid).find(
			(n) => n.name === "serve" && n.kind === "SYMBOL",
		);
		expect(mainNode).toBeDefined();
		expect(serveNode).toBeDefined();

		const result = richStorage.findPath({
			snapshotUid: richSnapshotUid,
			fromStableKey: mainNode!.stableKey,
			toStableKey: serveNode!.stableKey,
		});

		const qr: QueryResult<PathResult> = {
			command: "graph path",
			repo: "repo",
			snapshot: richSnapshotUid,
			snapshotScope: "full",
			basisCommit: null,
			results: [result],
			count: result.found ? 1 : 0,
			stale: false,
		};

		const json = formatQueryResult(qr, formatPathResultJson);
		const parsed = JSON.parse(json);

		expect(parsed.command).toBe("graph path");
		expect(Array.isArray(parsed.results)).toBe(true);
		expect(parsed.results.length).toBe(1);

		const pr = parsed.results[0];
		expect(pr.found).toBe(true);
		expect(pr.path_length).toBe(1);
		expect(pr.path).toHaveLength(2);

		// Exact steps: main → serve.
		expect(typeof pr.path[0].node_id).toBe("string");
		expect(pr.path[0].symbol).toContain("main");
		expect(pr.path[0].edge_type).toBe("");
		expect(pr.path[1].symbol).toContain("serve");
		expect(pr.path[1].edge_type).toBe("CALLS");
	});

	// ── dead ─────────────────────────────────────────────────

	it("findDeadNodes returns dead symbols from rich Rust-written DB", () => {
		const dead = richStorage.findDeadNodes({
			snapshotUid: richSnapshotUid,
			kind: "SYMBOL",
		});
		// isolated() should be dead (nobody calls it).
		const isolatedDead = dead.find((d) => d.symbol === "isolated");
		expect(isolatedDead).toBeDefined();
		// serve should NOT be dead (called by main).
		const serveDead = dead.find((d) => d.symbol === "serve");
		expect(serveDead).toBeUndefined();
	});

});

// ── Rust-23: end-to-end violations interop ───────────────────────
//
// The rich fixture uses flat files (src/*.ts) which cannot produce
// cross-module boundary violations. This block builds a dedicated
// subdirectory fixture for the violations flow.

describe("Rust-23: violations interop on subdirectory fixture", () => {
	let violTempDir: string;
	let violDbPath: string;
	let violProvider: SqliteConnectionProvider;
	let violStorage: SqliteStorage;
	let violSnapshotUid: string;

	beforeAll(() => {
		if (!existsSync(RUST_BINARY)) {
			throw new Error(
				`Rust binary not found at ${RUST_BINARY}. Run \`cargo build -p repo-graph-rgr\` first.`,
			);
		}

		violTempDir = mkdtempSync(join(tmpdir(), "rgr-interop-viol-"));
		const repoDir = join(violTempDir, "repo");
		mkdirSync(join(repoDir, "src/core"), { recursive: true });
		mkdirSync(join(repoDir, "src/adapters"), { recursive: true });
		mkdirSync(join(repoDir, "src/util"), { recursive: true });

		writeFileSync(
			join(repoDir, "package.json"),
			'{"dependencies":{}}',
		);
		// core imports from util (allowed).
		writeFileSync(
			join(repoDir, "src/core/service.ts"),
			'import { helper } from "../util/helper";\nexport function serve() { helper(); }\n',
		);
		// adapters imports from core (violating if boundary forbids it).
		writeFileSync(
			join(repoDir, "src/adapters/store.ts"),
			'import { serve } from "../core/service";\nexport function store() { serve(); }\n',
		);
		writeFileSync(
			join(repoDir, "src/util/helper.ts"),
			"export function helper() {}\n",
		);

		violDbPath = join(violTempDir, "viol.db");
		const cmd = `"${RUST_BINARY}" index "${repoDir}" "${violDbPath}"`;
		execSync(cmd, { encoding: "utf-8", timeout: 30000 });
		expect(existsSync(violDbPath)).toBe(true);

		violProvider = new SqliteConnectionProvider(violDbPath);
		violProvider.initialize();
		violStorage = new SqliteStorage(violProvider.getDatabase());

		const snap = violStorage.getLatestSnapshot("repo");
		expect(snap).not.toBeNull();
		violSnapshotUid = snap!.snapshotUid;
	});

	afterAll(() => {
		if (violProvider) {
			try {
				violProvider.close();
			} catch {
				// ignore
			}
		}
		if (violTempDir && existsSync(violTempDir)) {
			rmSync(violTempDir, { recursive: true, force: true });
		}
	});

	it("findImportsBetweenPaths finds cross-module violations on Rust-written DB", () => {
		// adapters imports from core → findImportsBetweenPaths should find it.
		const results = violStorage.findImportsBetweenPaths({
			snapshotUid: violSnapshotUid,
			sourcePrefix: "src/adapters",
			targetPrefix: "src/core",
		});
		expect(results.length).toBe(1);
		expect(results[0].sourceFile).toContain("store.ts");
		expect(results[0].targetFile).toContain("service.ts");
	});

	it("findImportsBetweenPaths returns empty for non-violating direction", () => {
		// core does NOT import from adapters.
		const results = violStorage.findImportsBetweenPaths({
			snapshotUid: violSnapshotUid,
			sourcePrefix: "src/core",
			targetPrefix: "src/adapters",
		});
		expect(results.length).toBe(0);
	});

	it("getActiveDeclarations reads boundary declarations from Rust-written DB", () => {
		const db = violProvider.getDatabase();
		db.prepare(
			`INSERT INTO declarations
			 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
			 VALUES (?, ?, ?, 'boundary', ?, '2024-01-01T00:00:00Z', 1)`,
		).run(
			"viol-boundary-1",
			"repo",
			"repo:src/adapters:MODULE",
			'{"forbids":"src/core","reason":"clean architecture"}',
		);

		const boundaries = violStorage.getActiveDeclarations({
			repoUid: "repo",
			kind: "boundary" as never,
		});
		const found = boundaries.find(
			(d) => d.targetStableKey === "repo:src/adapters:MODULE",
		);
		expect(found).toBeDefined();
		expect(JSON.parse(found!.valueJson).forbids).toBe("src/core");

		// Clean up.
		db.prepare("DELETE FROM declarations WHERE declaration_uid = ?").run(
			"viol-boundary-1",
		);
	});

	it("end-to-end: boundary + imports produces exact violation on Rust-written DB", () => {
		// Insert boundary: src/adapters --forbids--> src/core.
		const db = violProvider.getDatabase();
		db.prepare(
			`INSERT INTO declarations
			 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
			 VALUES (?, ?, ?, 'boundary', ?, '2024-01-01T00:00:00Z', 1)`,
		).run(
			"viol-boundary-e2e",
			"repo",
			"repo:src/adapters:MODULE",
			'{"forbids":"src/core","reason":"adapters must not depend on core"}',
		);

		// Read the boundary declaration.
		const boundaries = violStorage.getActiveDeclarations({
			repoUid: "repo",
			kind: "boundary" as never,
		});
		const boundary = boundaries.find(
			(d) => d.declarationUid === "viol-boundary-e2e",
		);
		expect(boundary).toBeDefined();

		const value = JSON.parse(boundary!.valueJson) as {
			forbids: string;
			reason: string;
		};

		// Query violating imports using the same logic as arch violations.
		const stableKey = boundary!.targetStableKey;
		const moduleMatch = stableKey.match(/^[^:]+:(.+):MODULE$/);
		expect(moduleMatch).not.toBeNull();
		const boundaryModule = moduleMatch![1];

		const violations = violStorage.findImportsBetweenPaths({
			snapshotUid: violSnapshotUid,
			sourcePrefix: boundaryModule,
			targetPrefix: value.forbids,
		});

		// Exact: 1 violation (store.ts → service.ts).
		expect(violations.length).toBe(1);
		expect(violations[0].sourceFile).toContain("store.ts");
		expect(violations[0].targetFile).toContain("service.ts");
		expect(
			violations[0].line === null || typeof violations[0].line === "number",
		).toBe(true);

		// Clean up.
		db.prepare("DELETE FROM declarations WHERE declaration_uid = ?").run(
			"viol-boundary-e2e",
		);
	});
});
