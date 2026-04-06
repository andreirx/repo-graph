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
import {
	readTsconfigAliases,
	readTsconfigAliasesFromPath,
} from "../../../src/adapters/config/tsconfig-reader.js";

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

describe("readTsconfigAliasesFromPath — extends chain", () => {
	it("inherits paths from parent when child has no paths", async () => {
		// base.json defines paths; child extends it without overriding.
		writeFileSync(
			join(workDir, "base.json"),
			JSON.stringify({
				compilerOptions: {
					paths: { "@shared/*": ["./shared/*"] },
				},
			}),
			"utf-8",
		);
		writeFileSync(
			join(workDir, "tsconfig.json"),
			JSON.stringify({
				extends: "./base.json",
				compilerOptions: { strict: true },
			}),
			"utf-8",
		);
		const aliases = await readTsconfigAliasesFromPath(
			join(workDir, "tsconfig.json"),
		);
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@shared/*");
	});

	it("child paths replace parent paths entirely (TypeScript merge rule)", async () => {
		writeFileSync(
			join(workDir, "base.json"),
			JSON.stringify({
				compilerOptions: {
					paths: { "@base/*": ["./base/*"] },
				},
			}),
			"utf-8",
		);
		writeFileSync(
			join(workDir, "tsconfig.json"),
			JSON.stringify({
				extends: "./base.json",
				compilerOptions: {
					paths: { "@child/*": ["./child/*"] },
				},
			}),
			"utf-8",
		);
		const aliases = await readTsconfigAliasesFromPath(
			join(workDir, "tsconfig.json"),
		);
		// Child has paths → parent paths NOT inherited.
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@child/*");
	});

	it("multi-level extends: grandchild → child → grandparent", async () => {
		writeFileSync(
			join(workDir, "grandparent.json"),
			JSON.stringify({
				compilerOptions: {
					paths: { "@gp/*": ["./gp/*"] },
				},
			}),
			"utf-8",
		);
		writeFileSync(
			join(workDir, "parent.json"),
			JSON.stringify({
				extends: "./grandparent.json",
				compilerOptions: { strict: true },
			}),
			"utf-8",
		);
		writeFileSync(
			join(workDir, "tsconfig.json"),
			JSON.stringify({
				extends: "./parent.json",
				compilerOptions: { strict: true },
			}),
			"utf-8",
		);
		const aliases = await readTsconfigAliasesFromPath(
			join(workDir, "tsconfig.json"),
		);
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@gp/*");
	});

	it("stops at non-relative extends (package-name) and returns empty", async () => {
		writeFileSync(
			join(workDir, "tsconfig.json"),
			JSON.stringify({
				extends: "@tsconfig/node18/tsconfig.json",
				compilerOptions: { strict: true },
			}),
			"utf-8",
		);
		const aliases = await readTsconfigAliasesFromPath(
			join(workDir, "tsconfig.json"),
		);
		expect(aliases?.entries).toEqual([]);
	});

	it("handles JSONC comments in extended configs", async () => {
		writeFileSync(
			join(workDir, "base.json"),
			`{
				// Base config comment
				"compilerOptions": {
					/* block comment */
					"paths": { "@base/*": ["./base/*"] }
				}
			}`,
			"utf-8",
		);
		writeFileSync(
			join(workDir, "tsconfig.json"),
			JSON.stringify({
				extends: "./base.json",
				compilerOptions: { strict: true },
			}),
			"utf-8",
		);
		const aliases = await readTsconfigAliasesFromPath(
			join(workDir, "tsconfig.json"),
		);
		expect(aliases?.entries.length).toBe(1);
		expect(aliases?.entries[0].pattern).toBe("@base/*");
	});
});
