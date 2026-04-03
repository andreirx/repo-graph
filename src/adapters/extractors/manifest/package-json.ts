/**
 * Manifest extractor for package.json.
 *
 * Deterministic extraction of package name and version from a
 * package.json file. These are domain versions — what the system
 * claims to be — distinct from repo-graph's internal provenance.
 *
 * See docs/architecture/measurement-model.txt "EXTRACTED DOMAIN VERSIONS".
 */

export interface PackageManifest {
	/** Package name from package.json "name" field. */
	packageName: string | null;
	/** Package version from package.json "version" field. */
	packageVersion: string | null;
	/** Repo-relative path to the manifest file. */
	sourcePath: string;
}

/**
 * Extract package name and version from a package.json content string.
 *
 * @param content - Raw JSON content of package.json.
 * @param relativePath - Repo-relative path (e.g. "package.json" or "packages/foo/package.json").
 * @returns Extracted manifest data, or null if the content is not valid JSON.
 */
export function extractPackageManifest(
	content: string,
	relativePath: string,
): PackageManifest | null {
	try {
		const parsed = JSON.parse(content) as Record<string, unknown>;
		return {
			packageName: typeof parsed.name === "string" ? parsed.name : null,
			packageVersion:
				typeof parsed.version === "string" ? parsed.version : null,
			sourcePath: relativePath,
		};
	} catch {
		return null;
	}
}
