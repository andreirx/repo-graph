/**
 * Repo-context classification types.
 *
 * This module provides deterministic repo-context hints for folder synthesis.
 * It does NOT classify semantic roles like "orchestration/control" or "adapter/boundary".
 * Those remain LLM-level.
 *
 * This classifier only determines a folder's role in the repository structure:
 * - Is this product code owned by the repo?
 * - Is this test/support infrastructure?
 * - Is this external/copied code used for validation?
 * - Is this artifact storage from tool runs?
 */

/**
 * Coarse classification of a folder's role in the repository.
 *
 * - product_code: Owned code that implements repo functionality
 * - test_support: Test infrastructure, helpers, mocks
 * - artifact_storage: Generated outputs, reports, logs
 * - fixture_storage: Test inputs, sample data
 * - external_code_fixtures: Copied third-party code for validation/testing
 * - validation_corpus: Real-world codebases used to validate tooling
 * - unknown: Insufficient evidence for classification
 */
export type RepoContextClass =
  | 'product_code'
  | 'test_support'
  | 'artifact_storage'
  | 'fixture_storage'
  | 'external_code_fixtures'
  | 'validation_corpus'
  | 'unknown';

/**
 * Classification result with confidence and evidence.
 */
export interface RepoContextHint {
  repoContextClass: RepoContextClass;
  confidence: 'high' | 'medium' | 'low';
  reasons: string[];
}

/**
 * Coarse repo type derived from manifests and structure.
 * Used to detect domain mismatches (e.g., Linux drivers in a code-analysis repo).
 */
export type RepoType =
  | 'code_analysis_tooling'
  | 'library'
  | 'application'
  | 'infra_tooling'
  | 'operating_system'
  | 'embedded_firmware'
  | 'unknown';

/**
 * Repo-level profile built once per generation run.
 */
export interface RepoProfile {
  /** Root directory name */
  rootName: string;

  /** Top-level directory names */
  topLevelDirs: string[];

  /** Derived repo type hint */
  repoType: RepoType;

  /** Evidence supporting the repo type */
  repoTypeReasons: string[];

  /** Package name from manifest if available */
  packageName?: string;

  /** Repository description from manifest if available */
  description?: string;
}

/**
 * Path segment categories for classification signals.
 */
export type PathSegmentCategory =
  | 'test'           // test, tests, __tests__, spec, specs
  | 'fixture'        // fixture, fixtures, testdata, test-data
  | 'validation'     // validation, corpus, smoke, parity
  | 'artifact'       // output, dist, build, coverage, reports
  | 'external'       // vendor, third_party, external, deps
  | 'example'        // examples, demo, samples
  | 'support'        // scripts, tools, utils
  | 'source'         // src, lib, pkg, packages, crates
  | 'docs'           // docs, documentation
  | 'config'         // config, configs, .config
  | 'neutral';       // no strong signal

/**
 * Signals extracted from a folder path.
 */
export interface PathSignals {
  /** Categorized path segments */
  segmentCategories: PathSegmentCategory[];

  /** Raw path segments */
  segments: string[];

  /** Depth from repo root */
  depth: number;

  /** Whether path contains timestamped directories */
  hasTimestampedSegment: boolean;

  /** Whether path looks like copied source tree */
  looksLikeCopiedSourceTree: boolean;
}

/**
 * Artifact-shape signals from folder structure.
 */
export interface ArtifactShapeSignals {
  /** Contains smoke protocol files (00-meta.json, etc.) */
  hasSmokeProtocolFiles: boolean;

  /** Contains report/output JSON files */
  hasReportFiles: boolean;

  /** Contains mixed report + foreign source */
  hasMixedReportAndSource: boolean;

  /** File extensions present */
  extensions: string[];
}
