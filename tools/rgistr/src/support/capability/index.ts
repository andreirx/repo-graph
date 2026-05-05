/**
 * Capability support module.
 *
 * Provides model capability registry and token budget planning.
 */

// Types
export type {
  ModelCapability,
  ContextBudget,
  BudgetPolicy,
  TokenEstimate,
} from './types.js';

// Registry
export {
  MODEL_CAPABILITIES,
  getModelCapability,
  supportsJsonMode,
  getMaxInputTokens,
  getMaxOutputTokens,
} from './registry.js';

// Budget planning
export {
  DEFAULT_BUDGET_POLICY,
  CONSERVATIVE_BUDGET_POLICY,
  calculateBudget,
  estimateTokens,
  estimateTokensFromSize,
  requiresChunking,
  recommendedChunkCount,
} from './budget.js';
