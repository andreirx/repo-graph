/**
 * Java receiver resolver — hover parser + validation tests.
 *
 * Tests the pure functions that gate what enrichment data is persisted.
 * Does NOT require jdtls to be installed (pure unit tests).
 */

import { describe, expect, it } from "vitest";
import {
	extractJavaTypeFromHover,
	isValidJavaTypeName,
} from "../../../src/adapters/enrichment/java-receiver-resolver.js";

// ── extractJavaTypeFromHover ────────────────────────────────────────

describe("extractJavaTypeFromHover — good payloads", () => {
	it("extracts type from variable declaration", () => {
		expect(extractJavaTypeFromHover("String name")).toBe("String");
	});

	it("extracts type from generic declaration", () => {
		expect(extractJavaTypeFromHover("List<String> items")).toBe("List");
	});

	it("extracts type from fully qualified class hover", () => {
		expect(extractJavaTypeFromHover(
			"org.springframework.web.bind.annotation.InitBinder",
		)).toBe("InitBinder");
	});

	it("extracts class name from class declaration", () => {
		expect(extractJavaTypeFromHover("class OwnerController")).toBe("OwnerController");
	});

	it("extracts interface name", () => {
		expect(extractJavaTypeFromHover("interface Repository")).toBe("Repository");
	});

	it("extracts return type from method signature", () => {
		expect(extractJavaTypeFromHover("public String getName()")).toBe("String");
	});

	it("extracts plain PascalCase type", () => {
		expect(extractJavaTypeFromHover("ResponseEntity")).toBe("ResponseEntity");
	});

	it("handles HashMap with generics", () => {
		expect(extractJavaTypeFromHover("HashMap<String, Integer> map")).toBe("HashMap");
	});
});

describe("extractJavaTypeFromHover — bad payloads", () => {
	it("returns null for empty string", () => {
		expect(extractJavaTypeFromHover("")).toBeNull();
	});

	it("returns null for just whitespace", () => {
		expect(extractJavaTypeFromHover("   ")).toBeNull();
	});

	it("returns null for lowercase-only text (likely variable name)", () => {
		// "org" alone is not a useful type.
		expect(extractJavaTypeFromHover("org")).toBeNull();
	});
});

// ── isValidJavaTypeName ─────────────────────────────────────────────

describe("isValidJavaTypeName — accepts valid types", () => {
	it("accepts PascalCase type names", () => {
		expect(isValidJavaTypeName("String")).toBe(true);
		expect(isValidJavaTypeName("HashMap")).toBe(true);
		expect(isValidJavaTypeName("ResponseEntity")).toBe(true);
	});

	it("accepts known primitive types", () => {
		expect(isValidJavaTypeName("int")).toBe(true);
		expect(isValidJavaTypeName("long")).toBe(true);
		expect(isValidJavaTypeName("boolean")).toBe(true);
	});
});

describe("isValidJavaTypeName — rejects non-types", () => {
	it("rejects Java keywords", () => {
		expect(isValidJavaTypeName("void")).toBe(false);
		expect(isValidJavaTypeName("null")).toBe(false);
		expect(isValidJavaTypeName("this")).toBe(false);
		expect(isValidJavaTypeName("class")).toBe(false);
		expect(isValidJavaTypeName("return")).toBe(false);
		expect(isValidJavaTypeName("public")).toBe(false);
	});

	it("rejects single character", () => {
		expect(isValidJavaTypeName("x")).toBe(false);
	});

	it("rejects empty string", () => {
		expect(isValidJavaTypeName("")).toBe(false);
	});

	it("rejects names with newlines", () => {
		expect(isValidJavaTypeName("String\nfoo")).toBe(false);
	});
});

// ── Pipeline: extract + validate ────────────────────────────────────

describe("Java hover parser pipeline", () => {
	function parseAndValidate(hover: string): string | null {
		const extracted = extractJavaTypeFromHover(hover);
		if (!extracted) return null;
		return isValidJavaTypeName(extracted) ? extracted : null;
	}

	it("String declaration → String (valid)", () => {
		expect(parseAndValidate("String name")).toBe("String");
	});

	it("qualified Spring annotation → InitBinder (valid)", () => {
		expect(parseAndValidate(
			"org.springframework.web.bind.annotation.InitBinder",
		)).toBe("InitBinder");
	});

	it("empty string → null", () => {
		expect(parseAndValidate("")).toBeNull();
	});

	it("bare 'org' → null (not a type)", () => {
		expect(parseAndValidate("org")).toBeNull();
	});

	it("List<String> items → List (valid)", () => {
		expect(parseAndValidate("List<String> items")).toBe("List");
	});
});
