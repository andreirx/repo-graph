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
 * No npm dependencies. Uses Node standard library only (crypto for hashing).
 */

import { createHash } from "node:crypto";

// ── Module candidate ───────────────────────────────────────────────

/**
 * Coarse module kind. Three-tier confidence ladder.
 *
 * Precedence order (for ownership tiebreaks):
 *   declared > operational > inferred
 *
 * See docs/architecture/module-discovery-layers.txt for the full contract.
 */
export type ModuleKind = "declared" | "operational" | "inferred";

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
 *
 * Layer 1 (declared): manifest-based sources
 * Layer 2 (operational): surface promotion sources
 * Layer 3 (inferred): build-system / structural / graph sources
 */
export type EvidenceSourceType =
	// Layer 1: declared module evidence
	| "package_json_workspaces"
	| "pnpm_workspace_yaml"
	| "cargo_workspace"
	| "cargo_crate"
	| "gradle_settings"
	| "pyproject_toml"
	// Layer 2: operational module evidence (surface promotion)
	| "surface_promotion_cli"
	| "surface_promotion_service"
	| "surface_promotion_web_app"
	| "surface_promotion_worker"
	// Layer 3: build-system derived evidence
	| "kbuild"
	// Layer 3: directory-structure inferred evidence
	| "directory_structure";

/**
 * Evidence kind: what the evidence item asserts.
 *
 * Layer 1: manifest-derived assertions
 * Layer 2: surface-promotion assertions
 * Layer 3: build-system / structural / graph assertions
 */
export type EvidenceKind =
	// Layer 1: declared module evidence
	| "workspace_member"       // listed as a member in a workspace manifest
	| "workspace_root"         // is itself the workspace root
	| "crate_root"             // Cargo.toml declares a crate at this path
	| "subproject"             // Gradle settings includes this project
	| "package_root"           // pyproject.toml declares a package here
	// Layer 2: operational module evidence
	| "operational_entrypoint" // promoted from unattached surface
	// Layer 3: build-system evidence
	| "kbuild_subdir"          // obj-y/obj-m directory assignment in Kbuild/Makefile
	// Layer 3: directory-structure evidence
	| "directory_pattern";     // immediate child of anchored root with file threshold

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

// ── Module discovery diagnostics ───────────────────────────────────

/**
 * Source type for diagnostics — which detector produced this diagnostic.
 * Maps to EvidenceSourceType values that produce diagnostics.
 */
export type DiagnosticSourceType = "kbuild";

/**
 * Diagnostic kind — what was skipped or flagged.
 * Kbuild kinds match the detector's internal diagnostic taxonomy.
 */
export type DiagnosticKind =
	// Kbuild diagnostic kinds
	| "skipped_conditional"        // ifeq/ifdef/etc block start/end
	| "skipped_inside_conditional" // obj-y/m assignment inside conditional
	| "skipped_config_gated"       // obj-$(CONFIG_...) assignment
	| "skipped_variable"           // unexpanded variable in assignment
	| "malformed_assignment";      // unparseable syntax

/**
 * Diagnostic severity.
 * - info: intentionally out-of-scope evidence (D1 exclusions)
 * - warning: malformed or ambiguous input worth investigating
 */
export type DiagnosticSeverity = "info" | "warning";

/**
 * Persisted module discovery diagnostic.
 * Audit evidence for why certain build-system patterns were not
 * converted to module candidates.
 */
export interface ModuleDiscoveryDiagnostic {
	/** Unique identifier for this diagnostic row. */
	readonly diagnosticUid: string;
	readonly snapshotUid: string;
	/** Which detector produced this diagnostic. */
	readonly sourceType: DiagnosticSourceType;
	/** Diagnostic category. */
	readonly diagnosticKind: DiagnosticKind;
	/** Repo-relative path to the source file. */
	readonly filePath: string;
	/** Line number (1-based) if applicable. */
	readonly line: number | null;
	/** Raw text that triggered the diagnostic (truncated). */
	readonly rawText: string | null;
	/** Human-readable description. */
	readonly message: string;
	/** Severity level. */
	readonly severity: DiagnosticSeverity;
	/** Optional structured metadata (JSON-serialized). */
	readonly metadataJson: string | null;
}

/**
 * Input for inserting module discovery diagnostics.
 * Excludes diagnosticUid (computed by storage layer).
 */
export interface ModuleDiscoveryDiagnosticInput {
	readonly snapshotUid: string;
	readonly sourceType: DiagnosticSourceType;
	readonly diagnosticKind: DiagnosticKind;
	readonly filePath: string;
	readonly line: number | null;
	readonly rawText: string | null;
	readonly message: string;
	readonly severity: DiagnosticSeverity;
	readonly metadataJson: string | null;
}

// ── Factory helpers ────────────────────────────────────────────────

/**
 * Build a stable module key from repo UID and root path.
 */
export function buildModuleKey(repoUid: string, canonicalRootPath: string): string {
	return `${repoUid}:${canonicalRootPath}:DISCOVERED_MODULE`;
}

/**
 * Build a deterministic diagnostic UID.
 * SHA-256 hash over: snapshotUid, sourceType, diagnosticKind, filePath, line, rawText.
 * This ensures the same diagnostic doesn't get inserted twice.
 *
 * Uses 128-bit truncation (32 hex chars) for collision safety on large repos.
 * Birthday collision probability ~50% at 2^64 items — effectively impossible.
 */
export function buildDiagnosticUid(input: ModuleDiscoveryDiagnosticInput): string {
	// Use JSON.stringify for unambiguous field encoding.
	// Raw `|` join is ambiguous when fields contain `|` characters.
	const parts = [
		input.snapshotUid,
		input.sourceType,
		input.diagnosticKind,
		input.filePath,
		input.line,
		input.rawText,
	];
	const str = JSON.stringify(parts);
	const hash = createHash("sha256").update(str).digest("hex");
	// Truncate to 128 bits (32 hex chars) for reasonable UID length
	return `diag:${hash.slice(0, 32)}`;
}
