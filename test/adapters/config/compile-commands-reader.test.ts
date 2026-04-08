/**
 * compile_commands.json reader — unit tests.
 */

import { describe, expect, it } from "vitest";
import { parseCompileCommands } from "../../../src/adapters/config/compile-commands-reader.js";

const REPO_ROOT = "/home/user/project";

function parse(entries: unknown[]) {
	return parseCompileCommands(JSON.stringify(entries), REPO_ROOT);
}

// ── Basic parsing ───────────────────────────────────────────────────

describe("parseCompileCommands — basic", () => {
	it("parses a single entry with arguments array", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-Iinclude", "-DFOO=1", "-c", "src/main.c", "-o", "main.o"],
		}]);
		expect(db.entries.size).toBe(1);
		const entry = db.entries.get("src/main.c");
		expect(entry).toBeDefined();
		expect(entry!.includePaths).toContain("include");
		expect(entry!.defines).toContain("FOO=1");
	});

	it("parses a single entry with command string", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			command: "gcc -Iinclude -DFOO=1 -c src/main.c -o main.o",
		}]);
		expect(db.entries.size).toBe(1);
		expect(db.entries.get("src/main.c")!.includePaths).toContain("include");
	});

	it("parses multiple entries", () => {
		const db = parse([
			{
				directory: REPO_ROOT,
				file: "src/a.c",
				arguments: ["gcc", "-c", "src/a.c"],
			},
			{
				directory: REPO_ROOT,
				file: "src/b.c",
				arguments: ["gcc", "-c", "src/b.c"],
			},
		]);
		expect(db.entries.size).toBe(2);
		expect(db.entries.has("src/a.c")).toBe(true);
		expect(db.entries.has("src/b.c")).toBe(true);
	});
});

// ── Include paths ───────────────────────────────────────────────────

describe("parseCompileCommands — include paths", () => {
	it("extracts -I flag with space separator", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-I", "include", "-c", "src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.includePaths).toContain("include");
	});

	it("extracts -I flag without space", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-Iinclude", "-c", "src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.includePaths).toContain("include");
	});

	it("extracts multiple -I flags", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-Iinclude", "-Ilib/include", "-c", "src/main.c"],
		}]);
		const paths = db.entries.get("src/main.c")!.includePaths;
		expect(paths).toContain("include");
		expect(paths).toContain("lib/include");
	});

	it("resolves relative include paths against directory", () => {
		const db = parse([{
			directory: REPO_ROOT + "/build",
			file: REPO_ROOT + "/src/main.c",
			arguments: ["gcc", "-I../include", "-c", "../src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.includePaths).toContain("include");
	});

	it("collects allIncludePaths from all entries", () => {
		const db = parse([
			{
				directory: REPO_ROOT,
				file: "src/a.c",
				arguments: ["gcc", "-Iinclude", "-c", "src/a.c"],
			},
			{
				directory: REPO_ROOT,
				file: "src/b.c",
				arguments: ["gcc", "-Ilib", "-c", "src/b.c"],
			},
		]);
		expect(db.allIncludePaths).toContain("include");
		expect(db.allIncludePaths).toContain("lib");
	});

	it("deduplicates allIncludePaths", () => {
		const db = parse([
			{
				directory: REPO_ROOT,
				file: "src/a.c",
				arguments: ["gcc", "-Iinclude", "-c", "src/a.c"],
			},
			{
				directory: REPO_ROOT,
				file: "src/b.c",
				arguments: ["gcc", "-Iinclude", "-c", "src/b.c"],
			},
		]);
		const includeCount = db.allIncludePaths.filter((p) => p === "include").length;
		expect(includeCount).toBe(1);
	});
});

// ── Defines ─────────────────────────────────────────────────────────

describe("parseCompileCommands — defines", () => {
	it("extracts -D flag without space", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-DFOO", "-c", "src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.defines).toContain("FOO");
	});

	it("extracts -D flag with value", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-DVERSION=2", "-c", "src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.defines).toContain("VERSION=2");
	});

	it("extracts -D flag with space separator", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			arguments: ["gcc", "-D", "BAR", "-c", "src/main.c"],
		}]);
		expect(db.entries.get("src/main.c")!.defines).toContain("BAR");
	});
});

// ── Edge cases ──────────────────────────────────────────────────────

describe("parseCompileCommands — edge cases", () => {
	it("returns empty for invalid JSON", () => {
		const db = parseCompileCommands("not json", REPO_ROOT);
		expect(db.entries.size).toBe(0);
	});

	it("returns empty for non-array JSON", () => {
		const db = parseCompileCommands('{"foo": 1}', REPO_ROOT);
		expect(db.entries.size).toBe(0);
	});

	it("skips entries without file field", () => {
		const db = parse([{
			directory: REPO_ROOT,
			arguments: ["gcc", "-c", "src/main.c"],
		}]);
		expect(db.entries.size).toBe(0);
	});

	it("handles absolute file paths", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: REPO_ROOT + "/src/main.c",
			arguments: ["gcc", "-c", REPO_ROOT + "/src/main.c"],
		}]);
		expect(db.entries.has("src/main.c")).toBe(true);
	});

	it("skips files outside repo root", () => {
		const db = parse([{
			directory: "/other/project",
			file: "/other/project/src/main.c",
			arguments: ["gcc", "-c", "/other/project/src/main.c"],
		}]);
		expect(db.entries.size).toBe(0);
	});

	it("handles command string with quoted paths", () => {
		const db = parse([{
			directory: REPO_ROOT,
			file: "src/main.c",
			command: 'gcc -I"include dir" -c src/main.c',
		}]);
		expect(db.entries.get("src/main.c")!.includePaths).toContain("include dir");
	});
});
