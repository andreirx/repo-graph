/**
 * Import commands: churn and coverage.
 * These are mutation workflows that read external data and persist measurements.
 */

import type { Command } from "commander";
import type { AppContext } from "../../../main.js";
import { outputError, resolveSnapshot } from "./helpers.js";

export function registerImportCommands(
	graph: Command,
	getCtx: () => AppContext,
): void {
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
		.command("coverage <repo> <report>")
		.description("Import coverage from a supported report format (auto-detected)")
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

				// Detect and import coverage via injected importers
				const { readFile } = await import("node:fs/promises");
				let sniffContent: string;
				try {
					const full = await readFile(reportPath, "utf-8");
					sniffContent = full.slice(0, 4096);
				} catch (err) {
					outputError(
						opts.json,
						`Cannot read report: ${err instanceof Error ? err.message : String(err)}`,
					);
					process.exitCode = 1;
					return;
				}

				const importer = ctx.coverageImporters.find((i) =>
					i.canHandle(reportPath, sniffContent),
				);
				if (!importer) {
					const formats = ctx.coverageImporters
						.map((i) => i.formatName)
						.join(", ");
					outputError(
						opts.json,
						`Unrecognized coverage format. Supported: ${formats}`,
					);
					process.exitCode = 1;
					return;
				}

				let result: Awaited<ReturnType<typeof importer.importReport>>;
				try {
					result = await importer.importReport(reportPath, repo.rootPath);
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
							source: `coverage-${importer.formatName}:0.1.0`,
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
							source: `coverage-${importer.formatName}:0.1.0`,
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
							source: `coverage-${importer.formatName}:0.1.0`,
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
}
