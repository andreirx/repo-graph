/**
 * Java extractor — unit tests.
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { JavaExtractor } from "../../../../src/adapters/extractors/java/java-extractor.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Visibility,
} from "../../../../src/core/model/index.js";
import type { ExtractionResult } from "../../../../src/core/ports/extractor.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../../fixtures/java/simple-project/src/main/java/com/example",
);
const REPO_UID = "test-java";
const SNAPSHOT_UID = "test-snapshot";

let extractor: JavaExtractor;

async function extractFile(filename: string): Promise<ExtractionResult> {
	const filePath = `src/main/java/com/example/${filename}`;
	const fileUid = `${REPO_UID}:${filePath}`;
	const source = await readFile(join(FIXTURE_ROOT, filename), "utf-8");
	return extractor.extract(source, filePath, fileUid, REPO_UID, SNAPSHOT_UID);
}

beforeAll(async () => {
	extractor = new JavaExtractor();
	await extractor.initialize();
});

describe("Java extractor — port contract", () => {
	it("has the correct name and languages", () => {
		expect(extractor.name).toBe("java-core:0.1.0");
		expect(extractor.languages).toEqual(["java"]);
	});

	it("exposes Java runtime builtins", () => {
		expect(extractor.runtimeBuiltins.identifiers).toContain("String");
		expect(extractor.runtimeBuiltins.identifiers).toContain("HashMap");
		expect(extractor.runtimeBuiltins.identifiers).toContain("List");
		expect(extractor.runtimeBuiltins.moduleSpecifiers).toContain("java.util");
	});
});

describe("Java extractor — FILE node", () => {
	it("emits one FILE node per file", async () => {
		const result = await extractFile("App.java");
		const fileNodes = result.nodes.filter((n) => n.kind === NodeKind.FILE);
		expect(fileNodes.length).toBe(1);
	});
});

describe("Java extractor — SYMBOL nodes", () => {
	it("extracts public class as CLASS with EXPORT visibility", async () => {
		const result = await extractFile("App.java");
		const appClass = result.nodes.find(
			(n) => n.name === "App" && n.subtype === NodeSubtype.CLASS,
		);
		expect(appClass).toBeDefined();
		expect(appClass?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts public interface as INTERFACE", async () => {
		const result = await extractFile("Service.java");
		const iface = result.nodes.find(
			(n) => n.name === "Service" && n.subtype === NodeSubtype.INTERFACE,
		);
		expect(iface).toBeDefined();
	});

	it("extracts methods with type-signature-disambiguated qualified names", async () => {
		const result = await extractFile("App.java");
		const methods = result.nodes.filter((n) => n.subtype === NodeSubtype.METHOD);
		expect(methods.length).toBeGreaterThan(0);
		// addScore has two overloads with DIFFERENT param types:
		// addScore(String, int) and addScore(String).
		// Same-arity overloads would have collided with count-only keys.
		const addScore2 = methods.find((n) => n.qualifiedName === "App.addScore(String,int)");
		const addScore1 = methods.find((n) => n.qualifiedName === "App.addScore(String)");
		expect(addScore2).toBeDefined();
		expect(addScore1).toBeDefined();
		expect(addScore2?.stableKey).not.toBe(addScore1?.stableKey);
	});

	it("disambiguates SAME-ARITY overloads by param types (P1 regression)", async () => {
		const result = await extractFile("App.java");
		const methods = result.nodes.filter((n) => n.subtype === NodeSubtype.METHOD);
		// format(String) and format(Integer) — SAME arity, DIFFERENT types.
		// This is the exact case that was colliding with count-only keys.
		const formatStr = methods.find((n) => n.qualifiedName === "App.format(String)");
		const formatInt = methods.find((n) => n.qualifiedName === "App.format(Integer)");
		expect(formatStr).toBeDefined();
		expect(formatInt).toBeDefined();
		expect(formatStr?.stableKey).not.toBe(formatInt?.stableKey);
	});

	it("disambiguates overloaded constructors by param types", async () => {
		const result = await extractFile("App.java");
		const ctors = result.nodes.filter((n) => n.subtype === NodeSubtype.CONSTRUCTOR);
		const ctor1 = ctors.find((n) => n.qualifiedName === "App.App(String)");
		const ctor0 = ctors.find((n) => n.qualifiedName === "App.App()");
		expect(ctor1).toBeDefined();
		expect(ctor0).toBeDefined();
		expect(ctor1?.stableKey).not.toBe(ctor0?.stableKey);
	});

	it("extracts private method with PRIVATE visibility", async () => {
		const result = await extractFile("App.java");
		const helper = result.nodes.find((n) => n.name === "helper");
		expect(helper).toBeDefined();
		expect(helper?.visibility).toBe(Visibility.PRIVATE);
	});

	it("no duplicate stable_keys", async () => {
		const result = await extractFile("App.java");
		const keys = result.nodes.map((n) => n.stableKey);
		expect(keys.length).toBe(new Set(keys).size);
	});
});

describe("Java extractor — IMPORTS edges", () => {
	it("emits IMPORTS edges for import declarations", async () => {
		const result = await extractFile("App.java");
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBeGreaterThanOrEqual(1);
	});
});

describe("Java extractor — import bindings", () => {
	it("emits import bindings with correct identifiers", async () => {
		const result = await extractFile("App.java");
		const hashMapBinding = result.importBindings.find(
			(b) => b.identifier === "HashMap",
		);
		expect(hashMapBinding).toBeDefined();
		expect(hashMapBinding?.specifier).toBe("java.util");
	});
});

describe("Java extractor — CALLS edges", () => {
	it("emits CALLS edges from method bodies", async () => {
		const result = await extractFile("App.java");
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		expect(calls.length).toBeGreaterThan(0);
	});
});
