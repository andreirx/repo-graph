/**
 * MAP.md file writer.
 * Handles frontmatter generation and file output.
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import matter from 'gray-matter';
import type { MapFrontmatter } from './types.js';

const VERSION = '0.1.0';

export interface WriteMapOptions {
  /** Absolute path to the folder where MAP.md should be written */
  folderPath: string;
  /** Generated summary content (markdown body) */
  content: string;
  /** Scope of this MAP (v0.1.0: always 'folder') */
  scope: 'folder';
  /** Path relative to repo root */
  relativePath: string;
  /** Adapter name used */
  adapter: string;
  /** Model name used */
  model: string;
  /** Git commit hash (if available) */
  basisCommit: string | null;
  /** Synthesis basis */
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs';
  /** Confidence level */
  confidence: 'high' | 'medium' | 'low';
  /** Child MAP.md paths (for folder scopes) */
  childMaps?: string[];
  /** Source files summarized (for file scope) */
  sourceFiles?: string[];
  /** Output filename (default: MAP.md) */
  filename?: string;
}

/**
 * Write a MAP.md file with proper frontmatter.
 */
export async function writeMap(options: WriteMapOptions): Promise<string> {
  const frontmatter: MapFrontmatter = {
    generated_by: 'rgistr',
    generator_version: VERSION,
    adapter: options.adapter,
    model: options.model,
    basis_commit: options.basisCommit,
    scope: options.scope,
    path: options.relativePath || '.',
    generated_at: new Date().toISOString(),
    synthesis_basis: options.synthesisBasis,
    confidence: options.confidence
  };

  if (options.childMaps && options.childMaps.length > 0) {
    frontmatter.child_maps = options.childMaps;
  }

  if (options.sourceFiles && options.sourceFiles.length > 0) {
    frontmatter.source_files = options.sourceFiles;
  }

  const filename = options.filename || 'MAP.md';
  const outputPath = path.join(options.folderPath, filename);
  const fileContent = matter.stringify(options.content, frontmatter);

  await fs.writeFile(outputPath, fileContent, 'utf-8');
  return outputPath;
}

/**
 * Read an existing MAP.md and parse its frontmatter.
 */
export async function readMap(mapPath: string): Promise<{
  frontmatter: Partial<MapFrontmatter>;
  content: string;
} | null> {
  try {
    const raw = await fs.readFile(mapPath, 'utf-8');
    const { data, content } = matter(raw);
    return {
      frontmatter: data as Partial<MapFrontmatter>,
      content: content.trim()
    };
  } catch {
    return null;
  }
}

/**
 * Check if a MAP.md exists and is fresh (newer than source files).
 */
export async function isMapFresh(
  mapPath: string,
  sourceFiles: string[]
): Promise<boolean> {
  try {
    const mapStat = await fs.stat(mapPath);
    const mapMtime = mapStat.mtime.getTime();

    for (const file of sourceFiles) {
      try {
        const fileStat = await fs.stat(file);
        if (fileStat.mtime.getTime() > mapMtime) {
          return false; // Source file is newer
        }
      } catch {
        // Source file doesn't exist, consider stale
        return false;
      }
    }

    return true;
  } catch {
    return false; // MAP doesn't exist
  }
}

/**
 * Get the current git commit hash, or null if not in a git repo.
 */
export async function getGitCommit(repoPath: string): Promise<string | null> {
  try {
    const { exec } = await import('node:child_process');
    const { promisify } = await import('node:util');
    const execAsync = promisify(exec);

    const { stdout } = await execAsync('git rev-parse HEAD', {
      cwd: repoPath
    });
    return stdout.trim();
  } catch {
    return null;
  }
}
