/**
 * readme-extractor tests.
 *
 * Pure function tests: verify candidates are produced correctly
 * for README.md and README.txt, and that empty files are dropped.
 */

import { describe, expect, it } from "vitest";
import { extractReadme } from "../../../../src/adapters/annotations/extractors/readme-extractor.js";

describe("extractReadme", () => {
	it("extracts the full content of README.md", () => {
		const src = "# My Module\n\nThis module does X.";
		const result = extractReadme("src/core/README.md", src);
		expect(result).not.toBeNull();
		expect(result!.content).toBe(src);
		expect(result!.sourceFile).toBe("src/core/README.md");
		expect(result!.language).toBe("markdown");
	});

	it("marks README.txt as language=text", () => {
		const result = extractReadme("src/core/README.txt", "plain text readme");
		expect(result!.language).toBe("text");
	});

	it("treats .md extension case-insensitively", () => {
		const result = extractReadme("README.MD", "# Heading");
		expect(result!.language).toBe("markdown");
	});

	it("returns null for empty content", () => {
		expect(extractReadme("README.md", "")).toBeNull();
	});

	it("returns null for whitespace-only content", () => {
		expect(extractReadme("README.md", "   \n\n  \t ")).toBeNull();
	});

	it("sourceLineStart is always 1", () => {
		const src = "line1\nline2\nline3";
		const result = extractReadme("README.md", src);
		expect(result!.sourceLineStart).toBe(1);
	});

	it("sourceLineEnd matches the file line count", () => {
		const src = "line1\nline2\nline3";
		const result = extractReadme("README.md", src);
		expect(result!.sourceLineEnd).toBe(3);
	});

	it("handles single-line README", () => {
		const result = extractReadme("README.md", "Just one line.");
		expect(result!.sourceLineStart).toBe(1);
		expect(result!.sourceLineEnd).toBe(1);
	});

	it("handles trailing newline correctly", () => {
		// "a\nb\n" has 3 lines when split by '\n': ['a', 'b', ''].
		// The empty trailing line is counted — this is a documented
		// consequence of split('\n') semantics.
		const result = extractReadme("README.md", "a\nb\n");
		expect(result!.sourceLineEnd).toBe(3);
	});
});
