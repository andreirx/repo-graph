/**
 * Unit tests for model capability registry.
 */

import { describe, it, expect } from 'vitest';
import {
  MODEL_CAPABILITIES,
  getModelCapability,
  supportsJsonMode,
  getMaxInputTokens,
  getMaxOutputTokens,
} from './registry.js';

describe('MODEL_CAPABILITIES', () => {
  it('has fallback entry', () => {
    const fallback = MODEL_CAPABILITIES.find(c => c.pattern === '*');
    expect(fallback).toBeDefined();
    expect(fallback!.family).toBe('Unknown');
  });

  it('has cloud models', () => {
    const cloudModels = MODEL_CAPABILITIES.filter(c =>
      c.transports.includes('openai_cloud')
    );
    expect(cloudModels.length).toBeGreaterThan(0);
  });

  it('has local models', () => {
    const localModels = MODEL_CAPABILITIES.filter(c =>
      c.transports.includes('openai_compatible')
    );
    expect(localModels.length).toBeGreaterThan(0);
  });
});

describe('getModelCapability', () => {
  it('returns GPT-4.1-mini capability', () => {
    const cap = getModelCapability('gpt-4.1-mini');
    expect(cap.family).toBe('GPT-4.1');
    expect(cap.maxInputTokens).toBe(1000000);
    expect(cap.supportsJsonMode).toBe(true);
  });

  it('returns Qwen 3.6 capability', () => {
    const cap = getModelCapability('qwen3.6-35b-instruct');
    expect(cap.family).toBe('Qwen 3.6');
    expect(cap.maxInputTokens).toBe(262144);
  });

  it('returns Llama 3.3 capability', () => {
    const cap = getModelCapability('llama3.3-70b');
    expect(cap.family).toBe('Llama 3.3');
    expect(cap.maxInputTokens).toBe(131072);
    expect(cap.supportsTools).toBe(true);
  });

  it('returns fallback for unknown model', () => {
    const cap = getModelCapability('some-totally-unknown-model-xyz');
    expect(cap.family).toBe('Unknown');
    expect(cap.maxInputTokens).toBe(8192); // Conservative fallback
  });

  it('matches case-insensitively', () => {
    const cap1 = getModelCapability('GPT-4O-MINI');
    const cap2 = getModelCapability('gpt-4o-mini');
    expect(cap1.family).toBe(cap2.family);
  });
});

describe('supportsJsonMode', () => {
  it('returns true for GPT-4o-mini', () => {
    expect(supportsJsonMode('gpt-4o-mini')).toBe(true);
  });

  it('returns true for Qwen models', () => {
    expect(supportsJsonMode('qwen2.5-coder-32b')).toBe(true);
  });

  it('returns false for old Llama', () => {
    expect(supportsJsonMode('llama2')).toBe(false);
  });

  it('returns false for unknown models', () => {
    expect(supportsJsonMode('unknown-model')).toBe(false);
  });
});

describe('getMaxInputTokens', () => {
  it('returns correct value for GPT-4.1-mini', () => {
    expect(getMaxInputTokens('gpt-4.1-mini')).toBe(1000000);
  });

  it('returns correct value for GPT-4o-mini', () => {
    expect(getMaxInputTokens('gpt-4o-mini')).toBe(128000);
  });

  it('returns correct value for Qwen 3.6', () => {
    expect(getMaxInputTokens('qwen3.6-27b')).toBe(262144);
  });

  it('returns conservative value for unknown', () => {
    expect(getMaxInputTokens('unknown-model')).toBe(8192);
  });
});

describe('getMaxOutputTokens', () => {
  it('returns correct value for GPT-4.1-mini', () => {
    expect(getMaxOutputTokens('gpt-4.1-mini')).toBe(32768);
  });

  it('returns correct value for Llama models', () => {
    expect(getMaxOutputTokens('llama3.3-70b')).toBe(4096);
  });
});
