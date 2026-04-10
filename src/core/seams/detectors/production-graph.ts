/**
 * Production detector graph — eager-loaded singleton.
 *
 * Loads `detectors.toml` ONCE at module-import time using the
 * production hook registry, then exports the resulting
 * `LoadedDetectorGraph` and registry as constants for the
 * env/fs detector wrapper modules to consume.
 *
 * Eager-load semantics (per the slice constraint):
 *  - any malformed declaration in `detectors.toml` throws at
 *    module-import time, NOT at first detector invocation
 *  - the test/build cycle fails fast on schema violations
 *  - the cost is one TOML parse + validation per process
 *
 * The TOML file path is resolved relative to this file's
 * `import.meta.url` so it works both when running from `src/`
 * (vitest, dev) and from `dist/` (built CLI). The build script
 * copies `detectors.toml` to `dist/core/seams/detectors/`
 * alongside the compiled JS.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createDefaultHookRegistry } from "./hooks.js";
import { loadDetectorGraph } from "./loader.js";
import type { HookRegistry, LoadedDetectorGraph } from "./types.js";

const TOML_PATH = join(
	dirname(fileURLToPath(import.meta.url)),
	"detectors.toml",
);

/**
 * The production hook registry. Frozen — every detector wrapper
 * shares the same registry instance.
 */
export const PRODUCTION_HOOK_REGISTRY: HookRegistry =
	createDefaultHookRegistry();

/**
 * The production detector graph, loaded and validated once at
 * module-import time. Throws synchronously on any malformed
 * declaration in `detectors.toml`.
 */
export const PRODUCTION_DETECTOR_GRAPH: LoadedDetectorGraph = loadDetectorGraph(
	readFileSync(TOML_PATH, "utf-8"),
	PRODUCTION_HOOK_REGISTRY,
);
