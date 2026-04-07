/**
 * Gradle build.gradle dependency reader (adapter).
 *
 * Reads dependency names from a build.gradle file (Groovy or Kotlin DSL)
 * and produces a PackageDependencySet DTO for the classifier. Analogous
 * to the Cargo.toml dependency reader for Rust.
 *
 * Parsing:
 *   Uses a conservative regex-based parser that extracts dependency
 *   coordinates from the `dependencies { }` block. Handles:
 *     - Groovy string literals: implementation 'group:artifact:version'
 *     - Groovy GString literals: implementation "group:artifact:version"
 *     - Kotlin DSL function calls: implementation("group:artifact:version")
 *     - All standard configurations: implementation, api, compileOnly,
 *       runtimeOnly, testImplementation, annotationProcessor
 *
 *   Extracts the `group:artifact` part (without version) as the
 *   dependency name.
 *
 *   It does NOT handle:
 *     - Project dependencies: implementation(project(":submodule"))
 *     - File dependencies: implementation(files("lib.jar"))
 *     - Variable interpolation: implementation "$depVersion"
 *     - Version catalogs: implementation(libs.some.dep)
 *
 * Failure modes (all yield null or empty, not exceptions):
 *   - file does not exist
 *   - file is unreadable
 *   - no dependencies block found
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { PackageDependencySet } from "../../core/classification/signals.js";

/**
 * Read dependency names from a build.gradle at the given directory.
 * Tries build.gradle first, then build.gradle.kts (Kotlin DSL).
 * Returns null if neither file exists or can be read.
 */
export async function readGradleDependencies(
	dirPath: string,
): Promise<PackageDependencySet | null> {
	// Try Groovy DSL first, then Kotlin DSL.
	for (const filename of ["build.gradle", "build.gradle.kts"]) {
		const gradlePath = join(dirPath, filename);
		try {
			const content = await readFile(gradlePath, "utf-8");
			return parseGradleDependencies(content);
		} catch {
			// File doesn't exist or can't be read; try next.
		}
	}

	return null;
}

/**
 * Parse dependency names from build.gradle content.
 * Exported for testing.
 */
export function parseGradleDependencies(
	content: string,
): PackageDependencySet {
	const names = new Set<string>();

	// Known Gradle dependency configurations.
	const CONFIGURATIONS = [
		"implementation",
		"api",
		"compileOnly",
		"runtimeOnly",
		"testImplementation",
		"testCompileOnly",
		"testRuntimeOnly",
		"annotationProcessor",
		"kapt",
		"compile",
		"testCompile",
		"provided",
	];

	// Build a regex that matches any configuration followed by a
	// dependency coordinate string.
	//
	// Groovy DSL forms:
	//   implementation 'group:artifact:version'
	//   implementation "group:artifact:version"
	//   implementation group: 'group', name: 'artifact', version: 'version'
	//
	// Kotlin DSL forms:
	//   implementation("group:artifact:version")
	//
	// We capture the coordinate string inside quotes.
	const configPattern = CONFIGURATIONS.join("|");

	// Pattern 1: configuration 'group:artifact:version' or configuration("group:artifact:version")
	// Handles both Groovy and Kotlin DSL with single or double quotes.
	const coordRegex = new RegExp(
		`(?:${configPattern})\\s*(?:\\(\\s*)?['"]([^'"]+)['"]`,
		"g",
	);

	let match: RegExpExecArray | null;
	while ((match = coordRegex.exec(content)) !== null) {
		const coordinate = match[1];
		// A Maven coordinate is group:artifact:version.
		// Store the GROUP ID as primary dep name because Java package
		// paths typically share a prefix with the Maven group.
		// E.g. group "org.springframework.boot" → imports from
		// "org.springframework.*" packages.
		// Also store group:artifact for exact matching if needed.
		const parts = coordinate.split(":");
		if (parts.length >= 2) {
			const group = parts[0];
			names.add(group); // full group ID
			names.add(`${group}:${parts[1]}`); // full coordinate
			// Add shorter group prefixes to catch transitive package
			// hierarchies. Maven group "org.springframework.boot" pulls
			// in classes from "org.springframework.web.*", so we also
			// add "org.springframework" (first 2 dot-segments).
			const segments = group.split(".");
			if (segments.length > 2) {
				names.add(segments.slice(0, 2).join("."));
			}
		}
	}

	// Pattern 2: map notation -- group: 'x', name: 'y', version: 'z'
	const mapRegex = new RegExp(
		`(?:${configPattern})\\s+group:\\s*['"]([^'"]+)['"]\\s*,\\s*name:\\s*['"]([^'"]+)['"]`,
		"g",
	);

	while ((match = mapRegex.exec(content)) !== null) {
		const group = match[1];
		const artifact = match[2];
		names.add(group);
		names.add(`${group}:${artifact}`);
		const segments = group.split(".");
		if (segments.length > 2) {
			names.add(segments.slice(0, 2).join("."));
		}
	}

	return { names: Object.freeze([...names].sort()) };
}
