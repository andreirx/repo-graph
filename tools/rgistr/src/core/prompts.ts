/**
 * Prompts for hierarchical documentation synthesis.
 *
 * Design principles:
 * - System prompt establishes strict behavior contract
 * - Sectioned markdown output (not JSON) for graceful degradation
 * - Anti-hallucination: distinguish observed/inferred/unknown
 * - Seam/stub discipline: never upgrade placeholders to active integrations
 * - Partial usefulness even if truncated
 */

import type { FileDigest } from './predigest.js';

// ─────────────────────────────────────────────────────────────────────────────
// System Prompt
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Stable system prompt for all synthesis operations.
 */
export const SYSTEM_PROMPT = `You produce lossy orientation summaries for AI agents with limited context windows.

Your job is compression, not authorship.

Non-negotiable rules:
1. Do not invent ownership, runtime roles, active integrations, entrypoints, or architectural importance unless directly supported by the provided evidence.
2. Distinguish clearly between:
   - observed facts
   - likely inferences
   - uncertainty
3. If code defines an interface, abstract base, null adapter, placeholder, reserved hook, TODO seam, or not-yet-wired extension point, describe it as a seam, stub, placeholder, or extension point. Do not describe it as active runtime integration.
4. Prefer concrete symbol names, filenames, and explicit responsibilities over generic prose.
5. Do not restate obvious syntax or boilerplate unless it matters structurally.
6. Do not claim relationships between sibling files or folders unless the supplied evidence supports those relationships.
7. If the evidence basis is code_only, do not claim caller behavior, ownership boundaries, module identity, or runtime/build context unless explicit in the code text itself.
8. If uncertain, say so explicitly in the Uncertainty section.
9. Keep outputs dense, specific, and mechanically useful for deciding what to read next.

Output format: sectioned markdown with the headers specified in the user message.`;

// ─────────────────────────────────────────────────────────────────────────────
// Output Section Schemas (for parsing)
// ─────────────────────────────────────────────────────────────────────────────

export interface FileSummarySchema {
  purpose: string;
  key_symbols: string[];
  notable_dependencies: string[];
  likely_change_reasons: string[];
  reading_hint: string;
  uncertainty: string | null;
}

export interface FolderSummarySchema {
  purpose: string;
  structure: string;
  key_components: string[];
  seams: string[];
  reading_order: string;
  uncertainty: string | null;
}

// ─────────────────────────────────────────────────────────────────────────────
// File Prompt - Whole File Mode (Default)
// ─────────────────────────────────────────────────────────────────────────────

export function filePromptWhole(
  filePath: string,
  content: string,
  language: string,
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs' = 'code_only',
  graphContext?: string
): string {
  const parts: string[] = [];

  parts.push(`SYNTHESIS_BASIS: ${synthesisBasis}`);
  parts.push('');
  parts.push('Summarize this source file for agent orientation.');
  parts.push('');
  parts.push('Output format - use these exact section headers:');
  parts.push('');
  parts.push('```');
  parts.push('# Purpose');
  parts.push('1-2 sentences explaining what this file is for.');
  parts.push('');
  parts.push('# Key Symbols');
  parts.push('- symbol_name');
  parts.push('- another_symbol');
  parts.push('');
  parts.push('# Notable Dependencies');
  parts.push('- dependency (why it matters)');
  parts.push('');
  parts.push('# Likely Change Reasons');
  parts.push('- reason someone would edit this');
  parts.push('');
  parts.push('# Reading Hint');
  parts.push('Where to start reading and what matters most.');
  parts.push('');
  parts.push('# Uncertainty');
  parts.push('What cannot be determined from the provided evidence. Write "None" if confident.');
  parts.push('```');
  parts.push('');
  parts.push('Rules:');
  parts.push('- Use exact symbol names from the file.');
  parts.push('- If the file is primarily a seam/stub/interface/null implementation, say that explicitly.');
  parts.push('- If not yet wired into runtime behavior, say so.');
  parts.push('- Do not claim callers, ownership, or runtime role unless evidence supports it.');
  parts.push('');

  parts.push('─'.repeat(60));
  parts.push(`FILE: ${filePath}`);
  parts.push(`LANGUAGE: ${language}`);
  parts.push('');

  if (graphContext && synthesisBasis !== 'code_only') {
    parts.push('GRAPH_CONTEXT:');
    parts.push(graphContext);
    parts.push('');
  }

  parts.push('SOURCE:');
  parts.push('```' + language);
  parts.push(content);
  parts.push('```');

  return parts.join('\n');
}

// ─────────────────────────────────────────────────────────────────────────────
// File Prompt - Digest Fallback (Oversized Files)
// ─────────────────────────────────────────────────────────────────────────────

export function filePromptDigest(
  digest: FileDigest,
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs' = 'code_only'
): string {
  const parts: string[] = [];

  parts.push(`SYNTHESIS_BASIS: ${synthesisBasis}`);
  parts.push('INPUT_MODE: digest_fallback (oversized file, excerpts only)');
  parts.push('');
  parts.push('Summarize this source file for agent orientation.');
  parts.push('');
  parts.push('Output format - use these exact section headers:');
  parts.push('');
  parts.push('```');
  parts.push('# Purpose');
  parts.push('# Key Symbols');
  parts.push('# Notable Dependencies');
  parts.push('# Likely Change Reasons');
  parts.push('# Reading Hint');
  parts.push('# Uncertainty');
  parts.push('```');
  parts.push('');
  parts.push('Important: You were NOT given the full file. Be conservative. Note limitations in Uncertainty.');
  parts.push('');

  parts.push('─'.repeat(60));
  parts.push(`FILE: ${digest.path}`);
  parts.push(`LANGUAGE: ${digest.language} | LINES: ${digest.lineCount}`);
  parts.push('');

  if (digest.exports.length > 0) {
    parts.push('EXPORTS: ' + digest.exports.join(', '));
  }
  if (digest.imports.length > 0) {
    parts.push('IMPORTS: ' + digest.imports.join(', '));
  }
  parts.push('');

  if (digest.headerComment) {
    parts.push('HEADER:');
    parts.push(digest.headerComment);
    parts.push('');
  }

  parts.push('CODE (head):');
  parts.push('```' + digest.language);
  parts.push(digest.excerptHead);
  parts.push('```');

  if (digest.excerptTail) {
    parts.push('');
    parts.push('CODE (tail):');
    parts.push('```' + digest.language);
    parts.push(digest.excerptTail);
    parts.push('```');
  }

  return parts.join('\n');
}

export const filePrompt = filePromptDigest;

// ─────────────────────────────────────────────────────────────────────────────
// Folder Synthesis Prompt
// ─────────────────────────────────────────────────────────────────────────────

export interface ChildSummary {
  name: string;
  isFolder: boolean;
  summary: string;
}

export function folderPrompt(
  folderPath: string,
  fileSummaries: ChildSummary[],
  folderSummaries: ChildSummary[],
  synthesisBasis: 'code_only' | 'code_and_graph' | 'code_graph_and_docs' = 'code_only'
): string {
  const parts: string[] = [];

  parts.push(`SYNTHESIS_BASIS: ${synthesisBasis}`);
  parts.push('');
  parts.push('Synthesize a summary of this folder from child summaries.');
  parts.push('');
  parts.push('Output format - use these exact section headers:');
  parts.push('');
  parts.push('```');
  parts.push('# Purpose');
  parts.push('1-2 sentences explaining this folder\'s role.');
  parts.push('');
  parts.push('# Structure');
  parts.push('How the folder is organized.');
  parts.push('');
  parts.push('# Key Components');
  parts.push('- component_name');
  parts.push('');
  parts.push('# Seams');
  parts.push('- interfaces, adapters, boundaries supported by child evidence');
  parts.push('');
  parts.push('# Reading Order');
  parts.push('Recommended order for reading this folder.');
  parts.push('');
  parts.push('# Uncertainty');
  parts.push('Architecturally relevant uncertainties from child summaries. Aggregate, do not dismiss.');
  parts.push('```');
  parts.push('');
  parts.push('Rules:');
  parts.push('- Synthesize only from provided child summaries.');
  parts.push('- Do not invent coordination between children unless summaries support it.');
  parts.push('- If a child is a seam/stub/placeholder, preserve that status.');
  parts.push('- Use exact filenames/folder names.');
  parts.push('- Key Components should be selective, not exhaustive.');
  parts.push('- Uncertainty: if any child reports uncertainty about external types, caller context,');
  parts.push('  or undefined interfaces, carry forward the most architecturally relevant ones.');
  parts.push('  Only write "None" if child summaries contain no meaningful unresolved uncertainty.');
  parts.push('');

  parts.push('─'.repeat(60));
  parts.push(`FOLDER: ${folderPath || '(root)'}`);
  parts.push('');

  if (folderSummaries.length > 0) {
    parts.push('CHILD FOLDERS:');
    for (const child of folderSummaries) {
      parts.push(`\n## ${child.name}/`);
      parts.push(child.summary);
    }
    parts.push('');
  }

  if (fileSummaries.length > 0) {
    parts.push('LOCAL FILES:');
    for (const child of fileSummaries) {
      parts.push(`\n## ${child.name}`);
      parts.push(child.summary);
    }
  }

  return parts.join('\n');
}

// ─────────────────────────────────────────────────────────────────────────────
// File Group Prompt (Large Folders)
// ─────────────────────────────────────────────────────────────────────────────

export function fileGroupPrompt(
  files: Array<{
    name: string;
    language: string;
    exports: string[];
    lineCount: number;
  }>
): string {
  const parts: string[] = [];

  parts.push('SYNTHESIS_BASIS: code_only (file listing only)');
  parts.push('');
  parts.push('Summarize this group of files based on their names and exports.');
  parts.push('This is a large folder - you only have metadata, not full code.');
  parts.push('');
  parts.push('Output format:');
  parts.push('');
  parts.push('```');
  parts.push('# Collective Purpose');
  parts.push('What this group of files handles.');
  parts.push('');
  parts.push('# Patterns');
  parts.push('- naming or structural patterns observed');
  parts.push('');
  parts.push('# Likely Categories');
  parts.push('- groupings within these files');
  parts.push('');
  parts.push('# Uncertainty');
  parts.push('What cannot be determined from names alone.');
  parts.push('```');
  parts.push('');

  parts.push('─'.repeat(60));
  parts.push(`FILES (${files.length} total):`);
  parts.push('');

  for (const f of files) {
    const exportsStr = f.exports.length > 0
      ? ` [${f.exports.slice(0, 5).join(', ')}${f.exports.length > 5 ? '...' : ''}]`
      : '';
    parts.push(`- ${f.name} (${f.language}, ${f.lineCount} lines)${exportsStr}`);
  }

  return parts.join('\n');
}

// ─────────────────────────────────────────────────────────────────────────────
// Repo/Subtree Prompt
// ─────────────────────────────────────────────────────────────────────────────

export function repoPrompt(
  name: string,
  topLevelSummaries: ChildSummary[],
  readme?: string
): string {
  const parts: string[] = [];

  parts.push('SYNTHESIS_BASIS: code_only');
  parts.push('');
  parts.push('Generate a high-level orientation summary for this root scope.');
  parts.push('');
  parts.push('Output format:');
  parts.push('');
  parts.push('```');
  parts.push('# One Liner');
  parts.push('# Architecture');
  parts.push('# Key Areas');
  parts.push('# Entry Points for Reading');
  parts.push('# Uncertainty');
  parts.push('```');
  parts.push('');

  parts.push('─'.repeat(60));
  parts.push(`ROOT_SCOPE: ${name}`);
  parts.push('');

  if (readme) {
    const truncated = readme.length > 1500 ? readme.slice(0, 1500) + '\n[truncated]' : readme;
    parts.push('EXISTING_README:');
    parts.push(truncated);
    parts.push('');
  }

  parts.push('TOP_LEVEL_SUMMARIES:');
  for (const child of topLevelSummaries) {
    parts.push(`\n## ${child.name}${child.isFolder ? '/' : ''}`);
    parts.push(child.summary);
  }

  return parts.join('\n');
}

// ─────────────────────────────────────────────────────────────────────────────
// Markdown Section Parsing
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Extract a section from markdown by header.
 * Returns content between header and next header (or end).
 */
export function extractSection(markdown: string, header: string): string | null {
  const headerPattern = new RegExp(`^#\\s*${header}\\s*$`, 'im');
  const match = markdown.match(headerPattern);
  if (!match || match.index === undefined) return null;

  const start = match.index + match[0].length;
  const rest = markdown.slice(start);

  // Find next header
  const nextHeader = rest.match(/^#\s+/m);
  const content = nextHeader?.index !== undefined
    ? rest.slice(0, nextHeader.index)
    : rest;

  return content.trim() || null;
}

/**
 * Extract bullet items from a section.
 */
export function extractBullets(markdown: string, header: string): string[] {
  const section = extractSection(markdown, header);
  if (!section) return [];

  const bullets: string[] = [];
  const lines = section.split('\n');
  for (const line of lines) {
    const match = line.match(/^[-*]\s+(.+)$/);
    if (match) {
      bullets.push(match[1].trim());
    }
  }
  return bullets;
}

/**
 * Parse file summary from markdown sections.
 */
export function parseFileSummary(markdown: string): FileSummarySchema {
  return {
    purpose: extractSection(markdown, 'Purpose') || markdown.slice(0, 200),
    key_symbols: extractBullets(markdown, 'Key Symbols'),
    notable_dependencies: extractBullets(markdown, 'Notable Dependencies'),
    likely_change_reasons: extractBullets(markdown, 'Likely Change Reasons'),
    reading_hint: extractSection(markdown, 'Reading Hint') || '',
    uncertainty: extractSection(markdown, 'Uncertainty')
  };
}

/**
 * Parse folder summary from markdown sections.
 */
export function parseFolderSummary(markdown: string): FolderSummarySchema {
  return {
    purpose: extractSection(markdown, 'Purpose') || markdown.slice(0, 200),
    structure: extractSection(markdown, 'Structure') || '',
    key_components: extractBullets(markdown, 'Key Components'),
    seams: extractBullets(markdown, 'Seams'),
    reading_order: extractSection(markdown, 'Reading Order') || '',
    uncertainty: extractSection(markdown, 'Uncertainty')
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering (pass-through for markdown output)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * For markdown output, the LLM response IS the rendered content.
 * Just clean up any artifacts.
 */
export function renderFileSummary(summary: FileSummarySchema): string {
  const parts: string[] = [];

  parts.push('# Purpose');
  parts.push(summary.purpose);
  parts.push('');

  if (summary.key_symbols.length > 0) {
    parts.push('# Key Symbols');
    summary.key_symbols.forEach(s => parts.push(`- ${s}`));
    parts.push('');
  }

  if (summary.notable_dependencies.length > 0) {
    parts.push('# Notable Dependencies');
    summary.notable_dependencies.forEach(d => parts.push(`- ${d}`));
    parts.push('');
  }

  if (summary.likely_change_reasons.length > 0) {
    parts.push('# Likely Change Reasons');
    summary.likely_change_reasons.forEach(r => parts.push(`- ${r}`));
    parts.push('');
  }

  if (summary.reading_hint) {
    parts.push('# Reading Hint');
    parts.push(summary.reading_hint);
    parts.push('');
  }

  if (summary.uncertainty) {
    parts.push('# Uncertainty');
    parts.push(summary.uncertainty);
  }

  return parts.join('\n').trim();
}

export function renderFolderSummary(summary: FolderSummarySchema): string {
  const parts: string[] = [];

  parts.push('# Purpose');
  parts.push(summary.purpose);
  parts.push('');

  if (summary.structure) {
    parts.push('# Structure');
    parts.push(summary.structure);
    parts.push('');
  }

  if (summary.key_components.length > 0) {
    parts.push('# Key Components');
    summary.key_components.forEach(c => parts.push(`- ${c}`));
    parts.push('');
  }

  if (summary.seams.length > 0) {
    parts.push('# Seams');
    summary.seams.forEach(s => parts.push(`- ${s}`));
    parts.push('');
  }

  if (summary.reading_order) {
    parts.push('# Reading Order');
    parts.push(summary.reading_order);
    parts.push('');
  }

  if (summary.uncertainty) {
    parts.push('# Uncertainty');
    parts.push(summary.uncertainty);
  }

  return parts.join('\n').trim();
}
