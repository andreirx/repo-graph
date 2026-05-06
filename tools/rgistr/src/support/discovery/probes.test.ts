/**
 * Discovery normalization tests.
 *
 * Verifies that all transport families (OpenAI cloud, OpenAI-compatible, Ollama)
 * produce normalized DTO output. Also verifies that preferred ranking is advisory
 * only and no silent execution defaults leak through.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { ProviderCandidate, ProbeResult, DiscoveredModel } from './types.js';
import { probeCandidate } from './probes.js';

// ══════════════════════════════════════════════════════════════════════════════
// Mock fetch for controlled probe testing
// ══════════════════════════════════════════════════════════════════════════════

const mockFetch = vi.fn();

beforeEach(() => {
  vi.stubGlobal('fetch', mockFetch);
  mockFetch.mockReset();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// ══════════════════════════════════════════════════════════════════════════════
// Test fixtures: mock responses for each transport family
// ══════════════════════════════════════════════════════════════════════════════

const MOCK_OPENAI_CLOUD_RESPONSE = {
  data: [
    { id: 'gpt-4.1-mini', object: 'model', owned_by: 'openai' },
    { id: 'gpt-4o', object: 'model', owned_by: 'openai' },
    { id: 'text-embedding-3-small', object: 'model', owned_by: 'openai' },
  ],
};

const MOCK_LMSTUDIO_RESPONSE = {
  data: [
    { id: 'lmstudio-community/qwen3.6-35b-a3b', object: 'model' },
    { id: 'local-model', object: 'model' },
    { id: 'llama-3-8b-instruct', object: 'model' },
  ],
};

const MOCK_OLLAMA_RESPONSE = {
  models: [
    { name: 'llama3.2:3b', model: 'llama3.2:3b', modified_at: '2025-01-01T00:00:00Z', size: 2000000000 },
    { name: 'qwen3.6:27b', model: 'qwen3.6:27b', modified_at: '2025-01-02T00:00:00Z', size: 15000000000 },
    { name: 'mistral:7b', model: 'mistral:7b', modified_at: '2025-01-03T00:00:00Z', size: 4000000000 },
  ],
};

// ══════════════════════════════════════════════════════════════════════════════
// DTO normalization tests
// ══════════════════════════════════════════════════════════════════════════════

describe('ProbeResult DTO normalization', () => {
  it('OpenAI cloud produces normalized ProbeResult', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OPENAI_CLOUD_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    const result = await probeCandidate(candidate, { apiKey: 'sk-test' });

    // Verify normalized DTO shape
    expect(result.candidate).toBe(candidate);
    expect(typeof result.success).toBe('boolean');
    expect(result.success).toBe(true);
    expect(Array.isArray(result.models)).toBe(true);
    expect(typeof result.latencyMs).toBe('number');
    expect(result.error).toBeUndefined();
  });

  it('OpenAI-compatible (LM Studio) produces normalized ProbeResult', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_LMSTUDIO_RESPONSE,
      headers: new Map([['server', 'LM Studio 0.3.0']]),
    });

    const candidate: ProviderCandidate = {
      id: 'lmstudio-1',
      transport: 'openai_compatible',
      flavor: 'lmstudio',
      endpoint: 'http://127.0.0.1:1234',  // Base endpoint without /v1 suffix
      label: 'LM Studio',
      source: 'default',
      priority: 10,
    };

    const result = await probeCandidate(candidate, {});

    // Verify normalized DTO shape
    expect(result.candidate).toBe(candidate);
    expect(typeof result.success).toBe('boolean');
    expect(result.success).toBe(true);
    expect(Array.isArray(result.models)).toBe(true);
    expect(typeof result.latencyMs).toBe('number');
    expect(result.error).toBeUndefined();
  });

  it('Ollama produces normalized ProbeResult', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OLLAMA_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'ollama-1',
      transport: 'ollama',
      flavor: null,
      endpoint: 'http://127.0.0.1:11434',
      label: 'Ollama',
      source: 'default',
      priority: 20,
    };

    const result = await probeCandidate(candidate, {});

    // Verify normalized DTO shape
    expect(result.candidate).toBe(candidate);
    expect(typeof result.success).toBe('boolean');
    expect(result.success).toBe(true);
    expect(Array.isArray(result.models)).toBe(true);
    expect(typeof result.latencyMs).toBe('number');
    expect(result.error).toBeUndefined();
  });

  it('all transports produce same DiscoveredModel shape', async () => {
    // OpenAI cloud
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OPENAI_CLOUD_RESPONSE,
    });

    const cloudCandidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    const cloudResult = await probeCandidate(cloudCandidate, { apiKey: 'sk-test' });

    // LM Studio
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_LMSTUDIO_RESPONSE,
      headers: new Map(),
    });

    const lmstudioCandidate: ProviderCandidate = {
      id: 'lmstudio-1',
      transport: 'openai_compatible',
      flavor: 'lmstudio',
      endpoint: 'http://127.0.0.1:1234',  // Base endpoint
      label: 'LM Studio',
      source: 'default',
      priority: 10,
    };

    const lmstudioResult = await probeCandidate(lmstudioCandidate, {});

    // Ollama
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OLLAMA_RESPONSE,
    });

    const ollamaCandidate: ProviderCandidate = {
      id: 'ollama-1',
      transport: 'ollama',
      flavor: null,
      endpoint: 'http://127.0.0.1:11434',
      label: 'Ollama',
      source: 'default',
      priority: 20,
    };

    const ollamaResult = await probeCandidate(ollamaCandidate, {});

    // Verify all three have models with same shape
    const allModels = [
      ...cloudResult.models,
      ...lmstudioResult.models,
      ...ollamaResult.models,
    ];

    for (const model of allModels) {
      expect(typeof model.id).toBe('string');
      expect(typeof model.name).toBe('string');
      expect(typeof model.isPreferred).toBe('boolean');
      expect(model.preferenceRank === null || typeof model.preferenceRank === 'number').toBe(true);
    }
  });
});

// ══════════════════════════════════════════════════════════════════════════════
// Flavor metadata tests
// ══════════════════════════════════════════════════════════════════════════════

describe('flavor metadata correctness', () => {
  it('LM Studio flavor uses correct endpoint paths', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_LMSTUDIO_RESPONSE,
      headers: new Map(),
    });

    const candidate: ProviderCandidate = {
      id: 'lmstudio-1',
      transport: 'openai_compatible',
      flavor: 'lmstudio',
      endpoint: 'http://127.0.0.1:1234',  // Base endpoint
      label: 'LM Studio',
      source: 'default',
      priority: 10,
    };

    await probeCandidate(candidate, {});

    // Verify correct endpoint was called (base + modelsPath from flavor profile)
    expect(mockFetch).toHaveBeenCalledWith(
      'http://127.0.0.1:1234/v1/models',
      expect.any(Object)
    );
  });

  it('Ollama uses api/tags endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OLLAMA_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'ollama-1',
      transport: 'ollama',
      flavor: null,
      endpoint: 'http://127.0.0.1:11434',
      label: 'Ollama',
      source: 'default',
      priority: 20,
    };

    await probeCandidate(candidate, {});

    expect(mockFetch).toHaveBeenCalledWith(
      'http://127.0.0.1:11434/api/tags',
      expect.any(Object)
    );
  });

  it('OpenAI cloud uses /models endpoint with auth header', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OPENAI_CLOUD_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    await probeCandidate(candidate, { apiKey: 'sk-testkey123' });

    expect(mockFetch).toHaveBeenCalledWith(
      'https://api.openai.com/v1/models',
      expect.objectContaining({
        headers: expect.objectContaining({
          'Authorization': 'Bearer sk-testkey123',
        }),
      })
    );
  });
});

// ══════════════════════════════════════════════════════════════════════════════
// Preferred ranking is advisory only
// ══════════════════════════════════════════════════════════════════════════════

describe('preferred ranking is advisory only', () => {
  it('preferred models are marked but not auto-selected', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OPENAI_CLOUD_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    const result = await probeCandidate(candidate, { apiKey: 'sk-test' });

    // Find preferred model
    const preferred = result.models.find(m => m.isPreferred);
    expect(preferred).toBeDefined();

    // Verify it's marked but result has no "selected" or "default" field
    expect(preferred!.isPreferred).toBe(true);
    expect(preferred!.preferenceRank).not.toBeNull();

    // ProbeResult should NOT have any auto-selection field
    expect((result as any).selectedModel).toBeUndefined();
    expect((result as any).defaultModel).toBeUndefined();
    expect((result as any).autoSelected).toBeUndefined();
  });

  it('non-preferred models have null preferenceRank', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        data: [
          { id: 'unknown-model-xyz', object: 'model' },
        ],
      }),
    });

    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    const result = await probeCandidate(candidate, { apiKey: 'sk-test' });

    expect(result.models.length).toBe(1);
    expect(result.models[0].isPreferred).toBe(false);
    expect(result.models[0].preferenceRank).toBeNull();
  });

  it('all discovered models are returned, not just preferred', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => MOCK_OPENAI_CLOUD_RESPONSE,
    });

    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    const result = await probeCandidate(candidate, { apiKey: 'sk-test' });

    // All three models should be present
    expect(result.models.length).toBe(3);

    const ids = result.models.map(m => m.id);
    expect(ids).toContain('gpt-4.1-mini');
    expect(ids).toContain('gpt-4o');
    expect(ids).toContain('text-embedding-3-small');
  });
});

// ══════════════════════════════════════════════════════════════════════════════
// No silent execution defaults
// ══════════════════════════════════════════════════════════════════════════════

describe('no silent execution defaults', () => {
  it('OpenAI cloud fails without API key (no fallback)', async () => {
    const candidate: ProviderCandidate = {
      id: 'openai-1',
      transport: 'openai_cloud',
      flavor: null,
      endpoint: 'https://api.openai.com/v1',
      label: 'OpenAI',
      source: 'env',
      priority: 1,
    };

    // No API key provided
    const result = await probeCandidate(candidate, {});

    expect(result.success).toBe(false);
    expect(result.error).toContain('OPENAI_API_KEY');
  });

  it('failed probe returns empty models array', async () => {
    mockFetch.mockRejectedValueOnce(new Error('Connection refused'));

    const candidate: ProviderCandidate = {
      id: 'lmstudio-1',
      transport: 'openai_compatible',
      flavor: 'lmstudio',
      endpoint: 'http://127.0.0.1:1234',  // Base endpoint
      label: 'LM Studio',
      source: 'default',
      priority: 10,
    };

    const result = await probeCandidate(candidate, {});

    expect(result.success).toBe(false);
    expect(result.models).toEqual([]);
    expect(result.error).toBeDefined();
  });

  it('HTTP error returns failure with error message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
    });

    const candidate: ProviderCandidate = {
      id: 'ollama-1',
      transport: 'ollama',
      flavor: null,
      endpoint: 'http://127.0.0.1:11434',
      label: 'Ollama',
      source: 'default',
      priority: 20,
    };

    const result = await probeCandidate(candidate, {});

    expect(result.success).toBe(false);
    expect(result.error).toContain('500');
    expect(result.models).toEqual([]);
  });

  it('unknown transport fails explicitly', async () => {
    const candidate: ProviderCandidate = {
      id: 'unknown-1',
      transport: 'some_future_transport' as any,
      flavor: null,
      endpoint: 'http://example.com',
      label: 'Unknown',
      source: 'config',
      priority: 100,
    };

    const result = await probeCandidate(candidate, {});

    expect(result.success).toBe(false);
    expect(result.error).toContain('Unknown transport');
  });
});
