/**
 * Detector graph types — externalized seam detector schema.
 *
 * The detector graph is a flat list of records loaded from a single
 * TOML file (`detectors.toml`). Each record describes one regex
 * pattern in one language for one detector family (env or fs).
 *
 * The schema is the source of truth for detector coverage. Any
 * compile-time runtime (TS now, Rust later) consumes the same file
 * and implements the same hook names. This file is the TS-side
 * schema definition.
 *
 * Pure types only. No I/O. No runtime dependencies.
 */

import type {
	DetectedEnvDependency,
	EnvAccessKind,
	EnvAccessPattern,
} from "../env-dependency.js";
import type {
	DetectedFsMutation,
	MutationKind,
	MutationPattern,
} from "../fs-mutation.js";

// ── Enums ──────────────────────────────────────────────────────────

/** Source language a detector record applies to. */
export type DetectorLanguage = "ts" | "py" | "rs" | "java" | "c";

/** Detector family — what kind of seam this detector reports. */
export type DetectorFamily = "env" | "fs";

/**
 * Allowed regex flag characters. Restricted to a safe subset; the
 * loader rejects any flag character outside this set.
 */
export const ALLOWED_REGEX_FLAGS = new Set(["g", "i", "m", "s", "u", "y"]);

// ── Loaded record shapes ───────────────────────────────────────────

/**
 * Common fields shared by all detector records.
 *
 * `compiledRegex` is set by the loader after parsing — it is not in
 * the TOML source. The loader compiles the regex string once at load
 * time so the walker hot path never recompiles.
 *
 * `singleMatchPerLine` opts the record into legacy single-match-per
 * line semantics: after the first regex hit on a given line, the
 * walker stops iterating that record's regex on that line. This
 * preserves byte-identical behavior for the four legacy detectors
 * that used `line.match()` (no `g`) or `regex.test(line)` followed
 * by a single emission. The flag is optional; default is normal
 * per-call-site iteration with the `g` flag.
 */
interface BaseDetectorRecord {
	readonly language: DetectorLanguage;
	readonly family: DetectorFamily;
	readonly patternId: string;
	readonly regex: string;
	readonly flags: string;
	readonly compiledRegex: RegExp;
	readonly confidence: number;
	/** Optional hook name; the walker dispatches to this hook for special-case logic. */
	readonly hook: string | null;
	/** Stop after first regex match on a line. Default false. */
	readonly singleMatchPerLine: boolean;
}

/**
 * Env-family detector record.
 *
 * `accessKind` and `accessPattern` are static defaults the walker
 * writes into matches when no hook overrides them. They are
 * OPTIONAL when a `hook` is declared and that hook is registered as
 * supplying the corresponding field — see `HookRegistry` and the
 * loader's "must-be-supplied" check. When no hook is present, both
 * fields are required.
 */
export interface EnvDetectorRecord extends BaseDetectorRecord {
	readonly family: "env";
	readonly accessKind: EnvAccessKind | null;
	readonly accessPattern: EnvAccessPattern | null;
}

/**
 * Fs-family detector record.
 *
 * `baseKind` and `basePattern` are static defaults the walker writes
 * into matches when no hook overrides them. They are OPTIONAL when a
 * `hook` is declared and that hook is registered as supplying the
 * corresponding field. When no hook is present, both are required.
 *
 * The `twoEnded` flag signals to the walker / hook that a destination
 * path may be present (rename, copy).
 *
 * `positionDedupGroup` declares this record as a participant in a
 * named overlap group. Records sharing the same non-null group name
 * deduplicate by `(line, match.index)` within a single walker call:
 * if a record matches at a position already claimed by another
 * record in the same group on the same line, the later emission is
 * suppressed. Records with `positionDedupGroup === null` never
 * participate in dedup and always emit.
 *
 * Named overlap groups are the explicit, declared replacement for
 * the legacy hardcoded "all Rust fs records dedupe by position"
 * behavior. The walker is generic and data-driven; the dedup
 * relation lives in the schema, not in the walker.
 *
 * fs-only — env records cannot declare this field.
 */
export interface FsDetectorRecord extends BaseDetectorRecord {
	readonly family: "fs";
	readonly baseKind: MutationKind | null;
	readonly basePattern: MutationPattern | null;
	readonly twoEnded: boolean;
	readonly positionDedupGroup: string | null;
}

export type DetectorRecord = EnvDetectorRecord | FsDetectorRecord;

// ── Loaded graph (post-load index) ─────────────────────────────────

/**
 * The full detector graph after loading and validation.
 *
 * Records are grouped by (family, language) for O(1) walker dispatch.
 * The map key is `${family}:${language}`.
 *
 * `allRecords` preserves the original declaration order from the
 * TOML file — required for deterministic walker output (per slice
 * constraint: walker must preserve table order).
 */
export interface LoadedDetectorGraph {
	readonly allRecords: readonly DetectorRecord[];
	readonly byFamilyLanguage: ReadonlyMap<string, readonly DetectorRecord[]>;
}

// ── Hook context + result types ────────────────────────────────────

/**
 * Context passed to an env hook on each regex match.
 *
 * DTO-shaped per Clean Architecture constraint: no storage, no fs,
 * no side-effect surfaces. The hook receives raw match context plus
 * the base record from the TOML entry, and returns a hook result.
 */
export interface EnvHookContext {
	readonly match: RegExpExecArray;
	readonly line: string;
	readonly lineNumber: number;
	readonly filePath: string;
	readonly baseRecord: EnvDetectorRecord;
}

/**
 * Result returned by an env hook.
 *
 * - `{ skip: true }` suppresses the match entirely.
 * - `{ records: [...] }` emits one or more env detection records.
 *   Each entry is a partial that the walker merges with defaults
 *   (filePath, lineNumber, confidence) from the base record.
 *
 * Returning an empty `records` array is equivalent to skip but more
 * explicit when a hook decides post-match that no records apply.
 */
export type EnvHookResult =
	| { readonly skip: true }
	| { readonly records: readonly EnvHookEmission[] };

/**
 * One env record a hook wants to emit, before walker merges defaults.
 *
 * `varName` is required because every env record is keyed by var name
 * and the hook is the only thing that knows it (capture-group source).
 * The walker rejects emissions without `varName`.
 */
export interface EnvHookEmission {
	readonly varName: string;
	readonly accessKind?: EnvAccessKind;
	readonly accessPattern?: EnvAccessPattern;
	readonly defaultValue?: string | null;
	readonly confidence?: number;
}

export type EnvHook = (ctx: EnvHookContext) => EnvHookResult;

/**
 * Context passed to an fs hook on each regex match.
 */
export interface FsHookContext {
	readonly match: RegExpExecArray;
	readonly line: string;
	readonly lineNumber: number;
	readonly filePath: string;
	readonly baseRecord: FsDetectorRecord;
}

/**
 * Result returned by an fs hook.
 */
export type FsHookResult =
	| { readonly skip: true }
	| { readonly records: readonly FsHookEmission[] };

/**
 * One fs record a hook wants to emit.
 *
 * Unlike env, every field is optional — the walker fills in defaults
 * (mutationKind, mutationPattern) from the base record when the hook
 * does not override them. `targetPath` may be null (dynamic).
 */
export interface FsHookEmission {
	readonly targetPath?: string | null;
	readonly destinationPath?: string | null;
	readonly mutationKind?: MutationKind;
	readonly mutationPattern?: MutationPattern;
	readonly dynamicPath?: boolean;
	readonly confidence?: number;
}

export type FsHook = (ctx: FsHookContext) => FsHookResult;

// ── Hook registry ──────────────────────────────────────────────────

/**
 * Field names an env hook may declare itself responsible for
 * supplying. The loader uses this to validate that every required
 * env output field is reachable, either from a static schema field
 * or from a hook's supplies set.
 *
 * Walker default behavior fills in `varName` from regex capture
 * group 1 when the hook does not supply it. `defaultValue` defaults
 * to null when neither static nor hook supplies it. Only `accessKind`
 * and `accessPattern` are "must-be-supplied" — they have no walker
 * default and the loader requires either a static value or a
 * hook-supplies declaration.
 */
export type EnvSuppliedField =
	| "varName"
	| "accessKind"
	| "accessPattern"
	| "defaultValue";

/**
 * Field names an fs hook may declare itself responsible for supplying.
 *
 * Walker default behavior fills in `targetPath` from regex capture
 * group 1, `destinationPath` from group 2 (when `two_ended` is true),
 * and derives `dynamicPath` from `targetPath === null`. Only
 * `mutationKind` and `mutationPattern` are "must-be-supplied".
 */
export type FsSuppliedField =
	| "targetPath"
	| "destinationPath"
	| "mutationKind"
	| "mutationPattern"
	| "dynamicPath";

/**
 * Required env output fields the loader must verify are reachable
 * for every record. A field is "reachable" if either the record
 * declares a static value for it OR the hook (if any) lists it in
 * `supplies`. This is the load-time check that replaces the previous
 * "always require static" rule.
 */
export const ENV_MUST_BE_SUPPLIED: ReadonlySet<EnvSuppliedField> = new Set([
	"accessKind",
	"accessPattern",
]);

/** Required fs output fields the loader must verify are reachable. */
export const FS_MUST_BE_SUPPLIED: ReadonlySet<FsSuppliedField> = new Set([
	"mutationKind",
	"mutationPattern",
]);

/**
 * One env hook entry in the registry: the function plus a declared
 * set of fields the hook is responsible for supplying. The loader
 * checks `supplies` against `ENV_MUST_BE_SUPPLIED` to ensure every
 * required field is reachable for any record that references this
 * hook without static defaults.
 *
 * The supplies set is the contract surface between the detector
 * graph and the procedural hook layer. It is the same shape the
 * future Rust hook registry will mirror, so drift between runtimes
 * becomes "do both registries declare the same supplies set" — a
 * narrow, auditable question.
 */
export interface EnvHookEntry {
	readonly fn: EnvHook;
	readonly supplies: ReadonlySet<EnvSuppliedField>;
}

export interface FsHookEntry {
	readonly fn: FsHook;
	readonly supplies: ReadonlySet<FsSuppliedField>;
}

/**
 * Named registry of env and fs hook entries. The loader validates
 * that any `hook = "..."` reference in the TOML resolves to a
 * registered entry for the matching family — unknown hook names
 * cause a hard load error per the slice's strict-validation
 * constraint. The loader also uses each entry's `supplies` set to
 * validate that every required output field is reachable.
 */
export interface HookRegistry {
	readonly env: ReadonlyMap<string, EnvHookEntry>;
	readonly fs: ReadonlyMap<string, FsHookEntry>;
}

// ── Walker output types (re-exports for clarity) ───────────────────

export type { DetectedEnvDependency, DetectedFsMutation };
