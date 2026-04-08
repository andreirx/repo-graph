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
import type { GraphNode, TrackedFile } from "../../../src/core/model/index.js";
import { NodeKind, NodeSubtype, SnapshotKind, Visibility } from "../../../src/core/model/index.js";
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

// ── queryAllNodes (Phase 3 read-back) ───────────────────────────────

describe("queryAllNodes round-trip", () => {
	function makeNode(
		snapshotUid: string,
		name: string,
		fileUid: string,
	): GraphNode {
		return {
			nodeUid: randomUUID(),
			snapshotUid,
			repoUid: REPO_UID,
			stableKey: `${REPO_UID}:${fileUid.split(":")[1] ?? "x"}#${name}:SYMBOL:FUNCTION`,
			kind: NodeKind.SYMBOL,
			subtype: NodeSubtype.FUNCTION,
			name,
			qualifiedName: name,
			fileUid,
			parentNodeUid: null,
			location: { lineStart: 1, colStart: 0, lineEnd: 10, colEnd: 0 },
			signature: `${name}()`,
			visibility: Visibility.EXPORT,
			docComment: null,
			metadataJson: null,
		};
	}

	function makeFile(path: string): TrackedFile {
		return {
			fileUid: `${REPO_UID}:${path}`,
			repoUid: REPO_UID,
			path,
			language: "typescript",
			isTest: false,
			isGenerated: false,
			isExcluded: false,
		};
	}

	it("reads back inserted nodes with all resolver-critical fields", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "helper", file.fileUid);
		storage.insertNodes([node]);

		const all = storage.queryAllNodes(snap.snapshotUid);
		expect(all.length).toBe(1);
		const n = all[0];
		expect(n.nodeUid).toBe(node.nodeUid);
		expect(n.stableKey).toBe(node.stableKey);
		expect(n.name).toBe("helper");
		expect(n.qualifiedName).toBe("helper");
		expect(n.kind).toBe(NodeKind.SYMBOL);
		expect(n.subtype).toBe(NodeSubtype.FUNCTION);
		expect(n.fileUid).toBe(file.fileUid);
		expect(n.signature).toBe("helper()");
		expect(n.visibility).toBe(Visibility.EXPORT);
	});

	it("reads back multiple nodes", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		storage.insertNodes([
			makeNode(snap.snapshotUid, "foo", file.fileUid),
			makeNode(snap.snapshotUid, "bar", file.fileUid),
		]);

		const all = storage.queryAllNodes(snap.snapshotUid);
		expect(all.length).toBe(2);
	});

	it("scopes to snapshot", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		storage.insertNodes([makeNode(snap1.snapshotUid, "inSnap1", file.fileUid)]);
		storage.insertNodes([makeNode(snap2.snapshotUid, "inSnap2", file.fileUid)]);

		const n1 = storage.queryAllNodes(snap1.snapshotUid);
		expect(n1.length).toBe(1);
		expect(n1[0].name).toBe("inSnap1");

		const n2 = storage.queryAllNodes(snap2.snapshotUid);
		expect(n2.length).toBe(1);
		expect(n2[0].name).toBe("inSnap2");
	});

	it("returns empty for snapshot with no nodes", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		expect(storage.queryAllNodes(snap.snapshotUid)).toEqual([]);
	});

	it("hydrates location correctly", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "fn", file.fileUid);
		storage.insertNodes([node]);

		const all = storage.queryAllNodes(snap.snapshotUid);
		expect(all[0].location).not.toBeNull();
		expect(all[0].location!.lineStart).toBe(1);
		expect(all[0].location!.lineEnd).toBe(10);
	});
});
