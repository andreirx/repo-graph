/**
 * Unit tests for directory-structure module detector.
 *
 * Pure function tests — no filesystem, no storage. Tests cover:
 *   - Anchored root recognition (src/, lib/, pkg/, internal/, drivers/, arch/)
 *   - Immediate child detection (not nested)
 *   - Excluded path segments (test/, vendor/, etc.)
 *   - File count threshold (>= 5)
 *   - Language coherence threshold (>= 80%)
 *   - Statistics tracking
 */

import { describe, expect, it } from "vitest";
import {
	detectDirectoryModules,
	type FileMetadata,
} from "../../../../src/core/modules/detectors/directory-detector.js";

function makeFile(
	path: string,
	language: string | null = "typescript",
	isTest = false,
	isExcluded = false,
): FileMetadata {
	return { path, language, isTest, isExcluded };
}

/**
 * Generate N files under a directory path.
 */
function makeFiles(
	dir: string,
	count: number,
	language: string | null = "typescript",
): FileMetadata[] {
	return Array.from({ length: count }, (_, i) =>
		makeFile(`${dir}/file${i}.ts`, language),
	);
}

// ── Anchored root detection ────────────────────────────────────────

describe("detectDirectoryModules — anchored roots", () => {
	it("detects immediate child of src/", () => {
		const files = makeFiles("src/core", 5);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("src/core");
		expect(result.modules[0].moduleKind).toBe("inferred");
		expect(result.modules[0].sourceType).toBe("directory_structure");
		expect(result.modules[0].evidenceKind).toBe("directory_pattern");
		expect(result.modules[0].confidence).toBe(0.7);
	});

	it("detects immediate child of lib/", () => {
		const files = makeFiles("lib/utils", 5, "python");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("lib/utils");
	});

	it("detects immediate child of pkg/", () => {
		const files = makeFiles("pkg/api", 5, "go");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("pkg/api");
	});

	it("detects immediate child of internal/", () => {
		const files = makeFiles("internal/config", 5, "go");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("internal/config");
	});

	it("detects immediate child of drivers/", () => {
		const files = makeFiles("drivers/net", 5, "c");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("drivers/net");
	});

	it("detects immediate child of arch/", () => {
		const files = makeFiles("arch/x86", 5, "c");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("arch/x86");
	});

	it("ignores non-anchored root directories", () => {
		const files = makeFiles("other/stuff", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("ignores files directly in anchored root (not a child dir)", () => {
		const files = [
			makeFile("src/index.ts"),
			makeFile("src/main.ts"),
			makeFile("src/app.ts"),
			makeFile("src/config.ts"),
			makeFile("src/utils.ts"),
		];
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});
});

// ── Immediate children only (not nested) ───────────────────────────

describe("detectDirectoryModules — immediate children only", () => {
	it("groups nested files under immediate child", () => {
		const files = [
			makeFile("src/core/model/entity.ts"),
			makeFile("src/core/model/value.ts"),
			makeFile("src/core/ports/storage.ts"),
			makeFile("src/core/ports/indexer.ts"),
			makeFile("src/core/index.ts"),
		];
		const result = detectDirectoryModules(files);

		// All files roll up to src/core, not src/core/model or src/core/ports
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("src/core");
	});

	it("detects multiple immediate children separately", () => {
		const files = [
			...makeFiles("src/core", 5),
			...makeFiles("src/adapters", 5),
			...makeFiles("src/cli", 5),
		];
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(3);
		const paths = result.modules.map((m) => m.rootPath).sort();
		expect(paths).toEqual(["src/adapters", "src/cli", "src/core"]);
	});
});

// ── Excluded path segments ─────────────────────────────────────────

describe("detectDirectoryModules — excluded segments", () => {
	it("excludes directories containing test segment", () => {
		const files = makeFiles("src/test", 10);
		const result = detectDirectoryModules(files);

		// Files under excluded segments are filtered before grouping,
		// so no candidate is ever created for src/test.
		expect(result.modules).toHaveLength(0);
		expect(result.stats.candidatesEvaluated).toBe(0);
	});

	it("excludes directories containing tests segment", () => {
		const files = makeFiles("src/tests", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("excludes directories containing __tests__ segment", () => {
		const files = makeFiles("src/__tests__", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("excludes directories containing vendor segment", () => {
		const files = makeFiles("src/vendor", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("excludes directories containing third_party segment", () => {
		const files = makeFiles("lib/third_party", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("excludes directories containing node_modules segment", () => {
		const files = makeFiles("src/node_modules", 10);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
	});

	it("skips files marked as isTest", () => {
		const files = [
			...makeFiles("src/core", 3), // 3 regular files
			makeFile("src/core/foo.test.ts", "typescript", true), // test file
			makeFile("src/core/bar.spec.ts", "typescript", true), // test file
		];
		const result = detectDirectoryModules(files);

		// Only 3 files count (tests are skipped), below threshold
		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedInsufficientFiles).toBe(1);
	});

	it("skips files marked as isExcluded", () => {
		const files = [
			...makeFiles("src/core", 3),
			makeFile("src/core/generated.ts", "typescript", false, true),
			makeFile("src/core/large.ts", "typescript", false, true),
		];
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedInsufficientFiles).toBe(1);
	});

	it("excludes files in nested vendor directory from file count", () => {
		const files = [
			...makeFiles("src/core", 3), // 3 regular files
			makeFile("src/core/vendor/lib.c", "c"), // nested vendor
			makeFile("src/core/vendor/util.c", "c"), // nested vendor
		];
		const result = detectDirectoryModules(files);

		// Only 3 files count (vendor files excluded), below threshold
		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedInsufficientFiles).toBe(1);
	});

	it("excludes files in nested third_party directory from file count", () => {
		const files = [
			...makeFiles("src/core", 4),
			makeFile("src/core/third_party/dep.ts", "typescript"),
		];
		const result = detectDirectoryModules(files);

		// Only 4 files count, below threshold
		expect(result.modules).toHaveLength(0);
	});

	it("excludes files in nested tests directory from file count", () => {
		const files = [
			...makeFiles("drivers/net", 4),
			makeFile("drivers/net/tests/helper.c", "c"),
			makeFile("drivers/net/tests/mock.c", "c"),
		];
		const result = detectDirectoryModules(files);

		// Only 4 files count, below threshold
		expect(result.modules).toHaveLength(0);
	});

	it("correctly counts files when nested excluded dir is present but threshold still met", () => {
		const files = [
			...makeFiles("src/core", 6), // 6 regular files, above threshold
			makeFile("src/core/vendor/lib.c", "c"), // excluded, doesn't count
		];
		const result = detectDirectoryModules(files);

		// 6 files pass threshold despite vendor file being present
		expect(result.modules).toHaveLength(1);
		const payload = result.modules[0].payload as { fileCount: number };
		expect(payload.fileCount).toBe(6); // vendor file not counted
	});
});

// ── File count threshold ───────────────────────────────────────────

describe("detectDirectoryModules — file count threshold", () => {
	it("rejects directories with fewer than 5 files", () => {
		const files = makeFiles("src/small", 4);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedInsufficientFiles).toBe(1);
	});

	it("accepts directories with exactly 5 files", () => {
		const files = makeFiles("src/exact", 5);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
	});

	it("accepts directories with more than 5 files", () => {
		const files = makeFiles("src/large", 20);
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
	});
});

// ── Language coherence threshold ───────────────────────────────────

describe("detectDirectoryModules — language coherence", () => {
	it("accepts 100% coherent (single language)", () => {
		const files = makeFiles("src/pure", 10, "typescript");
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		const payload = result.modules[0].payload as { languageCoherence: number };
		expect(payload.languageCoherence).toBe(1);
	});

	it("accepts 80% coherent (at threshold)", () => {
		const files = [
			...makeFiles("src/mixed", 8, "typescript"), // 80%
			makeFile("src/mixed/other1.py", "python"),
			makeFile("src/mixed/other2.c", "c"),
		];
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(1);
		const payload = result.modules[0].payload as { languageCoherence: number };
		expect(payload.languageCoherence).toBe(0.8);
	});

	it("rejects below 80% coherent", () => {
		const files = [
			...makeFiles("src/fragmented", 4, "typescript"), // 40%
			...makeFiles("src/fragmented", 3, "python").map((f, i) => ({
				...f,
				path: `src/fragmented/py${i}.py`,
			})), // 30%
			...makeFiles("src/fragmented", 3, "c").map((f, i) => ({
				...f,
				path: `src/fragmented/c${i}.c`,
			})), // 30%
		];
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedLowCoherence).toBe(1);
	});

	it("treats null language files as counting toward total but not coherence", () => {
		const files = [
			...makeFiles("src/withunknown", 4, "typescript"), // 4 known
			makeFile("src/withunknown/readme.md", null), // unknown
			makeFile("src/withunknown/config.json", null), // unknown
		];
		// Coherence = 4/6 = 0.67, below 0.8
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.rejectedLowCoherence).toBe(1);
	});

	it("includes dominant language in payload", () => {
		const files = makeFiles("src/typed", 10, "rust");
		const result = detectDirectoryModules(files);

		const payload = result.modules[0].payload as { dominantLanguage: string };
		expect(payload.dominantLanguage).toBe("rust");
	});
});

// ── Statistics ─────────────────────────────────────────────────────

describe("detectDirectoryModules — statistics", () => {
	it("tracks filesProcessed correctly", () => {
		const files = makeFiles("src/core", 7);
		const result = detectDirectoryModules(files);

		expect(result.stats.filesProcessed).toBe(7);
	});

	it("tracks candidatesEvaluated correctly", () => {
		const files = [
			...makeFiles("src/a", 3), // will be evaluated
			...makeFiles("src/b", 10), // will be evaluated
			...makeFiles("other/c", 10), // not anchored, not evaluated
		];
		const result = detectDirectoryModules(files);

		expect(result.stats.candidatesEvaluated).toBe(2);
	});

	it("tracks rejection reasons correctly", () => {
		const files = [
			...makeFiles("src/toosmall", 3), // insufficient files
			// Note: src/test files are filtered before grouping (excluded segment in path)
			// so no candidate is created for it. Use a non-excluded root to test
			// the rejectedExcludedSegment path if needed.
			...[
				...makeFiles("src/mixed", 3, "ts"),
				...makeFiles("src/mixed", 3, "py").map((f, i) => ({
					...f,
					path: `src/mixed/py${i}.py`,
				})),
			], // low coherence (50%)
			...makeFiles("src/good", 10), // passes
		];
		const result = detectDirectoryModules(files);

		expect(result.stats.rejectedInsufficientFiles).toBe(1);
		expect(result.stats.rejectedLowCoherence).toBe(1);
		expect(result.stats.candidatesPassed).toBe(1);
		// 3 candidates: toosmall, mixed, good
		expect(result.stats.candidatesEvaluated).toBe(3);
	});
});

// ── Display name ───────────────────────────────────────────────────

describe("detectDirectoryModules — display name", () => {
	it("uses directory name as display name", () => {
		const files = makeFiles("src/mymodule", 5);
		const result = detectDirectoryModules(files);

		expect(result.modules[0].displayName).toBe("mymodule");
	});

	it("handles single-segment paths", () => {
		// This shouldn't happen in practice (single segment = anchored root itself)
		// but the code handles it gracefully
		const files = makeFiles("src/x", 5);
		const result = detectDirectoryModules(files);

		expect(result.modules[0].displayName).toBe("x");
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("detectDirectoryModules — edge cases", () => {
	it("handles empty input", () => {
		const result = detectDirectoryModules([]);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.filesProcessed).toBe(0);
		expect(result.stats.candidatesEvaluated).toBe(0);
	});

	it("handles all files excluded", () => {
		const files = makeFiles("src/core", 10).map((f) => ({
			...f,
			isExcluded: true,
		}));
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.candidatesEvaluated).toBe(0);
	});

	it("handles all files as tests", () => {
		const files = makeFiles("src/core", 10).map((f) => ({
			...f,
			isTest: true,
		}));
		const result = detectDirectoryModules(files);

		expect(result.modules).toHaveLength(0);
		expect(result.stats.candidatesEvaluated).toBe(0);
	});
});

// ── Payload structure ──────────────────────────────────────────────

describe("detectDirectoryModules — payload", () => {
	it("includes expected payload fields", () => {
		const files = makeFiles("drivers/gpu", 10, "c");
		const result = detectDirectoryModules(files);

		const payload = result.modules[0].payload as {
			fileCount: number;
			languageCoherence: number;
			dominantLanguage: string;
			anchoredRoot: string;
		};

		expect(payload.fileCount).toBe(10);
		expect(payload.languageCoherence).toBe(1);
		expect(payload.dominantLanguage).toBe("c");
		expect(payload.anchoredRoot).toBe("drivers");
	});
});
