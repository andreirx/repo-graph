/**
 * Canonical version manifest.
 *
 * Single source of truth for all component versions.
 * All extractors, the indexer, the CLI, and snapshot provenance
 * read from this module. No hardcoded version strings elsewhere.
 *
 * Two kinds of version:
 *   - Build versions: identify the specific build of a component.
 *     Used for auditing ("what binary produced this edge?").
 *   - Compatibility versions: identify the data semantics.
 *     Used for comparison ("are these two snapshots comparable?").
 *
 * See docs/architecture/versioning-model.txt for the full design.
 */

// ── Build versions ──────────────────────────────────────────────────

/** CLI/application version. Shown in --version output. */
export const CLI_VERSION = "0.2.0";

/** Extractor build versions. Keyed by language. */
export const EXTRACTOR_VERSIONS = {
	typescript: "ts-core:0.2.0",
} as const;

/** Indexer build version. */
export const INDEXER_VERSION = "indexer:0.2.0";

/** Manifest extractor build version. */
export const MANIFEST_EXTRACTOR_VERSION = "manifest-extractor:0.1.0";

// ── Data compatibility versions ─────────────────────────────────────
// These are bumped only when the output semantics change.
// A patch or refactoring that produces identical data does NOT bump these.

/** Minimum schema migration version required to read snapshots
 *  produced by this toolchain. */
export const SCHEMA_COMPAT = 1;

/** Extraction semantics version. Bumped when node/edge output changes:
 *  new node kinds, changed resolution rules, binding model changes.
 *  History:
 *    1 — v1 baseline (syntax-only, no binding)
 *    2 — v1.5 (interface members, receiver binding, scope model,
 *              class-context this.method rewriting) */
export const EXTRACTION_SEMANTICS = 2;

/** Stable key identity format version. Bumped when the stable_key
 *  string format changes. Declarations reference stable keys, so
 *  a format change requires declaration migration.
 *  History:
 *    1 — <repo>:<path>#<name>:SYMBOL (retired, caused companion collisions)
 *    2 — <repo>:<path>#<name>:SYMBOL:<subtype> */
export const STABLE_KEY_FORMAT = 2;

// ── Snapshot toolchain provenance ───────────────────────────────────

/** Build the toolchain_json object for a snapshot. */
export function buildToolchainJson(): ToolchainJson {
	return {
		schema_compat: SCHEMA_COMPAT,
		extraction_semantics: EXTRACTION_SEMANTICS,
		stable_key_format: STABLE_KEY_FORMAT,
		extractor_versions: { ...EXTRACTOR_VERSIONS },
		indexer_version: INDEXER_VERSION,
		measurement_semantics: {
			"ast-metrics": 1,
			"domain-versions": 1,
			"git-churn": 1,
			coverage: 1,
		},
		measurement_versions: {
			"ast-metrics": "ast-metrics:0.1.0",
			"domain-versions": "manifest-extractor:0.1.0",
			"git-churn": "git-churn:0.1.0",
			coverage: "coverage-import:0.1.0",
		},
	};
}

/** Build the authored_basis_json object for a declaration. */
export function buildAuthoredBasisJson(): AuthoredBasisJson {
	return {
		stable_key_format: STABLE_KEY_FORMAT,
		schema_compat: SCHEMA_COMPAT,
	};
}

// ── Types ───────────────────────────────────────────────────────────

export interface ToolchainJson {
	schema_compat: number;
	extraction_semantics: number;
	stable_key_format: number;
	extractor_versions: Record<string, string>;
	indexer_version: string;
	measurement_semantics: Record<string, number>;
	measurement_versions: Record<string, string>;
}

export interface AuthoredBasisJson {
	stable_key_format: number;
	schema_compat: number;
}
