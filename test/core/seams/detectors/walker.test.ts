/**
 * Generic detector graph walker tests.
 *
 * Pure walker behavior — synthetic in-memory tables, NOT the real
 * detectors.toml. Verifies the contract between data and walker:
 *
 *  - hookless records produce one record per match using walker
 *    defaults (varName/targetPath from group 1) and the static
 *    base fields
 *  - hooked records dispatch to the registered hook and merge its
 *    output with walker defaults; hook fields override defaults
 *  - skip results suppress emission entirely
 *  - multi-emit hooks emit multiple records per match
 *  - env dedup is applied across all matches in one call
 *  - fs records preserve declaration order
 *  - per-line position dedup prevents double-emit on overlapping
 *    fs regex variants
 *
 * The byte-identical behavior gate against the real detector test
 * corpus is exercised in Substep D, not here.
 */

import { describe, expect, it } from "vitest";
import {
	loadDetectorGraph,
} from "../../../../src/core/seams/detectors/loader.js";
import type {
	EnvHook,
	EnvHookEntry,
	FsHook,
	FsHookEntry,
	HookRegistry,
} from "../../../../src/core/seams/detectors/types.js";
import {
	walkEnvDetectors,
	walkFsDetectors,
} from "../../../../src/core/seams/detectors/walker.js";

// ── Test helpers ───────────────────────────────────────────────────

function makeRegistry(opts?: {
	env?: ReadonlyMap<string, EnvHookEntry>;
	fs?: ReadonlyMap<string, FsHookEntry>;
}): HookRegistry {
	return {
		env: opts?.env ?? new Map(),
		fs: opts?.fs ?? new Map(),
	};
}

const EMPTY_REGISTRY = makeRegistry();

// ── Hookless env path ──────────────────────────────────────────────

describe("walkEnvDetectors — hookless records", () => {
	it("emits one record per match using static fields and walker default varName", () => {
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
		const content = `const port = process.env.PORT;`;
		const out = walkEnvDetectors(
			graph,
			EMPTY_REGISTRY,
			"ts",
			content,
			"src/foo.ts",
		);
		expect(out).toHaveLength(1);
		expect(out[0]).toEqual({
			varName: "PORT",
			accessKind: "required",
			accessPattern: "process_env_dot",
			filePath: "src/foo.ts",
			lineNumber: 1,
			defaultValue: null,
			confidence: 0.95,
		});
	});

	it("dedupes by (filePath, varName) across multiple matches", () => {
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
		const content = `
const a = process.env.PORT;
const b = process.env.PORT;
const c = process.env.HOST;
`;
		const out = walkEnvDetectors(graph, EMPTY_REGISTRY, "ts", content, "f.ts");
		expect(out.map((r) => r.varName)).toEqual(["PORT", "HOST"]);
	});

	it("filters out names that do not match upper-snake convention", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "loose_match"
regex = 'X(\\w+)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		// "Xfoo" → group 1 = "foo" (lowercase, rejected)
		// "XBAR" → group 1 = "BAR" (matches, kept)
		const content = `Xfoo XBAR Xbaz`;
		const out = walkEnvDetectors(graph, EMPTY_REGISTRY, "ts", content, "f.ts");
		expect(out.map((r) => r.varName)).toEqual(["BAR"]);
	});

	it("emits zero records when language has no detectors", () => {
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
		const out = walkEnvDetectors(graph, EMPTY_REGISTRY, "py", "anything", "f.py");
		expect(out).toEqual([]);
	});

	it("preserves line numbers across multi-line content", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\\.env\\.([A-Z_][A-Z0-9_]*)'
confidence = 0.9
access_kind = "required"
access_pattern = "process_env_dot"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const content = `// line 1
// line 2
const a = process.env.PORT;
// line 4
const b = process.env.HOST;`;
		const out = walkEnvDetectors(graph, EMPTY_REGISTRY, "ts", content, "f.ts");
		expect(out.find((r) => r.varName === "PORT")?.lineNumber).toBe(3);
		expect(out.find((r) => r.varName === "HOST")?.lineNumber).toBe(5);
	});
});

// ── Hooked env path ────────────────────────────────────────────────

describe("walkEnvDetectors — hooked records", () => {
	it("dispatches to the hook and merges its output with walker defaults", () => {
		const customHook: EnvHook = (ctx) => ({
			records: [
				{
					varName: ctx.match[1],
					accessKind: "optional",
					defaultValue: "from-hook",
				},
			],
		});
		const env = new Map<string, EnvHookEntry>([
			[
				"set_optional_with_default",
				{
					fn: customHook,
					supplies: new Set(["accessKind", "defaultValue"]),
				},
			],
		]);
		const registry = makeRegistry({ env });

		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "process_env_dot"
regex = 'process\\.env\\.([A-Z_][A-Z0-9_]*)'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "set_optional_with_default"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"x = process.env.PORT",
			"f.ts",
		);
		expect(out).toHaveLength(1);
		expect(out[0]).toEqual({
			varName: "PORT",
			accessKind: "optional",
			accessPattern: "process_env_dot", // from static
			filePath: "f.ts",
			lineNumber: 1,
			defaultValue: "from-hook",
			confidence: 0.9,
		});
	});

	it("hook returning skip suppresses emission", () => {
		const skipHook: EnvHook = () => ({ skip: true });
		const env = new Map<string, EnvHookEntry>([
			[
				"always_skip",
				{ fn: skipHook, supplies: new Set(["accessKind", "accessPattern"]) },
			],
		]);
		const registry = makeRegistry({ env });
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\\.env\\.([A-Z_]+)'
confidence = 0.9
hook = "always_skip"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"x = process.env.PORT",
			"f.ts",
		);
		expect(out).toEqual([]);
	});

	it("multi-emit hook produces multiple records per match", () => {
		const multiHook: EnvHook = (ctx) => {
			const body = ctx.match[1];
			if (body === undefined) return { skip: true };
			return {
				records: body.split(",").map((v) => ({
					varName: v.trim(),
					accessKind: "unknown" as const,
				})),
			};
		};
		const env = new Map<string, EnvHookEntry>([
			[
				"split_csv",
				{
					fn: multiHook,
					supplies: new Set(["varName", "accessKind"]),
				},
			],
		]);
		const registry = makeRegistry({ env });
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\\{([^}]+)\\}'
confidence = 0.9
access_pattern = "process_env_destructure"
hook = "split_csv"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"const { PORT, HOST, DB } = x",
			"f.ts",
		);
		expect(out.map((r) => r.varName)).toEqual(["PORT", "HOST", "DB"]);
	});

	it("hook varName overrides walker default group-1 varName", () => {
		const renameHook: EnvHook = () => ({
			records: [
				{
					varName: "RENAMED",
					accessKind: "required",
				},
			],
		});
		const env = new Map<string, EnvHookEntry>([
			[
				"rename",
				{ fn: renameHook, supplies: new Set(["varName", "accessKind"]) },
			],
		]);
		const registry = makeRegistry({ env });
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "x"
regex = 'process\\.env\\.([A-Z_]+)'
confidence = 0.9
access_pattern = "process_env_dot"
hook = "rename"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"x = process.env.ORIGINAL",
			"f.ts",
		);
		expect(out).toHaveLength(1);
		expect(out[0].varName).toBe("RENAMED");
	});
});

// ── Hookless fs path ───────────────────────────────────────────────

describe("walkFsDetectors — hookless records", () => {
	it("emits one record per match with walker default targetPath from group 1", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '(?:std::)?fs::write\\s*\\(\\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			'std::fs::write("output.txt", data)?;',
			"src/main.rs",
		);
		expect(out).toHaveLength(1);
		expect(out[0]).toEqual({
			filePath: "src/main.rs",
			lineNumber: 1,
			mutationKind: "write_file",
			mutationPattern: "rust_fs_write",
			targetPath: "output.txt",
			destinationPath: null,
			dynamicPath: false,
			confidence: 0.9,
		});
	});

	it("derives dynamicPath=true when group 1 is missing", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_dynamic"
regex = '(?:std::)?fs::write\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			"std::fs::write(some_var, data)?;",
			"src/main.rs",
		);
		expect(out).toHaveLength(1);
		expect(out[0].targetPath).toBeNull();
		expect(out[0].dynamicPath).toBe(true);
	});

	it("captures destinationPath from group 2 when two_ended is true", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_rename_literal"
regex = '(?:std::)?fs::rename\\s*\\(\\s*"([^"]+)"\\s*,\\s*"([^"]+)"'
confidence = 0.9
base_kind = "rename_path"
base_pattern = "rust_fs_rename"
two_ended = true
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			'fs::rename("a.txt", "b.txt")?;',
			"src/r.rs",
		);
		expect(out).toHaveLength(1);
		expect(out[0].targetPath).toBe("a.txt");
		expect(out[0].destinationPath).toBe("b.txt");
	});

	it("ignores group 2 when two_ended is false", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "captures_two_groups"
regex = '\\b(\\w+)\\s+(\\w+)'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			"foo bar",
			"f.rs",
		);
		expect(out).toHaveLength(1);
		expect(out[0].destinationPath).toBeNull();
	});
});

// ── Hooked fs path ─────────────────────────────────────────────────

describe("walkFsDetectors — hooked records", () => {
	it("dispatches to fs hook and merges its output with walker defaults", () => {
		const dispatchHook: FsHook = (ctx) => ({
			records: [
				{
					targetPath: "from-hook",
					mutationKind: "delete_path",
					mutationPattern: "py_os_remove",
					dynamicPath: false,
				},
			],
		});
		const fs = new Map<string, FsHookEntry>([
			[
				"dispatch_remove",
				{
					fn: dispatchHook,
					supplies: new Set([
						"targetPath",
						"dynamicPath",
						"mutationKind",
						"mutationPattern",
					]),
				},
			],
		]);
		const registry = makeRegistry({ fs });
		const toml = `
[[detector]]
language = "py"
family = "fs"
pattern_id = "x"
regex = '\\bremove\\s*\\('
confidence = 0.9
hook = "dispatch_remove"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkFsDetectors(
			graph,
			registry,
			"py",
			"os.remove(somevar)",
			"f.py",
		);
		expect(out).toHaveLength(1);
		expect(out[0]).toEqual({
			filePath: "f.py",
			lineNumber: 1,
			mutationKind: "delete_path",
			mutationPattern: "py_os_remove",
			targetPath: "from-hook",
			destinationPath: null,
			dynamicPath: false,
			confidence: 0.9,
		});
	});

	it("fs hook fields override walker default targetPath", () => {
		const overrideHook: FsHook = () => ({
			records: [
				{
					targetPath: "overridden",
					dynamicPath: false,
				},
			],
		});
		const fs = new Map<string, FsHookEntry>([
			[
				"override_path",
				{
					fn: overrideHook,
					supplies: new Set(["targetPath", "dynamicPath"]),
				},
			],
		]);
		const registry = makeRegistry({ fs });
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\\bcall\\s*\\(\\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
hook = "override_path"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkFsDetectors(
			graph,
			registry,
			"ts",
			'call("from-group-1")',
			"f.ts",
		);
		expect(out).toHaveLength(1);
		expect(out[0].targetPath).toBe("overridden");
	});

	it("hook returning skip suppresses emission", () => {
		const skipHook: FsHook = () => ({ skip: true });
		const fs = new Map<string, FsHookEntry>([
			[
				"always_skip",
				{
					fn: skipHook,
					supplies: new Set([
						"targetPath",
						"dynamicPath",
						"mutationKind",
						"mutationPattern",
					]),
				},
			],
		]);
		const registry = makeRegistry({ fs });
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "x"
regex = '\\ba\\b'
confidence = 0.9
hook = "always_skip"
`;
		const graph = loadDetectorGraph(toml, registry);
		const out = walkFsDetectors(graph, registry, "ts", "a a a", "f.ts");
		expect(out).toEqual([]);
	});
});

// ── Position dedup via named overlap groups ───────────────────────

describe("walkFsDetectors — position_dedup_group", () => {
	it("dedupes records sharing the same overlap group at the same position", () => {
		// Two records in the same overlap group: a literal-capturing
		// variant and a catch-all variant of the same Rust fs call.
		// They both match `fs::write("output.txt", ...)` at the
		// same start index. The first record (literal) wins.
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_literal"
regex = '(?:std::)?fs::write\\s*\\(\\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"

[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write_catchall"
regex = '(?:std::)?fs::write\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			'std::fs::write("output.txt", data)?;',
			"f.rs",
		);
		expect(out).toHaveLength(1);
		expect(out[0].targetPath).toBe("output.txt");
	});

	it("does NOT dedupe records with different overlap groups at the same position", () => {
		// Same regex twice, different groups → both emit. Groups
		// are independent dedup universes.
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "first"
regex = '\\bfs::write\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "group_a"

[[detector]]
language = "rs"
family = "fs"
pattern_id = "second"
regex = '\\bfs::write\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "group_b"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			"fs::write(x);",
			"f.rs",
		);
		expect(out).toHaveLength(2);
	});

	it("does NOT dedupe ungrouped records at the same position", () => {
		// Two records with no group → both emit even at the same
		// position. This pins the byte-identical translation of the
		// legacy JS/Python/Java/C fs detectors which have no
		// position dedup.
		const toml = `
[[detector]]
language = "ts"
family = "fs"
pattern_id = "first"
regex = '\\bfs\\.writeFile\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"

[[detector]]
language = "ts"
family = "fs"
pattern_id = "second"
regex = '\\bfs\\.writeFile\\s*\\('
confidence = 0.9
base_kind = "write_file"
base_pattern = "fs_write_file"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"ts",
			"fs.writeFile(x);",
			"f.ts",
		);
		expect(out).toHaveLength(2);
	});

	it("dedupe is per-line: same position on different lines does not collide", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "first"
regex = '\\bfs::write\\b'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			"fs::write(a);\nfs::write(b);\nfs::write(c);",
			"f.rs",
		);
		// Each line is an independent dedup universe.
		expect(out).toHaveLength(3);
		expect(out.map((r) => r.lineNumber)).toEqual([1, 2, 3]);
	});

	it("emits both records when call sites are at distinct positions on the same line", () => {
		const toml = `
[[detector]]
language = "rs"
family = "fs"
pattern_id = "rust_fs_write"
regex = '(?:std::)?fs::write\\s*\\(\\s*"([^"]+)"'
confidence = 0.9
base_kind = "write_file"
base_pattern = "rust_fs_write"
position_dedup_group = "rust_fs_write_callsite"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"rs",
			'std::fs::write("a.txt", x); std::fs::write("b.txt", y);',
			"f.rs",
		);
		expect(out).toHaveLength(2);
		expect(out.map((r) => r.targetPath)).toEqual(["a.txt", "b.txt"]);
	});
});

// ── single_match_per_line semantics ───────────────────────────────

describe("walker — single_match_per_line", () => {
	it("env: stops after the first regex match on a line when flag is true", () => {
		// Two destructure patterns on one line. Without the flag,
		// the walker would emit records for both. With the flag,
		// only the first is processed.
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\\{([^}]+)\\}\\s*=\\s*process\\.env'
confidence = 0.9
access_kind = "unknown"
access_pattern = "process_env_destructure"
hook = "split_csv"
single_match_per_line = true
`;
		const splitHook: EnvHook = (ctx) => {
			const body = ctx.match[1];
			if (body === undefined) return { skip: true };
			return {
				records: body.split(",").map((v) => ({
					varName: v.trim(),
					accessKind: "unknown" as const,
				})),
			};
		};
		const env = new Map<string, EnvHookEntry>([
			[
				"split_csv",
				{
					fn: splitHook,
					supplies: new Set(["varName", "accessKind"]),
				},
			],
		]);
		const registry = makeRegistry({ env });
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"const {A, B} = process.env; const {C, D} = process.env",
			"f.ts",
		);
		// Only first destructure processed → A and B emitted, C and D skipped.
		expect(out.map((r) => r.varName)).toEqual(["A", "B"]);
	});

	it("env: emits all matches on a line when flag is absent (default)", () => {
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "destructure"
regex = '\\{([^}]+)\\}\\s*=\\s*process\\.env'
confidence = 0.9
access_kind = "unknown"
access_pattern = "process_env_destructure"
hook = "split_csv"
`;
		const splitHook: EnvHook = (ctx) => {
			const body = ctx.match[1];
			if (body === undefined) return { skip: true };
			return {
				records: body.split(",").map((v) => ({
					varName: v.trim(),
					accessKind: "unknown" as const,
				})),
			};
		};
		const env = new Map<string, EnvHookEntry>([
			[
				"split_csv",
				{
					fn: splitHook,
					supplies: new Set(["varName", "accessKind"]),
				},
			],
		]);
		const registry = makeRegistry({ env });
		const graph = loadDetectorGraph(toml, registry);
		const out = walkEnvDetectors(
			graph,
			registry,
			"ts",
			"const {A, B} = process.env; const {C, D} = process.env",
			"f.ts",
		);
		// Default behavior: both destructures processed.
		expect(out.map((r) => r.varName).sort()).toEqual(["A", "B", "C", "D"]);
	});

	it("fs: stops after the first regex match on a line when flag is true", () => {
		const toml = `
[[detector]]
language = "java"
family = "fs"
pattern_id = "files_write"
regex = '\\bFiles\\.write\\s*\\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"java",
			"Files.write(a); Files.write(b);",
			"F.java",
		);
		expect(out).toHaveLength(1);
	});

	it("fs: still applies on subsequent lines when flag is true", () => {
		const toml = `
[[detector]]
language = "java"
family = "fs"
pattern_id = "files_write"
regex = '\\bFiles\\.write\\s*\\('
confidence = 0.85
base_kind = "write_file"
base_pattern = "java_files_write"
single_match_per_line = true
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkFsDetectors(
			graph,
			EMPTY_REGISTRY,
			"java",
			"Files.write(a); Files.write(b);\nFiles.write(c);",
			"F.java",
		);
		// First line: 1 match (single_match_per_line wins). Second line: 1 match.
		expect(out).toHaveLength(2);
		expect(out.map((r) => r.lineNumber)).toEqual([1, 2]);
	});
});

// ── Declaration order preservation ────────────────────────────────

describe("walker — declaration order", () => {
	it("env walker iterates records in TOML declaration order", () => {
		// Two records that BOTH match `x`, but only the first will
		// claim the (file, varName) key due to dedup. Order matters.
		const toml = `
[[detector]]
language = "ts"
family = "env"
pattern_id = "first"
regex = '\\b([A-Z]+)\\b'
confidence = 0.95
access_kind = "required"
access_pattern = "process_env_dot"

[[detector]]
language = "ts"
family = "env"
pattern_id = "second"
regex = '\\b([A-Z]+)\\b'
confidence = 0.5
access_kind = "optional"
access_pattern = "process_env_bracket"
`;
		const graph = loadDetectorGraph(toml, EMPTY_REGISTRY);
		const out = walkEnvDetectors(
			graph,
			EMPTY_REGISTRY,
			"ts",
			"PORT",
			"f.ts",
		);
		expect(out).toHaveLength(1);
		// First record wins.
		expect(out[0].accessKind).toBe("required");
		expect(out[0].accessPattern).toBe("process_env_dot");
		expect(out[0].confidence).toBe(0.95);
	});
});
