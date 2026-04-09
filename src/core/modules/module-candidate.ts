/**
 * Module candidate model — core domain types for discovered modules.
 *
 * A module candidate is a machine-derived structural fact: "this
 * directory root is a declared module with this evidence." It is
 * NOT a human declaration (those go in the declarations table) and
 * NOT a node-targeted inference (those go in the inferences table).
 *
 * Module candidates are snapshot-scoped. They coexist with the
 * existing directory-based MODULE nodes — they do not replace them.
 *
 * Identity is anchored by repo-relative root path, not by package
 * name or manifest alias. Names collide; paths don't (within a repo).
 *
 * moduleKey format: `{repoUid}:{canonicalRootPath}:DISCOVERED_MODULE`
 *
 * Zero external dependencies. Pure domain model.
 */

// ── Module candidate ───────────────────────────────────────────────

/**
 * Coarse module kind. Kept intentionally small in slice 1.
 * Finer evidence distinctions live in ModuleCandidateEvidence rows.
 */
export type ModuleKind = "declared";

/**
 * A discovered module candidate — one per detected module root.
 */
export interface ModuleCandidate {
	/** Unique identifier for this candidate row. */
	readonly moduleCandidateUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	/**
	 * Stable identity key, anchored by root path.
	 * Format: `{repoUid}:{canonicalRootPath}:DISCOVERED_MODULE`
	 */
	readonly moduleKey: string;
	readonly moduleKind: ModuleKind;
	/** Repo-relative path of the module root directory. */
	readonly canonicalRootPath: string;
	/** Confidence in this candidate's existence (0.0–1.0). */
	readonly confidence: number;
	/**
	 * Human-facing name derived from manifest (package name, crate name,
	 * Gradle project name, etc.). May be null if no name is declared.
	 */
	readonly displayName: string | null;
	/** Optional structured metadata (JSON-serialized). */
	readonly metadataJson: string | null;
}

// ── Evidence ───────────────────────────────────────────────────────

/**
 * Source type: which build system / manifest type produced this evidence.
 */
export type EvidenceSourceType =
	| "package_json_workspaces"
	| "pnpm_workspace_yaml"
	| "cargo_workspace"
	| "cargo_crate"
	| "gradle_settings"
	| "pyproject_toml";

/**
 * Evidence kind: what the evidence item asserts.
 */
export type EvidenceKind =
	| "workspace_member"    // listed as a member in a workspace manifest
	| "workspace_root"      // is itself the workspace root
	| "crate_root"          // Cargo.toml declares a crate at this path
	| "subproject"          // Gradle settings includes this project
	| "package_root";       // pyproject.toml declares a package here

/**
 * One evidence item supporting a module candidate.
 * A single candidate may have multiple evidence items (e.g., both
 * a package.json workspace entry and a pnpm-workspace.yaml glob match).
 */
export interface ModuleCandidateEvidence {
	readonly evidenceUid: string;
	readonly moduleCandidateUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	/** Which build system produced this evidence. */
	readonly sourceType: EvidenceSourceType;
	/** Path to the manifest file that contained the evidence. */
	readonly sourcePath: string;
	/** What this evidence item asserts. */
	readonly evidenceKind: EvidenceKind;
	/** Confidence of this specific evidence item (0.0–1.0). */
	readonly confidence: number;
	/** Structured payload (JSON-serialized). Contents vary by source type. */
	readonly payloadJson: string | null;
}

// ── File ownership ─────────────────────────────────────────────────

/**
 * How a file was assigned to a module candidate.
 */
export type AssignmentKind =
	| "root_containment"   // file is under the module's root path
	| "explicit_member";   // file is explicitly listed in a manifest

/**
 * One file-to-module ownership assignment.
 * In slice 1, each file has at most one declared owner.
 */
export interface ModuleFileOwnership {
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly fileUid: string;
	readonly moduleCandidateUid: string;
	readonly assignmentKind: AssignmentKind;
	/** Confidence of this assignment (0.0–1.0). */
	readonly confidence: number;
	/** Audit trail for the assignment decision (JSON-serialized). */
	readonly basisJson: string | null;
}

// ── Factory helpers ────────────────────────────────────────────────

/**
 * Build a stable module key from repo UID and root path.
 */
export function buildModuleKey(repoUid: string, canonicalRootPath: string): string {
	return `${repoUid}:${canonicalRootPath}:DISCOVERED_MODULE`;
}
