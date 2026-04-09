/**
 * Manifest scanner — filesystem adapter for module discovery.
 *
 * Implements ModuleDiscoveryPort. Walks the repository for manifest
 * and workspace files, reads their content, calls the pure manifest
 * detectors, and expands workspace glob patterns against real
 * directory listings.
 *
 * Supported manifest types (slice 1):
 *   - package.json (workspaces + standalone)
 *   - pnpm-workspace.yaml
 *   - Cargo.toml (workspace + standalone crate)
 *   - settings.gradle / settings.gradle.kts
 *   - pyproject.toml
 *
 * Glob expansion is intentionally simple: only single-level wildcards
 * (e.g., "packages/*") are expanded by listing the parent directory.
 * Deep globs ("packages/ **") are not supported in slice 1.
 */

import { existsSync } from "node:fs";
import { readFile, readdir } from "node:fs/promises";
import { join, relative, dirname } from "node:path";
import type { ModuleDiscoveryPort } from "../../core/ports/discovery.js";
import type { DiscoveredModuleRoot } from "../../core/modules/manifest-detectors.js";
import {
	detectCargoManifest,
	detectGradleSettings,
	detectPackageJsonRoot,
	detectPackageJsonWorkspaces,
	detectPnpmWorkspace,
	detectPyprojectToml,
} from "../../core/modules/manifest-detectors.js";

export class ManifestScanner implements ModuleDiscoveryPort {
	async discoverDeclaredModules(
		rootPath: string,
		_repoUid: string,
	): Promise<DiscoveredModuleRoot[]> {
		const results: DiscoveredModuleRoot[] = [];

		// Scan for all manifest files in the repo.
		const manifests = await this.findManifests(rootPath);

		// ── package.json workspaces + pnpm-workspace.yaml ──────────
		// Check root-level workspace files first.
		await this.processJsWorkspaces(rootPath, manifests, results);

		// Process all package.json files as standalone package roots.
		// Root package.json is also checked — if it has no workspaces
		// but has a name, it's a standalone declared package.
		for (const relPath of manifests.packageJsons) {
			const absPath = join(rootPath, relPath);
			let content: string;
			try {
				content = await readFile(absPath, "utf-8");
			} catch { continue; }

			const root = detectPackageJsonRoot(content, relPath);
			if (root) results.push(root);
		}

		// ── Cargo.toml ─────────────────────────────────────────────
		for (const relPath of manifests.cargoTomls) {
			const absPath = join(rootPath, relPath);
			let content: string;
			try {
				content = await readFile(absPath, "utf-8");
			} catch { continue; }

			const { workspaceMembers, crateRoot } = detectCargoManifest(content, relPath);

			if (workspaceMembers) {
				const manifestDir = relPath === "Cargo.toml" ? "" : dirname(relPath);
				const expanded = await this.expandGlobPatterns(
					rootPath,
					manifestDir,
					workspaceMembers,
				);
				for (const memberPath of expanded) {
					// Each member should have its own Cargo.toml — read it
					// to get the crate name.
					const memberCargoPath = join(rootPath, memberPath, "Cargo.toml");
					let memberName: string | null = null;
					try {
						const memberContent = await readFile(memberCargoPath, "utf-8");
						const memberResult = detectCargoManifest(memberContent, join(memberPath, "Cargo.toml"));
						memberName = memberResult.crateRoot?.displayName ?? null;
					} catch { /* no Cargo.toml in member dir */ }

					results.push({
						rootPath: memberPath,
						displayName: memberName,
						sourceType: "cargo_workspace",
						sourcePath: relPath,
						evidenceKind: "workspace_member",
						confidence: 0.95,
						payload: { pattern: workspaceMembers, resolvedPath: memberPath },
					});
				}
			}

			if (crateRoot) {
				results.push(crateRoot);
			}
		}

		// ── settings.gradle ────────────────────────────────────────
		for (const relPath of manifests.gradleSettings) {
			const absPath = join(rootPath, relPath);
			let content: string;
			try {
				content = await readFile(absPath, "utf-8");
			} catch { continue; }

			const subprojects = detectGradleSettings(content, relPath);
			results.push(...subprojects);
		}

		// ── pyproject.toml ─────────────────────────────────────────
		for (const relPath of manifests.pyprojectTomls) {
			const absPath = join(rootPath, relPath);
			let content: string;
			try {
				content = await readFile(absPath, "utf-8");
			} catch { continue; }

			const root = detectPyprojectToml(content, relPath);
			if (root) results.push(root);
		}

		return results;
	}

	// ── JS workspace processing ────────────────────────────────────

	private async processJsWorkspaces(
		rootPath: string,
		manifests: ManifestPaths,
		results: DiscoveredModuleRoot[],
	): Promise<void> {
		// Track patterns per source separately so evidence is attributed
		// only to the source that actually declared the pattern.
		let pkgJsonPatterns: string[] = [];
		let pnpmPatterns: string[] = [];

		// 1. Root package.json workspaces.
		if (manifests.packageJsons.includes("package.json")) {
			const absPath = join(rootPath, "package.json");
			try {
				const content = await readFile(absPath, "utf-8");
				const ws = detectPackageJsonWorkspaces(content, "package.json");
				if (ws) {
					pkgJsonPatterns = ws.patterns;
				}
			} catch { /* ignore */ }
		}

		// 2. pnpm-workspace.yaml.
		if (manifests.pnpmWorkspace) {
			const absPath = join(rootPath, manifests.pnpmWorkspace);
			try {
				const content = await readFile(absPath, "utf-8");
				const patterns = detectPnpmWorkspace(content, manifests.pnpmWorkspace);
				if (patterns) {
					pnpmPatterns = patterns;
				}
			} catch { /* ignore */ }
		}

		if (pkgJsonPatterns.length === 0 && pnpmPatterns.length === 0) return;

		// Expand each source's patterns separately to track provenance.
		const pkgJsonMembers = pkgJsonPatterns.length > 0
			? new Set(await this.expandGlobPatterns(rootPath, "", pkgJsonPatterns))
			: new Set<string>();
		const pnpmMembers = pnpmPatterns.length > 0
			? new Set(await this.expandGlobPatterns(rootPath, "", pnpmPatterns))
			: new Set<string>();

		// Union of all member paths.
		const allMembers = new Set([...pkgJsonMembers, ...pnpmMembers]);

		for (const memberPath of allMembers) {
			// Read the member's package.json to get its name.
			const memberPkgPath = join(rootPath, memberPath, "package.json");
			let memberName: string | null = null;
			try {
				const memberContent = await readFile(memberPkgPath, "utf-8");
				const parsed = JSON.parse(memberContent) as Record<string, unknown>;
				if (typeof parsed.name === "string") {
					memberName = parsed.name;
				}
			} catch { /* no package.json or unparseable */ }

			// Emit evidence only from sources that actually matched.
			if (pkgJsonMembers.has(memberPath)) {
				results.push({
					rootPath: memberPath,
					displayName: memberName,
					sourceType: "package_json_workspaces",
					sourcePath: "package.json",
					evidenceKind: "workspace_member",
					confidence: 0.95,
					payload: { patterns: pkgJsonPatterns, resolvedPath: memberPath },
				});
			}

			if (pnpmMembers.has(memberPath)) {
				results.push({
					rootPath: memberPath,
					displayName: memberName,
					sourceType: "pnpm_workspace_yaml",
					sourcePath: manifests.pnpmWorkspace!,
					evidenceKind: "workspace_member",
					confidence: 0.95,
					payload: { patterns: pnpmPatterns, resolvedPath: memberPath },
				});
			}
		}
	}

	// ── Manifest file discovery ────────────────────────────────────

	private async findManifests(rootPath: string): Promise<ManifestPaths> {
		const result: ManifestPaths = {
			packageJsons: [],
			pnpmWorkspace: null,
			cargoTomls: [],
			gradleSettings: [],
			pyprojectTomls: [],
		};

		// Check root-level files first (fast path).
		if (existsSync(join(rootPath, "package.json"))) {
			result.packageJsons.push("package.json");
		}
		if (existsSync(join(rootPath, "pnpm-workspace.yaml"))) {
			result.pnpmWorkspace = "pnpm-workspace.yaml";
		}
		if (existsSync(join(rootPath, "Cargo.toml"))) {
			result.cargoTomls.push("Cargo.toml");
		}
		if (existsSync(join(rootPath, "settings.gradle"))) {
			result.gradleSettings.push("settings.gradle");
		}
		if (existsSync(join(rootPath, "settings.gradle.kts"))) {
			result.gradleSettings.push("settings.gradle.kts");
		}
		if (existsSync(join(rootPath, "pyproject.toml"))) {
			result.pyprojectTomls.push("pyproject.toml");
		}

		// Walk subdirectories for nested manifests (max depth 4).
		await this.walkForManifests(rootPath, rootPath, result, 0, 4);

		return result;
	}

	private async walkForManifests(
		rootPath: string,
		dir: string,
		result: ManifestPaths,
		depth: number,
		maxDepth: number,
	): Promise<void> {
		if (depth >= maxDepth) return;

		let entries: import("node:fs").Dirent[];
		try {
			entries = await readdir(dir, { withFileTypes: true });
		} catch {
			return;
		}

		for (const entry of entries) {
			if (!entry.isDirectory()) continue;
			const name = entry.name;

			// Skip common non-source directories.
			if (
				name === "node_modules" ||
				name === ".git" ||
				name === "target" ||
				name === "build" ||
				name === "dist" ||
				name === ".gradle" ||
				name === "__pycache__" ||
				name === ".venv" ||
				name === "venv"
			) continue;

			const absDir = join(dir, name);
			const relDir = relative(rootPath, absDir);

			// Check for manifest files in this directory.
			if (existsSync(join(absDir, "package.json"))) {
				const relPath = join(relDir, "package.json");
				if (!result.packageJsons.includes(relPath)) {
					result.packageJsons.push(relPath);
				}
			}
			if (existsSync(join(absDir, "Cargo.toml"))) {
				const relPath = join(relDir, "Cargo.toml");
				if (!result.cargoTomls.includes(relPath)) {
					result.cargoTomls.push(relPath);
				}
			}
			if (existsSync(join(absDir, "pyproject.toml"))) {
				const relPath = join(relDir, "pyproject.toml");
				if (!result.pyprojectTomls.includes(relPath)) {
					result.pyprojectTomls.push(relPath);
				}
			}
			// settings.gradle only at root level — Gradle convention.

			await this.walkForManifests(rootPath, absDir, result, depth + 1, maxDepth);
		}
	}

	// ── Glob expansion ─────────────────────────────────────────────

	/**
	 * Expand workspace glob patterns against the real directory listing.
	 *
	 * Supports:
	 *   - "packages/*" → list children of packages/
	 *   - "tools/foo" → literal path (verify exists)
	 *
	 * Does NOT support:
	 *   - Deep globs ("packages/ * *")
	 *   - Negation ("!packages/internal")
	 *
	 * Returns repo-relative directory paths.
	 */
	private async expandGlobPatterns(
		rootPath: string,
		baseDir: string,
		patterns: string[],
	): Promise<string[]> {
		const results: string[] = [];
		const seen = new Set<string>();

		for (const pattern of patterns) {
			const fullPattern = baseDir ? `${baseDir}/${pattern}` : pattern;

			if (fullPattern.endsWith("/*")) {
				// Single-level wildcard: list children.
				const parentDir = fullPattern.slice(0, -2);
				const absParent = join(rootPath, parentDir);
				try {
					const entries = await readdir(absParent, { withFileTypes: true });
					for (const entry of entries) {
						if (!entry.isDirectory()) continue;
						if (entry.name.startsWith(".")) continue;
						const childPath = `${parentDir}/${entry.name}`;
						if (!seen.has(childPath)) {
							seen.add(childPath);
							results.push(childPath);
						}
					}
				} catch { /* parent dir doesn't exist */ }
			} else {
				// Literal path: verify it exists as a directory.
				const absPath = join(rootPath, fullPattern);
				if (existsSync(absPath)) {
					if (!seen.has(fullPattern)) {
						seen.add(fullPattern);
						results.push(fullPattern);
					}
				}
			}
		}

		return results;
	}
}

// ── Internal types ─────────────────────────────────────────────────

interface ManifestPaths {
	packageJsons: string[];
	pnpmWorkspace: string | null;
	cargoTomls: string[];
	gradleSettings: string[];
	pyprojectTomls: string[];
}
