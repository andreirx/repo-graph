/**
 * Model capability support module types.
 *
 * DTOs for model capabilities, context budgets, and planning.
 */

import type { TransportFamily } from '../discovery/types.js';

// ── Model capabilities ───────────────────────────────────────────────

/**
 * Known capabilities for a model.
 *
 * Used for planning prompt budgets and feature availability.
 */
export interface ModelCapability {
  /** Model ID pattern (may include wildcards). */
  pattern: string;

  /** Human-readable model family name. */
  family: string;

  /** Maximum input context in tokens. */
  maxInputTokens: number;

  /** Maximum output tokens. */
  maxOutputTokens: number;

  /** Whether JSON mode is supported. */
  supportsJsonMode: boolean;

  /** Whether streaming is supported. */
  supportsStreaming: boolean;

  /** Whether tool/function calling is supported. */
  supportsTools: boolean;

  /** Transport families this applies to. */
  transports: TransportFamily[];

  /** Source of capability data (hardcoded, probed, user). */
  source: 'hardcoded' | 'probed' | 'user';

  /** Notes about this model's behavior. */
  notes?: string;
}

// ── Context budget ───────────────────────────────────────────────────

/**
 * Calculated context budget for a generation task.
 */
export interface ContextBudget {
  /** Total available context (input + output). */
  totalTokens: number;

  /** Reserved for system prompt and wrapper overhead. */
  systemOverhead: number;

  /** Reserved for graph context (if enabled). */
  graphContextReserve: number;

  /** Reserved for output generation. */
  outputReserve: number;

  /** Available for source content. */
  sourceContentBudget: number;

  /** Safe chunk size for single-shot processing. */
  safeChunkTokens: number;

  /** Whether chunking is required for large files. */
  requiresChunking: (estimatedTokens: number) => boolean;
}

// ── Budget policy ────────────────────────────────────────────────────

/**
 * Policy for budget calculation.
 */
export interface BudgetPolicy {
  /** System prompt overhead estimate in tokens. */
  systemOverheadTokens: number;

  /** Output reserve as fraction of total (e.g., 0.25 = 25%). */
  outputReserveFraction: number;

  /** Minimum output reserve in tokens. */
  minOutputReserve: number;

  /** Maximum output reserve in tokens. */
  maxOutputReserve: number;

  /** Graph context reserve if enabled. */
  graphContextReserve: number;

  /** Safety margin fraction (e.g., 0.1 = 10% buffer). */
  safetyMargin: number;

  /** Chunk overlap fraction for continuity. */
  chunkOverlapFraction: number;
}

// ── Token estimation ─────────────────────────────────────────────────

/**
 * Token count estimate for content.
 */
export interface TokenEstimate {
  /** Estimated token count. */
  tokens: number;

  /** Estimation method used. */
  method: 'chars_div_4' | 'words' | 'tiktoken' | 'exact';

  /** Confidence in estimate (0-1). */
  confidence: number;
}
