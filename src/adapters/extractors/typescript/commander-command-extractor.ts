/**
 * Commander CLI command surface extractor (PROTOTYPE).
 *
 * Scans TypeScript/JavaScript source files for Commander.js command
 * registrations and emits normalized BoundaryProviderFact records
 * with mechanism `cli_command`.
 *
 * Supported patterns:
 *   - program.command("repo").description("...")
 *   - const repo = program.command("repo"); repo.command("add <path>")
 *   - .command("name <args>").description("...").option("--flag", "...")
 *
 * Command path composition:
 *   - Top-level: program.command("repo") → path "repo"
 *   - Nested: repo.command("add <path>") → path "repo add"
 *   - The parent is inferred from the receiver variable name matching
 *     a previously-seen command name.
 *
 * Implementation: LINE-BASED REGEX scanning. Not AST-backed.
 * Known limitations:
 *   - Receiver-to-parent mapping is heuristic (variable name = command name)
 *   - Multi-line .command() chains may miss some patterns
 *   - Does not handle dynamic command registration
 *   - Does not extract .argument() definitions (only .command() positionals)
 *   - Does not handle Commander subclass patterns
 *
 * Maturity: PROTOTYPE. Sufficient to extract repo-graph's own CLI surface.
 */

import type { BoundaryProviderFact } from "../../../core/classification/api-boundary.js";

/**
 * A detected Commander command registration.
 * Internal working type — not exported.
 */
interface DetectedCommand {
	/** The raw command name + args string from .command("name <args>"). */
	rawCommand: string;
	/** Just the command name (first word, no args). */
	name: string;
	/** Positional args extracted from the command string. */
	positionalArgs: string[];
	/** Description from .description("..."). */
	description: string | null;
	/** Options from .option("--flag", "..."). */
	options: string[];
	/** Whether this command has an .action() handler. */
	hasAction: boolean;
	/** The receiver variable name (e.g., "program", "repo", "graph"). */
	receiver: string;
	/** Source line number (1-indexed). */
	lineStart: number;
}

/**
 * Extract Commander CLI command provider facts from a TS/JS source file.
 *
 * @param source - Full source text.
 * @param filePath - Repo-relative path.
 * @param _repoUid - Repository UID (unused, kept for interface consistency).
 * @param symbols - Already-extracted symbols for handler attribution.
 */
export function extractCommanderCommands(
	source: string,
	filePath: string,
	_repoUid: string,
	symbols: Array<{
		stableKey: string;
		name: string;
		lineStart: number | null;
	}>,
): BoundaryProviderFact[] {
	// Quick gate: skip files that don't use Commander.
	if (!hasCommanderImport(source)) return [];

	const lines = source.split("\n");
	const commands: DetectedCommand[] = [];

	// Pass 1: find .command() registrations.
	// Track variable assignments: `const repo = program.command("repo")`
	// Maps variable name → index in the commands array. Using index
	// (not command name) ensures unambiguous identity when two parents
	// both define a subcommand with the same name.
	const variableToCommandIndex = new Map<string, number>();

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];

		// Pattern A: <receiver>.command("name <args>") — same line.
		// Pattern B: .command("name <args>") — continuation line, receiver on prior line.
		let cmdMatch = line.match(
			/(\w+)\s*\.\s*command\s*\(\s*["'`]([^"'`]+)["'`]\s*\)/,
		);

		// If not matched on same line, check for continuation pattern:
		// previous line ends with assignment or receiver, this line starts with .command
		if (!cmdMatch) {
			const contMatch = line.match(
				/^\s*\.command\s*\(\s*["'`]([^"'`]+)["'`]\s*\)/,
			);
			if (contMatch) {
				// Walk backwards to find the receiver.
				const recv = findReceiverAbove(lines, i);
				if (recv) {
					cmdMatch = [line, recv, contMatch[1]] as unknown as RegExpMatchArray;
				}
			}
		}

		if (!cmdMatch) continue;

		const receiver = cmdMatch[1];
		const rawCommand = cmdMatch[2];
		const name = rawCommand.split(/\s+/)[0];
		const positionalArgs = extractPositionalArgs(rawCommand);

		// Check if this is assigned to a variable on this or previous line:
		// const repo = program.command("repo")
		// OR: const repo = program\n  .command("repo")
		let assignMatch = line.match(
			/(?:const|let|var)\s+(\w+)\s*=\s*/,
		);
		if (!assignMatch && i > 0) {
			assignMatch = lines[i - 1].match(
				/(?:const|let|var)\s+(\w+)\s*=\s*\w+\s*$/,
			);
			if (!assignMatch) {
				assignMatch = lines[i - 1].match(
					/(?:const|let|var)\s+(\w+)\s*=\s*$/,
				);
			}
		}
		// Record the variable→command binding AFTER pushing to the array
		// (see below) so the index is correct.
		const pendingAssignVar = assignMatch ? assignMatch[1] : null;

		// Check for .description() on same line or next lines.
		let description: string | null = null;
		const descInLine = line.match(
			/\.description\s*\(\s*["'`]([^"'`]*)["'`]/,
		);
		if (descInLine) {
			description = descInLine[1];
		} else {
			// Check next 3 lines for chained .description().
			for (let j = i + 1; j < Math.min(i + 4, lines.length); j++) {
				const descMatch = lines[j].match(
					/^\s*\.description\s*\(\s*["'`]([^"'`]*)["'`]/,
				);
				if (descMatch) {
					description = descMatch[1];
					break;
				}
				// Stop if we hit something that isn't a chained method.
				if (!lines[j].trim().startsWith(".")) break;
			}
		}

		// Check if this command has .action() (leaf command vs group).
		const hasAction = hasActionHandler(lines, i);

		// Collect options from chained .option() calls.
		const options = collectOptions(lines, i);

		commands.push({
			rawCommand,
			name,
			positionalArgs,
			description,
			options,
			hasAction,
			receiver,
			lineStart: i + 1,
		});

		// Now that the command is in the array, record the variable binding.
		if (pendingAssignVar) {
			variableToCommandIndex.set(pendingAssignVar, commands.length - 1);
		}
	}

	// Pass 2: compose command paths and emit facts.
	// A command's full path is: parent path + own name.
	// Parent is determined by: receiver variable name → previously
	// registered command name via variableToCommand lookup.
	const facts: BoundaryProviderFact[] = [];

	// Build variable-name → DetectedCommand for parent lookups.
	// Uses the index recorded during Pass 1 — no ambiguous name search.
	const commandByVariable = new Map<string, DetectedCommand>();
	for (const [varName, idx] of variableToCommandIndex) {
		commandByVariable.set(varName, commands[idx]);
	}

	for (const cmd of commands) {
		// Determine parent command path.
		let parentPath = "";
		// Check if receiver is a known command variable.
		const parentCmd = commandByVariable.get(cmd.receiver);
		if (parentCmd) {
			// Recursively build parent path.
			parentPath = buildCommandPath(
				parentCmd,
				commandByVariable,
				0,
			);
		}

		const commandPath = parentPath
			? `${parentPath} ${cmd.name}`
			: cmd.name;

		// Only emit facts for leaf commands (with .action handler)
		// or group commands that have a description (they represent
		// a real command surface even if subcommands handle the work).
		if (!cmd.hasAction && !cmd.description) continue;

		const handler = findEnclosingSymbol(cmd.lineStart, symbols);

		facts.push({
			mechanism: "cli_command",
			operation: `CLI ${commandPath}`,
			address: commandPath,
			handlerStableKey:
				handler?.stableKey ?? `unknown:${filePath}:${cmd.lineStart}`,
			sourceFile: filePath,
			lineStart: cmd.lineStart,
			framework: "commander",
			basis: "registration",
			schemaRef: null,
			metadata: {
				rawCommand: cmd.rawCommand,
				description: cmd.description,
				positionalArgs: cmd.positionalArgs,
				options: cmd.options,
				hasAction: cmd.hasAction,
				receiver: cmd.receiver,
			},
		});
	}

	return facts;
}

// ── Helpers ─────────────────────────────────────────────────────────

/**
 * Walk backwards from a continuation line (.command on its own line)
 * to find the receiver identifier. Looks for patterns like:
 *   program
 *     .command(...)
 * or:
 *   boundary
 *     .command(...)
 */
function findReceiverAbove(lines: string[], currentLine: number): string | null {
	for (let j = currentLine - 1; j >= Math.max(0, currentLine - 3); j--) {
		const prevLine = lines[j].trim();
		// Line ends with just an identifier (the receiver on its own line).
		const bareIdMatch = prevLine.match(/^(\w+)$/);
		if (bareIdMatch) return bareIdMatch[1];
		// Line has an assignment ending with the receiver:
		// const boundary = program
		const assignEndMatch = prevLine.match(/=\s*(\w+)$/);
		if (assignEndMatch) return assignEndMatch[1];
		// Line is a chained call ending with the receiver for next chain:
		// ...description("..."); or similar — the receiver might be on a prior line.
		// If the line starts with . it's still a chain, keep looking.
		if (prevLine.startsWith(".")) continue;
		// Line has receiver.method() pattern — extract receiver.
		const receiverMatch = prevLine.match(/^(\w+)\s*\./);
		if (receiverMatch) return receiverMatch[1];
		// If we hit a non-continuation line, stop.
		break;
	}
	return null;
}

function hasCommanderImport(source: string): boolean {
	return (
		source.includes("'commander'") ||
		source.includes('"commander"') ||
		source.includes("from 'commander'") ||
		source.includes('from "commander"')
	);
}

/**
 * Extract positional arguments from a command string.
 * "add <path>" → ["<path>"]
 * "index [repo]" → ["[repo]"]
 * "summary <repo>" → ["<repo>"]
 */
function extractPositionalArgs(rawCommand: string): string[] {
	const args: string[] = [];
	const matches = rawCommand.matchAll(/<[^>]+>|\[[^\]]+\]/g);
	for (const m of matches) {
		args.push(m[0]);
	}
	return args;
}

/**
 * Check if a command registration has an .action() handler
 * on the same line or within the next few lines.
 */
function hasActionHandler(lines: string[], startLine: number): boolean {
	// Check the current line first.
	if (lines[startLine].includes(".action(")) return true;
	// Check next 10 lines for chained .action().
	for (let j = startLine + 1; j < Math.min(startLine + 11, lines.length); j++) {
		const trimmed = lines[j].trim();
		if (trimmed.startsWith(".action(")) return true;
		if (trimmed.includes(".action(")) return true;
		// Stop at next command registration or function.
		if (trimmed.match(/^\w+\.\s*command\s*\(/)) break;
		if (trimmed.match(/^(export\s+)?function\s/)) break;
		if (trimmed.match(/^(export\s+)?(const|let|var)\s+\w+\s*=/)) break;
	}
	return false;
}

/**
 * Collect --option flags from chained .option() calls.
 */
function collectOptions(lines: string[], startLine: number): string[] {
	const options: string[] = [];
	for (let j = startLine; j < Math.min(startLine + 15, lines.length); j++) {
		const optMatch = lines[j].match(
			/\.option\s*\(\s*["'`](--?[\w-]+(?:\s+<\w+>)?)/,
		);
		if (optMatch) {
			options.push(optMatch[1]);
		}
		// Stop at next command registration.
		if (j > startLine && lines[j].trim().match(/^\w+\.\s*command\s*\(/)) break;
	}
	return options;
}

/**
 * Recursively build the command path for a detected command.
 * Walks up the parent chain via variable → command lookup.
 */
function buildCommandPath(
	cmd: DetectedCommand,
	commandByVariable: Map<string, DetectedCommand>,
	depth: number,
): string {
	if (depth > 5) return cmd.name; // Cycle guard.

	const parentCmd = commandByVariable.get(cmd.receiver);
	if (!parentCmd || parentCmd === cmd) return cmd.name;

	const parentPath = buildCommandPath(
		parentCmd,
		commandByVariable,
		depth + 1,
	);
	return `${parentPath} ${cmd.name}`;
}

function findEnclosingSymbol(
	lineNumber: number,
	symbols: Array<{ stableKey: string; name: string; lineStart: number | null }>,
): { stableKey: string } | null {
	let best: { stableKey: string; lineStart: number } | null = null;
	for (const s of symbols) {
		if (s.lineStart === null) continue;
		if (s.lineStart <= lineNumber) {
			if (!best || s.lineStart > best.lineStart) {
				best = { stableKey: s.stableKey, lineStart: s.lineStart };
			}
		}
	}
	return best;
}
