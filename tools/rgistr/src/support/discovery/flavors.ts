/**
 * Backend flavor profiles.
 *
 * Defines flavor-specific defaults and quirks for OpenAI-compatible backends.
 * These are configuration data, not transport logic.
 */

import type { BackendFlavor, FlavorProfile, PreferredModelAlias, TransportFamily } from './types.js';

// ── Flavor profiles ──────────────────────────────────────────────────

export const FLAVOR_PROFILES: Record<BackendFlavor, FlavorProfile> = {
  lmstudio: {
    flavor: 'lmstudio',
    label: 'LM Studio',
    defaultEndpoint: 'http://127.0.0.1:1234',
    modelsPath: '/v1/models',
    healthPath: '/v1/models',
    quirks: [
      'model_id_includes_path',
      'no_explicit_context_length_in_list',
    ],
  },

  mlx: {
    flavor: 'mlx',
    label: 'MLX Server',
    defaultEndpoint: 'http://127.0.0.1:8080',
    modelsPath: '/v1/models',
    healthPath: '/v1/models',
    quirks: [
      'single_model_typical',
      'apple_silicon_only',
    ],
  },

  llamacpp: {
    flavor: 'llamacpp',
    label: 'llama.cpp Server',
    defaultEndpoint: 'http://127.0.0.1:8080',
    modelsPath: '/v1/models',
    healthPath: '/health',
    quirks: [
      'health_endpoint_separate',
      'model_id_may_be_default',
    ],
  },

  generic: {
    flavor: 'generic',
    label: 'OpenAI-compatible',
    defaultEndpoint: 'http://127.0.0.1:8080',
    modelsPath: '/v1/models',
    healthPath: '/v1/models',
    quirks: [],
  },
};

// ── Preferred model aliases ──────────────────────────────────────────

/**
 * Preferred model aliases for ranking discovered models.
 *
 * Order matters: earlier entries have higher priority.
 */
export const PREFERRED_MODELS: PreferredModelAlias[] = [
  // Cloud preferences
  {
    alias: 'gpt-4.1-mini',
    patterns: ['gpt-4.1-mini', 'gpt-4-1-mini'],
    rank: 1,
    transports: ['openai_cloud'],
  },
  {
    alias: 'gpt-4o-mini',
    patterns: ['gpt-4o-mini'],
    rank: 2,
    transports: ['openai_cloud'],
  },

  // Local preferences - Qwen 3.6 family (large context, good reasoning)
  {
    alias: 'qwen3.6-35b',
    patterns: [
      'qwen3.6-35b',
      'qwen/qwen3.6-35b',
      'qwen3.6:35b',
      'Qwen3.6-35B',
    ],
    rank: 10,
    transports: ['openai_compatible', 'ollama'],
  },
  {
    alias: 'qwen3.6-27b',
    patterns: [
      'qwen3.6-27b',
      'qwen/qwen3.6-27b',
      'qwen3.6:27b',
      'Qwen3.6-27B',
    ],
    rank: 11,
    transports: ['openai_compatible', 'ollama'],
  },

  // Qwen 2.5 family (fallback)
  {
    alias: 'qwen2.5-coder-32b',
    patterns: [
      'qwen2.5-coder-32b',
      'qwen2.5-coder:32b',
      'Qwen2.5-Coder-32B',
    ],
    rank: 20,
    transports: ['openai_compatible', 'ollama'],
  },

  // DeepSeek family
  {
    alias: 'deepseek-coder-v2',
    patterns: [
      'deepseek-coder-v2',
      'deepseek-coder:v2',
      'DeepSeek-Coder-V2',
    ],
    rank: 30,
    transports: ['openai_compatible', 'ollama'],
  },

  // Llama family (general fallback)
  {
    alias: 'llama3.3-70b',
    patterns: [
      'llama3.3-70b',
      'llama-3.3-70b',
      'llama3.3:70b',
    ],
    rank: 40,
    transports: ['openai_compatible', 'ollama'],
  },
];

// ── Default probe candidates ─────────────────────────────────────────

/**
 * Default endpoints to probe for each transport family.
 *
 * These are probed in order unless overridden by config.
 */
export const DEFAULT_PROBE_ENDPOINTS: Array<{
  transport: TransportFamily;
  flavor: BackendFlavor | null;
  endpoint: string;
  priority: number;
}> = [
  // OpenAI cloud (only if API key present)
  {
    transport: 'openai_cloud',
    flavor: null,
    endpoint: 'https://api.openai.com/v1',
    priority: 1,
  },

  // LM Studio default
  {
    transport: 'openai_compatible',
    flavor: 'lmstudio',
    endpoint: 'http://127.0.0.1:1234',
    priority: 10,
  },

  // MLX server default
  {
    transport: 'openai_compatible',
    flavor: 'mlx',
    endpoint: 'http://127.0.0.1:8080',
    priority: 11,
  },

  // llama.cpp alternative port
  {
    transport: 'openai_compatible',
    flavor: 'llamacpp',
    endpoint: 'http://127.0.0.1:8081',
    priority: 12,
  },

  // Ollama default
  {
    transport: 'ollama',
    flavor: null,
    endpoint: 'http://127.0.0.1:11434',
    priority: 20,
  },
];

// ── Helper functions ─────────────────────────────────────────────────

/**
 * Get flavor profile by flavor identifier.
 */
export function getFlavorProfile(flavor: BackendFlavor): FlavorProfile {
  return FLAVOR_PROFILES[flavor];
}

/**
 * Check if a model ID matches any preferred alias.
 *
 * Returns the matched alias and rank, or null if no match.
 */
export function matchPreferredModel(
  modelId: string,
  transport: TransportFamily
): { alias: string; rank: number } | null {
  const normalizedId = modelId.toLowerCase();

  for (const pref of PREFERRED_MODELS) {
    if (!pref.transports.includes(transport)) {
      continue;
    }

    for (const pattern of pref.patterns) {
      if (normalizedId.includes(pattern.toLowerCase())) {
        return { alias: pref.alias, rank: pref.rank };
      }
    }
  }

  return null;
}
