/**
 * D2c: Safe-subset edge promotion from enriched unresolved to resolved.
 *
 * Converts a narrow subset of compiler-enriched unresolved CALLS edges
 * into resolved graph edges. ONLY promotes when ALL 8 safety criteria
 * are met simultaneously:
 *
 *   1. category = calls_obj_method_needs_type_info
 *   2. enrichment exists with origin = "compiler"
 *   3. enriched type is internal (isExternalType = false)
 *   4. type maps to exactly ONE internal CLASS symbol in the graph
 *   5. method maps to exactly ONE METHOD on that class
 *   6. no union/intersection (type display name has no "|" or "&")
 *   7. targetKey is simple "receiver.method" (no chaining, no optional)
 *   8. enrichment origin is "compiler" (not "failed")
 *
 * Promoted edges:
 *   - inserted into the `edges` table with resolution="inferred"
 *   - extractor field: "compiler-promotion:<version>"
 *   - preserve original source location from the unresolved edge
 *
 * This module does NOT delete or modify the original unresolved edge.
 * Both the unresolved observation and the resolved edge coexist.
 * The unresolved row retains its classification and enrichment for
 * auditability.
 */

import type {
	GraphEdge,
	GraphNode,
} from "../../core/model/index.js";
import { EdgeType, Resolution } from "../../core/model/index.js";

const PROMOTER_VERSION = "compiler-promotion:0.1.0";

export interface PromotionCandidate {
	edgeUid: string;
	snapshotUid: string;
	repoUid: string;
	sourceNodeUid: string;
	targetKey: string;
	lineStart: number | null;
	colStart: number | null;
	lineEnd: number | null;
	colEnd: number | null;
	category: string;
	enrichment: {
		receiverType: string | null;
		typeDisplayName: string | null;
		isExternalType: boolean;
		origin: string;
	};
}

export interface PromotionResult {
	promoted: GraphEdge[];
	skippedReasons: Record<string, number>;
}

/**
 * Evaluate a batch of enriched unresolved edges and promote the
 * safe subset to resolved graph edges.
 *
 * @param candidates - enriched unresolved edges to evaluate
 * @param nodesByName - map of symbol name → GraphNode[] from the snapshot
 * @param classMethodIndex - map of "ClassName" → Set<"methodName"> from the snapshot
 */
export function promoteEdges(
	candidates: PromotionCandidate[],
	nodesByName: Map<string, GraphNode[]>,
	classMethodIndex: Map<string, Map<string, GraphNode>>,
): PromotionResult {
	const promoted: GraphEdge[] = [];
	const skippedReasons: Record<string, number> = {};

	function skip(reason: string): void {
		skippedReasons[reason] = (skippedReasons[reason] ?? 0) + 1;
	}

	for (const c of candidates) {
		// Gate 1: category (obj.method OR this.field.method)
		if (
			c.category !== "calls_obj_method_needs_type_info" &&
			c.category !== "calls_this_wildcard_method_needs_type_info"
		) {
			skip("wrong_category");
			continue;
		}

		// Gate 2+8: enrichment exists and is compiler-derived
		if (!c.enrichment || c.enrichment.origin !== "compiler") {
			skip("no_compiler_enrichment");
			continue;
		}

		// Gate 3: type is internal
		if (c.enrichment.isExternalType) {
			skip("external_type");
			continue;
		}

		const typeName = c.enrichment.typeDisplayName ?? c.enrichment.receiverType;
		if (!typeName) {
			skip("no_type_name");
			continue;
		}

		// Gate 6: no union/intersection
		if (typeName.includes("|") || typeName.includes("&")) {
			skip("union_or_intersection");
			continue;
		}

		// Gate 7: receiver.method OR this.field.method shape.
		// Allows exactly 2 parts (obj.method) or exactly 3 parts where
		// part[0] is "this" (this.field.method). Rejects deeper chains,
		// optional chaining, and element access.
		const parts = c.targetKey.split(".");
		let methodName: string;
		if (parts.length === 2) {
			// Simple: obj.method
			methodName = parts[1];
		} else if (parts.length === 3 && parts[0] === "this") {
			// this.field.method — the enrichment resolved the type of
			// "this.field" (the intermediate receiver).
			methodName = parts[2];
		} else {
			skip("not_simple_receiver_method");
			continue;
		}
		if (!methodName || methodName.includes("?") || methodName.includes("[")) {
			skip("optional_or_element_access");
			continue;
		}

		// Gate 4: type maps to exactly one CLASS symbol in the graph
		const classNodes = nodesByName.get(typeName);
		if (!classNodes) {
			skip("type_not_in_graph");
			continue;
		}
		const classOnly = classNodes.filter((n) => n.subtype === "CLASS");
		if (classOnly.length === 0) {
			skip("type_not_a_class");
			continue;
		}
		if (classOnly.length > 1) {
			skip("ambiguous_class_multiple_definitions");
			continue;
		}

		// Gate 5: method maps to exactly one METHOD on that class
		const classMethods = classMethodIndex.get(classOnly[0].stableKey);
		if (!classMethods) {
			skip("class_has_no_methods");
			continue;
		}
		const methodNode = classMethods.get(methodName);
		if (!methodNode) {
			skip("method_not_found_on_class");
			continue;
		}

		// All 8 gates passed. Promote.
		promoted.push({
			edgeUid: `promoted:${c.edgeUid}`,
			snapshotUid: c.snapshotUid,
			repoUid: c.repoUid,
			sourceNodeUid: c.sourceNodeUid,
			targetNodeUid: methodNode.nodeUid,
			type: EdgeType.CALLS,
			resolution: Resolution.INFERRED,
			extractor: PROMOTER_VERSION,
			location: c.lineStart !== null
				? {
						lineStart: c.lineStart,
						colStart: c.colStart ?? 0,
						lineEnd: c.lineEnd ?? c.lineStart,
						colEnd: c.colEnd ?? 0,
					}
				: null,
			metadataJson: JSON.stringify({
				promotedFrom: c.edgeUid,
				receiverType: typeName,
				methodName,
			}),
		});
	}

	return { promoted, skippedReasons };
}
