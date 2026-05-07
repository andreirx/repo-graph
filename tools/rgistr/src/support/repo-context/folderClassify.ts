/**
 * Folder repo-context classification.
 *
 * Determines a folder's role in the repository using weighted evidence:
 * - Path taxonomy signals
 * - Artifact-shape signals
 * - Repo-type mismatch detection
 *
 * This is NOT semantic role classification (orchestration/control, adapter/boundary).
 * This only determines: product code vs test support vs external fixtures vs storage.
 */

import * as path from 'path';
import {
  RepoContextClass,
  RepoContextHint,
  RepoProfile,
  PathSegmentCategory,
  PathSignals,
} from './types.js';

// ─────────────────────────────────────────────────────────────────────────────
// Path Segment Classification
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Segment patterns mapped to categories.
 * Order matters for some ambiguous cases.
 */
const SEGMENT_PATTERNS: Array<{ pattern: RegExp; category: PathSegmentCategory }> = [
  // Test infrastructure
  { pattern: /^(__)?tests?(__)?$/i, category: 'test' },
  { pattern: /^specs?$/i, category: 'test' },
  { pattern: /^__mocks__$/i, category: 'test' },

  // Fixtures and test data
  { pattern: /^fixtures?$/i, category: 'fixture' },
  { pattern: /^test[-_]?data$/i, category: 'fixture' },
  { pattern: /^testdata$/i, category: 'fixture' },
  { pattern: /^mocks?$/i, category: 'fixture' },
  { pattern: /^stubs?$/i, category: 'fixture' },

  // Validation and corpus
  { pattern: /^validation$/i, category: 'validation' },
  { pattern: /^corpus$/i, category: 'validation' },
  { pattern: /^smoke[-_]?runs?$/i, category: 'validation' },
  { pattern: /^smoke[-_]?test$/i, category: 'validation' },
  { pattern: /^parity[-_]?fixtures?$/i, category: 'validation' },
  { pattern: /^parity[-_]?test$/i, category: 'validation' },
  { pattern: /^reference[-_]?repos?$/i, category: 'validation' },
  { pattern: /^legacy[-_]?codebases?$/i, category: 'validation' },

  // Artifacts and outputs
  { pattern: /^dist$/i, category: 'artifact' },
  { pattern: /^build$/i, category: 'artifact' },
  { pattern: /^output$/i, category: 'artifact' },
  { pattern: /^out$/i, category: 'artifact' },
  { pattern: /^coverage$/i, category: 'artifact' },
  { pattern: /^reports?$/i, category: 'artifact' },
  { pattern: /^logs?$/i, category: 'artifact' },
  { pattern: /^\.cache$/i, category: 'artifact' },
  { pattern: /^node_modules$/i, category: 'artifact' },
  { pattern: /^target$/i, category: 'artifact' },

  // External/vendor code
  { pattern: /^vendor$/i, category: 'external' },
  { pattern: /^vendors?$/i, category: 'external' },
  { pattern: /^third[-_]?party$/i, category: 'external' },
  { pattern: /^external$/i, category: 'external' },
  { pattern: /^deps$/i, category: 'external' },
  { pattern: /^dependencies$/i, category: 'external' },

  // Examples and demos
  { pattern: /^examples?$/i, category: 'example' },
  { pattern: /^demos?$/i, category: 'example' },
  { pattern: /^samples?$/i, category: 'example' },
  { pattern: /^tutorials?$/i, category: 'example' },

  // Support/tooling
  { pattern: /^scripts?$/i, category: 'support' },
  { pattern: /^tools?$/i, category: 'support' },
  { pattern: /^utils?$/i, category: 'support' },
  { pattern: /^bin$/i, category: 'support' },
  { pattern: /^cli$/i, category: 'support' },

  // Source code
  { pattern: /^src$/i, category: 'source' },
  { pattern: /^lib$/i, category: 'source' },
  { pattern: /^pkg$/i, category: 'source' },
  { pattern: /^packages?$/i, category: 'source' },
  { pattern: /^crates?$/i, category: 'source' },
  { pattern: /^core$/i, category: 'source' },
  { pattern: /^internal$/i, category: 'source' },

  // Documentation
  { pattern: /^docs?$/i, category: 'docs' },
  { pattern: /^documentation$/i, category: 'docs' },

  // Config
  { pattern: /^\.?configs?$/i, category: 'config' },
  { pattern: /^settings?$/i, category: 'config' },
];

/**
 * Classify a single path segment.
 */
function classifySegment(segment: string): PathSegmentCategory {
  for (const { pattern, category } of SEGMENT_PATTERNS) {
    if (pattern.test(segment)) {
      return category;
    }
  }
  return 'neutral';
}

/**
 * Detect timestamped directory patterns.
 */
function isTimestampedSegment(segment: string): boolean {
  // Common timestamp patterns: 2024-01-15, 20240115, 1705334400, etc.
  return /^\d{4}[-_]?\d{2}[-_]?\d{2}/.test(segment) ||
         /^\d{10,13}$/.test(segment) ||
         /^run[-_]?\d+$/.test(segment);
}

/**
 * Detect copied source tree patterns.
 */
function looksLikeCopiedSourceTree(segments: string[]): boolean {
  // If path contains validation/corpus segment followed by what looks like
  // a project name (single word, lowercase, common project names)
  const validationIdx = segments.findIndex(s =>
    classifySegment(s) === 'validation' ||
    classifySegment(s) === 'fixture' ||
    classifySegment(s) === 'external'
  );

  if (validationIdx >= 0 && validationIdx < segments.length - 1) {
    const next = segments[validationIdx + 1];
    // Common copied project names
    if (/^(linux|nginx|sqlite|redis|postgres|openssl|curl|swupdate|petclinic)$/i.test(next)) {
      return true;
    }
    // Or any single-word name after a validation segment
    if (/^[a-z][\w-]*$/i.test(next) && next.length > 2) {
      return true;
    }
  }
  return false;
}

/**
 * Extract path signals from a repo-relative folder path.
 */
export function extractPathSignals(folderPath: string): PathSignals {
  const segments = folderPath.split(path.sep).filter(s => s && s !== '.');
  const segmentCategories = segments.map(classifySegment);

  return {
    segments,
    segmentCategories,
    depth: segments.length,
    hasTimestampedSegment: segments.some(isTimestampedSegment),
    looksLikeCopiedSourceTree: looksLikeCopiedSourceTree(segments),
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Folder Classification
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Domain keywords that indicate specific code domains.
 * Used for repo-type mismatch detection.
 */
const DOMAIN_KEYWORDS: Record<string, string[]> = {
  kernel: ['kernel', 'driver', 'interrupt', 'irq', 'dma', 'mailbox', 'amba', 'soc', 'mmio'],
  web_framework: ['react', 'vue', 'angular', 'express', 'fastify', 'django', 'flask', 'rails'],
  database: ['sql', 'postgres', 'mysql', 'mongo', 'redis', 'query', 'btree', 'wal'],
  crypto: ['aes', 'rsa', 'sha', 'hmac', 'cipher', 'encrypt', 'decrypt', 'tls', 'ssl'],
  network: ['tcp', 'udp', 'http', 'socket', 'dns', 'dhcp', 'icmp', 'packet'],
};

/**
 * Detect if child summaries indicate a specific code domain.
 */
function detectCodeDomain(childSummaries: string[]): string | null {
  const combined = childSummaries.join(' ').toLowerCase();

  for (const [domain, keywords] of Object.entries(DOMAIN_KEYWORDS)) {
    const matchCount = keywords.filter(kw => combined.includes(kw)).length;
    if (matchCount >= 2) {
      return domain;
    }
  }
  return null;
}

/**
 * Check if repo type and code domain are mismatched.
 */
function isRepoTypeMismatch(
  repoProfile: RepoProfile,
  codeDomain: string | null
): { mismatch: boolean; reason: string } {
  if (!codeDomain) {
    return { mismatch: false, reason: '' };
  }

  // Code-analysis tooling should not contain kernel drivers as product code
  if (repoProfile.repoType === 'code_analysis_tooling') {
    if (codeDomain === 'kernel') {
      return {
        mismatch: true,
        reason: 'kernel driver code inside code-analysis tooling repo',
      };
    }
    if (codeDomain === 'web_framework') {
      return {
        mismatch: true,
        reason: 'web framework code inside code-analysis tooling repo',
      };
    }
    if (codeDomain === 'database') {
      return {
        mismatch: true,
        reason: 'database implementation inside code-analysis tooling repo',
      };
    }
  }

  // Library repos typically don't contain full OS kernels
  if (repoProfile.repoType === 'library' && codeDomain === 'kernel') {
    return {
      mismatch: true,
      reason: 'kernel driver code inside library repo',
    };
  }

  return { mismatch: false, reason: '' };
}

/**
 * Evidence weights for classification.
 */
interface ClassificationEvidence {
  productCode: number;
  testSupport: number;
  artifactStorage: number;
  fixtureStorage: number;
  externalCodeFixtures: number;
  validationCorpus: number;
}

/**
 * Classify a folder's repo-context role.
 *
 * @param folderPath - Repo-relative path to the folder
 * @param repoProfile - Repo-level profile
 * @param childSummaries - Optional child file/folder summaries for domain detection
 */
export function classifyFolderContext(
  folderPath: string,
  repoProfile: RepoProfile,
  childSummaries: string[] = []
): RepoContextHint {
  const pathSignals = extractPathSignals(folderPath);
  const reasons: string[] = [];

  // Initialize evidence weights
  const evidence: ClassificationEvidence = {
    productCode: 0,
    testSupport: 0,
    artifactStorage: 0,
    fixtureStorage: 0,
    externalCodeFixtures: 0,
    validationCorpus: 0,
  };

  // ───────────────────────────────────────────────────────────────────────────
  // Path-based signals
  // ───────────────────────────────────────────────────────────────────────────

  for (const category of pathSignals.segmentCategories) {
    switch (category) {
      case 'test':
        evidence.testSupport += 2;
        reasons.push('path contains test directory');
        break;
      case 'fixture':
        evidence.fixtureStorage += 3;
        reasons.push('path contains fixture directory');
        break;
      case 'validation':
        evidence.validationCorpus += 4;
        reasons.push('path contains validation/corpus directory');
        break;
      case 'artifact':
        evidence.artifactStorage += 3;
        reasons.push('path contains artifact/output directory');
        break;
      case 'external':
        evidence.externalCodeFixtures += 4;
        reasons.push('path contains external/vendor directory');
        break;
      case 'example':
        evidence.fixtureStorage += 1;
        reasons.push('path contains example directory');
        break;
      case 'source':
        evidence.productCode += 2;
        reasons.push('path contains source directory');
        break;
      case 'support':
        // Support could be product code or test support
        evidence.productCode += 1;
        evidence.testSupport += 1;
        break;
      case 'docs':
      case 'config':
      case 'neutral':
        // No strong signal
        break;
    }
  }

  // Timestamped segments suggest artifacts/runs
  if (pathSignals.hasTimestampedSegment) {
    evidence.artifactStorage += 2;
    evidence.validationCorpus += 1;
    reasons.push('path contains timestamped segment');
  }

  // Copied source tree pattern
  if (pathSignals.looksLikeCopiedSourceTree) {
    evidence.externalCodeFixtures += 3;
    evidence.validationCorpus += 2;
    reasons.push('path structure suggests copied source tree');
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Repo-type mismatch detection
  // ───────────────────────────────────────────────────────────────────────────

  const codeDomain = detectCodeDomain(childSummaries);
  const mismatchResult = isRepoTypeMismatch(repoProfile, codeDomain);

  if (mismatchResult.mismatch) {
    // Strong signal: code domain doesn't match repo type
    evidence.externalCodeFixtures += 5;
    evidence.validationCorpus += 4;
    evidence.productCode -= 5;
    reasons.push(mismatchResult.reason);
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Depth heuristic
  // ───────────────────────────────────────────────────────────────────────────

  // Deep nesting under validation/fixture paths increases confidence
  if (pathSignals.depth >= 2) {
    const hasValidationAncestor = pathSignals.segmentCategories.some(
      (c: PathSegmentCategory) => c === 'validation' || c === 'fixture' || c === 'external'
    );
    if (hasValidationAncestor) {
      evidence.validationCorpus += 1;
      evidence.externalCodeFixtures += 1;
    }
  }

  // ───────────────────────────────────────────────────────────────────────────
  // Determine final classification
  // ───────────────────────────────────────────────────────────────────────────

  const entries = Object.entries(evidence) as Array<[keyof ClassificationEvidence, number]>;
  entries.sort((a, b) => b[1] - a[1]);

  const [topClass, topScore] = entries[0];
  const [secondClass, secondScore] = entries[1];

  // Map evidence key to RepoContextClass
  const classMap: Record<keyof ClassificationEvidence, RepoContextClass> = {
    productCode: 'product_code',
    testSupport: 'test_support',
    artifactStorage: 'artifact_storage',
    fixtureStorage: 'fixture_storage',
    externalCodeFixtures: 'external_code_fixtures',
    validationCorpus: 'validation_corpus',
  };

  // Determine confidence
  let confidence: 'high' | 'medium' | 'low';
  if (topScore >= 5 && topScore - secondScore >= 2) {
    confidence = 'high';
  } else if (topScore >= 3) {
    confidence = 'medium';
  } else if (topScore >= 1) {
    confidence = 'low';
  } else {
    // No evidence
    return {
      repoContextClass: 'unknown',
      confidence: 'low',
      reasons: ['no classification signals found'],
    };
  }

  // If evidence is very weak or mixed, prefer unknown
  if (topScore < 2 || (topScore === secondScore && topScore < 4)) {
    return {
      repoContextClass: 'unknown',
      confidence: 'low',
      reasons: reasons.length > 0 ? reasons : ['mixed/ambiguous signals'],
    };
  }

  return {
    repoContextClass: classMap[topClass],
    confidence,
    reasons,
  };
}
