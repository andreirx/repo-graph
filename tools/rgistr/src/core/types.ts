/**
 * Core types for rgistr.
 */

/**
 * MAP.md frontmatter contract.
 * Machine-readable metadata for generated documentation.
 *
 * NOTE: v0.1.0 only generates 'folder' scope. File, module, and repo
 * scopes are reserved for future versions but not yet implemented.
 */
export interface MapFrontmatter {
  /** Tool that generated this file */
  generated_by: 'rgistr';
  /** Version of the generator */
  generator_version: string;
  /** LLM adapter used (e.g. "lmstudio", "ollama", "openai") */
  adapter: string;
  /** Model name used for generation */
  model: string;
  /** Git commit hash at generation time (if available) */
  basis_commit: string | null;
  /**
   * Scope kind.
   * v0.1.0: only 'folder' is generated.
   * Reserved: 'file', 'module', 'repo' (future work).
   */
  scope: 'folder';
  /** Path relative to repo root */
  path: string;
  /** ISO 8601 timestamp */
  generated_at: string;
  /** Synthesis basis: code_only, code_and_graph, code_graph_and_docs */
  synthesis_basis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs';
  /** Confidence level: high (authored docs exist), medium (graph context), low (code only) */
  confidence: 'high' | 'medium' | 'low';
  /** List of child MAP.md files that were used as input */
  child_maps?: string[];
  /** List of source files summarized */
  source_files?: string[];
}

/**
 * Result of generating a single MAP.md file.
 */
export interface MapGenerationResult {
  /** Path where MAP.md was written */
  path: string;
  /** Scope kind (v0.1.0: always 'folder') */
  scope: 'folder';
  /** Whether generation succeeded */
  success: boolean;
  /** Error message if failed */
  error?: string;
  /** Tokens used (if available from provider) */
  tokensUsed?: number;
}

/**
 * Configuration for a generation run.
 */
export interface GenerationConfig {
  /** Root path to process */
  rootPath: string;
  /** Output filename (default: MAP.md) */
  outputFilename?: string;
  /** Maximum depth for recursion (-1 = unlimited) */
  maxDepth?: number;
  /** File patterns to include (glob) */
  includePatterns?: string[];
  /** File patterns to exclude (glob) */
  excludePatterns?: string[];
  /** Skip files larger than this (bytes, default: 100KB) */
  maxFileSize?: number;
  /** Only process these scopes */
  scopes?: ('file' | 'folder' | 'module' | 'repo')[];
  /** Dry run - show what would be generated without writing */
  dryRun?: boolean;
  /** Force regeneration even if MAP.md exists and is fresh */
  force?: boolean;
}

/**
 * File info collected during directory traversal.
 */
export interface FileInfo {
  /** Absolute path */
  path: string;
  /** Path relative to generation root */
  relativePath: string;
  /** File extension (lowercase, with dot) */
  extension: string;
  /** File size in bytes */
  size: number;
  /** Last modified time */
  mtime: Date;
  /** Whether this is a code file */
  isCode: boolean;
  /** Whether this is a documentation file */
  isDoc: boolean;
}

/**
 * Folder info during traversal.
 */
export interface FolderInfo {
  /** Absolute path */
  path: string;
  /** Path relative to generation root */
  relativePath: string;
  /** Child files (direct) */
  files: FileInfo[];
  /** Child folders (direct) */
  folders: FolderInfo[];
  /** Whether this folder has a MAP.md */
  hasMap: boolean;
  /** Depth from root (0 = root) */
  depth: number;
}
