/**
 * Token budget planning.
 *
 * Calculates context budgets and determines chunking requirements.
 */

import type { ContextBudget, BudgetPolicy, TokenEstimate } from './types.js';
import { getModelCapability } from './registry.js';

// ── Default policy ───────────────────────────────────────────────────

/**
 * Default budget policy.
 *
 * Conservative settings for reliable generation.
 */
export const DEFAULT_BUDGET_POLICY: BudgetPolicy = {
  // System prompt + JSON wrapper overhead
  systemOverheadTokens: 500,

  // Reserve 20% for output
  outputReserveFraction: 0.20,
  minOutputReserve: 1000,
  maxOutputReserve: 8000,

  // Graph context if enabled
  graphContextReserve: 2000,

  // 10% safety margin
  safetyMargin: 0.10,

  // 5% overlap between chunks for continuity
  chunkOverlapFraction: 0.05,
};

/**
 * Conservative policy for local models with smaller context.
 */
export const CONSERVATIVE_BUDGET_POLICY: BudgetPolicy = {
  systemOverheadTokens: 500,
  outputReserveFraction: 0.25,
  minOutputReserve: 1000,
  maxOutputReserve: 4000,
  graphContextReserve: 1000,
  safetyMargin: 0.15,
  chunkOverlapFraction: 0.10,
};

// ── Budget calculation ───────────────────────────────────────────────

/**
 * Calculate context budget for a model.
 */
export function calculateBudget(
  modelId: string,
  options: {
    policy?: BudgetPolicy;
    includeGraphContext?: boolean;
  } = {}
): ContextBudget {
  const capability = getModelCapability(modelId);
  const policy = options.policy ?? DEFAULT_BUDGET_POLICY;
  const includeGraph = options.includeGraphContext ?? false;

  const totalTokens = capability.maxInputTokens;

  // Calculate reserves
  const systemOverhead = policy.systemOverheadTokens;

  const graphContextReserve = includeGraph ? policy.graphContextReserve : 0;

  const outputReserve = Math.min(
    Math.max(
      Math.round(totalTokens * policy.outputReserveFraction),
      policy.minOutputReserve
    ),
    policy.maxOutputReserve
  );

  // Safety margin
  const safetyBuffer = Math.round(totalTokens * policy.safetyMargin);

  // Available for source content
  const sourceContentBudget = Math.max(
    0,
    totalTokens - systemOverhead - graphContextReserve - outputReserve - safetyBuffer
  );

  // Safe chunk size (leaving room for overlap on both ends)
  const overlapBuffer = Math.round(sourceContentBudget * policy.chunkOverlapFraction * 2);
  const safeChunkTokens = Math.max(1000, sourceContentBudget - overlapBuffer);

  return {
    totalTokens,
    systemOverhead,
    graphContextReserve,
    outputReserve,
    sourceContentBudget,
    safeChunkTokens,
    requiresChunking: (estimatedTokens: number) => estimatedTokens > sourceContentBudget,
  };
}

// ── Token estimation ─────────────────────────────────────────────────

/**
 * Estimate token count for text content.
 *
 * Uses simple heuristics. For production, consider using tiktoken.
 */
export function estimateTokens(content: string): TokenEstimate {
  // Simple heuristic: ~4 characters per token for code
  // This is conservative for most code which has more whitespace/symbols
  const charEstimate = Math.ceil(content.length / 4);

  // Alternative: word-based estimate (~1.3 tokens per word)
  const words = content.split(/\s+/).filter(w => w.length > 0).length;
  const wordEstimate = Math.ceil(words * 1.3);

  // Use the higher estimate for safety
  const tokens = Math.max(charEstimate, wordEstimate);

  return {
    tokens,
    method: 'chars_div_4',
    confidence: 0.7, // Heuristic estimate
  };
}

/**
 * Estimate tokens for a file based on byte size.
 *
 * Faster than reading content, less accurate.
 */
export function estimateTokensFromSize(byteSize: number): TokenEstimate {
  // Assume ~4 bytes per token on average for code
  // This accounts for UTF-8 encoding of mostly ASCII
  const tokens = Math.ceil(byteSize / 4);

  return {
    tokens,
    method: 'chars_div_4',
    confidence: 0.5, // Less accurate without content
  };
}

// ── Chunking decision ────────────────────────────────────────────────

/**
 * Determine if a file requires chunking.
 */
export function requiresChunking(
  contentOrSize: string | number,
  modelId: string,
  options?: { policy?: BudgetPolicy; includeGraphContext?: boolean }
): boolean {
  const budget = calculateBudget(modelId, options);

  const estimate = typeof contentOrSize === 'string'
    ? estimateTokens(contentOrSize)
    : estimateTokensFromSize(contentOrSize);

  return budget.requiresChunking(estimate.tokens);
}

/**
 * Calculate recommended chunk count for content.
 */
export function recommendedChunkCount(
  estimatedTokens: number,
  modelId: string,
  options?: { policy?: BudgetPolicy; includeGraphContext?: boolean }
): number {
  const budget = calculateBudget(modelId, options);

  if (estimatedTokens <= budget.sourceContentBudget) {
    return 1; // No chunking needed
  }

  // Calculate chunks with overlap consideration
  const effectiveChunkSize = budget.safeChunkTokens;
  const chunks = Math.ceil(estimatedTokens / effectiveChunkSize);

  return Math.max(1, chunks);
}
