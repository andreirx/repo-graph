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
import { digestFile, formatDigestForPrompt } from './predigest.js';
import {
  SYSTEM_PROMPT,
  filePromptWhole,
  filePromptDigest,
  folderPrompt,
  fileGroupPrompt,
  parseFileSummary,
  parseFolderSummary,
  renderFileSummary,
  renderFolderSummary,
  extractSection,
  type FileSummarySchema,
  type FolderSummarySchema,
  type ChildSummary
} from './prompts.js';

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

/** Maximum files to summarize individually in a folder */
const MAX_INDIVIDUAL_FILES = 8;

/** Maximum file size for whole-file summarization (bytes) - use digest above this */
const MAX_FILE_SIZE_WHOLE = 100 * 1024; // 100KB

/** Absolute maximum file size for any summarization (bytes) */
const MAX_FILE_SIZE_FOR_SUMMARY = 500 * 1024; // 500KB

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

  // Count total work items
  let totalFiles = 0;
  for (const folder of folders) {
    const codeFiles = getCodeFiles(folder);
    if (codeFiles.length <= MAX_INDIVIDUAL_FILES) {
      totalFiles += codeFiles.length;
    }
  }
  const totalFolders = folders.length;
  const total = totalFiles + totalFolders;

  let processed = 0;

  // Phase 2: Generate file MAPs (depth-first, files before folders)
  onProgress?.({ phase: 'generating-files', processed: 0, total, errors: 0 });

  for (const folder of folders) {
    const codeFiles = getCodeFiles(folder);

    // Only generate individual file MAPs for manageable folders
    if (codeFiles.length <= MAX_INDIVIDUAL_FILES) {
      for (const file of codeFiles) {
        // Skip large files
        if (file.size > MAX_FILE_SIZE_FOR_SUMMARY) continue;

        const fileMapName = fileMapFilename(path.basename(file.path));
        const fileMapPath = path.join(folder.path, fileMapName);

        // Check freshness
        if (!config.force) {
          const fresh = await isMapFresh(fileMapPath, [file.path]);
          if (fresh) continue;
        }

        onProgress?.({
          phase: 'generating-files',
          current: file.relativePath,
          processed,
          total,
          errors: results.filter(r => !r.success).length
        });

        try {
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

  // Choose prompting strategy based on file size
  let prompt: string;
  if (file.size <= MAX_FILE_SIZE_WHOLE) {
    // Default: feed whole file for 1M context models
    prompt = filePromptWhole(repoRelativePath, content, language, synthesisBasis, graphContext);
  } else {
    // Fallback: use pre-digest for oversized files
    const digest = await digestFile(file.path, repoRelativePath);
    prompt = filePromptDigest(digest, synthesisBasis);
  }

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

  // If no file MAPs exist but we have code files, summarize them inline
  if (fileSummaries.length === 0 && codeFiles.length > 0) {
    if (codeFiles.length <= MAX_INDIVIDUAL_FILES) {
      // Summarize each file inline (for folders without file MAPs)
      for (const file of codeFiles) {
        const content = await fs.readFile(file.path, 'utf-8');
        const ext = path.extname(file.path).slice(1);
        const language = detectLanguage(ext);

        let prompt: string;
        if (file.size <= MAX_FILE_SIZE_WHOLE) {
          prompt = filePromptWhole(file.relativePath, content, language, synthesisBasis);
        } else {
          const digest = await digestFile(file.path, file.relativePath);
          prompt = filePromptDigest(digest, synthesisBasis);
        }

        const response = await llm.complete(prompt, {
          maxTokens: 4000,
          temperature: 0.3,
          systemPrompt: SYSTEM_PROMPT
        });
        const summary = parseFileSummary(response);
        fileSummaries.push({
          name: path.basename(file.path),
          isFolder: false,
          summary: summary.purpose
        });
      }
    } else {
      // Large folder - group summary
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
        name: `[${codeFiles.length} files]`,
        isFolder: false,
        summary: collectivePurpose || 'Multiple code files in this folder.'
      });
    }
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
