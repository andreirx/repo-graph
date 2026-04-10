/**
 * Environment dependency model — env vars consumed by surfaces/modules.
 *
 * An env dependency is a fact: "this file reads env var X."
 * Linked to project surfaces via file ownership → module → surface.
 *
 * These are high-value, slow-changing architectural facts that tell
 * an agent what runtime configuration a surface depends on.
 *
 * Zero external dependencies. Pure domain model.
 */

// ── Env access kind ────────────────────────────────────────────────

/**
 * How the env var is accessed.
 */
export type EnvAccessKind =
	| "required"    // accessed without fallback (will crash if missing)
	| "optional"    // accessed with default/fallback
	| "unknown";    // can't determine required vs optional

/**
 * Which language pattern was used.
 */
export type EnvAccessPattern =
	| "process_env_dot"        // process.env.X
	| "process_env_bracket"    // process.env["X"]
	| "process_env_destructure" // const { X } = process.env
	| "os_environ"             // os.environ["X"] or os.environ.get("X")
	| "os_getenv"              // os.getenv("X")
	| "std_env_var"            // std::env::var("X")
	| "system_getenv"          // System.getenv("X")
	| "c_getenv"               // getenv("X")
	| "dotenv_reference";      // referenced in .env file

// ── Detected env dependency ────────────────────────────────────────

/**
 * A detected env var access in a source file.
 * Lightweight intermediate type — the orchestrator enriches this
 * with surface/module linkage.
 */
export interface DetectedEnvDependency {
	/** The env var name (e.g., "DATABASE_URL", "PORT"). */
	readonly varName: string;
	/** How it's accessed. */
	readonly accessKind: EnvAccessKind;
	/** Which pattern matched. */
	readonly accessPattern: EnvAccessPattern;
	/** Repo-relative path to the source file. */
	readonly filePath: string;
	/** Line number where the access was found. */
	readonly lineNumber: number;
	/** Default value if detectable (e.g., from || "fallback" or .get("X", "default")). */
	readonly defaultValue: string | null;
	/** Confidence in this detection. */
	readonly confidence: number;
}

// ── Persisted env dependency (identity) ───────────────────────────

/**
 * One env var dependency for a surface.
 * Identity: (snapshot_uid, project_surface_uid, env_name).
 * Deduped: one row per surface/env var, regardless of how many
 * files access it.
 */
export interface SurfaceEnvDependency {
	readonly surfaceEnvDependencyUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly projectSurfaceUid: string;
	readonly envName: string;
	/** Aggregate access kind: required if ANY access is required. */
	readonly accessKind: EnvAccessKind;
	/** Default value from the first occurrence that has one. */
	readonly defaultValue: string | null;
	readonly confidence: number;
	readonly metadataJson: string | null;
}

// ── Persisted env evidence (per-occurrence) ───────────────────────

/**
 * One source-file occurrence of an env var access.
 * Multiple files in the same surface may access the same env var.
 */
export interface SurfaceEnvEvidence {
	readonly surfaceEnvEvidenceUid: string;
	readonly surfaceEnvDependencyUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly sourceFilePath: string;
	readonly lineNumber: number;
	readonly accessPattern: EnvAccessPattern;
	readonly confidence: number;
	readonly metadataJson: string | null;
}
