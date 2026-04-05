/**
 * README extractor.
 *
 * Reads a README.md or README.txt file and returns an annotation
 * candidate. Pure function over raw file content — caller
 * (indexer) handles file I/O, README preference selection
 * (preferReadmeFile from attribution.ts), and target attribution.
 *
 * The entire README content is stored as the annotation body.
 * Content normalization (truncation, empty-drop) is applied by
 * the indexer via attribution.ts helpers.
 */

/**
 * Candidate produced by the README extractor. Caller builds the
 * full Annotation record by adding target attribution and metadata.
 */
export interface ReadmeCandidate {
	/** The README content verbatim (pre-normalization). */
	content: string;
	/** Repo-relative path to the README file. */
	sourceFile: string;
	/** Always 1 (README is the whole file). */
	sourceLineStart: number;
	/** Last line of the file (line count). */
	sourceLineEnd: number;
	/** "markdown" for .md, "text" for .txt. */
	language: "markdown" | "text";
}

/**
 * Extract the entire content of a README file as a candidate.
 * Returns null if content is empty (trimmed).
 *
 * The caller is responsible for:
 *   - choosing README.md over README.txt when both exist
 *     (use preferReadmeFile from attribution.ts)
 *   - attributing the result to the correct target
 *     (use attributeReadme from attribution.ts)
 */
export function extractReadme(
	sourceFile: string,
	rawSource: string,
): ReadmeCandidate | null {
	if (rawSource.trim().length === 0) return null;
	const lowerPath = sourceFile.toLowerCase();
	const language: "markdown" | "text" = lowerPath.endsWith(".md")
		? "markdown"
		: "text";
	// Count lines (including the last line even if no trailing newline)
	const lineCount = rawSource.split("\n").length;
	return {
		content: rawSource,
		sourceFile,
		sourceLineStart: 1,
		sourceLineEnd: lineCount,
		language,
	};
}
