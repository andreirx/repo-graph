/**
 * JSON output formatter.
 *
 * This is the serialization boundary between internal camelCase types
 * and the external snake_case wire format documented in v1-cli.txt.
 *
 * Every --json response passes through this formatter before reaching stdout.
 */

import type {
	CycleResult,
	DeadNodeResult,
	NodeResult,
	PathResult,
	QueryResult,
} from "../../core/model/index.js";

// ── Generic query result wrapper ───────────────────────────────────────

export function formatQueryResult<T>(
	result: QueryResult<T>,
	formatItem: (item: T) => Record<string, unknown>,
): string {
	const output = {
		command: result.command,
		repo: result.repo,
		snapshot: result.snapshot,
		snapshot_scope: result.snapshotScope,
		basis_commit: result.basisCommit,
		results: result.results.map(formatItem),
		count: result.count,
		stale: result.stale,
	};
	return JSON.stringify(output, null, 2);
}

// ── Per-type formatters ────────────────────────────────────────────────

export function formatNodeResult(r: NodeResult): Record<string, unknown> {
	return {
		node_id: r.nodeUid,
		symbol: r.symbol,
		kind: r.kind,
		subtype: r.subtype,
		file: r.file,
		line: r.line,
		column: r.column,
		edge_type: r.edgeType,
		resolution: r.resolution,
		evidence: r.evidence,
		depth: r.depth,
	};
}

export function formatDeadNodeResult(
	r: DeadNodeResult,
): Record<string, unknown> {
	return {
		node_id: r.nodeUid,
		symbol: r.symbol,
		kind: r.kind,
		subtype: r.subtype,
		file: r.file,
		line: r.line,
		lines_of_code: r.lineCount,
	};
}

export function formatPathResult(r: PathResult): Record<string, unknown> {
	return {
		found: r.found,
		path_length: r.pathLength,
		path: r.steps.map((s) => ({
			node_id: s.nodeUid,
			symbol: s.symbol,
			file: s.file,
			line: s.line,
			edge_type: s.edgeType,
		})),
	};
}

export function formatCycleResult(r: CycleResult): Record<string, unknown> {
	return {
		cycle_id: r.cycleId,
		length: r.length,
		nodes: r.nodes.map((n) => ({
			node_id: n.nodeUid,
			name: n.name,
			file: n.file,
		})),
	};
}
