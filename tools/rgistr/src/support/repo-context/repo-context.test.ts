/**
 * Unit tests for repo-context classification.
 */

import { describe, it, expect } from 'vitest';
import {
  extractPathSignals,
  classifyFolderContext,
  deriveRepoProfile,
  RepoProfile,
} from './index.js';

// ─────────────────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────────────────

const CODE_ANALYSIS_PROFILE: RepoProfile = {
  rootName: 'repo-graph',
  topLevelDirs: ['rust', 'tools', 'docs', 'test'],
  repoType: 'code_analysis_tooling',
  repoTypeReasons: ['name/description contains "graph"'],
  packageName: 'repo-graph',
  description: 'Deterministic code graph tool for AI agent consumption',
};

const LIBRARY_PROFILE: RepoProfile = {
  rootName: 'some-lib',
  topLevelDirs: ['src', 'test', 'examples'],
  repoType: 'library',
  repoTypeReasons: ['name/description contains "library"'],
  packageName: 'some-lib',
};

const UNKNOWN_PROFILE: RepoProfile = {
  rootName: 'mystery-project',
  topLevelDirs: ['stuff', 'things'],
  repoType: 'unknown',
  repoTypeReasons: ['no strong signals found'],
};

// ─────────────────────────────────────────────────────────────────────────────
// Path signal extraction tests
// ─────────────────────────────────────────────────────────────────────────────

describe('extractPathSignals', () => {
  it('identifies validation path segments', () => {
    const signals = extractPathSignals('smoke-runs/linux-inter-core-subset');
    expect(signals.segmentCategories).toContain('validation');
    expect(signals.segments).toEqual(['smoke-runs', 'linux-inter-core-subset']);
    expect(signals.depth).toBe(2);
  });

  it('identifies fixture path segments', () => {
    const signals = extractPathSignals('test/fixtures/nginx');
    expect(signals.segmentCategories).toContain('test');
    expect(signals.segmentCategories).toContain('fixture');
    expect(signals.looksLikeCopiedSourceTree).toBe(true);
  });

  it('identifies external/vendor path segments', () => {
    const signals = extractPathSignals('vendor/openssl');
    expect(signals.segmentCategories).toContain('external');
  });

  it('identifies source path segments', () => {
    const signals = extractPathSignals('tools/rgistr/src/support');
    expect(signals.segmentCategories).toContain('support');
    expect(signals.segmentCategories).toContain('source');
  });

  it('detects timestamped segments', () => {
    const signals1 = extractPathSignals('smoke-runs/2024-01-15/output');
    expect(signals1.hasTimestampedSegment).toBe(true);

    const signals2 = extractPathSignals('runs/run-123');
    expect(signals2.hasTimestampedSegment).toBe(true);
  });

  it('handles root path', () => {
    const signals = extractPathSignals('.');
    expect(signals.segments).toEqual([]);
    expect(signals.depth).toBe(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Folder classification tests
// ─────────────────────────────────────────────────────────────────────────────

describe('classifyFolderContext', () => {
  describe('validation corpus detection', () => {
    it('classifies smoke-runs/linux-inter-core-subset as validation_corpus', () => {
      const result = classifyFolderContext(
        'smoke-runs/linux-inter-core-subset',
        CODE_ANALYSIS_PROFILE,
        ['Linux kernel mailbox driver', 'interrupt handling', 'AMBA bus']
      );

      expect(result.repoContextClass).toBe('validation_corpus');
      expect(result.confidence).toBe('high');
      expect(result.reasons).toContain('path contains validation/corpus directory');
      expect(result.reasons).toContain('kernel driver code inside code-analysis tooling repo');
    });

    it('classifies smoke-runs root as validation_corpus', () => {
      const result = classifyFolderContext(
        'smoke-runs',
        CODE_ANALYSIS_PROFILE
      );

      expect(result.repoContextClass).toBe('validation_corpus');
      expectOneOf(result.confidence, ['high', 'medium']);
    });
  });

  describe('fixture storage detection', () => {
    it('classifies test/fixtures/nginx as fixture_storage or external_code_fixtures', () => {
      const result = classifyFolderContext(
        'test/fixtures/nginx',
        CODE_ANALYSIS_PROFILE
      );

      expect(['fixture_storage', 'external_code_fixtures']).toContain(result.repoContextClass);
    });

    it('classifies fixtures/ as fixture_storage', () => {
      const result = classifyFolderContext(
        'fixtures',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('fixture_storage');
    });
  });

  describe('external code fixtures detection', () => {
    it('classifies vendor/openssl as external_code_fixtures', () => {
      const result = classifyFolderContext(
        'vendor/openssl',
        CODE_ANALYSIS_PROFILE
      );

      expect(result.repoContextClass).toBe('external_code_fixtures');
      expect(result.reasons).toContain('path contains external/vendor directory');
    });

    it('classifies third-party/ as external_code_fixtures', () => {
      const result = classifyFolderContext(
        'third-party',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('external_code_fixtures');
    });
  });

  describe('product code detection', () => {
    it('classifies tools/rgistr/src/support as product_code', () => {
      const result = classifyFolderContext(
        'tools/rgistr/src/support',
        CODE_ANALYSIS_PROFILE
      );

      // This is owned support code, not fixture storage
      expect(result.repoContextClass).toBe('product_code');
    });

    it('classifies src/core as product_code', () => {
      const result = classifyFolderContext(
        'src/core',
        LIBRARY_PROFILE
      );

      expect(result.repoContextClass).toBe('product_code');
    });
  });

  describe('test support detection', () => {
    it('classifies __tests__ as test_support', () => {
      const result = classifyFolderContext(
        '__tests__',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('test_support');
    });

    it('classifies spec/ as test_support', () => {
      const result = classifyFolderContext(
        'spec',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('test_support');
    });
  });

  describe('artifact storage detection', () => {
    it('classifies dist/ as artifact_storage', () => {
      const result = classifyFolderContext(
        'dist',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('artifact_storage');
    });

    it('classifies coverage/ as artifact_storage', () => {
      const result = classifyFolderContext(
        'coverage',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('artifact_storage');
    });
  });

  describe('repo-type mismatch detection', () => {
    it('downgrades product_code when domain mismatches repo type', () => {
      const result = classifyFolderContext(
        'src/drivers',  // looks like source, but...
        CODE_ANALYSIS_PROFILE,
        ['Linux kernel driver', 'interrupt handler', 'DMA controller', 'mailbox']
      );

      // Even though path says src/, the content is clearly kernel code
      // which doesn't match code-analysis tooling
      expect(result.repoContextClass).not.toBe('product_code');
      expect(result.reasons).toContain('kernel driver code inside code-analysis tooling repo');
    });
  });

  describe('ambiguous paths', () => {
    it('returns unknown for neutral paths without signals', () => {
      const result = classifyFolderContext(
        'stuff/things',
        UNKNOWN_PROFILE
      );

      expect(result.repoContextClass).toBe('unknown');
      expect(result.confidence).toBe('low');
    });

    it('returns low confidence for weak signals', () => {
      const result = classifyFolderContext(
        'data',
        UNKNOWN_PROFILE
      );

      // 'data' has no strong category signal
      expectOneOf(result.confidence, ['low', 'medium']);
    });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Repo profile derivation tests (basic)
// ─────────────────────────────────────────────────────────────────────────────

describe('deriveRepoProfile', () => {
  it('derives profile from repo root (integration test - runs on actual repo-graph)', async () => {
    // Use path relative to this test file to find repo root
    // This test file is at: tools/rgistr/src/support/repo-context/repo-context.test.ts
    // __dirname = tools/rgistr/src/support/repo-context/
    // Repo root is 5 levels up: repo-context -> support -> src -> rgistr -> tools -> repo-graph
    const pathMod = await import('path');
    const url = await import('url');
    const __dirname = pathMod.dirname(url.fileURLToPath(import.meta.url));
    const repoRoot = pathMod.resolve(__dirname, '../../../../..');

    const profile = deriveRepoProfile(repoRoot);

    expect(profile.rootName).toBe('repo-graph');
    expect(profile.repoType).toBe('code_analysis_tooling');
    expect(profile.topLevelDirs).toContain('rust');
    expect(profile.topLevelDirs).toContain('tools');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Custom matcher for vitest
// ─────────────────────────────────────────────────────────────────────────────

// Helper for "one of" assertions
function expectOneOf<T>(actual: T, expected: T[]): void {
  expect(expected).toContain(actual);
}
