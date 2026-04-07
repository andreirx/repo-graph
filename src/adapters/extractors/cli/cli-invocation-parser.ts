/**
 * CLI invocation parser — shared support module.
 *
 * Conservative command-invocation extraction from shell-like strings.
 * Used by both package.json script consumer and shell script consumer.
 *
 * Responsibilities:
 *   - Split shell chains (&&, ||, ;)
 *   - Unwrap execution wrappers (npx, pnpm exec, yarn exec)
 *   - Detect script indirection (npm run, pnpm test, etc.)
 *   - Parse command invocations into binary + subcommand path + args
 *   - Filter shell builtins
 *   - Strip environment variable prefixes
 *
 * Does NOT handle:
 *   - Pipes (|) — skipped entirely, not partially extracted
 *   - Subshells ($(...), `...`)
 *   - Variable expansion (${VAR})
 *   - Here-docs
 *   - Quoted strings containing chain operators
 *   - Function definitions
 *   - Control flow (if, for, while, case)
 */

// ── Shell builtins ──────────────────────────────────────────────────

/** Shell built-ins and non-tool commands to skip. */
export const SHELL_BUILTINS = new Set([
	".", "cd", "echo", "exit", "export", "set", "unset", "source",
	"true", "false", "test", "read", "eval", "exec",
	"rm", "mkdir", "cp", "mv", "cat", "ls", "pwd", "touch",
	"chmod", "chown", "grep", "sed", "awk", "find", "xargs",
	"sleep", "wait", "kill", "trap", "shift", "return",
	"local", "declare", "typeset", "readonly",
]);

// ── Script indirection ──────────────────────────────────────────────

/**
 * Package manager run commands — these are script indirection, not direct invocations.
 * Note: "npm exec", "pnpm exec", "yarn exec" are NOT indirection — they execute
 * a binary directly. Those are handled by EXEC_WRAPPERS.
 */
export const SCRIPT_INDIRECTION_PREFIXES = [
	"npm run", "npm test", "npm start",
	"pnpm run", "pnpm test", "pnpm start",
	"yarn run", "yarn test", "yarn start",
];

// ── Execution wrappers ──────────────────────────────────────────────

/** Execution wrappers that should be unwrapped to reveal the real command. */
export const EXEC_WRAPPERS = new Set([
	"npx", "pnpx", "yarn dlx",
	"pnpm exec", "yarn exec", "npm exec",
]);

// ── Output type ─────────────────────────────────────────────────────

export interface ParsedCommand {
	/** The binary name (first token). */
	binary: string;
	/**
	 * The command path: binary + subcommand words.
	 * "vitest run" → commandPath = "vitest run"
	 * "tsc" → commandPath = "tsc"
	 * "biome check" → commandPath = "biome check"
	 */
	commandPath: string;
	/** Remaining arguments after the command path. */
	args: string[];
	/** Confidence in the extraction. */
	confidence: number;
}

// ── Core functions ──────────────────────────────────────────────────

/**
 * Split a shell command string on chain operators: &&, ||, ;
 * Does NOT handle quoted strings containing these operators.
 * Conservative: treats each segment as an independent command.
 */
export function splitShellChain(script: string): string[] {
	return script.split(/\s*(?:&&|\|\||;)\s*/);
}

/**
 * Check if a command segment is script indirection.
 */
export function isScriptIndirection(segment: string): boolean {
	const lower = segment.toLowerCase();
	return SCRIPT_INDIRECTION_PREFIXES.some((prefix) =>
		lower.startsWith(prefix),
	);
}

/**
 * Unwrap execution wrappers to reveal the underlying command.
 * "npx vitest run" → "vitest run"
 * "pnpm exec rgr boundary summary" → "rgr boundary summary"
 */
export function unwrapExecWrapper(segment: string): string {
	for (const wrapper of EXEC_WRAPPERS) {
		if (segment.startsWith(wrapper + " ")) {
			return segment.slice(wrapper.length + 1).trim();
		}
	}
	return segment;
}

/**
 * Check if a segment contains a pipe. Piped commands are skipped
 * entirely in the first slice — extracting only the first or last
 * command in a pipe produces misleading facts.
 */
export function containsPipe(segment: string): boolean {
	// Match | but not || (which is a chain operator handled by splitShellChain).
	return /(?<!\|)\|(?!\|)/.test(segment);
}

/**
 * Strip leading environment variable assignments from a command.
 * "NODE_ENV=test vitest run" → "vitest run"
 * "FOO=bar BAZ=qux command" → "command"
 */
export function stripEnvPrefix(segment: string): string {
	return segment.replace(/^(?:[A-Z_][A-Z0-9_]*=[^\s]*\s+)+/, "");
}

/**
 * Parse a command invocation string into structured parts.
 *
 * Heuristic: the command path consists of the binary name followed
 * by words that look like subcommands (lowercase, no leading dash,
 * no path separators). Stops at the first argument-like token.
 *
 * "vitest run test/adapters" → binary="vitest", path="vitest run", args=["test/adapters"]
 * "tsc --watch" → binary="tsc", path="tsc", args=["--watch"]
 * "biome check src/ test/" → binary="biome", path="biome check", args=["src/", "test/"]
 */
export function parseCommandInvocation(segment: string): ParsedCommand | null {
	const tokens = segment.split(/\s+/).filter((t) => t.length > 0);
	if (tokens.length === 0) return null;

	const binary = tokens[0];

	// Skip if binary looks like a path (./script.sh, /usr/bin/foo).
	if (binary.includes("/") && !binary.startsWith("node_modules/")) return null;

	// Build command path: binary + subsequent subcommand-like tokens.
	const pathTokens = [binary];
	let argStart = 1;

	for (let i = 1; i < tokens.length; i++) {
		const token = tokens[i];
		// Stop at flags (--flag, -f).
		if (token.startsWith("-")) break;
		// Stop at paths (contains /).
		if (token.includes("/")) break;
		// Stop at glob patterns (contains *).
		if (token.includes("*")) break;
		// Stop at tokens that look like arguments (contain = or .).
		if (token.includes("=")) break;
		if (token.includes(".") && !token.startsWith(".")) break;
		// Subcommand-like: lowercase word.
		if (/^[a-z][\w-]*$/.test(token)) {
			pathTokens.push(token);
			argStart = i + 1;
		} else {
			break;
		}
	}

	return {
		binary,
		commandPath: pathTokens.join(" "),
		args: tokens.slice(argStart),
		confidence: 0.85,
	};
}

/**
 * Full pipeline: process a raw command string into a ParsedCommand.
 * Returns null if the command should be skipped (indirection, builtin,
 * pipe, unparseable).
 */
export function processCommandSegment(raw: string): ParsedCommand | null {
	const trimmed = raw.trim();
	if (!trimmed) return null;

	// Skip script indirection.
	if (isScriptIndirection(trimmed)) return null;

	// Unwrap execution wrappers.
	const unwrapped = unwrapExecWrapper(trimmed);

	// Skip pipes.
	if (containsPipe(unwrapped)) return null;

	// Strip env prefixes.
	const stripped = stripEnvPrefix(unwrapped);

	// Parse the command.
	const parsed = parseCommandInvocation(stripped);
	if (!parsed) return null;

	// Skip shell builtins.
	if (SHELL_BUILTINS.has(parsed.binary)) return null;

	return parsed;
}
