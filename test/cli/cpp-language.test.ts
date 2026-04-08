/**
 * C/C++ language CLI integration test.
 *
 * Exercises the composition-root path: bootstrap() in main.ts
 * initializes CppExtractor, the indexer routes .c/.h files to it,
 * and the graph queries return C symbols.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/cpp/simple",
);
const REPO_NAME = "cpp-cli-test";

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

describe("C/C++ language — CLI composition root", () => {
	it("indexes .c/.h/.cpp files through bootstrap()", async () => {
		const r = await h.run("repo", "status", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const snapshot = json.snapshot as Record<string, unknown>;
		expect(snapshot).not.toBeNull();
		// main.c, include/util.h, engine.cpp, handler.c
		expect(snapshot.files_total).toBe(4);
		expect((snapshot.nodes_total as number)).toBeGreaterThan(0);
	});

	it("C symbols are queryable via graph commands", async () => {
		const r = await h.run("graph", "dead", REPO_NAME, "--kind", "SYMBOL", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const symbols = (json.results as Array<{ symbol: string }>).map((s) => s.symbol);
		// C side: helper, main, internal_func, Point should be in the graph.
		expect(symbols.some((s) => s === "main" || s === "helper" || s === "Point")).toBe(true);
	});

	it("C++ symbols from .cpp files are queryable through bootstrap()", async () => {
		const r = await h.run("graph", "dead", REPO_NAME, "--kind", "SYMBOL", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const symbols = (json.results as Array<{ symbol: string }>).map((s) => s.symbol);
		// C++ side: Engine class from engine.cpp must be in the graph.
		// This proves .cpp routing through bootstrap() → CppExtractor.
		expect(symbols.some((s) => s === "Engine" || s === "mylib::Engine::run")).toBe(true);
	});

	it("snapshot toolchain includes cpp-core provenance", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.toolchain?.extractor_versions?.c).toBe("cpp-core:0.1.0");
	});
});
