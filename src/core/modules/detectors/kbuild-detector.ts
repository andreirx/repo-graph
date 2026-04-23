/**
 * Kbuild module detector — Layer 3 build-system inference.
 *
 * Parses Linux kernel Kbuild/Makefile files to discover module boundaries.
 * Detects subdirectory assignments via obj-y and obj-m patterns.
 *
 * Scope (D1 minimal, locked in design slice):
 *   - obj-y assignments (built-in)
 *   - obj-m assignments (loadable module)
 *   - Direct directory assignments only (paths ending with `/`)
 *
 * NOT in scope:
 *   - Conditionals (ifeq, ifdef)
 *   - Kconfig variable resolution
 *   - Variable expansion beyond trivial forms
 *   - Object file assignments (not module boundaries)
 *
 * Pure function. No file I/O. Takes Makefile content, returns discovered
 * module roots.
 *
 * See docs/milestones/module-discovery-layer-3.md for design.
 */

import type { DiscoveredModuleRoot } from "../manifest-detectors.js";

/**
 * Result of parsing a Kbuild/Makefile for module subdirectories.
 */
export interface KbuildParseResult {
	/** Discovered subdirectory modules. */
	modules: DiscoveredModuleRoot[];
	/** Parsing diagnostics (warnings, skipped lines). */
	diagnostics: KbuildDiagnostic[];
}

export interface KbuildDiagnostic {
	line: number;
	kind: "skipped_conditional" | "skipped_variable" | "malformed_assignment";
	text: string;
}

/**
 * Detect Kbuild module subdirectories from Makefile content.
 *
 * Parses obj-y and obj-m assignments to find directory references.
 * Only directories (paths ending with `/`) are module boundaries.
 *
 * @param content - Raw Makefile/Kbuild file content
 * @param makefileRelPath - Repo-relative path to the Makefile
 * @returns Parse result with discovered modules and diagnostics
 */
export function detectKbuildModules(
	content: string,
	makefileRelPath: string,
): KbuildParseResult {
	const modules: DiscoveredModuleRoot[] = [];
	const diagnostics: KbuildDiagnostic[] = [];
	const seen = new Set<string>();

	// Determine the directory containing this Makefile
	const makefileDir = makefileRelPath.includes("/")
		? makefileRelPath.slice(0, makefileRelPath.lastIndexOf("/"))
		: ".";

	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const rawLine = lines[i];
		const line = rawLine.trim();
		const lineNum = i + 1;

		// Skip empty lines and comments
		if (!line || line.startsWith("#")) continue;

		// Skip conditionals (not in minimal scope)
		if (
			line.startsWith("ifeq") ||
			line.startsWith("ifneq") ||
			line.startsWith("ifdef") ||
			line.startsWith("ifndef") ||
			line.startsWith("else") ||
			line.startsWith("endif")
		) {
			diagnostics.push({
				line: lineNum,
				kind: "skipped_conditional",
				text: line.slice(0, 60),
			});
			continue;
		}

		// Match obj-y and obj-m assignments (with optional CONFIG variable)
		// Patterns:
		//   obj-y += dir/
		//   obj-y := dir/
		//   obj-$(CONFIG_FOO) += dir/
		//   obj-m += dir/
		const objMatch = line.match(
			/^obj-(?:y|m|\$\([^)]+\))\s*(?:\+?=|:=)\s*(.+)$/,
		);

		if (!objMatch) continue;

		const assignments = objMatch[1];

		// Handle line continuations (backslash at end)
		let fullAssignments = assignments;
		let j = i;
		while (fullAssignments.endsWith("\\") && j + 1 < lines.length) {
			j++;
			fullAssignments =
				fullAssignments.slice(0, -1).trim() + " " + lines[j].trim();
		}

		// Extract individual targets from the assignment
		const targets = fullAssignments
			.split(/\s+/)
			.map((t) => t.trim())
			.filter((t) => t.length > 0);

		for (const target of targets) {
			// Skip and record variable references we couldn't expand
			if (target.includes("$")) {
				diagnostics.push({
					line: lineNum,
					kind: "skipped_variable",
					text: target,
				});
				continue;
			}

			// Only directories (ending with /) are module boundaries
			if (!target.endsWith("/")) continue;

			// Normalize: remove trailing slash for path
			const subdir = target.slice(0, -1);

			// Build full path relative to repo root
			const fullPath =
				makefileDir === "." ? subdir : `${makefileDir}/${subdir}`;

			// Skip duplicates
			if (seen.has(fullPath)) continue;
			seen.add(fullPath);

			// Determine assignment type for evidence
			const isBuiltin = line.includes("obj-y");
			const isModule = line.includes("obj-m");
			const assignmentType = isBuiltin
				? "obj-y"
				: isModule
					? "obj-m"
					: "obj-$(CONFIG)";

			modules.push({
				rootPath: fullPath,
				displayName: subdir, // Use directory name as display name
				sourceType: "kbuild",
				sourcePath: makefileRelPath,
				evidenceKind: "kbuild_subdir",
				confidence: 0.9, // HIGH confidence (build-system derived)
				payload: {
					assignmentType,
					rawLine: line.slice(0, 100),
				},
			});
		}
	}

	return { modules, diagnostics };
}

/**
 * Check if a file path looks like a Kbuild file.
 *
 * Matches:
 *   - Makefile (exact name, any directory)
 *   - Kbuild (exact name, any directory)
 *
 * Does NOT match:
 *   - Makefile.am (GNU Automake)
 *   - *.mk includes
 */
export function isKbuildFile(relPath: string): boolean {
	const filename = relPath.includes("/")
		? relPath.slice(relPath.lastIndexOf("/") + 1)
		: relPath;
	return filename === "Makefile" || filename === "Kbuild";
}
