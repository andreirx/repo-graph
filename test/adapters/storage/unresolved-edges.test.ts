/**
 * Round-trip tests for the unresolved_edges persistence surface.
 *
 * Covers:
 *   - insertUnresolvedEdges bulk commit
 *   - queryUnresolvedEdges filter combinations + limit
 *   - queryUnresolvedEdges deterministic ordering
 *   - queryUnresolvedEdges source-file path resolution (join through nodes → files)
 *   - countUnresolvedEdges grouped by classification / category, ordered ASC
 *
 * These tests exercise the port contract directly. They do NOT run
 * the classifier — every row's category / classification / basisCode
 * is supplied by the test harness to match the "storage is a pure
 * persister" contract.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { UnresolvedEdgeCategory } from "../../../src/core/diagnostics/unresolved-edge-categories.js";
import {
	CURRENT_CLASSIFIER_VERSION,
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../../../src/core/diagnostics/unresolved-edge-classification.js";
import type {
	GraphNode,
	TrackedFile,
} from "../../../src/core/model/index.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Resolution,
	SnapshotKind,
	Visibility,
} from "../../../src/core/model/index.js";
import type { PersistedUnresolvedEdge } from "../../../src/core/ports/storage.js";

// ── Fixture setup ────────────────────────────────────────────────────

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
const OBSERVED_AT = "2026-04-05T12:00:00.000Z";

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

function makeNode(
	snapshotUid: string,
	name: string,
	fileUid: string | null,
): GraphNode {
	const filePath = fileUid?.split(":")[1] ?? "unknown";
	return {
		nodeUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		stableKey: `${REPO_UID}:${filePath}#${name}:SYMBOL:FUNCTION`,
		kind: NodeKind.SYMBOL,
		subtype: NodeSubtype.FUNCTION,
		name,
		qualifiedName: name,
		fileUid,
		parentNodeUid: null,
		location: { lineStart: 1, colStart: 0, lineEnd: 10, colEnd: 0 },
		signature: null,
		visibility: Visibility.EXPORT,
		docComment: null,
		metadataJson: null,
	};
}

function makeUnresolvedEdge(
	snapshotUid: string,
	sourceNodeUid: string,
	overrides?: Partial<PersistedUnresolvedEdge>,
): PersistedUnresolvedEdge {
	return {
		edgeUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		sourceNodeUid,
		targetKey: "someFunc",
		type: EdgeType.CALLS,
		resolution: Resolution.STATIC,
		extractor: "ts-core:0.1.0",
		location: { lineStart: 10, colStart: 4, lineEnd: 10, colEnd: 20 },
		metadataJson: null,
		category: UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
		classification: UnresolvedEdgeClassification.UNKNOWN,
		classifierVersion: CURRENT_CLASSIFIER_VERSION,
		basisCode: UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
		observedAt: OBSERVED_AT,
		...overrides,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-unresolved-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	storage.addRepo(REPO);
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore cleanup errors
	}
});

// ── Round-trip ───────────────────────────────────────────────────────

describe("insertUnresolvedEdges / queryUnresolvedEdges round-trip", () => {
	it("persists and reads back a single observation with joined file path", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "caller", file.fileUid);
		storage.insertNodes([node]);

		const edge = makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
			targetKey: "lodash.debounce",
			classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
			basisCode: UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		});
		storage.insertUnresolvedEdges([edge]);

		const rows = storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid });
		expect(rows.length).toBe(1);
		const row = rows[0];
		expect(row.edgeUid).toBe(edge.edgeUid);
		expect(row.targetKey).toBe("lodash.debounce");
		expect(row.classification).toBe(
			UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		);
		expect(row.category).toBe(
			UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
		);
		expect(row.basisCode).toBe(
			UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT,
		);
		expect(row.sourceNodeUid).toBe(node.nodeUid);
		expect(row.sourceStableKey).toBe(node.stableKey);
		expect(row.sourceFilePath).toBe("src/a.ts");
		expect(row.lineStart).toBe(10);
		expect(row.colStart).toBe(4);
	});

	it("returns empty array for a snapshot with no unresolved edges", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		expect(
			storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid }),
		).toEqual([]);
		expect(
			storage.countUnresolvedEdges({
				snapshotUid: snap.snapshotUid,
				groupBy: "classification",
			}),
		).toEqual([]);
	});

	it("persists batch insert atomically", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "caller", file.fileUid);
		storage.insertNodes([node]);

		const edges: PersistedUnresolvedEdge[] = [];
		for (let i = 0; i < 50; i++) {
			edges.push(
				makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
					targetKey: `target${i}`,
				}),
			);
		}
		storage.insertUnresolvedEdges(edges);

		const rows = storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid });
		expect(rows.length).toBe(50);
	});

	it("leaves sourceFilePath null when source node has no file_uid", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const node = makeNode(snap.snapshotUid, "caller", null);
		storage.insertNodes([node]);

		const edge = makeUnresolvedEdge(snap.snapshotUid, node.nodeUid);
		storage.insertUnresolvedEdges([edge]);

		const rows = storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid });
		expect(rows.length).toBe(1);
		expect(rows[0].sourceFilePath).toBeNull();
	});
});

// ── Filters ──────────────────────────────────────────────────────────

describe("queryUnresolvedEdges filters", () => {
	function seed() {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "caller", file.fileUid);
		storage.insertNodes([node]);
		return { snap, node };
	}

	it("filters by classification", () => {
		const { snap, node } = seed();
		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "ext1",
				classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "int1",
				classification: UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "unk1",
				classification: UnresolvedEdgeClassification.UNKNOWN,
			}),
		]);

		const externals = storage.queryUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
		});
		expect(externals.length).toBe(1);
		expect(externals[0].targetKey).toBe("ext1");
	});

	it("filters by category", () => {
		const { snap, node } = seed();
		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "t1",
				category: UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "t2",
				category: UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			}),
		]);

		const imports = storage.queryUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			category: UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
		});
		expect(imports.length).toBe(1);
		expect(imports[0].targetKey).toBe("t2");
	});

	it("filters by basisCode", () => {
		const { snap, node } = seed();
		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "t1",
				basisCode: UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "t2",
				basisCode: UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL,
			}),
		]);

		const aliasHits = storage.queryUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			basisCode: UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS,
		});
		expect(aliasHits.length).toBe(1);
		expect(aliasHits[0].targetKey).toBe("t1");
	});

	it("combines filters with AND semantics", () => {
		const { snap, node } = seed();
		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "match",
				classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
				category: UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "wrong_classification",
				classification: UnresolvedEdgeClassification.UNKNOWN,
				category: UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "wrong_category",
				classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
				category: UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
			}),
		]);

		const rows = storage.queryUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			classification: UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
			category: UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
		});
		expect(rows.length).toBe(1);
		expect(rows[0].targetKey).toBe("match");
	});

	it("applies limit", () => {
		const { snap, node } = seed();
		const edges: PersistedUnresolvedEdge[] = [];
		for (let i = 0; i < 20; i++) {
			edges.push(
				makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
					targetKey: `t${i.toString().padStart(2, "0")}`,
				}),
			);
		}
		storage.insertUnresolvedEdges(edges);

		const rows = storage.queryUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			limit: 5,
		});
		expect(rows.length).toBe(5);
	});
});

// ── Ordering ─────────────────────────────────────────────────────────

describe("queryUnresolvedEdges deterministic ordering", () => {
	it("orders by sourceFilePath ASC, then lineStart ASC", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fileA = makeFile("src/a.ts");
		const fileB = makeFile("src/b.ts");
		storage.upsertFiles([fileA, fileB]);
		const nodeA = makeNode(snap.snapshotUid, "fnA", fileA.fileUid);
		const nodeB = makeNode(snap.snapshotUid, "fnB", fileB.fileUid);
		storage.insertNodes([nodeA, nodeB]);

		// Insert in scrambled order — expect output sorted by (path, line).
		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, nodeB.nodeUid, {
				targetKey: "b_l5",
				location: { lineStart: 5, colStart: 0, lineEnd: 5, colEnd: 10 },
			}),
			makeUnresolvedEdge(snap.snapshotUid, nodeA.nodeUid, {
				targetKey: "a_l20",
				location: { lineStart: 20, colStart: 0, lineEnd: 20, colEnd: 10 },
			}),
			makeUnresolvedEdge(snap.snapshotUid, nodeA.nodeUid, {
				targetKey: "a_l10",
				location: { lineStart: 10, colStart: 0, lineEnd: 10, colEnd: 10 },
			}),
		]);

		const rows = storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid });
		expect(rows.map((r) => r.targetKey)).toEqual(["a_l10", "a_l20", "b_l5"]);
	});

	it("orders by targetKey ASC as tiebreak when file+line tie", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "caller", file.fileUid);
		storage.insertNodes([node]);

		storage.insertUnresolvedEdges([
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "zeta",
				location: { lineStart: 5, colStart: 0, lineEnd: 5, colEnd: 10 },
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "alpha",
				location: { lineStart: 5, colStart: 0, lineEnd: 5, colEnd: 10 },
			}),
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: "mu",
				location: { lineStart: 5, colStart: 0, lineEnd: 5, colEnd: 10 },
			}),
		]);

		const rows = storage.queryUnresolvedEdges({ snapshotUid: snap.snapshotUid });
		expect(rows.map((r) => r.targetKey)).toEqual(["alpha", "mu", "zeta"]);
	});
});

// ── Counts ───────────────────────────────────────────────────────────

describe("countUnresolvedEdges", () => {
	function seedDistribution() {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const file = makeFile("src/a.ts");
		storage.upsertFiles([file]);
		const node = makeNode(snap.snapshotUid, "caller", file.fileUid);
		storage.insertNodes([node]);

		// Distribution: 3 external, 2 internal, 1 unknown
		// Categories: 4 CALLS_FUNCTION, 2 IMPORTS_FILE_NOT_FOUND
		const mk = (
			cls: UnresolvedEdgeClassification,
			cat: UnresolvedEdgeCategory,
			t: string,
		) =>
			makeUnresolvedEdge(snap.snapshotUid, node.nodeUid, {
				targetKey: t,
				classification: cls,
				category: cat,
			});

		storage.insertUnresolvedEdges([
			mk(
				UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
				"e1",
			),
			mk(
				UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
				"e2",
			),
			mk(
				UnresolvedEdgeClassification.EXTERNAL_LIBRARY_CANDIDATE,
				UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
				"e3",
			),
			mk(
				UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
				"i1",
			),
			mk(
				UnresolvedEdgeClassification.INTERNAL_CANDIDATE,
				UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND,
				"i2",
			),
			mk(
				UnresolvedEdgeClassification.UNKNOWN,
				UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
				"u1",
			),
		]);
		return snap;
	}

	it("groups by classification and returns counts ordered by key ASC", () => {
		const snap = seedDistribution();
		const rows = storage.countUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			groupBy: "classification",
		});
		expect(rows).toEqual([
			{ key: "external_library_candidate", count: 3 },
			{ key: "internal_candidate", count: 2 },
			{ key: "unknown", count: 1 },
		]);
	});

	it("groups by category and returns counts ordered by key ASC", () => {
		const snap = seedDistribution();
		const rows = storage.countUnresolvedEdges({
			snapshotUid: snap.snapshotUid,
			groupBy: "category",
		});
		expect(rows).toEqual([
			{ key: "calls_function_ambiguous_or_missing", count: 4 },
			{ key: "imports_file_not_found", count: 2 },
		]);
	});

	it("scopes counts to the given snapshot", () => {
		const snap1 = seedDistribution();
		const snap2 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "def",
		});
		// snap2 has no unresolved edges
		const rows = storage.countUnresolvedEdges({
			snapshotUid: snap2.snapshotUid,
			groupBy: "classification",
		});
		expect(rows).toEqual([]);

		// snap1 counts are unaffected
		const rows1 = storage.countUnresolvedEdges({
			snapshotUid: snap1.snapshotUid,
			groupBy: "classification",
		});
		expect(rows1.reduce((acc, r) => acc + r.count, 0)).toBe(6);
	});
});
