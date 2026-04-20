/**
 * Integration tests for module dependency query adapter.
 *
 * Tests the full adapter pipeline:
 *   1. queryResolvedImportsWithFiles loads data from storage
 *   2. getModuleDependencyGraph transforms and calls pure derivation
 *   3. Enrichment attaches module identity to edges
 *
 * See docs/architecture/module-graph-contract.txt for the specification.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
	getModuleDependencyGraph,
	type ModuleDependencyStorageQueries,
} from "../../../src/adapters/query/module-dependency-query.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import type {
	GraphEdge,
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
import type {
	ModuleCandidate,
	ModuleFileOwnership,
} from "../../../src/core/modules/module-candidate.js";

// ── Test setup ─────────────────────────────────────────────────────

let storage: SqliteStorage & ModuleDependencyStorageQueries;
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
	fileUid: string | null,
	overrides?: Partial<GraphNode>,
): GraphNode {
	const subtype = overrides?.subtype ?? NodeSubtype.FUNCTION;
	const subtypeSuffix = subtype ? `:${subtype}` : "";
	const filePath = fileUid?.split(":")[1] ?? "unknown";
	const stableKey = `${repoUid}:${filePath}#${name}:SYMBOL${subtypeSuffix}`;
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
	type: EdgeType = EdgeType.IMPORTS,
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

function makeModuleCandidate(
	snapshotUid: string,
	repoUid: string,
	rootPath: string,
	displayName: string | null = null,
): ModuleCandidate {
	const uid = randomUUID();
	return {
		moduleCandidateUid: uid,
		snapshotUid,
		repoUid,
		moduleKey: `${repoUid}:${rootPath}:DISCOVERED_MODULE`,
		moduleKind: "declared",
		canonicalRootPath: rootPath,
		confidence: 1.0,
		displayName,
		metadataJson: null,
	};
}

function makeOwnership(
	snapshotUid: string,
	repoUid: string,
	fileUid: string,
	moduleCandidateUid: string,
): ModuleFileOwnership {
	return {
		snapshotUid,
		repoUid,
		fileUid,
		moduleCandidateUid,
		assignmentKind: "root_containment",
		confidence: 1.0,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase()) as SqliteStorage &
		ModuleDependencyStorageQueries;
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

// ── queryResolvedImportsWithFiles ──────────────────────────────────

describe("queryResolvedImportsWithFiles", () => {
	it("returns source and target file UIDs for resolved IMPORTS edges", () => {
		const snap = makeSnapshot();
		const fileA = makeFile(REPO_UID, "src/a.ts");
		const fileB = makeFile(REPO_UID, "src/b.ts");
		storage.upsertFiles([fileA, fileB]);

		const nodeA = makeNode(snap.snapshotUid, REPO_UID, "funcA", fileA.fileUid);
		const nodeB = makeNode(snap.snapshotUid, REPO_UID, "funcB", fileB.fileUid);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = storage.queryResolvedImportsWithFiles(snap.snapshotUid);

		expect(result).toHaveLength(1);
		expect(result[0].sourceFileUid).toBe(fileA.fileUid);
		expect(result[0].targetFileUid).toBe(fileB.fileUid);
	});

	it("excludes edges where source node has no file_uid", () => {
		const snap = makeSnapshot();
		const fileB = makeFile(REPO_UID, "src/b.ts");
		storage.upsertFiles([fileB]);

		// Source node has no file association.
		const nodeA = makeNode(snap.snapshotUid, REPO_UID, "funcA", null);
		const nodeB = makeNode(snap.snapshotUid, REPO_UID, "funcB", fileB.fileUid);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = storage.queryResolvedImportsWithFiles(snap.snapshotUid);
		expect(result).toHaveLength(0);
	});

	it("excludes edges where target node has no file_uid", () => {
		const snap = makeSnapshot();
		const fileA = makeFile(REPO_UID, "src/a.ts");
		storage.upsertFiles([fileA]);

		const nodeA = makeNode(snap.snapshotUid, REPO_UID, "funcA", fileA.fileUid);
		// Target node has no file association.
		const nodeB = makeNode(snap.snapshotUid, REPO_UID, "funcB", null);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = storage.queryResolvedImportsWithFiles(snap.snapshotUid);
		expect(result).toHaveLength(0);
	});

	it("only includes IMPORTS edges, not CALLS or other types", () => {
		const snap = makeSnapshot();
		const fileA = makeFile(REPO_UID, "src/a.ts");
		const fileB = makeFile(REPO_UID, "src/b.ts");
		storage.upsertFiles([fileA, fileB]);

		const nodeA = makeNode(snap.snapshotUid, REPO_UID, "funcA", fileA.fileUid);
		const nodeB = makeNode(snap.snapshotUid, REPO_UID, "funcB", fileB.fileUid);
		storage.insertNodes([nodeA, nodeB]);

		const importsEdge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		const callsEdge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.CALLS,
		);
		storage.insertEdges([importsEdge, callsEdge]);

		const result = storage.queryResolvedImportsWithFiles(snap.snapshotUid);
		expect(result).toHaveLength(1);
	});
});

// ── getModuleDependencyGraph ───────────────────────────────────────

describe("getModuleDependencyGraph — basic derivation", () => {
	it("derives edge when source and target files are in different modules", () => {
		const snap = makeSnapshot();
		// Set up two modules.
		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"core",
		);
		storage.insertModuleCandidates([moduleApi, moduleCore]);

		// Set up files and ownership.
		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB = makeFile(REPO_UID, "src/core/service.ts");
		storage.upsertFiles([fileA, fileB]);

		const ownershipA = makeOwnership(
			snap.snapshotUid,
			REPO_UID,
			fileA.fileUid,
			moduleApi.moduleCandidateUid,
		);
		const ownershipB = makeOwnership(
			snap.snapshotUid,
			REPO_UID,
			fileB.fileUid,
			moduleCore.moduleCandidateUid,
		);
		storage.insertModuleFileOwnership([ownershipA, ownershipB]);

		// Set up nodes and IMPORTS edge.
		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		// Run query.
		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].sourceModuleKey).toBe(moduleApi.moduleKey);
		expect(result.edges[0].targetModuleKey).toBe(moduleCore.moduleKey);
		expect(result.edges[0].importCount).toBe(1);
		expect(result.edges[0].sourceFileCount).toBe(1);
		expect(result.diagnostics.importsCrossModule).toBe(1);
	});

	it("excludes intra-module imports", () => {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		storage.insertModuleCandidates([moduleApi]);

		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB = makeFile(REPO_UID, "src/api/util.ts");
		storage.upsertFiles([fileA, fileB]);

		// Both files owned by the same module.
		const ownershipA = makeOwnership(
			snap.snapshotUid,
			REPO_UID,
			fileA.fileUid,
			moduleApi.moduleCandidateUid,
		);
		const ownershipB = makeOwnership(
			snap.snapshotUid,
			REPO_UID,
			fileB.fileUid,
			moduleApi.moduleCandidateUid,
		);
		storage.insertModuleFileOwnership([ownershipA, ownershipB]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"formatData",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsIntraModule).toBe(1);
	});
});

// ── Aggregation ────────────────────────────────────────────────────

describe("getModuleDependencyGraph — aggregation", () => {
	it("aggregates multiple imports into importCount", () => {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"core",
		);
		storage.insertModuleCandidates([moduleApi, moduleCore]);

		// One source file, two target files.
		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB1 = makeFile(REPO_UID, "src/core/service1.ts");
		const fileB2 = makeFile(REPO_UID, "src/core/service2.ts");
		storage.upsertFiles([fileA, fileB1, fileB2]);

		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA.fileUid,
				moduleApi.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB1.fileUid,
				moduleCore.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB2.fileUid,
				moduleCore.moduleCandidateUid,
			),
		]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB1 = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService1",
			fileB1.fileUid,
		);
		const nodeB2 = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService2",
			fileB2.fileUid,
		);
		storage.insertNodes([nodeA, nodeB1, nodeB2]);

		// Two IMPORTS edges from same source file to different targets in same module.
		const edge1 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB1.nodeUid,
			EdgeType.IMPORTS,
		);
		const edge2 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB2.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge1, edge2]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].importCount).toBe(2);
		expect(result.edges[0].sourceFileCount).toBe(1); // Same source file.
	});

	it("aggregates multiple source files into sourceFileCount", () => {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"core",
		);
		storage.insertModuleCandidates([moduleApi, moduleCore]);

		// Two source files, one target file.
		const fileA1 = makeFile(REPO_UID, "src/api/handler1.ts");
		const fileA2 = makeFile(REPO_UID, "src/api/handler2.ts");
		const fileB = makeFile(REPO_UID, "src/core/service.ts");
		storage.upsertFiles([fileA1, fileA2, fileB]);

		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA1.fileUid,
				moduleApi.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA2.fileUid,
				moduleApi.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB.fileUid,
				moduleCore.moduleCandidateUid,
			),
		]);

		const nodeA1 = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest1",
			fileA1.fileUid,
		);
		const nodeA2 = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest2",
			fileA2.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA1, nodeA2, nodeB]);

		// Two IMPORTS edges from different source files to same target.
		const edge1 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA1.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		const edge2 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA2.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge1, edge2]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].importCount).toBe(2);
		expect(result.edges[0].sourceFileCount).toBe(2); // Two different source files.
	});
});

// ── Missing data handling ──────────────────────────────────────────

describe("getModuleDependencyGraph — missing data", () => {
	it("excludes imports from files without module ownership", () => {
		const snap = makeSnapshot();

		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"core",
		);
		storage.insertModuleCandidates([moduleCore]);

		// fileA has no ownership, fileB is owned.
		const fileA = makeFile(REPO_UID, "src/orphan/handler.ts");
		const fileB = makeFile(REPO_UID, "src/core/service.ts");
		storage.upsertFiles([fileA, fileB]);

		// Only fileB has ownership.
		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB.fileUid,
				moduleCore.moduleCandidateUid,
			),
		]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"orphanFunc",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsSourceNoModule).toBe(1);
	});

	it("excludes imports to files without module ownership", () => {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		storage.insertModuleCandidates([moduleApi]);

		// fileA is owned, fileB has no ownership.
		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB = makeFile(REPO_UID, "src/orphan/service.ts");
		storage.upsertFiles([fileA, fileB]);

		// Only fileA has ownership.
		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA.fileUid,
				moduleApi.moduleCandidateUid,
			),
		]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"orphanService",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(0);
		expect(result.diagnostics.importsTargetNoModule).toBe(1);
	});
});

// ── Filter options ─────────────────────────────────────────────────

describe("getModuleDependencyGraph — filter", () => {
	function setupThreeModuleGraph() {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"api",
		);
		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"core",
		);
		const moduleUtils = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/utils",
			"utils",
		);
		storage.insertModuleCandidates([moduleApi, moduleCore, moduleUtils]);

		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB = makeFile(REPO_UID, "src/core/service.ts");
		const fileC = makeFile(REPO_UID, "src/utils/helpers.ts");
		storage.upsertFiles([fileA, fileB, fileC]);

		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA.fileUid,
				moduleApi.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB.fileUid,
				moduleCore.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileC.fileUid,
				moduleUtils.moduleCandidateUid,
			),
		]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService",
			fileB.fileUid,
		);
		const nodeC = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"formatData",
			fileC.fileUid,
		);
		storage.insertNodes([nodeA, nodeB, nodeC]);

		// api -> core, core -> utils
		const edge1 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		const edge2 = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeB.nodeUid,
			nodeC.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge1, edge2]);

		return { snap, moduleApi, moduleCore, moduleUtils };
	}

	it("filters to edges involving a specific module", () => {
		const { snap, moduleCore } = setupThreeModuleGraph();

		const result = getModuleDependencyGraph(storage, snap.snapshotUid, {
			moduleKey: moduleCore.moduleKey,
		});

		// core is involved in both edges (as target of api->core and source of core->utils).
		expect(result.edges).toHaveLength(2);
	});

	it("filters to outbound edges only", () => {
		const { snap, moduleCore, moduleUtils } = setupThreeModuleGraph();

		const result = getModuleDependencyGraph(storage, snap.snapshotUid, {
			moduleKey: moduleCore.moduleKey,
			outboundOnly: true,
		});

		// Only core -> utils.
		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].sourceModuleKey).toBe(moduleCore.moduleKey);
		expect(result.edges[0].targetModuleKey).toBe(moduleUtils.moduleKey);
	});

	it("filters to inbound edges only", () => {
		const { snap, moduleApi, moduleCore } = setupThreeModuleGraph();

		const result = getModuleDependencyGraph(storage, snap.snapshotUid, {
			moduleKey: moduleCore.moduleKey,
			inboundOnly: true,
		});

		// Only api -> core.
		expect(result.edges).toHaveLength(1);
		expect(result.edges[0].sourceModuleKey).toBe(moduleApi.moduleKey);
		expect(result.edges[0].targetModuleKey).toBe(moduleCore.moduleKey);
	});

	it("throws when outboundOnly is set without moduleKey", () => {
		const { snap } = setupThreeModuleGraph();

		expect(() =>
			getModuleDependencyGraph(storage, snap.snapshotUid, {
				outboundOnly: true,
			}),
		).toThrow("outboundOnly and inboundOnly filters require moduleKey");
	});

	it("throws when inboundOnly is set without moduleKey", () => {
		const { snap } = setupThreeModuleGraph();

		expect(() =>
			getModuleDependencyGraph(storage, snap.snapshotUid, {
				inboundOnly: true,
			}),
		).toThrow("outboundOnly and inboundOnly filters require moduleKey");
	});

	it("throws when both outboundOnly and inboundOnly are set", () => {
		const { snap, moduleCore } = setupThreeModuleGraph();

		expect(() =>
			getModuleDependencyGraph(storage, snap.snapshotUid, {
				moduleKey: moduleCore.moduleKey,
				outboundOnly: true,
				inboundOnly: true,
			}),
		).toThrow("outboundOnly and inboundOnly are mutually exclusive");
	});
});

// ── Enrichment ─────────────────────────────────────────────────────

describe("getModuleDependencyGraph — enrichment", () => {
	it("includes module identity fields in enriched edges", () => {
		const snap = makeSnapshot();

		const moduleApi = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/api",
			"@myorg/api",
		);
		const moduleCore = makeModuleCandidate(
			snap.snapshotUid,
			REPO_UID,
			"src/core",
			"@myorg/core",
		);
		storage.insertModuleCandidates([moduleApi, moduleCore]);

		const fileA = makeFile(REPO_UID, "src/api/handler.ts");
		const fileB = makeFile(REPO_UID, "src/core/service.ts");
		storage.upsertFiles([fileA, fileB]);

		storage.insertModuleFileOwnership([
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileA.fileUid,
				moduleApi.moduleCandidateUid,
			),
			makeOwnership(
				snap.snapshotUid,
				REPO_UID,
				fileB.fileUid,
				moduleCore.moduleCandidateUid,
			),
		]);

		const nodeA = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"handleRequest",
			fileA.fileUid,
		);
		const nodeB = makeNode(
			snap.snapshotUid,
			REPO_UID,
			"getService",
			fileB.fileUid,
		);
		storage.insertNodes([nodeA, nodeB]);

		const edge = makeEdge(
			snap.snapshotUid,
			REPO_UID,
			nodeA.nodeUid,
			nodeB.nodeUid,
			EdgeType.IMPORTS,
		);
		storage.insertEdges([edge]);

		const result = getModuleDependencyGraph(storage, snap.snapshotUid);

		expect(result.edges).toHaveLength(1);
		const enrichedEdge = result.edges[0];

		// Source module identity.
		expect(enrichedEdge.sourceModuleUid).toBe(moduleApi.moduleCandidateUid);
		expect(enrichedEdge.sourceModuleKey).toBe(moduleApi.moduleKey);
		expect(enrichedEdge.sourceRootPath).toBe("src/api");
		expect(enrichedEdge.sourceModuleKind).toBe("declared");
		expect(enrichedEdge.sourceDisplayName).toBe("@myorg/api");

		// Target module identity.
		expect(enrichedEdge.targetModuleUid).toBe(moduleCore.moduleCandidateUid);
		expect(enrichedEdge.targetModuleKey).toBe(moduleCore.moduleKey);
		expect(enrichedEdge.targetRootPath).toBe("src/core");
		expect(enrichedEdge.targetModuleKind).toBe("declared");
		expect(enrichedEdge.targetDisplayName).toBe("@myorg/core");
	});
});
