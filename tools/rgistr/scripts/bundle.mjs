#!/usr/bin/env node
/**
 * Bundle rgistr for Node SEA packaging.
 *
 * Reads workspace version from rust/Cargo.toml and injects it as __VERSION__.
 * Produces a single ESM bundle at build/rgistr.bundle.mjs.
 *
 * Usage: node scripts/bundle.mjs
 *
 * See: docs/slices/rgistr-1-binary-packaging.md
 */

import * as esbuild from 'esbuild';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const RGISTR_ROOT = path.resolve(__dirname, '..');
const REPO_ROOT = path.resolve(RGISTR_ROOT, '../..');
const CARGO_TOML = path.join(REPO_ROOT, 'rust/Cargo.toml');
const BUILD_DIR = path.join(RGISTR_ROOT, 'build');
const BUNDLE_OUTPUT = path.join(BUILD_DIR, 'rgistr.bundle.cjs');

/**
 * Extract version from [workspace.package] section of Cargo.toml.
 */
function getWorkspaceVersion() {
  const content = fs.readFileSync(CARGO_TOML, 'utf-8');

  // Find [workspace.package] section and extract version
  const workspaceMatch = content.match(/\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/);
  if (workspaceMatch) {
    return workspaceMatch[1];
  }

  // Fallback: try first version = line (less reliable)
  const versionMatch = content.match(/^version\s*=\s*"([^"]+)"/m);
  if (versionMatch) {
    return versionMatch[1];
  }

  throw new Error(`Could not extract version from ${CARGO_TOML}`);
}

async function bundle() {
  // Ensure build directory exists
  fs.mkdirSync(BUILD_DIR, { recursive: true });

  // Get workspace version
  const version = getWorkspaceVersion();
  console.log(`Bundling rgistr with version: ${version}`);

  // Bundle with esbuild
  // Use CommonJS format for Node SEA compatibility
  const result = await esbuild.build({
    entryPoints: [path.join(RGISTR_ROOT, 'src/cli.ts')],
    bundle: true,
    platform: 'node',
    target: 'node20',
    format: 'cjs',  // CommonJS for SEA, avoids ESM dynamic require issues
    outfile: BUNDLE_OUTPUT,
    // Inject version at build time
    define: {
      '__VERSION__': JSON.stringify(version),
    },
    // Bundle ALL dependencies - no externals for SEA
    // If a dependency fails to bundle, we stop and investigate
    external: [],
    // Generate source map for debugging (optional, can remove for smaller bundle)
    sourcemap: false,
    // Minify for smaller binary
    minify: true,
    // Keep names for better error messages
    keepNames: true,
    // No shebang in ESM bundle - SEA packaging handles execution
  });

  if (result.errors.length > 0) {
    console.error('Bundle errors:', result.errors);
    process.exit(1);
  }

  if (result.warnings.length > 0) {
    console.warn('Bundle warnings:', result.warnings);
  }

  const stats = fs.statSync(BUNDLE_OUTPUT);
  const sizeKB = (stats.size / 1024).toFixed(1);
  console.log(`Bundle created: ${BUNDLE_OUTPUT} (${sizeKB} KB)`);
}

bundle().catch((err) => {
  console.error('Bundle failed:', err);
  process.exit(1);
});
