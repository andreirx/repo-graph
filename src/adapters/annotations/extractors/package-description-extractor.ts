/**
 * package.json description extractor.
 *
 * Reads `description` field from package.json and returns an
 * annotation candidate. Pure function over raw file content —
 * caller (indexer) handles file I/O and target attribution.
 *
 * Contract: docs/architecture/annotations-contract.txt.
 * Attribution target is determined by the caller based on
 * whether the package.json is at repo root or a sub-directory
 * (see attributePackageDescription() in attribution.ts).
 *
 * Line-number accuracy: JSON does not carry native line-number
 * metadata from JSON.parse. This extractor locates the
 * `"description"` key via regex scan of the raw source and
 * reports the matching line. If the key appears more than once
 * (invalid JSON, but graceful handling) the first occurrence
 * wins.
 */

/**
 * Candidate produced by the package.json description extractor.
 * The caller attributes this to a target and builds the full
 * Annotation record.
 */
export interface PackageDescriptionCandidate {
	/** The raw description string from package.json. */
	content: string;
	/** Repo-relative path to the package.json that was read. */
	sourceFile: string;
	/** 1-indexed line number where the `"description"` key appears. */
	sourceLineStart: number;
	/** Same as sourceLineStart (description is typically one line). */
	sourceLineEnd: number;
}

/**
 * Extract the description field from package.json raw source.
 * Returns null if the file cannot be parsed, description is
 * absent, or description is not a non-empty string.
 */
export function extractPackageDescription(
	sourceFile: string,
	rawSource: string,
): PackageDescriptionCandidate | null {
	let parsed: unknown;
	try {
		parsed = JSON.parse(rawSource);
	} catch {
		return null;
	}
	if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
		return null;
	}
	const obj = parsed as Record<string, unknown>;
	const description = obj.description;
	if (typeof description !== "string" || description.trim().length === 0) {
		return null;
	}

	// Find the line number where the "description" key appears.
	// Regex-based scan is adequate for package.json (well-formed JSON
	// does not contain nested "description" keys at arbitrary depths
	// in practice). Use 1-indexed line numbers.
	const lineStart = findDescriptionLine(rawSource);

	return {
		content: description,
		sourceFile,
		sourceLineStart: lineStart,
		sourceLineEnd: lineStart,
	};
}

function findDescriptionLine(rawSource: string): number {
	const lines = rawSource.split("\n");
	// Match `"description"` as a JSON key (followed by whitespace and colon)
	const keyPattern = /"description"\s*:/;
	for (let i = 0; i < lines.length; i++) {
		if (keyPattern.test(lines[i])) {
			return i + 1;
		}
	}
	// Fallback if regex fails (shouldn't happen since we already parsed)
	return 1;
}
