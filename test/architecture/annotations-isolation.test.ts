/**
 * Annotations isolation invariant — architecture test.
 *
 * Enforces docs/architecture/annotations-contract.txt §7:
 * provisional annotations MUST NOT be read by computed-truth
 * surfaces. This test scans source files in the prohibited paths
 * and fails if ANY of them imports annotations modules.
 *
 * Prohibited reader paths:
 *   src/core/evaluator/*
 *   src/core/gate/*
 *   src/core/impact/*
 *   src/core/trust/*
 *   src/cli/commands/gate.ts
 *   src/cli/commands/evidence.ts
 *   src/cli/commands/graph/obligations.ts
 *   src/cli/commands/graph/queries.ts (dead/callers/callees/path)
 *   src/cli/commands/change.ts
 *
 * Indexer rule: src/adapters/indexer/* MAY WRITE annotations via
 * AnnotationsPort but MUST NOT READ them. This test verifies the
 * indexer does not invoke READ methods on the port (static check
 * against method names).
 *
 * This is a grep-based invariant test. It is coarse but sufficient:
 * any accidental import of a prohibited module produces an immediate
 * build failure. A TypeScript structural check would be stricter but
 * substantially more complex to implement without affecting runtime.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const REPO_ROOT = join(import.meta.dirname, "../..");
const SRC_ROOT = join(REPO_ROOT, "src");

/**
 * Paths under isolation. A prohibited import from any of these
 * files causes the test to fail.
 */
const PROHIBITED_READER_PATHS: readonly string[] = [
	"src/core/evaluator",
	"src/core/gate",
	"src/core/impact",
	"src/core/trust",
	"src/cli/commands/gate.ts",
	"src/cli/commands/evidence.ts",
	"src/cli/commands/graph/obligations.ts",
	"src/cli/commands/graph/queries.ts",
	"src/cli/commands/change.ts",
];

/**
 * Substrings that identify an annotations-related import. ANY of
 * these appearing in an import path flags the file as violating
 * the isolation invariant.
 */
const ANNOTATIONS_IMPORT_SIGNATURES: readonly string[] = [
	"core/annotations/",
	"core/ports/annotations",
	"adapters/annotations/",
];

/**
 * READ methods on AnnotationsPort that the indexer must not call.
 * WRITE methods (insertAnnotations, deleteAnnotationsBySnapshot)
 * are allowed. countAnnotationsBySnapshot is a read but does not
 * return annotation content — flagged anyway because the indexer
 * should not depend on it.
 */
const PROHIBITED_INDEXER_ANNOTATION_METHODS: readonly string[] = [
	"getAnnotationsByTarget",
	"resolveDocsTarget",
	"countAnnotationsBySnapshot",
];

function listTsFiles(path: string): string[] {
	const stat = statSync(path);
	if (stat.isFile()) {
		return path.endsWith(".ts") ? [path] : [];
	}
	const entries = readdirSync(path, { withFileTypes: true });
	const out: string[] = [];
	for (const e of entries) {
		const full = join(path, e.name);
		if (e.isDirectory()) {
			out.push(...listTsFiles(full));
		} else if (e.isFile() && e.name.endsWith(".ts")) {
			out.push(full);
		}
	}
	return out;
}

function findImportViolations(
	filePath: string,
): { file: string; line: number; importPath: string }[] {
	const content = readFileSync(filePath, "utf-8");
	const lines = content.split("\n");
	const violations: { file: string; line: number; importPath: string }[] = [];
	// Match any import/export ... from "..." statement
	// Single-line matching is sufficient because tree-shake imports
	// keep the from-clause on the same line as the quote.
	const importRegex = /(?:import|export)\s+.*?\bfrom\s+["']([^"']+)["']/g;
	for (let i = 0; i < lines.length; i++) {
		importRegex.lastIndex = 0;
		const matches = lines[i].matchAll(importRegex);
		for (const m of matches) {
			const importPath = m[1];
			for (const sig of ANNOTATIONS_IMPORT_SIGNATURES) {
				if (importPath.includes(sig)) {
					violations.push({
						file: relative(REPO_ROOT, filePath),
						line: i + 1,
						importPath,
					});
					break;
				}
			}
		}
	}
	return violations;
}

describe("annotations isolation invariant (contract §7)", () => {
	it("no prohibited path imports annotations modules", () => {
		const allViolations: Array<{
			file: string;
			line: number;
			importPath: string;
		}> = [];

		for (const prohibited of PROHIBITED_READER_PATHS) {
			const absPath = join(REPO_ROOT, prohibited);
			let tsFiles: string[];
			try {
				tsFiles = listTsFiles(absPath);
			} catch {
				// Path may not exist yet (graph/queries.ts might be future)
				continue;
			}
			for (const file of tsFiles) {
				allViolations.push(...findImportViolations(file));
			}
		}

		if (allViolations.length > 0) {
			const message = [
				"Annotations isolation invariant violated.",
				"The following files import annotations modules from paths",
				"that MUST NOT read provisional annotations:",
				"",
				...allViolations.map(
					(v) => `  ${v.file}:${v.line}  from "${v.importPath}"`,
				),
				"",
				"See docs/architecture/annotations-contract.txt §7.",
			].join("\n");
			expect.fail(message);
		}
	});

	it("indexer does not invoke annotation read methods", () => {
		const indexerFiles = listTsFiles(join(SRC_ROOT, "adapters/indexer"));
		const violations: Array<{ file: string; line: number; method: string }> =
			[];
		for (const file of indexerFiles) {
			const content = readFileSync(file, "utf-8");
			const lines = content.split("\n");
			for (let i = 0; i < lines.length; i++) {
				for (const method of PROHIBITED_INDEXER_ANNOTATION_METHODS) {
					// Match `.annotations.<method>` OR `annotationsPort.<method>`
					// invocations. False-positive-safe because these method names
					// are specific to AnnotationsPort.
					const pattern = new RegExp(`\\.${method}\\s*\\(`);
					if (pattern.test(lines[i])) {
						violations.push({
							file: relative(REPO_ROOT, file),
							line: i + 1,
							method,
						});
					}
				}
			}
		}
		if (violations.length > 0) {
			const message = [
				"Indexer is invoking annotation READ methods. Indexer is",
				"write-only on AnnotationsPort per contract §7.",
				"",
				...violations.map(
					(v) => `  ${v.file}:${v.line}  calls .${v.method}(...)`,
				),
			].join("\n");
			expect.fail(message);
		}
	});
});
