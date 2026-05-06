/**
 * Chunking support module.
 *
 * Provides chunk planning, identity generation, and artifact serialization
 * for processing large files in multiple passes.
 *
 * This module is pure support infrastructure - no LLM calls, no I/O.
 * The generator feature layer uses this to plan and track chunks.
 */

// ── Types ────────────────────────────────────────────────────────────

export type {
  // Source spans
  SourceSpan,

  // Chunk identity and planning
  ChunkIdentity,
  PlannedChunk,
  ChunkPlan,

  // Artifact scopes
  ArtifactScope,

  // Artifact types
  ChunkArtifact,
  FileArtifact,
  FolderArtifact,
  FileSynthesisMode,

  // Frontmatter contracts
  ChunkArtifactFrontmatter,
  FileArtifactFrontmatter,
  FolderArtifactFrontmatter,
} from './types.js';

// ── Identity ─────────────────────────────────────────────────────────

export {
  computeFileHash,
  createChunkIdentity,
  formatChunkId,
  parseChunkId,
  chunkArtifactFilename,
  fileArtifactFilename,
  parseArtifactFilename,
  isChunkArtifactStale,
  isFileArtifactStale,
} from './identity.js';

// ── Planner ──────────────────────────────────────────────────────────

export type { ChunkPlannerConfig } from './planner.js';

export {
  planChunks,
  extractChunkContent,
  countLines,
} from './planner.js';

// ── Artifacts ────────────────────────────────────────────────────────

export {
  getGeneratorString,
  serializeChunkArtifact,
  parseChunkArtifact,
  serializeFileArtifact,
  parseFileArtifact,
  serializeFolderArtifact,
  parseFolderArtifact,
} from './artifacts.js';
