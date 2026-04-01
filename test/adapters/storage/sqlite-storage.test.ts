import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import type {
	Declaration,
	GraphEdge,
	GraphNode,
	TrackedFile,
} from "../../../src/core/model/index.js";
import {
	DeclarationKind,
	EdgeType,
	NodeKind,
	NodeSubtype,
	Resolution,
	SnapshotKind,
	SnapshotStatus,
	Visibility,
} from "../../../src/core/model/index.js";

let storage: SqliteStorage;
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

function makeSnapshot(repoUid = REPO_UID) {
	return storage.createSnapshot({
		repoUid,
		kind: SnapshotKind.FULL,
		basisCommit: "abc123",
	});
}

function makeFile(
	repoUid: string,
	path: string,
	overrides?: Partial<TrackedFile>,
): TrackedFile {
	return {
		fileUid: `${repoUid}:${path}`,
		repoUid,
		path,
		language: "typescript",
		isTest: false,
		isGenerated: false,
		isExcluded: false,
		...overrides,
	};
}

function makeNode(
	snapshotUid: string,
	repoUid: string,
	name: string,
	fileUid: string,
	overrides?: Partial<GraphNode>,
): GraphNode {
	// Stable key v2: includes subtype for SYMBOL nodes.
	// If overrides supply a subtype, use it in the key; default is FUNCTION.
	// If overrides supply a non-SYMBOL kind (e.g. MODULE), the subtype
	// suffix may be semantically wrong but won't collide — these test nodes
	// are not looked up by stable_key in cycle/module tests.
	const subtype = overrides?.subtype ?? NodeSubtype.FUNCTION;
	const subtypeSuffix = subtype ? `:${subtype}` : "";
	const stableKey = `${repoUid}:${fileUid.split(":")[1] ?? "unknown"}#${name}:SYMBOL${subtypeSuffix}`;
	return {
		nodeUid: randomUUID(),
		snapshotUid,
		repoUid,
		stableKey,
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
		...overrides,
	};
}

function makeEdge(
	snapshotUid: string,
	repoUid: string,
	sourceUid: string,
	targetUid: string,
	type: EdgeType = EdgeType.CALLS,
): GraphEdge {
	return {
		edgeUid: randomUUID(),
		snapshotUid,
		repoUid,
		sourceNodeUid: sourceUid,
		targetNodeUid: targetUid,
		type,
		resolution: Resolution.STATIC,
		extractor: "test:0.0.1",
		location: null,
		metadataJson: null,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-test-${randomUUID()}.db`);
	storage = new SqliteStorage(dbPath);
	storage.initialize();
	storage.addRepo(REPO);
});

afterEach(() => {
	storage.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore cleanup errors
	}
});

// ── Repo CRUD ──────────────────────────────────────────────────────────

describe("repos", () => {
	it("adds and retrieves a repo by uid", () => {
		const repo = storage.getRepo({ uid: REPO_UID });
		expect(repo).not.toBeNull();
		expect(repo?.name).toBe("test-repo");
	});

	it("retrieves a repo by name", () => {
		const repo = storage.getRepo({ name: "test-repo" });
		expect(repo).not.toBeNull();
		expect(repo?.repoUid).toBe(REPO_UID);
	});

	it("retrieves a repo by root path", () => {
		const repo = storage.getRepo({ rootPath: "/tmp/test-repo" });
		expect(repo).not.toBeNull();
	});

	it("returns null for nonexistent repo", () => {
		expect(storage.getRepo({ uid: "nope" })).toBeNull();
	});

	it("lists all repos", () => {
		const repos = storage.listRepos();
		expect(repos.length).toBe(1);
	});

	it("removes a repo", () => {
		storage.removeRepo(REPO_UID);
		expect(storage.getRepo({ uid: REPO_UID })).toBeNull();
	});
});

// ── Snapshots ──────────────────────────────────────────────────────────

describe("snapshots", () => {
	it("creates a snapshot in BUILDING status", () => {
		const snap = makeSnapshot();
		expect(snap.status).toBe(SnapshotStatus.BUILDING);
		expect(snap.repoUid).toBe(REPO_UID);
	});

	it("updates snapshot status", () => {
		const snap = makeSnapshot();
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});
		const updated = storage.getSnapshot(snap.snapshotUid);
		expect(updated?.status).toBe(SnapshotStatus.READY);
	});

	it("gets latest ready snapshot", () => {
		const snap = makeSnapshot();
		expect(storage.getLatestSnapshot(REPO_UID)).toBeNull(); // still BUILDING

		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});
		const latest = storage.getLatestSnapshot(REPO_UID);
		expect(latest).not.toBeNull();
		expect(latest?.snapshotUid).toBe(snap.snapshotUid);
	});
});

// ── Files ──────────────────────────────────────────────────────────────

describe("files", () => {
	it("upserts and retrieves files", () => {
		const files = [
			makeFile(REPO_UID, "src/a.ts"),
			makeFile(REPO_UID, "src/b.ts"),
		];
		storage.upsertFiles(files);
		const result = storage.getFilesByRepo(REPO_UID);
		expect(result.length).toBe(2);
	});

	it("excludes is_excluded files from getFilesByRepo", () => {
		storage.upsertFiles([
			makeFile(REPO_UID, "src/a.ts"),
			makeFile(REPO_UID, "src/excluded.ts", { isExcluded: true }),
		]);
		const result = storage.getFilesByRepo(REPO_UID);
		expect(result.length).toBe(1);
		expect(result[0].path).toBe("src/a.ts");
	});
});

// ── Nodes & Edges ──────────────────────────────────────────────────────

describe("nodes and edges", () => {
	it("inserts nodes and retrieves by stable key", () => {
		const snap = makeSnapshot();
		const file = makeFile(REPO_UID, "src/a.ts");
		storage.upsertFiles([file]);

		const node = makeNode(snap.snapshotUid, REPO_UID, "doStuff", file.fileUid);
		storage.insertNodes([node]);

		const found = storage.getNodeByStableKey(snap.snapshotUid, node.stableKey);
		expect(found).not.toBeNull();
		expect(found?.name).toBe("doStuff");
	});

	it("deletes nodes and edges by file", () => {
		const snap = makeSnapshot();
		const fileA = makeFile(REPO_UID, "src/a.ts");
		const fileB = makeFile(REPO_UID, "src/b.ts");
		storage.upsertFiles([fileA, fileB]);

		const nodeA = makeNode(snap.snapshotUid, REPO_UID, "fnA", fileA.fileUid);
		const nodeB = makeNode(snap.snapshotUid, REPO_UID, "fnB", fileB.fileUid);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
		);
		storage.insertEdges([edge]);

		storage.deleteNodesByFile(snap.snapshotUid, fileA.fileUid);

		// nodeA and its edge should be gone
		expect(
			storage.getNodeByStableKey(snap.snapshotUid, nodeA.stableKey),
		).toBeNull();
		// nodeB should remain
		expect(
			storage.getNodeByStableKey(snap.snapshotUid, nodeB.stableKey),
		).not.toBeNull();
	});
});

// ── Declarations ───────────────────────────────────────────────────────

describe("declarations", () => {
	it("inserts and retrieves active declarations", () => {
		const decl: Declaration = {
			declarationUid: randomUUID(),
			repoUid: REPO_UID,
			snapshotUid: null,
			targetStableKey: `${REPO_UID}:src/core#PaymentService:MODULE`,
			kind: DeclarationKind.ENTRYPOINT,
			valueJson: JSON.stringify({ type: "route_handler" }),
			createdAt: new Date().toISOString(),
			createdBy: "test",
			supersedesUid: null,
			isActive: true,
		};
		storage.insertDeclaration(decl);

		const results = storage.getActiveDeclarations({ repoUid: REPO_UID });
		expect(results.length).toBe(1);
		expect(results[0].kind).toBe(DeclarationKind.ENTRYPOINT);
	});

	it("deactivates a declaration", () => {
		const decl: Declaration = {
			declarationUid: randomUUID(),
			repoUid: REPO_UID,
			snapshotUid: null,
			targetStableKey: "test-key",
			kind: DeclarationKind.BOUNDARY,
			valueJson: JSON.stringify({ forbids: "src/infra" }),
			createdAt: new Date().toISOString(),
			createdBy: null,
			supersedesUid: null,
			isActive: true,
		};
		storage.insertDeclaration(decl);
		storage.deactivateDeclaration(decl.declarationUid);

		const results = storage.getActiveDeclarations({ repoUid: REPO_UID });
		expect(results.length).toBe(0);
	});
});

// ── Graph Queries ──────────────────────────────────────────────────────

describe("graph queries", () => {
	// Build a small graph:
	//   A -> B -> C
	//   A -> D
	//   E (dead — no incoming edges)
	let snap: ReturnType<typeof makeSnapshot>;
	let nodeA: GraphNode;
	let nodeB: GraphNode;
	let nodeC: GraphNode;
	let nodeD: GraphNode;
	let nodeE: GraphNode;

	beforeEach(() => {
		snap = makeSnapshot();
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});

		const file = makeFile(REPO_UID, "src/main.ts");
		storage.upsertFiles([file]);

		nodeA = makeNode(snap.snapshotUid, REPO_UID, "fnA", file.fileUid);
		nodeB = makeNode(snap.snapshotUid, REPO_UID, "fnB", file.fileUid);
		nodeC = makeNode(snap.snapshotUid, REPO_UID, "fnC", file.fileUid);
		nodeD = makeNode(snap.snapshotUid, REPO_UID, "fnD", file.fileUid);
		nodeE = makeNode(snap.snapshotUid, REPO_UID, "fnE", file.fileUid);
		storage.insertNodes([nodeA, nodeB, nodeC, nodeD, nodeE]);

		storage.insertEdges([
			makeEdge(snap.snapshotUid, REPO_UID, nodeA.nodeUid, nodeB.nodeUid),
			makeEdge(snap.snapshotUid, REPO_UID, nodeB.nodeUid, nodeC.nodeUid),
			makeEdge(snap.snapshotUid, REPO_UID, nodeA.nodeUid, nodeD.nodeUid),
		]);
	});

	describe("findCallers", () => {
		it("finds direct callers", () => {
			const callers = storage.findCallers({
				snapshotUid: snap.snapshotUid,
				stableKey: nodeB.stableKey,
			});
			expect(callers.length).toBe(1);
			expect(callers[0].symbol).toBe("fnA");
			expect(callers[0].depth).toBe(1);
		});

		it("finds no callers for root node", () => {
			const callers = storage.findCallers({
				snapshotUid: snap.snapshotUid,
				stableKey: nodeA.stableKey,
			});
			expect(callers.length).toBe(0);
		});
	});

	describe("findCallees", () => {
		it("finds direct callees", () => {
			const callees = storage.findCallees({
				snapshotUid: snap.snapshotUid,
				stableKey: nodeA.stableKey,
			});
			expect(callees.length).toBe(2);
			const names = callees.map((c) => c.symbol).sort();
			expect(names).toEqual(["fnB", "fnD"]);
		});

		it("finds transitive callees with depth", () => {
			const callees = storage.findCallees({
				snapshotUid: snap.snapshotUid,
				stableKey: nodeA.stableKey,
				maxDepth: 2,
			});
			const names = callees.map((c) => c.symbol).sort();
			expect(names).toContain("fnC"); // A -> B -> C
		});
	});

	describe("findPath", () => {
		it("finds shortest path between two nodes", () => {
			const result = storage.findPath({
				snapshotUid: snap.snapshotUid,
				fromStableKey: nodeA.stableKey,
				toStableKey: nodeC.stableKey,
				edgeTypes: [EdgeType.CALLS],
			});
			expect(result.found).toBe(true);
			expect(result.pathLength).toBe(2); // A -> B -> C
			expect(result.steps.length).toBe(3); // 3 nodes in path
		});

		it("returns not found for disconnected nodes", () => {
			const result = storage.findPath({
				snapshotUid: snap.snapshotUid,
				fromStableKey: nodeE.stableKey,
				toStableKey: nodeA.stableKey,
				edgeTypes: [EdgeType.CALLS],
			});
			expect(result.found).toBe(false);
		});
	});

	describe("findDeadNodes", () => {
		it("finds nodes with no incoming edges", () => {
			const dead = storage.findDeadNodes({
				snapshotUid: snap.snapshotUid,
			});
			// A and E have no incoming CALLS edges
			// (A calls others but nobody calls A)
			const names = dead.map((d) => d.symbol).sort();
			expect(names).toContain("fnA");
			expect(names).toContain("fnE");
			expect(names).not.toContain("fnB"); // B is called by A
		});

		it("excludes declared entrypoints from dead nodes", () => {
			// Declare A as an entrypoint
			storage.insertDeclaration({
				declarationUid: randomUUID(),
				repoUid: REPO_UID,
				snapshotUid: null,
				targetStableKey: nodeA.stableKey,
				kind: DeclarationKind.ENTRYPOINT,
				valueJson: JSON.stringify({ type: "route_handler" }),
				createdAt: new Date().toISOString(),
				createdBy: "test",
				supersedesUid: null,
				isActive: true,
			});

			const dead = storage.findDeadNodes({
				snapshotUid: snap.snapshotUid,
			});
			const names = dead.map((d) => d.symbol);
			expect(names).not.toContain("fnA"); // excluded by declaration
			expect(names).toContain("fnE"); // still dead
		});
	});

	describe("resolveSymbol", () => {
		it("finds symbols by partial name", () => {
			const results = storage.resolveSymbol({
				snapshotUid: snap.snapshotUid,
				query: "fnB",
			});
			expect(results.length).toBe(1);
			expect(results[0].name).toBe("fnB");
		});

		it("returns empty for no match", () => {
			const results = storage.resolveSymbol({
				snapshotUid: snap.snapshotUid,
				query: "nonexistent",
			});
			expect(results.length).toBe(0);
		});
	});

	describe("findCycles", () => {
		it("detects a three-node cycle exactly once", () => {
			// Build a cycle: modX -> modY -> modZ -> modX
			const file = makeFile(REPO_UID, "src/modules.ts");
			storage.upsertFiles([file]);

			const modX = makeNode(snap.snapshotUid, REPO_UID, "modX", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modY = makeNode(snap.snapshotUid, REPO_UID, "modY", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modZ = makeNode(snap.snapshotUid, REPO_UID, "modZ", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			storage.insertNodes([modX, modY, modZ]);

			storage.insertEdges([
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modX.nodeUid,
					modY.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modY.nodeUid,
					modZ.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modZ.nodeUid,
					modX.nodeUid,
					EdgeType.IMPORTS,
				),
			]);

			const cycles = storage.findCycles({
				snapshotUid: snap.snapshotUid,
				level: "module",
			});

			// Must produce exactly one canonical cycle, not three rotations
			expect(cycles.length).toBe(1);
			expect(cycles[0].length).toBe(3); // 3 edges in the ring
			expect(cycles[0].nodes.length).toBe(3); // 3 unique nodes
			const names = cycles[0].nodes.map((n) => n.name).sort();
			expect(names).toEqual(["modX", "modY", "modZ"]);
		});

		it("returns empty when there are no cycles", () => {
			// The existing graph (A->B->C, A->D, E) has no module-level IMPORTS cycles
			const cycles = storage.findCycles({
				snapshotUid: snap.snapshotUid,
				level: "module",
			});
			expect(cycles.length).toBe(0);
		});

		it("detects multiple distinct cycles", () => {
			const file = makeFile(REPO_UID, "src/multi.ts");
			storage.upsertFiles([file]);

			// Cycle 1: p -> q -> p
			const modP = makeNode(snap.snapshotUid, REPO_UID, "modP", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modQ = makeNode(snap.snapshotUid, REPO_UID, "modQ", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			// Cycle 2: r -> s -> t -> r
			const modR = makeNode(snap.snapshotUid, REPO_UID, "modR", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modS = makeNode(snap.snapshotUid, REPO_UID, "modS", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modT = makeNode(snap.snapshotUid, REPO_UID, "modT", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			storage.insertNodes([modP, modQ, modR, modS, modT]);

			storage.insertEdges([
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modP.nodeUid,
					modQ.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modQ.nodeUid,
					modP.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modR.nodeUid,
					modS.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modS.nodeUid,
					modT.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modT.nodeUid,
					modR.nodeUid,
					EdgeType.IMPORTS,
				),
			]);

			const cycles = storage.findCycles({
				snapshotUid: snap.snapshotUid,
				level: "module",
			});

			expect(cycles.length).toBe(2);
			const cycleLengths = cycles.map((c) => c.length).sort();
			expect(cycleLengths).toEqual([2, 3]); // one 2-edge cycle, one 3-edge cycle
		});
	});
});
