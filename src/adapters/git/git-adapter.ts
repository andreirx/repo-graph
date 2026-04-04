/**
 * Git adapter — wraps git CLI commands behind the GitPort interface.
 *
 * All git interaction goes through this adapter. Uses child_process
 * to invoke git commands. Parses output deterministically.
 */

import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { FileChurnEntry, GitPort } from "../../core/ports/git.js";

const execFileAsync = promisify(execFile);

export class GitAdapter implements GitPort {
	async getCurrentCommit(repoPath: string): Promise<string | null> {
		try {
			const { stdout } = await execFileAsync("git", ["rev-parse", "HEAD"], {
				cwd: repoPath,
			});
			const hash = stdout.trim();
			return hash.length > 0 ? hash : null;
		} catch {
			return null;
		}
	}

	async isDirty(repoPath: string): Promise<boolean> {
		try {
			const { stdout } = await execFileAsync("git", ["status", "--porcelain"], {
				cwd: repoPath,
			});
			return stdout.trim().length > 0;
		} catch {
			return false;
		}
	}

	async getFileChurn(
		repoPath: string,
		since: string,
	): Promise<FileChurnEntry[]> {
		// Step 1: get per-file commit counts
		// git log --format="" --name-only --since=<date>
		// Each commit produces a block of filenames separated by blank lines.
		const commitCounts = new Map<string, number>();
		try {
			const { stdout } = await execFileAsync(
				"git",
				// --pretty="format:" (not --format="") produces a blank line
				// between each commit's file list, which we rely on for
				// per-commit counting via split("\n\n").
				["log", "--pretty=format:", "--name-only", `--since=${since}`],
				{ cwd: repoPath, maxBuffer: 50 * 1024 * 1024 },
			);
			// Each non-empty line is a file touched by a commit.
			// Blank lines separate commits. Count distinct commits per file.
			const blocks = stdout.split("\n\n");
			for (const block of blocks) {
				const files = new Set(
					block
						.split("\n")
						.map((l) => l.trim())
						.filter((l) => l.length > 0),
				);
				for (const file of files) {
					commitCounts.set(file, (commitCounts.get(file) ?? 0) + 1);
				}
			}
		} catch {
			return [];
		}

		// Step 2: get per-file line changes
		// git log --numstat --format="" --since=<date>
		const lineChanges = new Map<string, number>();
		try {
			const { stdout } = await execFileAsync(
				"git",
				["log", "--numstat", "--format=", `--since=${since}`],
				{ cwd: repoPath, maxBuffer: 50 * 1024 * 1024 },
			);
			for (const line of stdout.split("\n")) {
				const trimmed = line.trim();
				if (trimmed.length === 0) continue;
				// Format: <added>\t<deleted>\t<file>
				// Binary files show "-" for added/deleted.
				const parts = trimmed.split("\t");
				if (parts.length < 3) continue;
				const added = parts[0] === "-" ? 0 : Number.parseInt(parts[0], 10);
				const deleted = parts[1] === "-" ? 0 : Number.parseInt(parts[1], 10);
				const file = parts.slice(2).join("\t"); // handle tabs in filenames
				if (Number.isNaN(added) || Number.isNaN(deleted)) continue;
				lineChanges.set(file, (lineChanges.get(file) ?? 0) + added + deleted);
			}
		} catch {
			// If numstat fails, we still have commit counts
		}

		// Merge both maps
		const allFiles = new Set([...commitCounts.keys(), ...lineChanges.keys()]);
		const results: FileChurnEntry[] = [];
		for (const filePath of allFiles) {
			results.push({
				filePath: filePath.replace(/\\/g, "/"), // normalize to forward slashes
				commitCount: commitCounts.get(filePath) ?? 0,
				linesChanged: lineChanges.get(filePath) ?? 0,
			});
		}

		return results.sort((a, b) => b.linesChanged - a.linesChanged);
	}
}
