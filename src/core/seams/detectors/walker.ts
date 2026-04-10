/**
 * Generic detector graph walker.
 *
 * Consumes a `LoadedDetectorGraph` plus a hook registry and produces
 * `DetectedEnvDependency[]` or `DetectedFsMutation[]` for one source
 * file. The walker is the procedural counterpart to the declarative
 * detector graph: it iterates lines, runs each record's compiled
 * regex, dispatches to hooks when present, and merges hook output
 * with the static record's base fields.
 *
 * Walker default behavior (must match types.ts comments exactly):
 *  - env varName ← regex capture group 1 (when hook does not supply)
 *  - fs targetPath ← regex capture group 1 (when hook does not supply)
 *  - fs destinationPath ← regex capture group 2 (when two_ended)
 *  - fs dynamicPath ← derived from targetPath === null
 *  - confidence ← record.confidence
 *  - filePath / lineNumber ← walker context
 *
 * Env-family deduplication: the walker tracks `(filePath, varName)`
 * across all matches in a single call and skips records whose key
 * has already been emitted. First-occurrence-wins, matching the
 * existing `deduplicateByVarAndFile` behavior exactly.
 *
 * Fs-family: no deduplication. All matches accumulate in declaration
 * order then line order.
 *
 * Pure function. No I/O. No external dependencies beyond the loaded
 * graph and registry passed in.
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
import type {
	DetectorLanguage,
	EnvDetectorRecord,
	EnvHookContext,
	FsDetectorRecord,
	FsHookContext,
	HookRegistry,
	LoadedDetectorGraph,
} from "./types.js";

// ── Public entry points ───────────────────────────────────────────

/**
 * Walk env-family detectors for one source file. Returns the same
 * shape as the legacy `detectEnvAccesses` so callers can swap to the
 * walker without touching the surrounding pipeline.
 *
 * Caller is responsible for selecting the language from the file
 * path and for passing the comment-masked content. The walker only
 * consumes records and content.
 */
export function walkEnvDetectors(
	graph: LoadedDetectorGraph,
	registry: HookRegistry,
	language: DetectorLanguage,
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const records = graph.byFamilyLanguage.get(`env:${language}`);
	if (!records || records.length === 0) return [];

	const results: DetectedEnvDependency[] = [];
	const seen = new Set<string>();
	const lines = content.split("\n");

	for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
		const line = lines[lineIdx];
		const lineNumber = lineIdx + 1;

		for (const r of records) {
			if (r.family !== "env") continue;
			const regex = r.compiledRegex;
			regex.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = regex.exec(line)) !== null) {
				const emissions = emitEnvFromMatch(r, m, line, lineNumber, filePath, registry);
				for (const e of emissions) {
					const key = `${filePath}:${e.varName}`;
					if (seen.has(key)) continue;
					seen.add(key);
					results.push(e);
				}
				// Legacy single-match-per-line opt-in: stop after
				// the first regex hit on this line for this record.
				// Preserves byte-identical behavior of detectors
				// that originally used `line.match(...)` (no `g`).
				if (r.singleMatchPerLine) break;
				// Guard against zero-width-match infinite loops on
				// regexes that can match empty strings.
				if (m[0].length === 0) regex.lastIndex++;
			}
		}
	}

	return results;
}

/**
 * Walk fs-family detectors for one source file.
 */
export function walkFsDetectors(
	graph: LoadedDetectorGraph,
	registry: HookRegistry,
	language: DetectorLanguage,
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const records = graph.byFamilyLanguage.get(`fs:${language}`);
	if (!records || records.length === 0) return [];

	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
		const line = lines[lineIdx];
		const lineNumber = lineIdx + 1;

		// Per-line, per-group seen-position sets. Records with no
		// positionDedupGroup do NOT participate in dedup; their
		// matches always emit. Records sharing the same non-null
		// group name share a single seen-position set on this line:
		// when a later record in the group matches at a position
		// already claimed by an earlier record in the same group,
		// the later emission is suppressed.
		//
		// This is the explicit, declared replacement for the legacy
		// hardcoded "all Rust fs records dedupe by position" rule.
		// Different group names do not interfere with each other.
		const seenByGroup = new Map<string, Set<number>>();

		for (const r of records) {
			if (r.family !== "fs") continue;
			const regex = r.compiledRegex;
			regex.lastIndex = 0;
			const group = r.positionDedupGroup;
			let m: RegExpExecArray | null;
			while ((m = regex.exec(line)) !== null) {
				if (group !== null) {
					let seen = seenByGroup.get(group);
					if (seen === undefined) {
						seen = new Set<number>();
						seenByGroup.set(group, seen);
					}
					if (seen.has(m.index)) {
						if (m[0].length === 0) regex.lastIndex++;
						continue;
					}
					seen.add(m.index);
				}
				const emissions = emitFsFromMatch(r, m, line, lineNumber, filePath, registry);
				for (const e of emissions) results.push(e);
				// Legacy single-match-per-line opt-in: stop after
				// the first regex hit on this line for this record.
				if (r.singleMatchPerLine) break;
				if (m[0].length === 0) regex.lastIndex++;
			}
		}
	}

	return results;
}

// ── Per-match emission (env) ──────────────────────────────────────

function emitEnvFromMatch(
	record: EnvDetectorRecord,
	match: RegExpExecArray,
	line: string,
	lineNumber: number,
	filePath: string,
	registry: HookRegistry,
): DetectedEnvDependency[] {
	// Walker default: varName from regex capture group 1.
	const walkerVarName = match[1] ?? null;

	if (record.hook === null) {
		// Hookless path: walker default fills varName, static fields
		// fill the rest. Loader has already verified that
		// accessKind and accessPattern are statically declared.
		if (walkerVarName === null) return [];
		// Reject names that don't match the upper-snake convention.
		// Preserves the legacy detectors' implicit filtering.
		if (!isLikelyVarName(walkerVarName)) return [];
		return [
			{
				varName: walkerVarName,
				accessKind: record.accessKind as EnvAccessKind,
				accessPattern: record.accessPattern as EnvAccessPattern,
				filePath,
				lineNumber,
				defaultValue: null,
				confidence: record.confidence,
			},
		];
	}

	// Hooked path: dispatch to the registered hook.
	const entry = registry.env.get(record.hook);
	if (!entry) {
		// Loader should have caught this; defense in depth.
		throw new Error(
			`walker: env hook "${record.hook}" not registered (record ${record.patternId})`,
		);
	}

	const ctx: EnvHookContext = {
		match,
		line,
		lineNumber,
		filePath,
		baseRecord: record,
	};
	const result = entry.fn(ctx);
	if ("skip" in result) return [];

	const out: DetectedEnvDependency[] = [];
	for (const e of result.records) {
		// Merge: hook fields override walker defaults override static.
		const varName = e.varName ?? walkerVarName;
		if (varName === null) continue;
		const accessKind: EnvAccessKind | null =
			e.accessKind ?? record.accessKind;
		const accessPattern: EnvAccessPattern | null =
			e.accessPattern ?? record.accessPattern;
		if (accessKind === null || accessPattern === null) {
			throw new Error(
				`walker: env record ${record.patternId} via hook ${record.hook} produced an emission with missing accessKind or accessPattern. Loader should have rejected this configuration.`,
			);
		}
		const defaultValue =
			e.defaultValue !== undefined ? e.defaultValue : null;
		const confidence = e.confidence ?? record.confidence;
		out.push({
			varName,
			accessKind,
			accessPattern,
			filePath,
			lineNumber,
			defaultValue,
			confidence,
		});
	}
	return out;
}

// ── Per-match emission (fs) ──────────────────────────────────────

function emitFsFromMatch(
	record: FsDetectorRecord,
	match: RegExpExecArray,
	line: string,
	lineNumber: number,
	filePath: string,
	registry: HookRegistry,
): DetectedFsMutation[] {
	// Walker default: targetPath from regex capture group 1,
	// destinationPath from group 2 if two_ended.
	const walkerTargetPath = match[1] ?? null;
	const walkerDestinationPath = record.twoEnded ? (match[2] ?? null) : null;

	if (record.hook === null) {
		// Hookless path: walker defaults fill targetPath and
		// destinationPath; static base fields fill kind and pattern.
		// Loader has verified base_kind and base_pattern are present.
		return [
			{
				filePath,
				lineNumber,
				mutationKind: record.baseKind as MutationKind,
				mutationPattern: record.basePattern as MutationPattern,
				targetPath: walkerTargetPath,
				destinationPath: walkerDestinationPath,
				dynamicPath: walkerTargetPath === null,
				confidence: record.confidence,
			},
		];
	}

	// Hooked path: dispatch to the registered hook.
	const entry = registry.fs.get(record.hook);
	if (!entry) {
		throw new Error(
			`walker: fs hook "${record.hook}" not registered (record ${record.patternId})`,
		);
	}

	const ctx: FsHookContext = {
		match,
		line,
		lineNumber,
		filePath,
		baseRecord: record,
	};
	const result = entry.fn(ctx);
	if ("skip" in result) return [];

	const out: DetectedFsMutation[] = [];
	for (const e of result.records) {
		const targetPath =
			e.targetPath !== undefined ? e.targetPath : walkerTargetPath;
		const destinationPath =
			e.destinationPath !== undefined
				? e.destinationPath
				: walkerDestinationPath;
		const dynamicPath =
			e.dynamicPath !== undefined ? e.dynamicPath : targetPath === null;
		const mutationKind: MutationKind | null =
			e.mutationKind ?? record.baseKind;
		const mutationPattern: MutationPattern | null =
			e.mutationPattern ?? record.basePattern;
		if (mutationKind === null || mutationPattern === null) {
			throw new Error(
				`walker: fs record ${record.patternId} via hook ${record.hook} produced an emission with missing mutationKind or mutationPattern. Loader should have rejected this configuration.`,
			);
		}
		const confidence = e.confidence ?? record.confidence;
		out.push({
			filePath,
			lineNumber,
			mutationKind,
			mutationPattern,
			targetPath,
			destinationPath,
			dynamicPath,
			confidence,
		});
	}
	return out;
}

// ── Helpers ───────────────────────────────────────────────────────

/**
 * Match the upper-snake env var name convention used by the legacy
 * detectors. Names that don't match are filtered out at emission
 * time, preserving the implicit filtering done by the original
 * regexes (which all use `[A-Z_][A-Z0-9_]*`).
 *
 * The walker applies this only to walker-default varName extraction
 * (regex group 1). Hook-supplied varNames pass through unchanged
 * because hooks may have their own filtering rules (e.g.
 * `expand_js_env_destructure`).
 */
function isLikelyVarName(name: string): boolean {
	return /^[A-Z_][A-Z0-9_]*$/.test(name);
}
