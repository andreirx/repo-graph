/**
 * Makefile CLI consumer extractor (PROTOTYPE).
 *
 * Scans Makefile and *.mk files for recipe-line tool invocations
 * and emits BoundaryConsumerFact records with mechanism `cli_command`.
 *
 * Makefile structure:
 *   - Target declarations: `target: deps` (not extracted as commands)
 *   - Recipe lines: tab-indented shell commands (extracted)
 *   - Variable assignments: `VAR = value` (not extracted)
 *   - Comments: `# ...` (skipped)
 *
 * Uses the shared cli-invocation-parser for command parsing.
 * Recipe lines are treated as shell commands — same chain splitting,
 * builtin filtering, and wrapper unwrapping as shell scripts.
 *
 * Conservative: skips lines with Make variable expansions ($(VAR))
 * unless the expansion is clearly a prefix that can be stripped.
 * Does NOT evaluate Make variables or conditionals.
 *
 * Extracted:
 *   - Direct tool invocations in recipe lines
 *   - Chained commands (&&, ||, ;)
 *
 * Skipped:
 *   - Target declarations
 *   - Variable assignments
 *   - Make directives (include, ifdef, ifeq, etc.)
 *   - Lines dominated by $(VAR) expansions
 *   - Comments
 *   - Continuation lines (backslash)
 *
 * Maturity: PROTOTYPE.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";
import {
	processCommandSegment,
	splitShellChain,
} from "./cli-invocation-parser.js";

/** Make directives that are not recipe commands. */
const MAKE_DIRECTIVES = new Set([
	"include", "-include", "sinclude",
	"ifdef", "ifndef", "ifeq", "ifneq", "else", "endif",
	"define", "endef", "undefine",
	"override", "export", "unexport",
	"vpath", ".PHONY", ".INTERMEDIATE", ".PRECIOUS",
	".SUFFIXES", ".DEFAULT", ".SECONDARY", ".DELETE_ON_ERROR",
]);

/**
 * Extract CLI consumer facts from a Makefile.
 *
 * @param source - Full source text of the Makefile.
 * @param filePath - Repo-relative path.
 * @param _repoUid - Repository UID.
 */
export function extractMakefileConsumers(
	source: string,
	filePath: string,
	_repoUid: string,
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];
	const lines = source.split("\n");
	let inContinuation = false;

	for (let i = 0; i < lines.length; i++) {
		const raw = lines[i];

		// Recipe lines start with a tab character.
		// Non-recipe lines are targets, assignments, directives, or blank.
		const isRecipe = raw.startsWith("\t");
		if (!isRecipe) {
			// Reset continuation state on non-recipe lines.
			inContinuation = false;
			continue;
		}

		const trimmed = raw.trim();

		// Skip continuation fragments.
		if (inContinuation) {
			inContinuation = trimmed.endsWith("\\");
			continue;
		}
		if (trimmed.endsWith("\\")) {
			inContinuation = true;
			continue;
		}

		// Skip empty recipe lines.
		if (!trimmed) continue;

		// Skip comments.
		if (trimmed.startsWith("#")) continue;

		// Strip Make recipe prefixes: @ (silent), - (ignore errors), + (force).
		// These can appear in any combination: @-+, +@, -@, etc.
		let command = trimmed;
		while (command.length > 0 && (command[0] === "@" || command[0] === "-" || command[0] === "+")) {
			command = command.slice(1);
		}
		command = command.trim();

		// Skip lines dominated by Make variable expansions.
		// If the first token is a $(VAR), ${VAR}, or a quote-wrapped
		// variable like "$(LDLIBS)", skip — these are Make-evaluated
		// commands we can't parse statically.
		if (command.startsWith("$(") || command.startsWith("${")) {
			// Exception: $(Q) is a common quiet-prefix — strip it.
			if (command.startsWith("$(Q)") || command.startsWith("${Q}")) {
				command = command.slice(4).trim();
			} else {
				continue;
			}
		}
		// Also catch quote-wrapped variables: "$(VAR)" or '$(VAR)'.
		if (/^["']?\$[({]/.test(command)) {
			continue;
		}

		// Skip Make directives that appear in recipes (rare but possible).
		const firstWord = command.split(/\s+/)[0];
		if (MAKE_DIRECTIVES.has(firstWord)) continue;

		// Process as shell command.
		const segments = splitShellChain(command);
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
				confidence: parsed.confidence * 0.85, // Lower than shell (Make recipes are more indirect).
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
