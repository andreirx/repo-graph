/**
 * Staging tables round-trip tests.
 *
 * Covers:
 *   - insertStagedEdges / queryStagedEdges / deleteStagedEdges
 *   - insertFileSignals / queryFileSignals / queryAllFileSignals / deleteFileSignals
 *   - Snapshot scoping
 *   - Cleanup semantics
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { SnapshotKind } from "../../../src/core/model/index.js";
import type { StagedEdge, FileSignalRow } from "../../../src/core/ports/storage.js";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let dbPath: string;

const REPO_UID = "test-repo";
const REPO = {
	repoUid: REPO_UID,
	name: "test-repo",
	rootPath: "/tmp/test-repo",
	defaultBranch: "main",
	createdAt: new Date().toISOString(),
	metadataJson: null,
};

function makeStagedEdge(
	snapshotUid: string,
	overrides?: Partial<StagedEdge>,
): StagedEdge {
	return {
		edgeUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		sourceNodeUid: randomUUID(),
		targetKey: "someFunc",
		type: "CALLS",
		resolution: "static",
		extractor: "test:0.1",
		lineStart: 10,
		colStart: 4,
		lineEnd: 10,
		colEnd: 20,
		metadataJson: null,
		sourceFileUid: `${REPO_UID}:src/a.ts`,
		...overrides,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-staging-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	storage.addRepo(REPO);
});

afterEach(() => {
	provider.close();
	try { unlinkSync(dbPath); } catch { /* ignore */ }
});

// ── Staged edges ────────────────────────────────────────────────────

describe("staged_edges round-trip", () => {
	it("inserts and reads back staged edges", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const edge = makeStagedEdge(snap.snapshotUid);
		storage.insertStagedEdges([edge]);

		const rows = storage.queryStagedEdges(snap.snapshotUid);
		expect(rows.length).toBe(1);
		expect(rows[0].edgeUid).toBe(edge.edgeUid);
		expect(rows[0].targetKey).toBe("someFunc");
		expect(rows[0].type).toBe("CALLS");
		expect(rows[0].sourceFileUid).toBe(`${REPO_UID}:src/a.ts`);
		expect(rows[0].lineStart).toBe(10);
	});

	it("inserts batch atomically", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const edges = Array.from({ length: 50 }, () => makeStagedEdge(snap.snapshotUid));
		storage.insertStagedEdges(edges);

		const rows = storage.queryStagedEdges(snap.snapshotUid);
		expect(rows.length).toBe(50);
	});

	it("scopes to snapshot", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertStagedEdges([makeStagedEdge(snap1.snapshotUid)]);
		storage.insertStagedEdges([makeStagedEdge(snap2.snapshotUid)]);

		expect(storage.queryStagedEdges(snap1.snapshotUid).length).toBe(1);
		expect(storage.queryStagedEdges(snap2.snapshotUid).length).toBe(1);
	});

	it("deletes staged edges by snapshot", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertStagedEdges([
			makeStagedEdge(snap.snapshotUid),
			makeStagedEdge(snap.snapshotUid),
		]);
		expect(storage.queryStagedEdges(snap.snapshotUid).length).toBe(2);

		storage.deleteStagedEdges(snap.snapshotUid);
		expect(storage.queryStagedEdges(snap.snapshotUid).length).toBe(0);
	});

	it("delete does not affect other snapshots", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertStagedEdges([makeStagedEdge(snap1.snapshotUid)]);
		storage.insertStagedEdges([makeStagedEdge(snap2.snapshotUid)]);

		storage.deleteStagedEdges(snap1.snapshotUid);
		expect(storage.queryStagedEdges(snap1.snapshotUid).length).toBe(0);
		expect(storage.queryStagedEdges(snap2.snapshotUid).length).toBe(1);
	});

	it("returns empty for snapshot with no staged edges", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		expect(storage.queryStagedEdges(snap.snapshotUid)).toEqual([]);
	});
});

// ── File signals ────────────────────────────────────────────────────

describe("file_signals round-trip", () => {
	it("inserts and reads back file signals", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const signal: FileSignalRow = {
			snapshotUid: snap.snapshotUid,
			fileUid: `${REPO_UID}:src/a.ts`,
			importBindingsJson: JSON.stringify([
				{ identifier: "foo", specifier: "./foo", isRelative: true, isTypeOnly: false },
			]),
		};
		storage.insertFileSignals([signal]);

		const row = storage.queryFileSignals(snap.snapshotUid, `${REPO_UID}:src/a.ts`);
		expect(row).not.toBeNull();
		expect(row!.fileUid).toBe(`${REPO_UID}:src/a.ts`);

		const bindings = JSON.parse(row!.importBindingsJson!);
		expect(bindings.length).toBe(1);
		expect(bindings[0].identifier).toBe("foo");
	});

	it("returns null for missing file", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const row = storage.queryFileSignals(snap.snapshotUid, "nonexistent");
		expect(row).toBeNull();
	});

	it("queryAllFileSignals returns all signals for a snapshot", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]" },
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:b.ts`, importBindingsJson: "[]" },
		]);

		const all = storage.queryAllFileSignals(snap.snapshotUid);
		expect(all.length).toBe(2);
	});

	it("scopes to snapshot", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap1.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]" },
		]);

		expect(storage.queryFileSignals(snap1.snapshotUid, `${REPO_UID}:a.ts`)).not.toBeNull();
		expect(storage.queryFileSignals(snap2.snapshotUid, `${REPO_UID}:a.ts`)).toBeNull();
	});

	it("deletes file signals by snapshot", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]" },
		]);
		expect(storage.queryAllFileSignals(snap.snapshotUid).length).toBe(1);

		storage.deleteFileSignals(snap.snapshotUid);
		expect(storage.queryAllFileSignals(snap.snapshotUid).length).toBe(0);
	});

	it("upserts on conflict (same snapshot + file)", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: '[{"old":true}]' },
		]);
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: '[{"new":true}]' },
		]);

		const row = storage.queryFileSignals(snap.snapshotUid, `${REPO_UID}:a.ts`);
		expect(row!.importBindingsJson).toBe('[{"new":true}]');
	});
});
