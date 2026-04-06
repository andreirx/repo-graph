/**
 * Cargo.toml dependency reader (adapter).
 *
 * Reads dependency names from a Cargo.toml file and produces a
 * PackageDependencySet DTO for the classifier. Analogous to the
 * package.json dependency reader for TypeScript.
 *
 * TOML parsing:
 *   Cargo.toml is TOML, not JSON. This reader uses a conservative
 *   line-based parser that extracts dependency names from:
 *     - [dependencies] section
 *     - [dev-dependencies] section
 *     - [build-dependencies] section
 *   It handles both inline table form (`foo = "1.0"`) and expanded
 *   table form (`foo = { version = "1.0", ... }`).
 *
 *   It does NOT handle:
 *     - target-specific dependencies (`[target.'cfg(...)'.dependencies]`)
 *     - workspace dependency inheritance (`foo.workspace = true`)
 *       — the dependency NAME is still captured even if the version
 *       is inherited
 *
 * Failure modes (all yield null or empty, not exceptions):
 *   - file does not exist
 *   - file is unreadable
 *   - no dependency sections found
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { PackageDependencySet } from "../../core/classification/signals.js";

/**
 * Read dependency names from a Cargo.toml at the given directory.
 * Returns null if the file doesn't exist or can't be read.
 */
export async function readCargoDependencies(
	dirPath: string,
): Promise<PackageDependencySet | null> {
	const cargoPath = join(dirPath, "Cargo.toml");
	let content: string;
	try {
		content = await readFile(cargoPath, "utf-8");
	} catch {
		return null;
	}

	return parseCargoDependencies(content);
}

/**
 * Parse dependency names from Cargo.toml content.
 * Exported for testing.
 */
export function parseCargoDependencies(
	content: string,
): PackageDependencySet {
	const names = new Set<string>();
	const lines = content.split("\n");

	// Track which TOML section we're in.
	let inDepSection = false;

	const DEP_SECTIONS = new Set([
		"[dependencies]",
		"[dev-dependencies]",
		"[build-dependencies]",
	]);

	for (const rawLine of lines) {
		const line = rawLine.trim();

		// Section header: [something]
		if (line.startsWith("[")) {
			inDepSection = DEP_SECTIONS.has(line);
			continue;
		}

		// Skip empty lines and comments.
		if (line === "" || line.startsWith("#")) continue;

		// If we're in a dependency section, extract the dependency name.
		if (inDepSection) {
			// Dependency lines have the form:
			//   name = "version"
			//   name = { version = "...", features = [...] }
			//   name.version = "..."
			//   name.workspace = true
			const eqIdx = line.indexOf("=");
			if (eqIdx <= 0) continue;

			let key = line.slice(0, eqIdx).trim();
			// Handle dotted keys: `foo.version = "1.0"` → dep name is "foo"
			const dotIdx = key.indexOf(".");
			if (dotIdx > 0) {
				key = key.slice(0, dotIdx).trim();
			}

			// Validate: dependency names are alphanumeric + hyphens + underscores.
			if (/^[a-zA-Z0-9_-]+$/.test(key)) {
				names.add(key);
			}
		}
	}

	return { names: Object.freeze([...names].sort()) };
}
