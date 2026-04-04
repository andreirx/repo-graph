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
			authoredBasisJson: null,
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
			authoredBasisJson: null,
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
				authoredBasisJson: null,
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

	describe("computeModuleStats", () => {
		it("computes fan-in and fan-out for modules with IMPORTS edges", () => {
			// Build: modA -> modB -> modC
			const file = makeFile(REPO_UID, "src/test.ts");
			storage.upsertFiles([file]);

			const modA = makeNode(snap.snapshotUid, REPO_UID, "modA", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modB = makeNode(snap.snapshotUid, REPO_UID, "modB", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			const modC = makeNode(snap.snapshotUid, REPO_UID, "modC", file.fileUid, {
				kind: NodeKind.MODULE,
				subtype: null,
			});
			// Give each module a file via OWNS edge so they appear in results
			const fileA = makeFile(REPO_UID, "modA/a.ts");
			const fileB = makeFile(REPO_UID, "modB/b.ts");
			const fileC = makeFile(REPO_UID, "modC/c.ts");
			storage.upsertFiles([fileA, fileB, fileC]);

			const fileNodeA = makeNode(
				snap.snapshotUid,
				REPO_UID,
				"a.ts",
				fileA.fileUid,
				{ kind: NodeKind.FILE, subtype: null },
			);
			const fileNodeB = makeNode(
				snap.snapshotUid,
				REPO_UID,
				"b.ts",
				fileB.fileUid,
				{ kind: NodeKind.FILE, subtype: null },
			);
			const fileNodeC = makeNode(
				snap.snapshotUid,
				REPO_UID,
				"c.ts",
				fileC.fileUid,
				{ kind: NodeKind.FILE, subtype: null },
			);
			storage.insertNodes([modA, modB, modC, fileNodeA, fileNodeB, fileNodeC]);

			storage.insertEdges([
				// Module IMPORTS
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modA.nodeUid,
					modB.nodeUid,
					EdgeType.IMPORTS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modB.nodeUid,
					modC.nodeUid,
					EdgeType.IMPORTS,
				),
				// OWNS edges
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modA.nodeUid,
					fileNodeA.nodeUid,
					EdgeType.OWNS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modB.nodeUid,
					fileNodeB.nodeUid,
					EdgeType.OWNS,
				),
				makeEdge(
					snap.snapshotUid,
					REPO_UID,
					modC.nodeUid,
					fileNodeC.nodeUid,
					EdgeType.OWNS,
				),
			]);

			const stats = storage.computeModuleStats(snap.snapshotUid);
			const byName = new Map(stats.map((s) => [s.name, s]));

			const a = byName.get("modA");
			expect(a).toBeDefined();
			expect(a?.fanIn).toBe(0);
			expect(a?.fanOut).toBe(1);
			expect(a?.instability).toBe(1);

			const b = byName.get("modB");
			expect(b).toBeDefined();
			expect(b?.fanIn).toBe(1);
			expect(b?.fanOut).toBe(1);
			expect(b?.instability).toBe(0.5);

			const c = byName.get("modC");
			expect(c).toBeDefined();
			expect(c?.fanIn).toBe(1);
			expect(c?.fanOut).toBe(0);
			expect(c?.instability).toBe(0);
		});

		it("excludes modules with zero files", () => {
			// Module with no OWNS edges should not appear
			const file = makeFile(REPO_UID, "src/empty-test.ts");
			storage.upsertFiles([file]);

			const emptyMod = makeNode(
				snap.snapshotUid,
				REPO_UID,
				"emptyMod",
				file.fileUid,
				{ kind: NodeKind.MODULE, subtype: null },
			);
			storage.insertNodes([emptyMod]);

			const stats = storage.computeModuleStats(snap.snapshotUid);
			const names = stats.map((s) => s.name);
			expect(names).not.toContain("emptyMod");
		});
	});

	describe("insertMeasurements", () => {
		it("inserts and stores measurements", () => {
			storage.insertMeasurements([
				{
					measurementUid: randomUUID(),
					snapshotUid: snap.snapshotUid,
					repoUid: REPO_UID,
					targetStableKey: `${REPO_UID}:src:MODULE`,
					kind: "fan_in",
					valueJson: JSON.stringify({ value: 5 }),
					source: "graph-stats:0.1.0",
					createdAt: new Date().toISOString(),
				},
			]);

			// Verify it's in the DB
			const row = storage["db"]
				.prepare("SELECT * FROM measurements WHERE target_stable_key = ?")
				.get(`${REPO_UID}:src:MODULE`) as Record<string, unknown>;
			expect(row).toBeDefined();
			expect(row.kind).toBe("fan_in");
		});
	});
});

// ── Measurement idempotency ────────────────────────────────────────────

describe("deleteMeasurementsByKind + re-insert", () => {
	it("delete-then-insert produces the same row count as a single insert", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		const now = new Date().toISOString();
		const makeMeasurement = (kind: string, key: string) => ({
			measurementUid: randomUUID(),
			snapshotUid: snap.snapshotUid,
			repoUid: REPO_UID,
			targetStableKey: key,
			kind,
			valueJson: JSON.stringify({ value: 1 }),
			source: "test",
			createdAt: now,
		});

		// Insert 3 churn measurements
		storage.insertMeasurements([
			makeMeasurement("change_frequency", `${REPO_UID}:a.ts:FILE`),
			makeMeasurement("churn_lines", `${REPO_UID}:a.ts:FILE`),
			makeMeasurement("change_frequency", `${REPO_UID}:b.ts:FILE`),
		]);

		// Verify 3 rows
		const count1 = (
			storage["db"]
				.prepare(
					"SELECT COUNT(*) as c FROM measurements WHERE snapshot_uid = ?",
				)
				.get(snap.snapshotUid) as { c: number }
		).c;
		expect(count1).toBe(3);

		// Delete churn kinds and re-insert 2 rows (simulating idempotent re-import)
		storage.deleteMeasurementsByKind(snap.snapshotUid, [
			"change_frequency",
			"churn_lines",
		]);
		storage.insertMeasurements([
			makeMeasurement("change_frequency", `${REPO_UID}:a.ts:FILE`),
			makeMeasurement("churn_lines", `${REPO_UID}:a.ts:FILE`),
		]);

		// Verify exactly 2 rows, not 5
		const count2 = (
			storage["db"]
				.prepare(
					"SELECT COUNT(*) as c FROM measurements WHERE snapshot_uid = ?",
				)
				.get(snap.snapshotUid) as { c: number }
		).c;
		expect(count2).toBe(2);
	});
});

// ── Function metric queries ────────────────────────────────────────────

describe("queryFunctionMetrics", () => {
	it("returns function metrics sorted by CC descending", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});

		const fileObj = makeFile(REPO_UID, "src/test.ts");
		storage.upsertFiles([fileObj]);

		const fnA = makeNode(snap.snapshotUid, REPO_UID, "fnA", fileObj.fileUid);
		const fnB = makeNode(snap.snapshotUid, REPO_UID, "fnB", fileObj.fileUid);
		storage.insertNodes([fnA, fnB]);

		const now = new Date().toISOString();
		storage.insertMeasurements([
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnA.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 5 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnA.stableKey,
				kind: "parameter_count",
				valueJson: JSON.stringify({ value: 2 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnA.stableKey,
				kind: "max_nesting_depth",
				valueJson: JSON.stringify({ value: 3 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnB.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 10 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnB.stableKey,
				kind: "parameter_count",
				valueJson: JSON.stringify({ value: 1 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnB.stableKey,
				kind: "max_nesting_depth",
				valueJson: JSON.stringify({ value: 4 }),
				source: "test",
				createdAt: now,
			},
		]);

		const results = storage.queryFunctionMetrics({
			snapshotUid: snap.snapshotUid,
		});

		expect(results.length).toBe(2);
		// Default sort by CC descending: fnB(10) before fnA(5)
		expect(results[0].symbol).toBe("fnB");
		expect(results[0].cyclomaticComplexity).toBe(10);
		expect(results[0].parameterCount).toBe(1);
		expect(results[0].maxNestingDepth).toBe(4);
		expect(results[1].symbol).toBe("fnA");
		expect(results[1].cyclomaticComplexity).toBe(5);
	});

	it("respects limit parameter", () => {
		// Uses same data from previous test setup (separate snapshot)
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});

		const fileObj = makeFile(REPO_UID, "src/limit-test.ts");
		storage.upsertFiles([fileObj]);

		const fn1 = makeNode(snap.snapshotUid, REPO_UID, "fn1", fileObj.fileUid);
		const fn2 = makeNode(snap.snapshotUid, REPO_UID, "fn2", fileObj.fileUid);
		const fn3 = makeNode(snap.snapshotUid, REPO_UID, "fn3", fileObj.fileUid);
		storage.insertNodes([fn1, fn2, fn3]);

		const now = new Date().toISOString();
		for (const fn of [fn1, fn2, fn3]) {
			storage.insertMeasurements([
				{
					measurementUid: randomUUID(),
					snapshotUid: snap.snapshotUid,
					repoUid: REPO_UID,
					targetStableKey: fn.stableKey,
					kind: "cyclomatic_complexity",
					valueJson: JSON.stringify({ value: 1 }),
					source: "test",
					createdAt: now,
				},
			]);
		}

		const results = storage.queryFunctionMetrics({
			snapshotUid: snap.snapshotUid,
			limit: 2,
		});
		expect(results.length).toBe(2);
	});
});

describe("queryModuleMetricAggregates", () => {
	it("aggregates function metrics per module directory", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});

		// Two files in different directories
		const fileA = makeFile(REPO_UID, "src/core/a.ts");
		const fileB = makeFile(REPO_UID, "src/cli/b.ts");
		storage.upsertFiles([fileA, fileB]);

		const fnA = makeNode(snap.snapshotUid, REPO_UID, "fnA", fileA.fileUid);
		const fnB = makeNode(snap.snapshotUid, REPO_UID, "fnB", fileB.fileUid);
		storage.insertNodes([fnA, fnB]);

		const now = new Date().toISOString();
		storage.insertMeasurements([
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnA.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 8 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnA.stableKey,
				kind: "max_nesting_depth",
				valueJson: JSON.stringify({ value: 3 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnB.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 2 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fnB.stableKey,
				kind: "max_nesting_depth",
				valueJson: JSON.stringify({ value: 1 }),
				source: "test",
				createdAt: now,
			},
		]);

		const results = storage.queryModuleMetricAggregates(snap.snapshotUid);
		expect(results.length).toBe(2);

		// Sorted by max CC desc: src/core (8) before src/cli (2)
		const core = results.find((r) => r.modulePath === "src/core");
		expect(core).toBeDefined();
		expect(core?.functionCount).toBe(1);
		expect(core?.maxCyclomaticComplexity).toBe(8);
		expect(core?.maxNestingDepth).toBe(3);

		const cli = results.find((r) => r.modulePath === "src/cli");
		expect(cli).toBeDefined();
		expect(cli?.functionCount).toBe(1);
		expect(cli?.maxCyclomaticComplexity).toBe(2);
	});
});

describe("toolchain provenance includes measurement versions", () => {
	it("snapshot toolchain_json includes ast-metrics semantics", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			toolchainJson: JSON.stringify({
				schema_compat: 1,
				extraction_semantics: 2,
				stable_key_format: 2,
				extractor_versions: { typescript: "ts-core:0.2.0" },
				indexer_version: "indexer:0.2.0",
				measurement_semantics: { "ast-metrics": 1 },
				measurement_versions: { "ast-metrics": "ast-metrics:0.1.0" },
			}),
		});

		const retrieved = storage.getSnapshot(snap.snapshotUid);
		expect(retrieved?.toolchainJson).toBeDefined();

		const toolchain = JSON.parse(retrieved?.toolchainJson ?? "{}");
		expect(toolchain.measurement_semantics["ast-metrics"]).toBe(1);
		expect(toolchain.measurement_versions["ast-metrics"]).toBe(
			"ast-metrics:0.1.0",
		);
	});
});

// ── Hotspot input query ────────────────────────────────────────────────

describe("queryHotspotInputs", () => {
	it("joins churn and complexity measurements per file", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		storage.updateSnapshotStatus({
			snapshotUid: snap.snapshotUid,
			status: SnapshotStatus.READY,
		});

		// Create a file and two functions in it
		const fileObj = makeFile(REPO_UID, "src/hot.ts");
		storage.upsertFiles([fileObj]);

		const fileNode = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"hot.ts",
			fileObj.fileUid,
			{ kind: NodeKind.FILE, subtype: null },
		);
		const fn1 = makeNode(snap.snapshotUid, REPO_UID, "fnA", fileObj.fileUid);
		const fn2 = makeNode(snap.snapshotUid, REPO_UID, "fnB", fileObj.fileUid);
		storage.insertNodes([fileNode, fn1, fn2]);

		const now = new Date().toISOString();

		// Complexity measurements: fn1 CC=5, fn2 CC=8 → sum should be 13
		storage.insertMeasurements([
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fn1.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 5 }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: fn2.stableKey,
				kind: "cyclomatic_complexity",
				valueJson: JSON.stringify({ value: 8 }),
				source: "test",
				createdAt: now,
			},
		]);

		// Churn measurements for the file
		storage.insertMeasurements([
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/hot.ts:FILE`,
				kind: "churn_lines",
				valueJson: JSON.stringify({ value: 200, since: "90.days.ago" }),
				source: "test",
				createdAt: now,
			},
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/hot.ts:FILE`,
				kind: "change_frequency",
				valueJson: JSON.stringify({ value: 10, since: "90.days.ago" }),
				source: "test",
				createdAt: now,
			},
		]);

		const inputs = storage.queryHotspotInputs(snap.snapshotUid);

		expect(inputs.length).toBe(1);
		expect(inputs[0].filePath).toBe("src/hot.ts");
		expect(inputs[0].churnLines).toBe(200);
		expect(inputs[0].changeFrequency).toBe(10);
		// sum of CC across both functions in the file
		expect(inputs[0].sumComplexity).toBe(13);

		// Verify hotspot formula: raw = churn * sum_cc
		const rawScore = inputs[0].churnLines * inputs[0].sumComplexity;
		expect(rawScore).toBe(2600);
	});

	it("excludes files with churn but no complexity", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});

		const fileObj = makeFile(REPO_UID, "src/no-cc.ts");
		storage.upsertFiles([fileObj]);

		const now = new Date().toISOString();
		// Churn only, no CC
		storage.insertMeasurements([
			{
				measurementUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/no-cc.ts:FILE`,
				kind: "churn_lines",
				valueJson: JSON.stringify({ value: 100 }),
				source: "test",
				createdAt: now,
			},
		]);

		const inputs = storage.queryHotspotInputs(snap.snapshotUid);
		// Should not include files without complexity data
		const noCC = inputs.find((i) => i.filePath === "src/no-cc.ts");
		expect(noCC).toBeUndefined();
	});
});

// ── Assessment-run markers ─────────────────────────────────────────────

describe("assessment-run markers", () => {
	it("queryInferences can distinguish marker presence from absence", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		const now = new Date().toISOString();

		// No marker yet — should be empty
		const before = storage.queryInferences(
			snap.snapshotUid,
			"assessment_run:hotspot_score",
		);
		expect(before.length).toBe(0);

		// Insert marker
		storage.insertInferences([
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:.:MODULE`,
				kind: "assessment_run:hotspot_score",
				valueJson: JSON.stringify({
					assessment: "hotspot_score",
					formula_version: 1,
					files_assessed: 0,
					computed_at: now,
				}),
				confidence: 1.0,
				basisJson: "{}",
				extractor: "test",
				createdAt: now,
			},
		]);

		// Marker present — should be found
		const after = storage.queryInferences(
			snap.snapshotUid,
			"assessment_run:hotspot_score",
		);
		expect(after.length).toBe(1);
		const val = JSON.parse(after[0].valueJson);
		expect(val.files_assessed).toBe(0);
	});

	it("deleteInferencesByKind removes markers idempotently", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		const now = new Date().toISOString();

		storage.insertInferences([
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:.:MODULE`,
				kind: "assessment_run:hotspot_score",
				valueJson: "{}",
				confidence: 1.0,
				basisJson: "{}",
				extractor: "test",
				createdAt: now,
			},
		]);

		storage.deleteInferencesByKind(
			snap.snapshotUid,
			"assessment_run:hotspot_score",
		);
		const after = storage.queryInferences(
			snap.snapshotUid,
			"assessment_run:hotspot_score",
		);
		expect(after.length).toBe(0);
	});
});

// ── Inference storage (hotspots) ───────────────────────────────────────

describe("inference storage", () => {
	it("inserts and queries inferences by kind", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		const now = new Date().toISOString();
		storage.insertInferences([
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/a.ts:FILE`,
				kind: "hotspot_score",
				valueJson: JSON.stringify({
					normalized_score: 1.0,
					raw_score: 500,
					churn_lines: 100,
					change_frequency: 5,
					sum_complexity: 5,
					formula_version: 1,
				}),
				confidence: 1.0,
				basisJson: JSON.stringify({ formula: "churn * cc" }),
				extractor: "hotspot-analyzer:0.1.0",
				createdAt: now,
			},
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/b.ts:FILE`,
				kind: "hotspot_score",
				valueJson: JSON.stringify({
					normalized_score: 0.5,
					raw_score: 250,
					churn_lines: 50,
					change_frequency: 2,
					sum_complexity: 5,
					formula_version: 1,
				}),
				confidence: 1.0,
				basisJson: JSON.stringify({ formula: "churn * cc" }),
				extractor: "hotspot-analyzer:0.1.0",
				createdAt: now,
			},
		]);

		const results = storage.queryInferences(snap.snapshotUid, "hotspot_score");
		expect(results.length).toBe(2);
		// Sorted by normalized_score desc
		const first = JSON.parse(results[0].valueJson);
		expect(first.normalized_score).toBe(1.0);
		expect(first.raw_score).toBe(500);
	});

	it("deleteInferencesByKind is idempotent for recompute", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
		});
		const now = new Date().toISOString();

		// Insert 2 inferences
		storage.insertInferences([
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/a.ts:FILE`,
				kind: "hotspot_score",
				valueJson: "{}",
				confidence: 1.0,
				basisJson: "{}",
				extractor: "test",
				createdAt: now,
			},
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/b.ts:FILE`,
				kind: "hotspot_score",
				valueJson: "{}",
				confidence: 1.0,
				basisJson: "{}",
				extractor: "test",
				createdAt: now,
			},
		]);

		expect(
			storage.queryInferences(snap.snapshotUid, "hotspot_score").length,
		).toBe(2);

		// Delete and re-insert 1
		storage.deleteInferencesByKind(snap.snapshotUid, "hotspot_score");
		storage.insertInferences([
			{
				inferenceUid: randomUUID(),
				snapshotUid: snap.snapshotUid,
				repoUid: REPO_UID,
				targetStableKey: `${REPO_UID}:src/a.ts:FILE`,
				kind: "hotspot_score",
				valueJson: "{}",
				confidence: 1.0,
				basisJson: "{}",
				extractor: "test",
				createdAt: now,
			},
		]);

		// Should be 1, not 3
		expect(
			storage.queryInferences(snap.snapshotUid, "hotspot_score").length,
		).toBe(1);
	});
});

// ── Schema migration upgrade path ─────────────────────────────────────

describe("schema migration from v1 baseline", () => {
	let upgradeDbPath: string;

	afterEach(() => {
		try {
			unlinkSync(upgradeDbPath);
		} catch {
			// ignore
		}
	});

	/**
	 * Create a database with only the v1 schema (no toolchain_json,
	 * no authored_basis_json, no measurements table). Then open it
	 * with SqliteStorage.initialize() and verify all migrations ran.
	 */
	it("upgrades a v1 database to current schema", async () => {
		// Dynamic import to get raw Database constructor
		const Database = (await import("better-sqlite3")).default;

		upgradeDbPath = join(tmpdir(), `rgr-upgrade-${randomUUID()}.db`);
		const rawDb = new Database(upgradeDbPath);

		// Create v1 schema WITHOUT the new columns
		rawDb.exec(`
			PRAGMA journal_mode = WAL;
			PRAGMA foreign_keys = ON;

			CREATE TABLE repos (
				repo_uid TEXT PRIMARY KEY, name TEXT NOT NULL,
				root_path TEXT NOT NULL, default_branch TEXT,
				created_at TEXT NOT NULL, metadata_json TEXT
			);
			CREATE TABLE snapshots (
				snapshot_uid TEXT PRIMARY KEY,
				repo_uid TEXT NOT NULL REFERENCES repos(repo_uid),
				parent_snapshot_uid TEXT, kind TEXT NOT NULL,
				basis_ref TEXT, basis_commit TEXT, dirty_hash TEXT,
				status TEXT NOT NULL,
				files_total INTEGER DEFAULT 0, nodes_total INTEGER DEFAULT 0,
				edges_total INTEGER DEFAULT 0,
				created_at TEXT NOT NULL, completed_at TEXT, label TEXT
			);
			CREATE TABLE files (
				file_uid TEXT PRIMARY KEY, repo_uid TEXT NOT NULL,
				path TEXT NOT NULL, language TEXT,
				is_test INTEGER DEFAULT 0, is_generated INTEGER DEFAULT 0,
				is_excluded INTEGER DEFAULT 0
			);
			CREATE TABLE file_versions (
				snapshot_uid TEXT NOT NULL, file_uid TEXT NOT NULL,
				content_hash TEXT NOT NULL, ast_hash TEXT, extractor TEXT,
				parse_status TEXT NOT NULL, size_bytes INTEGER,
				line_count INTEGER, indexed_at TEXT NOT NULL,
				PRIMARY KEY (snapshot_uid, file_uid)
			);
			CREATE TABLE nodes (
				node_uid TEXT PRIMARY KEY, snapshot_uid TEXT NOT NULL,
				repo_uid TEXT NOT NULL, stable_key TEXT NOT NULL,
				kind TEXT NOT NULL, subtype TEXT, name TEXT NOT NULL,
				qualified_name TEXT, file_uid TEXT, parent_node_uid TEXT,
				line_start INTEGER, col_start INTEGER,
				line_end INTEGER, col_end INTEGER,
				signature TEXT, visibility TEXT, doc_comment TEXT,
				metadata_json TEXT
			);
			CREATE TABLE edges (
				edge_uid TEXT PRIMARY KEY, snapshot_uid TEXT NOT NULL,
				repo_uid TEXT NOT NULL, source_node_uid TEXT NOT NULL,
				target_node_uid TEXT NOT NULL, type TEXT NOT NULL,
				resolution TEXT NOT NULL, extractor TEXT NOT NULL,
				line_start INTEGER, col_start INTEGER,
				line_end INTEGER, col_end INTEGER, metadata_json TEXT
			);
			CREATE TABLE declarations (
				declaration_uid TEXT PRIMARY KEY, repo_uid TEXT NOT NULL,
				snapshot_uid TEXT, target_stable_key TEXT NOT NULL,
				kind TEXT NOT NULL, value_json TEXT NOT NULL,
				created_at TEXT NOT NULL, created_by TEXT,
				supersedes_uid TEXT, is_active INTEGER DEFAULT 1
			);
			CREATE TABLE inferences (
				inference_uid TEXT PRIMARY KEY, snapshot_uid TEXT NOT NULL,
				repo_uid TEXT NOT NULL, target_stable_key TEXT NOT NULL,
				kind TEXT NOT NULL, value_json TEXT NOT NULL,
				confidence REAL NOT NULL, basis_json TEXT NOT NULL,
				extractor TEXT NOT NULL, created_at TEXT NOT NULL
			);
			CREATE TABLE artifacts (
				artifact_uid TEXT PRIMARY KEY, snapshot_uid TEXT NOT NULL,
				repo_uid TEXT NOT NULL, kind TEXT NOT NULL,
				relative_path TEXT NOT NULL, content_hash TEXT,
				size_bytes INTEGER, format TEXT, created_at TEXT NOT NULL
			);
			CREATE TABLE evidence_links (
				evidence_link_uid TEXT PRIMARY KEY, snapshot_uid TEXT NOT NULL,
				subject_type TEXT NOT NULL, subject_uid TEXT NOT NULL,
				artifact_uid TEXT NOT NULL, note TEXT
			);
			CREATE TABLE schema_migrations (
				version INTEGER PRIMARY KEY, name TEXT NOT NULL,
				applied_at TEXT NOT NULL
			);
			INSERT INTO schema_migrations VALUES (1, '001-initial', datetime('now'));
		`);
		rawDb.close();

		// Open with SqliteStorage — this should run migrations 002 and 003
		const upgraded = new SqliteStorage(upgradeDbPath);
		upgraded.initialize();

		// Verify migration 002: toolchain_json on snapshots
		const rawDb2 = new Database(upgradeDbPath);
		const snapCols = (
			rawDb2.prepare("PRAGMA table_info(snapshots)").all() as Array<{
				name: string;
			}>
		).map((c) => c.name);
		expect(snapCols).toContain("toolchain_json");

		// Verify migration 002: authored_basis_json on declarations
		const declCols = (
			rawDb2.prepare("PRAGMA table_info(declarations)").all() as Array<{
				name: string;
			}>
		).map((c) => c.name);
		expect(declCols).toContain("authored_basis_json");

		// Verify migration 003: measurements table exists
		const tables = (
			rawDb2
				.prepare(
					"SELECT name FROM sqlite_master WHERE type='table' AND name='measurements'",
				)
				.all() as Array<{ name: string }>
		).map((t) => t.name);
		expect(tables).toContain("measurements");

		// Verify all migrations recorded
		const migrations = rawDb2
			.prepare("SELECT version FROM schema_migrations ORDER BY version")
			.all() as Array<{ version: number }>;
		expect(migrations.map((m) => m.version)).toEqual([1, 2, 3]);

		rawDb2.close();
		upgraded.close();
	});
});
