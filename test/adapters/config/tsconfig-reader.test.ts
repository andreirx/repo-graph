/**
 * tsconfig alias reader tests.
 *
 * Uses a tmpdir per test to isolate filesystem state. Every test
 * writes a tsconfig.json, calls the reader, then cleans up.
 */

import { randomUUID } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { readTsconfigAliases } from "../../../src/adapters/config/tsconfig-reader.js";

let workDir: string;

beforeEach(() => {
	workDir = mkdtempSync(join(tmpdir(), `rgr-tsconfig-${randomUUID()}-`));
});

afterEach(() => {
	try {
		rmSync(workDir, { recursive: true, force: true });
	} catch {
		// ignore
	}
});

function writeTsconfig(content: string): void {
	writeFileSync(join(workDir, "tsconfig.json"), content, "utf-8");
}

describe("readTsconfigAliases — happy path", () => {
	it("reads compilerOptions.paths into alias entries", async () => {
		writeTsconfig(
			JSON.stringify({
				compilerOptions: {
					paths: {
						"@/*": ["./src/*"],
						"@types": ["./src/types/index.ts"],
					},
				},
			}),
		);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases).not.toBeNull();
		expect(aliases?.entries.length).toBe(2);
		const byPattern = new Map(
			aliases?.entries.map((e) => [e.pattern, e.substitutions]),
		);
		expect(byPattern.get("@/*")).toEqual(["./src/*"]);
		expect(byPattern.get("@types")).toEqual(["./src/types/index.ts"]);
	});

	it("returns empty entries when compilerOptions.paths is absent", async () => {
		writeTsconfig(
			JSON.stringify({
				compilerOptions: {
					target: "esnext",
				},
			}),
		);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases).not.toBeNull();
		expect(aliases?.entries).toEqual([]);
	});

	it("returns empty entries when compilerOptions is absent", async () => {
		writeTsconfig(JSON.stringify({}));
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases).not.toBeNull();
		expect(aliases?.entries).toEqual([]);
	});
});

describe("readTsconfigAliases — JSONC handling", () => {
	it("strips single-line comments before parsing", async () => {
		writeTsconfig(`
			// This is a config file comment.
			{
				"compilerOptions": {
					// Paths configuration:
					"paths": {
						"@/*": ["./src/*"]
					}
				}
			}
		`);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@/*");
	});

	it("strips block comments before parsing", async () => {
		writeTsconfig(`
			/* Header comment
			   spanning multiple lines */
			{
				"compilerOptions": {
					/* inline block */
					"paths": {
						"@/*": ["./src/*"]
					}
				}
			}
		`);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries.length).toBe(1);
	});

	it("strips both comment styles together", async () => {
		writeTsconfig(`
			// line comment
			{
				/* block */
				"compilerOptions": {
					"paths": { "@/*": ["./src/*"] } // trailing
				}
			}
		`);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries.length).toBe(1);
	});
});

describe("readTsconfigAliases — failure modes", () => {
	it("returns null when tsconfig.json does not exist", async () => {
		expect(await readTsconfigAliases(workDir)).toBeNull();
	});

	it("returns null for unparseable JSON (even after comment strip)", async () => {
		writeTsconfig("{ this: is: not: valid: json }");
		expect(await readTsconfigAliases(workDir)).toBeNull();
	});

	it("returns null when content is empty", async () => {
		writeTsconfig("");
		expect(await readTsconfigAliases(workDir)).toBeNull();
	});

	it("returns empty entries when paths is malformed (not an object)", async () => {
		writeTsconfig(
			JSON.stringify({
				compilerOptions: { paths: "not an object" },
			}),
		);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries).toEqual([]);
	});

	it("preserves pattern with empty substitutions if substitutions malformed", async () => {
		writeTsconfig(
			JSON.stringify({
				compilerOptions: {
					paths: {
						"@/*": "not an array",
					},
				},
			}),
		);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@/*");
		expect(aliases?.entries[0].substitutions).toEqual([]);
	});

	it("filters non-string entries from substitutions array", async () => {
		writeTsconfig(
			JSON.stringify({
				compilerOptions: {
					paths: {
						"@/*": ["./src/*", 42, null, "./lib/*"],
					},
				},
			}),
		);
		const aliases = await readTsconfigAliases(workDir);
		expect(aliases?.entries[0].substitutions).toEqual(["./src/*", "./lib/*"]);
	});
});
