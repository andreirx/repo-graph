/**
 * Core types for rgistr.
 */

/**
 * MAP.md frontmatter contract.
 * Machine-readable metadata for generated documentation.
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
   * - 'file': single source file summary
   * - 'folder': folder summary synthesized from children
   * - 'repo': reserved for repo-level summary
   */
  scope: 'file' | 'folder' | 'repo';
  /** Path relative to repo root */
  path: string;
  /** ISO 8601 timestamp */
  generated_at: string;
  /** Synthesis basis: code_only, code_and_graph, code_graph_and_docs */
  synthesis_basis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs';
  /** Confidence level: high (authored docs exist), medium (graph context), low (code only) */
  confidence: 'high' | 'medium' | 'low';
  /** List of child MAP.md files that were used as input (for folder scope) */
  child_maps?: string[];
  /** List of source files summarized (for folder scope) */
  source_files?: string[];
  /** List of file MAPs used in synthesis (for folder scope) */
  file_maps?: string[];
  /** Original source filename (for file scope - avoids underscore parsing issues) */
  source_filename?: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Graph Context Adapter Interface
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Compact graph context for a scope.
 * This is the seam between rgistr and repo-graph.
 *
 * The adapter populates this from rmap (or returns null for code_only mode).
 * The DTO is designed to be token-efficient and stable.
 */
export interface GraphContext {
  /** Scope kind this context applies to */
  scope_kind: 'file' | 'folder';
  /** Path relative to repo root */
  scope_path: string;
  /** Top symbols defined in this scope */
  symbols?: Array<{
    name: string;
    kind: string;  // 'function', 'class', 'const', etc.
    exported: boolean;
  }>;
  /** Compact imports summary (not full list) */
  imports_summary?: string;
  /** Compact exports summary */
  exports_summary?: string;
  /** Summary of callers/callees relationships */
  callers_callees_summary?: string;
  /** Owning module if detected */
  owning_module?: string;
  /** Relevant authored docs */
  relevant_docs?: string[];
  /** Boundary/seam information */
  boundary_summary?: string;
  /** Runtime/build context if available */
  runtime_build_summary?: string;
  /** Trust/degradation markers */
  trust_markers?: Record<string, string>;
}

/**
 * Adapter interface for graph context injection.
 *
 * Implementations:
 * - NullGraphContextAdapter: returns null (code_only mode)
 * - RmapGraphContextAdapter: shells out to `rmap context` (future)
 */
export interface IGraphContextAdapter {
  /**
   * Get graph context for a scope.
   * Returns null if not available.
   */
  getContext(
    scopeKind: 'file' | 'folder',
    scopePath: string
  ): Promise<GraphContext | null>;

  /**
   * Check if the adapter is available.
   */
  isAvailable(): Promise<boolean>;

  /**
   * Adapter name for frontmatter.
   */
  readonly name: string;
}

/**
 * Result of generating a single MAP.md file.
 */
export interface MapGenerationResult {
  /** Path where MAP.md was written */
  path: string;
  /** Scope kind */
  scope: 'file' | 'folder' | 'repo';
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
