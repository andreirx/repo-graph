/**
 * Generator integration tests.
 *
 * Tests the chunked file generation path and freshness detection.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import type { ILLMAdapter, CompletionOptions } from '../adapters/llm/index.js';
import { generate } from './generator.js';
import { parseFileArtifact } from '../support/chunking/index.js';

// ── Mock LLM Adapter ─────────────────────────────────────────────────────────

/**
 * Mock LLM adapter that returns predictable responses.
 */
class MockLLMAdapter implements ILLMAdapter {
  readonly modelName = 'mock-model';
  readonly adapterName = 'mock';

  private callCount = 0;

  async complete(prompt: string, _options?: CompletionOptions): Promise<string> {
    this.callCount++;

    // Detect prompt type and return appropriate mock response
    if (prompt.includes('INPUT_MODE: chunk')) {
      // Chunk prompt
      const chunkMatch = prompt.match(/CHUNK: (\d+) of (\d+)/);
      const chunkNum = chunkMatch ? chunkMatch[1] : '?';
      return `# Purpose
Mock chunk ${chunkNum} summary for testing.

# Key Symbols
- mockSymbol${chunkNum}

# Notable Dependencies
- mockDep

# Cross-Chunk References
- None visible

# Policy Signals
- None

# Uncertainty
This is a mock response for testing.`;
    }

    if (prompt.includes('INPUT_MODE: chunk_rollup')) {
      // Rollup prompt
      return `# Purpose
Mock file summary from chunk rollup.

# Key Symbols
- mainSymbol

# Notable Dependencies
- mockDep

# Likely Change Reasons
- Testing changes

# Reading Hint
Start with chunk 1.

# Policy Signals
None

# Chunking Notes
File was processed in multiple chunks.

# Uncertainty
Cross-chunk visibility limited.`;
    }

    if (prompt.includes('Summarize this source file')) {
      // Whole file or digest prompt
      return `# Purpose
Mock file summary.

# Key Symbols
- testSymbol

# Notable Dependencies
- mockDep

# Likely Change Reasons
- Testing

# Reading Hint
Read from top.

# Policy Signals
None

# Uncertainty
None`;
    }

    if (prompt.includes('Synthesize a summary of this folder')) {
      // Folder prompt
      return `# Purpose
Mock folder summary.

# Structure
Contains test files.

# Key Components
- testFile

# Seams
- None

# Policy Seams
None

# Reading Order
Start with main file.

# Uncertainty
None`;
    }

    // Default response
    return '# Purpose\nMock response.';
  }

  async testConnection(): Promise<boolean> {
    return true;
  }

  getCallCount(): number {
    return this.callCount;
  }

  resetCallCount(): void {
    this.callCount = 0;
  }
}

// ── Test Fixtures ────────────────────────────────────────────────────────────

let tempDir: string;
let mockLlm: MockLLMAdapter;

beforeEach(async () => {
  tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'rgistr-test-'));
  mockLlm = new MockLLMAdapter();
});

afterEach(async () => {
  await fs.rm(tempDir, { recursive: true, force: true });
});

// ── Helper Functions ─────────────────────────────────────────────────────────

async function createTestFile(relativePath: string, content: string): Promise<string> {
  const fullPath = path.join(tempDir, relativePath);
  await fs.mkdir(path.dirname(fullPath), { recursive: true });
  await fs.writeFile(fullPath, content, 'utf-8');
  return fullPath;
}

function generateLargeContent(lines: number): string {
  // Generate content that exceeds 200KB threshold
  const lineContent = 'const someVeryLongVariableName = "this is a long string value with padding";';
  return Array.from({ length: lines }, (_, i) => `${lineContent} // line ${i + 1}`).join('\n');
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('generate() chunked file path', () => {
  it('routes oversized files through chunked generation', async () => {
    // Create a file > 200KB (chunking threshold)
    const largeContent = generateLargeContent(3000); // ~240KB
    await createTestFile('large.ts', largeContent);

    const results = await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024, // 1MB to allow scanner to include large files
        force: true,
      },
      repoRoot: tempDir,
    });

    // Should have generated file MAP
    expect(results.length).toBeGreaterThan(0);
    // Result path uses {base}_{ext}_MAP.md format
    const fileResult = results.find(r => r.path.includes('large_ts_MAP.md'));
    expect(fileResult).toBeDefined();
    expect(fileResult!.success).toBe(true);

    // File MAP should exist (filename is {base}_{ext}_MAP.md)
    const fileMapPath = path.join(tempDir, 'large_ts_MAP.md');
    const fileMapExists = await fs.access(fileMapPath).then(() => true).catch(() => false);
    expect(fileMapExists).toBe(true);

    // File artifact should be chunk_rollup mode
    const fileMapContent = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(fileMapContent);
    expect(artifact).not.toBeNull();
    expect(artifact!.synthesisMode).toBe('chunk_rollup');
    expect(artifact!.chunkBasis).toBeDefined();
    expect(artifact!.chunkBasis!.length).toBeGreaterThan(0);
  });

  it('writes chunk artifacts for oversized files', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('chunked.ts', largeContent);

    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Check that chunk artifacts exist
    const files = await fs.readdir(tempDir);
    const chunkFiles = files.filter(f => f.includes('.chunk-') && f.endsWith('.gist.md'));
    expect(chunkFiles.length).toBeGreaterThan(0);

    // Verify chunk artifact format
    const firstChunkPath = path.join(tempDir, chunkFiles[0]);
    const chunkContent = await fs.readFile(firstChunkPath, 'utf-8');
    expect(chunkContent).toContain('scope: chunk');
    expect(chunkContent).toContain('source_file:');
    expect(chunkContent).toContain('chunk_index:');
  });

  it('adds uncertainty note when graph context is dropped', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('with-graph.ts', largeContent);

    // Request code_and_graph synthesis (will be downgraded for chunked files)
    await generate({
      llm: mockLlm,
      graphAdapter: {
        name: 'mock-graph',
        getContext: async () => null,
        isAvailable: async () => true,
      },
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    const fileMapPath = path.join(tempDir, 'with-graph_ts_MAP.md');
    const fileMapContent = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(fileMapContent);

    expect(artifact).not.toBeNull();
    expect(artifact!.uncertaintyNotes).toBeDefined();
    const graphNote = artifact!.uncertaintyNotes!.find(n =>
      n.includes('Graph context was requested')
    );
    expect(graphNote).toBeDefined();
  });
});

describe('chunked file freshness detection', () => {
  it('regenerates when chunk artifacts are missing', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('missing-chunks.ts', largeContent);

    // First generation
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Find and delete some chunk artifacts
    const files = await fs.readdir(tempDir);
    const chunkFiles = files.filter(f => f.includes('missing-chunks.ts.chunk-'));
    expect(chunkFiles.length).toBeGreaterThan(0);

    // Delete the first chunk
    await fs.unlink(path.join(tempDir, chunkFiles[0]));

    // Reset call count
    mockLlm.resetCallCount();

    // Second generation without force - should detect missing chunk and regenerate
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: false, // Not forcing, but should still regenerate
      },
      repoRoot: tempDir,
    });

    // LLM should have been called (regeneration happened)
    expect(mockLlm.getCallCount()).toBeGreaterThan(0);

    // Chunk should be restored
    const filesAfter = await fs.readdir(tempDir);
    const chunkFilesAfter = filesAfter.filter(f => f.includes('missing-chunks.ts.chunk-'));
    expect(chunkFilesAfter.length).toBe(chunkFiles.length);
  });

  it('regenerates when file artifact is unparsable', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('corrupt.ts', largeContent);

    // First generation
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Corrupt the file artifact
    const fileMapPath = path.join(tempDir, 'corrupt_ts_MAP.md');
    await fs.writeFile(fileMapPath, 'not valid yaml frontmatter at all', 'utf-8');

    // Reset call count
    mockLlm.resetCallCount();

    // Second generation without force - should detect corrupt artifact and regenerate
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: false,
      },
      repoRoot: tempDir,
    });

    // LLM should have been called (regeneration happened)
    expect(mockLlm.getCallCount()).toBeGreaterThan(0);

    // Artifact should be valid now
    const content = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(content);
    expect(artifact).not.toBeNull();
  });

  it('skips regeneration when all chunks are intact', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('intact.ts', largeContent);

    // First generation
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Reset call count
    mockLlm.resetCallCount();

    // Second generation without force - should skip (all intact)
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: false,
      },
      repoRoot: tempDir,
    });

    // LLM should NOT have been called for file (freshness check passed)
    // Note: folder MAP might still be generated, so we check the call count
    // is less than what a full regeneration would require
    const fullRegenerationCalls = mockLlm.getCallCount();

    // Reset and do a forced regeneration to compare
    mockLlm.resetCallCount();
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    const forcedCalls = mockLlm.getCallCount();

    // Fresh check should have fewer calls than forced regeneration
    expect(fullRegenerationCalls).toBeLessThan(forcedCalls);
  });
});

describe('generate() small file path', () => {
  it('uses whole-file generation for small files', async () => {
    // Create a small file (< 100KB)
    const smallContent = 'const x = 1;\nexport { x };';
    await createTestFile('small.ts', smallContent);

    const results = await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Should have generated file MAP ({base}_{ext}_MAP.md format)
    const fileResult = results.find(r => r.path.includes('small_ts_MAP.md'));
    expect(fileResult).toBeDefined();
    expect(fileResult!.success).toBe(true);

    // No chunk artifacts should exist
    const files = await fs.readdir(tempDir);
    const chunkFiles = files.filter(f => f.includes('.chunk-'));
    expect(chunkFiles.length).toBe(0);
  });
});

// ══════════════════════════════════════════════════════════════════════════════
// Fixture-based chunk rollup tests
// ══════════════════════════════════════════════════════════════════════════════

describe('chunk rollup ordering and determinism', () => {
  it('chunk_basis array preserves chunk index order', async () => {
    // Create a file that will produce multiple chunks
    const largeContent = generateLargeContent(3000);
    await createTestFile('ordered.ts', largeContent);

    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Read the file artifact
    const fileMapPath = path.join(tempDir, 'ordered_ts_MAP.md');
    const fileMapContent = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(fileMapContent);

    expect(artifact).not.toBeNull();
    expect(artifact!.chunkBasis).toBeDefined();
    expect(artifact!.chunkBasis!.length).toBeGreaterThan(1);

    // Verify chunk IDs are in index order (chunk-0, chunk-1, chunk-2, ...)
    for (let i = 0; i < artifact!.chunkBasis!.length; i++) {
      const chunkId = artifact!.chunkBasis![i];
      // chunkId format: fileHash:index:startLine-endLine
      const match = chunkId.match(/:(\d+):/);
      expect(match).not.toBeNull();
      expect(parseInt(match![1], 10)).toBe(i);
    }
  });

  it('chunk artifacts have sequential indices', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('sequential.ts', largeContent);

    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    // Find all chunk artifacts
    const files = await fs.readdir(tempDir);
    const chunkFiles = files
      .filter(f => f.includes('sequential.ts.chunk-'))
      .sort((a, b) => {
        const aMatch = a.match(/chunk-(\d+)/);
        const bMatch = b.match(/chunk-(\d+)/);
        return parseInt(aMatch![1], 10) - parseInt(bMatch![1], 10);
      });

    expect(chunkFiles.length).toBeGreaterThan(1);

    // Verify sequential indices starting from 0
    for (let i = 0; i < chunkFiles.length; i++) {
      expect(chunkFiles[i]).toContain(`chunk-${i}`);
    }
  });

  it('deterministic output: same input produces same artifact structure', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('deterministic.ts', largeContent);

    // First generation
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    const fileMapPath = path.join(tempDir, 'deterministic_ts_MAP.md');
    const firstContent = await fs.readFile(fileMapPath, 'utf-8');
    const firstArtifact = parseFileArtifact(firstContent);

    // Delete artifacts and regenerate
    const files = await fs.readdir(tempDir);
    for (const f of files) {
      if (f.includes('deterministic')) {
        await fs.unlink(path.join(tempDir, f));
      }
    }

    // Re-create the file with same content
    await createTestFile('deterministic.ts', largeContent);

    // Second generation
    mockLlm.resetCallCount();
    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    const secondContent = await fs.readFile(fileMapPath, 'utf-8');
    const secondArtifact = parseFileArtifact(secondContent);

    // Structural determinism: same chunk count, same basis ordering
    expect(secondArtifact!.chunkBasis!.length).toBe(firstArtifact!.chunkBasis!.length);
    expect(secondArtifact!.synthesisMode).toBe(firstArtifact!.synthesisMode);
    expect(secondArtifact!.fileHash).toBe(firstArtifact!.fileHash);

    // Chunk IDs should match (same file hash, same indices, same spans)
    for (let i = 0; i < firstArtifact!.chunkBasis!.length; i++) {
      expect(secondArtifact!.chunkBasis![i]).toBe(firstArtifact!.chunkBasis![i]);
    }
  });

  it('file artifact has correct frontmatter shape for chunk_rollup mode', async () => {
    const largeContent = generateLargeContent(3000);
    await createTestFile('shape.ts', largeContent);

    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        maxFileSize: 1024 * 1024,
        force: true,
      },
      repoRoot: tempDir,
    });

    const fileMapPath = path.join(tempDir, 'shape_ts_MAP.md');
    const content = await fs.readFile(fileMapPath, 'utf-8');
    const artifact = parseFileArtifact(content);

    // Required fields for chunk_rollup mode
    expect(artifact).not.toBeNull();
    expect(artifact!.scope).toBe('file');
    expect(artifact!.sourceFile).toBe('shape.ts');
    expect(artifact!.fileHash).toMatch(/^[a-f0-9]{8}$/);
    expect(artifact!.synthesisMode).toBe('chunk_rollup');
    expect(artifact!.chunkBasis).toBeDefined();
    expect(Array.isArray(artifact!.chunkBasis)).toBe(true);
    expect(artifact!.generatedAt).toBeDefined();
    expect(artifact!.model).toBe('mock-model');
    expect(artifact!.provider).toBe('mock');
    expect(artifact!.content).toBeTruthy();
  });

  it('whole_file mode has correct frontmatter shape (legacy MAP format)', async () => {
    // Note: whole-file mode currently uses writeMap (legacy MAP.md format)
    // not serializeFileArtifact (new FileArtifact format).
    // This test verifies the legacy format. Format unification is future work.
    const smallContent = 'const x = 1;\nexport { x };';
    await createTestFile('whole.ts', smallContent);

    await generate({
      llm: mockLlm,
      config: {
        rootPath: tempDir,
        maxDepth: 1,
        force: true,
      },
      repoRoot: tempDir,
    });

    const fileMapPath = path.join(tempDir, 'whole_ts_MAP.md');
    const content = await fs.readFile(fileMapPath, 'utf-8');

    // Parse using gray-matter (legacy MAP format)
    const matter = await import('gray-matter');
    const { data: frontmatter, content: body } = matter.default(content);

    // Verify legacy MAP frontmatter shape
    expect(frontmatter.generated_by).toBe('rgistr');
    expect(frontmatter.generator_version).toBeDefined();
    expect(frontmatter.scope).toBe('file');
    expect(frontmatter.source_filename).toBe('whole.ts');
    expect(frontmatter.adapter).toBe('mock');
    expect(frontmatter.model).toBe('mock-model');
    expect(frontmatter.synthesis_basis).toBe('code_only');
    expect(frontmatter.confidence).toBe('low');
    expect(frontmatter.generated_at).toBeDefined();
    expect(body.trim()).toBeTruthy();
  });
});
