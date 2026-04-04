import type { Command } from "commander";
import type {
	CycleResult,
	DeadNodeResult,
	ModuleStats,
	NodeKind,
	PathResult,
	QueryResult,
} from "../../core/model/index.js";
import type {
	DomainVersionRow,
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
	formatPathResult as formatPathResultJson,
	formatQueryResult,
} from "../formatters/json.js";
import {
	formatCycles,
	formatDeadNodes,
	formatFunctionMetrics,
	formatModuleMetricAggregates,
	formatModuleStats,
	formatPathResult,
} from "../formatters/table.js";
import { registerAssessmentCommands } from "./graph/assessments.js";
import {
	outputError,
	outputNodeResults,
	parseEdgeTypes,
	resolveSnapshot,
	resolveSymbolKey,
} from "./graph/helpers.js";
import { registerImportCommands } from "./graph/imports.js";
import { registerObligationCommands } from "./graph/obligations.js";

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

	graph
		.command("versions <repo>")
		.description("Extracted domain versions from manifest files")
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

			const versions = ctx.storage.queryDomainVersions(snapshotUid);

			if (opts.json) {
				const qr: QueryResult<DomainVersionRow> = {
					command: "graph versions",
					repo: snap.repoName,
					snapshot: snapshotUid,
					snapshotScope: snap.snapshotScope,
					basisCommit: snap.basisCommit,
					results: versions,
					count: versions.length,
					stale: snap.stale,
				};
				console.log(
					formatQueryResult(qr, (v) => ({
						kind: v.kind,
						value: v.value,
						source_file: v.sourceFile,
					})),
				);
			} else {
				if (versions.length === 0) {
					console.log("No domain versions found. No package.json detected.");
				} else {
					for (const v of versions) {
						console.log(`${v.kind}: ${v.value}  (from ${v.sourceFile})`);
					}
				}
			}
		});

	// Delegate to extracted command modules
	registerImportCommands(graph, getCtx);
	registerAssessmentCommands(graph, getCtx);
	registerObligationCommands(graph, getCtx);
}
