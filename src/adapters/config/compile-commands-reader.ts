/**
 * compile_commands.json reader (adapter).
 *
 * Reads a Clang compilation database (compile_commands.json) and extracts
 * per-file compilation context: include paths, defines, and the compiled
 * file list. This is the standard mechanism for C/C++ project understanding.
 *
 * Format: JSON array of compilation entries. Each entry has:
 *   - "file": absolute or relative source file path
 *   - "directory": working directory for the compilation
 *   - "command" or "arguments": the full compiler invocation
 *
 * Extracted per-file context:
 *   - include paths (-I flags, -isystem flags)
 *   - defines (-D flags)
 *   - the source file itself (for file-list validation)
 *
 * The indexer uses include paths to resolve `#include "header.h"` against
 * the actual file tree, enabling cross-file IMPORTS edge resolution.
 *
 * Does NOT handle:
 *   - Compiler-specific flags beyond -I/-D/-isystem
 *   - Response files (@file)
 *   - PCH (precompiled header) resolution
 *   - Target-triple-dependent include paths
 *
 * Maturity: PROTOTYPE.
 */

import { readFile } from "node:fs/promises";
import { join, relative, resolve, isAbsolute } from "node:path";

// ── Output types ────────────────────────────────────────────────────

/**
 * Compilation context for a single source file.
 */
export interface CompileEntry {
	/** Repo-relative source file path. */
	file: string;
	/** Include paths (from -I and -isystem flags), repo-relative. */
	includePaths: string[];
	/** Preprocessor defines (from -D flags). */
	defines: string[];
}

/**
 * The full compilation database for a repo.
 */
export interface CompilationDatabase {
	/** Per-file compilation entries, keyed by repo-relative path. */
	entries: Map<string, CompileEntry>;
	/** All unique include paths across the entire project, repo-relative. */
	allIncludePaths: string[];
}

// ── Reader ──────────────────────────────────────────────────────────

/**
 * Read and parse compile_commands.json from a repo root.
 * Returns null if the file doesn't exist.
 */
export async function readCompileCommands(
	repoRootPath: string,
): Promise<CompilationDatabase | null> {
	const dbPath = join(repoRootPath, "compile_commands.json");
	let content: string;
	try {
		content = await readFile(dbPath, "utf-8");
	} catch {
		return null;
	}

	return parseCompileCommands(content, repoRootPath);
}

/**
 * Parse compile_commands.json content.
 *
 * @param content - JSON string content of compile_commands.json.
 * @param repoRootPath - Absolute path to the repo root, used to
 *   make file paths and include paths repo-relative.
 */
export function parseCompileCommands(
	content: string,
	repoRootPath: string,
): CompilationDatabase {
	let raw: unknown[];
	try {
		raw = JSON.parse(content);
	} catch {
		return { entries: new Map(), allIncludePaths: [] };
	}

	if (!Array.isArray(raw)) {
		return { entries: new Map(), allIncludePaths: [] };
	}

	const entries = new Map<string, CompileEntry>();
	const allIncludePathsSet = new Set<string>();

	for (const item of raw) {
		if (!item || typeof item !== "object") continue;
		const entry = item as Record<string, unknown>;

		const fileRaw = entry.file as string | undefined;
		const directoryRaw = (entry.directory as string) ?? repoRootPath;
		// Resolve relative directory against repo root.
		const directory = isAbsolute(directoryRaw)
			? directoryRaw
			: resolve(repoRootPath, directoryRaw);
		if (!fileRaw) continue;

		// Resolve the file path to repo-relative.
		const absFile = isAbsolute(fileRaw)
			? fileRaw
			: resolve(directory, fileRaw);
		const relFile = relative(repoRootPath, absFile);

		// Skip files outside the repo root.
		if (relFile.startsWith("..")) continue;

		// Extract flags from either "arguments" array or "command" string.
		let args: string[];
		if (Array.isArray(entry.arguments)) {
			args = entry.arguments as string[];
		} else if (typeof entry.command === "string") {
			args = splitCommandString(entry.command as string);
		} else {
			continue;
		}

		const includePaths: string[] = [];
		const defines: string[] = [];

		for (let i = 0; i < args.length; i++) {
			const arg = args[i];

			// -I/path or -I /path
			if (arg === "-I" || arg === "-isystem") {
				const next = args[i + 1];
				if (next) {
					const absPath = isAbsolute(next)
						? next
						: resolve(directory, next);
					const relPath = relative(repoRootPath, absPath);
					if (!relPath.startsWith("..")) {
						includePaths.push(relPath);
						allIncludePathsSet.add(relPath);
					}
					i++; // Skip next arg (consumed).
				}
			} else if (arg.startsWith("-I")) {
				const pathPart = arg.slice(2);
				const absPath = isAbsolute(pathPart)
					? pathPart
					: resolve(directory, pathPart);
				const relPath = relative(repoRootPath, absPath);
				if (!relPath.startsWith("..")) {
					includePaths.push(relPath);
					allIncludePathsSet.add(relPath);
				}
			} else if (arg.startsWith("-isystem")) {
				const pathPart = arg.slice(8);
				if (pathPart) {
					const absPath = isAbsolute(pathPart)
						? pathPart
						: resolve(directory, pathPart);
					const relPath = relative(repoRootPath, absPath);
					if (!relPath.startsWith("..")) {
						includePaths.push(relPath);
						allIncludePathsSet.add(relPath);
					}
				}
			}

			// -DFOO or -DFOO=value
			if (arg === "-D") {
				const next = args[i + 1];
				if (next) {
					defines.push(next);
					i++;
				}
			} else if (arg.startsWith("-D")) {
				defines.push(arg.slice(2));
			}
		}

		entries.set(relFile, { file: relFile, includePaths, defines });
	}

	return {
		entries,
		allIncludePaths: [...allIncludePathsSet],
	};
}

/**
 * Split a command string into arguments.
 * Handles basic quoting (single and double quotes).
 * Does NOT handle escape sequences within quotes.
 */
function splitCommandString(cmd: string): string[] {
	const args: string[] = [];
	let current = "";
	let inSingle = false;
	let inDouble = false;

	for (const ch of cmd) {
		if (ch === "'" && !inDouble) {
			inSingle = !inSingle;
		} else if (ch === '"' && !inSingle) {
			inDouble = !inDouble;
		} else if (ch === " " && !inSingle && !inDouble) {
			if (current) {
				args.push(current);
				current = "";
			}
		} else {
			current += ch;
		}
	}
	if (current) args.push(current);
	return args;
}
