/**
 * Project surface model — how a module is operationalized.
 *
 * A project surface describes a runnable/deployable/importable aspect
 * of a module. One module can have zero, one, or many surfaces.
 *
 * Examples:
 *   - A package with both a CLI binary and a library API → 2 surfaces
 *   - A backend module with an HTTP service and a worker → 2 surfaces
 *   - A pure library with no entrypoints → 1 surface (library)
 *   - A test fixture package → 1 surface (test_harness)
 *   - An infra directory with Terraform configs → 1 surface (infra_root)
 *
 * Project surfaces are separate from module identity (ModuleCandidate).
 * Module identity says "this root path is a declared module."
 * Project surface says "this module runs as X."
 *
 * Zero external dependencies. Pure domain model.
 */

// ── Surface kind ───────────────────────────────────────────────────

/**
 * How a module is operationalized. Kept deliberately coarse in slice 1.
 */
export type SurfaceKind =
	| "cli"              // command-line tool with a binary entrypoint
	| "web_app"          // browser-facing application (bundler/framework)
	| "backend_service"  // server-side HTTP/RPC service
	| "worker"           // background job processor / queue consumer
	| "library"          // importable package without its own entrypoint
	| "infra_root"       // infrastructure-as-code root (Terraform/Pulumi/Helm)
	| "test_harness";    // test infrastructure (test runner config, fixtures)

/**
 * Build system that owns this surface's compilation/packaging.
 */
export type BuildSystem =
	| "typescript_tsc"
	| "typescript_bundler"  // webpack, vite, esbuild, rollup
	| "cargo"
	| "gradle"
	| "maven"
	| "python_setuptools"
	| "python_poetry"
	| "make"
	| "cmake"
	| "bazel"
	| "unknown";

/**
 * Runtime environment kind.
 */
export type RuntimeKind =
	| "node"
	| "deno"
	| "bun"
	| "browser"
	| "rust_native"
	| "jvm"
	| "python"
	| "native_c_cpp"
	| "container"
	| "lambda"
	| "unknown";

// ── Project surface ────────────────────────────────────────────────

/**
 * One operational surface of a module.
 */
export interface ProjectSurface {
	readonly projectSurfaceUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	/** Link to the owning module candidate. */
	readonly moduleCandidateUid: string;
	readonly surfaceKind: SurfaceKind;
	/** Human-facing name (binary name, service name, etc.). */
	readonly displayName: string | null;
	/** Repo-relative root path of this surface (may equal module root). */
	readonly rootPath: string;
	/** Repo-relative path to the primary entrypoint file, if identifiable. */
	readonly entrypointPath: string | null;
	readonly buildSystem: BuildSystem;
	readonly runtimeKind: RuntimeKind;
	readonly confidence: number;
	/** Structured runtime/build context (JSON-serialized). */
	readonly metadataJson: string | null;

	// ── Identity fields (nullable for legacy compatibility) ────────────
	/**
	 * Primary source type for this surface.
	 * NULL for legacy rows (pre-migration 018). Required for new rows.
	 */
	readonly sourceType: SurfaceEvidenceSourceType | null;
	/**
	 * Source-specific identity string (e.g., "binName:entrypoint", "serviceName").
	 * NULL for legacy rows. Required for new rows.
	 */
	readonly sourceSpecificId: string | null;
	/**
	 * Snapshot-independent stable identity key (128-bit hex).
	 * NULL for legacy rows. Required for new rows.
	 * Used for cross-snapshot comparison and deduplication.
	 */
	readonly stableSurfaceKey: string | null;
}

// ── Evidence ───────────────────────────────────────────────────────

/**
 * Evidence source type for project surface detection.
 *
 * Phase 0 (implemented):
 *   package_json_bin, package_json_main, cargo_bin_target, cargo_lib_target,
 *   pyproject_scripts, pyproject_entry_points, framework_dependency
 *
 * Phase 1 (enum reserved, detectors not implemented):
 *   dockerfile, docker_compose, makefile_target, workspace types
 */
export type SurfaceEvidenceSourceType =
	// ── Package ecosystems (Phase 0) ───────────────────────────────────
	| "package_json_bin"
	| "package_json_scripts"
	| "package_json_main"
	| "package_json_deps"
	| "cargo_bin_target"
	| "cargo_lib_target"
	| "gradle_application_plugin"
	| "pyproject_scripts"
	| "pyproject_entry_points"
	| "framework_dependency"
	// ── Container/service (Phase 1) ────────────────────────────────────
	| "dockerfile"
	| "docker_compose"
	// ── Build systems (Phase 1) ────────────────────────────────────────
	| "makefile_target"
	| "cmake_target"
	// ── Workspace/monorepo (Phase 1) ───────────────────────────────────
	| "pnpm_workspace"
	| "npm_workspaces"
	| "yarn_workspaces"
	| "lerna_json"
	| "nx_json"
	| "rush_json"
	| "cargo_workspace"
	// ── Infra/config (Phase 1) ─────────────────────────────────────────
	| "terraform_root"
	| "pulumi_yaml"
	| "helm_chart"
	// ── Compilation (Phase 1) ──────────────────────────────────────────
	| "compile_commands"
	| "tsconfig_outdir";

/**
 * What the evidence item asserts about the surface.
 *
 * Phase 0 (implemented):
 *   binary_entrypoint, main_export, framework_signal
 *
 * Phase 1 (enum reserved, not yet used):
 *   container_config, workspace_member, workspace_root, etc.
 */
export type SurfaceEvidenceKind =
	// ── Entrypoint/export (Phase 0) ────────────────────────────────────
	| "binary_entrypoint"
	| "script_command"
	| "main_export"
	| "framework_signal"
	// ── Build/deploy (Phase 1) ─────────────────────────────────────────
	| "build_target"
	| "deploy_config"
	| "container_config"
	// ── Workspace (Phase 1) ────────────────────────────────────────────
	| "workspace_root"
	| "workspace_member"
	// ── Infrastructure (Phase 1) ───────────────────────────────────────
	| "infra_config"
	| "infra_module"
	// ── Testing/compilation (Phase 1) ──────────────────────────────────
	| "test_config"
	| "compile_config";

/**
 * One evidence item supporting a project surface.
 */
export interface ProjectSurfaceEvidence {
	readonly projectSurfaceEvidenceUid: string;
	readonly projectSurfaceUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly sourceType: SurfaceEvidenceSourceType;
	readonly sourcePath: string;
	readonly evidenceKind: SurfaceEvidenceKind;
	readonly confidence: number;
	readonly payloadJson: string | null;
}
