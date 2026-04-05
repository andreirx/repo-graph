/**
 * tsconfig.json alias reader (adapter).
 *
 * Reads `compilerOptions.paths` from a repo's root tsconfig.json and
 * produces a `TsconfigAliases` DTO for the classifier.
 *
 * Scope (first slice):
 *   - reads `<repoRoot>/tsconfig.json` ONLY
 *   - does NOT follow `extends`
 *   - does NOT look at nested package tsconfigs in monorepos
 *
 * JSONC handling:
 *   tsconfig.json commonly includes comments. This reader strips
 *   comments via a conservative regex pass BEFORE `JSON.parse`:
 *     - block comments /* ... *​/
 *     - line comments   // ...
 *   Known limitation: the regex does not distinguish comment
 *   syntax appearing inside string values. For tsconfig-style
 *   config files this is very rarely an issue (paths are
 *   filesystem strings). If parsing still fails after stripping,
 *   the reader returns `null` rather than attempting recovery.
 *
 * Failure modes (all yield null, not an exception):
 *   - file does not exist
 *   - file is unreadable
 *   - content cannot be parsed as JSON (even after stripping)
 *   - `compilerOptions` or `paths` missing or not objects
 *
 * Returning null is the signal for "no alias data available"; the
 * classifier treats it as an empty alias set.
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type {
	TsconfigAliasEntry,
	TsconfigAliases,
} from "../../core/classification/signals.js";

interface ParsedTsconfig {
	compilerOptions?: {
		paths?: Record<string, unknown>;
	};
}

/**
 * Strip `//` line comments and `/* ... *​/` block comments from a
 * JSONC string. Conservative regex pass — does NOT detect comments
 * that appear inside string values. Documented limitation.
 */
function stripJsonComments(source: string): string {
	// Remove block comments (non-greedy, handles multi-line).
	let result = source.replace(/\/\*[\s\S]*?\*\//g, "");
	// Remove line comments up to end of line.
	result = result.replace(/\/\/.*$/gm, "");
	return result;
}

/**
 * Read and parse tsconfig.json at `<repoRootPath>/tsconfig.json`.
 * Returns a TsconfigAliases DTO, or null on any failure.
 */
export async function readTsconfigAliases(
	repoRootPath: string,
): Promise<TsconfigAliases | null> {
	const tsconfigPath = join(repoRootPath, "tsconfig.json");
	let raw: string;
	try {
		raw = await readFile(tsconfigPath, "utf-8");
	} catch {
		return null;
	}

	const stripped = stripJsonComments(raw);
	let parsed: ParsedTsconfig;
	try {
		parsed = JSON.parse(stripped) as ParsedTsconfig;
	} catch {
		return null;
	}

	const paths = parsed.compilerOptions?.paths;
	if (!paths || typeof paths !== "object" || Array.isArray(paths)) {
		return { entries: Object.freeze([]) };
	}

	const entries: TsconfigAliasEntry[] = [];
	for (const [pattern, substitutions] of Object.entries(paths)) {
		// Substitutions must be an array of strings. If malformed,
		// preserve the pattern with an empty substitution list rather
		// than dropping the entry — the classifier can still use the
		// pattern for alias matching.
		const subs: string[] = [];
		if (Array.isArray(substitutions)) {
			for (const s of substitutions) {
				if (typeof s === "string") subs.push(s);
			}
		}
		entries.push({ pattern, substitutions: subs });
	}

	return { entries: Object.freeze(entries) };
}
