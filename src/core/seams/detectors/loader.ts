/**
 * Detector graph loader with strict validation.
 *
 * Loads a TOML detector graph and produces a `LoadedDetectorGraph`
 * ready for walker consumption. The loader is the gate for every
 * malformed declaration: any error is a hard throw at load time, NOT
 * silent coercion at runtime.
 *
 * Hard validation rules (per slice constraint):
 *  - unknown family / language values reject
 *  - missing required fields reject (no defaults for required)
 *  - invalid regex (parse error) rejects
 *  - unknown hook name (not in the supplied registry) rejects
 *  - duplicate (language, family, patternId) rejects
 *  - confidence outside [0, 1] rejects
 *  - regex flags outside the allowed set reject
 *  - unknown top-level keys reject (defense against typos)
 *
 * No I/O: callers pass the TOML source as a string. The loader is
 * pure given a registry. Caller is responsible for reading the file.
 */

import TOML from "@iarna/toml";
import type { EnvAccessKind, EnvAccessPattern } from "../env-dependency.js";
import type { MutationKind, MutationPattern } from "../fs-mutation.js";
import {
	ALLOWED_REGEX_FLAGS,
	type DetectorFamily,
	type DetectorLanguage,
	type DetectorRecord,
	ENV_MUST_BE_SUPPLIED,
	type EnvDetectorRecord,
	type EnvSuppliedField,
	FS_MUST_BE_SUPPLIED,
	type FsDetectorRecord,
	type FsSuppliedField,
	type HookRegistry,
	type LoadedDetectorGraph,
} from "./types.js";

// ── Allowed enum value sets ────────────────────────────────────────

const ALLOWED_LANGUAGES: ReadonlySet<DetectorLanguage> = new Set([
	"ts",
	"py",
	"rs",
	"java",
	"c",
]);

const ALLOWED_FAMILIES: ReadonlySet<DetectorFamily> = new Set(["env", "fs"]);

const ALLOWED_ENV_ACCESS_KINDS: ReadonlySet<EnvAccessKind> = new Set([
	"required",
	"optional",
	"unknown",
]);

const ALLOWED_ENV_ACCESS_PATTERNS: ReadonlySet<EnvAccessPattern> = new Set([
	"process_env_dot",
	"process_env_bracket",
	"process_env_destructure",
	"os_environ",
	"os_getenv",
	"std_env_var",
	"system_getenv",
	"c_getenv",
	"dotenv_reference",
]);

const ALLOWED_MUTATION_KINDS: ReadonlySet<MutationKind> = new Set([
	"write_file",
	"append_file",
	"delete_path",
	"create_dir",
	"rename_path",
	"copy_path",
	"chmod_path",
	"create_temp",
]);

const ALLOWED_MUTATION_PATTERNS: ReadonlySet<MutationPattern> = new Set([
	"fs_write_file",
	"fs_append_file",
	"fs_unlink",
	"fs_rm",
	"fs_mkdir",
	"fs_create_write_stream",
	"fs_rename",
	"fs_copy_file",
	"fs_chmod",
	"py_open_write",
	"py_open_append",
	"py_os_remove",
	"py_os_unlink",
	"py_shutil_rmtree",
	"py_os_mkdir",
	"py_os_makedirs",
	"py_pathlib_write",
	"py_pathlib_mkdir",
	"py_tempfile",
	"rust_fs_write",
	"rust_fs_remove_file",
	"rust_fs_remove_dir_all",
	"rust_fs_create_dir",
	"rust_fs_rename",
	"rust_fs_copy",
	"java_files_write",
	"java_files_delete",
	"java_files_create_directory",
	"java_file_output_stream",
	"c_fopen_write",
	"c_fopen_append",
	"c_unlink",
	"c_remove",
	"c_rmdir",
	"c_mkdir",
]);

// ── Allowed top-level field names per family ──────────────────────

const COMMON_FIELDS = new Set([
	"language",
	"family",
	"pattern_id",
	"regex",
	"flags",
	"confidence",
	"hook",
	"single_match_per_line",
]);

const ENV_FIELDS = new Set([
	...COMMON_FIELDS,
	"access_kind",
	"access_pattern",
]);

const FS_FIELDS = new Set([
	...COMMON_FIELDS,
	"base_kind",
	"base_pattern",
	"two_ended",
	"position_dedup_group",
]);

// ── Loader entry ──────────────────────────────────────────────────

/**
 * Parse and validate a TOML detector graph string.
 *
 * @param tomlSource raw TOML text
 * @param registry   the hook registry; unknown hook references reject
 * @returns          a loaded, validated, indexed detector graph
 * @throws           DetectorLoadError on any validation failure
 */
export function loadDetectorGraph(
	tomlSource: string,
	registry: HookRegistry,
): LoadedDetectorGraph {
	let parsed: Record<string, unknown>;
	try {
		parsed = TOML.parse(tomlSource) as Record<string, unknown>;
	} catch (err) {
		throw new DetectorLoadError(
			`TOML parse error: ${(err as Error).message}`,
		);
	}

	const topKeys = Object.keys(parsed);
	for (const k of topKeys) {
		if (k !== "detector") {
			throw new DetectorLoadError(
				`Unknown top-level key "${k}". Only "detector" is allowed.`,
			);
		}
	}

	// Missing `detector` key is treated as an empty graph (zero records).
	// Present-but-not-an-array is a hard error (the only legal shape for
	// `detector` is an array of tables).
	const rawDetectors =
		parsed.detector === undefined ? [] : parsed.detector;
	if (!Array.isArray(rawDetectors)) {
		throw new DetectorLoadError(
			'Top-level "detector" must be an array of tables ([[detector]]).',
		);
	}

	const records: DetectorRecord[] = [];
	const seenKeys = new Set<string>();

	for (let i = 0; i < rawDetectors.length; i++) {
		const raw = rawDetectors[i];
		if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
			throw new DetectorLoadError(
				`detector[${i}] must be a table.`,
			);
		}
		const record = validateRecord(raw as Record<string, unknown>, i, registry);

		// Duplicate guard within (language, family, patternId).
		const dupKey = `${record.language}:${record.family}:${record.patternId}`;
		if (seenKeys.has(dupKey)) {
			throw new DetectorLoadError(
				`detector[${i}] (${record.patternId}): duplicate (language=${record.language}, family=${record.family}, pattern_id=${record.patternId}). Each combination must be unique.`,
			);
		}
		seenKeys.add(dupKey);

		records.push(record);
	}

	// Build the (family, language) index, preserving original order
	// within each bucket (declaration order — required for
	// deterministic walker output).
	const byFamilyLanguage = new Map<string, DetectorRecord[]>();
	for (const r of records) {
		const key = `${r.family}:${r.language}`;
		const list = byFamilyLanguage.get(key);
		if (list) {
			list.push(r);
		} else {
			byFamilyLanguage.set(key, [r]);
		}
	}

	return {
		allRecords: records,
		byFamilyLanguage,
	};
}

// ── Per-record validation ─────────────────────────────────────────

function validateRecord(
	raw: Record<string, unknown>,
	index: number,
	registry: HookRegistry,
): DetectorRecord {
	const ctx = `detector[${index}]`;

	// language (required)
	const language = requireString(raw, "language", ctx);
	if (!ALLOWED_LANGUAGES.has(language as DetectorLanguage)) {
		throw new DetectorLoadError(
			`${ctx}: unknown language "${language}". Allowed: ${[...ALLOWED_LANGUAGES].join(", ")}`,
		);
	}

	// family (required)
	const family = requireString(raw, "family", ctx);
	if (!ALLOWED_FAMILIES.has(family as DetectorFamily)) {
		throw new DetectorLoadError(
			`${ctx}: unknown family "${family}". Allowed: ${[...ALLOWED_FAMILIES].join(", ")}`,
		);
	}

	// pattern_id (required)
	const patternId = requireString(raw, "pattern_id", ctx);

	// regex (required)
	const regexSource = requireString(raw, "regex", ctx);

	// flags (optional, default "g")
	const flags = optionalString(raw, "flags", ctx) ?? "g";
	for (const f of flags) {
		if (!ALLOWED_REGEX_FLAGS.has(f)) {
			throw new DetectorLoadError(
				`${ctx} (${patternId}): unknown regex flag "${f}". Allowed: ${[...ALLOWED_REGEX_FLAGS].join("")}`,
			);
		}
	}

	// confidence (required, 0..1)
	const confidence = requireNumber(raw, "confidence", ctx);
	if (confidence < 0 || confidence > 1 || !Number.isFinite(confidence)) {
		throw new DetectorLoadError(
			`${ctx} (${patternId}): confidence must be a finite number in [0, 1], got ${confidence}`,
		);
	}

	// hook (optional)
	const hook = optionalString(raw, "hook", ctx);

	// single_match_per_line (optional, default false)
	const singleMatchPerLine =
		optionalBoolean(raw, "single_match_per_line", ctx) ?? false;

	// Compile regex now to fail fast on syntax errors.
	let compiledRegex: RegExp;
	try {
		compiledRegex = new RegExp(regexSource, flags);
	} catch (err) {
		throw new DetectorLoadError(
			`${ctx} (${patternId}): regex compile error: ${(err as Error).message}`,
		);
	}

	if (family === "env") {
		// Field allowlist enforcement (defense against typos).
		for (const k of Object.keys(raw)) {
			if (!ENV_FIELDS.has(k)) {
				throw new DetectorLoadError(
					`${ctx} (${patternId}): unknown field "${k}" for env family. Allowed: ${[...ENV_FIELDS].join(", ")}`,
				);
			}
		}

		// Static fields are now optional. They become "the walker
		// default" when present, and can be omitted entirely when a
		// hook supplies them. The must-be-supplied check below
		// validates that every required field is reachable.
		const accessKindStr = optionalString(raw, "access_kind", ctx);
		if (
			accessKindStr !== null &&
			!ALLOWED_ENV_ACCESS_KINDS.has(accessKindStr as EnvAccessKind)
		) {
			throw new DetectorLoadError(
				`${ctx} (${patternId}): unknown access_kind "${accessKindStr}". Allowed: ${[...ALLOWED_ENV_ACCESS_KINDS].join(", ")}`,
			);
		}

		const accessPatternStr = optionalString(raw, "access_pattern", ctx);
		if (
			accessPatternStr !== null &&
			!ALLOWED_ENV_ACCESS_PATTERNS.has(accessPatternStr as EnvAccessPattern)
		) {
			throw new DetectorLoadError(
				`${ctx} (${patternId}): unknown access_pattern "${accessPatternStr}". Allowed: ${[...ALLOWED_ENV_ACCESS_PATTERNS].join(", ")}`,
			);
		}

		// Validate hook name (if present) against env registry.
		const hookEntry = hook !== null ? registry.env.get(hook) : null;
		if (hook !== null && hookEntry === undefined) {
			throw new DetectorLoadError(
				`${ctx} (${patternId}): unknown env hook "${hook}". Registered: ${[...registry.env.keys()].join(", ") || "(none)"}`,
			);
		}

		// Must-be-supplied check: every required env output field
		// must be reachable, either via a static schema field on
		// this record or via the referenced hook's `supplies` set.
		// This is the load-time replacement for "always require
		// static fields" — runtime validation is unnecessary because
		// the loader proves at parse time that the merged record
		// will always have the required fields.
		const staticSupplied = new Set<EnvSuppliedField>();
		if (accessKindStr !== null) staticSupplied.add("accessKind");
		if (accessPatternStr !== null) staticSupplied.add("accessPattern");
		const hookSupplied = hookEntry?.supplies ?? new Set<EnvSuppliedField>();
		for (const required of ENV_MUST_BE_SUPPLIED) {
			if (!staticSupplied.has(required) && !hookSupplied.has(required)) {
				throw new DetectorLoadError(
					`${ctx} (${patternId}): required env field "${required}" is neither declared statically nor supplied by hook "${hook ?? "(none)"}". Either set "${toSnake(required)}" in the record or register a hook whose supplies set includes "${required}".`,
				);
			}
		}

		const record: EnvDetectorRecord = {
			language: language as DetectorLanguage,
			family: "env",
			patternId,
			regex: regexSource,
			flags,
			compiledRegex,
			confidence,
			hook,
			singleMatchPerLine,
			accessKind: accessKindStr as EnvAccessKind | null,
			accessPattern: accessPatternStr as EnvAccessPattern | null,
		};
		return record;
	}

	// fs family
	for (const k of Object.keys(raw)) {
		if (!FS_FIELDS.has(k)) {
			throw new DetectorLoadError(
				`${ctx} (${patternId}): unknown field "${k}" for fs family. Allowed: ${[...FS_FIELDS].join(", ")}`,
			);
		}
	}

	// Static fields are now optional. They become "the walker
	// default" when present, and can be omitted when a hook supplies
	// them. The must-be-supplied check below validates reachability.
	const baseKindStr = optionalString(raw, "base_kind", ctx);
	if (
		baseKindStr !== null &&
		!ALLOWED_MUTATION_KINDS.has(baseKindStr as MutationKind)
	) {
		throw new DetectorLoadError(
			`${ctx} (${patternId}): unknown base_kind "${baseKindStr}". Allowed: ${[...ALLOWED_MUTATION_KINDS].join(", ")}`,
		);
	}

	const basePatternStr = optionalString(raw, "base_pattern", ctx);
	if (
		basePatternStr !== null &&
		!ALLOWED_MUTATION_PATTERNS.has(basePatternStr as MutationPattern)
	) {
		throw new DetectorLoadError(
			`${ctx} (${patternId}): unknown base_pattern "${basePatternStr}". Allowed: ${[...ALLOWED_MUTATION_PATTERNS].join(", ")}`,
		);
	}

	const twoEnded = optionalBoolean(raw, "two_ended", ctx) ?? false;

	// Optional named overlap group. Default is absence (null), not
	// empty string. Empty strings are rejected by optionalString.
	const positionDedupGroup = optionalString(raw, "position_dedup_group", ctx);

	const fsHookEntry = hook !== null ? registry.fs.get(hook) : null;
	if (hook !== null && fsHookEntry === undefined) {
		throw new DetectorLoadError(
			`${ctx} (${patternId}): unknown fs hook "${hook}". Registered: ${[...registry.fs.keys()].join(", ") || "(none)"}`,
		);
	}

	// Must-be-supplied check for fs: every required output field
	// must be reachable from either the static schema or the hook's
	// supplies set.
	const fsStaticSupplied = new Set<FsSuppliedField>();
	if (baseKindStr !== null) fsStaticSupplied.add("mutationKind");
	if (basePatternStr !== null) fsStaticSupplied.add("mutationPattern");
	const fsHookSupplied = fsHookEntry?.supplies ?? new Set<FsSuppliedField>();
	for (const required of FS_MUST_BE_SUPPLIED) {
		if (!fsStaticSupplied.has(required) && !fsHookSupplied.has(required)) {
			// Map output-field name back to its TOML schema name for
			// the error message: mutationKind ↔ base_kind,
			// mutationPattern ↔ base_pattern.
			const tomlFieldName =
				required === "mutationKind"
					? "base_kind"
					: required === "mutationPattern"
						? "base_pattern"
						: toSnake(required);
			throw new DetectorLoadError(
				`${ctx} (${patternId}): required fs field "${required}" is neither declared statically nor supplied by hook "${hook ?? "(none)"}". Either set "${tomlFieldName}" in the record or register a hook whose supplies set includes "${required}".`,
			);
		}
	}

	const record: FsDetectorRecord = {
		language: language as DetectorLanguage,
		family: "fs",
		patternId,
		regex: regexSource,
		flags,
		compiledRegex,
		confidence,
		hook,
		singleMatchPerLine,
		baseKind: baseKindStr as MutationKind | null,
		basePattern: basePatternStr as MutationPattern | null,
		twoEnded,
		positionDedupGroup,
	};
	return record;
}

/** camelCase → snake_case for error messages that show TOML field names. */
function toSnake(camel: string): string {
	return camel.replace(/[A-Z]/g, (m) => `_${m.toLowerCase()}`);
}

// ── Field-extraction helpers (no silent coercion) ────────────────

function requireString(
	raw: Record<string, unknown>,
	field: string,
	ctx: string,
): string {
	const v = raw[field];
	if (v === undefined) {
		throw new DetectorLoadError(`${ctx}: missing required field "${field}"`);
	}
	if (typeof v !== "string") {
		throw new DetectorLoadError(
			`${ctx}: field "${field}" must be a string, got ${typeOf(v)}`,
		);
	}
	if (v.length === 0) {
		throw new DetectorLoadError(`${ctx}: field "${field}" must not be empty`);
	}
	return v;
}

function optionalString(
	raw: Record<string, unknown>,
	field: string,
	ctx: string,
): string | null {
	const v = raw[field];
	if (v === undefined) return null;
	if (typeof v !== "string") {
		throw new DetectorLoadError(
			`${ctx}: field "${field}" must be a string, got ${typeOf(v)}`,
		);
	}
	if (v.length === 0) {
		throw new DetectorLoadError(`${ctx}: field "${field}" must not be empty`);
	}
	return v;
}

function requireNumber(
	raw: Record<string, unknown>,
	field: string,
	ctx: string,
): number {
	const v = raw[field];
	if (v === undefined) {
		throw new DetectorLoadError(`${ctx}: missing required field "${field}"`);
	}
	if (typeof v !== "number") {
		throw new DetectorLoadError(
			`${ctx}: field "${field}" must be a number, got ${typeOf(v)}`,
		);
	}
	return v;
}

function optionalBoolean(
	raw: Record<string, unknown>,
	field: string,
	ctx: string,
): boolean | null {
	const v = raw[field];
	if (v === undefined) return null;
	if (typeof v !== "boolean") {
		throw new DetectorLoadError(
			`${ctx}: field "${field}" must be a boolean, got ${typeOf(v)}`,
		);
	}
	return v;
}

function typeOf(v: unknown): string {
	if (v === null) return "null";
	if (Array.isArray(v)) return "array";
	return typeof v;
}

// ── Error type ────────────────────────────────────────────────────

/**
 * Thrown by `loadDetectorGraph` on any validation failure.
 *
 * The message identifies the failing record by index and pattern_id
 * (when available) so callers and authors can locate the offending
 * declaration in the source TOML quickly.
 */
export class DetectorLoadError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "DetectorLoadError";
	}
}
