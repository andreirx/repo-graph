/**
 * Python dependency reader (adapter).
 *
 * Reads dependency names from Python package manifests and produces
 * a PackageDependencySet DTO for the classifier. Analogous to the
 * package.json reader for TS and Cargo.toml reader for Rust.
 *
 * Supported formats:
 *   1. pyproject.toml — [project].dependencies (PEP 621)
 *   2. requirements.txt — one package per line
 *
 * Priority: pyproject.toml first (authoritative), requirements.txt
 * as fallback.
 *
 * Package name normalization:
 *   PEP 503 normalizes package names: lowercase, replace [-_.] with -.
 *   However, Python import specifiers often differ from package names:
 *     - beautifulsoup4 → import bs4
 *     - PyYAML → import yaml
 *     - Pillow → import PIL
 *
 *   This first slice stores the declared package names as-is
 *   (lowercased). The classifier matches import specifiers against
 *   these names. Exact matches work (e.g., "chromadb" matches
 *   "import chromadb"). Mismatches (bs4 vs beautifulsoup4) remain
 *   unclassified. A curated alias map may be added later.
 *
 * TOML parsing:
 *   Uses conservative line-based parsing, not a full TOML parser.
 *   Sufficient for standard pyproject.toml layouts. Does not handle:
 *     - Multi-line array continuations with complex quoting
 *     - Dynamic dependencies via setuptools plugins
 *     - Poetry [tool.poetry.dependencies] format (different schema)
 *
 * Failure modes: returns null if no manifest found, empty set if
 * manifest has no dependencies. Does not throw.
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { PackageDependencySet } from "../../core/classification/signals.js";

/**
 * Read Python dependency names from the nearest manifest in a directory.
 * Tries pyproject.toml first, then requirements.txt.
 * Returns null if neither exists.
 */
export async function readPythonDependencies(
	dirPath: string,
): Promise<PackageDependencySet | null> {
	// Try pyproject.toml first (authoritative).
	const pyprojectResult = await readPyprojectDeps(dirPath);
	if (pyprojectResult !== null) return pyprojectResult;

	// Fallback: requirements.txt.
	const reqResult = await readRequirementsTxt(dirPath);
	if (reqResult !== null) return reqResult;

	return null;
}

// ── pyproject.toml reader ───────────────────────────────────────────

async function readPyprojectDeps(
	dirPath: string,
): Promise<PackageDependencySet | null> {
	const filePath = join(dirPath, "pyproject.toml");
	let content: string;
	try {
		content = await readFile(filePath, "utf-8");
	} catch {
		return null;
	}

	return parsePyprojectDependencies(content);
}

/**
 * Parse dependency names from pyproject.toml content.
 *
 * Looks for `dependencies = [...]` under `[project]` section.
 * Each entry is a PEP 508 specifier: "package>=version" or "package".
 * Extracts only the package name (before any version/extra specifiers).
 */
export function parsePyprojectDependencies(
	content: string,
): PackageDependencySet {
	const names: string[] = [];
	const lines = content.split("\n");

	let inProjectSection = false;
	let inDepsArray = false;
	let inOptionalDeps = false;

	for (const line of lines) {
		const trimmed = line.trim();

		// Track section headers.
		if (trimmed.startsWith("[")) {
			inProjectSection = trimmed === "[project]";
			inOptionalDeps = trimmed.startsWith("[project.optional-dependencies");
			if (!inProjectSection && !inOptionalDeps) {
				inDepsArray = false;
			}
			continue;
		}

		// Detect start of dependencies array.
		if (inProjectSection && trimmed.startsWith("dependencies")) {
			const eqIdx = trimmed.indexOf("=");
			if (eqIdx >= 0) {
				const afterEq = trimmed.slice(eqIdx + 1).trim();
				if (afterEq.startsWith("[")) {
					// Inline array or start of multi-line array.
					inDepsArray = true;
					// Extract any deps on this same line.
					extractDepsFromLine(afterEq, names);
					if (afterEq.includes("]")) {
						inDepsArray = false;
					}
					continue;
				}
			}
		}

		// Detect optional dependency arrays (dev = [...]).
		if (inOptionalDeps && trimmed.includes("=")) {
			const afterEq = trimmed.slice(trimmed.indexOf("=") + 1).trim();
			if (afterEq.startsWith("[")) {
				extractDepsFromLine(afterEq, names);
				// If the array is not closed on this line, keep reading.
				if (!afterEq.includes("]")) {
					inDepsArray = true;
				}
				continue;
			}
		}

		// Inside a multi-line array.
		if (inDepsArray) {
			extractDepsFromLine(trimmed, names);
			if (trimmed.includes("]")) {
				inDepsArray = false;
			}
		}
	}

	return { names: Object.freeze(names.map((n) => n.toLowerCase())) };
}

/**
 * Extract package names from a TOML array line.
 * Handles: `"chromadb>=0.4.0",`, `"pyyaml>=6.0"`, `"pytest>=7.0"`
 */
function extractDepsFromLine(line: string, names: string[]): void {
	// Match quoted strings within the line.
	const matches = line.matchAll(/["']([^"']+)["']/g);
	for (const m of matches) {
		const spec = m[1].trim();
		// Extract package name from PEP 508 specifier.
		// "chromadb>=0.4.0" → "chromadb"
		// "requests[security]>=2.0" → "requests"
		const name = extractPackageName(spec);
		if (name) names.push(name);
	}
}

/**
 * Extract the package name from a PEP 508 dependency specifier.
 * Strips version constraints, extras, environment markers.
 *
 * "chromadb>=0.4.0" → "chromadb"
 * "requests[security]>=2.0" → "requests"
 * "pyyaml" → "pyyaml"
 * "package ; python_version >= '3.8'" → "package"
 */
function extractPackageName(spec: string): string | null {
	// PEP 508: name followed by optional extras, version, markers.
	// Name chars: letters, digits, -, _, .
	const match = spec.match(/^([A-Za-z0-9][\w.\-]*)/);
	return match ? match[1] : null;
}

// ── requirements.txt reader ─────────────────────────────────────────

async function readRequirementsTxt(
	dirPath: string,
): Promise<PackageDependencySet | null> {
	const filePath = join(dirPath, "requirements.txt");
	let content: string;
	try {
		content = await readFile(filePath, "utf-8");
	} catch {
		return null;
	}

	return parseRequirementsTxt(content);
}

/**
 * Parse dependency names from requirements.txt content.
 * One package per line. Lines starting with # are comments.
 * -r, -e, --index-url, etc. are skipped.
 */
export function parseRequirementsTxt(
	content: string,
): PackageDependencySet {
	const names: string[] = [];

	for (const line of content.split("\n")) {
		const trimmed = line.trim();
		if (!trimmed) continue;
		if (trimmed.startsWith("#")) continue;
		if (trimmed.startsWith("-")) continue;
		if (trimmed.startsWith("--")) continue;

		const name = extractPackageName(trimmed);
		if (name) names.push(name.toLowerCase());
	}

	return { names: Object.freeze(names) };
}
