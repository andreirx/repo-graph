/**
 * Directory scanner for rgistr.
 * Traverses a codebase and collects file/folder information.
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { FileInfo, FolderInfo, GenerationConfig } from './types.js';

/** File extensions considered code files */
const CODE_EXTENSIONS = new Set([
  '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs',
  '.py', '.pyi',
  '.rs',
  '.java', '.kt', '.kts',
  '.c', '.h', '.cpp', '.hpp', '.cc', '.cxx',
  '.go',
  '.rb',
  '.php',
  '.swift',
  '.scala',
  '.cs',
  '.vue', '.svelte'
]);

/** File extensions considered documentation */
const DOC_EXTENSIONS = new Set([
  '.md', '.mdx', '.txt', '.rst', '.adoc'
]);

/** Directories to always skip */
const SKIP_DIRS = new Set([
  'node_modules', '.git', '.svn', '.hg',
  'dist', 'build', 'out', 'target',
  '.next', '.nuxt', '.output',
  '__pycache__', '.pytest_cache', '.mypy_cache',
  'coverage', '.coverage',
  '.idea', '.vscode',
  'vendor', 'third_party', 'external'
]);

/** Files to always skip */
const SKIP_FILES = new Set([
  '.DS_Store', 'Thumbs.db',
  'package-lock.json', 'pnpm-lock.yaml', 'yarn.lock',
  'Cargo.lock', 'Gemfile.lock', 'poetry.lock'
]);

export interface ScanOptions {
  /** Maximum depth (-1 = unlimited) */
  maxDepth?: number;
  /** Maximum file size in bytes (default: 100KB) */
  maxFileSize?: number;
  /** Additional directories to skip */
  skipDirs?: string[];
  /** Additional files to skip */
  skipFiles?: string[];
}

/**
 * Scan a directory tree and collect file/folder information.
 */
export async function scanDirectory(
  rootPath: string,
  options: ScanOptions = {}
): Promise<FolderInfo> {
  const maxDepth = options.maxDepth ?? -1;
  const maxFileSize = options.maxFileSize ?? 100 * 1024;
  const skipDirs = new Set([...SKIP_DIRS, ...(options.skipDirs || [])]);
  const skipFiles = new Set([...SKIP_FILES, ...(options.skipFiles || [])]);

  async function scan(dirPath: string, relativePath: string, depth: number): Promise<FolderInfo> {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    const files: FileInfo[] = [];
    const folders: FolderInfo[] = [];
    let hasMap = false;

    for (const entry of entries) {
      const entryPath = path.join(dirPath, entry.name);
      const entryRelative = relativePath ? path.join(relativePath, entry.name) : entry.name;

      if (entry.isDirectory()) {
        if (skipDirs.has(entry.name)) continue;
        if (maxDepth >= 0 && depth >= maxDepth) continue;

        const subFolder = await scan(entryPath, entryRelative, depth + 1);
        folders.push(subFolder);
      } else if (entry.isFile()) {
        if (skipFiles.has(entry.name)) continue;
        if (entry.name === 'MAP.md') {
          hasMap = true;
          continue; // Don't include MAP.md in files list
        }

        const stat = await fs.stat(entryPath);
        if (stat.size > maxFileSize) continue;

        const ext = path.extname(entry.name).toLowerCase();
        files.push({
          path: entryPath,
          relativePath: entryRelative,
          extension: ext,
          size: stat.size,
          mtime: stat.mtime,
          isCode: CODE_EXTENSIONS.has(ext),
          isDoc: DOC_EXTENSIONS.has(ext)
        });
      }
    }

    // Sort for deterministic output
    files.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
    folders.sort((a, b) => a.relativePath.localeCompare(b.relativePath));

    return {
      path: dirPath,
      relativePath,
      files,
      folders,
      hasMap,
      depth
    };
  }

  return scan(rootPath, '', 0);
}

/**
 * Get all code files in a folder (non-recursive).
 */
export function getCodeFiles(folder: FolderInfo): FileInfo[] {
  return folder.files.filter(f => f.isCode);
}

/**
 * Get all documentation files in a folder (non-recursive).
 */
export function getDocFiles(folder: FolderInfo): FileInfo[] {
  return folder.files.filter(f => f.isDoc);
}

/**
 * Count total files in a folder tree.
 */
export function countFiles(folder: FolderInfo): number {
  let count = folder.files.length;
  for (const sub of folder.folders) {
    count += countFiles(sub);
  }
  return count;
}

/**
 * Count total code files in a folder tree.
 */
export function countCodeFiles(folder: FolderInfo): number {
  let count = folder.files.filter(f => f.isCode).length;
  for (const sub of folder.folders) {
    count += countCodeFiles(sub);
  }
  return count;
}

/**
 * Get folders that need MAP.md generation (depth-first order).
 *
 * Returns all folders with code content. Caller is responsible for
 * staleness checks on folders that already have MAPs.
 *
 * @param force - If true, include all folders regardless of existing MAP
 * @param includeWithMap - If true, include folders with existing MAPs (for staleness check)
 */
export function getFoldersForGeneration(
  root: FolderInfo,
  force: boolean = false,
  includeWithMap: boolean = true
): FolderInfo[] {
  const result: FolderInfo[] = [];

  function collect(folder: FolderInfo): void {
    // Depth-first: process children first
    for (const sub of folder.folders) {
      collect(sub);
    }

    // Only include if has code files or subfolders with code
    const hasCode = getCodeFiles(folder).length > 0 ||
                    folder.folders.some(f => countCodeFiles(f) > 0);

    if (!hasCode) return;

    if (force) {
      // Force: include everything
      result.push(folder);
    } else if (!folder.hasMap) {
      // No MAP: always include
      result.push(folder);
    } else if (includeWithMap) {
      // Has MAP but caller wants to check staleness
      result.push(folder);
    }
    // else: has MAP and caller doesn't want staleness check - skip
  }

  collect(root);
  return result;
}
