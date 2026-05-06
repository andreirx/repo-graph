/**
 * Chunking support module tests.
 */

import { describe, it, expect } from 'vitest';
import {
  // Identity
  computeFileHash,
  createChunkIdentity,
  formatChunkId,
  parseChunkId,
  chunkArtifactFilename,
  fileArtifactFilename,
  parseArtifactFilename,
  isChunkArtifactStale,
  isFileArtifactStale,

  // Planner
  planChunks,
  extractChunkContent,
  countLines,

  // Artifacts
  getGeneratorString,
  serializeChunkArtifact,
  parseChunkArtifact,
  serializeFileArtifact,
  parseFileArtifact,
  serializeFolderArtifact,
  parseFolderArtifact,

  // Types
  type ChunkArtifact,
  type FileArtifact,
  type FolderArtifact,
} from './index.js';

// ══════════════════════════════════════════════════════════════════════
// Identity tests
// ══════════════════════════════════════════════════════════════════════

describe('computeFileHash', () => {
  it('returns 8-char hex string', () => {
    const hash = computeFileHash('hello world');
    expect(hash).toMatch(/^[a-f0-9]{8}$/);
  });

  it('is deterministic', () => {
    const h1 = computeFileHash('test content');
    const h2 = computeFileHash('test content');
    expect(h1).toBe(h2);
  });

  it('differs for different content', () => {
    const h1 = computeFileHash('content A');
    const h2 = computeFileHash('content B');
    expect(h1).not.toBe(h2);
  });
});

describe('createChunkIdentity', () => {
  it('creates identity with correct fields', () => {
    const identity = createChunkIdentity('abcd1234', 0, 3, {
      startLine: 1,
      endLine: 50,
    });

    expect(identity.fileHash).toBe('abcd1234');
    expect(identity.chunkIndex).toBe(0);
    expect(identity.chunkCount).toBe(3);
    expect(identity.span.startLine).toBe(1);
    expect(identity.span.endLine).toBe(50);
    expect(identity.id).toBe('abcd1234:0:1-50');
  });
});

describe('formatChunkId', () => {
  it('formats correctly', () => {
    const id = formatChunkId('abcd1234', 2, { startLine: 101, endLine: 150 });
    expect(id).toBe('abcd1234:2:101-150');
  });
});

describe('parseChunkId', () => {
  it('parses valid chunk ID', () => {
    const parsed = parseChunkId('abcd1234:2:101-150');
    expect(parsed).toEqual({
      fileHash: 'abcd1234',
      chunkIndex: 2,
      startLine: 101,
      endLine: 150,
    });
  });

  it('returns null for invalid format', () => {
    expect(parseChunkId('invalid')).toBeNull();
    expect(parseChunkId('abc:1:2-3')).toBeNull(); // hash too short
    expect(parseChunkId('abcd1234:x:2-3')).toBeNull(); // non-numeric index
    expect(parseChunkId('abcd1234:1:2')).toBeNull(); // missing end line
  });
});

describe('chunkArtifactFilename', () => {
  it('generates correct filename', () => {
    expect(chunkArtifactFilename('parser.ts', 0)).toBe('parser.ts.chunk-0.gist.md');
    expect(chunkArtifactFilename('parser.ts', 5)).toBe('parser.ts.chunk-5.gist.md');
  });
});

describe('fileArtifactFilename', () => {
  it('generates correct filename', () => {
    expect(fileArtifactFilename('parser.ts')).toBe('parser.ts.gist.md');
  });
});

describe('parseArtifactFilename', () => {
  it('parses chunk artifact filename', () => {
    const parsed = parseArtifactFilename('parser.ts.chunk-2.gist.md');
    expect(parsed).toEqual({
      sourceFilename: 'parser.ts',
      scope: 'chunk',
      chunkIndex: 2,
    });
  });

  it('parses file artifact filename', () => {
    const parsed = parseArtifactFilename('parser.ts.gist.md');
    expect(parsed).toEqual({
      sourceFilename: 'parser.ts',
      scope: 'file',
    });
  });

  it('returns null for unrecognized format', () => {
    expect(parseArtifactFilename('parser.ts')).toBeNull();
    expect(parseArtifactFilename('README.md')).toBeNull();
  });
});

describe('staleness detection', () => {
  it('detects stale chunk artifact', () => {
    expect(isChunkArtifactStale('abcd1234', 'abcd1234')).toBe(false);
    expect(isChunkArtifactStale('abcd1234', 'efgh5678')).toBe(true);
  });

  it('detects stale file artifact', () => {
    expect(isFileArtifactStale('abcd1234', 'abcd1234')).toBe(false);
    expect(isFileArtifactStale('abcd1234', 'efgh5678')).toBe(true);
  });
});

// ══════════════════════════════════════════════════════════════════════
// Planner tests
// ══════════════════════════════════════════════════════════════════════

describe('planChunks', () => {
  it('returns whole_file method for small files', () => {
    const content = 'function hello() { return "world"; }';
    const plan = planChunks('test.ts', content, { modelId: 'gpt-4o' });

    expect(plan.requiresChunking).toBe(false);
    expect(plan.method).toBe('whole_file');
    expect(plan.chunks).toHaveLength(0);
    expect(plan.filePath).toBe('test.ts');
  });

  it('chunks large files', () => {
    // Generate a very large file - need to exceed ~100K tokens for GPT-4o
    // Each line is roughly 15-20 tokens. Need many lines with substantial content.
    const lines = Array.from({ length: 10000 }, (_, i) =>
      `const someVeryLongVariableName${i} = "this is a long string value with some extra text to increase token count ${i}"; // additional padding comment with more text here for extra tokens`
    );
    const content = lines.join('\n');

    const plan = planChunks('large.ts', content, { modelId: 'gpt-4o' });

    expect(plan.requiresChunking).toBe(true);
    expect(plan.method).toBe('line_window');
    expect(plan.chunks.length).toBeGreaterThan(1);

    // All chunks should have valid identities
    for (const chunk of plan.chunks) {
      expect(chunk.identity.fileHash).toBe(plan.fileHash);
      expect(chunk.identity.chunkCount).toBe(plan.chunks.length);
    }
  });

  it('includes file hash in plan', () => {
    const content = 'const x = 1;';
    const plan = planChunks('test.ts', content, { modelId: 'gpt-4o' });

    expect(plan.fileHash).toMatch(/^[a-f0-9]{8}$/);
    expect(plan.fileHash).toBe(computeFileHash(content));
  });
});

describe('extractChunkContent', () => {
  it('extracts correct lines (1-based inclusive)', () => {
    const content = 'line1\nline2\nline3\nline4\nline5';

    expect(extractChunkContent(content, { startLine: 1, endLine: 2 })).toBe('line1\nline2');
    expect(extractChunkContent(content, { startLine: 2, endLine: 4 })).toBe('line2\nline3\nline4');
    expect(extractChunkContent(content, { startLine: 5, endLine: 5 })).toBe('line5');
  });

  it('handles single line', () => {
    const content = 'only line';
    expect(extractChunkContent(content, { startLine: 1, endLine: 1 })).toBe('only line');
  });
});

describe('countLines', () => {
  it('counts lines correctly', () => {
    expect(countLines('')).toBe(1); // empty string has 1 line
    expect(countLines('a')).toBe(1);
    expect(countLines('a\nb')).toBe(2);
    expect(countLines('a\nb\nc')).toBe(3);
    expect(countLines('a\n')).toBe(2); // trailing newline counts
  });
});

// ══════════════════════════════════════════════════════════════════════
// Artifact serialization tests
// ══════════════════════════════════════════════════════════════════════

describe('getGeneratorString', () => {
  it('returns versioned generator name', () => {
    const gen = getGeneratorString();
    expect(gen).toMatch(/^rgistr@\d+\.\d+\.\d+$/);
  });
});

describe('chunk artifact serialization', () => {
  const artifact: ChunkArtifact = {
    scope: 'chunk',
    sourceFile: 'src/parser.ts',
    sourceSpan: { startLine: 1, endLine: 50 },
    chunkId: 'abcd1234:0:1-50',
    chunkIndex: 0,
    chunkCount: 3,
    content: '## Summary\n\nThis chunk handles parsing.',
    generatedAt: '2025-01-15T10:00:00Z',
    model: 'gpt-4o',
    provider: 'openai',
    sourceTokens: 500,
    outputTokens: 100,
  };

  it('serializes to markdown with frontmatter', () => {
    const md = serializeChunkArtifact(artifact);

    expect(md).toContain('---');
    expect(md).toContain('scope: chunk');
    expect(md).toContain('source_file: src/parser.ts');
    expect(md).toContain('source_line_start: 1');
    expect(md).toContain('source_line_end: 50');
    expect(md).toContain('chunk_index: 0');
    expect(md).toContain('chunk_count: 3');
    expect(md).toContain('chunk_id: "abcd1234:0:1-50"'); // quoted due to colons
    expect(md).toContain('model: gpt-4o');
    expect(md).toContain('source_tokens: 500');
    expect(md).toContain('output_tokens: 100');
    expect(md).toContain('## Summary');
  });

  it('roundtrips correctly', () => {
    const md = serializeChunkArtifact(artifact);
    const parsed = parseChunkArtifact(md);

    expect(parsed).not.toBeNull();
    expect(parsed!.scope).toBe('chunk');
    expect(parsed!.sourceFile).toBe('src/parser.ts');
    expect(parsed!.sourceSpan.startLine).toBe(1);
    expect(parsed!.sourceSpan.endLine).toBe(50);
    expect(parsed!.chunkId).toBe('abcd1234:0:1-50');
    expect(parsed!.chunkIndex).toBe(0);
    expect(parsed!.chunkCount).toBe(3);
    expect(parsed!.content).toBe('## Summary\n\nThis chunk handles parsing.');
    expect(parsed!.model).toBe('gpt-4o');
    expect(parsed!.sourceTokens).toBe(500);
    expect(parsed!.outputTokens).toBe(100);
  });

  it('returns null for non-chunk scope', () => {
    const md = serializeChunkArtifact(artifact).replace('scope: chunk', 'scope: file');
    expect(parseChunkArtifact(md)).toBeNull();
  });

  it('returns null for missing frontmatter', () => {
    expect(parseChunkArtifact('# No frontmatter')).toBeNull();
  });
});

describe('file artifact serialization', () => {
  const artifact: FileArtifact = {
    scope: 'file',
    sourceFile: 'src/parser.ts',
    fileHash: 'abcd1234',
    synthesisMode: 'chunk_rollup',
    chunkBasis: ['abcd1234:0:1-50', 'abcd1234:1:45-100'],
    content: '## File Summary\n\nParser implementation.',
    generatedAt: '2025-01-15T10:00:00Z',
    model: 'gpt-4o',
    provider: 'openai',
    uncertaintyNotes: ['Cross-chunk calls may be incomplete'],
  };

  it('serializes with array fields', () => {
    const md = serializeFileArtifact(artifact);

    expect(md).toContain('scope: file');
    expect(md).toContain('synthesis_mode: chunk_rollup');
    expect(md).toContain('chunk_basis:');
    expect(md).toContain('- "abcd1234:0:1-50"'); // quoted due to colons
    expect(md).toContain('- "abcd1234:1:45-100"'); // quoted due to colons
    expect(md).toContain('uncertainty_notes:');
    expect(md).toContain('- Cross-chunk calls may be incomplete');
  });

  it('roundtrips correctly', () => {
    const md = serializeFileArtifact(artifact);
    const parsed = parseFileArtifact(md);

    expect(parsed).not.toBeNull();
    expect(parsed!.scope).toBe('file');
    expect(parsed!.sourceFile).toBe('src/parser.ts');
    expect(parsed!.fileHash).toBe('abcd1234');
    expect(parsed!.synthesisMode).toBe('chunk_rollup');
    expect(parsed!.chunkBasis).toEqual(['abcd1234:0:1-50', 'abcd1234:1:45-100']);
    expect(parsed!.uncertaintyNotes).toEqual(['Cross-chunk calls may be incomplete']);
    expect(parsed!.content).toBe('## File Summary\n\nParser implementation.');
  });

  it('handles whole_file mode without chunk basis', () => {
    const wholeFileArtifact: FileArtifact = {
      ...artifact,
      synthesisMode: 'whole_file',
      chunkBasis: undefined,
      uncertaintyNotes: undefined,
    };

    const md = serializeFileArtifact(wholeFileArtifact);
    const parsed = parseFileArtifact(md);

    expect(parsed!.synthesisMode).toBe('whole_file');
    expect(parsed!.chunkBasis).toBeUndefined(); // not present in YAML
  });
});

describe('folder artifact serialization', () => {
  const artifact: FolderArtifact = {
    scope: 'folder',
    folderPath: 'src/core',
    fileBasis: ['parser.ts', 'lexer.ts', 'ast.ts'],
    childFolderBasis: ['utils'],
    content: '## Core Module\n\nParser and lexer implementation.',
    generatedAt: '2025-01-15T10:00:00Z',
    model: 'gpt-4o',
    provider: 'openai',
  };

  it('serializes folder artifact', () => {
    const md = serializeFolderArtifact(artifact);

    expect(md).toContain('scope: folder');
    expect(md).toContain('folder_path: src/core');
    expect(md).toContain('file_basis:');
    expect(md).toContain('- parser.ts');
    expect(md).toContain('child_folder_basis:');
    expect(md).toContain('- utils');
  });

  it('roundtrips correctly', () => {
    const md = serializeFolderArtifact(artifact);
    const parsed = parseFolderArtifact(md);

    expect(parsed).not.toBeNull();
    expect(parsed!.scope).toBe('folder');
    expect(parsed!.folderPath).toBe('src/core');
    expect(parsed!.fileBasis).toEqual(['parser.ts', 'lexer.ts', 'ast.ts']);
    expect(parsed!.childFolderBasis).toEqual(['utils']);
    expect(parsed!.content).toBe('## Core Module\n\nParser and lexer implementation.');
  });
});

describe('content whitespace preservation', () => {
  it('preserves leading newlines in content', () => {
    const artifact: ChunkArtifact = {
      scope: 'chunk',
      sourceFile: 'test.ts',
      sourceSpan: { startLine: 1, endLine: 10 },
      chunkId: 'abcd1234:0:1-10',
      chunkIndex: 0,
      chunkCount: 1,
      content: '\nlead\ntrail\n',
      generatedAt: '2025-01-15T10:00:00Z',
      model: 'gpt-4o',
      provider: 'openai',
      sourceTokens: 50,
      outputTokens: 10,
    };

    const md = serializeChunkArtifact(artifact);
    const parsed = parseChunkArtifact(md);

    expect(parsed!.content).toBe('\nlead\ntrail\n');
  });

  it('preserves trailing whitespace in content', () => {
    const artifact: FileArtifact = {
      scope: 'file',
      sourceFile: 'test.ts',
      fileHash: 'abcd1234',
      synthesisMode: 'whole_file',
      content: 'content with trailing spaces   \nand newlines\n\n',
      generatedAt: '2025-01-15T10:00:00Z',
      model: 'gpt-4o',
      provider: 'openai',
    };

    const md = serializeFileArtifact(artifact);
    const parsed = parseFileArtifact(md);

    expect(parsed!.content).toBe('content with trailing spaces   \nand newlines\n\n');
  });

  it('preserves code fence at start of content', () => {
    const artifact: ChunkArtifact = {
      scope: 'chunk',
      sourceFile: 'test.ts',
      sourceSpan: { startLine: 1, endLine: 10 },
      chunkId: 'abcd1234:0:1-10',
      chunkIndex: 0,
      chunkCount: 1,
      content: '```typescript\nconst x = 1;\n```',
      generatedAt: '2025-01-15T10:00:00Z',
      model: 'gpt-4o',
      provider: 'openai',
      sourceTokens: 50,
      outputTokens: 10,
    };

    const md = serializeChunkArtifact(artifact);
    const parsed = parseChunkArtifact(md);

    expect(parsed!.content).toBe('```typescript\nconst x = 1;\n```');
  });
});

describe('YAML edge cases', () => {
  it('handles strings with colons', () => {
    const artifact: FileArtifact = {
      scope: 'file',
      sourceFile: 'src/time:based:parser.ts', // colons in path
      fileHash: 'abcd1234',
      synthesisMode: 'whole_file',
      content: 'Content here',
      generatedAt: '2025-01-15T10:00:00Z',
      model: 'gpt-4o',
      provider: 'openai',
    };

    const md = serializeFileArtifact(artifact);
    const parsed = parseFileArtifact(md);

    // Should be quoted in YAML
    expect(md).toContain('"src/time:based:parser.ts"');
    expect(parsed!.sourceFile).toBe('src/time:based:parser.ts');
  });

  it('handles empty arrays', () => {
    const artifact: FolderArtifact = {
      scope: 'folder',
      folderPath: 'src',
      fileBasis: [],
      childFolderBasis: [],
      content: 'Empty folder',
      generatedAt: '2025-01-15T10:00:00Z',
      model: 'gpt-4o',
      provider: 'openai',
    };

    const md = serializeFolderArtifact(artifact);
    expect(md).toContain('file_basis: []');

    const parsed = parseFolderArtifact(md);
    expect(parsed!.fileBasis).toEqual([]);
  });
});
