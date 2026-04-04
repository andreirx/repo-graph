/**
 * Assessment commands: hotspots and risk.
 * These compute and persist inferences from existing measurements.
 */

import type { Command } from "commander";
import type { AppContext } from "../../../main.js";
import { outputError, resolveSnapshot } from "./helpers.js";

export function registerAssessmentCommands(
	graph: Command,
	getCtx: () => AppContext,
): void {
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
				// Still write the assessment-run marker so trend knows
				// computation was attempted (zero results, not "never run").
				ctx.storage.deleteInferencesByKind(
					snapshotUid,
					"assessment_run:hotspot_score",
				);
				ctx.storage.insertInferences([
					{
						inferenceUid: crypto.randomUUID(),
						snapshotUid,
						repoUid,
						targetStableKey: `${repoUid}:.:MODULE`,
						kind: "assessment_run:hotspot_score",
						valueJson: JSON.stringify({
							assessment: "hotspot_score",
							formula_version: 1,
							files_assessed: 0,
							computed_at: new Date().toISOString(),
						}),
						confidence: 1.0,
						basisJson: JSON.stringify({
							formula: "churn_lines * sum_cyclomatic_complexity",
						}),
						extractor: "hotspot-analyzer:0.1.0",
						createdAt: new Date().toISOString(),
					},
				]);
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

			// Persist assessment-run marker so trend can distinguish
			// "never computed" from "computed with zero results."
			ctx.storage.deleteInferencesByKind(
				snapshotUid,
				"assessment_run:hotspot_score",
			);
			ctx.storage.insertInferences([
				{
					inferenceUid: crypto.randomUUID(),
					snapshotUid,
					repoUid,
					targetStableKey: `${repoUid}:.:MODULE`,
					kind: "assessment_run:hotspot_score",
					valueJson: JSON.stringify({
						assessment: "hotspot_score",
						formula_version: 1,
						files_assessed: hotspots.length,
						computed_at: now,
					}),
					confidence: 1.0,
					basisJson: JSON.stringify({
						formula: "churn_lines * sum_cyclomatic_complexity",
					}),
					extractor: "hotspot-analyzer:0.1.0",
					createdAt: now,
				},
			]);

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
				// Write marker even on empty path
				ctx.storage.deleteInferencesByKind(
					snapshotUid,
					"assessment_run:under_tested_hotspot",
				);
				ctx.storage.insertInferences([
					{
						inferenceUid: crypto.randomUUID(),
						snapshotUid,
						repoUid,
						targetStableKey: `${repoUid}:.:MODULE`,
						kind: "assessment_run:under_tested_hotspot",
						valueJson: JSON.stringify({
							assessment: "under_tested_hotspot",
							formula_version: 1,
							files_assessed: 0,
							computed_at: new Date().toISOString(),
						}),
						confidence: 1.0,
						basisJson: JSON.stringify({
							formula: "hotspot_score * (1 - line_coverage)",
						}),
						extractor: "risk-analyzer:0.1.0",
						createdAt: new Date().toISOString(),
					},
				]);
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

			// Persist assessment-run marker
			ctx.storage.deleteInferencesByKind(
				snapshotUid,
				"assessment_run:under_tested_hotspot",
			);
			ctx.storage.insertInferences([
				{
					inferenceUid: crypto.randomUUID(),
					snapshotUid,
					repoUid,
					targetStableKey: `${repoUid}:.:MODULE`,
					kind: "assessment_run:under_tested_hotspot",
					valueJson: JSON.stringify({
						assessment: "under_tested_hotspot",
						formula_version: 1,
						files_assessed: riskEntries.length,
						computed_at: now,
					}),
					confidence: 1.0,
					basisJson: JSON.stringify({
						formula: "hotspot_score * (1 - line_coverage)",
					}),
					extractor: "risk-analyzer:0.1.0",
					createdAt: now,
				},
			]);

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
}
