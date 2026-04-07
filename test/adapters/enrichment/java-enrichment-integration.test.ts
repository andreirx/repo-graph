/**
 * Live Java enrichment integration test.
 *
 * Exercises the end-to-end path through jdtls:
 *   - subprocess startup with server-request handling
 *   - project import via language/status readiness (ServiceReady)
 *   - live hover requests on real Java code
 *   - type extraction from jdtls hover array format
 *   - failure handling for bad positions
 *
 * REQUIRES jdtls on PATH and a compilable Gradle fixture.
 * Skips gracefully if jdtls is not available.
 *
 * Slower than unit tests (~30-120s) because of Gradle project import.
 */

import { execSync } from "node:child_process";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { resolveJavaReceiverTypes } from "../../../src/adapters/enrichment/java-receiver-resolver.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../fixtures/java/spring-mini",
);

let jdtlsAvailable = false;
try {
	execSync("which jdtls", { stdio: "pipe" });
	jdtlsAvailable = true;
} catch {
	// not on PATH
}

const itJDT = jdtlsAvailable ? it : it.skip;

describe("resolveJavaReceiverTypes — live jdtls", () => {
	itJDT(
		"resolves known receiver types from the Java fixture",
		async () => {
			// Line numbers verified against fixture:
			// 11: scores.put(key, value)  → scores is HashMap
			//     ^col 8
			// 15: return new ArrayList<>(scores.keySet())
			//                ^col 15 on "new ArrayList"... actually
			// 20: sb.append("Scores: ")   → sb is StringBuilder
			//     ^col 8
			const sites = [
				{
					edgeUid: "e-hashmap",
					sourceFilePath: "src/main/java/com/example/App.java",
					lineStart: 11,
					colStart: 8,
					targetKey: "scores.put",
				},
				{
					edgeUid: "e-stringbuilder",
					sourceFilePath: "src/main/java/com/example/App.java",
					lineStart: 20,
					colStart: 8,
					targetKey: "sb.append",
				},
				{
					edgeUid: "e-bad-line",
					sourceFilePath: "src/main/java/com/example/App.java",
					lineStart: 999,
					colStart: 0,
					targetKey: "nothing.here",
				},
			];

			const results = await resolveJavaReceiverTypes(FIXTURE_ROOT, sites);
			expect(results.length).toBe(3);

			// HashMap receiver: MUST resolve.
			const hm = results.find((r) => r.edgeUid === "e-hashmap");
			expect(hm).toBeDefined();
			expect(hm!.receiverType).toBe("HashMap");
			expect(hm!.receiverTypeOrigin).toBe("compiler");
			expect(hm!.failureReason).toBeNull();

			// StringBuilder receiver: MUST resolve.
			const sb = results.find((r) => r.edgeUid === "e-stringbuilder");
			expect(sb).toBeDefined();
			expect(sb!.receiverType).toBe("StringBuilder");
			expect(sb!.receiverTypeOrigin).toBe("compiler");

			// Bad line: MUST fail, not crash.
			const bad = results.find((r) => r.edgeUid === "e-bad-line");
			expect(bad).toBeDefined();
			expect(bad!.receiverTypeOrigin).toBe("failed");
		},
		300000, // 5-minute timeout for Gradle import
	);

	itJDT(
		"returns failed for nonexistent source file",
		async () => {
			const results = await resolveJavaReceiverTypes(FIXTURE_ROOT, [
				{
					edgeUid: "e-missing",
					sourceFilePath: "src/main/java/Missing.java",
					lineStart: 1,
					colStart: 0,
					targetKey: "x.y",
				},
			]);
			expect(results.length).toBe(1);
			expect(results[0].receiverTypeOrigin).toBe("failed");
		},
		300000,
	);

	itJDT(
		"returns empty for empty input",
		async () => {
			const results = await resolveJavaReceiverTypes(FIXTURE_ROOT, []);
			expect(results).toEqual([]);
		},
		5000,
	);
});
