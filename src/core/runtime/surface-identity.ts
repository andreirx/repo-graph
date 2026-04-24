/**
 * Surface identity computation — pure functions.
 *
 * This module owns the identity policy for project surfaces:
 * - Canonical JSON serialization for deterministic hashing
 * - Source-specific identity computation
 * - Stable surface key generation (snapshot-independent)
 * - Project surface UID generation (snapshot-scoped)
 * - Evidence UID generation
 *
 * Storage persists and enforces; core computes. This module is core.
 *
 * Zero external dependencies except uuid. Pure functions only.
 */

import { v5 as uuidv5 } from "uuid";
import { createHash } from "crypto";
import type { SurfaceEvidenceSourceType } from "./project-surface.js";

// ── Namespaces for UUID5 ───────────────────────────────────────────

/** UUID5 namespace for project_surface_uid generation. */
const SURFACE_UID_NAMESPACE = "a1b2c3d4-e5f6-4a5b-8c7d-9e0f1a2b3c4d";

/** UUID5 namespace for project_surface_evidence_uid generation. */
const EVIDENCE_UID_NAMESPACE = "b2c3d4e5-f6a7-5b6c-9d8e-0f1a2b3c4d5e";

// ── Canonical JSON ─────────────────────────────────────────────────

/**
 * Recursive canonical JSON serialization with sorted keys at all levels.
 *
 * Ensures identical objects produce identical strings regardless of
 * property insertion order. Required for deterministic hashing.
 */
export function canonicalJsonStringify(value: unknown): string {
	if (value === null) {
		return "null";
	}
	if (typeof value === "undefined") {
		return "undefined";
	}
	if (typeof value === "boolean" || typeof value === "number") {
		return JSON.stringify(value);
	}
	if (typeof value === "string") {
		return JSON.stringify(value);
	}
	if (Array.isArray(value)) {
		const elements = value.map((el) => canonicalJsonStringify(el));
		return "[" + elements.join(",") + "]";
	}
	if (typeof value === "object") {
		const obj = value as Record<string, unknown>;
		const keys = Object.keys(obj).sort();
		const pairs = keys.map(
			(k) => JSON.stringify(k) + ":" + canonicalJsonStringify(obj[k]),
		);
		return "{" + pairs.join(",") + "}";
	}
	// Fallback for functions, symbols, etc.
	return "null";
}

// ── Source-specific identity ───────────────────────────────────────

/**
 * Input for computing source-specific identity.
 * Different source types use different fields.
 *
 * Phase 0 fields: binName, frameworkName, scriptName, serviceName, dockerfilePath
 * Phase 1 fields: makefilePath, workspaceName, chartName, infraModulePath
 */
export interface SourceSpecificIdInput {
	sourceType: SurfaceEvidenceSourceType;

	// ── Phase 0 identity fields ────────────────────────────────────────
	/** For package_json_bin, cargo_bin_target: the binary name. */
	binName?: string;
	/** For framework_dependency: the framework name. */
	frameworkName?: string;
	/** For pyproject_scripts: the script/entrypoint name. */
	scriptName?: string;
	/** For docker_compose: the service name. */
	serviceName?: string;
	/** For dockerfile: path to the Dockerfile. */
	dockerfilePath?: string;

	// ── Phase 1 identity fields (reserved) ─────────────────────────────
	/** For makefile_target: path to Makefile. */
	makefilePath?: string;
	/** For makefile_target: target name (required for identity). */
	targetName?: string;
	/** For workspace source types: workspace member name or path. */
	workspaceName?: string;
	/** For helm_chart: chart name. */
	chartName?: string;
	/** For terraform_root, pulumi_yaml: infra module path. */
	infraModulePath?: string;

	// ── Fallbacks ──────────────────────────────────────────────────────
	/** Fallback: entrypoint path if available. */
	entrypointPath?: string;
	/** Ultimate fallback: root path. */
	rootPath: string;
}

/**
 * Error thrown when a required identity field is missing for a source type.
 */
export class MissingIdentityFieldError extends Error {
	constructor(
		public readonly sourceType: string,
		public readonly missingField: string,
		public readonly rootPath: string,
	) {
		super(
			`Source type "${sourceType}" requires "${missingField}" for identity computation, ` +
			`but it was not provided (rootPath: ${rootPath}). ` +
			`This is a detector bug — fix the detector to populate the required field.`,
		);
		this.name = "MissingIdentityFieldError";
	}
}

/**
 * Compute the source-specific identity string for a detected surface.
 *
 * This string, combined with other identity fields, forms the stable
 * surface key. Different source types use different identity fields
 * to prevent collapse of distinct surfaces.
 *
 * Fails closed: if a source type requires a specific identity field
 * (e.g., binName for package_json_bin) and that field is missing,
 * throws MissingIdentityFieldError rather than falling back to defaults
 * that could cause identity collapse.
 *
 * @throws MissingIdentityFieldError if a required identity field is missing
 */
export function computeSourceSpecificId(input: SourceSpecificIdInput): string {
	const { sourceType, rootPath } = input;

	switch (sourceType) {
		case "package_json_bin":
		case "cargo_bin_target": {
			// Bin aliases: binName is REQUIRED to prevent collapse of same-script aliases
			if (!input.binName) {
				throw new MissingIdentityFieldError(sourceType, "binName", rootPath);
			}
			const entrypoint = input.entrypointPath ?? rootPath;
			return `${input.binName}:${entrypoint}`;
		}

		case "docker_compose": {
			// Compose services: serviceName is REQUIRED
			if (!input.serviceName) {
				throw new MissingIdentityFieldError(sourceType, "serviceName", rootPath);
			}
			return input.serviceName;
		}

		case "dockerfile": {
			// Dockerfile: dockerfilePath is REQUIRED
			if (!input.dockerfilePath) {
				throw new MissingIdentityFieldError(sourceType, "dockerfilePath", rootPath);
			}
			return input.dockerfilePath;
		}

		case "framework_dependency": {
			// Framework: frameworkName is REQUIRED
			if (!input.frameworkName) {
				throw new MissingIdentityFieldError(sourceType, "frameworkName", rootPath);
			}
			return `framework:${input.frameworkName}`;
		}

		case "pyproject_scripts": {
			// Python scripts: scriptName is REQUIRED
			if (!input.scriptName) {
				throw new MissingIdentityFieldError(sourceType, "scriptName", rootPath);
			}
			return input.scriptName;
		}

		case "pyproject_entry_points": {
			// Entry points: scriptName is REQUIRED for CLI, optional for library
			// For library surfaces from pyproject, scriptName may not exist
			if (input.scriptName) {
				return input.scriptName;
			}
			// Fall through to entrypoint/rootPath for library surfaces
			return input.entrypointPath ?? rootPath;
		}

		case "package_json_main":
		case "cargo_lib_target": {
			// Library surfaces: entrypoint or root (no required fields)
			return input.entrypointPath ?? rootPath;
		}

		// ── Phase 1 source types (enum reserved, strict validation) ───────

		case "makefile_target": {
			// Makefile targets: both makefilePath AND targetName are REQUIRED
			// Multiple targets from the same Makefile must produce distinct keys
			if (!input.makefilePath) {
				throw new MissingIdentityFieldError(sourceType, "makefilePath", rootPath);
			}
			if (!input.targetName) {
				throw new MissingIdentityFieldError(sourceType, "targetName", rootPath);
			}
			return `${input.makefilePath}:${input.targetName}`;
		}

		case "cmake_target": {
			// CMake targets: deferred, but enum reserved
			// When implemented, will require cmake File API metadata, not source parsing
			if (!input.makefilePath) {
				throw new MissingIdentityFieldError(sourceType, "makefilePath", rootPath);
			}
			return input.makefilePath;
		}

		case "pnpm_workspace":
		case "npm_workspaces":
		case "yarn_workspaces":
		case "lerna_json":
		case "nx_json":
		case "rush_json":
		case "cargo_workspace": {
			// Workspace members: workspaceName is REQUIRED
			if (!input.workspaceName) {
				throw new MissingIdentityFieldError(sourceType, "workspaceName", rootPath);
			}
			return `workspace:${input.workspaceName}`;
		}

		case "helm_chart": {
			// Helm charts: chartName is REQUIRED
			if (!input.chartName) {
				throw new MissingIdentityFieldError(sourceType, "chartName", rootPath);
			}
			return `chart:${input.chartName}`;
		}

		case "terraform_root":
		case "pulumi_yaml": {
			// Infra modules: infraModulePath is REQUIRED
			if (!input.infraModulePath) {
				throw new MissingIdentityFieldError(sourceType, "infraModulePath", rootPath);
			}
			return input.infraModulePath;
		}

		default: {
			// Fallback chain for other source types without strict identity requirements
			// Includes: package_json_scripts, package_json_deps, gradle_application_plugin,
			// compile_commands, tsconfig_outdir
			if (input.entrypointPath) {
				return input.entrypointPath;
			}
			return rootPath;
		}
	}
}

// ── Stable surface key ─────────────────────────────────────────────

/**
 * Input for computing the stable surface key.
 */
export interface StableSurfaceKeyInput {
	repoUid: string;
	/** Module's canonical root path (stable across snapshots). */
	moduleCanonicalRootPath: string;
	surfaceKind: string;
	sourceType: SurfaceEvidenceSourceType;
	sourceSpecificId: string;
}

/**
 * Compute the stable surface key (snapshot-independent identity).
 *
 * This key is stable across reindexes of the same commit. It enables
 * cross-snapshot comparison and deduplication.
 *
 * Returns a 32-character hex string (128 bits).
 */
export function computeStableSurfaceKey(input: StableSurfaceKeyInput): string {
	const identityArray = [
		input.repoUid,
		input.moduleCanonicalRootPath,
		input.surfaceKind,
		input.sourceType,
		input.sourceSpecificId,
	];
	const preimage = canonicalJsonStringify(identityArray);
	const hash = createHash("sha256").update(preimage).digest("hex");
	return hash.slice(0, 32); // 128 bits
}

// ── Project surface UID ────────────────────────────────────────────

/**
 * Compute the project_surface_uid (snapshot-scoped primary key).
 *
 * This UID changes with each snapshot. It includes the stable key
 * plus the snapshot UID.
 */
export function computeProjectSurfaceUid(
	snapshotUid: string,
	stableSurfaceKey: string,
): string {
	const identityArray = [snapshotUid, stableSurfaceKey];
	const preimage = canonicalJsonStringify(identityArray);
	return uuidv5(preimage, SURFACE_UID_NAMESPACE);
}

// ── Evidence UID ───────────────────────────────────────────────────

/**
 * Input for computing evidence UID.
 */
export interface EvidenceUidInput {
	projectSurfaceUid: string;
	sourceType: SurfaceEvidenceSourceType;
	sourcePath: string;
	/** The evidence payload, will be canonically serialized and hashed. */
	payload: Record<string, unknown> | null;
}

/**
 * Compute the project_surface_evidence_uid.
 *
 * Evidence UIDs are deterministic within a snapshot, enabling deduplication
 * of identical evidence from multiple detection paths.
 */
export function computeProjectSurfaceEvidenceUid(
	input: EvidenceUidInput,
): string {
	const payloadHash = input.payload
		? createHash("sha256")
				.update(canonicalJsonStringify(input.payload))
				.digest("hex")
				.slice(0, 32)
		: "null";

	const identityArray = [
		input.projectSurfaceUid,
		input.sourceType,
		input.sourcePath,
		payloadHash,
	];
	const preimage = canonicalJsonStringify(identityArray);
	return uuidv5(preimage, EVIDENCE_UID_NAMESPACE);
}
