/**
 * Detector graph loader tests.
 *
 * Pure function tests — no I/O. Verifies strict validation:
 *  - well-formed records load
 *  - malformed records reject with a precise error message
 *  - unknown hook names reject
 *  - missing required fields reject
 *  - type mismatches reject
 *  - duplicate (language, family, patternId) rejects
 *  - unknown top-level keys reject (typo defense)
 *  - duplicates and order preservation invariants
 */

import { describe, expect, it } from "vitest";
import {
	DetectorLoadError,
	loadDetectorGraph,
} from "../../../../src/core/seams/detectors/loader.js";
import type {
	EnvHook,
	EnvHookEntry,
	EnvSuppliedField,
	FsHook,
	FsHookEntry,
	FsSuppliedField,
	HookRegistry,
} from "../../../../src/core/seams/detectors/types.js";

// ── Test helpers ───────────────────────────────────────────────────

const dummyEnvHook: EnvHook = () => ({ skip: true });
const dummyFsHook: FsHook = () => ({ skip: true });

interface EnvHookSpec {
	name: string;
	supplies?: readonly EnvSuppliedField[];
}

interface FsHookSpec {
	name: string;
	supplies?: readonly FsSuppliedField[];
}

function makeRegistry(opts?: {
	envHooks?: readonly (string | EnvHookSpec)[];
	fsHooks?: readonly (string | FsHookSpec)[];
}): HookRegistry {
	const env = new Map<string, EnvHookEntry>();
	const fs = new Map<string, FsHookEntry>();
	for (const h of opts?.envHooks ?? []) {
		const spec = typeof h === "string" ? { name: h } : h;
		env.set(spec.name, {
			fn: dummyEnvHook,
			supplies: new Set(spec.supplies ?? []),
		});
	}
	for (const h of opts?.fsHooks ?? []) {
		const spec = typeof h === "string" ? { name: h } : h;
		fs.set(spec.name, {
			fn: dummyFsHook,
			supplies: new Set(spec.supplies ?? []),
		});
	}
	return { env, fs };
}

const EMPTY_REGISTRY = makeRegistry();

// ── Happy path ─────────────────────────────────────────────────────

describe("loadDetectorGraph — happy path", () => {
	it("loads a single env record with all required fields", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\\.env\\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(1);
		const r = graph.allRecords[0];
		expect(r.family).toBe("env");
		expect(r.language).toBe("ts");
		expect(r.patternId).toBe("process_env_dot");
		expect(r.confidence).toBe(0.95);
		expect(r.flags).toBe("g"); // default
		expect(r.compiledRegex).toBeInstanceOf(RegExp);
		expect(r.compiledRegex.flags).toBe("g");
		expect(r.hook).toBeNull();
	});

	it("loads a single fs record with all required fields", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_write_file"
regex = '\\bfs\\.writeFile(?:Sync)?\\s*\\('
confidence = 0.90
base_kind = "write_file"
base_pattern = "fs_write_file"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(1);
		const r = graph.allRecords[0];
		expect(r.family).toBe("fs");
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.baseKind).toBe("write_file");
		expect(r.basePattern).toBe("fs_write_file");
		expect(r.twoEnded).toBe(false); // default
	});

	it("accepts an env record that references a registered hook", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\\.env\\.([A-Z_][A-Z0-9_]*)'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"
hook = "infer_js_env_access_with_default"
`;
		const registry = makeRegistry({
			envHooks: ["infer_js_env_access_with_default"],
		});
		const graph = loadDetectorGraph(toml, registry);
		expect(graph.allRecords[0].hook).toBe("infer_js_env_access_with_default");
	});

	it("accepts an fs record that references a registered hook", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_rename"
regex = '\\bfs\\.rename(?:Sync)?\\s*\\('
confidence = 0.90
base_kind = "rename_path"
base_pattern = "fs_rename"
two_ended = true
hook = "extract_two_ended_args"
`;
		const registry = makeRegistry({ fsHooks: ["extract_two_ended_args"] });
		const graph = loadDetectorGraph(toml, registry);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.twoEnded).toBe(true);
		expect(r.hook).toBe("extract_two_ended_args");
	});

	it("preserves declaration order in allRecords", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "py"
family = "env"
pattern_id = "third"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords.map((r) => r.patternId)).toEqual([
			"first",
			"second",
			"third",
		]);
	});

	it("indexes records by (family, language) for walker dispatch", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "ts_env_a"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "ts_env_b"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "py"
family = "env"
pattern_id = "py_env"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "ts_fs"
regex = 'd'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.byFamilyLanguage.get("env:ts")).toHaveLength(2);
		expect(graph.byFamilyLanguage.get("env:py")).toHaveLength(1);
		expect(graph.byFamilyLanguage.get("fs:ts")).toHaveLength(1);
		expect(graph.byFamilyLanguage.get("fs:py")).toBeUndefined();
	});

	it("preserves order within each (family, language) bucket", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_bracket"

[[detector]]
language = "ts"
family = "env"
pattern_id = "third"
regex = 'c'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_destructure"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const ids = graph.byFamilyLanguage.get("env:ts")!.map((r) => r.patternId);
		expect(ids).toEqual(["first", "second", "third"]);
	});

	it("compiles regex with custom flags", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "case_insensitive"
regex = 'foo'
flags = "gi"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords[0].compiledRegex.flags).toBe("gi");
	});
});

// ── Malformed TOML ─────────────────────────────────────────────────

describe("loadDetectorGraph — malformed TOML", () => {
	it("rejects invalid TOML syntax", () => {
		expect(() =>
			loadDetectorGraph("[[detector\nbroken", EMPTY_REGISTRY),
		).toThrow(DetectorLoadError);
		expect(() =>
			loadDetectorGraph("[[detector\nbroken", EMPTY_REGISTRY),
		).toThrow(/TOML parse error/);
	});

	it("rejects unknown top-level keys", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "ok"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[settings]
unknown = true
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/Unknown top-level key "settings"/,
		);
	});

	it("rejects when detector is not an array of tables", () => {
		const toml = `detector = "not an array"`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/must be an array of tables/,
		);
	});
});

// ── Required field validation ──────────────────────────────────────

describe("loadDetectorGraph — required field validation", () => {
	it("rejects missing language", () => {
		const toml = `
[[detector]]
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/missing required field "language"/,
		);
	});

	it("rejects missing family", () => {
		const toml = `
[[detector]]
language = "ts"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/missing required field "family"/,
		);
	});

	it("rejects missing pattern_id", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/missing required field "pattern_id"/,
		);
	});

	it("rejects missing regex", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/missing required field "regex"/,
		);
	});

	it("rejects missing confidence", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/missing required field "confidence"/,
		);
	});

	it("rejects env record missing access_kind when no hook supplies it", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/required env field "accessKind" is neither declared statically nor supplied by hook/,
		);
	});

	it("rejects env record missing access_pattern when no hook supplies it", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/required env field "accessPattern" is neither declared statically nor supplied by hook/,
		);
	});

	it("rejects fs record missing base_kind when no hook supplies it", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_pattern = "fs_write_file"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/required fs field "mutationKind" is neither declared statically nor supplied by hook/,
		);
	});

	it("rejects fs record missing base_pattern when no hook supplies it", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/required fs field "mutationPattern" is neither declared statically nor supplied by hook/,
		);
	});
});

// ── Enum validation ────────────────────────────────────────────────

describe("loadDetectorGraph — enum validation", () => {
	it("rejects unknown language", () => {
		const toml = `
[[detector]]
language = "go"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown language "go"/,
		);
	});

	it("rejects unknown family", () => {
		const toml = `
[[detector]]
language = "ts"
family = "network"
pattern_id = "x"
regex = 'a'
confidence = 0.9
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown family "network"/,
		);
	});

	it("rejects unknown env access_kind", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "maybe"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown access_kind "maybe"/,
		);
	});

	it("rejects unknown env access_pattern", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_void"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown access_pattern/,
		);
	});

	it("rejects unknown fs base_kind", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "vaporize"
base_pattern = "fs_write_file"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown base_kind "vaporize"/,
		);
	});

	it("rejects unknown fs base_pattern", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_void"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown base_pattern/,
		);
	});
});

// ── Type validation ────────────────────────────────────────────────

describe("loadDetectorGraph — type validation", () => {
	it("rejects non-string language", () => {
		const toml = `
[[detector]]
language = 1
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"language" must be a string/,
		);
	});

	it("rejects non-number confidence", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = "high"
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"confidence" must be a number/,
		);
	});

	it("rejects confidence outside [0, 1]", () => {
		const tomlHigh = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 1.5
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(tomlHigh, EMPTY_REGISTRY)).toThrow(
			/confidence must be a finite number in \[0, 1\]/,
		);

		const tomlLow = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = -0.1
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(tomlLow, EMPTY_REGISTRY)).toThrow(
			/confidence must be a finite number in \[0, 1\]/,
		);
	});

	it("rejects empty string field", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = ""
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"pattern_id" must not be empty/,
		);
	});

	it("rejects non-boolean two_ended", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "rename_path"
base_pattern = "fs_rename"
two_ended = "yes"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"two_ended" must be a boolean/,
		);
	});
});

// ── Regex validation ───────────────────────────────────────────────

describe("loadDetectorGraph — regex validation", () => {
	it("rejects invalid regex syntax", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "broken"
regex = '[unclosed'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/regex compile error/,
		);
	});

	it("rejects unknown regex flag", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
flags = "gz"
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown regex flag "z"/,
		);
	});
});

// ── Hook validation ────────────────────────────────────────────────

describe("loadDetectorGraph — hook validation", () => {
	it("rejects unknown env hook name", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "nonexistent_hook"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown env hook "nonexistent_hook"/,
		);
	});

	it("rejects unknown fs hook name", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "nonexistent_hook"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown fs hook "nonexistent_hook"/,
		);
	});

	it("env hook from fs registry is rejected (cross-family)", () => {
		// fs hook registered, but env record references it -> reject.
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "extract_first_string_arg"
`;
		const registry = makeRegistry({ fsHooks: ["extract_first_string_arg"] });
		expect(() => loadDetectorGraph(toml, registry)).toThrow(
			/unknown env hook "extract_first_string_arg"/,
		);
	});
});

// ── Duplicate detection ────────────────────────────────────────────

describe("loadDetectorGraph — duplicate detection", () => {
	it("rejects duplicate (language, family, pattern_id)", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/duplicate \(language=ts, family=env, pattern_id=process_env_dot\)/,
		);
	});

	it("allows same pattern_id across different languages", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "shared_id"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "py"
family = "env"
pattern_id = "shared_id"
regex = 'b'
confidence = 0.9
access_kind = "required"
access_pattern = "os_environ"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(2);
	});

	it("allows same pattern_id across different families", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "shared_id"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "shared_id"
regex = 'b'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(2);
	});
});

// ── Field allowlist (typo defense) ────────────────────────────────

describe("loadDetectorGraph — field allowlist", () => {
	it("rejects unknown field on env record", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
typo_field = true
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown field "typo_field"/,
		);
	});

	it("rejects fs-only field on env record", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
two_ended = false
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown field "two_ended" for env family/,
		);
	});

	it("rejects env-only field on fs record", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
access_kind = "required"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown field "access_kind" for fs family/,
		);
	});
});

// ── Must-be-supplied semantics (hook + static field interaction) ──

describe("loadDetectorGraph — must-be-supplied (env)", () => {
	it("accepts env record with NO static fields when hook supplies both", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_kind_and_pattern"
`;
		const registry = makeRegistry({
			envHooks: [{
				name: "supplies_kind_and_pattern",
				supplies: ["accessKind", "accessPattern"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		expect(graph.allRecords).toHaveLength(1);
		const r = graph.allRecords[0];
		if (r.family !== "env") throw new Error("type narrow");
		expect(r.accessKind).toBeNull();
		expect(r.accessPattern).toBeNull();
		expect(r.hook).toBe("supplies_kind_and_pattern");
	});

	it("accepts env record with hook supplying only accessKind, static accessPattern", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "supplies_kind_only"
`;
		const registry = makeRegistry({
			envHooks: [{
				name: "supplies_kind_only",
				supplies: ["accessKind"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		expect(graph.allRecords).toHaveLength(1);
		const r = graph.allRecords[0];
		if (r.family !== "env") throw new Error("type narrow");
		expect(r.accessKind).toBeNull();
		expect(r.accessPattern).toBe("process_env_dot");
	});

	it("accepts env record with both static AND hook (override semantics)", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
hook = "infer_kind"
`;
		const registry = makeRegistry({
			envHooks: [{
				name: "infer_kind",
				supplies: ["accessKind"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		const r = graph.allRecords[0];
		if (r.family !== "env") throw new Error("type narrow");
		expect(r.accessKind).toBe("required");
		expect(r.accessPattern).toBe("process_env_dot");
		expect(r.hook).toBe("infer_kind");
	});

	it("rejects env record where hook supplies only accessKind and accessPattern is missing", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_kind_only"
`;
		const registry = makeRegistry({
			envHooks: [{
				name: "supplies_kind_only",
				supplies: ["accessKind"],
			}],
		});
		expect(() => loadDetectorGraph(toml, registry)).toThrow(
			/required env field "accessPattern" is neither declared statically nor supplied by hook "supplies_kind_only"/,
		);
	});

	it("rejects env record where hook does not supply any required field and statics are missing", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
hook = "supplies_nothing_required"
`;
		const registry = makeRegistry({
			envHooks: [{
				name: "supplies_nothing_required",
				supplies: ["varName", "defaultValue"],
			}],
		});
		expect(() => loadDetectorGraph(toml, registry)).toThrow(
			/required env field "accessKind" is neither declared statically nor supplied by hook "supplies_nothing_required"/,
		);
	});
});

describe("loadDetectorGraph — must-be-supplied (fs)", () => {
	it("accepts fs record with NO static fields when hook supplies both kind and pattern", () => {
		const toml = `
[[detector]]
language = "c"
family = "fs"
pattern_id = "c_fopen"
regex = '\\bfopen\\s*\\('
confidence = 0.85
hook = "dispatch_c_fopen_mode"
`;
		const registry = makeRegistry({
			fsHooks: [{
				name: "dispatch_c_fopen_mode",
				supplies: ["mutationKind", "mutationPattern", "targetPath"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		expect(graph.allRecords).toHaveLength(1);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.baseKind).toBeNull();
		expect(r.basePattern).toBeNull();
	});

	it("accepts fs record with hook supplying only mutationPattern, static base_kind", () => {
		const toml = `
[[detector]]
language = "py"
family = "fs"
pattern_id = "py_remove"
regex = '\\b(os\\.(?:remove|unlink)|shutil\\.rmtree)\\s*\\('
confidence = 0.90
base_kind = "delete_path"
hook = "dispatch_python_remove_call"
`;
		const registry = makeRegistry({
			fsHooks: [{
				name: "dispatch_python_remove_call",
				supplies: ["mutationPattern"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.baseKind).toBe("delete_path");
		expect(r.basePattern).toBeNull();
	});

	it("rejects fs record where hook does not supply mutationKind and base_kind is missing", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\\bfs\\.writeFile\\s*\\('
confidence = 0.9
base_pattern = "fs_write_file"
hook = "extract_first_string_arg"
`;
		const registry = makeRegistry({
			fsHooks: [{
				name: "extract_first_string_arg",
				supplies: ["targetPath", "dynamicPath"],
			}],
		});
		expect(() => loadDetectorGraph(toml, registry)).toThrow(
			/required fs field "mutationKind" is neither declared statically nor supplied by hook "extract_first_string_arg"/,
		);
	});

	it("accepts fs record with full static + hook that only supplies path-related fields", () => {
		// The realistic majority case: kind + pattern static, hook just
		// extracts the literal path via state machine.
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "fs_write_file"
regex = '\\bfs\\.writeFile(?:Sync)?\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "extract_first_string_arg"
`;
		const registry = makeRegistry({
			fsHooks: [{
				name: "extract_first_string_arg",
				supplies: ["targetPath", "dynamicPath"],
			}],
		});
		const graph = loadDetectorGraph(toml, registry);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.baseKind).toBe("write_file");
		expect(r.basePattern).toBe("fs_write_file");
		expect(r.hook).toBe("extract_first_string_arg");
	});
});

// ── single_match_per_line field (both families) ───────────────────

describe("loadDetectorGraph — single_match_per_line", () => {
	it("defaults singleMatchPerLine to false when absent", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords[0].singleMatchPerLine).toBe(false);
	});

	it("accepts single_match_per_line = true on env record", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
single_match_per_line = true
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords[0].singleMatchPerLine).toBe(true);
	});

	it("accepts single_match_per_line = true on fs record", () => {
		const toml = `
[[detector]]
language = "java"
family = "fs"
pattern_id = "x"
regex = '\\bFiles\\.write\\s*\\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		expect(graph.allRecords[0].singleMatchPerLine).toBe(true);
	});

	it("rejects non-boolean single_match_per_line", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'a'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
single_match_per_line = "yes"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"single_match_per_line" must be a boolean/,
		);
	});
});

// ── position_dedup_group field (fs only) ──────────────────────────

describe("loadDetectorGraph — position_dedup_group", () => {
	it("accepts fs record with named overlap group", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '\\bfs::write\\s*\\(\\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.positionDedupGroup).toBe("rust_fs_write_callsite");
	});

	it("defaults positionDedupGroup to null when absent", () => {
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\\bfs\\.writeFile\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const r = graph.allRecords[0];
		if (r.family !== "fs") throw new Error("type narrow");
		expect(r.positionDedupGroup).toBeNull();
	});

	it("rejects empty string position_dedup_group (default is absence, not empty)", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "x"
regex = '\\bfs::write\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = ""
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"position_dedup_group" must not be empty/,
		);
	});

	it("rejects position_dedup_group on env records (fs-only)", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\\.env\\.([A-Z_]+)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
position_dedup_group = "some_group"
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/unknown field "position_dedup_group" for env family/,
		);
	});

	it("rejects non-string position_dedup_group", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "x"
regex = 'a'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = 42
`;
		expect(() => loadDetectorGraph(toml, EMPTY_REGISTRY)).toThrow(
			/"position_dedup_group" must be a string/,
		);
	});
});

// ── Empty input ────────────────────────────────────────────────────

describe("loadDetectorGraph — edge cases", () => {
	it("loads an empty graph from empty TOML", () => {
		const graph = loadDetectorGraph("", EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(0);
		expect(graph.byFamilyLanguage.size).toBe(0);
	});

	it("loads an empty graph from TOML with no detectors", () => {
		// Use only-comments TOML (legal empty document).
		const graph = loadDetectorGraph("# comment only\n", EMPTY_REGISTRY);
		expect(graph.allRecords).toHaveLength(0);
	});
});
