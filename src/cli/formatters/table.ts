/**
 * Human-readable table output formatter.
 * Default output mode when --json is not specified.
 */

import { humanLabelForCategory } from "../../core/diagnostics/unresolved-edge-categories.js";
import type {
	BoundaryViolation,
	CycleResult,
	DeadNodeResult,
	ModuleStats,
	NodeResult,
	PathResult,
	Repo,
	Snapshot,
} from "../../core/model/index.js";
import type { IndexResult } from "../../core/ports/indexer.js";
import type {
	FunctionMetricRow,
	ModuleMetricAggregate,
} from "../../core/ports/storage.js";

export function formatRepoTable(repos: Repo[]): string {
	if (repos.length === 0) return "No repositories registered.";
	const lines = ["NAME            PATH"];
	for (const r of repos) {
		lines.push(`${r.name.padEnd(16)}${r.rootPath}`);
	}
	return lines.join("\n");
}

export function formatRepoStatus(
	repo: Repo,
	snapshot: Snapshot | null,
): string {
	const lines = [
		`Repository: ${repo.name}`,
		`Path:       ${repo.rootPath}`,
		`UID:        ${repo.repoUid}`,
	];
	if (snapshot) {
		lines.push(
			`Snapshot:   ${snapshot.snapshotUid}`,
			`Kind:       ${snapshot.kind}`,
			`Status:     ${snapshot.status}`,
			`Commit:     ${snapshot.basisCommit ?? "(none)"}`,
			`Files:      ${snapshot.filesTotal}`,
			`Nodes:      ${snapshot.nodesTotal}`,
			`Edges:      ${snapshot.edgesTotal}`,
			`Created:    ${snapshot.createdAt}`,
		);
	} else {
		lines.push("Snapshot:   (not indexed yet)");
	}
	return lines.join("\n");
}

export function formatIndexResult(result: IndexResult): string {
	const lines = [
		`Indexed ${result.filesTotal} files in ${result.durationMs}ms`,
		`  Nodes:      ${result.nodesTotal}`,
		`  Edges:      ${result.edgesTotal}`,
		`  Unresolved: ${result.edgesUnresolved}`,
	];

	// Show breakdown if there are unresolved edges. The stored keys are
	// machine-stable; render them as human labels for CLI display.
	const entries = Object.entries(result.unresolvedBreakdown);
	if (entries.length > 0) {
		lines.push("  Unresolved breakdown:");
		for (const [category, count] of entries.sort((a, b) => b[1] - a[1])) {
			const label = humanLabelForCategory(category);
			lines.push(`    ${String(count).padStart(5)}  ${label}`);
		}
	}

	lines.push(`  Snapshot:   ${result.snapshotUid}`);
	return lines.join("\n");
}

export function formatNodeResults(results: NodeResult[]): string {
	if (results.length === 0) return "No results.";
	const lines = [
		"SYMBOL                          FILE                            LINE  DEPTH",
	];
	for (const r of results) {
		const sym = (r.symbol ?? "").padEnd(32);
		const file = (r.file ?? "").padEnd(32);
		const line = r.line != null ? String(r.line).padStart(5) : "    -";
		const depth = String(r.depth).padStart(5);
		lines.push(`${sym}${file}${line}${depth}`);
	}
	return lines.join("\n");
}

export function formatDeadNodes(results: DeadNodeResult[]): string {
	if (results.length === 0) return "No dead nodes found.";
	const lines = [
		"SYMBOL                          FILE                            LINE  LOC",
	];
	for (const r of results) {
		const sym = (r.symbol ?? "").padEnd(32);
		const file = (r.file ?? "").padEnd(32);
		const line = r.line != null ? String(r.line).padStart(5) : "    -";
		const loc = r.lineCount != null ? String(r.lineCount).padStart(5) : "    -";
		lines.push(`${sym}${file}${line}${loc}`);
	}
	return lines.join("\n");
}

export function formatPathResult(result: PathResult): string {
	if (!result.found) return "No path found.";
	const lines = [`Path (${result.pathLength} edges):`];
	for (let i = 0; i < result.steps.length; i++) {
		const s = result.steps[i];
		const prefix = i === 0 ? "  " : `  --[${s.edgeType}]--> `;
		lines.push(`${prefix}${s.symbol} (${s.file}:${s.line ?? "?"})`);
	}
	return lines.join("\n");
}

export function formatCycles(results: CycleResult[]): string {
	if (results.length === 0) return "No cycles detected.";
	const lines = [`Found ${results.length} cycle(s):`];
	for (const c of results) {
		const names = c.nodes.map((n) => n.name).join(" -> ");
		lines.push(`  ${c.cycleId}: ${names} -> ${c.nodes[0]?.name ?? "?"}`);
	}
	return lines.join("\n");
}

export function formatModuleStats(stats: ModuleStats[]): string {
	if (stats.length === 0) return "No modules with source files found.";

	const lines = [
		"MODULE                               FAN_IN  FAN_OUT  INSTAB  ABSTR  DIST  FILES  SYMBOLS",
	];
	for (const s of stats) {
		const mod = s.path.padEnd(37);
		const fi = String(s.fanIn).padStart(6);
		const fo = String(s.fanOut).padStart(8);
		const inst = s.instability.toFixed(2).padStart(7);
		const abs = s.abstractness.toFixed(2).padStart(6);
		const dist = s.distanceFromMainSequence.toFixed(2).padStart(6);
		const files = String(s.fileCount).padStart(6);
		const syms = String(s.symbolCount).padStart(8);
		lines.push(`${mod}${fi}${fo}${inst}${abs}${dist}${files}${syms}`);
	}

	const totalModules = stats.length;
	const avgInstability =
		stats.reduce((sum, s) => sum + s.instability, 0) / totalModules;
	const avgDistance =
		stats.reduce((sum, s) => sum + s.distanceFromMainSequence, 0) /
		totalModules;
	const maxFanIn = Math.max(...stats.map((s) => s.fanIn));
	const maxFanOut = Math.max(...stats.map((s) => s.fanOut));

	lines.push("");
	lines.push(`${totalModules} modules with source files`);
	lines.push(`Avg instability: ${avgInstability.toFixed(2)}`);
	lines.push(`Avg distance from main sequence: ${avgDistance.toFixed(2)}`);
	lines.push(`Max fan-in: ${maxFanIn}  Max fan-out: ${maxFanOut}`);

	return lines.join("\n");
}

export function formatFunctionMetrics(metrics: FunctionMetricRow[]): string {
	if (metrics.length === 0)
		return "No function metrics found. Run `rgr repo index` first.";

	const lines = [
		"FUNCTION                                    FILE                            LINE   CC  PARAMS  DEPTH",
	];
	for (const m of metrics) {
		const sym = (m.symbol ?? "").padEnd(44);
		const file = (m.file ?? "").padEnd(32);
		const line = m.line != null ? String(m.line).padStart(5) : "    -";
		const cc = String(m.cyclomaticComplexity).padStart(5);
		const params = String(m.parameterCount).padStart(7);
		const depth = String(m.maxNestingDepth).padStart(6);
		lines.push(`${sym}${file}${line}${cc}${params}${depth}`);
	}

	const total = metrics.length;
	const avgCC = metrics.reduce((s, m) => s + m.cyclomaticComplexity, 0) / total;
	const maxCC = Math.max(...metrics.map((m) => m.cyclomaticComplexity));

	lines.push("");
	lines.push(`${total} functions measured`);
	lines.push(`Avg cyclomatic complexity: ${avgCC.toFixed(1)}`);
	lines.push(`Max cyclomatic complexity: ${maxCC}`);

	return lines.join("\n");
}

export function formatModuleMetricAggregates(
	aggregates: ModuleMetricAggregate[],
): string {
	if (aggregates.length === 0)
		return "No module metrics found. Run `rgr repo index` first.";

	const lines = [
		"MODULE                               FUNCS  AVG_CC  MAX_CC  AVG_NEST  MAX_NEST",
	];
	for (const a of aggregates) {
		const mod = a.modulePath.padEnd(37);
		const funcs = String(a.functionCount).padStart(5);
		const avgCC = a.avgCyclomaticComplexity.toFixed(1).padStart(7);
		const maxCC = String(a.maxCyclomaticComplexity).padStart(7);
		const avgN = a.avgNestingDepth.toFixed(1).padStart(9);
		const maxN = String(a.maxNestingDepth).padStart(9);
		lines.push(`${mod}${funcs}${avgCC}${maxCC}${avgN}${maxN}`);
	}
	return lines.join("\n");
}

export function formatViolations(violations: BoundaryViolation[]): string {
	if (violations.length === 0) return "No boundary violations found.";

	// Group by boundary rule for readability
	const groups = new Map<string, BoundaryViolation[]>();
	for (const v of violations) {
		const key = `${v.boundaryModule} --/-> ${v.forbiddenModule}`;
		const group = groups.get(key) ?? [];
		group.push(v);
		groups.set(key, group);
	}

	const lines = [
		`Found ${violations.length} violation(s) across ${groups.size} boundary rule(s):\n`,
	];
	for (const [rule, group] of groups) {
		const reason = group[0].reason ? ` (${group[0].reason})` : "";
		lines.push(`  ${rule}${reason}`);
		for (const v of group) {
			const loc = v.line != null ? `:${v.line}` : "";
			lines.push(`    ${v.sourceFile}${loc} -> ${v.targetFile}`);
		}
		lines.push("");
	}
	return lines.join("\n");
}
