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

	graph
		.command("churn <repo>")
		.description("Import and display per-file git churn measurements")
		.option("--since <period>", "Time window (git date format)", "90.days.ago")
		.option("--limit <n>", "Max results to display")
		.option("--json", "JSON output")
		.action(
			async (
				repoRef: string,
				opts: { since: string; limit?: string; json?: boolean },
			) => {
				const ctx = getCtx();
				const snap = resolveSnapshot(ctx, repoRef);
				const { snapshotUid, repoUid } = snap;
				if (!snapshotUid || !repoUid) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}

				const repo = ctx.storage.getRepo({ uid: repoUid });
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				// Import churn from git
				const rawChurn = await ctx.git.getFileChurn(repo.rootPath, opts.since);

				// Filter to files that exist as indexed FILE nodes.
				// Git history includes docs, configs, lockfiles etc. that
				// are not in the graph. Only persist churn for indexed files.
				const indexedFiles = new Set<string>();
				const files = ctx.storage.getFilesByRepo(repoUid);
				for (const f of files) {
					indexedFiles.add(f.path);
				}
				const churnData = rawChurn.filter((e) => indexedFiles.has(e.filePath));

				// Idempotent: delete previous churn measurements for this
				// snapshot before inserting fresh data.
				ctx.storage.deleteMeasurementsByKind(snapshotUid, [
					"change_frequency",
					"churn_lines",
				]);

				// Persist as measurements
				const now = new Date().toISOString();
				const measurements: Array<{
					measurementUid: string;
					snapshotUid: string;
					repoUid: string;
					targetStableKey: string;
					kind: string;
					valueJson: string;
					source: string;
					createdAt: string;
				}> = [];

				for (const entry of churnData) {
					const fileKey = `${repoUid}:${entry.filePath}:FILE`;
					measurements.push({
						measurementUid: crypto.randomUUID(),
						snapshotUid,
						repoUid,
						targetStableKey: fileKey,
						kind: "change_frequency",
						valueJson: JSON.stringify({
							value: entry.commitCount,
							since: opts.since,
						}),
						source: "git-churn:0.1.0",
						createdAt: now,
					});
					measurements.push({
						measurementUid: crypto.randomUUID(),
						snapshotUid,
						repoUid,
						targetStableKey: fileKey,
						kind: "churn_lines",
						valueJson: JSON.stringify({
							value: entry.linesChanged,
							since: opts.since,
						}),
						source: "git-churn:0.1.0",
						createdAt: now,
					});
				}

				if (measurements.length > 0) {
					ctx.storage.insertMeasurements(measurements);
				}

				// Display results
				let display = churnData;
				if (opts.limit) {
					display = display.slice(0, Number.parseInt(opts.limit, 10));
				}

				if (opts.json) {
					const results = display.map((e) => ({
						file: e.filePath,
						commit_count: e.commitCount,
						lines_changed: e.linesChanged,
					}));
					const qr = {
						command: "graph churn",
						repo: snap.repoName,
						snapshot: snapshotUid,
						snapshot_scope: snap.snapshotScope,
						basis_commit: snap.basisCommit,
						results,
						count: results.length,
						stale: snap.stale,
						since: opts.since,
					};
					console.log(JSON.stringify(qr, null, 2));
				} else {
					if (display.length === 0) {
						console.log("No file changes found in the specified window.");
					} else {
						console.log(
							"FILE                                         COMMITS  LINES_CHANGED",
						);
						for (const e of display) {
							const file = e.filePath.padEnd(45);
							const commits = String(e.commitCount).padStart(7);
							const lines = String(e.linesChanged).padStart(14);
							console.log(`${file}${commits}${lines}`);
						}
						console.log("");
						console.log(
							`${churnData.length} files changed (showing ${display.length}, --since ${opts.since})`,
						);
					}
				}
			},
		);

	graph
		.command("hotspots <repo>")
		.description(
			"File-level hotspots: churn x complexity (requires prior churn import)",
		)
		.option("--limit <n>", "Max results", "20")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { limit: string; json?: boolean }) => {
			const ctx = getCtx();
			const snap = resolveSnapshot(ctx, repoRef);
			const { snapshotUid, repoUid } = snap;
			if (!snapshotUid || !repoUid) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}

			// Read hotspot inputs (churn + complexity per file)
			const inputs = ctx.storage.queryHotspotInputs(snapshotUid);

			if (inputs.length === 0) {
				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "graph hotspots",
								repo: snap.repoName,
								snapshot: snapshotUid,
								results: [],
								count: 0,
								total_files: 0,
								formula: "churn_lines * sum_cyclomatic_complexity",
								formula_version: 1,
							},
							null,
							2,
						),
					);
				} else {
					console.log(
						"No hotspot data. Run `graph churn` first to import churn measurements.",
					);
				}
				return;
			}

			// Compute hotspot scores
			const scored = inputs.map((inp) => ({
				...inp,
				rawScore: inp.churnLines * inp.sumComplexity,
			}));
			const maxRaw = Math.max(...scored.map((s) => s.rawScore));
			const hotspots = scored.map((s) => ({
				...s,
				normalizedScore:
					maxRaw > 0 ? Math.round((s.rawScore / maxRaw) * 100) / 100 : 0,
			}));

			// Persist as inferences (idempotent)
			ctx.storage.deleteInferencesByKind(snapshotUid, "hotspot_score");
			const now = new Date().toISOString();
			const inferences = hotspots.map((h) => ({
				inferenceUid: crypto.randomUUID(),
				snapshotUid,
				repoUid,
				targetStableKey: h.fileStableKey,
				kind: "hotspot_score",
				valueJson: JSON.stringify({
					normalized_score: h.normalizedScore,
					raw_score: h.rawScore,
					churn_lines: h.churnLines,
					change_frequency: h.changeFrequency,
					sum_complexity: h.sumComplexity,
					formula_version: 1,
				}),
				confidence: 1.0,
				basisJson: JSON.stringify({
					measurements: [
						"churn_lines",
						"change_frequency",
						"cyclomatic_complexity",
					],
					formula: "churn_lines * sum_cyclomatic_complexity",
				}),
				extractor: "hotspot-analyzer:0.1.0",
				createdAt: now,
			}));
			ctx.storage.insertInferences(inferences);

			// Display
			const limit = Number.parseInt(opts.limit, 10);
			const display = hotspots.slice(0, limit);

			if (opts.json) {
				const results = display.map((h) => ({
					file: h.filePath,
					normalized_score: h.normalizedScore,
					raw_score: h.rawScore,
					churn_lines: h.churnLines,
					change_frequency: h.changeFrequency,
					sum_complexity: h.sumComplexity,
				}));
				console.log(
					JSON.stringify(
						{
							command: "graph hotspots",
							repo: snap.repoName,
							snapshot: snapshotUid,
							results,
							count: results.length,
							total_files: hotspots.length,
							formula: "churn_lines * sum_cyclomatic_complexity",
							formula_version: 1,
						},
						null,
						2,
					),
				);
			} else {
				console.log(
					"FILE                                         SCORE  CHURN  COMMITS  SUM_CC",
				);
				for (const h of display) {
					const file = h.filePath.padEnd(45);
					const score = h.normalizedScore.toFixed(2).padStart(5);
					const churn = String(h.churnLines).padStart(7);
					const commits = String(h.changeFrequency).padStart(8);
					const cc = String(h.sumComplexity).padStart(7);
					console.log(`${file}${score}${churn}${commits}${cc}`);
				}
				console.log("");
				console.log(
					`${hotspots.length} files with hotspot data (showing ${display.length})`,
				);
				console.log("Formula: churn_lines * sum_cyclomatic_complexity (v1)");
			}
		});
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
