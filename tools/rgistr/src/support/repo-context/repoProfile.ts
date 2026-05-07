/**
 * Repo profile derivation.
 *
 * Builds a coarse profile of the repository from manifests and structure.
 * Used to detect domain mismatches when classifying folders.
 */

import * as fs from 'fs';
import * as path from 'path';
import { RepoProfile, RepoType } from './types.js';

/**
 * Keywords that suggest code-analysis/tooling repos.
 */
const CODE_ANALYSIS_KEYWORDS = [
  'lint', 'linter', 'analyzer', 'analysis', 'ast', 'parser',
  'graph', 'index', 'indexer', 'code-intel', 'code intel',
  'static analysis', 'semantic', 'symbol', 'call graph',
  'dependency', 'import', 'repo-graph', 'sourcegraph',
  'tree-sitter', 'treesitter', 'language server', 'lsp',
];

/**
 * Keywords that suggest library repos.
 */
const LIBRARY_KEYWORDS = [
  'sdk', 'client', 'api', 'wrapper', 'binding', 'library',
];

/**
 * Keywords that suggest application repos.
 */
const APPLICATION_KEYWORDS = [
  'app', 'application', 'service', 'server', 'platform',
  'dashboard', 'portal', 'frontend', 'backend',
];

/**
 * Keywords that suggest infra/tooling repos.
 */
const INFRA_KEYWORDS = [
  'deploy', 'ci', 'cd', 'terraform', 'kubernetes', 'k8s',
  'docker', 'ansible', 'puppet', 'chef', 'infra',
];

/**
 * Keywords that suggest OS/kernel repos.
 */
const OS_KEYWORDS = [
  'kernel', 'linux', 'bsd', 'operating system', 'os',
  'bootloader', 'firmware', 'rtos', 'embedded',
];

/**
 * Derive repo profile from the repo root.
 */
export function deriveRepoProfile(repoRoot: string): RepoProfile {
  const rootName = path.basename(repoRoot);
  const topLevelDirs = getTopLevelDirs(repoRoot);
  const reasons: string[] = [];

  // Try to read manifests
  const packageJson = tryReadPackageJson(repoRoot);
  const cargoToml = tryReadCargoToml(repoRoot);

  // Extract metadata
  const packageName = packageJson?.name || cargoToml?.name;
  const description = packageJson?.description || cargoToml?.description;

  // Derive repo type from evidence
  const repoType = deriveRepoType(
    rootName,
    topLevelDirs,
    packageName,
    description,
    reasons
  );

  return {
    rootName,
    topLevelDirs,
    repoType,
    repoTypeReasons: reasons,
    packageName,
    description,
  };
}

/**
 * Get top-level directory names.
 */
function getTopLevelDirs(repoRoot: string): string[] {
  try {
    const entries = fs.readdirSync(repoRoot, { withFileTypes: true });
    return entries
      .filter(e => e.isDirectory() && !e.name.startsWith('.'))
      .map(e => e.name);
  } catch {
    return [];
  }
}

/**
 * Try to read and parse package.json.
 */
function tryReadPackageJson(repoRoot: string): { name?: string; description?: string } | null {
  try {
    const content = fs.readFileSync(path.join(repoRoot, 'package.json'), 'utf-8');
    const parsed = JSON.parse(content);
    return {
      name: parsed.name,
      description: parsed.description,
    };
  } catch {
    return null;
  }
}

/**
 * Try to read Cargo.toml and extract basic metadata.
 */
function tryReadCargoToml(repoRoot: string): { name?: string; description?: string } | null {
  try {
    const content = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf-8');

    // Simple extraction without full TOML parser
    const nameMatch = content.match(/^name\s*=\s*"([^"]+)"/m);
    const descMatch = content.match(/^description\s*=\s*"([^"]+)"/m);

    return {
      name: nameMatch?.[1],
      description: descMatch?.[1],
    };
  } catch {
    return null;
  }
}

/**
 * Derive repo type from available evidence.
 */
function deriveRepoType(
  rootName: string,
  topLevelDirs: string[],
  packageName: string | undefined,
  description: string | undefined,
  reasons: string[]
): RepoType {
  const searchText = [
    rootName,
    packageName || '',
    description || '',
  ].join(' ').toLowerCase();

  // Check for code-analysis tooling
  for (const keyword of CODE_ANALYSIS_KEYWORDS) {
    if (searchText.includes(keyword)) {
      reasons.push(`name/description contains "${keyword}"`);
      return 'code_analysis_tooling';
    }
  }

  // Check top-level dirs for tooling signals
  if (topLevelDirs.includes('rust') && topLevelDirs.includes('tools')) {
    reasons.push('has rust/ and tools/ directories suggesting multi-language tooling');
    return 'code_analysis_tooling';
  }

  // Check for OS/kernel signals
  for (const keyword of OS_KEYWORDS) {
    if (searchText.includes(keyword)) {
      reasons.push(`name/description contains "${keyword}"`);
      return 'operating_system';
    }
  }

  // Check for embedded/firmware by directory structure
  if (topLevelDirs.some(d => ['arch', 'drivers', 'kernel', 'fs'].includes(d))) {
    const matches = topLevelDirs.filter(d => ['arch', 'drivers', 'kernel', 'fs'].includes(d));
    if (matches.length >= 2) {
      reasons.push(`has kernel-like top-level dirs: ${matches.join(', ')}`);
      return 'operating_system';
    }
  }

  // Check for infra tooling
  for (const keyword of INFRA_KEYWORDS) {
    if (searchText.includes(keyword)) {
      reasons.push(`name/description contains "${keyword}"`);
      return 'infra_tooling';
    }
  }

  // Check for library
  for (const keyword of LIBRARY_KEYWORDS) {
    if (searchText.includes(keyword)) {
      reasons.push(`name/description contains "${keyword}"`);
      return 'library';
    }
  }

  // Check for application
  for (const keyword of APPLICATION_KEYWORDS) {
    if (searchText.includes(keyword)) {
      reasons.push(`name/description contains "${keyword}"`);
      return 'application';
    }
  }

  // Default to unknown
  reasons.push('no strong signals found');
  return 'unknown';
}
