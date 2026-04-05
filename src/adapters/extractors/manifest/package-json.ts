/**
 * Manifest extractor for package.json.
 *
 * Deterministic extraction of package name and version from a
 * package.json file. These are domain versions — what the system
 * claims to be — distinct from repo-graph's internal provenance.
 *
 * See docs/architecture/measurement-model.txt "EXTRACTED DOMAIN VERSIONS".
 *
 * This file also provides `extractPackageDependencies`, a separate
 * function that produces a classifier-signal DTO from the same
 * content. The two functions share no state by design — manifest
 * extraction is a domain-version concern, dependency extraction is
 * a classification-signal concern.
 */

import type { PackageDependencySet } from "../../../core/classification/signals.js";

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

/**
 * Dependency-names signal set for the unresolved-edge classifier.
 *
 * Produces the union of keys from `dependencies`, `devDependencies`,
 * `peerDependencies`, and `optionalDependencies`, deduplicated and
 * sorted for determinism.
 *
 * Returns null on JSON parse failure. Returns an empty set if the
 * file is valid JSON but declares no dependency fields.
 *
 * This function is intentionally distinct from
 * `extractPackageManifest`: the two have different consumers and
 * different semantic shapes. The cost of re-parsing the same
 * content string is negligible at package.json scale.
 */
export function extractPackageDependencies(
	content: string,
): PackageDependencySet | null {
	let parsed: Record<string, unknown>;
	try {
		parsed = JSON.parse(content) as Record<string, unknown>;
	} catch {
		return null;
	}

	const names = new Set<string>();
	const fields = [
		"dependencies",
		"devDependencies",
		"peerDependencies",
		"optionalDependencies",
	];
	for (const field of fields) {
		const obj = parsed[field];
		if (obj && typeof obj === "object" && !Array.isArray(obj)) {
			for (const key of Object.keys(obj as Record<string, unknown>)) {
				names.add(key);
			}
		}
	}
	return { names: Object.freeze([...names].sort()) };
}
