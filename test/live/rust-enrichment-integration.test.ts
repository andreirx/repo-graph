/**
 * Live Rust enrichment integration test.
 *
 * Exercises the end-to-end path through rust-analyzer:
 *   - subprocess startup + project warm-up
 *   - live hover requests on real Rust code
 *   - type extraction from hover responses
 *   - failure handling
 *
 * REQUIRES rust-analyzer on PATH and a compilable Cargo fixture.
 * Skips gracefully if rust-analyzer is not available.
 *
 * This test is slower (~30-60s) because it starts a real
 * rust-analyzer process and waits for project loading.
 */

import { execSync } from "node:child_process";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { resolveRustReceiverTypes } from "../../src/adapters/enrichment/rust-receiver-resolver.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../fixtures/rust/enrichment-crate",
);

let rustAnalyzerAvailable = false;
try {
	execSync("rust-analyzer --version", { stdio: "pipe" });
	rustAnalyzerAvailable = true;
} catch {
	// not on PATH
}

const itRA = rustAnalyzerAvailable ? it : it.skip;

describe("resolveRustReceiverTypes — live rust-analyzer", () => {
	itRA(
		"resolves known receiver types from the enrichment fixture",
		async () => {
			// Positions verified against actual extractor output.
			// The Rust extractor's colStart lands on the receiver
			// expression start (first char of the call expression span).
			// These match what `rgr enrich` would pass from real edges.
			const sites = [
				{
					edgeUid: "e-gamestate",
					sourceFilePath: "src/lib.rs",
					lineStart: 49,
					colStart: 17, // col of 'state' in "state.get_scores()"
					targetKey: "state.get_scores",
				},
				{
					edgeUid: "e-vec-sort",
					sourceFilePath: "src/lib.rs",
					lineStart: 51,
					colStart: 4, // col of 'names' in "names.sort()"
					targetKey: "names.sort",
				},
				{
					edgeUid: "e-vec-dedup",
					sourceFilePath: "src/lib.rs",
					lineStart: 52,
					colStart: 4, // col of 'names' in "names.dedup()"
					targetKey: "names.dedup",
				},
				{
					edgeUid: "e-hashmap",
					sourceFilePath: "src/lib.rs",
					lineStart: 36,
					colStart: 8, // col of 'self' in "self.players.insert(...)"
					targetKey: "self.players.insert",
				},
				{
					edgeUid: "e-bad-line",
					sourceFilePath: "src/lib.rs",
					lineStart: 999,
					colStart: 0,
					targetKey: "nothing.here",
				},
			];

			const results = await resolveRustReceiverTypes(FIXTURE_ROOT, sites);
			expect(results.length).toBe(5);

			// GameState receiver: MUST resolve (not conditional).
			const gs = results.find((r) => r.edgeUid === "e-gamestate");
			expect(gs).toBeDefined();
			expect(gs!.receiverType).toBe("GameState");
			expect(gs!.receiverTypeOrigin).toBe("compiler");
			expect(gs!.failureReason).toBeNull();

			// Vec receiver: MUST resolve.
			const vec = results.find((r) => r.edgeUid === "e-vec-sort");
			expect(vec).toBeDefined();
			expect(vec!.receiverType).toBe("Vec");
			expect(vec!.receiverTypeOrigin).toBe("compiler");

			// Vec dedup: MUST also resolve to Vec.
			const dedup = results.find((r) => r.edgeUid === "e-vec-dedup");
			expect(dedup).toBeDefined();
			expect(dedup!.receiverType).toBe("Vec");

			// Bad line: MUST fail, not crash.
			const bad = results.find((r) => r.edgeUid === "e-bad-line");
			expect(bad).toBeDefined();
			expect(bad!.receiverTypeOrigin).toBe("failed");
		},
		180000,
	);

	itRA(
		"returns failed for nonexistent source file",
		async () => {
			const results = await resolveRustReceiverTypes(FIXTURE_ROOT, [
				{
					edgeUid: "e-missing",
					sourceFilePath: "src/nonexistent.rs",
					lineStart: 1,
					colStart: 0,
					targetKey: "x.y",
				},
			]);
			expect(results.length).toBe(1);
			expect(results[0].receiverTypeOrigin).toBe("failed");
			expect(results[0].receiverType).toBeNull();
		},
		180000,
	);

	itRA(
		"returns empty for empty input",
		async () => {
			const results = await resolveRustReceiverTypes(FIXTURE_ROOT, []);
			expect(results).toEqual([]);
		},
		5000,
	);
});
