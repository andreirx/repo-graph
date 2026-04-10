/**
 * Detector hook registry implementations.
 *
 * Each hook is the procedural counterpart of one or more declared
 * detector records in `detectors.toml`. Hooks own the parts of
 * detector behavior that are NOT cleanly representable as data:
 * line-context inspection, multi-record emission, mode-based
 * dispatch, and state-machine string-literal extraction.
 *
 * Every hook is lifted byte-identical from the original
 * `env-detectors.ts` and `fs-mutation-detectors.ts` to preserve
 * detector output exactly across the externalization boundary.
 *
 * Hook contract (per types.ts):
 *  - DTO-shaped I/O — no storage, no fs, no side-effect surface
 *  - Returns either `{ skip: true }` or `{ records: [...] }`
 *  - Each emission is a partial; the walker merges with walker
 *    defaults and the static record's base fields
 *
 * Walker default behavior (must match types.ts comments exactly):
 *  - env varName ← regex capture group 1 (if hook does not supply)
 *  - fs targetPath ← regex capture group 1 (if hook does not supply)
 *  - fs destinationPath ← regex capture group 2 (when two_ended)
 *  - fs dynamicPath ← derived from targetPath === null
 *  - confidence ← record.confidence
 *
 * Pure functions only. No I/O. No external dependencies.
 */

import type {
	EnvHook,
	EnvHookEmission,
	EnvHookEntry,
	EnvHookResult,
	EnvSuppliedField,
	FsHook,
	FsHookEntry,
	FsHookResult,
	FsSuppliedField,
	HookRegistry,
} from "./types.js";

// ── Env hooks ──────────────────────────────────────────────────────

/**
 * Infer required vs optional access kind for JS env access by reading
 * the line context AFTER the matched var name. Lifted from
 * env-detectors.ts inferJsAccessKind + extractJsDefault.
 *
 * Behavior preserved exactly:
 *  - `process.env.X || "fallback"` → optional, default "fallback"
 *  - `process.env.X ?? "fallback"` → optional, default "fallback"
 *  - `process.env.X)` (inside a function call) → unknown
 *  - bare `process.env.X` → required, no default
 *
 * Uses `line.indexOf(varName)` exactly as the original code does,
 * not the regex match position, to preserve any pre-existing edge
 * case behavior.
 */
const infer_js_env_access_with_default: EnvHook = (ctx): EnvHookResult => {
	const varName = ctx.match[1];
	if (varName === undefined) return { skip: true };

	const afterVar = ctx.line.slice(
		ctx.line.indexOf(varName) + varName.length,
	);

	let accessKind: "required" | "optional" | "unknown";
	if (/^\s*(\|\||&&|\?\?)/.test(afterVar)) {
		accessKind = "optional";
	} else if (/^\s*\)/.test(afterVar)) {
		accessKind = "unknown";
	} else {
		accessKind = "required";
	}

	const defaultMatch = afterVar.match(
		/^\s*(?:\|\||&&|\?\?)\s*["']([^"']+)["']/,
	);
	const defaultValue = defaultMatch ? defaultMatch[1] : null;

	return {
		records: [
			{
				varName,
				accessKind,
				defaultValue,
			},
		],
	};
};

/**
 * Expand a JS destructure pattern into one record per declared var.
 *
 * Lifted from the destructure branch in detectJsEnvAccesses. Handles
 * renaming syntax (`X: localName`), filters out names that don't
 * match the upper-snake convention, and emits each surviving var as
 * a separate record with hardcoded `accessKind = "unknown"`.
 *
 * Walker default for varName does NOT apply because regex group 1
 * is the entire destructure body, not an individual var.
 */
const expand_js_env_destructure: EnvHook = (ctx): EnvHookResult => {
	const body = ctx.match[1];
	if (body === undefined) return { skip: true };

	const records: EnvHookEmission[] = [];
	const vars = body.split(",").map((v) => v.trim());
	for (const v of vars) {
		// Handle renaming: `VAR_NAME: localName`
		const varName = v.includes(":") ? v.split(":")[0].trim() : v;
		if (/^[A-Z_][A-Z0-9_]*$/.test(varName)) {
			records.push({
				varName,
				accessKind: "unknown",
				defaultValue: null,
			});
		}
	}

	if (records.length === 0) return { skip: true };
	return { records };
};

/**
 * Read default value from regex capture group 2 for Python
 * `os.environ.get("X", "default")` and `os.getenv("X", "default")`.
 *
 * Behavior preserved exactly:
 *  - if group 2 is present, accessKind = "optional", defaultValue = group 2
 *  - if group 2 is absent, accessKind = "unknown", defaultValue = null
 *
 * Note: Python's `os.environ["X"]` is required (different regex,
 * different hook-less record).
 */
const python_get_default_from_group2: EnvHook = (ctx): EnvHookResult => {
	const varName = ctx.match[1];
	if (varName === undefined) return { skip: true };
	const defaultValue = ctx.match[2] ?? null;
	return {
		records: [
			{
				varName,
				accessKind: defaultValue !== null ? "optional" : "unknown",
				defaultValue,
			},
		],
	};
};

// ── Fs hooks ───────────────────────────────────────────────────────

/**
 * Extract the literal first string argument from a function call's
 * argument list using a state machine that respects quotes, escapes,
 * and template literal interpolation.
 *
 * Used by JS fs detectors where the regex matches the call site
 * (e.g. `\bfs.writeFile\s*\(`) and the path follows immediately.
 * Returns null if the first arg is not a literal string (dynamic).
 *
 * Lifted byte-identical from extractFirstStringArg in
 * fs-mutation-detectors.ts.
 */
const extract_first_string_arg: FsHook = (ctx): FsHookResult => {
	const after = ctx.line.slice(ctx.match.index + ctx.match[0].length);
	const literal = extractFirstStringArgImpl(after);
	return {
		records: [
			{
				targetPath: literal,
				dynamicPath: literal === null,
			},
		],
	};
};

/**
 * Extract first AND second string args for two-ended ops (rename, copy).
 *
 * If first arg is dynamic, returns target = null and destination = null.
 * If first is literal but second is dynamic, returns target = literal,
 * destination = null. Both literal → both extracted.
 *
 * Lifted byte-identical from extractFirstStringArg + extractSecondStringArg.
 */
const extract_two_ended_args: FsHook = (ctx): FsHookResult => {
	const after = ctx.line.slice(ctx.match.index + ctx.match[0].length);
	const first = extractFirstStringArgImpl(after);
	let second: string | null = null;
	if (first !== null) {
		second = extractSecondStringArgImpl(after);
	}
	return {
		records: [
			{
				targetPath: first,
				destinationPath: second,
				dynamicPath: first === null,
			},
		],
	};
};

/**
 * Java `Files.write(Paths.get("..."))` literal unwrap.
 *
 * Each Java fs call type uses its own specific inner regex to
 * extract the literal — the legacy code uses three independent
 * `line.match(...)` calls, one per call type. Sharing one generic
 * inner regex would cause cross-contamination between records on
 * lines containing multiple Java fs call types.
 */
const unwrap_java_files_write_literal: FsHook = (ctx): FsHookResult => {
	const innerMatch = ctx.line.match(
		/Files\.write(?:String)?\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/,
	);
	const literal = innerMatch ? innerMatch[1] : null;
	return {
		records: [
			{
				targetPath: literal,
				dynamicPath: literal === null,
			},
		],
	};
};

const unwrap_java_files_delete_literal: FsHook = (ctx): FsHookResult => {
	const innerMatch = ctx.line.match(
		/Files\.delete\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/,
	);
	const literal = innerMatch ? innerMatch[1] : null;
	return {
		records: [
			{
				targetPath: literal,
				dynamicPath: literal === null,
			},
		],
	};
};

const unwrap_java_files_create_directory_literal: FsHook = (ctx): FsHookResult => {
	const innerMatch = ctx.line.match(
		/createDirector(?:y|ies)\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/,
	);
	const literal = innerMatch ? innerMatch[1] : null;
	return {
		records: [
			{
				targetPath: literal,
				dynamicPath: literal === null,
			},
		],
	};
};

/**
 * Java `new FileOutputStream("...")` literal-or-dynamic dispatch
 * with confidence override.
 *
 * Legacy semantics:
 *  - if a literal FOS exists anywhere on the line: targetPath =
 *    that literal, confidence = 0.85
 *  - else if a non-literal FOS call exists: targetPath = null,
 *    confidence = 0.80
 *
 * Walker default confidence (record's static `confidence`) covers
 * the literal case (0.85). The hook overrides confidence to 0.80
 * in the dynamic case.
 */
const unwrap_java_file_output_stream_literal: FsHook = (ctx): FsHookResult => {
	const innerMatch = ctx.line.match(
		/new\s+FileOutputStream\s*\(\s*"([^"]+)"/,
	);
	if (innerMatch) {
		return {
			records: [
				{
					targetPath: innerMatch[1],
					dynamicPath: false,
				},
			],
		};
	}
	return {
		records: [
			{
				targetPath: null,
				dynamicPath: true,
				confidence: 0.80,
			},
		],
	};
};

/**
 * C `fopen(path, "wa+")` mode-based dispatch. The regex captures
 * path in group 1 (or undefined for dynamic) and mode in group 2.
 *
 * Mode starting with "a" → append_file / c_fopen_append
 * Otherwise → write_file / c_fopen_write
 *
 * The walker default for targetPath does NOT apply because this
 * hook supplies all relevant fields. mutationKind and mutationPattern
 * are dispatched here, not declared statically.
 */
const dispatch_c_fopen_mode: FsHook = (ctx): FsHookResult => {
	const path = ctx.match[1] ?? null;
	const mode = ctx.match[2];
	if (mode === undefined) return { skip: true };
	const isAppend = mode.startsWith("a");
	return {
		records: [
			{
				targetPath: path,
				dynamicPath: path === null,
				mutationKind: isAppend ? "append_file" : "write_file",
				mutationPattern: isAppend ? "c_fopen_append" : "c_fopen_write",
			},
		],
	};
};

/**
 * Python `os.remove`/`os.unlink`/`shutil.rmtree` pattern dispatch.
 *
 * Group 1 is the call name (e.g. "os.remove"), group 2 is the
 * literal path (or undefined if dynamic). The hook dispatches the
 * mutationPattern based on group 1; mutationKind is always
 * `delete_path` and is supplied statically by the record.
 */
const dispatch_python_remove_call: FsHook = (ctx): FsHookResult => {
	const callName = ctx.match[1];
	const path = ctx.match[2] ?? null;
	if (callName === undefined) return { skip: true };
	let pattern: "py_os_remove" | "py_os_unlink" | "py_shutil_rmtree";
	if (callName === "os.unlink") {
		pattern = "py_os_unlink";
	} else if (callName === "shutil.rmtree") {
		pattern = "py_shutil_rmtree";
	} else {
		pattern = "py_os_remove";
	}
	return {
		records: [
			{
				targetPath: path,
				dynamicPath: path === null,
				mutationPattern: pattern,
			},
		],
	};
};

/**
 * Python `open(path, "w"|"a")` mode-based dispatch.
 *
 * Group 1 is the literal path (or undefined if dynamic), group 2
 * is the mode ("w" or "a"). The hook dispatches both mutationKind
 * and mutationPattern based on the mode character.
 */
const dispatch_python_open_mode: FsHook = (ctx): FsHookResult => {
	const path = ctx.match[1] ?? null;
	const mode = ctx.match[2];
	if (mode === undefined) return { skip: true };
	const isAppend = mode === "a";
	return {
		records: [
			{
				targetPath: path,
				dynamicPath: path === null,
				mutationKind: isAppend ? "append_file" : "write_file",
				mutationPattern: isAppend ? "py_open_append" : "py_open_write",
			},
		],
	};
};

/**
 * Python `os.mkdir`/`os.makedirs` pattern dispatch.
 *
 * Group 1 is the call name ("mkdir" or "makedirs"), group 2 is the
 * literal path (or undefined if dynamic). mutationKind is always
 * `create_dir` and is supplied statically.
 */
const dispatch_python_mkdir_call: FsHook = (ctx): FsHookResult => {
	const callName = ctx.match[1];
	const path = ctx.match[2] ?? null;
	if (callName === undefined) return { skip: true };
	const pattern: "py_os_mkdir" | "py_os_makedirs" =
		callName === "makedirs" ? "py_os_makedirs" : "py_os_mkdir";
	return {
		records: [
			{
				targetPath: path,
				dynamicPath: path === null,
				mutationPattern: pattern,
			},
		],
	};
};


// ── State-machine string-literal extractors (lifted from fs-mutation-detectors.ts) ──

/**
 * Extract a string literal from the start of an argument list.
 * Returns the literal contents if the first character (after
 * whitespace) is a quote; otherwise null.
 *
 * Input is the substring AFTER the matched call's opening "(".
 *
 * Lifted byte-identical from extractFirstStringArg.
 */
function extractFirstStringArgImpl(after: string): string | null {
	let i = 0;
	while (i < after.length && /\s/.test(after[i])) i++;
	if (i >= after.length) return null;
	const ch = after[i];
	if (ch !== '"' && ch !== "'" && ch !== "`") return null;
	const quote = ch;
	let result = "";
	i++;
	while (i < after.length && after[i] !== quote) {
		if (quote === "`" && after[i] === "$" && after[i + 1] === "{") {
			return null;
		}
		if (after[i] === "\\" && i + 1 < after.length) {
			result += after[i + 1];
			i += 2;
			continue;
		}
		result += after[i];
		i++;
	}
	if (i >= after.length) return null;
	return result;
}

/**
 * Extract the SECOND string literal from an argument list. Used by
 * two-ended ops (rename, copy).
 *
 * Lifted byte-identical from extractSecondStringArg.
 */
function extractSecondStringArgImpl(after: string): string | null {
	let i = 0;
	while (i < after.length && /\s/.test(after[i])) i++;
	if (i >= after.length) return null;
	const firstQuote = after[i];
	if (firstQuote !== '"' && firstQuote !== "'" && firstQuote !== "`") {
		return null;
	}
	i++;
	while (i < after.length && after[i] !== firstQuote) {
		if (after[i] === "\\") {
			i += 2;
			continue;
		}
		i++;
	}
	if (i >= after.length) return null;
	i++; // past closing quote of first arg
	while (i < after.length && /[\s,]/.test(after[i])) i++;
	if (i >= after.length) return null;
	const ch = after[i];
	if (ch !== '"' && ch !== "'" && ch !== "`") return null;
	const quote = ch;
	let result = "";
	i++;
	while (i < after.length && after[i] !== quote) {
		if (quote === "`" && after[i] === "$" && after[i + 1] === "{") {
			return null;
		}
		if (after[i] === "\\" && i + 1 < after.length) {
			result += after[i + 1];
			i += 2;
			continue;
		}
		result += after[i];
		i++;
	}
	if (i >= after.length) return null;
	return result;
}

// ── Registry construction ──────────────────────────────────────────

/**
 * Build the canonical hook registry for the production detector
 * graph. The set of registered hooks must match the set of hook
 * names referenced in `detectors.toml`. The loader rejects any
 * record that references an unregistered hook.
 *
 * Each entry's `supplies` set is the contract surface for the
 * loader's must-be-supplied check. Adding a new hook requires
 * declaring exactly which output fields it is responsible for
 * supplying. The TypeScript type system enforces that supplied
 * field names are members of `EnvSuppliedField` / `FsSuppliedField`,
 * so typos are compile errors.
 */
export function createDefaultHookRegistry(): HookRegistry {
	const env = new Map<string, EnvHookEntry>();
	const fs = new Map<string, FsHookEntry>();

	// ── env ────────────────────────────────────────────────────────
	env.set("infer_js_env_access_with_default", {
		fn: infer_js_env_access_with_default,
		supplies: new Set<EnvSuppliedField>([
			"varName",
			"accessKind",
			"defaultValue",
		]),
	});
	env.set("expand_js_env_destructure", {
		fn: expand_js_env_destructure,
		supplies: new Set<EnvSuppliedField>([
			"varName",
			"accessKind",
			"defaultValue",
		]),
	});
	env.set("python_get_default_from_group2", {
		fn: python_get_default_from_group2,
		supplies: new Set<EnvSuppliedField>([
			"varName",
			"accessKind",
			"defaultValue",
		]),
	});

	// ── fs ─────────────────────────────────────────────────────────
	fs.set("extract_first_string_arg", {
		fn: extract_first_string_arg,
		supplies: new Set<FsSuppliedField>(["targetPath", "dynamicPath"]),
	});
	fs.set("extract_two_ended_args", {
		fn: extract_two_ended_args,
		supplies: new Set<FsSuppliedField>([
			"targetPath",
			"destinationPath",
			"dynamicPath",
		]),
	});
	fs.set("unwrap_java_files_write_literal", {
		fn: unwrap_java_files_write_literal,
		supplies: new Set<FsSuppliedField>(["targetPath", "dynamicPath"]),
	});
	fs.set("unwrap_java_files_delete_literal", {
		fn: unwrap_java_files_delete_literal,
		supplies: new Set<FsSuppliedField>(["targetPath", "dynamicPath"]),
	});
	fs.set("unwrap_java_files_create_directory_literal", {
		fn: unwrap_java_files_create_directory_literal,
		supplies: new Set<FsSuppliedField>(["targetPath", "dynamicPath"]),
	});
	fs.set("unwrap_java_file_output_stream_literal", {
		fn: unwrap_java_file_output_stream_literal,
		supplies: new Set<FsSuppliedField>(["targetPath", "dynamicPath"]),
	});
	fs.set("dispatch_c_fopen_mode", {
		fn: dispatch_c_fopen_mode,
		supplies: new Set<FsSuppliedField>([
			"targetPath",
			"dynamicPath",
			"mutationKind",
			"mutationPattern",
		]),
	});
	fs.set("dispatch_python_open_mode", {
		fn: dispatch_python_open_mode,
		supplies: new Set<FsSuppliedField>([
			"targetPath",
			"dynamicPath",
			"mutationKind",
			"mutationPattern",
		]),
	});
	fs.set("dispatch_python_remove_call", {
		fn: dispatch_python_remove_call,
		supplies: new Set<FsSuppliedField>([
			"targetPath",
			"dynamicPath",
			"mutationPattern",
		]),
	});
	fs.set("dispatch_python_mkdir_call", {
		fn: dispatch_python_mkdir_call,
		supplies: new Set<FsSuppliedField>([
			"targetPath",
			"dynamicPath",
			"mutationPattern",
		]),
	});

	return { env, fs };
}
