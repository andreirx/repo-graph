/**
 * Chunking support module types.
 *
 * DTOs for chunk planning, chunk artifacts, and file rollup.
 */

// ── Source span ──────────────────────────────────────────────────────

/**
 * A span of lines in a source file.
 */
export interface SourceSpan {
  /** 1-based start line (inclusive). */
  startLine: number;

  /** 1-based end line (inclusive). */
  endLine: number;

  /** Start column (1-based, optional). */
  startColumn?: number;

  /** End column (1-based, optional). */
  endColumn?: number;
}

// ── Chunk identity ───────────────────────────────────────────────────

/**
 * Deterministic chunk identifier.
 *
 * Format: `{file_hash}:{chunk_index}:{line_start}-{line_end}`
 *
 * This enables:
 * - Staleness detection (file_hash changes if content changes)
 * - Ordering (chunk_index)
 * - Provenance (line range)
 */
export interface ChunkIdentity {
  /** Hash of the source file content (first 8 chars of SHA-256). */
  fileHash: string;

  /** 0-based chunk index within the file. */
  chunkIndex: number;

  /** Total number of chunks in the file. */
  chunkCount: number;

  /** Line span this chunk covers. */
  span: SourceSpan;

  /** Stringified identity for use as artifact key. */
  readonly id: string;
}

// ── Chunk plan ───────────────────────────────────────────────────────

/**
 * A planned chunk before extraction.
 */
export interface PlannedChunk {
  /** Chunk identity. */
  identity: ChunkIdentity;

  /** Estimated token count. */
  estimatedTokens: number;

  /** Structural boundary type (if structural chunking was used). */
  boundaryType?: 'function' | 'class' | 'method' | 'block' | 'arbitrary';

  /** Overlap with previous chunk (number of lines). */
  overlapLines: number;
}

/**
 * A chunk plan for a file.
 */
export interface ChunkPlan {
  /** Source file path (repo-relative). */
  filePath: string;

  /** Hash of the source file content. */
  fileHash: string;

  /** Total file size in bytes. */
  fileSizeBytes: number;

  /** Estimated total tokens in file. */
  estimatedTotalTokens: number;

  /** Whether chunking is required. */
  requiresChunking: boolean;

  /** Planned chunks (empty if whole-file processing). */
  chunks: PlannedChunk[];

  /** Planning method used. */
  method: 'whole_file' | 'structural' | 'line_window' | 'recursive';
}

// ── Chunk artifact ───────────────────────────────────────────────────

/**
 * Artifact scope for generated content.
 */
export type ArtifactScope = 'chunk' | 'file' | 'folder' | 'repo';

/**
 * A chunk-level generated artifact.
 *
 * Produced by summarizing a single chunk of a file.
 */
export interface ChunkArtifact {
  /** Artifact scope. */
  scope: 'chunk';

  /** Source file path (repo-relative). */
  sourceFile: string;

  /** Source span this chunk covers. */
  sourceSpan: SourceSpan;

  /** Chunk identity. */
  chunkId: string;

  /** Chunk index (0-based). */
  chunkIndex: number;

  /** Total chunk count in file. */
  chunkCount: number;

  /** Generated summary/gist content. */
  content: string;

  /** Generation timestamp (ISO 8601). */
  generatedAt: string;

  /** Model used for generation. */
  model: string;

  /** Provider used for generation. */
  provider: string;

  /** Token count of source chunk. */
  sourceTokens: number;

  /** Token count of generated content. */
  outputTokens: number;
}

// ── File artifact ────────────────────────────────────────────────────

/**
 * Synthesis mode for file-level artifacts.
 */
export type FileSynthesisMode = 'whole_file' | 'chunk_rollup';

/**
 * A file-level generated artifact.
 *
 * Produced by either:
 * - Direct whole-file summarization
 * - Rolling up chunk artifacts
 */
export interface FileArtifact {
  /** Artifact scope. */
  scope: 'file';

  /** Source file path (repo-relative). */
  sourceFile: string;

  /** File hash for staleness detection. */
  fileHash: string;

  /** How this artifact was synthesized. */
  synthesisMode: FileSynthesisMode;

  /** Chunk IDs that were rolled up (if chunk_rollup mode). */
  chunkBasis?: string[];

  /** Generated summary/gist content. */
  content: string;

  /** Generation timestamp (ISO 8601). */
  generatedAt: string;

  /** Model used for generation. */
  model: string;

  /** Provider used for generation. */
  provider: string;

  /** Uncertainty notes from chunking (if applicable). */
  uncertaintyNotes?: string[];
}

// ── Folder artifact ──────────────────────────────────────────────────

/**
 * A folder-level generated artifact (MAP.md).
 *
 * Synthesized from file artifacts and child folder artifacts.
 */
export interface FolderArtifact {
  /** Artifact scope. */
  scope: 'folder';

  /** Folder path (repo-relative). */
  folderPath: string;

  /** File artifacts that were synthesized. */
  fileBasis: string[];

  /** Child folder artifacts that were synthesized. */
  childFolderBasis: string[];

  /** Generated MAP.md content. */
  content: string;

  /** Generation timestamp (ISO 8601). */
  generatedAt: string;

  /** Model used for generation. */
  model: string;

  /** Provider used for generation. */
  provider: string;
}

// ── Artifact frontmatter ─────────────────────────────────────────────

/**
 * Frontmatter fields for chunk artifacts.
 *
 * This defines the YAML frontmatter contract for chunk artifact files.
 */
export interface ChunkArtifactFrontmatter {
  /** Always 'chunk'. */
  scope: 'chunk';

  /** Source file path. */
  source_file: string;

  /** Start line of chunk. */
  source_line_start: number;

  /** End line of chunk. */
  source_line_end: number;

  /** Chunk index (0-based). */
  chunk_index: number;

  /** Total chunks in file. */
  chunk_count: number;

  /** Chunk identity string. */
  chunk_id: string;

  /** File content hash. */
  file_hash: string;

  /** Generation timestamp. */
  generated_at: string;

  /** Generator tool and version. */
  generator: string;

  /** Model used. */
  model: string;

  /** Provider used. */
  provider: string;

  /** Token count of source chunk. */
  source_tokens: number;

  /** Token count of generated content. */
  output_tokens: number;
}

/**
 * Frontmatter fields for file artifacts.
 */
export interface FileArtifactFrontmatter {
  /** Always 'file'. */
  scope: 'file';

  /** Source file path. */
  source_file: string;

  /** File content hash. */
  file_hash: string;

  /** Synthesis mode. */
  synthesis_mode: FileSynthesisMode;

  /** Chunk IDs if chunk_rollup mode. */
  chunk_basis?: string[];

  /** Generation timestamp. */
  generated_at: string;

  /** Generator tool and version. */
  generator: string;

  /** Model used. */
  model: string;

  /** Provider used. */
  provider: string;

  /** Uncertainty notes from chunking. */
  uncertainty_notes?: string[];
}

/**
 * Frontmatter fields for folder artifacts (MAP.md).
 */
export interface FolderArtifactFrontmatter {
  /** Always 'folder'. */
  scope: 'folder';

  /** Folder path. */
  folder_path: string;

  /** Files included in synthesis. */
  file_basis: string[];

  /** Child folders included in synthesis. */
  child_folder_basis: string[];

  /** Generation timestamp. */
  generated_at: string;

  /** Generator tool and version. */
  generator: string;

  /** Model used. */
  model: string;

  /** Provider used. */
  provider: string;
}
