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
});
