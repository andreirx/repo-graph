/**
 * Chunk identity generation.
 *
 * Deterministic chunk identifiers for staleness detection and provenance.
 */

import { createHash } from 'node:crypto';
import type { ChunkIdentity, SourceSpan } from './types.js';

// ── File hashing ─────────────────────────────────────────────────────

/**
 * Compute a short hash of file content.
 *
 * Returns first 8 characters of SHA-256 hex digest.
 */
export function computeFileHash(content: string): string {
  const hash = createHash('sha256');
  hash.update(content, 'utf-8');
  return hash.digest('hex').slice(0, 8);
}

// ── Chunk identity ───────────────────────────────────────────────────

/**
 * Create a chunk identity.
 */
export function createChunkIdentity(
  fileHash: string,
  chunkIndex: number,
  chunkCount: number,
  span: SourceSpan
): ChunkIdentity {
  const id = formatChunkId(fileHash, chunkIndex, span);

  return {
    fileHash,
    chunkIndex,
    chunkCount,
    span,
    get id() {
      return id;
    },
  };
}

/**
 * Format chunk ID string.
 *
 * Format: `{file_hash}:{chunk_index}:{line_start}-{line_end}`
 */
export function formatChunkId(
  fileHash: string,
  chunkIndex: number,
  span: SourceSpan
): string {
  return `${fileHash}:${chunkIndex}:${span.startLine}-${span.endLine}`;
}

/**
 * Parse a chunk ID string.
 *
 * Returns null if the format is invalid.
 */
export function parseChunkId(id: string): {
  fileHash: string;
  chunkIndex: number;
  startLine: number;
  endLine: number;
} | null {
  const match = id.match(/^([a-f0-9]{8}):(\d+):(\d+)-(\d+)$/);
  if (!match) {
    return null;
  }

  return {
    fileHash: match[1],
    chunkIndex: parseInt(match[2], 10),
    startLine: parseInt(match[3], 10),
    endLine: parseInt(match[4], 10),
  };
}

// ── Chunk artifact naming ────────────────────────────────────────────

/**
 * Generate artifact filename for a chunk.
 *
 * Format: `{source_basename}.chunk-{index}.gist.md`
 *
 * Example: `generator.ts.chunk-0.gist.md`
 */
export function chunkArtifactFilename(
  sourceFilename: string,
  chunkIndex: number
): string {
  return `${sourceFilename}.chunk-${chunkIndex}.gist.md`;
}

/**
 * Generate artifact filename for a file-level gist.
 *
 * Format: `{source_basename}.gist.md`
 *
 * Example: `generator.ts.gist.md`
 */
export function fileArtifactFilename(sourceFilename: string): string {
  return `${sourceFilename}.gist.md`;
}

/**
 * Parse artifact filename to extract source file and chunk index.
 *
 * Returns null if the format is not recognized.
 */
export function parseArtifactFilename(filename: string): {
  sourceFilename: string;
  scope: 'chunk' | 'file';
  chunkIndex?: number;
} | null {
  // Check for chunk artifact pattern
  const chunkMatch = filename.match(/^(.+)\.chunk-(\d+)\.gist\.md$/);
  if (chunkMatch) {
    return {
      sourceFilename: chunkMatch[1],
      scope: 'chunk',
      chunkIndex: parseInt(chunkMatch[2], 10),
    };
  }

  // Check for file artifact pattern
  const fileMatch = filename.match(/^(.+)\.gist\.md$/);
  if (fileMatch) {
    return {
      sourceFilename: fileMatch[1],
      scope: 'file',
    };
  }

  return null;
}

// ── Staleness detection ──────────────────────────────────────────────

/**
 * Check if a chunk artifact is stale.
 *
 * An artifact is stale if the file hash has changed.
 */
export function isChunkArtifactStale(
  artifactFileHash: string,
  currentFileHash: string
): boolean {
  return artifactFileHash !== currentFileHash;
}

/**
 * Check if a file artifact is stale.
 *
 * For whole_file mode: stale if file hash changed.
 * For chunk_rollup mode: stale if file hash changed OR any chunk is stale.
 */
export function isFileArtifactStale(
  artifactFileHash: string,
  currentFileHash: string
): boolean {
  return artifactFileHash !== currentFileHash;
}
