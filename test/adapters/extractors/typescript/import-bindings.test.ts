/**
 * Import-binding side-channel emission tests.
 *
 * Covers the extractor-level contract added in step 3.1:
 *
 *   - ImportBinding records are emitted for every identifier-bearing
 *     static import (relative + non-relative + aliased + default +
 *     namespace + named + renamed + default+named combinations).
 *
 *   - Side-effect imports (`import "m"`) produce no bindings.
 *
 *   - Statement-level `import type` propagates isTypeOnly=true to
 *     every binding in that statement.
 *
 *   - isRelative is TRUE iff the specifier starts with "."  — never
 *     for bare or tsconfig-alias-shaped specifiers.
 *
 *   - Unresolved IMPORTS EDGE emission is unchanged from prior
 *     behavior: only relative specifiers produce edges. Non-relative
 *     specifiers produce importBindings records but NO new edges —
 *     trust posture is preserved.
 *
 * These tests use inline source strings rather than fixture files so
 * the import-statement coverage is explicit in the test body.
 */

import { beforeAll, describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../../../src/adapters/extractors/typescript/ts-extractor.js";
import { EdgeType } from "../../../../src/core/model/index.js";
import type { ExtractionResult } from "../../../../src/core/ports/extractor.js";

const REPO_UID = "test-repo";
const SNAPSHOT_UID = "test-snapshot";
const FILE_PATH = "src/entry.ts";
const FILE_UID = `${REPO_UID}:${FILE_PATH}`;

let extractor: TypeScriptExtractor;

async function extractSource(source: string): Promise<ExtractionResult> {
	return extractor.extract(
		source,
		FILE_PATH,
		FILE_UID,
		REPO_UID,
		SNAPSHOT_UID,
	);
}

beforeAll(async () => {
	extractor = new TypeScriptExtractor();
	await extractor.initialize();
});

// ── Per-form coverage ───────────────────────────────────────────────

describe("importBindings — per import form", () => {
	it("default import: one binding with the default identifier", async () => {
		const result = await extractSource(`import React from "react";\n`);
		expect(result.importBindings.length).toBe(1);
		expect(result.importBindings[0].identifier).toBe("React");
		expect(result.importBindings[0].specifier).toBe("react");
		expect(result.importBindings[0].isRelative).toBe(false);
		expect(result.importBindings[0].isTypeOnly).toBe(false);
	});

	it("named imports: one binding per specifier", async () => {
		const result = await extractSource(`import { a, b, c } from "lib";\n`);
		const names = result.importBindings.map((b) => b.identifier);
		expect(names).toEqual(["a", "b", "c"]);
		for (const binding of result.importBindings) {
			expect(binding.specifier).toBe("lib");
			expect(binding.isRelative).toBe(false);
		}
	});

	it("renamed named import: local alias is the identifier", async () => {
		const result = await extractSource(
			`import { foo as bar } from "lib";\n`,
		);
		expect(result.importBindings.length).toBe(1);
		expect(result.importBindings[0].identifier).toBe("bar");
		expect(result.importBindings[0].specifier).toBe("lib");
	});

	it("namespace import: namespace alias is the identifier", async () => {
		const result = await extractSource(`import * as utils from "lib";\n`);
		expect(result.importBindings.length).toBe(1);
		expect(result.importBindings[0].identifier).toBe("utils");
		expect(result.importBindings[0].specifier).toBe("lib");
	});

	it("default + named combined: all bindings emitted", async () => {
		const result = await extractSource(
			`import React, { useState, useEffect } from "react";\n`,
		);
		const names = result.importBindings.map((b) => b.identifier);
		expect(names).toEqual(["React", "useState", "useEffect"]);
	});

	it("side-effect import: no bindings emitted", async () => {
		const result = await extractSource(`import "./styles.css";\n`);
		expect(result.importBindings.length).toBe(0);
	});
});

// ── isTypeOnly handling ─────────────────────────────────────────────

describe("importBindings — isTypeOnly", () => {
	it("statement-level `import type` flags all bindings as type-only", async () => {
		const result = await extractSource(
			`import type { FooType, BarType } from "./types";\n`,
		);
		expect(result.importBindings.length).toBe(2);
		for (const binding of result.importBindings) {
			expect(binding.isTypeOnly).toBe(true);
		}
	});

	it("non-type-only import: isTypeOnly is false", async () => {
		const result = await extractSource(`import { Foo } from "./types";\n`);
		expect(result.importBindings.length).toBe(1);
		expect(result.importBindings[0].isTypeOnly).toBe(false);
	});

	it("specifier-level `{ type X }` is NOT distinguished (first-slice scope)", async () => {
		// First-slice behavior: specifier-level type keyword is not
		// captured — both X (type-only) and Y (value) are emitted with
		// isTypeOnly=false because the containing statement is NOT
		// `import type`.
		const result = await extractSource(
			`import { type X, Y } from "./mixed";\n`,
		);
		expect(result.importBindings.length).toBe(2);
		for (const binding of result.importBindings) {
			expect(binding.isTypeOnly).toBe(false);
		}
	});
});

// ── isRelative classification ───────────────────────────────────────

describe("importBindings — isRelative", () => {
	it("relative specifier './...' → isRelative=true", async () => {
		const result = await extractSource(`import x from "./local";\n`);
		expect(result.importBindings[0].isRelative).toBe(true);
	});

	it("relative specifier '../...' → isRelative=true", async () => {
		const result = await extractSource(`import x from "../sibling";\n`);
		expect(result.importBindings[0].isRelative).toBe(true);
	});

	it("bare package specifier → isRelative=false", async () => {
		const result = await extractSource(`import x from "lodash";\n`);
		expect(result.importBindings[0].isRelative).toBe(false);
	});

	it("scoped package specifier → isRelative=false", async () => {
		const result = await extractSource(`import x from "@acme/utils";\n`);
		expect(result.importBindings[0].isRelative).toBe(false);
	});

	it("alias-shaped specifier '@/...' → isRelative=false (classifier handles alias)", async () => {
		const result = await extractSource(`import x from "@/lib/foo";\n`);
		expect(result.importBindings[0].isRelative).toBe(false);
	});

	it("absolute path '/...' → isRelative=false (not classified as relative in first slice)", async () => {
		const result = await extractSource(`import x from "/abs/path";\n`);
		expect(result.importBindings[0].isRelative).toBe(false);
	});
});

// ── Trust-posture preservation: edge emission unchanged ─────────────

describe("importBindings — edge emission preserved", () => {
	it("relative import still produces an IMPORTS edge", async () => {
		const result = await extractSource(`import { Foo } from "./foo";\n`);
		const importEdges = result.edges.filter(
			(e) => e.type === EdgeType.IMPORTS,
		);
		expect(importEdges.length).toBe(1);
		// And binding is emitted too
		expect(result.importBindings.length).toBe(1);
	});

	it("non-relative import produces binding but NO edge", async () => {
		const result = await extractSource(`import React from "react";\n`);
		const importEdges = result.edges.filter(
			(e) => e.type === EdgeType.IMPORTS,
		);
		expect(importEdges.length).toBe(0);
		expect(result.importBindings.length).toBe(1);
	});

	it("alias-shaped import produces binding but NO edge", async () => {
		const result = await extractSource(`import x from "@/lib/foo";\n`);
		const importEdges = result.edges.filter(
			(e) => e.type === EdgeType.IMPORTS,
		);
		expect(importEdges.length).toBe(0);
		expect(result.importBindings.length).toBe(1);
	});

	it("mix of relative and non-relative: edges only for relative, bindings for both", async () => {
		const result = await extractSource(
			`import React from "react";\n` +
				`import { Foo } from "./foo";\n` +
				`import { Bar } from "@/lib/bar";\n`,
		);
		const importEdges = result.edges.filter(
			(e) => e.type === EdgeType.IMPORTS,
		);
		expect(importEdges.length).toBe(1);
		expect(result.importBindings.length).toBe(3);
		const specifiers = result.importBindings.map((b) => b.specifier).sort();
		expect(specifiers).toEqual(["./foo", "@/lib/bar", "react"]);
	});
});

// ── Source location ─────────────────────────────────────────────────

describe("importBindings — source location", () => {
	it("location carries the import statement's line", async () => {
		const source = `// leading comment\nimport { foo } from "./bar";\n`;
		const result = await extractSource(source);
		expect(result.importBindings.length).toBe(1);
		expect(result.importBindings[0].location?.lineStart).toBe(2);
	});

	it("every binding in a statement shares the same location", async () => {
		const result = await extractSource(
			`import { a, b, c } from "./multi";\n`,
		);
		const locations = result.importBindings.map((b) => b.location?.lineStart);
		expect(new Set(locations).size).toBe(1);
	});
});
