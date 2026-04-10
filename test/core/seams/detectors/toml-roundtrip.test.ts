/**
 * Roundtrip test for the production detectors.toml file.
 *
 * Verifies that the actual detector graph file:
 *   - parses without TOML errors
 *   - validates against the loader's strict schema
 *   - resolves every hook reference against the production hook
 *     registry built by `createDefaultHookRegistry`
 *   - declares every previously-supported pattern_id (loose
 *     coverage check — exact byte-identical detector output is
 *     verified at Substep D when the env-detectors.ts and
 *     fs-mutation-detectors.ts wrappers are wired through the
 *     walker)
 *
 * This test is the file-level gate. If it passes, the data file
 * is internally consistent with the loader, hook registry, and
 * walker. If a future detector author edits the file in a way
 * that violates the schema or references an unregistered hook,
 * this test catches it before any indexer code runs.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { createDefaultHookRegistry } from "../../../../src/core/seams/detectors/hooks.js";
import { loadDetectorGraph } from "../../../../src/core/seams/detectors/loader.js";

const TOML_PATH = join(
	import.meta.dirname,
	"../../../../src/core/seams/detectors/detectors.toml",
);

describe("detectors.toml — production graph roundtrip", () => {
	it("loads cleanly with the default hook registry", () => {
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);
		expect(graph.allRecords.length).toBeGreaterThan(0);
	});

	it("indexes records under every expected (family, language) bucket", () => {
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);

		// Env coverage: every language family the legacy detector
		// dispatched on must have at least one env record.
		expect(graph.byFamilyLanguage.get("env:ts")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("env:py")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("env:rs")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("env:java")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("env:c")?.length).toBeGreaterThanOrEqual(1);

		// Fs coverage: same expectation.
		expect(graph.byFamilyLanguage.get("fs:ts")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("fs:py")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("fs:rs")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("fs:java")?.length).toBeGreaterThanOrEqual(1);
		expect(graph.byFamilyLanguage.get("fs:c")?.length).toBeGreaterThanOrEqual(1);
	});

	it("declares every legacy env access pattern", () => {
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);

		// All EnvAccessPattern enum values used by the legacy
		// detectors must be reachable from at least one record's
		// static access_pattern declaration. (Hook-only patterns
		// would also count, but the production graph declares all
		// patterns statically.)
		const patternsSeen = new Set<string>();
		for (const r of graph.allRecords) {
			if (r.family === "env" && r.accessPattern !== null) {
				patternsSeen.add(r.accessPattern);
			}
		}
		expect(patternsSeen).toContain("process_env_dot");
		expect(patternsSeen).toContain("process_env_bracket");
		expect(patternsSeen).toContain("process_env_destructure");
		expect(patternsSeen).toContain("os_environ");
		expect(patternsSeen).toContain("os_getenv");
		expect(patternsSeen).toContain("std_env_var");
		expect(patternsSeen).toContain("system_getenv");
		expect(patternsSeen).toContain("c_getenv");
	});

	it("declares every legacy fs mutation pattern", () => {
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);

		// MutationPattern enum values may come from static
		// base_pattern fields OR from hook output. The production
		// graph declares most as static base_pattern; a few
		// (py_open_*, c_fopen_*, py_os_remove/unlink/rmtree,
		// py_os_mkdir/makedirs) are hook-supplied at runtime and
		// are NOT in the static base_pattern set. The static set
		// is the load-time-visible coverage proof.
		const staticPatterns = new Set<string>();
		for (const r of graph.allRecords) {
			if (r.family === "fs" && r.basePattern !== null) {
				staticPatterns.add(r.basePattern);
			}
		}
		// Static base patterns we expect to see declared:
		expect(staticPatterns).toContain("fs_write_file");
		expect(staticPatterns).toContain("fs_append_file");
		expect(staticPatterns).toContain("fs_unlink");
		expect(staticPatterns).toContain("fs_rm");
		expect(staticPatterns).toContain("fs_mkdir");
		expect(staticPatterns).toContain("fs_create_write_stream");
		expect(staticPatterns).toContain("fs_rename");
		expect(staticPatterns).toContain("fs_copy_file");
		expect(staticPatterns).toContain("fs_chmod");
		expect(staticPatterns).toContain("py_pathlib_write");
		expect(staticPatterns).toContain("py_pathlib_mkdir");
		expect(staticPatterns).toContain("py_tempfile");
		expect(staticPatterns).toContain("rust_fs_write");
		expect(staticPatterns).toContain("rust_fs_remove_file");
		expect(staticPatterns).toContain("rust_fs_remove_dir_all");
		expect(staticPatterns).toContain("rust_fs_create_dir");
		expect(staticPatterns).toContain("rust_fs_rename");
		expect(staticPatterns).toContain("rust_fs_copy");
		expect(staticPatterns).toContain("java_files_write");
		expect(staticPatterns).toContain("java_files_delete");
		expect(staticPatterns).toContain("java_files_create_directory");
		expect(staticPatterns).toContain("java_file_output_stream");
		expect(staticPatterns).toContain("c_unlink");
		expect(staticPatterns).toContain("c_remove");
		expect(staticPatterns).toContain("c_rmdir");
		expect(staticPatterns).toContain("c_mkdir");
	});

	it("position_dedup_group is only declared on Rust fs records", () => {
		// Sanity check: the only legacy detector with same-position
		// dedup is the Rust fs paired-pattern set. The production
		// graph should declare position_dedup_group ONLY on Rust
		// fs records. Any other use is an authoring mistake that
		// would silently change behavior on another language.
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);
		for (const r of graph.allRecords) {
			if (r.family !== "fs") continue;
			if (r.positionDedupGroup !== null) {
				expect(r.language).toBe("rs");
			}
		}
	});

	it("single_match_per_line records cover the legacy single-match-per-line cases", () => {
		const tomlSource = readFileSync(TOML_PATH, "utf-8");
		const registry = createDefaultHookRegistry();
		const graph = loadDetectorGraph(tomlSource, registry);
		const flagged = graph.allRecords
			.filter((r) => r.singleMatchPerLine)
			.map((r) => r.patternId);
		// The four legacy single-match-per-line cases:
		expect(flagged).toContain("process_env_destructure");
		expect(flagged).toContain("py_tempfile");
		expect(flagged).toContain("java_files_write");
		expect(flagged).toContain("java_files_delete");
		expect(flagged).toContain("java_files_create_directory");
		expect(flagged).toContain("java_file_output_stream");
	});
});
