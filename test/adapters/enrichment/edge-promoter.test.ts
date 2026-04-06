/**
 * Edge promoter — unit tests for the 8-gate safe-subset promotion.
 */

import { describe, expect, it } from "vitest";
import {
	promoteEdges,
	type PromotionCandidate,
} from "../../../src/adapters/enrichment/edge-promoter.js";
import type { GraphNode } from "../../../src/core/model/index.js";
import { NodeKind, NodeSubtype, Visibility } from "../../../src/core/model/index.js";

function makeNode(
	name: string,
	qualifiedName: string,
	subtype: string = NodeSubtype.CLASS,
): GraphNode {
	return {
		nodeUid: `uid-${name}`,
		snapshotUid: "snap",
		repoUid: "repo",
		stableKey: `repo:src/${name}.ts#${qualifiedName}:SYMBOL:${subtype}`,
		kind: NodeKind.SYMBOL,
		subtype: subtype as any,
		name,
		qualifiedName,
		fileUid: `repo:src/${name}.ts`,
		parentNodeUid: null,
		location: null,
		signature: null,
		visibility: Visibility.EXPORT,
		docComment: null,
		metadataJson: null,
	};
}

function makeCandidate(
	targetKey: string,
	typeName: string,
	category = "calls_obj_method_needs_type_info",
): PromotionCandidate {
	return {
		edgeUid: `edge-${targetKey}`,
		snapshotUid: "snap",
		repoUid: "repo",
		sourceNodeUid: "source-node",
		targetKey,
		lineStart: 10,
		colStart: 0,
		lineEnd: null,
		colEnd: null,
		category,
		enrichment: {
			receiverType: typeName,
			typeDisplayName: typeName,
			isExternalType: false,
			origin: "compiler",
		},
	};
}

function buildIndexes(
	classNodes: GraphNode[],
	methods: Array<{ className: string; methodName: string; node: GraphNode }>,
) {
	const nodesByName = new Map<string, GraphNode[]>();
	for (const n of classNodes) {
		const list = nodesByName.get(n.name) ?? [];
		list.push(n);
		nodesByName.set(n.name, list);
	}
	const classMethodIndex = new Map<string, Map<string, GraphNode>>();
	for (const m of methods) {
		const classNode = classNodes.find((n) => n.name === m.className);
		if (!classNode) continue;
		const map = classMethodIndex.get(classNode.stableKey) ?? new Map();
		map.set(m.methodName, m.node);
		classMethodIndex.set(classNode.stableKey, map);
	}
	return { nodesByName, classMethodIndex };
}

describe("promoteEdges — gate enforcement", () => {
	const classNode = makeNode("TreeStore", "TreeStore", "CLASS");
	const methodNode = makeNode("getNode", "TreeStore.getNode", "METHOD");
	const indexes = buildIndexes([classNode], [
		{ className: "TreeStore", methodName: "getNode", node: methodNode },
	]);

	it("promotes a valid obj.method candidate", () => {
		const result = promoteEdges(
			[makeCandidate("store.getNode", "TreeStore")],
			indexes.nodesByName,
			indexes.classMethodIndex,
		);
		expect(result.promoted.length).toBe(1);
		expect(result.promoted[0].targetNodeUid).toBe(methodNode.nodeUid);
		expect(result.promoted[0].resolution).toBe("inferred");
	});

	it("promotes a valid this.field.method candidate", () => {
		const result = promoteEdges(
			[makeCandidate("this.store.getNode", "TreeStore", "calls_this_wildcard_method_needs_type_info")],
			indexes.nodesByName,
			indexes.classMethodIndex,
		);
		expect(result.promoted.length).toBe(1);
	});

	it("skips wrong category", () => {
		const result = promoteEdges(
			[makeCandidate("x", "TreeStore", "calls_function_ambiguous_or_missing")],
			indexes.nodesByName,
			indexes.classMethodIndex,
		);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.wrong_category).toBe(1);
	});

	it("skips external types", () => {
		const c = makeCandidate("store.getNode", "TreeStore");
		c.enrichment.isExternalType = true;
		const result = promoteEdges([c], indexes.nodesByName, indexes.classMethodIndex);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.external_type).toBe(1);
	});

	it("skips union types", () => {
		const c = makeCandidate("store.getNode", "TreeStore | null");
		const result = promoteEdges([c], indexes.nodesByName, indexes.classMethodIndex);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.union_or_intersection).toBe(1);
	});

	it("skips deep chains (a.b.c.method)", () => {
		const c = makeCandidate("a.b.c.method", "TreeStore");
		const result = promoteEdges([c], indexes.nodesByName, indexes.classMethodIndex);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.not_simple_receiver_method).toBe(1);
	});

	it("skips when type is not a CLASS", () => {
		const interfaceNode = makeNode("IStore", "IStore", "INTERFACE");
		const idx = buildIndexes([interfaceNode], []);
		const result = promoteEdges(
			[makeCandidate("store.get", "IStore")],
			idx.nodesByName,
			idx.classMethodIndex,
		);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.type_not_a_class).toBe(1);
	});

	it("skips when method not found on class", () => {
		const result = promoteEdges(
			[makeCandidate("store.missingMethod", "TreeStore")],
			indexes.nodesByName,
			indexes.classMethodIndex,
		);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.method_not_found_on_class).toBe(1);
	});

	it("skips when class has multiple definitions (ambiguous)", () => {
		const dup1 = makeNode("TreeStore", "TreeStore", "CLASS");
		const dup2 = { ...makeNode("TreeStore", "TreeStore", "CLASS"), nodeUid: "uid-TreeStore-2" };
		const idx = buildIndexes([dup1, dup2], []);
		const result = promoteEdges(
			[makeCandidate("store.getNode", "TreeStore")],
			idx.nodesByName,
			idx.classMethodIndex,
		);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.ambiguous_class_multiple_definitions).toBe(1);
	});

	it("skips when enrichment origin is 'failed'", () => {
		const c = makeCandidate("store.getNode", "TreeStore");
		c.enrichment.origin = "failed";
		const result = promoteEdges([c], indexes.nodesByName, indexes.classMethodIndex);
		expect(result.promoted.length).toBe(0);
		expect(result.skippedReasons.no_compiler_enrichment).toBe(1);
	});
});
