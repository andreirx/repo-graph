/**
 * Python language CLI integration test.
 *
 * Exercises the composition-root path: bootstrap() in main.ts
 * initializes PythonExtractor, the indexer routes .py files to it,
 * and the graph queries return Python symbols.
 *
 * This test runs the compiled CLI binary, not manual extractor
 * construction. It pins that main.ts actually registers the
 * PythonExtractor.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/python/simple",
);
const REPO_NAME = "python-cli-test";

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", REPO_NAME);
	const indexResult = await h.run("repo", "index", REPO_NAME);
	if (indexResult.exitCode !== 0) {
		throw new Error(`Index failed: ${indexResult.stderr}`);
	}
}, 30000);

afterAll(() => {
	h.cleanup();
});

describe("Python language — CLI composition root", () => {
	it("indexes .py files through bootstrap()", async () => {
		const r = await h.run("repo", "status", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const snapshot = json.snapshot as Record<string, unknown>;
		expect(snapshot).not.toBeNull();
		expect(snapshot.files_total).toBe(3);
		expect((snapshot.nodes_total as number)).toBeGreaterThan(0);
	});

	it("Python symbols are queryable via graph commands", async () => {
		const r = await h.run("graph", "dead", REPO_NAME, "--kind", "SYMBOL", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		// UserService, process_items, helper, API_URL should be in the graph.
		const symbols = (json.results as Array<{ symbol: string }>).map((s) => s.symbol);
		expect(symbols.some((s) => s === "UserService")).toBe(true);
		expect(symbols.some((s) => s === "process_items")).toBe(true);
	});

	it("snapshot toolchain includes python-core provenance", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.toolchain?.extractor_versions?.python).toBe("python-core:0.1.0");
	});
});
