/**
 * Git-based CLI test harness.
 *
 * Creates a temporary git repository with predictable TypeScript files,
 * two commits (so churn is non-empty and differentiated), and a helper
 * to write synthetic Istanbul coverage reports.
 *
 * Designed to exercise the full assessment chain:
 *   graph churn -> graph hotspots -> graph coverage -> graph risk -> obligations
 *
 * Lifecycle: per-test-suite. Created by createGitTestRepo(), cleaned up
 * by the returned cleanup() function.
 *
 * File design:
 *   src/complex.ts  — branching functions, modified in both commits (high churn + high CC)
 *   src/simple.ts   — linear functions, small modification in commit 2 (moderate churn, low CC)
 *   src/stable.ts   — const + interface only, untouched after commit 1 (low churn, no CC)
 *   src/index.ts    — imports from all three (creates IMPORTS edges)
 *   package.json    — enables manifest version extraction
 */

import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// ── Public types ─────────────────────────────────────────────────────

export interface GitTestRepo {
	/** Absolute path to the temp repo root. */
	repoDir: string;
	/**
	 * Write a synthetic Istanbul coverage report targeting repo files.
	 * Returns the absolute path to the generated report file.
	 *
	 * File paths in entries are relative to repo root.
	 * The helper converts them to absolute paths (as Istanbul does).
	 */
	writeCoverageReport(
		entries: Array<{
			file: string;
			statements: { covered: number; total: number };
			functions: { covered: number; total: number };
			branches: { covered: number; total: number };
		}>,
	): string;
	/** Remove the temp directory and all its contents. */
	cleanup(): void;
}

// ── Source file contents ─────────────────────────────────────────────

const PACKAGE_JSON = JSON.stringify(
	{ name: "git-harness-fixture", version: "1.0.0" },
	null,
	2,
);

// Commit 1: initial version of complex.ts
// processData: for(+1) + if(+1) + if(+1) + else-if(+1) = CC 5
// validateInput: if(+1) + ||(+1) + if(+1) = CC 4
const COMPLEX_V1 = `\
export function processData(items: string[]): string[] {
  const result: string[] = [];
  for (const item of items) {
    if (item.length > 5) {
      if (item.startsWith("test")) {
        result.push(item.toUpperCase());
      } else if (item.includes("-")) {
        result.push(item.replace("-", "_"));
      } else {
        result.push(item);
      }
    } else {
      result.push(item.trim());
    }
  }
  return result;
}

export function validateInput(input: unknown): boolean {
  if (input === null || input === undefined) return false;
  if (typeof input !== "object") return false;
  return true;
}
`;

// Commit 2: append a third function to complex.ts
// transformRecord: for(+1) + if(+1) + else-if(+1) + else-if(+1) = CC 5
// Total file sum_cc after commit 2: 5 + 4 + 5 = 14
const COMPLEX_V2_APPEND = `
export function transformRecord(record: Record<string, unknown>): Record<string, string> {
  const output: Record<string, string> = {};
  for (const [key, value] of Object.entries(record)) {
    if (typeof value === "string") {
      output[key] = value;
    } else if (typeof value === "number") {
      output[key] = String(value);
    } else if (value === null) {
      output[key] = "";
    } else {
      output[key] = JSON.stringify(value);
    }
  }
  return output;
}
`;

// simple.ts — linear functions, CC = 1 each
const SIMPLE_V1 = `\
export function greet(name: string): string {
  return \`Hello, \${name}\`;
}

export function add(a: number, b: number): number {
  return a + b;
}
`;

// Commit 2: append one more linear function
// Total file sum_cc after commit 2: 1 + 1 + 1 = 3
const SIMPLE_V2_APPEND = `
export function subtract(a: number, b: number): number {
  return a - b;
}
`;

// stable.ts — no functions, just const + interface. Untouched after commit 1.
// sum_cc = 0 → excluded from hotspot join
const STABLE_V1 = `\
export const VERSION = "1.0.0";

export interface Config {
  debug: boolean;
  verbose: boolean;
}
`;

// index.ts — imports creating IMPORTS edges
const INDEX_V1 = `\
import { processData } from "./complex";
import { greet } from "./simple";
import { VERSION } from "./stable";

export { processData, greet, VERSION };
`;

// ── Git helper ───────────────────────────────────────────────────────

function git(repoDir: string, ...args: string[]): void {
	execFileSync("git", args, {
		cwd: repoDir,
		stdio: "pipe",
		env: {
			...process.env,
			GIT_AUTHOR_NAME: "Test",
			GIT_AUTHOR_EMAIL: "test@test.com",
			GIT_COMMITTER_NAME: "Test",
			GIT_COMMITTER_EMAIL: "test@test.com",
		},
	});
}

// ── Factory ──────────────────────────────────────────────────────────

export async function createGitTestRepo(): Promise<GitTestRepo> {
	const repoDir = join(tmpdir(), `rgr-git-test-${randomUUID()}`);
	mkdirSync(join(repoDir, "src"), { recursive: true });

	// ── Commit 1: initial files ──────────────────────────────────────
	writeFileSync(join(repoDir, "package.json"), PACKAGE_JSON);
	writeFileSync(join(repoDir, "src", "complex.ts"), COMPLEX_V1);
	writeFileSync(join(repoDir, "src", "simple.ts"), SIMPLE_V1);
	writeFileSync(join(repoDir, "src", "stable.ts"), STABLE_V1);
	writeFileSync(join(repoDir, "src", "index.ts"), INDEX_V1);

	git(repoDir, "init");
	git(repoDir, "add", ".");
	git(repoDir, "commit", "-m", "initial commit");

	// ── Commit 2: modify complex.ts and simple.ts ───────────────────
	// Append new functions (increases both churn_lines and complexity)
	writeFileSync(
		join(repoDir, "src", "complex.ts"),
		COMPLEX_V1 + COMPLEX_V2_APPEND,
	);
	writeFileSync(
		join(repoDir, "src", "simple.ts"),
		SIMPLE_V1 + SIMPLE_V2_APPEND,
	);

	git(repoDir, "add", ".");
	git(repoDir, "commit", "-m", "add features");

	// ── Coverage report helper ───────────────────────────────────────
	const reportDir = join(repoDir, ".coverage");
	mkdirSync(reportDir, { recursive: true });

	function writeCoverageReport(
		entries: Array<{
			file: string;
			statements: { covered: number; total: number };
			functions: { covered: number; total: number };
			branches: { covered: number; total: number };
		}>,
	): string {
		const report: Record<string, unknown> = {};
		for (const entry of entries) {
			const absPath = join(repoDir, entry.file);
			// Build Istanbul-format hit-count maps
			const s: Record<string, number> = {};
			for (let i = 0; i < entry.statements.total; i++) {
				s[String(i)] = i < entry.statements.covered ? 1 : 0;
			}
			const f: Record<string, number> = {};
			for (let i = 0; i < entry.functions.total; i++) {
				f[String(i)] = i < entry.functions.covered ? 1 : 0;
			}
			// Istanbul b map: each key is a branch point, value is an array
			// of hit counts — one element per path through that branch.
			// We emit single-element arrays so that branches.total maps
			// 1:1 to the importer's flattened branch count.
			const b: Record<string, number[]> = {};
			for (let i = 0; i < entry.branches.total; i++) {
				b[String(i)] = [i < entry.branches.covered ? 1 : 0];
			}
			report[absPath] = {
				s,
				f,
				b,
				statementMap: {},
				fnMap: {},
				branchMap: {},
			};
		}

		const reportPath = join(reportDir, "coverage-final.json");
		writeFileSync(reportPath, JSON.stringify(report));
		return reportPath;
	}

	return {
		repoDir,
		writeCoverageReport,
		cleanup: () => {
			try {
				rmSync(repoDir, { recursive: true, force: true });
			} catch {
				// best effort
			}
		},
	};
}
