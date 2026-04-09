/**
 * Invalidation plan — core domain types for delta indexing.
 *
 * The invalidation planner takes parent snapshot state and current
 * filesystem state, and produces a plan that classifies every file
 * into one of: unchanged (copy forward), changed (re-extract),
 * new (extract fresh), deleted (do not copy), or config-widened
 * (re-extract due to manifest/config change in scope).
 *
 * Pure domain model. No filesystem, no storage, no side effects.
 */

// ── File classification ────────────────────────────────────────────

/**
 * How a file was classified by the invalidation planner.
 */
export type FileDisposition =
	| "unchanged"      // content hash matches parent — copy forward
	| "changed"        // content hash differs — re-extract
	| "new"            // not in parent snapshot — extract fresh
	| "deleted"        // in parent but not in current tree — do not copy
	| "config_widened"; // unchanged content but config/manifest in scope changed

/**
 * One file's classification in the invalidation plan.
 */
export interface FileClassification {
	/** Repo-relative file path. */
	readonly path: string;
	/** File UID (repoUid:path format). */
	readonly fileUid: string;
	/** How this file was classified. */
	readonly disposition: FileDisposition;
	/** Why this file was classified this way. */
	readonly reason: string;
	/** Content hash from current filesystem (null for deleted files). */
	readonly currentHash: string | null;
	/** Content hash from parent snapshot (null for new files). */
	readonly parentHash: string | null;
}

// ── Config change detection ────────────────────────────────────────

/**
 * A manifest or config file that changed between parent and current.
 * Used to determine config-widening scope.
 */
export interface ChangedConfig {
	/** Repo-relative path to the config file. */
	readonly path: string;
	/** Config type for scope determination. */
	readonly configType: ConfigType;
}

/**
 * Types of config files that trigger invalidation widening.
 */
export type ConfigType =
	| "package_json"
	| "pnpm_workspace_yaml"
	| "tsconfig_json"
	| "cargo_toml"
	| "build_gradle"
	| "settings_gradle"
	| "pyproject_toml"
	| "compile_commands_json";

/**
 * Map from config type to the file patterns that identify it.
 */
export const CONFIG_FILE_PATTERNS: ReadonlyMap<string, ConfigType> = new Map([
	["package.json", "package_json"],
	["pnpm-workspace.yaml", "pnpm_workspace_yaml"],
	["tsconfig.json", "tsconfig_json"],
	["tsconfig.base.json", "tsconfig_json"],
	["Cargo.toml", "cargo_toml"],
	["build.gradle", "build_gradle"],
	["build.gradle.kts", "build_gradle"],
	["settings.gradle", "settings_gradle"],
	["settings.gradle.kts", "settings_gradle"],
	["pyproject.toml", "pyproject_toml"],
	["compile_commands.json", "compile_commands_json"],
]);

// ── Invalidation plan ──────────────────────────────────────────────

/**
 * The complete invalidation plan for a delta indexing run.
 */
export interface InvalidationPlan {
	/** Parent snapshot UID this plan is relative to. */
	readonly parentSnapshotUid: string;
	/** Per-file classifications. */
	readonly files: FileClassification[];
	/** Config files that changed (triggers for widening). */
	readonly changedConfigs: ChangedConfig[];

	// ── Computed subsets for the indexer ────────────────────────

	/** Files to re-extract (changed + new + config_widened). */
	readonly filesToExtract: string[];
	/** Files to copy forward from parent (unchanged). */
	readonly filesToCopy: string[];
	/** Files to delete (in parent, not in current). */
	readonly filesToDelete: string[];

	// ── Summary counts ─────────────────────────────────────────

	readonly counts: InvalidationCounts;
}

/**
 * Summary counts for trust metadata and diagnostics.
 */
export interface InvalidationCounts {
	readonly unchanged: number;
	readonly changed: number;
	readonly new: number;
	readonly deleted: number;
	readonly configWidened: number;
	readonly total: number;
}
