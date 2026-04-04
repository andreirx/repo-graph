/**
 * trend — snapshot-to-snapshot health vector delta.
 *
 * Compares the latest snapshot against its parent to show whether
 * the codebase is improving or decaying across measured dimensions.
 *
 * Checks toolchain comparability before computing deltas.
 * Reports NOT_COMPARABLE when measurement semantics differ.
 */

import { resolve } from "node:path";
import type { Command } from "commander";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";
import type { ToolchainJson } from "../../version.js";

interface SnapshotHealth {
	snapshotUid: string;
	createdAt: string;
	filesTotal: number;
	nodesTotal: number;
	edgesTotal: number;
	cycleCount: number;
	/**
	 * Violations evaluated using CURRENT boundary declarations against
	 * the snapshot's code. This is not a historical record of what was
	 * violated at snapshot time — it is today's policy applied to old code.
	 */
	violationCount: number;
	avgComplexity: number;
	maxComplexity: number;
	functionCount: number;
	avgCoverage: number | null;
	coverageFiles: number;
	/** null when hotspot computation was never run for this snapshot. */
	hotspotFiles: number | null;
}

interface MetricDelta {
	metric: string;
	from: number | null;
	to: number | null;
	delta: number | null;
	direction: "improved" | "worsened" | "unchanged" | "new" | "no_data";
}

export function registerTrendCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("trend <repo>")
		.description(
			"Compare health metrics between latest snapshot and its parent",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repo =
				ctx.storage.getRepo({ uid: repoRef }) ??
				ctx.storage.getRepo({ name: repoRef }) ??
				ctx.storage.getRepo({ rootPath: resolve(repoRef) });

			if (!repo) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const latest = ctx.storage.getLatestSnapshot(repo.repoUid);
			if (!latest) {
				outputError(opts.json, "No snapshots found. Run `repo index` first.");
				process.exitCode = 1;
				return;
			}

			if (!latest.parentSnapshotUid) {
				outputError(
					opts.json,
					"Only one snapshot exists. Run `repo refresh` to create a second snapshot for comparison.",
				);
				process.exitCode = 1;
				return;
			}

			const parent = ctx.storage.getSnapshot(latest.parentSnapshotUid);
			if (!parent) {
				outputError(opts.json, "Parent snapshot not found in database.");
				process.exitCode = 1;
				return;
			}

			// Check comparability via toolchain provenance
			const latestToolchain = latest.toolchainJson
				? (JSON.parse(latest.toolchainJson) as ToolchainJson)
				: null;
			const parentToolchain = parent.toolchainJson
				? (JSON.parse(parent.toolchainJson) as ToolchainJson)
				: null;

			const incompatReasons: string[] = [];
			if (!latestToolchain || !parentToolchain) {
				incompatReasons.push("One or both snapshots lack toolchain provenance");
			} else {
				if (
					latestToolchain.extraction_semantics !==
					parentToolchain.extraction_semantics
				) {
					incompatReasons.push(
						`Extraction semantics differ: ${parentToolchain.extraction_semantics} -> ${latestToolchain.extraction_semantics}`,
					);
				}
				if (
					latestToolchain.stable_key_format !==
					parentToolchain.stable_key_format
				) {
					incompatReasons.push(
						`Stable key format differs: ${parentToolchain.stable_key_format} -> ${latestToolchain.stable_key_format}`,
					);
				}
				// Check measurement semantics
				const parentMeasSem = parentToolchain.measurement_semantics ?? {};
				const latestMeasSem = latestToolchain.measurement_semantics ?? {};
				for (const key of new Set([
					...Object.keys(parentMeasSem),
					...Object.keys(latestMeasSem),
				])) {
					if (
						(parentMeasSem as Record<string, number>)[key] !==
						(latestMeasSem as Record<string, number>)[key]
					) {
						incompatReasons.push(`Measurement semantics differ for "${key}"`);
					}
				}
			}

			const comparable = incompatReasons.length === 0;

			// Compute health for both snapshots
			const latestHealth = computeHealth(ctx, latest.snapshotUid, repo.repoUid);
			const parentHealth = computeHealth(ctx, parent.snapshotUid, repo.repoUid);

			// Compute deltas
			const deltas: MetricDelta[] = [];

			if (comparable) {
				addDelta(
					deltas,
					"files",
					parentHealth.filesTotal,
					latestHealth.filesTotal,
					"neutral",
				);
				addDelta(
					deltas,
					"nodes",
					parentHealth.nodesTotal,
					latestHealth.nodesTotal,
					"neutral",
				);
				addDelta(
					deltas,
					"edges",
					parentHealth.edgesTotal,
					latestHealth.edgesTotal,
					"neutral",
				);
				addDelta(
					deltas,
					"cycles",
					parentHealth.cycleCount,
					latestHealth.cycleCount,
					"lower_better",
				);
				addDelta(
					deltas,
					"violations",
					parentHealth.violationCount,
					latestHealth.violationCount,
					"lower_better",
				);
				addDelta(
					deltas,
					"avg_complexity",
					parentHealth.avgComplexity,
					latestHealth.avgComplexity,
					"lower_better",
				);
				addDelta(
					deltas,
					"max_complexity",
					parentHealth.maxComplexity,
					latestHealth.maxComplexity,
					"lower_better",
				);
				addDelta(
					deltas,
					"functions_measured",
					parentHealth.functionCount,
					latestHealth.functionCount,
					"neutral",
				);
				addDeltaNullable(
					deltas,
					"avg_coverage",
					parentHealth.avgCoverage,
					latestHealth.avgCoverage,
					"higher_better",
				);
				addDeltaNullable(
					deltas,
					"hotspot_files",
					parentHealth.hotspotFiles,
					latestHealth.hotspotFiles,
					"lower_better",
				);
			}

			if (opts.json) {
				console.log(
					JSON.stringify(
						{
							command: "trend",
							repo: repo.name,
							from_snapshot: parent.snapshotUid,
							to_snapshot: latest.snapshotUid,
							from_created: parent.createdAt,
							to_created: latest.createdAt,
							comparable,
							incompatibility_reasons:
								incompatReasons.length > 0 ? incompatReasons : undefined,
							deltas: comparable ? deltas : undefined,
							notes: comparable
								? [
										"violations: evaluated using current boundary declarations, not historical",
										"hotspot_files: null = never computed, 0 = computed with zero results (assessment-run marker based)",
									]
								: undefined,
						},
						null,
						2,
					),
				);
			} else {
				console.log(`Trend: ${repo.name}`);
				console.log(`From: ${parent.snapshotUid} (${parent.createdAt})`);
				console.log(`To:   ${latest.snapshotUid} (${latest.createdAt})`);
				console.log("");

				if (!comparable) {
					console.log("NOT COMPARABLE:");
					for (const reason of incompatReasons) {
						console.log(`  - ${reason}`);
					}
					console.log("");
					console.log(
						"Snapshots were produced with different toolchain semantics.",
					);
					console.log(
						"Re-index with the current toolchain to create comparable snapshots.",
					);
					return;
				}

				console.log(
					"METRIC                  FROM        TO     DELTA  DIRECTION",
				);
				for (const d of deltas) {
					const metric = d.metric.padEnd(24);
					const from = d.from !== null ? String(d.from).padStart(5) : "    -";
					const to = d.to !== null ? String(d.to).padStart(10) : "         -";
					const delta =
						d.delta !== null
							? (d.delta >= 0 ? `+${d.delta}` : String(d.delta)).padStart(10)
							: "         -";
					const dir = d.direction.padStart(10);
					console.log(`${metric}${from}${to}${delta}${dir}`);
				}

				const improved = deltas.filter(
					(d) => d.direction === "improved",
				).length;
				const worsened = deltas.filter(
					(d) => d.direction === "worsened",
				).length;
				console.log("");
				console.log(
					`${improved} improved, ${worsened} worsened, ${deltas.length - improved - worsened} unchanged/neutral`,
				);
				console.log(
					"Note: violations are evaluated using current boundary declarations against historical code.",
				);
			}
		});
}

// ── Health computation ────────────────────────────────────────────────

function computeHealth(
	ctx: AppContext,
	snapshotUid: string,
	repoUid: string,
): SnapshotHealth {
	const snap = ctx.storage.getSnapshot(snapshotUid);

	// Cycles
	const cycles = ctx.storage.findCycles({ snapshotUid, level: "module" });

	// Violations
	const boundaries = ctx.storage.getActiveDeclarations({
		repoUid,
		kind: DeclarationKind.BOUNDARY,
	});
	let violationCount = 0;
	for (const decl of boundaries) {
		const stableKey = decl.targetStableKey;
		const moduleMatch = stableKey.match(/^[^:]+:(.+):MODULE$/);
		if (!moduleMatch) continue;
		const modulePath = moduleMatch[1];
		const value = JSON.parse(decl.valueJson) as { forbids: string };
		const violations = ctx.storage.findImportsBetweenPaths({
			snapshotUid,
			sourcePrefix: modulePath,
			targetPrefix: value.forbids,
		});
		violationCount += violations.length;
	}

	// Complexity
	const ccRows = ctx.storage.queryMeasurementsByKind(
		snapshotUid,
		"cyclomatic_complexity",
	);
	const ccValues = ccRows.map(
		(r) => (JSON.parse(r.valueJson) as { value: number }).value,
	);
	const functionCount = ccValues.length;
	const avgComplexity =
		functionCount > 0
			? Math.round((ccValues.reduce((a, b) => a + b, 0) / functionCount) * 10) /
				10
			: 0;
	const maxComplexity = functionCount > 0 ? Math.max(...ccValues) : 0;

	// Coverage
	const covRows = ctx.storage.queryMeasurementsByKind(
		snapshotUid,
		"line_coverage",
	);
	const covValues = covRows.map(
		(r) => (JSON.parse(r.valueJson) as { value: number }).value,
	);
	const avgCoverage =
		covValues.length > 0
			? Math.round(
					(covValues.reduce((a, b) => a + b, 0) / covValues.length) * 10000,
				) / 10000
			: null;

	// Hotspots — use assessment-run marker to distinguish "never computed"
	// from "computed with zero results." The marker is persisted by
	// `graph hotspots` when it runs.
	const hotspots = ctx.storage.queryInferences(snapshotUid, "hotspot_score");
	const hotspotRunMarker = ctx.storage.queryInferences(
		snapshotUid,
		"assessment_run:hotspot_score",
	);
	const hotspotFiles = hotspotRunMarker.length > 0 ? hotspots.length : null;

	return {
		snapshotUid,
		createdAt: snap?.createdAt ?? "",
		filesTotal: snap?.filesTotal ?? 0,
		nodesTotal: snap?.nodesTotal ?? 0,
		edgesTotal: snap?.edgesTotal ?? 0,
		cycleCount: cycles.length,
		violationCount,
		avgComplexity,
		maxComplexity,
		functionCount,
		avgCoverage,
		coverageFiles: covValues.length,
		hotspotFiles,
	};
}

// ── Delta helpers ─────────────────────────────────────────────────────

function addDelta(
	deltas: MetricDelta[],
	metric: string,
	from: number,
	to: number,
	preference: "lower_better" | "higher_better" | "neutral",
): void {
	const delta = to - from;
	let direction: MetricDelta["direction"] = "unchanged";
	if (delta !== 0) {
		if (preference === "neutral") {
			direction = delta > 0 ? "improved" : "improved"; // neutral = no judgment
			direction = "unchanged"; // treat size changes as neutral
			if (delta !== 0) direction = delta > 0 ? "new" : "no_data";
		} else if (preference === "lower_better") {
			direction = delta < 0 ? "improved" : "worsened";
		} else {
			direction = delta > 0 ? "improved" : "worsened";
		}
	}
	// For neutral metrics, just report the delta without judgment
	if (preference === "neutral" && delta !== 0) {
		direction = "unchanged";
	}
	deltas.push({ metric, from, to, delta, direction });
}

function addDeltaNullable(
	deltas: MetricDelta[],
	metric: string,
	from: number | null,
	to: number | null,
	preference: "lower_better" | "higher_better",
): void {
	if (from === null && to === null) {
		deltas.push({
			metric,
			from: null,
			to: null,
			delta: null,
			direction: "no_data",
		});
		return;
	}
	if (from === null) {
		deltas.push({ metric, from: null, to, delta: null, direction: "new" });
		return;
	}
	if (to === null) {
		deltas.push({ metric, from, to: null, delta: null, direction: "no_data" });
		return;
	}
	const delta = Math.round((to - from) * 10000) / 10000;
	let direction: MetricDelta["direction"] = "unchanged";
	if (delta !== 0) {
		direction =
			preference === "higher_better"
				? delta > 0
					? "improved"
					: "worsened"
				: delta < 0
					? "improved"
					: "worsened";
	}
	deltas.push({ metric, from, to, delta, direction });
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
