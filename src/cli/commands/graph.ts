import { resolve } from "node:path";
import type { Command } from "commander";
import type {
	CycleResult,
	DeadNodeResult,
	EdgeType,
	ModuleStats,
	NodeKind,
	NodeResult,
	PathResult,
	QueryResult,
} from "../../core/model/index.js";
import type {
	FunctionMetricRow,
	ModuleMetricAggregate,
} from "../../core/ports/storage.js";
import type { AppContext } from "../../main.js";
import {
	formatCycleResult,
	formatDeadNodeResult,
	formatFunctionMetricResult,
	formatModuleMetricAggResult,
	formatModuleStatsResult,
	formatNodeResult,
	formatPathResult as formatPathResultJson,
	formatQueryResult,
} from "../formatters/json.js";
import {
	formatCycles,
	formatDeadNodes,
	formatFunctionMetrics,
	formatModuleMetricAggregates,
	formatModuleStats,
	formatNodeResults,
	formatPathResult,
} from "../formatters/table.js";

export function registerGraphCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const graph = program
		.command("graph")
		.description("Structural graph queries");

	graph
		.command("callers <repo> <symbol>")
		.description("Find all callers of a symbol")
		.option("--depth <n>", "Transitive depth", "1")
		.option("--edge-types <types>", "Comma-separated edge types", "CALLS")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				symbol: string,
				opts: { depth: string; edgeTypes: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				if (!snapshotUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				const stableKey = resolveSymbolKey(ctx, snapshotUid, symbol);
				if (!stableKey) {
					outputError(opts.json, `Symbol not found: ${symbol}`);
					process.exitCode = 1;
					return;
				}

				const parsed = parseEdgeTypes(opts.edgeTypes);
				if (!parsed.ok) {
					outputError(opts.json, parsed.error);
					process.exitCode = 1;
					return;
				}

				const results = ctx.storage.findCallers({
					snapshotUid,
					stableKey,
					maxDepth: Number.parseInt(opts.depth, 10),
					edgeTypes: parsed.types,
				});

				outputNodeResults("graph callers", snap, results, opts.json);
			},
		);

	graph
		.command("callees <repo> <symbol>")
		.description("Find all symbols called by this symbol")
		.option("--depth <n>", "Transitive depth", "1")
		.option("--edge-types <types>", "Comma-separated edge types", "CALLS")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				symbol: string,
				opts: { depth: string; edgeTypes: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				if (!snapshotUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				const stableKey = resolveSymbolKey(ctx, snapshotUid, symbol);
				if (!stableKey) {
					outputError(opts.json, `Symbol not found: ${symbol}`);
					process.exitCode = 1;
					return;
				}

				const parsed = parseEdgeTypes(opts.edgeTypes);
				if (!parsed.ok) {
					outputError(opts.json, parsed.error);
					process.exitCode = 1;
					return;
				}

				const results = ctx.storage.findCallees({
					snapshotUid,
					stableKey,
					maxDepth: Number.parseInt(opts.depth, 10),
					edgeTypes: parsed.types,
				});

				outputNodeResults("graph callees", snap, results, opts.json);
			},
		);

	graph
		.command("imports <repo> <file>")
		.description("Find imports of a file or module")
		.option("--depth <n>", "Transitive depth", "1")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				file: string,
				opts: { depth: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				const repoUid = snap.repoUid;
				if (!snapshotUid || !repoUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				// Try as a FILE stable key
				const fileKey = `${repoUid}:${file}:FILE`;
				const fileNode = ctx.storage.getNodeByStableKey(snapshotUid, fileKey);
				const stableKey = fileNode
					? fileKey
					: resolveSymbolKey(ctx, snapshotUid, file);

				if (!stableKey) {
					outputError(opts.json, `File or module not found: ${file}`);
					process.exitCode = 1;
					return;
				}

				const results = ctx.storage.findImports({
					snapshotUid,
					stableKey,
					maxDepth: Number.parseInt(opts.depth, 10),
				});

				outputNodeResults("graph imports", snap, results, opts.json);
			},
		);

	graph
		.command("path <repo> <from> <to>")
		.description("Find shortest path between two nodes")
		.option("--max-depth <n>", "Maximum traversal depth", "8")
		.option(
			"--edge-types <types>",
			"Comma-separated edge types",
			"CALLS,IMPORTS",
		)
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				from: string,
				to: string,
				opts: { maxDepth: string; edgeTypes: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				if (!snapshotUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				const fromKey = resolveSymbolKey(ctx, snapshotUid, from);
				const toKey = resolveSymbolKey(ctx, snapshotUid, to);
				if (!fromKey || !toKey) {
					outputError(opts.json, `Symbol not found: ${!fromKey ? from : to}`);
					process.exitCode = 1;
					return;
				}

				const parsed = parseEdgeTypes(opts.edgeTypes);
				if (!parsed.ok) {
					outputError(opts.json, parsed.error);
					process.exitCode = 1;
					return;
				}

				const result = ctx.storage.findPath({
					snapshotUid,
					fromStableKey: fromKey,
					toStableKey: toKey,
					maxDepth: Number.parseInt(opts.maxDepth, 10),
					edgeTypes: parsed.types,
				});

				if (opts.json) {
					const qr: QueryResult<PathResult> = {
						command: "graph path",
						repo: snap.repoName,
						snapshot: snapshotUid,
						snapshotScope: snap.snapshotScope,
						basisCommit: snap.basisCommit,
						results: [result],
						count: result.found ? 1 : 0,
						stale: snap.stale,
					};
					console.log(formatQueryResult(qr, formatPathResultJson));
				} else {
					console.log(formatPathResult(result));
				}
			},
		);

	graph
		.command("dead <repo>")
		.description("Find nodes with no incoming edges")
		.option("--kind <kind>", "Filter by node kind (SYMBOL, FILE, MODULE)")
		.option("--min-lines <n>", "Minimum lines of code")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				opts: { kind?: string; minLines?: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				if (!snapshotUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				const results = ctx.storage.findDeadNodes({
					snapshotUid,
					kind: opts.kind as NodeKind | undefined,
					minLines: opts.minLines
						? Number.parseInt(opts.minLines, 10)
						: undefined,
				});

				if (opts.json) {
					const qr: QueryResult<DeadNodeResult> = {
						command: "graph dead",
						repo: snap.repoName,
						snapshot: snapshotUid,
						snapshotScope: snap.snapshotScope,
						basisCommit: snap.basisCommit,
						results,
						count: results.length,
						stale: snap.stale,
					};
					console.log(formatQueryResult(qr, formatDeadNodeResult));
				} else {
					console.log(formatDeadNodes(results));
				}
			},
		);

	graph
		.command("cycles <repo>")
		.description("Detect dependency cycles")
		.option("--level <level>", "Detection level: file or module", "module")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { level: string; json?: boolean }) => {
			const ctx = getCtx();
			const snap = resolveSnapshot(ctx, repoRef);
			const { snapshotUid } = snap;
			if (!snapshotUid) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}

			const results = ctx.storage.findCycles({
				snapshotUid,
				level: opts.level as "file" | "module",
			});

			if (opts.json) {
				const qr: QueryResult<CycleResult> = {
					command: "graph cycles",
					repo: snap.repoName,
					snapshot: snapshotUid,
					snapshotScope: snap.snapshotScope,
					basisCommit: snap.basisCommit,
					results,
					count: results.length,
					stale: snap.stale,
				};
				console.log(formatQueryResult(qr, formatCycleResult));
			} else {
				console.log(formatCycles(results));
			}
		});

	graph
		.command("stats <repo>")
		.description(
			"Module structural metrics (fan-in/out, instability, abstractness)",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const snap = resolveSnapshot(ctx, repoRef);
			const { snapshotUid } = snap;
			if (!snapshotUid) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}

			const stats = ctx.storage.computeModuleStats(snapshotUid);

			if (opts.json) {
				const qr: QueryResult<ModuleStats> = {
					command: "graph stats",
					repo: snap.repoName,
					snapshot: snapshotUid,
					snapshotScope: snap.snapshotScope,
					basisCommit: snap.basisCommit,
					results: stats,
					count: stats.length,
					stale: snap.stale,
				};
				console.log(formatQueryResult(qr, formatModuleStatsResult));
			} else {
				console.log(formatModuleStats(stats));
			}
		});

	graph
		.command("metrics <repo>")
		.description("Function-level complexity metrics from stored measurements")
		.option("--sort <field>", "Sort by: cc, params, nesting", "cc")
		.option("--limit <n>", "Max results")
		.option("--module", "Aggregate per module instead of per function")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				opts: {
					sort: string;
					limit?: string;
					module?: boolean;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid } = snap;
				if (!snapshotUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				if (opts.module) {
					let aggregates = ctx.storage.queryModuleMetricAggregates(snapshotUid);
					// Module mode sorts by max CC desc (natural order).
					// --sort is not applicable (module aggregates have different fields).
					// --limit is applied as a simple slice.
					if (opts.limit) {
						aggregates = aggregates.slice(0, Number.parseInt(opts.limit, 10));
					}

					if (opts.json) {
						const qr: QueryResult<ModuleMetricAggregate> = {
							command: "graph metrics --module",
							repo: snap.repoName,
							snapshot: snapshotUid,
							snapshotScope: snap.snapshotScope,
							basisCommit: snap.basisCommit,
							results: aggregates,
							count: aggregates.length,
							stale: snap.stale,
						};
						console.log(formatQueryResult(qr, formatModuleMetricAggResult));
					} else {
						console.log(formatModuleMetricAggregates(aggregates));
					}
					return;
				}

				const sortMap: Record<string, string> = {
					cc: "cyclomatic_complexity",
					params: "parameter_count",
					nesting: "max_nesting_depth",
				};
				const sortBy = (sortMap[opts.sort] ?? "cyclomatic_complexity") as
					| "cyclomatic_complexity"
					| "parameter_count"
					| "max_nesting_depth";

				const metrics = ctx.storage.queryFunctionMetrics({
					snapshotUid,
					sortBy,
					limit: opts.limit ? Number.parseInt(opts.limit, 10) : undefined,
				});

				if (opts.json) {
					const qr: QueryResult<FunctionMetricRow> = {
						command: "graph metrics",
						repo: snap.repoName,
						snapshot: snapshotUid,
						snapshotScope: snap.snapshotScope,
						basisCommit: snap.basisCommit,
						results: metrics,
						count: metrics.length,
						stale: snap.stale,
					};
					console.log(formatQueryResult(qr, formatFunctionMetricResult));
				} else {
					console.log(formatFunctionMetrics(metrics));
				}
			},
		);
}

// ── Helpers ────────────────────────────────────────────────────────────

interface ResolvedSnapshot {
	snapshotUid: string | null;
	repoName: string;
	repoUid: string | null;
	snapshotScope: string;
	basisCommit: string | null;
	stale: boolean;
}

function resolveSnapshot(ctx: AppContext, ref: string): ResolvedSnapshot {
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
		// Map internal snapshot kinds to the documented wire format:
		// "full" -> "full", everything else -> "incremental"
		snapshotScope: snapshot?.kind === "full" ? "full" : "incremental",
		basisCommit: snapshot?.basisCommit ?? null,
		// In v1, `stale` is always false because refresh does full re-extraction
		// and nothing writes parse_status='stale'. When v2 adds a file watcher
		// or incremental diff, changed files will be marked stale and this
		// field will activate.
		stale: staleFiles.length > 0,
	};
}

function resolveSymbolKey(
	ctx: AppContext,
	snapshotUid: string,
	query: string,
): string | null {
	// Try as a direct stable key first
	const direct = ctx.storage.getNodeByStableKey(snapshotUid, query);
	if (direct) return query;

	// Try fuzzy symbol resolution
	const candidates = ctx.storage.resolveSymbol({
		snapshotUid,
		query,
		limit: 1,
	});
	return candidates.length > 0 ? candidates[0].stableKey : null;
}

function outputNodeResults(
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

/**
 * Parse and validate a comma-separated edge types string.
 * Returns null with an error message if any type is invalid.
 */
function parseEdgeTypes(
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

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
