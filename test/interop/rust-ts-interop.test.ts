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
import type { ModuleStats, QueryResult } from "../../src/core/model/index.js";
import {
	formatQueryResult,
	formatModuleStatsResult,
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
