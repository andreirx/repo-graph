#!/usr/bin/env node

/**
 * Build tree-sitter WASM grammars from installed npm packages.
 *
 * Usage:
 *   pnpm run build:grammars           # Build all grammars
 *   node scripts/build-grammars.mjs   # Same
 *
 * Prerequisites:
 *   - pnpm install (with tree-sitter-cli in onlyBuiltDependencies)
 *   - The tree-sitter-cli native binary must be installed. If it's
 *     missing, run: pnpm rebuild tree-sitter-cli
 *
 * Output: grammars/*.wasm files used by web-tree-sitter at runtime.
 *
 * This script is the reproducible grammar-build path. Do not manually
 * build WASM files with cargo-installed tree-sitter or other ad hoc
 * methods. All grammars should be buildable from this script on any
 * machine with the correct Node.js + pnpm setup.
 */

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const GRAMMARS_DIR = join(ROOT, "grammars");

// Grammar packages and their output filenames.
const GRAMMARS = [
	{ pkg: "tree-sitter-typescript/typescript", out: "tree-sitter-typescript.wasm" },
	{ pkg: "tree-sitter-typescript/tsx", out: "tree-sitter-tsx.wasm" },
	{ pkg: "tree-sitter-rust", out: "tree-sitter-rust.wasm" },
	{ pkg: "tree-sitter-java", out: "tree-sitter-java.wasm" },
	{ pkg: "tree-sitter-python", out: "tree-sitter-python.wasm" },
	{ pkg: "tree-sitter-c", out: "tree-sitter-c.wasm" },
	{ pkg: "tree-sitter-cpp", out: "tree-sitter-cpp.wasm" },
];

// Find the tree-sitter CLI binary.
function findTreeSitterBin() {
	// Try the pnpm-installed path first.
	const pnpmBin = join(ROOT, "node_modules", ".bin", "tree-sitter");
	if (existsSync(pnpmBin)) return pnpmBin;

	// Try system PATH.
	try {
		execFileSync("which", ["tree-sitter"], { encoding: "utf-8" });
		return "tree-sitter";
	} catch {
		// Not on PATH.
	}

	return null;
}

// Main.
const treeSitter = findTreeSitterBin();
if (!treeSitter) {
	console.error("Error: tree-sitter CLI not found.");
	console.error("Run: pnpm rebuild tree-sitter-cli");
	console.error("Or ensure tree-sitter is on PATH.");
	process.exit(1);
}

mkdirSync(GRAMMARS_DIR, { recursive: true });

let built = 0;
let failed = 0;

for (const { pkg, out } of GRAMMARS) {
	const pkgDir = join(ROOT, "node_modules", pkg);
	const outPath = join(GRAMMARS_DIR, out);

	if (!existsSync(pkgDir)) {
		console.log(`  skip: ${pkg} (not installed)`);
		continue;
	}

	console.log(`  build: ${pkg} -> ${out}`);
	try {
		execFileSync(treeSitter, ["build", "--wasm", "-o", outPath], {
			cwd: pkgDir,
			stdio: "pipe",
			timeout: 60000,
		});
		built++;
	} catch (err) {
		console.error(`  FAILED: ${pkg}: ${err.message ?? err}`);
		failed++;
	}
}

console.log(`\nGrammars built: ${built}, failed: ${failed}, skipped: ${GRAMMARS.length - built - failed}`);
if (failed > 0) process.exit(1);
