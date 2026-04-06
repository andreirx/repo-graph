/**
 * Rust receiver resolver — hover parser + validation tests.
 *
 * Tests the pure functions that gate what enrichment data is persisted.
 * Does NOT require rust-analyzer to be installed (pure unit tests).
 */

import { describe, expect, it } from "vitest";
import {
	extractTypeFromHover,
	isValidRustTypeName,
} from "../../../src/adapters/enrichment/rust-receiver-resolver.js";

// ── extractTypeFromHover ────────────────────────────────────────────

describe("extractTypeFromHover — good hover payloads", () => {
	it("extracts type from let-binding annotation", () => {
		expect(extractTypeFromHover("```rust\nlet lines: Vec<String>\n```")).toBe("Vec");
	});

	it("extracts type from reference (hover on self)", () => {
		expect(extractTypeFromHover("```rust\n&EngineContext\n```")).toBe("EngineContext");
	});

	it("extracts type from &mut reference", () => {
		expect(extractTypeFromHover("```rust\n&mut GameState\n```")).toBe("GameState");
	});

	it("extracts type from struct definition header", () => {
		expect(extractTypeFromHover("```rust\npub struct GameState\n```")).toBe("GameState");
	});

	it("extracts type from enum definition header", () => {
		expect(extractTypeFromHover("```rust\npub enum Status\n```")).toBe("Status");
	});

	it("extracts type from trait definition header", () => {
		expect(extractTypeFromHover("```rust\npub trait Processor\n```")).toBe("Processor");
	});

	it("extracts plain PascalCase type name", () => {
		expect(extractTypeFromHover("```rust\nHashMap\n```")).toBe("HashMap");
	});

	it("strips generic parameters from type annotation", () => {
		const result = extractTypeFromHover("```rust\nlet map: HashMap<String, i32>\n```");
		expect(result).toBe("HashMap");
	});

	it("handles field annotation (name: Type)", () => {
		expect(extractTypeFromHover("```rust\nscore: u32\n```")).toBe("u32");
	});

	it("extracts last segment from qualified path (crate::engine::EngineContext)", () => {
		expect(extractTypeFromHover("```rust\nlet ctx: crate::engine::EngineContext\n```")).toBe("EngineContext");
	});

	it("extracts last segment from std qualified path (std::collections::HashMap)", () => {
		expect(extractTypeFromHover("```rust\nlet map: std::collections::HashMap<String, i32>\n```")).toBe("HashMap");
	});

	it("extracts last segment from alloc path (alloc::vec::Vec)", () => {
		expect(extractTypeFromHover("```rust\nlet v: alloc::vec::Vec<u8>\n```")).toBe("Vec");
	});
});

describe("extractTypeFromHover — bad/ambiguous hover payloads", () => {
	it("returns null for empty hover", () => {
		expect(extractTypeFromHover("")).toBeNull();
	});

	it("returns null for only markdown fences", () => {
		expect(extractTypeFromHover("```rust\n```")).toBeNull();
	});

	it("returns null for bare self (no type info)", () => {
		// If hover only shows "self" without a type, should return null.
		expect(extractTypeFromHover("```rust\nself\n```")).toBeNull();
	});

	it("returns null for &self without type name", () => {
		expect(extractTypeFromHover("```rust\n&self\n```")).toBeNull();
	});

	it("returns null for fn signature (not a type)", () => {
		// Hovering on a function name shows the signature.
		expect(extractTypeFromHover("```rust\npub fn process(input: &str)\n```")).toBeNull();
	});

	it("returns null for let without type annotation", () => {
		expect(extractTypeFromHover("```rust\nlet x = 5\n```")).toBeNull();
	});
});

// ── isValidRustTypeName ─────────────────────────────────────────────

describe("isValidRustTypeName — accepts valid types", () => {
	it("accepts PascalCase type names", () => {
		expect(isValidRustTypeName("Vec")).toBe(true);
		expect(isValidRustTypeName("HashMap")).toBe(true);
		expect(isValidRustTypeName("EngineContext")).toBe(true);
		expect(isValidRustTypeName("GameState")).toBe(true);
	});

	it("accepts known primitive types", () => {
		expect(isValidRustTypeName("u32")).toBe(true);
		expect(isValidRustTypeName("f64")).toBe(true);
		expect(isValidRustTypeName("bool")).toBe(true);
		expect(isValidRustTypeName("str")).toBe(true);
		expect(isValidRustTypeName("usize")).toBe(true);
	});
});

describe("isValidRustTypeName — rejects non-types", () => {
	it("rejects self", () => {
		expect(isValidRustTypeName("self")).toBe(false);
	});

	it("rejects Rust keywords", () => {
		expect(isValidRustTypeName("let")).toBe(false);
		expect(isValidRustTypeName("mut")).toBe(false);
		expect(isValidRustTypeName("fn")).toBe(false);
		expect(isValidRustTypeName("impl")).toBe(false);
		expect(isValidRustTypeName("pub")).toBe(false);
		expect(isValidRustTypeName("const")).toBe(false);
	});

	it("rejects single-character names", () => {
		expect(isValidRustTypeName("e")).toBe(false);
		expect(isValidRustTypeName("x")).toBe(false);
	});

	it("rejects empty string", () => {
		expect(isValidRustTypeName("")).toBe(false);
	});

	it("rejects names with newlines (markdown leaks)", () => {
		expect(isValidRustTypeName("AssetManifest\n\npub struct")).toBe(false);
	});

	it("rejects common hover artifacts", () => {
		expect(isValidRustTypeName("test")).toBe(false);
		expect(isValidRustTypeName("def")).toBe(false);
		expect(isValidRustTypeName("any")).toBe(false);
		expect(isValidRustTypeName("unknown")).toBe(false);
		expect(isValidRustTypeName("{unknown}")).toBe(false);
	});

	it("rejects lowercase non-primitive identifiers", () => {
		// These are likely variable names, not types.
		expect(isValidRustTypeName("name")).toBe(false);
		expect(isValidRustTypeName("manifest")).toBe(false);
		expect(isValidRustTypeName("config")).toBe(false);
	});
});

// ── Integration: extractTypeFromHover + isValidRustTypeName ─────────

describe("hover parser + validation pipeline", () => {
	function parseAndValidate(hover: string): string | null {
		const extracted = extractTypeFromHover(hover);
		if (!extracted) return null;
		return isValidRustTypeName(extracted) ? extracted : null;
	}

	it("Vec<String> annotation → Vec (valid)", () => {
		expect(parseAndValidate("```rust\nlet v: Vec<String>\n```")).toBe("Vec");
	});

	it("&EngineContext reference → EngineContext (valid)", () => {
		expect(parseAndValidate("```rust\n&EngineContext\n```")).toBe("EngineContext");
	});

	it("bare self → null (rejected)", () => {
		expect(parseAndValidate("```rust\nself\n```")).toBeNull();
	});

	it("let without annotation → null (no type)", () => {
		expect(parseAndValidate("```rust\nlet x = 5\n```")).toBeNull();
	});

	it("u32 field annotation → u32 (valid primitive)", () => {
		expect(parseAndValidate("```rust\nscore: u32\n```")).toBe("u32");
	});

	it("qualified crate path → EngineContext (valid, last segment)", () => {
		expect(parseAndValidate("```rust\nlet ctx: crate::engine::EngineContext\n```")).toBe("EngineContext");
	});

	it("std::collections::HashMap → HashMap (valid, last segment)", () => {
		expect(parseAndValidate("```rust\nlet m: std::collections::HashMap<String, u32>\n```")).toBe("HashMap");
	});
});
