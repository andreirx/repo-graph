/**
 * Shared helpers for graph command handlers.
 */

import { resolve } from "node:path";
import type {
	EdgeType,
	NodeResult,
	QueryResult,
} from "../../../core/model/index.js";
import type { AppContext } from "../../../main.js";
import { formatNodeResult, formatQueryResult } from "../../formatters/json.js";
import { formatNodeResults } from "../../formatters/table.js";

export interface ResolvedSnapshot {
	snapshotUid: string | null;
	repoName: string;
	repoUid: string | null;
	snapshotScope: string;
	basisCommit: string | null;
	stale: boolean;
}

export function resolveSnapshot(
	ctx: AppContext,
	ref: string,
): ResolvedSnapshot {
	const repo =
		ctx.storage.getRepo({ uid: ref }) ??
		ctx.storage.getRepo({ name: ref }) ??
		ctx.storage.getRepo({ rootPath: resolve(ref) });

	if (!repo)
		return {
			snapshotUid: null,
			repoName: ref,
			repoUid: null,
			snapshotScope: "full",
			basisCommit: null,
			stale: false,
		};

	const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
	const staleFiles = snapshot
		? ctx.storage.getStaleFiles(snapshot.snapshotUid)
		: [];

	return {
		snapshotUid: snapshot?.snapshotUid ?? null,
		repoName: repo.name,
		repoUid: repo.repoUid,
		snapshotScope: snapshot?.kind === "full" ? "full" : "incremental",
		basisCommit: snapshot?.basisCommit ?? null,
		stale: staleFiles.length > 0,
	};
}

export function resolveSymbolKey(
	ctx: AppContext,
	snapshotUid: string,
	query: string,
): string | null {
	const direct = ctx.storage.getNodeByStableKey(snapshotUid, query);
	if (direct) return query;

	const candidates = ctx.storage.resolveSymbol({
		snapshotUid,
		query,
		limit: 1,
	});
	return candidates.length > 0 ? candidates[0].stableKey : null;
}

export function outputNodeResults(
	command: string,
	snap: ResolvedSnapshot,
	results: NodeResult[],
	json?: boolean,
): void {
	if (json) {
		const qr: QueryResult<NodeResult> = {
			command,
			repo: snap.repoName,
			snapshot: snap.snapshotUid ?? "",
			snapshotScope: snap.snapshotScope,
			basisCommit: snap.basisCommit,
			results,
			count: results.length,
			stale: snap.stale,
		};
		console.log(formatQueryResult(qr, formatNodeResult));
	} else {
		console.log(formatNodeResults(results));
	}
}

const VALID_EDGE_TYPES = new Set([
	"IMPORTS",
	"CALLS",
	"IMPLEMENTS",
	"INSTANTIATES",
	"READS",
	"WRITES",
	"EMITS",
	"CONSUMES",
	"ROUTES_TO",
	"REGISTERED_BY",
	"GATED_BY",
	"DEPENDS_ON",
	"OWNS",
	"TESTED_BY",
	"COVERS",
	"THROWS",
	"CATCHES",
	"TRANSITIONS_TO",
]);

export function parseEdgeTypes(
	raw: string,
): { ok: true; types: EdgeType[] } | { ok: false; error: string } {
	const types = raw.split(",").map((t) => t.trim());
	for (const t of types) {
		if (!VALID_EDGE_TYPES.has(t)) {
			const valid = [...VALID_EDGE_TYPES].sort().join(", ");
			return {
				ok: false,
				error: `Unknown edge type "${t}". Valid types: ${valid}`,
			};
		}
	}
	return { ok: true, types: types as EdgeType[] };
}

export function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
