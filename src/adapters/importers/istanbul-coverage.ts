/**
 * Istanbul/c8 coverage importer.
 *
 * Reads coverage-final.json produced by Istanbul, c8, or nyc.
 * Implements the CoverageImporterPort interface.
 */

import { readFile } from "node:fs/promises";
import { relative, resolve } from "node:path";
import type {
	CoverageImporterPort,
	CoverageImportResult,
	FileCoverage,
} from "../../core/ports/coverage-importer.js";

export class IstanbulCoverageImporter implements CoverageImporterPort {
	readonly formatName = "istanbul";

	canHandle(reportPath: string, content: string): boolean {
		// Istanbul coverage-final.json is a JSON object keyed by absolute file paths
		// where each value has s, f, b maps.
		if (!reportPath.endsWith(".json")) return false;
		try {
			const parsed = JSON.parse(content);
			if (
				typeof parsed !== "object" ||
				parsed === null ||
				Array.isArray(parsed)
			)
				return false;
			// Check first entry for Istanbul shape
			const firstKey = Object.keys(parsed)[0];
			if (!firstKey) return false;
			const entry = parsed[firstKey];
			return (
				typeof entry === "object" &&
				entry !== null &&
				("s" in entry || "f" in entry)
			);
		} catch {
			return false;
		}
	}

	async importReport(
		reportPath: string,
		repoRoot: string,
	): Promise<CoverageImportResult> {
		const content = await readFile(reportPath, "utf-8");
		const raw = JSON.parse(content) as Record<string, unknown>;
		const absRepoRoot = resolve(repoRoot);
		const files: FileCoverage[] = [];

		for (const [absPath, entry] of Object.entries(raw)) {
			if (typeof entry !== "object" || entry === null) continue;
			const fileEntry = entry as Record<string, unknown>;
			const s = fileEntry.s as Record<string, number> | undefined;
			const f = fileEntry.f as Record<string, number> | undefined;
			const b = fileEntry.b as Record<string, number[]> | undefined;
			if (!s && !f) continue;

			let filePath: string;
			try {
				filePath = relative(absRepoRoot, absPath).replace(/\\/g, "/");
			} catch {
				continue;
			}
			if (filePath.startsWith("..") || filePath.startsWith("/")) continue;

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

		return { files, reportPath: resolve(reportPath), format: "istanbul" };
	}
}
