/**
 * Artifact serialization.
 *
 * Functions for reading and writing chunk/file/folder artifacts
 * with YAML frontmatter.
 */

import type {
  ChunkArtifact,
  FileArtifact,
  FolderArtifact,
  ChunkArtifactFrontmatter,
  FileArtifactFrontmatter,
  FolderArtifactFrontmatter,
} from './types.js';

// ── Generator metadata ───────────────────────────────────────────────

const GENERATOR_NAME = 'rgistr';
const GENERATOR_VERSION = '0.2.0';

export function getGeneratorString(): string {
  return `${GENERATOR_NAME}@${GENERATOR_VERSION}`;
}

// ── Frontmatter serialization ────────────────────────────────────────

/**
 * Serialize value to YAML-safe string.
 */
function yamlValue(value: unknown): string {
  if (typeof value === 'string') {
    // Quote strings that might be ambiguous
    if (value.includes(':') || value.includes('#') || value.includes('\n') ||
        value.startsWith(' ') || value.endsWith(' ')) {
      return JSON.stringify(value);
    }
    return value;
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return '[]';
    }
    return '\n' + value.map(v => `  - ${yamlValue(v)}`).join('\n');
  }
  return JSON.stringify(value);
}

/**
 * Build YAML frontmatter string.
 */
function buildFrontmatter(fields: Record<string, unknown>): string {
  const lines: string[] = ['---'];

  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined || value === null) {
      continue;
    }
    const yamlKey = key; // Keys are already snake_case
    const yamlVal = yamlValue(value);

    if (yamlVal.startsWith('\n')) {
      // Array with items on separate lines
      lines.push(`${yamlKey}:${yamlVal}`);
    } else {
      lines.push(`${yamlKey}: ${yamlVal}`);
    }
  }

  lines.push('---');
  return lines.join('\n');
}

// ── Chunk artifact serialization ─────────────────────────────────────

/**
 * Serialize a chunk artifact to markdown with frontmatter.
 */
export function serializeChunkArtifact(artifact: ChunkArtifact): string {
  const frontmatter: ChunkArtifactFrontmatter = {
    scope: 'chunk',
    source_file: artifact.sourceFile,
    source_line_start: artifact.sourceSpan.startLine,
    source_line_end: artifact.sourceSpan.endLine,
    chunk_index: artifact.chunkIndex,
    chunk_count: artifact.chunkCount,
    chunk_id: artifact.chunkId,
    file_hash: artifact.chunkId.split(':')[0], // Extract from chunk ID
    generated_at: artifact.generatedAt,
    generator: getGeneratorString(),
    model: artifact.model,
    provider: artifact.provider,
    source_tokens: artifact.sourceTokens,
    output_tokens: artifact.outputTokens,
  };

  return `${buildFrontmatter(frontmatter as unknown as Record<string, unknown>)}\n\n${artifact.content}`;
}

/**
 * Parse a chunk artifact from markdown with frontmatter.
 */
export function parseChunkArtifact(markdown: string): ChunkArtifact | null {
  const parsed = parseFrontmatter(markdown);
  if (!parsed || parsed.frontmatter.scope !== 'chunk') {
    return null;
  }

  const fm = parsed.frontmatter as unknown as ChunkArtifactFrontmatter;

  return {
    scope: 'chunk',
    sourceFile: fm.source_file,
    sourceSpan: {
      startLine: fm.source_line_start,
      endLine: fm.source_line_end,
    },
    chunkId: fm.chunk_id,
    chunkIndex: fm.chunk_index,
    chunkCount: fm.chunk_count,
    content: parsed.content,
    generatedAt: fm.generated_at,
    model: fm.model,
    provider: fm.provider,
    sourceTokens: fm.source_tokens,
    outputTokens: fm.output_tokens
  };
}

// ── File artifact serialization ──────────────────────────────────────

/**
 * Serialize a file artifact to markdown with frontmatter.
 */
export function serializeFileArtifact(artifact: FileArtifact): string {
  const frontmatter: FileArtifactFrontmatter = {
    scope: 'file',
    source_file: artifact.sourceFile,
    file_hash: artifact.fileHash,
    synthesis_mode: artifact.synthesisMode,
    chunk_basis: artifact.chunkBasis,
    generated_at: artifact.generatedAt,
    generator: getGeneratorString(),
    model: artifact.model,
    provider: artifact.provider,
    uncertainty_notes: artifact.uncertaintyNotes,
  };

  return `${buildFrontmatter(frontmatter as unknown as Record<string, unknown>)}\n\n${artifact.content}`;
}

/**
 * Parse a file artifact from markdown with frontmatter.
 */
export function parseFileArtifact(markdown: string): FileArtifact | null {
  const parsed = parseFrontmatter(markdown);
  if (!parsed || parsed.frontmatter.scope !== 'file') {
    return null;
  }

  const fm = parsed.frontmatter as unknown as FileArtifactFrontmatter;

  return {
    scope: 'file',
    sourceFile: fm.source_file,
    fileHash: fm.file_hash,
    synthesisMode: fm.synthesis_mode,
    chunkBasis: fm.chunk_basis,
    content: parsed.content,
    generatedAt: fm.generated_at,
    model: fm.model,
    provider: fm.provider,
    uncertaintyNotes: fm.uncertainty_notes,
  };
}

// ── Folder artifact serialization ────────────────────────────────────

/**
 * Serialize a folder artifact (MAP.md) to markdown with frontmatter.
 */
export function serializeFolderArtifact(artifact: FolderArtifact): string {
  const frontmatter: FolderArtifactFrontmatter = {
    scope: 'folder',
    folder_path: artifact.folderPath,
    file_basis: artifact.fileBasis,
    child_folder_basis: artifact.childFolderBasis,
    generated_at: artifact.generatedAt,
    generator: getGeneratorString(),
    model: artifact.model,
    provider: artifact.provider,
  };

  return `${buildFrontmatter(frontmatter as unknown as Record<string, unknown>)}\n\n${artifact.content}`;
}

/**
 * Parse a folder artifact from markdown with frontmatter.
 */
export function parseFolderArtifact(markdown: string): FolderArtifact | null {
  const parsed = parseFrontmatter(markdown);
  if (!parsed || parsed.frontmatter.scope !== 'folder') {
    return null;
  }

  const fm = parsed.frontmatter as unknown as FolderArtifactFrontmatter;

  return {
    scope: 'folder',
    folderPath: fm.folder_path,
    fileBasis: fm.file_basis,
    childFolderBasis: fm.child_folder_basis,
    content: parsed.content,
    generatedAt: fm.generated_at,
    model: fm.model,
    provider: fm.provider,
  };
}

// ── Generic frontmatter parsing ──────────────────────────────────────

/**
 * Parse YAML frontmatter from markdown.
 *
 * Returns null if no valid frontmatter is found.
 */
function parseFrontmatter(markdown: string): {
  frontmatter: Record<string, unknown>;
  content: string;
} | null {
  if (!markdown.startsWith('---')) {
    return null;
  }

  const endIndex = markdown.indexOf('\n---', 3);
  if (endIndex === -1) {
    return null;
  }

  const frontmatterStr = markdown.slice(4, endIndex);
  // After closing `---\n`, we have `\n{content}` due to the blank line we add
  // during serialization. Remove exactly one leading newline if present.
  let content = markdown.slice(endIndex + 5); // skip `\n---\n`
  if (content.startsWith('\n')) {
    content = content.slice(1); // remove the blank line separator
  }

  try {
    const frontmatter = parseSimpleYaml(frontmatterStr);
    return { frontmatter, content };
  } catch {
    return null;
  }
}

/**
 * Parse simple YAML (subset needed for artifact frontmatter).
 *
 * Supports: strings, numbers, booleans, arrays of strings.
 */
function parseSimpleYaml(yaml: string): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const lines = yaml.split('\n');
  let currentKey: string | null = null;
  let currentArray: string[] | null = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }

    // Array item
    if (trimmed.startsWith('- ')) {
      if (currentKey && currentArray) {
        const value = trimmed.slice(2).trim();
        currentArray.push(parseYamlValue(value) as string);
      }
      continue;
    }

    // Key-value pair
    const colonIndex = trimmed.indexOf(':');
    if (colonIndex > 0) {
      // Save previous array if any
      if (currentKey && currentArray) {
        result[currentKey] = currentArray;
        currentArray = null;
      }

      const key = trimmed.slice(0, colonIndex).trim();
      const valueStr = trimmed.slice(colonIndex + 1).trim();

      if (valueStr === '' || valueStr === '[]') {
        // Empty value or empty array - start collecting array items
        currentKey = key;
        currentArray = [];
      } else {
        result[key] = parseYamlValue(valueStr);
        currentKey = null;
        currentArray = null;
      }
    }
  }

  // Save last array if any
  if (currentKey && currentArray) {
    result[currentKey] = currentArray;
  }

  return result;
}

/**
 * Parse a simple YAML value.
 */
function parseYamlValue(str: string): string | number | boolean {
  // Quoted string
  if ((str.startsWith('"') && str.endsWith('"')) ||
      (str.startsWith("'") && str.endsWith("'"))) {
    return str.slice(1, -1);
  }

  // Boolean
  if (str === 'true') return true;
  if (str === 'false') return false;

  // Number
  const num = Number(str);
  if (!Number.isNaN(num) && str !== '') {
    return num;
  }

  // Plain string
  return str;
}
