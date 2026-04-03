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
	VerificationObligation,
} from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
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

	graph
		.command("coverage <repo> <report>")
		.description("Import coverage from Istanbul/c8 JSON report")
		.option("--json", "JSON output")
		.action(
			async (repoRef: string, reportPath: string, opts: { json?: boolean }) => {
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

				// Import coverage
				const { importCoverageReport } = await import(
					"../../adapters/importers/coverage-import.js"
				);
				let result: Awaited<ReturnType<typeof importCoverageReport>>;
				try {
					result = await importCoverageReport(reportPath, repo.rootPath);
				} catch (err) {
					outputError(
						opts.json,
						`Failed to read coverage report: ${err instanceof Error ? err.message : String(err)}`,
					);
					process.exitCode = 1;
					return;
				}

				// Filter to indexed files only
				const indexedFiles = new Set<string>();
				const files = ctx.storage.getFilesByRepo(repoUid);
				for (const f of files) {
					indexedFiles.add(f.path);
				}
				const matched = result.files.filter((f) =>
					indexedFiles.has(f.filePath),
				);

				// Idempotent: delete previous coverage measurements
				ctx.storage.deleteMeasurementsByKind(snapshotUid, [
					"line_coverage",
					"function_coverage",
					"branch_coverage",
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

				for (const fc of matched) {
					const fileKey = `${repoUid}:${fc.filePath}:FILE`;
					if (fc.lineCoverage !== null) {
						measurements.push({
							measurementUid: crypto.randomUUID(),
							snapshotUid,
							repoUid,
							targetStableKey: fileKey,
							kind: "line_coverage",
							valueJson: JSON.stringify({
								value: Math.round(fc.lineCoverage * 10000) / 10000,
								covered: Math.round(fc.lineCoverage * fc.totalLines),
								total: fc.totalLines,
							}),
							source: "coverage-import:0.1.0",
							createdAt: now,
						});
					}
					if (fc.functionCoverage !== null) {
						measurements.push({
							measurementUid: crypto.randomUUID(),
							snapshotUid,
							repoUid,
							targetStableKey: fileKey,
							kind: "function_coverage",
							valueJson: JSON.stringify({
								value: Math.round(fc.functionCoverage * 10000) / 10000,
								covered: Math.round(fc.functionCoverage * fc.totalFunctions),
								total: fc.totalFunctions,
							}),
							source: "coverage-import:0.1.0",
							createdAt: now,
						});
					}
					if (fc.branchCoverage !== null) {
						measurements.push({
							measurementUid: crypto.randomUUID(),
							snapshotUid,
							repoUid,
							targetStableKey: fileKey,
							kind: "branch_coverage",
							valueJson: JSON.stringify({
								value: Math.round(fc.branchCoverage * 10000) / 10000,
								covered: Math.round(fc.branchCoverage * fc.totalBranches),
								total: fc.totalBranches,
							}),
							source: "coverage-import:0.1.0",
							createdAt: now,
						});
					}
				}

				if (measurements.length > 0) {
					ctx.storage.insertMeasurements(measurements);
				}

				// Display
				if (opts.json) {
					const results = matched.map((fc) => ({
						file: fc.filePath,
						line_coverage: fc.lineCoverage,
						function_coverage: fc.functionCoverage,
						branch_coverage: fc.branchCoverage,
					}));
					console.log(
						JSON.stringify(
							{
								command: "graph coverage",
								repo: snap.repoName,
								snapshot: snapshotUid,
								results,
								count: results.length,
								total_in_report: result.files.length,
								matched_to_index: matched.length,
							},
							null,
							2,
						),
					);
				} else {
					if (matched.length === 0) {
						console.log(
							`Coverage report contains ${result.files.length} files, but none matched the indexed file set.`,
						);
					} else {
						console.log(
							"FILE                                         LINE%  FUNC%  BRANCH%",
						);
						for (const fc of matched) {
							const file = fc.filePath.padEnd(45);
							const line =
								fc.lineCoverage !== null
									? `${(fc.lineCoverage * 100).toFixed(0)}%`.padStart(5)
									: "   - ";
							const func =
								fc.functionCoverage !== null
									? `${(fc.functionCoverage * 100).toFixed(0)}%`.padStart(6)
									: "    - ";
							const branch =
								fc.branchCoverage !== null
									? `${(fc.branchCoverage * 100).toFixed(0)}%`.padStart(8)
									: "      - ";
							console.log(`${file}${line}${func}${branch}`);
						}
						console.log("");
						console.log(
							`${matched.length} files imported from ${result.files.length} in report`,
						);
					}
				}
			},
		);

	graph
		.command("risk <repo>")
		.description(
			"Under-tested hotspots: files that are complex, churned, AND poorly covered",
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

			// Read existing hotspot inferences
			const hotspots = ctx.storage.queryInferences(
				snapshotUid,
				"hotspot_score",
			);
			if (hotspots.length === 0) {
				const empty = {
					command: "graph risk",
					repo: snap.repoName,
					snapshot: snapshotUid,
					results: [] as unknown[],
					count: 0,
					total_files: 0,
					formula: "hotspot_score * (1 - line_coverage)",
					formula_version: 1,
				};
				if (opts.json) {
					console.log(JSON.stringify(empty, null, 2));
				} else {
					console.log(
						"No hotspot data. Run `graph churn` then `graph hotspots` first.",
					);
				}
				return;
			}

			// Build hotspot map from inferences
			const hotspotMap = new Map<
				string,
				{ normalizedScore: number; churnLines: number; sumComplexity: number }
			>();
			for (const inf of hotspots) {
				const val = JSON.parse(inf.valueJson) as Record<string, number>;
				hotspotMap.set(inf.targetStableKey, {
					normalizedScore: val.normalized_score ?? 0,
					churnLines: val.churn_lines ?? 0,
					sumComplexity: val.sum_complexity ?? 0,
				});
			}

			// Read line_coverage measurements
			const coverageRows = ctx.storage.queryMeasurementsByKind(
				snapshotUid,
				"line_coverage",
			);
			const coverageMap = new Map<string, number>();
			for (const row of coverageRows) {
				const val = JSON.parse(row.valueJson) as { value: number };
				coverageMap.set(row.targetStableKey, val.value);
			}

			// Compute risk: hotspot_score * (1 - line_coverage)
			// Files without coverage data are treated as 0% covered (maximum risk multiplier).
			const riskEntries: Array<{
				fileKey: string;
				filePath: string;
				riskScore: number;
				hotspotScore: number;
				lineCoverage: number | null;
				churnLines: number;
				sumComplexity: number;
			}> = [];

			for (const [fileKey, hs] of hotspotMap) {
				const coverage = coverageMap.get(fileKey) ?? null;
				const coverageMultiplier = coverage !== null ? 1 - coverage : 1;
				const riskScore =
					Math.round(hs.normalizedScore * coverageMultiplier * 100) / 100;
				const filePath = fileKey
					.replace(`${repoUid}:`, "")
					.replace(/:FILE$/, "");

				riskEntries.push({
					fileKey,
					filePath,
					riskScore,
					hotspotScore: hs.normalizedScore,
					lineCoverage: coverage,
					churnLines: hs.churnLines,
					sumComplexity: hs.sumComplexity,
				});
			}

			riskEntries.sort((a, b) => b.riskScore - a.riskScore);

			// Persist as inferences (idempotent)
			ctx.storage.deleteInferencesByKind(snapshotUid, "under_tested_hotspot");
			const now = new Date().toISOString();
			const inferences = riskEntries.map((r) => ({
				inferenceUid: crypto.randomUUID(),
				snapshotUid,
				repoUid,
				targetStableKey: r.fileKey,
				kind: "under_tested_hotspot",
				valueJson: JSON.stringify({
					normalized_risk: r.riskScore,
					hotspot_score: r.hotspotScore,
					line_coverage: r.lineCoverage,
					churn_lines: r.churnLines,
					sum_complexity: r.sumComplexity,
					formula_version: 1,
				}),
				confidence: 1.0,
				basisJson: JSON.stringify({
					inputs: ["hotspot_score", "line_coverage"],
					formula: "hotspot_score * (1 - line_coverage)",
				}),
				extractor: "risk-analyzer:0.1.0",
				createdAt: now,
			}));
			ctx.storage.insertInferences(inferences);

			// Display
			const limit = Number.parseInt(opts.limit, 10);
			const display = riskEntries.slice(0, limit);

			if (opts.json) {
				const results = display.map((r) => ({
					file: r.filePath,
					risk_score: r.riskScore,
					hotspot_score: r.hotspotScore,
					line_coverage: r.lineCoverage,
					churn_lines: r.churnLines,
					sum_complexity: r.sumComplexity,
				}));
				console.log(
					JSON.stringify(
						{
							command: "graph risk",
							repo: snap.repoName,
							snapshot: snapshotUid,
							results,
							count: results.length,
							total_files: riskEntries.length,
							formula: "hotspot_score * (1 - line_coverage)",
							formula_version: 1,
						},
						null,
						2,
					),
				);
			} else {
				console.log(
					"FILE                                         RISK  HOTSPOT  COVERAGE  CHURN  SUM_CC",
				);
				for (const r of display) {
					const file = r.filePath.padEnd(45);
					const risk = r.riskScore.toFixed(2).padStart(5);
					const hs = r.hotspotScore.toFixed(2).padStart(8);
					const cov =
						r.lineCoverage !== null
							? `${(r.lineCoverage * 100).toFixed(0)}%`.padStart(9)
							: "     none";
					const churn = String(r.churnLines).padStart(7);
					const cc = String(r.sumComplexity).padStart(7);
					console.log(`${file}${risk}${hs}${cov}${churn}${cc}`);
				}
				console.log("");
				console.log(
					`${riskEntries.length} files assessed (showing ${display.length})`,
				);
				console.log("Formula: hotspot_score * (1 - line_coverage) (v1)");
				console.log(
					"Files without coverage data are treated as 0% covered (maximum risk).",
				);
			}
		});

	graph
		.command("obligations <repo>")
		.description(
			"Evaluate verification obligations against current measurements",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
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

			// Load all active requirements
			const requirements = ctx.storage.getActiveDeclarations({
				repoUid,
				kind: DeclarationKind.REQUIREMENT,
			});

			if (requirements.length === 0) {
				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "graph obligations",
								repo: snap.repoName,
								snapshot: snapshotUid,
								results: [],
								count: 0,
							},
							null,
							2,
						),
					);
				} else {
					console.log(
						"No requirements declared. Use `declare requirement` to create one.",
					);
				}
				return;
			}

			type Verdict = "PASS" | "FAIL" | "MISSING_EVIDENCE" | "UNSUPPORTED";

			interface ObligationResult {
				reqId: string;
				reqVersion: number;
				obligation: string;
				method: string;
				target: string | null;
				threshold: number | null;
				operator: string | null;
				verdict: Verdict;
				evidence: Record<string, unknown>;
			}

			const results: ObligationResult[] = [];

			for (const reqDecl of requirements) {
				const val = JSON.parse(reqDecl.valueJson) as {
					req_id: string;
					version: number;
					verification?: VerificationObligation[];
				};

				if (!val.verification || val.verification.length === 0) continue;

				for (const obl of val.verification) {
					const result: ObligationResult = {
						reqId: val.req_id,
						reqVersion: val.version,
						obligation: obl.obligation,
						method: obl.method,
						target: obl.target ?? null,
						threshold: obl.threshold ?? null,
						operator: obl.operator ?? null,
						verdict: "UNSUPPORTED",
						evidence: {},
					};

					// Evaluate based on method
					switch (obl.method) {
						case "arch_violations": {
							if (!obl.target) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = { reason: "no target specified" };
								break;
							}
							// Check boundary violations for the target module
							const boundaries = ctx.storage.getActiveDeclarations({
								repoUid,
								kind: "boundary" as DeclarationKind,
								targetStableKey: `${repoUid}:${obl.target}:MODULE`,
							});
							if (boundaries.length === 0) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: "no boundary declarations for target",
								};
								break;
							}
							let totalViolations = 0;
							for (const bd of boundaries) {
								const bv = JSON.parse(bd.valueJson) as { forbids: string };
								const violations = ctx.storage.findImportsBetweenPaths({
									snapshotUid,
									sourcePrefix: obl.target,
									targetPrefix: bv.forbids,
								});
								totalViolations += violations.length;
							}
							result.verdict = totalViolations === 0 ? "PASS" : "FAIL";
							result.evidence = {
								violation_count: totalViolations,
								snapshot: snapshotUid,
							};
							break;
						}
						case "coverage_threshold": {
							if (!obl.target || obl.threshold === undefined) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: "target or threshold not specified",
								};
								break;
							}
							// Read coverage measurements for files under the target path
							const coverageRows = ctx.storage.queryMeasurementsByKind(
								snapshotUid,
								"line_coverage",
							);
							const prefix = `${repoUid}:${obl.target}/`;
							const matchingCoverage = coverageRows.filter((r) =>
								r.targetStableKey.startsWith(prefix),
							);
							if (matchingCoverage.length === 0) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: "no coverage data for target path",
								};
								break;
							}
							const avgCoverage =
								matchingCoverage.reduce((sum, r) => {
									const v = JSON.parse(r.valueJson) as { value: number };
									return sum + v.value;
								}, 0) / matchingCoverage.length;
							const op = obl.operator ?? ">=";
							const pass =
								op === ">="
									? avgCoverage >= obl.threshold
									: op === ">"
										? avgCoverage > obl.threshold
										: op === "<="
											? avgCoverage <= obl.threshold
											: op === "<"
												? avgCoverage < obl.threshold
												: avgCoverage === obl.threshold;
							result.verdict = pass ? "PASS" : "FAIL";
							result.evidence = {
								avg_coverage: Math.round(avgCoverage * 10000) / 10000,
								threshold: obl.threshold,
								operator: op,
								files_measured: matchingCoverage.length,
							};
							break;
						}
						case "complexity_threshold": {
							if (!obl.target || obl.threshold === undefined) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: "target or threshold not specified",
								};
								break;
							}
							const ccRows = ctx.storage.queryMeasurementsByKind(
								snapshotUid,
								"cyclomatic_complexity",
							);
							const ccPrefix = `${repoUid}:${obl.target}/`;
							const matchingCC = ccRows.filter((r) =>
								r.targetStableKey.includes(ccPrefix),
							);
							if (matchingCC.length === 0) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: "no complexity data for target path",
								};
								break;
							}
							const maxCC = Math.max(
								...matchingCC.map((r) => {
									const v = JSON.parse(r.valueJson) as { value: number };
									return v.value;
								}),
							);
							const ccOp = obl.operator ?? "<=";
							const ccPass =
								ccOp === "<="
									? maxCC <= obl.threshold
									: ccOp === "<"
										? maxCC < obl.threshold
										: ccOp === ">="
											? maxCC >= obl.threshold
											: ccOp === ">"
												? maxCC > obl.threshold
												: maxCC === obl.threshold;
							result.verdict = ccPass ? "PASS" : "FAIL";
							result.evidence = {
								max_complexity: maxCC,
								threshold: obl.threshold,
								operator: ccOp,
								functions_measured: matchingCC.length,
							};
							break;
						}
						case "hotspot_threshold": {
							if (obl.threshold === undefined) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = { reason: "threshold not specified" };
								break;
							}
							let hotspots = ctx.storage.queryInferences(
								snapshotUid,
								"hotspot_score",
							);
							// Filter to target path if specified
							if (obl.target) {
								const hsPrefix = `${repoUid}:${obl.target}/`;
								hotspots = hotspots.filter((h) =>
									h.targetStableKey.startsWith(hsPrefix),
								);
							}
							if (hotspots.length === 0) {
								result.verdict = "MISSING_EVIDENCE";
								result.evidence = {
									reason: obl.target
										? "no hotspot data for target path"
										: "no hotspot data",
								};
								break;
							}
							const maxHotspot = Math.max(
								...hotspots.map((h) => {
									const v = JSON.parse(h.valueJson) as {
										normalized_score: number;
									};
									return v.normalized_score;
								}),
							);
							const hsOp = obl.operator ?? "<=";
							const hsPass =
								hsOp === "<="
									? maxHotspot <= obl.threshold
									: maxHotspot < obl.threshold;
							result.verdict = hsPass ? "PASS" : "FAIL";
							result.evidence = {
								max_hotspot_score: maxHotspot,
								threshold: obl.threshold,
							};
							break;
						}
						default:
							result.verdict = "UNSUPPORTED";
							result.evidence = {
								reason: `method "${obl.method}" not yet supported`,
							};
					}

					results.push(result);
				}
			}

			// Output
			if (opts.json) {
				const passCount = results.filter((r) => r.verdict === "PASS").length;
				const failCount = results.filter((r) => r.verdict === "FAIL").length;
				console.log(
					JSON.stringify(
						{
							command: "graph obligations",
							repo: snap.repoName,
							snapshot: snapshotUid,
							results: results.map((r) => ({
								req_id: r.reqId,
								req_version: r.reqVersion,
								obligation: r.obligation,
								method: r.method,
								target: r.target,
								threshold: r.threshold,
								verdict: r.verdict,
								evidence: r.evidence,
							})),
							count: results.length,
							pass: passCount,
							fail: failCount,
						},
						null,
						2,
					),
				);
			} else {
				if (results.length === 0) {
					console.log("No verification obligations found in any requirement.");
				} else {
					for (const r of results) {
						const icon =
							r.verdict === "PASS"
								? "[PASS]"
								: r.verdict === "FAIL"
									? "[FAIL]"
									: r.verdict === "MISSING_EVIDENCE"
										? "[MISS]"
										: "[SKIP]";
						console.log(`${icon} ${r.reqId} v${r.reqVersion}: ${r.obligation}`);
						if (r.verdict === "FAIL" || r.verdict === "MISSING_EVIDENCE") {
							console.log(`       ${JSON.stringify(r.evidence)}`);
						}
					}
					const passCount = results.filter((r) => r.verdict === "PASS").length;
					const failCount = results.filter((r) => r.verdict === "FAIL").length;
					const missCount = results.filter(
						(r) => r.verdict === "MISSING_EVIDENCE",
					).length;
					console.log("");
					console.log(
						`${results.length} obligations: ${passCount} pass, ${failCount} fail, ${missCount} missing evidence`,
					);
				}
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
