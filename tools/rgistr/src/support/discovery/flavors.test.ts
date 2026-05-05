/**
 * Unit tests for flavor profiles and preferred models.
 */

import { describe, it, expect } from 'vitest';
import {
  FLAVOR_PROFILES,
  PREFERRED_MODELS,
  getFlavorProfile,
  matchPreferredModel,
} from './flavors.js';

describe('FLAVOR_PROFILES', () => {
  it('has all expected flavors', () => {
    expect(FLAVOR_PROFILES.lmstudio).toBeDefined();
    expect(FLAVOR_PROFILES.mlx).toBeDefined();
    expect(FLAVOR_PROFILES.llamacpp).toBeDefined();
    expect(FLAVOR_PROFILES.generic).toBeDefined();
  });

  it('lmstudio has correct defaults', () => {
    const profile = FLAVOR_PROFILES.lmstudio;
    expect(profile.defaultEndpoint).toBe('http://127.0.0.1:1234');
    expect(profile.modelsPath).toBe('/v1/models');
    expect(profile.label).toBe('LM Studio');
  });

  it('ollama endpoint is different from openai-compatible', () => {
    // Ollama uses a different API, so it's not in FLAVOR_PROFILES
    // But the default probe endpoint should be different
    expect(FLAVOR_PROFILES.lmstudio.defaultEndpoint).not.toBe('http://127.0.0.1:11434');
  });
});

describe('getFlavorProfile', () => {
  it('returns correct profile for known flavor', () => {
    const profile = getFlavorProfile('lmstudio');
    expect(profile.flavor).toBe('lmstudio');
    expect(profile.label).toBe('LM Studio');
  });

  it('returns generic profile for generic flavor', () => {
    const profile = getFlavorProfile('generic');
    expect(profile.flavor).toBe('generic');
    expect(profile.label).toBe('OpenAI-compatible');
  });
});

describe('PREFERRED_MODELS', () => {
  it('has cloud and local preferences', () => {
    const cloudModels = PREFERRED_MODELS.filter(m => m.transports.includes('openai_cloud'));
    const localModels = PREFERRED_MODELS.filter(m => m.transports.includes('openai_compatible'));

    expect(cloudModels.length).toBeGreaterThan(0);
    expect(localModels.length).toBeGreaterThan(0);
  });

  it('cloud models have lower ranks than local', () => {
    const cloudModels = PREFERRED_MODELS.filter(m => m.transports.includes('openai_cloud'));
    const localModels = PREFERRED_MODELS.filter(m =>
      m.transports.includes('openai_compatible') && !m.transports.includes('openai_cloud')
    );

    const minCloudRank = Math.min(...cloudModels.map(m => m.rank));
    const minLocalRank = Math.min(...localModels.map(m => m.rank));

    expect(minCloudRank).toBeLessThan(minLocalRank);
  });
});

describe('matchPreferredModel', () => {
  it('matches gpt-4.1-mini for cloud', () => {
    const match = matchPreferredModel('gpt-4.1-mini', 'openai_cloud');
    expect(match).not.toBeNull();
    expect(match!.alias).toBe('gpt-4.1-mini');
    expect(match!.rank).toBe(1);
  });

  it('matches qwen3.6 for local', () => {
    const match = matchPreferredModel('qwen3.6-35b-instruct', 'openai_compatible');
    expect(match).not.toBeNull();
    expect(match!.alias).toBe('qwen3.6-35b');
  });

  it('matches case-insensitively', () => {
    const match = matchPreferredModel('GPT-4.1-MINI', 'openai_cloud');
    expect(match).not.toBeNull();
    expect(match!.alias).toBe('gpt-4.1-mini');
  });

  it('returns null for unknown model', () => {
    const match = matchPreferredModel('some-unknown-model', 'openai_compatible');
    expect(match).toBeNull();
  });

  it('respects transport filter', () => {
    // gpt-4.1-mini should not match for local transport
    const match = matchPreferredModel('gpt-4.1-mini', 'openai_compatible');
    expect(match).toBeNull();
  });
});
