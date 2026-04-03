/**
 * CLI integration test harness.
 *
 * Runs the compiled CLI (dist/) against a temporary database.
 * The `pnpm test` script runs `tsc` before vitest, so dist/ is
 * always fresh when tests execute.
 *
 * Usage:
 *   const h = await createTestHarness();
 *   await h.run("repo", "add", fixturesPath);
 *   await h.run("repo", "index", "simple-imports");
 *   const result = await h.run("graph", "stats", "simple-imports", "--json");
 *   expect(result.exitCode).toBe(0);
 *   const json = result.json();
 *   expect(json.count).toBeGreaterThan(0);
 *   h.cleanup();
 */

import { execFile } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

/**
 * Run the compiled CLI. The test script ensures a fresh build
 * before running tests (pnpm run build && vitest).
 */
const CLI_PATH = join(import.meta.dirname, "../../dist/cli/index.js");

export interface CommandResult {
	stdout: string;
	stderr: string;
	exitCode: number;
	/** Parse stdout as JSON. Throws if not valid JSON. */
	json: () => Record<string, unknown>;
}

export interface TestHarness {
	/** Run a CLI command. Arguments are passed after the CLI binary. */
	run: (...args: string[]) => Promise<CommandResult>;
	/** Path to the temporary database file. */
	dbPath: string;
	/** Clean up the temporary database. */
	cleanup: () => void;
}

/**
 * Create a test harness with a fresh temporary database.
 * The harness sets RGR_DB_PATH so the CLI uses the temp DB.
 */
export async function createTestHarness(): Promise<TestHarness> {
	const dir = join(tmpdir(), `rgr-cli-test-${randomUUID()}`);
	mkdirSync(dir, { recursive: true });
	const dbPath = join(dir, "test.db");

	async function run(...args: string[]): Promise<CommandResult> {
		try {
			const { stdout, stderr } = await execFileAsync(
				process.execPath,
				[CLI_PATH, ...args],
				{
					env: { ...process.env, RGR_DB_PATH: dbPath },
					timeout: 30000,
				},
			);
			return {
				stdout,
				stderr,
				exitCode: 0,
				json: () => JSON.parse(stdout),
			};
		} catch (err: unknown) {
			const e = err as {
				stdout?: string;
				stderr?: string;
				code?: number;
			};
			const stdout = e.stdout ?? "";
			const stderr = e.stderr ?? "";
			return {
				stdout,
				stderr,
				exitCode: e.code ?? 1,
				json: () => JSON.parse(stdout),
			};
		}
	}

	return {
		run,
		dbPath,
		cleanup: () => {
			try {
				unlinkSync(dbPath);
			} catch {
				// ignore
			}
		},
	};
}
