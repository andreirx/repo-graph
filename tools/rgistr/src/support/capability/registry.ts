/**
 * Model capability registry.
 *
 * Known capabilities for common models. Used when backend does not
 * report capabilities at runtime.
 */

import type { ModelCapability } from './types.js';

// ── Known model capabilities ─────────────────────────────────────────

/**
 * Registry of known model capabilities.
 *
 * Ordered by specificity - more specific patterns first.
 */
export const MODEL_CAPABILITIES: ModelCapability[] = [
  // ── OpenAI Cloud ───────────────────────────────────────────────────

  {
    pattern: 'gpt-4.1-mini',
    family: 'GPT-4.1',
    maxInputTokens: 1000000,
    maxOutputTokens: 32768,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_cloud'],
    source: 'hardcoded',
    notes: 'GPT-4.1 mini with 1M context',
  },
  {
    pattern: 'gpt-4o-mini',
    family: 'GPT-4o',
    maxInputTokens: 128000,
    maxOutputTokens: 16384,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_cloud'],
    source: 'hardcoded',
  },
  {
    pattern: 'gpt-4o',
    family: 'GPT-4o',
    maxInputTokens: 128000,
    maxOutputTokens: 16384,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_cloud'],
    source: 'hardcoded',
  },
  {
    pattern: 'gpt-4-turbo',
    family: 'GPT-4 Turbo',
    maxInputTokens: 128000,
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_cloud'],
    source: 'hardcoded',
  },

  // ── Qwen 3.6 family ────────────────────────────────────────────────

  {
    pattern: 'qwen3.6',
    family: 'Qwen 3.6',
    maxInputTokens: 262144, // 256k
    maxOutputTokens: 8192,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
    notes: 'Qwen 3.6 series with 256k context',
  },

  // ── Qwen 2.5 family ────────────────────────────────────────────────

  {
    pattern: 'qwen2.5-coder',
    family: 'Qwen 2.5 Coder',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 8192,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'qwen2.5',
    family: 'Qwen 2.5',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 8192,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },

  // ── DeepSeek family ────────────────────────────────────────────────

  {
    pattern: 'deepseek-coder-v2',
    family: 'DeepSeek Coder V2',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 8192,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'deepseek',
    family: 'DeepSeek',
    maxInputTokens: 65536, // 64k default
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },

  // ── Llama family ───────────────────────────────────────────────────

  {
    pattern: 'llama3.3',
    family: 'Llama 3.3',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'llama3.2',
    family: 'Llama 3.2',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'llama3.1',
    family: 'Llama 3.1',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'llama',
    family: 'Llama',
    maxInputTokens: 8192, // Conservative default for older versions
    maxOutputTokens: 2048,
    supportsJsonMode: false,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },

  // ── Mistral family ─────────────────────────────────────────────────

  {
    pattern: 'mistral-large',
    family: 'Mistral Large',
    maxInputTokens: 131072, // 128k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: true,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'mixtral',
    family: 'Mixtral',
    maxInputTokens: 32768, // 32k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },
  {
    pattern: 'mistral',
    family: 'Mistral',
    maxInputTokens: 32768, // 32k
    maxOutputTokens: 4096,
    supportsJsonMode: true,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama'],
    source: 'hardcoded',
  },

  // ── Fallback ───────────────────────────────────────────────────────

  {
    pattern: '*',
    family: 'Unknown',
    maxInputTokens: 8192, // Very conservative
    maxOutputTokens: 2048,
    supportsJsonMode: false,
    supportsStreaming: true,
    supportsTools: false,
    transports: ['openai_compatible', 'ollama', 'openai_cloud'],
    source: 'hardcoded',
    notes: 'Fallback for unknown models - very conservative limits',
  },
];

// ── Lookup functions ─────────────────────────────────────────────────

/**
 * Look up capabilities for a model by ID.
 *
 * Returns the first matching capability entry.
 */
export function getModelCapability(modelId: string): ModelCapability {
  const normalizedId = modelId.toLowerCase();

  for (const cap of MODEL_CAPABILITIES) {
    if (cap.pattern === '*') continue; // Skip fallback in first pass

    if (normalizedId.includes(cap.pattern.toLowerCase())) {
      return cap;
    }
  }

  // Return fallback
  return MODEL_CAPABILITIES[MODEL_CAPABILITIES.length - 1];
}

/**
 * Check if a model supports JSON mode.
 */
export function supportsJsonMode(modelId: string): boolean {
  return getModelCapability(modelId).supportsJsonMode;
}

/**
 * Get maximum input tokens for a model.
 */
export function getMaxInputTokens(modelId: string): number {
  return getModelCapability(modelId).maxInputTokens;
}

/**
 * Get maximum output tokens for a model.
 */
export function getMaxOutputTokens(modelId: string): number {
  return getModelCapability(modelId).maxOutputTokens;
}
