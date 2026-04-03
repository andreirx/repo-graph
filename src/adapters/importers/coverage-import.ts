/**
 * Coverage importer for Istanbul/c8 JSON format.
 *
 * Reads a coverage-final.json or coverage-summary.json file
 * produced by Istanbul, c8, or nyc. Extracts per-file coverage
 * percentages and returns them as measurement-ready data.
 *
 * Evidence model:
 *   Raw coverage JSON → artifact record (kind = "coverage_report")
 *   Per-file percentages → measurements (line_coverage, function_coverage, branch_coverage)
 *   "Under-tested hotspot" → assessment (future, not this module)
 *
 * See docs/architecture/measurement-model.txt.
 */

import { readFile } from "node:fs/promises";
import { relative, resolve } from "node:path";

export interface FileCoverage {
	/** Repo-relative file path (forward slashes). */
	filePath: string;
	/** Lines covered / total lines. 0.0 to 1.0. null if not available. */
	lineCoverage: number | null;
	/** Functions covered / total functions. 0.0 to 1.0. null if not available. */
	functionCoverage: number | null;
	/** Branches covered / total branches. 0.0 to 1.0. null if not available. */
	branchCoverage: number | null;
	/** Total lines in coverage data. */
	totalLines: number;
	/** Total functions in coverage data. */
	totalFunctions: number;
	/** Total branches in coverage data. */
	totalBranches: number;
}

export interface CoverageImportResult {
	/** Per-file coverage data. */
	files: FileCoverage[];
	/** Absolute path to the raw report file (for artifact record). */
	reportPath: string;
}

/**
 * Import coverage from an Istanbul/c8 JSON report.
 *
 * @param reportPath - Absolute path to coverage-final.json or similar.
 * @param repoRoot - Absolute path to the repository root (for path rebasing).
 * @returns Parsed per-file coverage data with repo-relative paths.
 */
export async function importCoverageReport(
	reportPath: string,
	repoRoot: string,
): Promise<CoverageImportResult> {
	const content = await readFile(reportPath, "utf-8");
	const raw = JSON.parse(content) as Record<string, unknown>;

	const files: FileCoverage[] = [];
	const absRepoRoot = resolve(repoRoot);

	for (const [absPath, entry] of Object.entries(raw)) {
		// Skip non-object entries (e.g. summary keys)
		if (typeof entry !== "object" || entry === null) continue;

		const fileEntry = entry as Record<string, unknown>;

		// Istanbul format: each file entry has s (statements), f (functions), b (branches)
		// as count maps, plus statementMap/fnMap/branchMap for locations.
		const s = fileEntry.s as Record<string, number> | undefined;
		const f = fileEntry.f as Record<string, number> | undefined;
		const b = fileEntry.b as Record<string, number[]> | undefined;

		// Skip entries that don't look like Istanbul file coverage
		if (!s && !f) continue;

		// Rebase absolute path to repo-relative
		let filePath: string;
		try {
			filePath = relative(absRepoRoot, absPath).replace(/\\/g, "/");
		} catch {
			continue;
		}
		// Skip files outside the repo root
		if (filePath.startsWith("..") || filePath.startsWith("/")) continue;

		// Compute coverage ratios
		const stmtValues = s ? Object.values(s) : [];
		const funcValues = f ? Object.values(f) : [];
		const branchValues = b ? Object.values(b).flatMap((arr) => arr) : [];

		const totalLines = stmtValues.length;
		const coveredLines = stmtValues.filter((v) => v > 0).length;
		const totalFunctions = funcValues.length;
		const coveredFunctions = funcValues.filter((v) => v > 0).length;
		const totalBranches = branchValues.length;
		const coveredBranches = branchValues.filter((v) => v > 0).length;

		files.push({
			filePath,
			lineCoverage: totalLines > 0 ? coveredLines / totalLines : null,
			functionCoverage:
				totalFunctions > 0 ? coveredFunctions / totalFunctions : null,
			branchCoverage:
				totalBranches > 0 ? coveredBranches / totalBranches : null,
			totalLines,
			totalFunctions,
			totalBranches,
		});
	}

	return { files, reportPath: resolve(reportPath) };
}
