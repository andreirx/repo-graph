/**
 * File pre-digest: mechanical extraction before LLM summarization.
 *
 * Extracts structured facts from source files without using LLM:
 * - imports
 * - exports / top-level symbols
 * - header comments
 * - selected excerpts (not naive truncation)
 *
 * This reduces LLM token waste and improves summary quality by
 * presenting structured facts rather than raw code blobs.
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';

/**
 * Pre-digested file facts for LLM consumption.
 */
export interface FileDigest {
  /** Relative path from generation root */
  path: string;
  /** Detected language */
  language: string;
  /** Line count */
  lineCount: number;
  /** Byte size */
  byteSize: number;
  /** Extracted imports (module specifiers) */
  imports: string[];
  /** Extracted exports / top-level symbols */
  exports: string[];
  /** Header comment or docstring (first doc block) */
  headerComment: string | null;
  /** Head excerpt (first ~80 lines after imports) */
  excerptHead: string;
  /** Tail excerpt (last ~40 lines if file is long) */
  excerptTail: string | null;
  /** Whether the file was truncated */
  truncated: boolean;
}

/** Language detection by extension */
const LANGUAGE_MAP: Record<string, string> = {
  '.ts': 'typescript',
  '.tsx': 'typescript',
  '.js': 'javascript',
  '.jsx': 'javascript',
  '.mjs': 'javascript',
  '.cjs': 'javascript',
  '.py': 'python',
  '.pyi': 'python',
  '.rs': 'rust',
  '.go': 'go',
  '.java': 'java',
  '.kt': 'kotlin',
  '.c': 'c',
  '.h': 'c',
  '.cpp': 'cpp',
  '.hpp': 'cpp',
  '.rb': 'ruby',
  '.php': 'php',
  '.swift': 'swift',
  '.scala': 'scala',
  '.cs': 'csharp',
  '.vue': 'vue',
  '.svelte': 'svelte',
};

/**
 * Pre-digest a source file for LLM summarization.
 */
export async function digestFile(
  absolutePath: string,
  relativePath: string
): Promise<FileDigest> {
  const content = await fs.readFile(absolutePath, 'utf-8');
  const ext = path.extname(absolutePath).toLowerCase();
  const language = LANGUAGE_MAP[ext] || 'unknown';
  const lines = content.split('\n');

  const digest: FileDigest = {
    path: relativePath,
    language,
    lineCount: lines.length,
    byteSize: Buffer.byteLength(content, 'utf-8'),
    imports: extractImports(content, language),
    exports: extractExports(content, language),
    headerComment: extractHeaderComment(content, language),
    excerptHead: '',
    excerptTail: null,
    truncated: false,
  };

  // Extract excerpts
  const { head, tail, truncated } = extractExcerpts(lines, language);
  digest.excerptHead = head;
  digest.excerptTail = tail;
  digest.truncated = truncated;

  return digest;
}

/**
 * Extract import statements.
 */
function extractImports(content: string, language: string): string[] {
  const imports: string[] = [];

  switch (language) {
    case 'typescript':
    case 'javascript': {
      // ES6 imports: import X from 'Y', import { X } from 'Y'
      const esImports = content.matchAll(/import\s+(?:[\s\S]*?)\s+from\s+['"]([^'"]+)['"]/g);
      for (const m of esImports) {
        imports.push(m[1]);
      }
      // require() calls
      const requires = content.matchAll(/require\s*\(\s*['"]([^'"]+)['"]\s*\)/g);
      for (const m of requires) {
        imports.push(m[1]);
      }
      break;
    }
    case 'python': {
      // import X, from X import Y
      const pyImports = content.matchAll(/^(?:from\s+(\S+)\s+import|import\s+(\S+))/gm);
      for (const m of pyImports) {
        imports.push(m[1] || m[2]);
      }
      break;
    }
    case 'rust': {
      // use X::Y
      const useStatements = content.matchAll(/^use\s+([^;]+);/gm);
      for (const m of useStatements) {
        imports.push(m[1].split('::')[0]);
      }
      break;
    }
    case 'go': {
      // import "X" or import ( "X" "Y" )
      const goImports = content.matchAll(/import\s+(?:\(\s*)?["']([^"']+)["']/g);
      for (const m of goImports) {
        imports.push(m[1]);
      }
      break;
    }
    // Add more languages as needed
  }

  // Deduplicate and limit
  return [...new Set(imports)].slice(0, 20);
}

/**
 * Extract export/top-level symbol names.
 */
function extractExports(content: string, language: string): string[] {
  const exports: string[] = [];

  switch (language) {
    case 'typescript':
    case 'javascript': {
      // export const/let/var/function/class/interface/type X
      const namedExports = content.matchAll(
        /export\s+(?:const|let|var|function|class|interface|type|enum|async\s+function)\s+(\w+)/g
      );
      for (const m of namedExports) {
        exports.push(m[1]);
      }
      // export default X
      const defaultExport = content.match(/export\s+default\s+(?:function|class)?\s*(\w+)?/);
      if (defaultExport?.[1]) {
        exports.push(`default:${defaultExport[1]}`);
      }
      break;
    }
    case 'python': {
      // Top-level def/class
      const pyDefs = content.matchAll(/^(?:def|class|async\s+def)\s+(\w+)/gm);
      for (const m of pyDefs) {
        exports.push(m[1]);
      }
      break;
    }
    case 'rust': {
      // pub fn/struct/enum/trait/mod/const/type
      const pubItems = content.matchAll(/^pub\s+(?:fn|struct|enum|trait|mod|const|type|async\s+fn)\s+(\w+)/gm);
      for (const m of pubItems) {
        exports.push(m[1]);
      }
      break;
    }
    case 'go': {
      // Capitalized function/type names
      const goExports = content.matchAll(/^(?:func|type)\s+([A-Z]\w*)/gm);
      for (const m of goExports) {
        exports.push(m[1]);
      }
      break;
    }
  }

  return [...new Set(exports)].slice(0, 30);
}

/**
 * Extract header comment (first doc block).
 */
function extractHeaderComment(content: string, language: string): string | null {
  let match: RegExpMatchArray | null = null;

  switch (language) {
    case 'typescript':
    case 'javascript':
    case 'rust':
    case 'go':
    case 'java':
    case 'cpp':
    case 'c': {
      // /** ... */ or /* ... */ at start
      match = content.match(/^\s*(\/\*\*[\s\S]*?\*\/|\/\*[\s\S]*?\*\/)/);
      if (!match) {
        // // comments at start
        const lineComments = content.match(/^(\s*\/\/[^\n]*\n)+/);
        if (lineComments) {
          match = lineComments;
        }
      }
      break;
    }
    case 'python': {
      // """...""" or '''...''' docstring at start (after imports)
      match = content.match(/^(?:.*\n)*?(?:"""[\s\S]*?"""|'''[\s\S]*?''')/);
      break;
    }
  }

  if (match) {
    const comment = match[0].trim();
    // Limit to 500 chars
    return comment.length > 500 ? comment.slice(0, 500) + '...' : comment;
  }

  return null;
}

/**
 * Extract head and tail excerpts intelligently.
 */
function extractExcerpts(
  lines: string[],
  language: string
): { head: string; tail: string | null; truncated: boolean } {
  const HEAD_LINES = 80;
  const TAIL_LINES = 40;
  const LONG_FILE_THRESHOLD = 150;

  // Find where imports end
  let importEndLine = 0;
  for (let i = 0; i < Math.min(lines.length, 50); i++) {
    const line = lines[i];
    if (isImportLine(line, language)) {
      importEndLine = i + 1;
    } else if (line.trim() && !isCommentLine(line, language) && importEndLine > 0) {
      // Non-import, non-comment line after imports - stop
      break;
    }
  }

  const contentStart = importEndLine;
  const contentLines = lines.slice(contentStart);

  if (contentLines.length <= HEAD_LINES) {
    // Short file - include everything
    return {
      head: contentLines.join('\n'),
      tail: null,
      truncated: false,
    };
  }

  // Long file - take head and tail
  const head = contentLines.slice(0, HEAD_LINES).join('\n');

  let tail: string | null = null;
  if (contentLines.length > LONG_FILE_THRESHOLD) {
    tail = contentLines.slice(-TAIL_LINES).join('\n');
  }

  return {
    head,
    tail,
    truncated: true,
  };
}

function isImportLine(line: string, language: string): boolean {
  const trimmed = line.trim();
  switch (language) {
    case 'typescript':
    case 'javascript':
      return trimmed.startsWith('import ') || trimmed.startsWith('require(');
    case 'python':
      return trimmed.startsWith('import ') || trimmed.startsWith('from ');
    case 'rust':
      return trimmed.startsWith('use ') || trimmed.startsWith('mod ');
    case 'go':
      return trimmed.startsWith('import ');
    default:
      return false;
  }
}

function isCommentLine(line: string, language: string): boolean {
  const trimmed = line.trim();
  switch (language) {
    case 'typescript':
    case 'javascript':
    case 'rust':
    case 'go':
    case 'java':
    case 'cpp':
    case 'c':
      return trimmed.startsWith('//') || trimmed.startsWith('/*') || trimmed.startsWith('*');
    case 'python':
      return trimmed.startsWith('#');
    default:
      return false;
  }
}

/**
 * Format a digest for inclusion in an LLM prompt.
 * Produces a compact, structured representation.
 */
export function formatDigestForPrompt(digest: FileDigest): string {
  const parts: string[] = [];

  parts.push(`## File: ${digest.path}`);
  parts.push(`Language: ${digest.language} | Lines: ${digest.lineCount} | Size: ${digest.byteSize} bytes`);

  if (digest.imports.length > 0) {
    parts.push(`\n### Imports\n${digest.imports.map(i => `- ${i}`).join('\n')}`);
  }

  if (digest.exports.length > 0) {
    parts.push(`\n### Exports/Symbols\n${digest.exports.map(e => `- ${e}`).join('\n')}`);
  }

  if (digest.headerComment) {
    parts.push(`\n### Header Comment\n${digest.headerComment}`);
  }

  parts.push(`\n### Code Excerpt (head)\n\`\`\`${digest.language}\n${digest.excerptHead}\n\`\`\``);

  if (digest.excerptTail) {
    parts.push(`\n### Code Excerpt (tail)\n\`\`\`${digest.language}\n${digest.excerptTail}\n\`\`\``);
  }

  if (digest.truncated) {
    parts.push(`\n[File truncated - showing head and tail excerpts]`);
  }

  return parts.join('\n');
}
