/**
 * Shell script CLI consumer extractor (PROTOTYPE).
 *
 * Scans .sh and .bash files for command invocations and emits
 * BoundaryConsumerFact records with mechanism `cli_command`.
 *
 * Uses the shared cli-invocation-parser for command parsing.
 * Conservative line-based extraction — NOT a shell parser.
 *
 * Extracted:
 *   - Direct tool invocations: `rgr repo index myrepo`
 *   - Chained commands: `tsc && vitest run`
 *   - Execution wrappers: `npx vitest`
 *   - Environment-prefixed commands: `NODE_ENV=test vitest`
 *
 * Skipped:
 *   - Comment lines (# ...)
 *   - Empty lines
 *   - Shell control flow (if, then, else, fi, for, do, done, while, case, esac)
 *   - Function definitions (function foo { ... })
 *   - Variable assignments without commands (FOO=bar)
 *   - Piped commands (cmd | other) — skipped entirely
 *   - Here-docs
 *   - Command substitution ($(...), `...`)
 *   - Continuation lines (\)
 *   - Sourced files (source, .)
 *   - Shell builtins (cd, echo, exit, etc.)
 *
 * Maturity: PROTOTYPE. Sufficient for build/deploy/CI helper scripts.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";
import {
	processCommandSegment,
	splitShellChain,
} from "./cli-invocation-parser.js";

/** Shell control flow keywords to skip. */
const CONTROL_FLOW = new Set([
	"if", "then", "else", "elif", "fi",
	"for", "do", "done", "while", "until",
	"case", "esac", "select", "in",
	"function",
]);

/**
 * Extract CLI consumer facts from a shell script file.
 *
 * @param source - Full source text of the shell script.
 * @param filePath - Repo-relative path.
 * @param _repoUid - Repository UID.
 */
export function extractShellScriptConsumers(
	source: string,
	filePath: string,
	_repoUid: string,
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];
	const lines = source.split("\n");
	let heredocDelimiter: string | null = null;
	let inContinuation = false;

	for (let i = 0; i < lines.length; i++) {
		const raw = lines[i];
		const trimmed = raw.trim();

		// Skip heredoc content. Detect start: << EOF or << "EOF" or << 'EOF'.
		if (heredocDelimiter !== null) {
			if (trimmed === heredocDelimiter) {
				heredocDelimiter = null;
			}
			continue;
		}
		const heredocMatch = raw.match(/<<-?\s*["']?(\w+)["']?\s*$/);
		if (heredocMatch) {
			heredocDelimiter = heredocMatch[1];
			// The line before the heredoc may still have a command (cat << EOF).
			// But extracting it is fragile — skip the whole line.
			continue;
		}

		// Skip empty lines.
		if (!trimmed) continue;

		// Skip comments.
		if (trimmed.startsWith("#")) continue;

		// Skip shebang.
		if (i === 0 && trimmed.startsWith("#!")) continue;

		// Skip pure variable assignments (VAR=value with no subsequent command).
		if (isVariableAssignment(trimmed)) continue;

		// Skip lines that look like continuation flags from multi-line
		// commands (--flag, --option value). These are fragments, not
		// standalone commands.
		if (trimmed.startsWith("--") || trimmed.startsWith("-") && trimmed.length <= 3) continue;

		// Skip continuation fragments. If the previous line ended with \,
		// this line is a continuation of that command — skip it.
		if (inContinuation) {
			// If this line also ends with \, we're still continuing.
			inContinuation = trimmed.endsWith("\\");
			continue;
		}

		// Skip lines ending with \ (continuation start) — the next line
		// is a continuation fragment of this command. Skip this line too
		// rather than extracting a partial command.
		if (trimmed.endsWith("\\")) {
			inContinuation = true;
			continue;
		}

		// Skip lines that are clearly not commands:
		// - Single punctuation or special chars: ), }, {, ], [, etc.
		// - Lines starting with quotes (string fragments from multi-line expressions).
		// - Lines starting with $ (variable references on their own).
		if (/^[)}\]'"`$]/.test(trimmed) && trimmed.length < 5) continue;

		// Skip lines inside subshell/arithmetic expressions.
		if (trimmed.startsWith("$(") || trimmed.startsWith("$((")) continue;

		// Skip shell control flow keywords.
		const firstWord = trimmed.split(/\s+/)[0];
		if (CONTROL_FLOW.has(firstWord)) continue;

		// Skip function definitions.
		if (trimmed.match(/^(?:function\s+)?\w+\s*\(\)\s*\{?$/)) continue;

		// Skip closing braces.
		if (trimmed === "}" || trimmed === "{") continue;

		// Skip continuation-only lines.
		if (trimmed === "\\") continue;

		// Split on chain operators and process each segment.
		const segments = splitShellChain(trimmed);

		for (const segment of segments) {
			const parsed = processCommandSegment(segment);
			if (!parsed) continue;

			facts.push({
				mechanism: "cli_command",
				operation: `CLI ${parsed.commandPath}`,
				address: parsed.commandPath,
				callerStableKey: `${filePath}:${i + 1}`,
				sourceFile: filePath,
				lineStart: i + 1,
				basis: "literal",
				confidence: parsed.confidence * 0.9, // Slightly lower than package.json (less structured).
				schemaRef: null,
				metadata: {
					rawCommand: segment.trim().slice(0, 100),
					binary: parsed.binary,
					args: parsed.args,
				},
			});
		}
	}

	return facts;
}

/**
 * Check if a line is a pure variable assignment with no command.
 * "FOO=bar" → true (assignment only)
 * "FOO=bar command" → false (assignment as env prefix)
 * "export FOO=bar" → true (export is a builtin)
 */
function isVariableAssignment(line: string): boolean {
	// Matches: VAR=value or VAR="value" or VAR='value'
	// Does NOT match: VAR=value command (has tokens after the assignment)
	return /^[A-Za-z_]\w*=\S*$/.test(line);
}
