/**
 * Coverage importer port.
 *
 * Abstracts coverage report parsing so that Istanbul/c8, JaCoCo,
 * coverage.py, lcov, and other formats can be added as adapters
 * without changing the measurement model or CLI.
 *
 * Each adapter reads a specific report format and normalizes it
 * into the canonical FileCoverage shape.
 */

export interface CoverageImporterPort {
	/** Human-readable name of the format this importer handles. */
	readonly formatName: string;

	/**
	 * Check if this importer can handle the given report file.
	 * Used for auto-detection: the CLI tries each registered importer
	 * until one returns true.
	 *
	 * @param reportPath - Absolute path to the report file.
	 * @param content - First portion of the file content (for sniffing).
	 */
	canHandle(reportPath: string, content: string): boolean;

	/**
	 * Import coverage data from a report file.
	 *
	 * @param reportPath - Absolute path to the report file.
	 * @param repoRoot - Absolute path to the repository root (for path rebasing).
	 * @returns Normalized per-file coverage data.
	 */
	importReport(
		reportPath: string,
		repoRoot: string,
	): Promise<CoverageImportResult>;
}

export interface CoverageImportResult {
	/** Per-file coverage data, normalized to repo-relative paths. */
	files: FileCoverage[];
	/** Absolute path to the raw report file. */
	reportPath: string;
	/** Format name that produced this data. */
	format: string;
}

export interface FileCoverage {
	/** Repo-relative file path (forward slashes). */
	filePath: string;
	/** Statements covered / total. 0.0 to 1.0. null if not available. */
	lineCoverage: number | null;
	/** Functions covered / total. 0.0 to 1.0. null if not available. */
	functionCoverage: number | null;
	/** Branches covered / total. 0.0 to 1.0. null if not available. */
	branchCoverage: number | null;
	/** Total statement count. */
	totalLines: number;
	/** Total function count. */
	totalFunctions: number;
	/** Total branch count. */
	totalBranches: number;
}
