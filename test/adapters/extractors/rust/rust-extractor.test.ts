/**
 * Rust extractor — unit tests.
 *
 * Uses a dedicated fixture (simple-crate/) with known Rust symbols
 * and verifiable extraction output.
 */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { RustExtractor } from "../../../../src/adapters/extractors/rust/rust-extractor.js";
import {
	EdgeType,
	NodeKind,
	NodeSubtype,
	Visibility,
} from "../../../../src/core/model/index.js";
import type { ExtractionResult } from "../../../../src/core/ports/extractor.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../../fixtures/rust/simple-crate/src",
);
const REPO_UID = "test-rust";
const SNAPSHOT_UID = "test-snapshot";

let extractor: RustExtractor;

async function extractFile(filename: string): Promise<ExtractionResult> {
	const filePath = `src/${filename}`;
	const fileUid = `${REPO_UID}:${filePath}`;
	const source = await readFile(join(FIXTURE_ROOT, filename), "utf-8");
	return extractor.extract(source, filePath, fileUid, REPO_UID, SNAPSHOT_UID);
}

beforeAll(async () => {
	extractor = new RustExtractor();
	await extractor.initialize();
});

describe("Rust extractor — port contract", () => {
	it("has the correct name and languages", () => {
		expect(extractor.name).toBe("rust-core:0.1.0");
		expect(extractor.languages).toEqual(["rust"]);
	});

	it("exposes Rust runtime builtins", () => {
		const builtins = extractor.runtimeBuiltins;
		expect(builtins.identifiers.length).toBeGreaterThan(0);
		expect(builtins.identifiers).toContain("Vec");
		expect(builtins.identifiers).toContain("HashMap");
		expect(builtins.identifiers).toContain("String");
		expect(builtins.identifiers).toContain("Option");
		expect(builtins.identifiers).toContain("Result");
	});
});

describe("Rust extractor — FILE node", () => {
	it("emits one FILE node per file", async () => {
		const result = await extractFile("lib.rs");
		const fileNodes = result.nodes.filter((n) => n.kind === NodeKind.FILE);
		expect(fileNodes.length).toBe(1);
		expect(fileNodes[0].stableKey).toBe(`${REPO_UID}:src/lib.rs:FILE`);
	});
});

describe("Rust extractor — SYMBOL nodes", () => {
	it("extracts pub struct as CLASS with EXPORT visibility", async () => {
		const result = await extractFile("lib.rs");
		const config = result.nodes.find(
			(n) => n.name === "Config" && n.subtype === NodeSubtype.CLASS,
		);
		expect(config).toBeDefined();
		expect(config?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts pub enum as ENUM", async () => {
		const result = await extractFile("lib.rs");
		const status = result.nodes.find(
			(n) => n.name === "Status" && n.subtype === NodeSubtype.ENUM,
		);
		expect(status).toBeDefined();
		expect(status?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts pub trait as INTERFACE", async () => {
		const result = await extractFile("lib.rs");
		const processor = result.nodes.find(
			(n) => n.name === "Processor" && n.subtype === NodeSubtype.INTERFACE,
		);
		expect(processor).toBeDefined();
	});

	it("extracts impl methods as METHOD with qualified name", async () => {
		const result = await extractFile("lib.rs");
		const newMethod = result.nodes.find(
			(n) =>
				n.name === "new" &&
				n.subtype === NodeSubtype.METHOD &&
				n.qualifiedName === "Config.new",
		);
		expect(newMethod).toBeDefined();
	});

	it("extracts pub fn as FUNCTION", async () => {
		const result = await extractFile("lib.rs");
		const createConfig = result.nodes.find(
			(n) => n.name === "create_config" && n.subtype === NodeSubtype.FUNCTION,
		);
		expect(createConfig).toBeDefined();
		expect(createConfig?.visibility).toBe(Visibility.EXPORT);
	});

	it("extracts private fn with PRIVATE visibility", async () => {
		const result = await extractFile("lib.rs");
		const helper = result.nodes.find(
			(n) => n.name === "helper" && n.subtype === NodeSubtype.FUNCTION,
		);
		expect(helper).toBeDefined();
		expect(helper?.visibility).toBe(Visibility.PRIVATE);
	});

	it("extracts const as CONSTANT", async () => {
		const result = await extractFile("lib.rs");
		const maxSize = result.nodes.find(
			(n) => n.name === "MAX_SIZE" && n.subtype === NodeSubtype.CONSTANT,
		);
		expect(maxSize).toBeDefined();
	});

	it("no duplicate stable_keys (cfg dedup)", async () => {
		const result = await extractFile("lib.rs");
		const keys = result.nodes.map((n) => n.stableKey);
		const unique = new Set(keys);
		expect(keys.length).toBe(unique.size);
	});
});

describe("Rust extractor — IMPORTS edges", () => {
	it("emits IMPORTS edge for use declaration", async () => {
		const result = await extractFile("lib.rs");
		const imports = result.edges.filter((e) => e.type === EdgeType.IMPORTS);
		expect(imports.length).toBeGreaterThanOrEqual(1);
		// Should have an import for std::collections::HashMap
		const hashMapImport = imports.find((e) =>
			e.targetKey.includes("HashMap") || e.targetKey.includes("collections"),
		);
		expect(hashMapImport).toBeDefined();
	});
});

describe("Rust extractor — import bindings", () => {
	it("emits import bindings for use items", async () => {
		const result = await extractFile("lib.rs");
		expect(result.importBindings.length).toBeGreaterThanOrEqual(1);
		const hashMapBinding = result.importBindings.find(
			(b) => b.identifier === "HashMap",
		);
		expect(hashMapBinding).toBeDefined();
		expect(hashMapBinding?.specifier).toContain("std::collections");
	});
});

describe("Rust extractor — CALLS edges", () => {
	it("emits CALLS edges from function bodies", async () => {
		const result = await extractFile("lib.rs");
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		expect(calls.length).toBeGreaterThan(0);
	});

	it("emits self.method calls with self receiver", async () => {
		const result = await extractFile("lib.rs");
		const calls = result.edges.filter((e) => e.type === EdgeType.CALLS);
		const selfCall = calls.find(
			(e) => e.targetKey.includes("self.") || e.targetKey.includes("values"),
		);
		// get_value calls self.values.get — should produce a call edge
		expect(selfCall).toBeDefined();
	});
});
