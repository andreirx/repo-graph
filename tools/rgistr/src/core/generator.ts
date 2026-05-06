/**
 * Recursive documentation generator.
 * Orchestrates the depth-first MAP.md generation process.
 *
 * Generation flow:
 * 1. Scan directory tree
 * 2. For each folder (depth-first):
 *    a. Generate file MAPs for local code files
 *    b. Read child folder MAPs
 *    c. Read local file MAPs
 *    d. Synthesize folder MAP from children
 * 3. Report results
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { ILLMAdapter } from '../adapters/llm/index.js';
import type { IGraphContextAdapter } from '../adapters/graph/index.js';
import type { FolderInfo, FileInfo, MapGenerationResult, GenerationConfig } from './types.js';
import { scanDirectory, getCodeFiles, getFoldersForGeneration } from './scanner.js';
import { writeMap, readMap, getGitCommit, isMapFresh, fileMapFilename, isFileMapFilename } from './map-writer.js';
import { digestFile } from './predigest.js';
import {
  SYSTEM_PROMPT,
  filePromptWhole,
  folderPrompt,
  fileGroupPrompt,
  chunkPrompt,
  chunkRollupPrompt,
  parseFileSummary,
  parseFolderSummary,
  parseChunkSummary,
  renderFileSummary,
  renderFolderSummary,
  renderChunkSummary,
  extractSection,
  type FileSummarySchema,
  type FolderSummarySchema,
  type ChunkSummaryInput,
  type ChildSummary
} from './prompts.js';
import {
  planChunks,
  extractChunkContent,
  countLines,
  computeFileHash,
  serializeChunkArtifact,
  serializeFileArtifact,
  parseFileArtifact,
  chunkArtifactFilename,
  type ChunkArtifact,
  type FileArtifact,
} from '../support/chunking/index.js';
import { estimateTokens } from '../support/capability/index.js';

export interface GeneratorOptions {
  /** LLM adapter to use */
  llm: ILLMAdapter;
  /** Graph context adapter (null adapter for code_only) */
  graphAdapter?: IGraphContextAdapter;
  /** Generation configuration */
  config: GenerationConfig;
  /** Repository root path (for correct path provenance in frontmatter) */
  repoRoot?: string;
  /** Progress callback */
  onProgress?: (status: GenerationStatus) => void;
}

export interface GenerationStatus {
  phase: 'scanning' | 'generating-files' | 'generating-folders' | 'complete';
  current?: string;
  processed: number;
  total: number;
  errors: number;
}

/** Threshold for whole-file vs chunked summarization (bytes).
 * Files at or below this size use whole-file prompt.
 * Files above this size use chunked generation. */
const WHOLE_FILE_THRESHOLD = 200 * 1024; // 200KB

/**
 * Generate MAP.md files for a codebase.
 */
export async function generate(options: GeneratorOptions): Promise<MapGenerationResult[]> {
  const { llm, graphAdapter, config, repoRoot, onProgress } = options;
  const results: MapGenerationResult[] = [];
  const synthesisBasis = graphAdapter ? 'code_and_graph' : 'code_only';

  // Repo root for correct path provenance (defaults to scan root if not specified)
  const effectiveRepoRoot = repoRoot || config.rootPath;

  // Phase 1: Scan the directory tree
  onProgress?.({ phase: 'scanning', processed: 0, total: 0, errors: 0 });

  const root = await scanDirectory(config.rootPath, {
    maxDepth: config.maxDepth,
    maxFileSize: config.maxFileSize
  });

  // Get folders that need generation (depth-first order)
  const folders = getFoldersForGeneration(root, config.force, true);

  // Get git commit for provenance
  const basisCommit = await getGitCommit(config.rootPath);

  // Count total work items (all eligible files + all folders)
  let totalFiles = 0;
  for (const folder of folders) {
    const codeFiles = getCodeFiles(folder);
    totalFiles += codeFiles.length;
  }
  const totalFolders = folders.length;
  const total = totalFiles + totalFolders;

  let processed = 0;

  // Phase 2: Generate file MAPs (depth-first, files before folders)
  onProgress?.({ phase: 'generating-files', processed: 0, total, errors: 0 });

  for (const folder of folders) {
    const codeFiles = getCodeFiles(folder);

    // Generate file MAPs for all eligible source files
    for (const file of codeFiles) {
      const fileMapName = fileMapFilename(path.basename(file.path));
      const fileMapPath = path.join(folder.path, fileMapName);

      // Check freshness
      if (!config.force) {
        const fresh = await isMapFresh(fileMapPath, [file.path]);
        if (fresh) {
          // For chunked files, also verify chunk artifacts are intact
          if (file.size > WHOLE_FILE_THRESHOLD) {
            const chunksIntact = await areChunkArtifactsIntact(fileMapPath, folder.path);
            if (!chunksIntact) {
              // Chunk artifacts missing or corrupted - regenerate
              // Fall through to generation
            } else {
              continue;
            }
          } else {
            continue;
          }
        }
      }

      onProgress?.({
        phase: 'generating-files',
        current: file.relativePath,
        processed,
        total,
        errors: results.filter(r => !r.success).length
      });

      try {
        // Two-mode routing: whole-file or chunked
        if (file.size > WHOLE_FILE_THRESHOLD) {
          // Chunked path for files exceeding threshold
          const result = await generateChunkedFileMap(file, folder.path, {
            llm,
            graphAdapter,
            rootPath: config.rootPath,
            repoRoot: effectiveRepoRoot,
            basisCommit,
            synthesisBasis,
            dryRun: config.dryRun,
            chunkSizeKb: config.chunkSizeKb
          });
          results.push(result);
        } else {
          // Whole-file path
          const result = await generateFileMap(file, folder.path, {
            llm,
            graphAdapter,
            rootPath: config.rootPath,
            repoRoot: effectiveRepoRoot,
            basisCommit,
            synthesisBasis,
            dryRun: config.dryRun
          });
          results.push(result);
        }
      } catch (error) {
        results.push({
          path: fileMapPath,
          scope: 'file',
          success: false,
          error: error instanceof Error ? error.message : String(error)
        });
      }

      processed++;
    }
  }

  // Phase 3: Generate folder MAPs
  onProgress?.({ phase: 'generating-folders', processed, total, errors: results.filter(r => !r.success).length });

  for (const folder of folders) {
    const relativePath = folder.relativePath || '.';

    // Check freshness for folder MAP
    const folderMapPath = path.join(folder.path, 'MAP.md');
    if (!config.force && folder.hasMap) {
      // Check against source files AND child MAPs AND local file MAPs
      const sourceFiles = getCodeFiles(folder).map(f => f.path);
      const childMapFiles = folder.folders
        .filter(f => f.hasMap)
        .map(f => path.join(f.path, 'MAP.md'));
      const localFileMaps = await getLocalFileMaps(folder.path);
      const allSources = [...sourceFiles, ...childMapFiles, ...localFileMaps];
      const fresh = await isMapFresh(folderMapPath, allSources);
      if (fresh) {
        processed++;
        continue;
      }
    }

    onProgress?.({
      phase: 'generating-folders',
      current: relativePath,
      processed,
      total,
      errors: results.filter(r => !r.success).length
    });

    try {
      const result = await generateFolderMap(folder, {
        llm,
        graphAdapter,
        rootPath: config.rootPath,
        repoRoot: effectiveRepoRoot,
        basisCommit,
        synthesisBasis,
        dryRun: config.dryRun
      });
      results.push(result);
    } catch (error) {
      results.push({
        path: folderMapPath,
        scope: 'folder',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    processed++;
  }

  onProgress?.({
    phase: 'complete',
    processed: total,
    total,
    errors: results.filter(r => !r.success).length
  });

  return results;
}

// ─────────────────────────────────────────────────────────────────────────────
// File MAP Generation
// ─────────────────────────────────────────────────────────────────────────────

interface FileMapOptions {
  llm: ILLMAdapter;
  graphAdapter?: IGraphContextAdapter;
  rootPath: string;
  repoRoot: string;
  basisCommit: string | null;
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs';
  dryRun?: boolean;
  chunkSizeKb?: number;
}

async function generateFileMap(
  file: FileInfo,
  folderPath: string,
  options: FileMapOptions
): Promise<MapGenerationResult> {
  const { llm, graphAdapter, rootPath, repoRoot, basisCommit, synthesisBasis, dryRun } = options;
  const fileMapName = fileMapFilename(path.basename(file.path));
  const outputPath = path.join(folderPath, fileMapName);

  // Compute repo-relative path for frontmatter (P1 fix)
  const repoRelativePath = path.relative(repoRoot, file.path);

  // Read file content
  const content = await fs.readFile(file.path, 'utf-8');
  const ext = path.extname(file.path).slice(1);
  const language = detectLanguage(ext);

  // Get graph context if available
  let graphContext: string | undefined;
  if (graphAdapter && synthesisBasis !== 'code_only') {
    const ctx = await graphAdapter.getContext('file', repoRelativePath);
    if (ctx) {
      graphContext = formatGraphContext(ctx);
    }
  }

  // Whole-file prompt (routing ensures only files within threshold reach here)
  const prompt = filePromptWhole(repoRelativePath, content, language, synthesisBasis, graphContext);

  // Generate summary via LLM (markdown output, no JSON)
  const response = await llm.complete(prompt, {
    maxTokens: 4000,
    temperature: 0.3,
    systemPrompt: SYSTEM_PROMPT
  });

  // Parse markdown sections
  const summary = parseFileSummary(response);
  const renderedContent = renderFileSummary(summary);

  // Determine confidence
  const confidence = synthesisBasis === 'code_only' ? 'low' : 'medium';

  if (dryRun) {
    console.log(`[dry-run] Would write: ${outputPath}`);
    return { path: outputPath, scope: 'file', success: true };
  }

  // Write the file MAP with repo-relative path
  await writeMap({
    folderPath,
    content: renderedContent,
    scope: 'file',
    relativePath: repoRelativePath,
    adapter: llm.adapterName,
    model: llm.modelName,
    basisCommit,
    synthesisBasis,
    confidence,
    filename: fileMapName,
    sourceFilename: path.basename(file.path)  // P2 fix: preserve original filename
  });

  return { path: outputPath, scope: 'file', success: true };
}

/** Detect language from file extension */
function detectLanguage(ext: string): string {
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'typescript',
    js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
    py: 'python', pyi: 'python',
    rs: 'rust',
    go: 'go',
    java: 'java', kt: 'kotlin',
    c: 'c', h: 'c', cpp: 'cpp', hpp: 'cpp',
    rb: 'ruby', php: 'php', swift: 'swift', scala: 'scala', cs: 'csharp',
    vue: 'vue', svelte: 'svelte'
  };
  return map[ext] || ext || 'text';
}

// ─────────────────────────────────────────────────────────────────────────────
// Chunked File MAP Generation (Oversized Files)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Generate file MAP via chunked processing for oversized files.
 *
 * Flow:
 * 1. Plan chunks using the chunking support module
 * 2. Generate chunk artifacts for each chunk
 * 3. Roll up chunk summaries into file summary
 * 4. Write file artifact with chunk_rollup synthesis mode
 */
async function generateChunkedFileMap(
  file: FileInfo,
  folderPath: string,
  options: FileMapOptions
): Promise<MapGenerationResult> {
  const { llm, rootPath, repoRoot, basisCommit, synthesisBasis, dryRun } = options;
  const fileMapName = fileMapFilename(path.basename(file.path));
  const outputPath = path.join(folderPath, fileMapName);

  // Compute repo-relative path for frontmatter
  const repoRelativePath = path.relative(repoRoot, file.path);

  // Read file content
  const content = await fs.readFile(file.path, 'utf-8');
  const ext = path.extname(file.path).slice(1);
  const language = detectLanguage(ext);
  const totalLines = countLines(content);

  // Chunked files always use code_only synthesis.
  // Graph context per-chunk is not implemented - acknowledge the limitation honestly.
  const effectiveSynthesisBasis = 'code_only' as const;
  const graphContextDropped = synthesisBasis !== 'code_only';

  // Plan chunks using the chunking support module.
  // forceChunking: true because the CLI already decided this file exceeds
  // WHOLE_FILE_THRESHOLD (200KB). The byte-size threshold is the product decision;
  // token budget is only for chunk sizing.
  const chunkSizeBytes = (options.chunkSizeKb || 200) * 1024;
  const plan = planChunks(repoRelativePath, content, {
    modelId: llm.modelName,
    includeGraphContext: false, // No graph context in chunked path
    forceChunking: true, // CLI decided to chunk based on byte size
    targetChunkBytes: chunkSizeBytes,
  });

  // With forceChunking: true, plan.requiresChunking should always be true.
  // This guard is defensive only.
  if (!plan.requiresChunking || plan.chunks.length === 0) {
    console.error(`[WARN] Chunked path for ${file.relativePath} produced no chunks. Falling back to whole-file.`);
    return generateFileMap(file, folderPath, options);
  }

  const chunkArtifacts: ChunkArtifact[] = [];
  const chunkSummaryInputs: ChunkSummaryInput[] = [];
  const generatedAt = new Date().toISOString();

  // Generate chunk artifacts
  for (const plannedChunk of plan.chunks) {
    const chunkContent = extractChunkContent(content, plannedChunk.identity.span);
    const { startLine, endLine } = plannedChunk.identity.span;

    // Generate chunk summary via LLM
    const prompt = chunkPrompt(
      repoRelativePath,
      chunkContent,
      language,
      plannedChunk.identity.chunkIndex,
      plannedChunk.identity.chunkCount,
      startLine,
      endLine,
      effectiveSynthesisBasis
    );

    const response = await llm.complete(prompt, {
      maxTokens: 2000,
      temperature: 0.3,
      systemPrompt: SYSTEM_PROMPT
    });

    // Parse and render chunk summary
    const chunkSummary = parseChunkSummary(response);
    const renderedChunkContent = renderChunkSummary(chunkSummary);

    // Build chunk artifact
    const chunkArtifact: ChunkArtifact = {
      scope: 'chunk',
      sourceFile: repoRelativePath,
      sourceSpan: plannedChunk.identity.span,
      chunkId: plannedChunk.identity.id,
      chunkIndex: plannedChunk.identity.chunkIndex,
      chunkCount: plannedChunk.identity.chunkCount,
      content: renderedChunkContent,
      generatedAt,
      model: llm.modelName,
      provider: llm.adapterName,
      sourceTokens: estimateTokens(chunkContent).tokens,
      outputTokens: estimateTokens(renderedChunkContent).tokens,
    };

    chunkArtifacts.push(chunkArtifact);

    // Collect for rollup
    chunkSummaryInputs.push({
      chunkIndex: plannedChunk.identity.chunkIndex,
      lineStart: startLine,
      lineEnd: endLine,
      summary: renderedChunkContent,
    });

    // Write chunk artifact (unless dry run)
    if (!dryRun) {
      const chunkFilename = chunkArtifactFilename(path.basename(file.path), plannedChunk.identity.chunkIndex);
      const chunkPath = path.join(folderPath, chunkFilename);
      const serialized = serializeChunkArtifact(chunkArtifact);
      await fs.writeFile(chunkPath, serialized, 'utf-8');
    }
  }

  // Generate file-level rollup from chunk summaries
  const rollupPrompt = chunkRollupPrompt(
    repoRelativePath,
    chunkSummaryInputs,
    language,
    totalLines,
    effectiveSynthesisBasis
  );

  const rollupResponse = await llm.complete(rollupPrompt, {
    maxTokens: 4000,
    temperature: 0.3,
    systemPrompt: SYSTEM_PROMPT
  });

  // Parse file summary from rollup
  const fileSummary = parseFileSummary(rollupResponse);
  const renderedContent = renderFileSummary(fileSummary);

  // Build uncertainty notes
  const uncertaintyNotes: string[] = [
    `File processed in ${plan.chunks.length} chunks due to size (${plan.fileSizeBytes} bytes).`,
    'Cross-chunk relationships may be incomplete.',
  ];
  if (graphContextDropped) {
    uncertaintyNotes.push(
      `Graph context was requested (${synthesisBasis}) but not available for chunked files. Synthesis used code_only.`
    );
  }

  // Build file artifact with chunk_rollup mode
  const fileArtifact: FileArtifact = {
    scope: 'file',
    sourceFile: repoRelativePath,
    fileHash: plan.fileHash,
    synthesisMode: 'chunk_rollup',
    chunkBasis: chunkArtifacts.map(c => c.chunkId),
    content: renderedContent,
    generatedAt,
    model: llm.modelName,
    provider: llm.adapterName,
    uncertaintyNotes,
  };

  if (dryRun) {
    console.log(`[dry-run] Would write: ${outputPath} (chunked: ${plan.chunks.length} chunks)`);
    return { path: outputPath, scope: 'file', success: true };
  }

  // Write file artifact using the chunking module serialization
  const serializedFile = serializeFileArtifact(fileArtifact);
  await fs.writeFile(outputPath, serializedFile, 'utf-8');

  return { path: outputPath, scope: 'file', success: true };
}

// ─────────────────────────────────────────────────────────────────────────────
// Folder MAP Generation
// ─────────────────────────────────────────────────────────────────────────────

interface FolderMapOptions {
  llm: ILLMAdapter;
  graphAdapter?: IGraphContextAdapter;
  rootPath: string;
  repoRoot: string;
  basisCommit: string | null;
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs';
  dryRun?: boolean;
}

async function generateFolderMap(
  folder: FolderInfo,
  options: FolderMapOptions
): Promise<MapGenerationResult> {
  const { llm, graphAdapter, rootPath, basisCommit, synthesisBasis, dryRun } = options;
  const outputPath = path.join(folder.path, 'MAP.md');
  const codeFiles = getCodeFiles(folder);

  const fileSummaries: ChildSummary[] = [];
  const folderSummaries: ChildSummary[] = [];
  const usedFileMaps: string[] = [];
  const usedChildMaps: string[] = [];

  // Collect child folder MAP summaries
  for (const sub of folder.folders) {
    const subMapPath = path.join(sub.path, 'MAP.md');
    const subMap = await readMap(subMapPath);
    if (subMap) {
      folderSummaries.push({
        name: path.basename(sub.path),
        isFolder: true,
        summary: extractPurpose(subMap.content)
      });
      // P1 fix: use repo-relative path for child maps
      usedChildMaps.push(path.relative(options.repoRoot, subMapPath));
    }
  }

  // Collect local file MAP summaries
  const localFileMaps = await getLocalFileMaps(folder.path);
  for (const mapPath of localFileMaps) {
    const mapData = await readMap(mapPath);
    if (mapData) {
      // P2 fix: use source_filename from frontmatter if available
      const sourceName = mapData.frontmatter.source_filename ||
        path.basename(mapPath).replace(/_MAP\.md$/, '').replace(/_([^_]+)$/, '.$1');
      fileSummaries.push({
        name: sourceName,
        isFolder: false,
        summary: extractPurpose(mapData.content)
      });
      usedFileMaps.push(path.relative(options.repoRoot, mapPath));
    }
  }

  // Fallback: if no file MAPs exist but we have code files (e.g., all skipped due to size),
  // provide a minimal grouped summary at folder synthesis stage
  if (fileSummaries.length === 0 && codeFiles.length > 0) {
    const filesInfo = await Promise.all(codeFiles.map(async f => {
      const digest = await digestFile(f.path, f.relativePath);
      return {
        name: path.basename(f.path),
        language: digest.language,
        exports: digest.exports,
        lineCount: digest.lineCount
      };
    }));
    const prompt = fileGroupPrompt(filesInfo);
    const response = await llm.complete(prompt, {
      maxTokens: 2000,
      temperature: 0.3,
      systemPrompt: SYSTEM_PROMPT
    });
    // Extract collective purpose from markdown
    const collectivePurpose = extractSection(response, 'Collective Purpose');
    fileSummaries.push({
      name: `[${codeFiles.length} files - no individual MAPs due to size]`,
      isFolder: false,
      summary: collectivePurpose || 'Multiple code files in this folder.'
    });
  }

  // Generate folder summary
  const prompt = folderPrompt(
    folder.relativePath,
    fileSummaries,
    folderSummaries,
    synthesisBasis
  );
  const response = await llm.complete(prompt, {
    maxTokens: 8000,
    temperature: 0.3,
    systemPrompt: SYSTEM_PROMPT
  });

  const summary = parseFolderSummary(response);
  const content = renderFolderSummary(summary);

  // Determine confidence
  const hasChildMaps = usedChildMaps.length > 0 || usedFileMaps.length > 0;
  const confidence = hasChildMaps ? 'medium' : 'low';

  if (dryRun) {
    console.log(`[dry-run] Would write: ${outputPath}`);
    return { path: outputPath, scope: 'folder', success: true };
  }

  // P1 fix: compute repo-relative paths for frontmatter
  const repoRelativeFolderPath = path.relative(options.repoRoot, folder.path) || '.';
  const repoRelativeSourceFiles = codeFiles.map(f => path.relative(options.repoRoot, f.path));

  // Write the folder MAP
  await writeMap({
    folderPath: folder.path,
    content,
    scope: 'folder',
    relativePath: repoRelativeFolderPath,
    adapter: llm.adapterName,
    model: llm.modelName,
    basisCommit,
    synthesisBasis,
    confidence,
    childMaps: usedChildMaps.length > 0 ? usedChildMaps : undefined,
    sourceFiles: repoRelativeSourceFiles,
    fileMaps: usedFileMaps.length > 0 ? usedFileMaps : undefined,
    filename: 'MAP.md'
  });

  return { path: outputPath, scope: 'folder', success: true };
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Get local file MAP paths in a folder.
 */
async function getLocalFileMaps(folderPath: string): Promise<string[]> {
  try {
    const entries = await fs.readdir(folderPath, { withFileTypes: true });
    return entries
      .filter(e => e.isFile() && isFileMapFilename(e.name))
      .map(e => path.join(folderPath, e.name))
      .sort();
  } catch {
    return [];
  }
}

/**
 * Check if chunk artifacts referenced by a chunked file MAP are intact.
 *
 * Returns true if:
 * - The file MAP was not generated via chunk_rollup (no chunks to check)
 * - The file MAP was generated via chunk_rollup and all chunk artifacts exist
 *
 * Returns false if:
 * - The file MAP references chunk artifacts that no longer exist
 * - The file MAP cannot be read or parsed (triggers regeneration)
 */
async function areChunkArtifactsIntact(fileMapPath: string, folderPath: string): Promise<boolean> {
  try {
    const content = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(content);

    if (!artifact) {
      // Could not parse as file artifact - malformed or legacy format, regenerate
      return false;
    }

    if (artifact.synthesisMode !== 'chunk_rollup') {
      // Not a chunked file, no chunks to check
      return true;
    }

    if (!artifact.chunkBasis || artifact.chunkBasis.length === 0) {
      // No chunk basis recorded - inconsistent state, regenerate
      return false;
    }

    // Check that all referenced chunk artifacts exist
    // Chunk IDs are in format: {file_hash}:{chunk_index}:{line_start}-{line_end}
    // We need to derive the chunk filename from the source filename and chunk index
    const sourceBasename = path.basename(artifact.sourceFile);

    for (const chunkId of artifact.chunkBasis) {
      // Extract chunk index from ID (format: hash:index:lines)
      const parts = chunkId.split(':');
      if (parts.length < 2) {
        return false; // Invalid chunk ID format
      }
      const chunkIndex = parseInt(parts[1], 10);
      if (Number.isNaN(chunkIndex)) {
        return false;
      }

      const chunkFilename = chunkArtifactFilename(sourceBasename, chunkIndex);
      const chunkPath = path.join(folderPath, chunkFilename);

      try {
        await fs.access(chunkPath);
      } catch {
        // Chunk artifact does not exist
        return false;
      }
    }

    return true;
  } catch {
    // Error reading file MAP - cannot verify integrity, regenerate
    return false;
  }
}

/**
 * Extract purpose from MAP content (first paragraph or line).
 */
function extractPurpose(content: string): string {
  const trimmed = content.trim();
  // Take first paragraph (up to double newline)
  const firstPara = trimmed.split(/\n\n/)[0];
  // Limit to ~300 chars for parent synthesis
  return firstPara.length > 300 ? firstPara.slice(0, 300) + '...' : firstPara;
}

/**
 * Format graph context for prompt inclusion.
 */
function formatGraphContext(ctx: import('./types.js').GraphContext): string {
  const parts: string[] = [];

  if (ctx.symbols && ctx.symbols.length > 0) {
    parts.push('Symbols: ' + ctx.symbols.map(s => `${s.name} (${s.kind})`).join(', '));
  }
  if (ctx.imports_summary) {
    parts.push('Imports: ' + ctx.imports_summary);
  }
  if (ctx.exports_summary) {
    parts.push('Exports: ' + ctx.exports_summary);
  }
  if (ctx.callers_callees_summary) {
    parts.push('Callers/Callees: ' + ctx.callers_callees_summary);
  }
  if (ctx.owning_module) {
    parts.push('Module: ' + ctx.owning_module);
  }
  if (ctx.relevant_docs && ctx.relevant_docs.length > 0) {
    parts.push('Related docs: ' + ctx.relevant_docs.join(', '));
  }
  if (ctx.boundary_summary) {
    parts.push('Boundaries: ' + ctx.boundary_summary);
  }

  return parts.join('\n');
}
