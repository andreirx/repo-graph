/**
 * tsconfig.json alias reader (adapter).
 *
 * Reads `compilerOptions.paths` from a tsconfig.json file and
 * produces a `TsconfigAliases` DTO for the classifier.
 *
 * Supports:
 *   - reading any tsconfig.json by absolute path
 *   - following `extends` chains (relative paths only in first slice)
 *   - TypeScript's merge rule: child `paths` replaces parent `paths`
 *     entirely (first `paths` found in the chain wins)
 *
 * JSONC handling:
 *   tsconfig.json commonly includes comments. This reader strips
 *   comments via a conservative regex pass BEFORE `JSON.parse`:
 *     - block comments
 *     - line comments
 *   Known limitation: the regex does not distinguish comment syntax
 *   inside string values. For tsconfig files this is rarely an issue.
 *
 * Extends limitations (first slice):
 *   - only relative `extends` paths are followed
 *   - package-name extends (e.g. "@tsconfig/node18") are NOT resolved
 *     (would require node_modules lookup)
 *   - circular extends chains are detected and broken (max 10 hops)
 *
 * Failure modes (all yield null or empty, not exceptions):
 *   - file does not exist
 *   - file is unreadable
 *   - content cannot be parsed (even after comment stripping)
 *   - extends target cannot be resolved or read
 */

import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import type {
	TsconfigAliasEntry,
	TsconfigAliases,
} from "../../core/classification/signals.js";

interface ParsedTsconfig {
	extends?: string;
	compilerOptions?: {
		paths?: Record<string, unknown>;
	};
}

const EMPTY_ALIASES: TsconfigAliases = { entries: Object.freeze([]) };
const MAX_EXTENDS_DEPTH = 10;

/**
 * Strip `//` line comments and `/* ... *​/` block comments from a
 * JSONC string. Conservative regex pass.
 */
function stripJsonComments(source: string): string {
	let result = source.replace(/\/\*[\s\S]*?\*\//g, "");
	result = result.replace(/\/\/.*$/gm, "");
	return result;
}

/**
 * Parse a tsconfig.json file at the given absolute path.
 * Returns the parsed object, or null on any failure.
 */
async function parseTsconfigFile(
	absolutePath: string,
): Promise<ParsedTsconfig | null> {
	let raw: string;
	try {
		raw = await readFile(absolutePath, "utf-8");
	} catch {
		return null;
	}
	const stripped = stripJsonComments(raw);
	try {
		return JSON.parse(stripped) as ParsedTsconfig;
	} catch {
		return null;
	}
}

/**
 * Extract TsconfigAliases from a parsed tsconfig's
 * compilerOptions.paths. Returns null if no paths key exists
 * (distinct from empty paths, which returns an empty entries array).
 */
function extractPathsFromParsed(
	parsed: ParsedTsconfig,
): TsconfigAliases | null {
	const paths = parsed.compilerOptions?.paths;
	if (!paths || typeof paths !== "object" || Array.isArray(paths)) {
		return null;
	}
	const entries: TsconfigAliasEntry[] = [];
	for (const [pattern, substitutions] of Object.entries(paths)) {
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

/**
 * Read tsconfig.json at the given absolute path, following `extends`
 * chains to find the effective `compilerOptions.paths`.
 *
 * TypeScript merge rule: child's `paths` object REPLACES parent's
 * entirely. So the first `paths` found in the extends chain wins.
 *
 * Returns TsconfigAliases (possibly empty), or null if the file
 * does not exist or cannot be parsed.
 */
export async function readTsconfigAliasesFromPath(
	absolutePath: string,
): Promise<TsconfigAliases | null> {
	const visited = new Set<string>();
	let currentPath = absolutePath;

	for (let depth = 0; depth < MAX_EXTENDS_DEPTH; depth++) {
		const resolved = resolve(currentPath);
		if (visited.has(resolved)) break; // circular
		visited.add(resolved);

		const parsed = await parseTsconfigFile(resolved);
		if (!parsed) {
			// If the initial file fails, return null.
			// If an extends target fails, stop chain and return empty.
			return depth === 0 ? null : EMPTY_ALIASES;
		}

		// If this config has paths, that's the effective result
		// (child paths replace parent paths in TypeScript).
		const aliases = extractPathsFromParsed(parsed);
		if (aliases !== null) return aliases;

		// No paths here — follow extends if present.
		if (!parsed.extends || typeof parsed.extends !== "string") {
			return EMPTY_ALIASES;
		}

		// Only follow relative extends paths. Package-name extends
		// (e.g. "@tsconfig/node18") require node_modules resolution
		// which is deferred.
		const ext = parsed.extends;
		if (!ext.startsWith(".") && !ext.startsWith("/")) {
			return EMPTY_ALIASES;
		}

		currentPath = join(dirname(resolved), ext);
		// Append .json if not already present (TypeScript convention).
		if (!currentPath.endsWith(".json")) {
			currentPath += ".json";
		}
	}

	return EMPTY_ALIASES;
}

/**
 * Convenience: read tsconfig.json at `<dirPath>/tsconfig.json`.
 * Used by the indexer's per-directory walk.
 */
export async function readTsconfigAliases(
	dirPath: string,
): Promise<TsconfigAliases | null> {
	return readTsconfigAliasesFromPath(join(dirPath, "tsconfig.json"));
}
