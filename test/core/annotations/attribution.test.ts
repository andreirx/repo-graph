/**
 * Annotation attribution rules — pure function tests.
 *
 * Attribution is deterministic. These tests enumerate the rules
 * from docs/architecture/annotations-contract.txt and verify each
 * produces the expected decision.
 */

import { describe, expect, it } from "vitest";
import {
	applyContentTruncation,
	attributeFileHeader,
	attributeJsdoc,
	attributePackageDescription,
	attributeReadme,
	isEmptyContent,
	isFileHeaderBoilerplate,
	preferReadmeFile,
	resolveCollisions,
	stripJsdocDecorations,
	stripLineCommentDecorations,
} from "../../../src/core/annotations/attribution.js";
import {
	AnnotationKind,
	AnnotationTargetKind,
	CONTENT_MAX_BYTES,
	CONTENT_TRUNCATION_MARKER,
} from "../../../src/core/annotations/types.js";

// ── JSDoc decoration stripping ──────────────────────────────────

describe("stripJsdocDecorations", () => {
	it("strips /** and */ and per-line ` * `", () => {
		const raw = "/**\n * First line.\n * Second line.\n */";
		expect(stripJsdocDecorations(raw)).toBe("First line.\nSecond line.");
	});

	it("preserves markdown formatting", () => {
		const raw = "/**\n * # Heading\n * - list item\n * **bold**\n */";
		expect(stripJsdocDecorations(raw)).toBe("# Heading\n- list item\n**bold**");
	});

	it("handles single-line /** ... */", () => {
		const raw = "/** Short description */";
		// After stripping opener+closer: " Short description "
		// After per-line strip (no ` *` prefix matches): trimmed → "Short description"
		expect(stripJsdocDecorations(raw)).toBe("Short description");
	});

	it("tolerates /* (non-JSDoc) opener", () => {
		const raw = "/* a comment\n * body\n */";
		expect(stripJsdocDecorations(raw)).toBe("a comment\nbody");
	});

	it("returns empty string for empty block", () => {
		expect(stripJsdocDecorations("/** */")).toBe("");
	});
});

describe("stripLineCommentDecorations", () => {
	it("strips `//` and one space per line", () => {
		const raw = "// First line\n// Second line";
		expect(stripLineCommentDecorations(raw)).toBe("First line\nSecond line");
	});

	it("preserves content when no leading space after //", () => {
		const raw = "//no-space\n//also-no-space";
		expect(stripLineCommentDecorations(raw)).toBe("no-space\nalso-no-space");
	});
});

// ── Content truncation ──────────────────────────────────────────

describe("applyContentTruncation", () => {
	it("returns content unchanged when under the cap", () => {
		const short = "small content";
		expect(applyContentTruncation(short)).toBe(short);
	});

	it("appends truncation marker when over the cap", () => {
		const large = "a".repeat(CONTENT_MAX_BYTES + 500);
		const out = applyContentTruncation(large);
		expect(out.endsWith(CONTENT_TRUNCATION_MARKER)).toBe(true);
		expect(Buffer.byteLength(out, "utf-8")).toBeLessThanOrEqual(
			CONTENT_MAX_BYTES,
		);
	});

	it("handles UTF-8 multi-byte characters correctly", () => {
		// 4-byte emoji × many → triggers truncation with multi-byte boundary
		const emoji = "\u{1F600}"; // 4 bytes in UTF-8
		const count = Math.floor(CONTENT_MAX_BYTES / 4) + 10;
		const input = emoji.repeat(count);
		const out = applyContentTruncation(input);
		expect(Buffer.byteLength(out, "utf-8")).toBeLessThanOrEqual(
			CONTENT_MAX_BYTES,
		);
		expect(out.endsWith(CONTENT_TRUNCATION_MARKER)).toBe(true);
	});
});

// ── Drop rules ───────────────────────────────────────────────────

describe("isEmptyContent", () => {
	it("true for empty string", () => {
		expect(isEmptyContent("")).toBe(true);
	});

	it("true for whitespace-only", () => {
		expect(isEmptyContent("   \n\t  \n")).toBe(true);
	});

	it("false for short non-empty content", () => {
		expect(isEmptyContent("x")).toBe(false);
	});

	it("false for content with surrounding whitespace", () => {
		expect(isEmptyContent("  hello  ")).toBe(false);
	});
});

describe("isFileHeaderBoilerplate", () => {
	it("matches Copyright (case-insensitive)", () => {
		expect(isFileHeaderBoilerplate("Copyright 2024 Acme Inc.")).toBe(true);
		expect(isFileHeaderBoilerplate("COPYRIGHT")).toBe(true);
		expect(isFileHeaderBoilerplate("copyright")).toBe(true);
	});

	it("matches License", () => {
		expect(isFileHeaderBoilerplate("This file is under MIT License")).toBe(
			true,
		);
	});

	it("matches SPDX identifier", () => {
		expect(isFileHeaderBoilerplate("SPDX-License-Identifier: MIT")).toBe(true);
		expect(isFileHeaderBoilerplate("spdx: Apache-2.0")).toBe(true);
	});

	it("false for non-boilerplate intent comments", () => {
		expect(
			isFileHeaderBoilerplate(
				"This module handles payment gateway integration.",
			),
		).toBe(false);
		expect(
			isFileHeaderBoilerplate("Central registry for block renderers."),
		).toBe(false);
	});

	it("respects the 500-char window (match after window is ignored)", () => {
		const padding = "non-boilerplate intent statement ".repeat(20); // ~660 chars
		expect(
			isFileHeaderBoilerplate(padding + "copyright notice at the end"),
		).toBe(false);
	});
});

// ── Attribution ─────────────────────────────────────────────────

describe("attributeJsdoc", () => {
	it("attaches to SYMBOL when declaration is exported", () => {
		const result = attributeJsdoc({
			declarationExported: true,
			symbolStableKey: "repo:src/a.ts#Foo:SYMBOL:FUNCTION",
		});
		expect(result).toEqual({
			target_kind: AnnotationTargetKind.SYMBOL,
			target_stable_key: "repo:src/a.ts#Foo:SYMBOL:FUNCTION",
		});
	});

	it("drops (returns null) when declaration is not exported", () => {
		expect(
			attributeJsdoc({
				declarationExported: false,
				symbolStableKey: "repo:src/a.ts#Foo:SYMBOL:FUNCTION",
			}),
		).toBeNull();
	});
});

describe("attributePackageDescription", () => {
	it("attaches to REPO for repo-root package.json", () => {
		const result = attributePackageDescription({
			isRepoRoot: true,
			repoStableKey: "repo:.:REPO",
			owningModuleStableKey: null,
		});
		expect(result).toEqual({
			target_kind: AnnotationTargetKind.REPO,
			target_stable_key: "repo:.:REPO",
		});
	});

	it("attaches to MODULE for sub-directory package.json with a module", () => {
		const result = attributePackageDescription({
			isRepoRoot: false,
			repoStableKey: "repo:.:REPO",
			owningModuleStableKey: "repo:packages/engine:MODULE",
		});
		expect(result).toEqual({
			target_kind: AnnotationTargetKind.MODULE,
			target_stable_key: "repo:packages/engine:MODULE",
		});
	});

	it("drops when sub-directory has no MODULE node", () => {
		expect(
			attributePackageDescription({
				isRepoRoot: false,
				repoStableKey: "repo:.:REPO",
				owningModuleStableKey: null,
			}),
		).toBeNull();
	});
});

describe("attributeFileHeader", () => {
	it("attaches to FILE", () => {
		expect(
			attributeFileHeader({
				fileStableKey: "repo:src/a.ts:FILE",
			}),
		).toEqual({
			target_kind: AnnotationTargetKind.FILE,
			target_stable_key: "repo:src/a.ts:FILE",
		});
	});
});

describe("attributeReadme", () => {
	it("attaches to REPO for repo-root README", () => {
		const result = attributeReadme({
			isRepoRoot: true,
			repoStableKey: "repo:.:REPO",
			owningModuleStableKey: null,
		});
		expect(result).toEqual({
			target_kind: AnnotationTargetKind.REPO,
			target_stable_key: "repo:.:REPO",
		});
	});

	it("attaches to MODULE for sub-directory README with a module", () => {
		const result = attributeReadme({
			isRepoRoot: false,
			repoStableKey: "repo:.:REPO",
			owningModuleStableKey: "repo:src/core:MODULE",
		});
		expect(result).toEqual({
			target_kind: AnnotationTargetKind.MODULE,
			target_stable_key: "repo:src/core:MODULE",
		});
	});

	it("drops when sub-directory has no MODULE node", () => {
		expect(
			attributeReadme({
				isRepoRoot: false,
				repoStableKey: "repo:.:REPO",
				owningModuleStableKey: null,
			}),
		).toBeNull();
	});
});

describe("preferReadmeFile", () => {
	it("prefers README.md over README.txt", () => {
		expect(preferReadmeFile(["README.md", "README.txt"])).toBe("README.md");
		expect(preferReadmeFile(["readme.txt", "README.md"])).toBe("README.md");
	});

	it("uses README.txt when .md is absent", () => {
		expect(preferReadmeFile(["README.txt"])).toBe("README.txt");
	});

	it("returns null when no README is present", () => {
		expect(preferReadmeFile(["index.ts", "package.json"])).toBeNull();
	});

	it("case-insensitive match, returns original case", () => {
		expect(preferReadmeFile(["Readme.MD"])).toBe("Readme.MD");
	});
});

// ── Collision resolution ────────────────────────────────────────

describe("resolveCollisions", () => {
	const make = (
		target_stable_key: string,
		annotation_kind: AnnotationKind,
		source_file: string,
		source_line_start: number,
	) => ({
		target_kind: AnnotationTargetKind.MODULE,
		target_stable_key,
		annotation_kind,
		source_file,
		source_line_start,
		source_line_end: source_line_start + 10,
	});

	it("keeps all when no collisions exist", () => {
		const candidates = [
			make("mod-a", AnnotationKind.MODULE_README, "a/README.md", 1),
			make("mod-b", AnnotationKind.MODULE_README, "b/README.md", 1),
			make(
				"mod-a",
				AnnotationKind.PACKAGE_DESCRIPTION,
				"a/package.json",
				1,
			),
		];
		const result = resolveCollisions(candidates);
		expect(result.keptIndices).toEqual([0, 1, 2]);
		expect(result.droppedCount).toBe(0);
	});

	it("picks lowest source_line_start on same-kind collision", () => {
		const candidates = [
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "a.ts", 10),
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "a.ts", 3),
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "a.ts", 7),
		];
		const result = resolveCollisions(candidates);
		expect(result.keptIndices).toEqual([1]); // line_start=3
		expect(result.droppedCount).toBe(2);
	});

	it("ties broken by source_file alphabetical, then source_line_end", () => {
		const candidates = [
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "z.ts", 5),
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "a.ts", 5),
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "m.ts", 5),
		];
		const result = resolveCollisions(candidates);
		// Winner: a.ts (alphabetical first)
		expect(result.keptIndices).toEqual([1]);
		expect(result.droppedCount).toBe(2);
	});

	it("keeps cross-kind annotations on same target", () => {
		const candidates = [
			make("mod-a", AnnotationKind.MODULE_README, "README.md", 1),
			make("mod-a", AnnotationKind.PACKAGE_DESCRIPTION, "package.json", 1),
			make("mod-a", AnnotationKind.FILE_HEADER_COMMENT, "index.ts", 1),
		];
		const result = resolveCollisions(candidates);
		expect(result.keptIndices).toHaveLength(3);
		expect(result.droppedCount).toBe(0);
	});
});
