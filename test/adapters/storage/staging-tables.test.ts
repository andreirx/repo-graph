/**
 * Extraction edges and file signals round-trip tests.
 *
 * Covers:
 *   - insertExtractionEdges / queryExtractionEdgesBatch
 *   - queryExtractionEdgesByFiles / copyExtractionEdgesForFiles
 *   - insertFileSignals / queryFileSignals / queryAllFileSignals / deleteFileSignals
 *   - Snapshot scoping
 *   - Durable retention (no delete-on-finalize)
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
import type { ExtractionEdge, FileSignalRow } from "../../../src/core/ports/storage.js";

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

function makeExtractionEdge(
	snapshotUid: string,
	overrides?: Partial<ExtractionEdge>,
): ExtractionEdge {
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
	dbPath = join(tmpdir(), `rgr-extraction-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	storage.addRepo(REPO);
});

afterEach(() => {
	provider.close();
	try { unlinkSync(dbPath); } catch { /* ignore */ }
});

// ── Extraction edges ──────────────────────────────────────────────

describe("extraction_edges round-trip", () => {
	it("inserts and reads back extraction edges via batch cursor", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const edge = makeExtractionEdge(snap.snapshotUid);
		storage.insertExtractionEdges([edge]);

		const rows = storage.queryExtractionEdgesBatch(snap.snapshotUid, 100, null);
		expect(rows.length).toBe(1);
		expect(rows[0].edgeUid).toBe(edge.edgeUid);
		expect(rows[0].targetKey).toBe("someFunc");
		expect(rows[0].type).toBe("CALLS");
		expect(rows[0].sourceFileUid).toBe(`${REPO_UID}:src/a.ts`);
		expect(rows[0].lineStart).toBe(10);
	});

	it("inserts batch atomically", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const edges = Array.from({ length: 50 }, () => makeExtractionEdge(snap.snapshotUid));
		storage.insertExtractionEdges(edges);

		const rows = storage.queryExtractionEdgesBatch(snap.snapshotUid, 100, null);
		expect(rows.length).toBe(50);
	});

	it("batch cursor paginates correctly", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const edges = Array.from({ length: 5 }, () => makeExtractionEdge(snap.snapshotUid));
		storage.insertExtractionEdges(edges);

		const page1 = storage.queryExtractionEdgesBatch(snap.snapshotUid, 2, null);
		expect(page1.length).toBe(2);

		const page2 = storage.queryExtractionEdgesBatch(snap.snapshotUid, 2, page1[1].edgeUid);
		expect(page2.length).toBe(2);

		const page3 = storage.queryExtractionEdgesBatch(snap.snapshotUid, 2, page2[1].edgeUid);
		expect(page3.length).toBe(1);

		// All 5 edges covered.
		const allUids = [...page1, ...page2, ...page3].map((e) => e.edgeUid);
		expect(new Set(allUids).size).toBe(5);
	});

	it("scopes to snapshot", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertExtractionEdges([makeExtractionEdge(snap1.snapshotUid)]);
		storage.insertExtractionEdges([makeExtractionEdge(snap2.snapshotUid)]);

		expect(storage.queryExtractionEdgesBatch(snap1.snapshotUid, 100, null).length).toBe(1);
		expect(storage.queryExtractionEdgesBatch(snap2.snapshotUid, 100, null).length).toBe(1);
	});

	it("returns empty for snapshot with no extraction edges", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		expect(storage.queryExtractionEdgesBatch(snap.snapshotUid, 100, null)).toEqual([]);
	});
});

// ── Per-file queries ──────────────────────────────────────────────

describe("extraction_edges per-file queries", () => {
	it("queryExtractionEdgesByFiles returns edges for specified files only", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertExtractionEdges([
			makeExtractionEdge(snap.snapshotUid, { sourceFileUid: `${REPO_UID}:src/a.ts` }),
			makeExtractionEdge(snap.snapshotUid, { sourceFileUid: `${REPO_UID}:src/b.ts` }),
			makeExtractionEdge(snap.snapshotUid, { sourceFileUid: `${REPO_UID}:src/c.ts` }),
		]);

		const result = storage.queryExtractionEdgesByFiles(
			snap.snapshotUid,
			[`${REPO_UID}:src/a.ts`, `${REPO_UID}:src/c.ts`],
		);
		expect(result.length).toBe(2);
		const files = result.map((e) => e.sourceFileUid).sort();
		expect(files).toEqual([`${REPO_UID}:src/a.ts`, `${REPO_UID}:src/c.ts`]);
	});

	it("returns empty for non-matching files", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertExtractionEdges([
			makeExtractionEdge(snap.snapshotUid, { sourceFileUid: `${REPO_UID}:src/a.ts` }),
		]);

		const result = storage.queryExtractionEdgesByFiles(
			snap.snapshotUid,
			[`${REPO_UID}:src/nonexistent.ts`],
		);
		expect(result).toEqual([]);
	});
});

// ── Cross-snapshot copy ───────────────────────────────────────────

describe("extraction_edges cross-snapshot copy", () => {
	it("copies edges from parent to child snapshot for specified files", () => {
		const parent = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertExtractionEdges([
			makeExtractionEdge(parent.snapshotUid, { sourceFileUid: `${REPO_UID}:src/a.ts` }),
			makeExtractionEdge(parent.snapshotUid, { sourceFileUid: `${REPO_UID}:src/a.ts` }),
			makeExtractionEdge(parent.snapshotUid, { sourceFileUid: `${REPO_UID}:src/b.ts` }),
		]);

		const child = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.REFRESH,
			parentSnapshotUid: parent.snapshotUid,
		});

		// Copy only src/a.ts edges to child.
		const copied = storage.copyExtractionEdgesForFiles(
			parent.snapshotUid,
			child.snapshotUid,
			REPO_UID,
			[`${REPO_UID}:src/a.ts`],
		);
		expect(copied).toBe(2);

		// Child should have 2 edges (from a.ts), parent still has 3.
		const childEdges = storage.queryExtractionEdgesBatch(child.snapshotUid, 100, null);
		expect(childEdges.length).toBe(2);
		expect(childEdges.every((e) => e.snapshotUid === child.snapshotUid)).toBe(true);
		expect(childEdges.every((e) => e.sourceFileUid === `${REPO_UID}:src/a.ts`)).toBe(true);

		const parentEdges = storage.queryExtractionEdgesBatch(parent.snapshotUid, 100, null);
		expect(parentEdges.length).toBe(3);
	});

	it("copies zero edges for empty file list", () => {
		const parent = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertExtractionEdges([makeExtractionEdge(parent.snapshotUid)]);

		const child = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.REFRESH,
			parentSnapshotUid: parent.snapshotUid,
		});

		const copied = storage.copyExtractionEdgesForFiles(
			parent.snapshotUid,
			child.snapshotUid,
			REPO_UID,
			[],
		);
		expect(copied).toBe(0);
	});
});

// ── File signals ──────────────────────────────────────────────────

describe("file_signals round-trip", () => {
	it("inserts and reads back file signals", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const signal: FileSignalRow = {
			snapshotUid: snap.snapshotUid,
			fileUid: `${REPO_UID}:src/a.ts`,
			importBindingsJson: JSON.stringify([
				{ identifier: "foo", specifier: "./foo", isRelative: true, isTypeOnly: false },
			]),
			packageDependenciesJson: null,
			tsconfigAliasesJson: null,
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
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]", packageDependenciesJson: null, tsconfigAliasesJson: null },
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:b.ts`, importBindingsJson: "[]", packageDependenciesJson: null, tsconfigAliasesJson: null },
		]);

		const all = storage.queryAllFileSignals(snap.snapshotUid);
		expect(all.length).toBe(2);
	});

	it("scopes to snapshot", () => {
		const snap1 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		const snap2 = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap1.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]", packageDependenciesJson: null, tsconfigAliasesJson: null },
		]);

		expect(storage.queryFileSignals(snap1.snapshotUid, `${REPO_UID}:a.ts`)).not.toBeNull();
		expect(storage.queryFileSignals(snap2.snapshotUid, `${REPO_UID}:a.ts`)).toBeNull();
	});

	it("deletes file signals by snapshot", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: "[]", packageDependenciesJson: null, tsconfigAliasesJson: null },
		]);
		expect(storage.queryAllFileSignals(snap.snapshotUid).length).toBe(1);

		storage.deleteFileSignals(snap.snapshotUid);
		expect(storage.queryAllFileSignals(snap.snapshotUid).length).toBe(0);
	});

	it("upserts on conflict (same snapshot + file)", () => {
		const snap = storage.createSnapshot({ repoUid: REPO_UID, kind: SnapshotKind.FULL });
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: '[{"old":true}]', packageDependenciesJson: null, tsconfigAliasesJson: null },
		]);
		storage.insertFileSignals([
			{ snapshotUid: snap.snapshotUid, fileUid: `${REPO_UID}:a.ts`, importBindingsJson: '[{"new":true}]', packageDependenciesJson: null, tsconfigAliasesJson: null },
		]);

		const row = storage.queryFileSignals(snap.snapshotUid, `${REPO_UID}:a.ts`);
		expect(row!.importBindingsJson).toBe('[{"new":true}]');
	});
});

// ── queryAllNodes (Phase 3 read-back) ─────────────────────────────

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
