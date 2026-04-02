/**
 * Human-readable table output formatter.
 * Default output mode when --json is not specified.
 */

import type {
	BoundaryViolation,
	CycleResult,
	DeadNodeResult,
	NodeResult,
	PathResult,
	Repo,
	Snapshot,
} from "../../core/model/index.js";
import type { IndexResult } from "../../core/ports/indexer.js";

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

	// Show breakdown if there are unresolved edges
	const entries = Object.entries(result.unresolvedBreakdown);
	if (entries.length > 0) {
		lines.push("  Unresolved breakdown:");
		for (const [category, count] of entries.sort((a, b) => b[1] - a[1])) {
			lines.push(`    ${String(count).padStart(5)}  ${category}`);
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
