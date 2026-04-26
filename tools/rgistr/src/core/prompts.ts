/**
 * Prompts for hierarchical documentation synthesis.
 *
 * Design principles:
 * - Concise output: summaries should be 2-4 paragraphs, not essays
 * - Structural focus: what does this do, what are its key parts
 * - Purpose clarity: why does this exist, what problem does it solve
 * - Relationship awareness: how does this connect to siblings/parents
 */

/**
 * Prompt for summarizing a single code file.
 */
export function filePrompt(
  relativePath: string,
  content: string,
  context?: { folderPurpose?: string; siblingFiles?: string[] }
): string {
  const contextSection = context?.folderPurpose
    ? `\nThis file is part of a folder whose purpose is: ${context.folderPurpose}\n`
    : '';

  const siblingsSection = context?.siblingFiles?.length
    ? `\nSibling files in this folder: ${context.siblingFiles.join(', ')}\n`
    : '';

  return `Summarize this code file for a developer who needs to quickly understand what it does.

File: ${relativePath}
${contextSection}${siblingsSection}
---
${content}
---

Write a summary that covers:
1. **Purpose**: What does this file do? Why does it exist?
2. **Key exports/functions**: What are the main things defined here?
3. **Dependencies**: What does it depend on (imports)?
4. **Usage context**: When would someone need to read or modify this file?

Keep the summary to 2-4 short paragraphs. Focus on what a developer needs to know, not line-by-line description.
Do not include code blocks in your response.
Do not start with "This file" - be more specific.`;
}

/**
 * Prompt for summarizing a folder from its child summaries.
 */
export function folderPrompt(
  relativePath: string,
  childSummaries: { name: string; summary: string; isFolder: boolean }[],
  context?: { parentPurpose?: string; siblingFolders?: string[] }
): string {
  const contextSection = context?.parentPurpose
    ? `\nThis folder is part of: ${context.parentPurpose}\n`
    : '';

  const siblingsSection = context?.siblingFolders?.length
    ? `\nSibling folders: ${context.siblingFolders.join(', ')}\n`
    : '';

  const childrenText = childSummaries
    .map(c => `### ${c.isFolder ? '[folder]' : '[file]'} ${c.name}\n${c.summary}`)
    .join('\n\n');

  return `Synthesize a summary of this folder from its contents.

Folder: ${relativePath || '(root)'}
${contextSection}${siblingsSection}
## Contents:

${childrenText}

---

Write a folder summary that:
1. **Purpose**: What is this folder's role in the codebase?
2. **Organization**: How is it structured? What are the main groupings?
3. **Key components**: What are the most important files/subfolders?
4. **Developer guidance**: When would someone work in this folder?

Keep it to 2-4 paragraphs. Synthesize the child summaries into a coherent picture - do not just list them.
Focus on the folder's purpose and structure, not exhaustive content listing.`;
}

/**
 * Prompt for generating a repo-level summary.
 */
export function repoPrompt(
  repoName: string,
  topLevelSummaries: { name: string; summary: string; isFolder: boolean }[],
  existingReadme?: string
): string {
  const readmeSection = existingReadme
    ? `\n## Existing README (for reference, may be outdated):\n${existingReadme.slice(0, 2000)}\n`
    : '';

  const topLevelText = topLevelSummaries
    .map(c => `### ${c.isFolder ? '[folder]' : '[file]'} ${c.name}\n${c.summary}`)
    .join('\n\n');

  return `Generate a high-level repository summary from its top-level structure.

Repository: ${repoName}
${readmeSection}
## Top-level contents:

${topLevelText}

---

Write a repository overview that:
1. **What is this?**: One sentence describing what this repository is.
2. **Architecture**: How is the codebase organized?
3. **Key areas**: What are the main functional areas?
4. **Entry points**: Where should a new developer start?

Keep it to 3-5 paragraphs. This is the "30-second orientation" for someone new to the repo.
If an existing README exists, you may reference it but focus on synthesizing the actual structure.`;
}

/**
 * Prompt for deciding whether a file is important enough to summarize individually.
 * Returns a simple yes/no decision.
 */
export function importancePrompt(
  relativePath: string,
  size: number,
  siblingCount: number
): string {
  return `Should this file be individually summarized, or should it be grouped with siblings?

File: ${relativePath}
Size: ${size} bytes
Siblings in folder: ${siblingCount} files

Answer with just "individual" or "group".
Use "individual" for:
- Entry points (index.ts, main.ts, cli.ts, app.ts)
- Configuration files
- Files over 200 lines
- Files with unique, important functionality

Use "group" for:
- Small utility files
- Type definition files
- Test files in large test folders
- Files that are part of a cohesive set`;
}
