/**
 * package-description-extractor tests.
 *
 * Pure function tests: given raw package.json content, verify the
 * extracted candidate matches expectations.
 */

import { describe, expect, it } from "vitest";
import { extractPackageDescription } from "../../../../src/adapters/annotations/extractors/package-description-extractor.js";

describe("extractPackageDescription", () => {
	it("extracts the description string", () => {
		const src = JSON.stringify(
			{
				name: "my-pkg",
				version: "1.0.0",
				description: "A test package for unit tests.",
			},
			null,
			2,
		);
		const result = extractPackageDescription("package.json", src);
		expect(result).not.toBeNull();
		expect(result!.content).toBe("A test package for unit tests.");
		expect(result!.sourceFile).toBe("package.json");
		expect(result!.sourceLineStart).toBeGreaterThan(0);
		expect(result!.sourceLineEnd).toBe(result!.sourceLineStart);
	});

	it("returns null when description is absent", () => {
		const src = JSON.stringify({ name: "my-pkg", version: "1.0.0" });
		expect(extractPackageDescription("package.json", src)).toBeNull();
	});

	it("returns null when description is empty string", () => {
		const src = JSON.stringify({
			name: "my-pkg",
			version: "1.0.0",
			description: "",
		});
		expect(extractPackageDescription("package.json", src)).toBeNull();
	});

	it("returns null when description is whitespace-only", () => {
		const src = JSON.stringify({
			name: "my-pkg",
			version: "1.0.0",
			description: "   \n  ",
		});
		expect(extractPackageDescription("package.json", src)).toBeNull();
	});

	it("returns null when description is not a string", () => {
		const src = JSON.stringify({
			name: "my-pkg",
			version: "1.0.0",
			description: { nested: "object" },
		});
		expect(extractPackageDescription("package.json", src)).toBeNull();
	});

	it("returns null for invalid JSON", () => {
		expect(
			extractPackageDescription("package.json", "{ not valid json"),
		).toBeNull();
	});

	it("returns null for non-object JSON (array)", () => {
		expect(extractPackageDescription("package.json", "[1, 2, 3]")).toBeNull();
	});

	it("returns null for non-object JSON (null)", () => {
		expect(extractPackageDescription("package.json", "null")).toBeNull();
	});

	it("finds the correct line number of the description key", () => {
		const src = `{
  "name": "my-pkg",
  "version": "1.0.0",
  "description": "line 4",
  "main": "index.js"
}`;
		const result = extractPackageDescription("package.json", src);
		expect(result!.sourceLineStart).toBe(4);
	});

	it("preserves the sourceFile path passed in", () => {
		const src = JSON.stringify({ description: "x" });
		const result = extractPackageDescription(
			"packages/engine/package.json",
			src,
		);
		expect(result!.sourceFile).toBe("packages/engine/package.json");
	});
});
