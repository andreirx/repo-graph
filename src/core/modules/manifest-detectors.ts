/**
 * Manifest-based module discovery detectors.
 *
 * Pure functions that take parsed manifest content and return
 * discovered module roots with evidence. No file I/O — the
 * adapters read files and pass content here.
 *
 * Each detector returns an array of DiscoveredModuleRoot objects.
 * The orchestrator (module-discovery.ts) converts these into
 * ModuleCandidate + ModuleCandidateEvidence + ModuleFileOwnership.
 *
 * Slice 1 detectors (declared/high-trust only):
 *   - package.json workspaces
 *   - pnpm-workspace.yaml
 *   - Cargo.toml workspace members + crate roots
 *   - settings.gradle / settings.gradle.kts subprojects
 *   - pyproject.toml project roots
 *
 * NOT in this slice:
 *   - __init__.py (weaker evidence, not declared-module-tier)
 *   - entrypoint/operational detection
 *   - graph-based inference
 */

import type { EvidenceKind, EvidenceSourceType, ModuleKind } from "./module-candidate.js";

// ── Detector output ────────────────────────────────────────────────

/**
 * A discovered module root from a manifest detector.
 * Lightweight intermediate type — the orchestrator enriches this
 * into full ModuleCandidate + evidence + ownership rows.
 *
 * The detector owns the semantic classification:
 *   - declared: package manager manifests (package.json, Cargo.toml, etc.)
 *   - operational: surface promotion (CLI, service, web app, worker)
 *   - inferred: build-system (Kbuild) or structural patterns
 *
 * The orchestrator validates coherence but does not derive moduleKind.
 */
export interface DiscoveredModuleRoot {
	/** Repo-relative path of the module root directory. */
	rootPath: string;
	/** Human-facing name from the manifest (package name, crate name, etc.). */
	displayName: string | null;
	/**
	 * Module classification. Set by the detector, not derived by orchestrator.
	 * - declared: from package manager manifests
	 * - operational: from surface promotion
	 * - inferred: from build-system or structural analysis
	 */
	moduleKind: ModuleKind;
	/** Evidence source type. */
	sourceType: EvidenceSourceType;
	/** Repo-relative path to the manifest file that declared this module. */
	sourcePath: string;
	/** What kind of evidence this is. */
	evidenceKind: EvidenceKind;
	/** Confidence (0.0–1.0). */
	confidence: number;
	/** Optional structured payload for the evidence row. */
	payload: Record<string, unknown> | null;
}

// ── package.json workspaces ────────────────────────────────────────

/**
 * Detect workspace members from a root package.json.
 *
 * Reads the `workspaces` field, which can be:
 *   - an array of glob patterns: `["packages/*", "apps/*"]`
 *   - an object with `packages` array: `{ packages: ["packages/*"] }`
 *
 * This detector does NOT glob-expand patterns against the filesystem.
 * It returns the raw patterns — the orchestrator resolves them against
 * the actual directory listing.
 *
 * The workspace root itself is only a module candidate if it has a
 * `name` field (i.e., it's also a publishable package).
 */
export function detectPackageJsonWorkspaces(
	content: string,
	_manifestRelPath: string,
): { patterns: string[]; rootName: string | null } | null {
	let pkg: Record<string, unknown>;
	try {
		pkg = JSON.parse(content);
	} catch {
		return null;
	}

	const workspaces = pkg.workspaces;
	if (!workspaces) return null;

	let patterns: string[];
	if (Array.isArray(workspaces)) {
		patterns = workspaces.filter((w): w is string => typeof w === "string");
	} else if (
		typeof workspaces === "object" &&
		workspaces !== null &&
		"packages" in workspaces &&
		Array.isArray((workspaces as Record<string, unknown>).packages)
	) {
		patterns = ((workspaces as Record<string, unknown>).packages as unknown[]).filter(
			(w): w is string => typeof w === "string",
		);
	} else {
		return null;
	}

	if (patterns.length === 0) return null;

	const rootName = typeof pkg.name === "string" ? pkg.name : null;
	return { patterns, rootName };
}

/**
 * Detect a package root from a non-workspace package.json.
 * Returns the package name if `name` field exists.
 */
export function detectPackageJsonRoot(
	content: string,
	manifestRelPath: string,
): DiscoveredModuleRoot | null {
	let pkg: Record<string, unknown>;
	try {
		pkg = JSON.parse(content);
	} catch {
		return null;
	}

	// Skip workspace roots — those are handled by detectPackageJsonWorkspaces.
	if (pkg.workspaces) return null;

	const name = typeof pkg.name === "string" ? pkg.name : null;
	if (!name) return null;

	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";

	return {
		rootPath: dir,
		displayName: name,
		moduleKind: "declared",
		sourceType: "package_json_workspaces",
		sourcePath: manifestRelPath,
		evidenceKind: "package_root",
		confidence: 0.95,
		payload: { packageName: name },
	};
}

// ── pnpm-workspace.yaml ────────────────────────────────────────────

/**
 * Detect workspace member patterns from pnpm-workspace.yaml.
 *
 * Format:
 *   packages:
 *     - 'packages/*'
 *     - 'apps/*'
 *
 * Returns raw patterns for the orchestrator to resolve.
 */
export function detectPnpmWorkspace(
	content: string,
	_manifestRelPath: string,
): string[] | null {
	// Minimal YAML parsing — pnpm-workspace.yaml has a very simple
	// structure: one top-level `packages:` key with a list of strings.
	const patterns: string[] = [];
	let inPackages = false;

	for (const rawLine of content.split("\n")) {
		const line = rawLine.trim();
		if (line === "packages:" || line === "packages :") {
			inPackages = true;
			continue;
		}
		if (inPackages) {
			// List item: `- 'pattern'` or `- "pattern"` or `- pattern`
			const match = line.match(/^-\s+['"]?([^'"]+)['"]?\s*$/);
			if (match) {
				patterns.push(match[1]);
			} else if (line && !line.startsWith("-") && !line.startsWith("#")) {
				// New top-level key — stop collecting.
				break;
			}
		}
	}

	return patterns.length > 0 ? patterns : null;
}

// ── Cargo.toml ─────────────────────────────────────────────────────

/**
 * Detect Cargo workspace members and individual crate roots.
 *
 * Workspace members are listed in `[workspace]` section as:
 *   members = ["crates/*", "tools/foo"]
 *
 * A standalone crate (no workspace) has `[package]` with `name`.
 *
 * Returns both workspace member patterns (for orchestrator resolution)
 * and standalone crate evidence.
 */
export function detectCargoManifest(
	content: string,
	manifestRelPath: string,
): {
	workspaceMembers: string[] | null;
	crateRoot: DiscoveredModuleRoot | null;
} {
	const lines = content.split("\n");
	let inWorkspace = false;
	let inPackage = false;
	let membersLine = "";
	let crateName: string | null = null;

	for (const rawLine of lines) {
		const line = rawLine.trim();
		if (line === "[workspace]") {
			inWorkspace = true;
			inPackage = false;
			continue;
		}
		if (line === "[package]") {
			inPackage = true;
			inWorkspace = false;
			continue;
		}
		if (line.startsWith("[") && line.endsWith("]")) {
			inWorkspace = false;
			inPackage = false;
			continue;
		}

		if (inWorkspace && line.startsWith("members")) {
			// members = ["crates/*", "tools/foo"]
			// May span multiple lines — collect until we see ]
			membersLine += line;
			if (!membersLine.includes("]")) {
				// Continue collecting on next lines
				for (const next of lines.slice(lines.indexOf(rawLine) + 1)) {
					membersLine += " " + next.trim();
					if (membersLine.includes("]")) break;
				}
			}
		}

		if (inPackage) {
			const nameMatch = line.match(/^name\s*=\s*"([^"]+)"/);
			if (nameMatch) {
				crateName = nameMatch[1];
			}
		}
	}

	// Parse workspace members.
	let workspaceMembers: string[] | null = null;
	if (membersLine) {
		const arrMatch = membersLine.match(/\[([^\]]*)\]/);
		if (arrMatch) {
			workspaceMembers = arrMatch[1]
				.split(",")
				.map((s) => s.trim().replace(/^["']|["']$/g, ""))
				.filter((s) => s.length > 0);
		}
	}

	// Crate root: emitted whenever [package] name is present,
	// regardless of whether this manifest also declares workspace
	// members. In Cargo, a workspace root can legitimately be both
	// a workspace host AND a crate/module itself.
	let crateRoot: DiscoveredModuleRoot | null = null;
	if (crateName) {
		const dir = manifestRelPath.includes("/")
			? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
			: ".";
		crateRoot = {
			rootPath: dir,
			displayName: crateName,
			moduleKind: "declared",
			sourceType: "cargo_crate",
			sourcePath: manifestRelPath,
			evidenceKind: "crate_root",
			confidence: 0.95,
			payload: { crateName },
		};
	}

	return { workspaceMembers, crateRoot };
}

// ── settings.gradle ────────────────────────────────────────────────

/**
 * Detect Gradle subprojects from settings.gradle or settings.gradle.kts.
 *
 * Patterns:
 *   include 'subproject-a', 'subproject-b'
 *   include(':subproject-a')
 *   include ':subproject-a', ':subproject-b'
 *   includeFlat 'subproject-a'
 */
export function detectGradleSettings(
	content: string,
	manifestRelPath: string,
): DiscoveredModuleRoot[] {
	const results: DiscoveredModuleRoot[] = [];
	const seen = new Set<string>();

	// Match include/includeFlat with various quoting styles.
	const includePattern = /include(?:Flat)?\s*\(?([^)\n]+)\)?/g;
	let match: RegExpExecArray | null;

	while ((match = includePattern.exec(content)) !== null) {
		const args = match[1];
		// Extract quoted strings: 'foo', "foo", ':foo'
		const projectPattern = /['"][:.]?([^'"]+)['"]/g;
		let projMatch: RegExpExecArray | null;
		while ((projMatch = projectPattern.exec(args)) !== null) {
			const projectPath = projMatch[1].replace(/:/g, "/");
			if (seen.has(projectPath)) continue;
			seen.add(projectPath);

			results.push({
				rootPath: projectPath,
				displayName: projectPath.split("/").pop() ?? projectPath,
				moduleKind: "declared",
				sourceType: "gradle_settings",
				sourcePath: manifestRelPath,
				evidenceKind: "subproject",
				confidence: 0.95,
				payload: { gradleProjectPath: projMatch[1] },
			});
		}
	}

	return results;
}

// ── pyproject.toml ─────────────────────────────────────────────────

/**
 * Detect a Python project root from pyproject.toml.
 *
 * A pyproject.toml is a module candidate if it declares a project name:
 *   [project]
 *   name = "my-package"
 *
 * Or for Poetry:
 *   [tool.poetry]
 *   name = "my-package"
 *
 * Does NOT detect `__init__.py` — that's weaker evidence excluded
 * from the declared/high-trust layer.
 */
export function detectPyprojectToml(
	content: string,
	manifestRelPath: string,
): DiscoveredModuleRoot | null {
	const lines = content.split("\n");
	let inProject = false;
	let inPoetry = false;
	let packageName: string | null = null;

	for (const rawLine of lines) {
		const line = rawLine.trim();
		if (line === "[project]") {
			inProject = true;
			inPoetry = false;
			continue;
		}
		if (line === "[tool.poetry]") {
			inPoetry = true;
			inProject = false;
			continue;
		}
		if (line.startsWith("[") && line.endsWith("]")) {
			inProject = false;
			inPoetry = false;
			continue;
		}

		if ((inProject || inPoetry) && !packageName) {
			const nameMatch = line.match(/^name\s*=\s*"([^"]+)"/);
			if (nameMatch) {
				packageName = nameMatch[1];
			}
		}
	}

	if (!packageName) return null;

	const dir = manifestRelPath.includes("/")
		? manifestRelPath.slice(0, manifestRelPath.lastIndexOf("/"))
		: ".";

	return {
		rootPath: dir,
		displayName: packageName,
		moduleKind: "declared",
		sourceType: "pyproject_toml",
		sourcePath: manifestRelPath,
		evidenceKind: "package_root",
		confidence: 0.90,
		payload: { packageName },
	};
}
