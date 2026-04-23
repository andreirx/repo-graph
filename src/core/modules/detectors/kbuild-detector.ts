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
	kind:
		| "skipped_conditional"
		| "skipped_inside_conditional"
		| "skipped_variable"
		| "skipped_config_gated"
		| "malformed_assignment";
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

	// Track conditional block nesting depth.
	// While depth > 0, assignments are inside a conditional block and must be skipped.
	// This implements D1 "do NOT parse conditionals" — we skip the entire block,
	// not just the directive lines.
	let conditionalDepth = 0;

	for (let i = 0; i < lines.length; i++) {
		const rawLine = lines[i];
		const line = rawLine.trim();
		const lineNum = i + 1;

		// Skip empty lines and comments
		if (!line || line.startsWith("#")) continue;

		// Track conditional block boundaries
		if (
			line.startsWith("ifeq") ||
			line.startsWith("ifneq") ||
			line.startsWith("ifdef") ||
			line.startsWith("ifndef")
		) {
			conditionalDepth++;
			diagnostics.push({
				line: lineNum,
				kind: "skipped_conditional",
				text: line.slice(0, 60),
			});
			continue;
		}

		if (line.startsWith("endif")) {
			if (conditionalDepth > 0) conditionalDepth--;
			diagnostics.push({
				line: lineNum,
				kind: "skipped_conditional",
				text: line.slice(0, 60),
			});
			continue;
		}

		// "else" does not change depth, just record it
		if (line.startsWith("else")) {
			diagnostics.push({
				line: lineNum,
				kind: "skipped_conditional",
				text: line.slice(0, 60),
			});
			continue;
		}

		// D1 scope: only obj-y and obj-m assignments.
		// obj-$(CONFIG_...) is config-gated and out of scope.
		// Patterns matched:
		//   obj-y += dir/
		//   obj-y := dir/
		//   obj-m += dir/
		const objMatch = line.match(
			/^obj-(?:y|m)\s*(?:\+?=|:=)\s*(.+)$/,
		);

		// Check for config-gated assignments (out of D1 scope).
		// These depend on Kconfig evaluation which we do not perform.
		if (!objMatch) {
			const configMatch = line.match(
				/^obj-\$\([^)]+\)\s*(?:\+?=|:=)\s*.+$/,
			);
			if (configMatch) {
				diagnostics.push({
					line: lineNum,
					kind: "skipped_config_gated",
					text: line.slice(0, 60),
				});
			}
			continue;
		}

		// Skip assignments inside conditional blocks (D1: conditionals out of scope).
		// We record a diagnostic but do not emit modules — their existence depends
		// on Kconfig evaluation which we do not perform.
		if (conditionalDepth > 0) {
			diagnostics.push({
				line: lineNum,
				kind: "skipped_inside_conditional",
				text: line.slice(0, 60),
			});
			continue;
		}

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

			// Determine assignment type for evidence.
			// D1 scope: only obj-y (built-in) and obj-m (loadable module).
			const assignmentType = line.includes("obj-y") ? "obj-y" : "obj-m";

			modules.push({
				rootPath: fullPath,
				displayName: subdir, // Use directory name as display name
				moduleKind: "inferred", // Layer 3A: build-system derived
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
