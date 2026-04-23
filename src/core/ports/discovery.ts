/**
 * Discovery port — the contract for module discovery adapters.
 *
 * The indexer depends on this port, not on filesystem manifest
 * scanning logic directly. This keeps the dependency direction
 * clean: core/indexer → port ← adapter.
 *
 * The adapter is responsible for:
 *   - Walking the filesystem for manifest files
 *   - Reading manifest content
 *   - Expanding workspace glob patterns against real directories
 *   - Calling the pure manifest detectors
 *   - Returning DiscoveredModuleRoot[] for the orchestrator
 *
 * The orchestrator (src/core/modules/module-discovery.ts) converts
 * discovered roots into candidates, evidence, and ownership.
 */

import type { DiscoveredModuleRoot } from "../modules/manifest-detectors.js";

// ── Build-system discovery result ──────────────────────────────────

/**
 * Diagnostic from build-system module discovery.
 *
 * Contains all fields needed for persistence. The indexer transforms
 * these into ModuleDiscoveryDiagnosticInput for storage.
 */
export interface BuildSystemDiagnostic {
	/** Which detector produced this diagnostic (e.g., "kbuild"). */
	sourceType: string;
	/** Source file that produced this diagnostic. */
	filePath: string;
	/** Line number (1-based) if applicable. */
	line?: number;
	/** Diagnostic category (e.g., "skipped_config_gated"). */
	kind: string;
	/** Raw text of the line that triggered the diagnostic (truncated). */
	rawText?: string;
	/** Human-readable description. */
	message: string;
	/** Severity: info (D1 exclusions) or warning (parse issues). */
	severity: "info" | "warning";
}

/**
 * Result of build-system module discovery.
 * Includes both discovered modules and diagnostics for auditability.
 */
export interface BuildSystemDiscoveryResult {
	/** Discovered module roots. */
	modules: DiscoveredModuleRoot[];
	/** Diagnostics (skipped conditionals, variables, parse issues). */
	diagnostics: BuildSystemDiagnostic[];
	/** Number of eligible files scanned. */
	filesScanned: number;
	/** Number of files that produced at least one module. */
	filesWithModules: number;
}

// ── Discovery port ─────────────────────────────────────────────────

/**
 * Discovers declared module roots in a repository.
 *
 * Implementations scan the filesystem for manifest/workspace files,
 * parse them, expand glob patterns, and return the detected roots.
 */
export interface ModuleDiscoveryPort {
	/**
	 * Scan the repository for declared module roots (Layer 1).
	 * Sources: package.json, Cargo.toml, settings.gradle, pyproject.toml.
	 *
	 * @param rootPath - Absolute path to the repository root.
	 * @param repoUid - Repository UID (for file UID construction).
	 * @returns Discovered module roots with evidence metadata.
	 */
	discoverDeclaredModules(
		rootPath: string,
		repoUid: string,
	): Promise<DiscoveredModuleRoot[]>;

	/**
	 * Scan the repository for build-system-derived modules (Layer 3A).
	 * Sources: Kbuild, Makefile (future: CMake, GNU Makefile).
	 *
	 * Returns modules with `moduleKind: "inferred"`.
	 * Diagnostics are persisted to module_discovery_diagnostics table.
	 *
	 * @param rootPath - Absolute path to the repository root.
	 * @param repoUid - Repository UID (for evidence attribution).
	 * @returns Discovery result with modules, diagnostics, and scan stats.
	 */
	discoverBuildSystemModules?(
		rootPath: string,
		repoUid: string,
	): Promise<BuildSystemDiscoveryResult>;
}
