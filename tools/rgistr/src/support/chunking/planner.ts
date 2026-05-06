/**
 * Chunk planner.
 *
 * Plans how to split a file into chunks for processing.
 */

import type { ChunkPlan, PlannedChunk, SourceSpan } from './types.js';
import { computeFileHash, createChunkIdentity } from './identity.js';
import { estimateTokens, calculateBudget } from '../capability/index.js';

// ── Planning configuration ───────────────────────────────────────────

export interface ChunkPlannerConfig {
  /** Model ID for budget calculation. */
  modelId: string;

  /** Target chunk size as fraction of safe budget (default: 0.8). */
  targetChunkFraction?: number;

  /** Minimum chunk size in lines (default: 10). */
  minChunkLines?: number;

  /** Maximum chunk size in lines (default: 500). */
  maxChunkLines?: number;

  /** Overlap lines between chunks (default: 5). */
  overlapLines?: number;

  /** Whether to include graph context in budget. */
  includeGraphContext?: boolean;
}

const DEFAULT_CONFIG: Required<Omit<ChunkPlannerConfig, 'modelId'>> = {
  targetChunkFraction: 0.8,
  minChunkLines: 10,
  maxChunkLines: 500,
  overlapLines: 5,
  includeGraphContext: false,
};

// ── Main planner ─────────────────────────────────────────────────────

/**
 * Plan chunks for a file.
 *
 * Returns a chunk plan that determines whether chunking is needed
 * and how to split the file.
 */
export function planChunks(
  filePath: string,
  content: string,
  config: ChunkPlannerConfig
): ChunkPlan {
  const cfg = { ...DEFAULT_CONFIG, ...config };
  const fileHash = computeFileHash(content);
  const fileSizeBytes = Buffer.byteLength(content, 'utf-8');
  const tokenEstimate = estimateTokens(content);
  const budget = calculateBudget(cfg.modelId, {
    includeGraphContext: cfg.includeGraphContext,
  });

  const targetTokens = Math.floor(budget.safeChunkTokens * cfg.targetChunkFraction);

  // Check if whole-file processing is possible
  if (tokenEstimate.tokens <= budget.sourceContentBudget) {
    return {
      filePath,
      fileHash,
      fileSizeBytes,
      estimatedTotalTokens: tokenEstimate.tokens,
      requiresChunking: false,
      chunks: [],
      method: 'whole_file',
    };
  }

  // Need to chunk - use line-window chunking
  const lines = content.split('\n');
  const chunks = planLineWindowChunks(
    lines,
    fileHash,
    targetTokens,
    cfg.minChunkLines,
    cfg.maxChunkLines,
    cfg.overlapLines
  );

  return {
    filePath,
    fileHash,
    fileSizeBytes,
    estimatedTotalTokens: tokenEstimate.tokens,
    requiresChunking: true,
    chunks,
    method: 'line_window',
  };
}

// ── Line-window chunking ─────────────────────────────────────────────

/**
 * Plan chunks using line-window approach.
 *
 * This is the fallback when structural chunking is not available.
 */
function planLineWindowChunks(
  lines: string[],
  fileHash: string,
  targetTokens: number,
  minLines: number,
  maxLines: number,
  overlapLines: number
): PlannedChunk[] {
  const chunks: PlannedChunk[] = [];
  let currentStart = 0;

  while (currentStart < lines.length) {
    // Find the end of this chunk
    let currentEnd = currentStart;
    let currentTokens = 0;

    while (currentEnd < lines.length) {
      const lineTokens = estimateTokens(lines[currentEnd]).tokens;

      // Check if adding this line would exceed target
      if (currentTokens + lineTokens > targetTokens && currentEnd - currentStart >= minLines) {
        break;
      }

      currentTokens += lineTokens;
      currentEnd++;

      // Respect maximum chunk size
      if (currentEnd - currentStart >= maxLines) {
        break;
      }
    }

    // Ensure we have at least minLines
    if (currentEnd - currentStart < minLines && currentEnd < lines.length) {
      currentEnd = Math.min(currentStart + minLines, lines.length);
      currentTokens = estimateTokens(lines.slice(currentStart, currentEnd).join('\n')).tokens;
    }

    // Create the chunk
    const span: SourceSpan = {
      startLine: currentStart + 1, // 1-based
      endLine: currentEnd, // 1-based, inclusive
    };

    const identity = createChunkIdentity(
      fileHash,
      chunks.length,
      0, // Will be updated after all chunks are planned
      span
    );

    chunks.push({
      identity,
      estimatedTokens: currentTokens,
      boundaryType: 'arbitrary',
      overlapLines: chunks.length > 0 ? overlapLines : 0,
    });

    // Move to next chunk with overlap
    currentStart = currentEnd - overlapLines;
    if (currentStart <= chunks[chunks.length - 1].identity.span.startLine - 1) {
      // Avoid infinite loop if overlap is larger than chunk
      currentStart = currentEnd;
    }
  }

  // Update chunk counts
  const totalChunks = chunks.length;
  for (const chunk of chunks) {
    // Recreate identity with correct chunk count
    chunk.identity = createChunkIdentity(
      fileHash,
      chunk.identity.chunkIndex,
      totalChunks,
      chunk.identity.span
    );
  }

  return chunks;
}

// ── Chunk content extraction ─────────────────────────────────────────

/**
 * Extract content for a chunk from file content.
 */
export function extractChunkContent(
  content: string,
  span: SourceSpan
): string {
  const lines = content.split('\n');
  // Convert 1-based span to 0-based array indices
  const startIdx = span.startLine - 1;
  const endIdx = span.endLine; // endLine is inclusive, slice is exclusive
  return lines.slice(startIdx, endIdx).join('\n');
}

/**
 * Get line count for content.
 */
export function countLines(content: string): number {
  return content.split('\n').length;
}
