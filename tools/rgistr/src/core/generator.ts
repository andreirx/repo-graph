/**
 * Recursive documentation generator.
 * Orchestrates the depth-first MAP.md generation process.
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { ILLMAdapter } from '../adapters/llm/index.js';
import type { FolderInfo, FileInfo, MapGenerationResult, GenerationConfig } from './types.js';
import { scanDirectory, getCodeFiles, getFoldersForGeneration } from './scanner.js';
import { writeMap, readMap, getGitCommit, isMapFresh } from './map-writer.js';
import { filePrompt, folderPrompt, repoPrompt } from './prompts.js';

export interface GeneratorOptions {
  /** LLM adapter to use */
  llm: ILLMAdapter;
  /** Generation configuration */
  config: GenerationConfig;
  /** Progress callback */
  onProgress?: (status: GenerationStatus) => void;
}

export interface GenerationStatus {
  phase: 'scanning' | 'generating' | 'complete';
  current?: string;
  processed: number;
  total: number;
  errors: number;
}

/**
 * Generate MAP.md files for a codebase.
 */
export async function generate(options: GeneratorOptions): Promise<MapGenerationResult[]> {
  const { llm, config, onProgress } = options;
  const results: MapGenerationResult[] = [];

  // Phase 1: Scan the directory tree
  onProgress?.({ phase: 'scanning', processed: 0, total: 0, errors: 0 });

  const root = await scanDirectory(config.rootPath, {
    maxDepth: config.maxDepth,
    maxFileSize: config.maxFileSize
  });

  // Get folders that need generation (depth-first order)
  // Include folders with existing MAPs so we can check staleness
  const folders = getFoldersForGeneration(root, config.force, true);
  const outputFilename = config.outputFilename || 'MAP.md';

  // Get git commit for provenance
  const basisCommit = await getGitCommit(config.rootPath);

  // Phase 2: Filter by staleness, then generate
  // Check which folders actually need regeneration
  const foldersToGenerate: FolderInfo[] = [];
  for (const folder of folders) {
    if (config.force || !folder.hasMap) {
      foldersToGenerate.push(folder);
    } else {
      // Has MAP - check if stale against code files AND child MAPs
      const mapPath = path.join(folder.path, outputFilename);
      const sourceFiles = getCodeFiles(folder).map(f => f.path);
      // Include child MAP files in freshness basis - parent summaries
      // are synthesized from child summaries
      const childMapFiles = folder.folders
        .filter(f => f.hasMap)
        .map(f => path.join(f.path, outputFilename));
      const allSources = [...sourceFiles, ...childMapFiles];
      const fresh = await isMapFresh(mapPath, allSources);
      if (!fresh) {
        foldersToGenerate.push(folder);
      }
      // else: MAP is fresh relative to all inputs, skip
    }
  }

  const total = foldersToGenerate.length;
  onProgress?.({ phase: 'generating', processed: 0, total, errors: 0 });

  // Generate MAPs depth-first
  // Because getFoldersForGeneration returns depth-first order,
  // child folders are processed before parents.
  for (let i = 0; i < foldersToGenerate.length; i++) {
    const folder = foldersToGenerate[i];
    const relativePath = folder.relativePath || '.';

    onProgress?.({
      phase: 'generating',
      current: relativePath,
      processed: i,
      total,
      errors: results.filter(r => !r.success).length
    });

    try {
      const result = await generateFolderMap(folder, {
        llm,
        rootPath: config.rootPath,
        basisCommit,
        outputFilename,
        dryRun: config.dryRun
      });
      results.push(result);
    } catch (error) {
      results.push({
        path: path.join(folder.path, outputFilename),
        scope: 'folder',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  onProgress?.({
    phase: 'complete',
    processed: total,
    total,
    errors: results.filter(r => !r.success).length
  });

  return results;
}

interface FolderMapOptions {
  llm: ILLMAdapter;
  rootPath: string;
  basisCommit: string | null;
  outputFilename?: string;
  dryRun?: boolean;
}

/**
 * Generate a MAP.md for a single folder.
 */
async function generateFolderMap(
  folder: FolderInfo,
  options: FolderMapOptions
): Promise<MapGenerationResult> {
  const { llm, rootPath, basisCommit, outputFilename = 'MAP.md', dryRun } = options;
  const codeFiles = getCodeFiles(folder);
  const childSummaries: { name: string; summary: string; isFolder: boolean }[] = [];

  // Collect child MAP.md summaries from subfolders
  for (const sub of folder.folders) {
    const subMapPath = path.join(sub.path, outputFilename);
    const subMap = await readMap(subMapPath);
    if (subMap) {
      childSummaries.push({
        name: path.basename(sub.path),
        summary: subMap.content.slice(0, 500), // Truncate for context
        isFolder: true
      });
    }
  }

  // Summarize code files
  // For folders with few files, summarize each
  // For folders with many files, summarize in groups
  if (codeFiles.length <= 5) {
    for (const file of codeFiles) {
      const content = await fs.readFile(file.path, 'utf-8');
      const summary = await summarizeFile(llm, file, content, {
        siblingFiles: codeFiles.map(f => path.basename(f.path))
      });
      childSummaries.push({
        name: path.basename(file.path),
        summary,
        isFolder: false
      });
    }
  } else {
    // Group summarization for larger folders
    const groupSummary = await summarizeFileGroup(llm, codeFiles);
    childSummaries.push({
      name: `[${codeFiles.length} files]`,
      summary: groupSummary,
      isFolder: false
    });
  }

  // Generate folder summary
  const prompt = folderPrompt(folder.relativePath, childSummaries);
  const folderSummary = await llm.complete(prompt, { maxTokens: 1000 });

  // Determine confidence based on what we had available
  const hasChildMaps = folder.folders.some(f => f.hasMap);
  const confidence = hasChildMaps ? 'medium' : 'low';

  if (dryRun) {
    console.log(`[dry-run] Would write: ${path.join(folder.path, outputFilename)}`);
    return {
      path: path.join(folder.path, outputFilename),
      scope: 'folder',
      success: true
    };
  }

  // Write the MAP.md
  const outputPath = await writeMap({
    folderPath: folder.path,
    content: folderSummary,
    scope: 'folder',
    relativePath: folder.relativePath,
    adapter: llm.adapterName,
    model: llm.modelName,
    basisCommit,
    synthesisBasis: 'code_only',
    confidence,
    childMaps: folder.folders
      .filter(f => f.hasMap)
      .map(f => path.join(f.relativePath, outputFilename)),
    sourceFiles: codeFiles.map(f => f.relativePath),
    filename: outputFilename
  });

  return {
    path: outputPath,
    scope: 'folder',
    success: true
  };
}

/**
 * Summarize a single file.
 */
async function summarizeFile(
  llm: ILLMAdapter,
  file: FileInfo,
  content: string,
  context?: { folderPurpose?: string; siblingFiles?: string[] }
): Promise<string> {
  // Truncate very long files
  const truncatedContent = content.length > 8000
    ? content.slice(0, 8000) + '\n\n[... truncated ...]'
    : content;

  const prompt = filePrompt(file.relativePath, truncatedContent, context);
  return llm.complete(prompt, { maxTokens: 500 });
}

/**
 * Summarize a group of files together.
 */
async function summarizeFileGroup(
  llm: ILLMAdapter,
  files: FileInfo[]
): Promise<string> {
  const fileList = files
    .map(f => `- ${path.basename(f.path)} (${f.size} bytes)`)
    .join('\n');

  const prompt = `Briefly describe what this group of ${files.length} code files likely contains based on their names:

${fileList}

Provide a 2-3 sentence summary of what this collection of files appears to handle.
Do not speculate on implementation details - just describe the apparent scope.`;

  return llm.complete(prompt, { maxTokens: 200 });
}
