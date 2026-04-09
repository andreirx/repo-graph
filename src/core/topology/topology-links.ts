/**
 * Topology link model — persisted relationships that constitute
 * current-repo topology.
 *
 * These are narrow link tables that connect existing entities
 * (module candidates, project surfaces) to config roots and
 * entrypoints. They capture the stable, slow-changing facts
 * that agents need for orientation.
 *
 * Persisted because:
 *   - they are high-value and slow-changing
 *   - they avoid repeated per-query recomputation
 *   - they form the substrate for the daemon's in-memory model
 *
 * Zero external dependencies. Pure domain model.
 */

// ── Config root ────────────────────────────────────────────────────

/**
 * What kind of config file this is.
 */
export type ConfigRootKind =
	| "package_json"
	| "tsconfig_json"
	| "cargo_toml"
	| "build_gradle"
	| "settings_gradle"
	| "pyproject_toml"
	| "compile_commands_json"
	| "dockerfile"
	| "docker_compose"
	| "makefile";

/**
 * A config root that governs a project surface.
 * One surface can be governed by multiple config files
 * (e.g., package.json + tsconfig.json + .env).
 */
export interface SurfaceConfigRoot {
	readonly surfaceConfigRootUid: string;
	readonly projectSurfaceUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	/** Repo-relative path to the config file. */
	readonly configPath: string;
	readonly configKind: ConfigRootKind;
	/** Confidence that this config file governs this surface. */
	readonly confidence: number;
	/** Optional structured metadata (JSON-serialized). */
	readonly metadataJson: string | null;
}

// ── Entrypoint ─────────────────────────────────────────────────────

/**
 * What kind of entrypoint this is.
 */
export type EntrypointKind =
	| "binary"          // CLI binary (package.json bin, Cargo [[bin]])
	| "main_module"     // library main/exports entry (package.json main)
	| "server_start"    // server bootstrap file
	| "handler"         // Lambda/serverless handler
	| "script"          // console_scripts / pyproject scripts
	| "test_entry"      // test runner config entry
	| "build_target";   // Makefile/CMake/Gradle build target

/**
 * An explicit entrypoint for a project surface.
 * One surface can have multiple entrypoints (e.g., a CLI with
 * multiple bin targets, or a library with both CJS and ESM entries).
 */
export interface SurfaceEntrypoint {
	readonly surfaceEntrypointUid: string;
	readonly projectSurfaceUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	/** Repo-relative path to the entrypoint file, if file-based. */
	readonly entrypointPath: string | null;
	/** For Python/module-based entries: module:function target. */
	readonly entrypointTarget: string | null;
	readonly entrypointKind: EntrypointKind;
	/** Human-facing name (binary name, script name, etc.). */
	readonly displayName: string | null;
	/** Confidence in this entrypoint. */
	readonly confidence: number;
	/** Optional structured metadata (JSON-serialized). */
	readonly metadataJson: string | null;
}
