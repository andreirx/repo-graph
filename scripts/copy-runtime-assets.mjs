#!/usr/bin/env node

/**
 * Post-`tsc` runnable-dist preparation.
 *
 * `tsc` only emits TypeScript output. Two things still need to
 * happen for `dist/` to be a usable artifact:
 *
 *   1. Non-TS runtime assets (currently only `detectors.toml`)
 *      must be copied to the same directory layout in `dist/` so
 *      the compiled code can resolve them via `import.meta.url`.
 *
 *   2. The CLI entry file must be marked executable. `tsc`
 *      preserves the shebang text but does not set the executable
 *      bit; on POSIX systems this leaves `dist/cli/index.js` at
 *      mode 644, breaking local symlink-based invocations of
 *      `rgr` after every clean build. `npm install -g` flows
 *      handle this automatically; local `pnpm run build` does
 *      not, so we must do it ourselves.
 *
 * This script is the cross-platform replacement for `cp` and
 * `chmod` in shell scripts. It uses node:fs only — no shell
 * dependency, works on Windows, Linux, and macOS identically.
 * `chmodSync` is a no-op on Windows file systems.
 *
 * Usage:
 *   pnpm run build       # build script invokes this after tsc
 *   node scripts/copy-runtime-assets.mjs
 *
 * Failure mode:
 *   - missing source asset → exit 1, error printed to stderr
 *   - missing dist subdirectory → mkdir -p semantics (recursive)
 *   - missing executable target → exit 1, error printed to stderr
 *   - other I/O error → exit 1, error printed to stderr
 *
 * Adding a new runtime asset: append to the ASSETS array.
 * Adding a new executable: append to the EXECUTABLES array.
 * All entries are repo-root-relative.
 */

import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Runtime assets that must be present in `dist/` for the compiled
 * CLI to function. Each entry is repo-root-relative.
 */
const ASSETS = [
	{
		src: "src/core/seams/detectors/detectors.toml",
		dest: "dist/core/seams/detectors/detectors.toml",
	},
];

/**
 * Files in `dist/` that must be marked executable after build.
 * tsc emits these at mode 644 even when the source has a `#!`
 * shebang; we set mode 755 explicitly.
 *
 * On Windows file systems, chmodSync is silently no-op (FAT/NTFS
 * have no concept of POSIX permissions); npm/pnpm generate `.cmd`
 * shims at install time for the Windows case via the package.json
 * `bin` field. Local Windows development without an install step
 * is not supported by this repo's local-build flow.
 */
const EXECUTABLES = ["dist/cli/index.js"];

let failed = false;

// ── Step 1: copy non-TS runtime assets ────────────────────────────
for (const { src, dest } of ASSETS) {
	const absSrc = join(REPO_ROOT, src);
	const absDest = join(REPO_ROOT, dest);

	if (!existsSync(absSrc)) {
		console.error(`copy-runtime-assets: missing source: ${src}`);
		failed = true;
		continue;
	}

	try {
		mkdirSync(dirname(absDest), { recursive: true });
		copyFileSync(absSrc, absDest);
	} catch (err) {
		console.error(
			`copy-runtime-assets: failed to copy ${src} -> ${dest}: ${err.message}`,
		);
		failed = true;
	}
}

// ── Step 2: mark CLI entry files executable ───────────────────────
for (const rel of EXECUTABLES) {
	const abs = join(REPO_ROOT, rel);
	if (!existsSync(abs)) {
		console.error(
			`copy-runtime-assets: missing executable target: ${rel} (did tsc emit it?)`,
		);
		failed = true;
		continue;
	}
	try {
		chmodSync(abs, 0o755);
	} catch (err) {
		console.error(
			`copy-runtime-assets: failed to chmod ${rel}: ${err.message}`,
		);
		failed = true;
	}
}

if (failed) process.exit(1);
