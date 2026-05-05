/**
 * Unit tests for token budget planning.
 */

import { describe, it, expect } from 'vitest';
import {
  calculateBudget,
  estimateTokens,
  estimateTokensFromSize,
  requiresChunking,
  recommendedChunkCount,
  DEFAULT_BUDGET_POLICY,
} from './budget.js';

describe('calculateBudget', () => {
  it('calculates budget for large-context model', () => {
    const budget = calculateBudget('gpt-4.1-mini');

    expect(budget.totalTokens).toBe(1000000);
    expect(budget.systemOverhead).toBe(DEFAULT_BUDGET_POLICY.systemOverheadTokens);
    expect(budget.sourceContentBudget).toBeGreaterThan(0);
    expect(budget.sourceContentBudget).toBeLessThan(budget.totalTokens);
  });

  it('calculates budget for smaller model', () => {
    const budget = calculateBudget('llama2-7b');

    expect(budget.totalTokens).toBe(8192); // Fallback
    expect(budget.sourceContentBudget).toBeGreaterThan(0);
    expect(budget.sourceContentBudget).toBeLessThan(budget.totalTokens);
  });

  it('reserves space for graph context when enabled', () => {
    const withoutGraph = calculateBudget('gpt-4o-mini', { includeGraphContext: false });
    const withGraph = calculateBudget('gpt-4o-mini', { includeGraphContext: true });

    expect(withGraph.graphContextReserve).toBeGreaterThan(0);
    expect(withoutGraph.graphContextReserve).toBe(0);
    expect(withGraph.sourceContentBudget).toBeLessThan(withoutGraph.sourceContentBudget);
  });

  it('requiresChunking returns correct value', () => {
    const budget = calculateBudget('gpt-4o-mini');

    // Small content should not require chunking
    expect(budget.requiresChunking(1000)).toBe(false);

    // Huge content should require chunking
    expect(budget.requiresChunking(budget.totalTokens)).toBe(true);
  });
});

describe('estimateTokens', () => {
  it('estimates tokens for code content', () => {
    const code = `
function hello() {
  console.log("Hello, world!");
}
    `.trim();

    const estimate = estimateTokens(code);

    expect(estimate.tokens).toBeGreaterThan(0);
    expect(estimate.method).toBe('chars_div_4');
    expect(estimate.confidence).toBeGreaterThan(0);
    expect(estimate.confidence).toBeLessThanOrEqual(1);
  });

  it('estimates more tokens for longer content', () => {
    const short = 'hello';
    const long = 'hello '.repeat(100);

    const shortEst = estimateTokens(short);
    const longEst = estimateTokens(long);

    expect(longEst.tokens).toBeGreaterThan(shortEst.tokens);
  });

  it('handles empty content', () => {
    const estimate = estimateTokens('');
    expect(estimate.tokens).toBe(0);
  });
});

describe('estimateTokensFromSize', () => {
  it('estimates from byte size', () => {
    const estimate = estimateTokensFromSize(4000);

    expect(estimate.tokens).toBe(1000); // 4000 / 4
    expect(estimate.method).toBe('chars_div_4');
    expect(estimate.confidence).toBeLessThan(1); // Less confident
  });

  it('handles zero size', () => {
    const estimate = estimateTokensFromSize(0);
    expect(estimate.tokens).toBe(0);
  });
});

describe('requiresChunking', () => {
  it('returns false for small content', () => {
    const smallCode = 'const x = 1;';
    expect(requiresChunking(smallCode, 'gpt-4o-mini')).toBe(false);
  });

  it('returns true for very large content', () => {
    // Create content that exceeds even GPT-4.1-mini's budget
    const hugeContent = 'x'.repeat(5000000); // 5MB of content
    expect(requiresChunking(hugeContent, 'gpt-4o-mini')).toBe(true);
  });

  it('works with byte size', () => {
    // Small file
    expect(requiresChunking(1000, 'gpt-4o-mini')).toBe(false);

    // Very large file
    expect(requiresChunking(1000000000, 'gpt-4o-mini')).toBe(true);
  });
});

describe('recommendedChunkCount', () => {
  it('returns 1 for small content', () => {
    expect(recommendedChunkCount(1000, 'gpt-4o-mini')).toBe(1);
  });

  it('returns multiple chunks for large content', () => {
    // Content larger than budget
    const budget = calculateBudget('llama2-7b'); // 8k context
    const largeTokens = budget.totalTokens * 3;

    const chunks = recommendedChunkCount(largeTokens, 'llama2-7b');
    expect(chunks).toBeGreaterThan(1);
  });

  it('returns at least 1', () => {
    expect(recommendedChunkCount(0, 'gpt-4o-mini')).toBe(1);
  });
});
