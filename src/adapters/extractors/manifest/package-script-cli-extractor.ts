/**
 * package.json script CLI consumer extractor.
 *
 * Parses package.json "scripts" entries and emits BoundaryConsumerFact
 * records with mechanism `cli_command` for each direct tool invocation.
 *
 * Supported patterns:
 *   Direct binary invocation:
 *     "build": "tsc"
 *     "lint": "biome check src/"
 *     "test": "vitest run test/adapters"
 *
 *   Chained commands (&&, ||, ;):
 *     "test": "tsc && vitest run"
 *     → emits two consumer facts: "tsc" and "vitest run"
 *
 *   npx/pnpm exec/yarn exec unwrapping:
 *     "check": "npx vitest run"
 *     → emits consumer fact for "vitest run" (unwraps npx prefix)
 *
 * NOT extracted in this first slice:
 *   - Script indirection: "npm run build", "pnpm test" (these invoke
 *     other package.json scripts, not direct binaries)
 *   - Shell subshells: $(command), `command`
 *   - Environment variable prefixes: NODE_ENV=test vitest
 *   - Pipe targets: command | other
 *   - Redirects: command > file
 *   - Shell built-ins that are not tool invocations (cd, echo, exit)
 *
 * Maturity: PROTOTYPE. Sufficient to extract repo-graph's own script
 * surface and demonstrate end-to-end cli_command boundary linking.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";

/** Shell built-ins and non-tool commands to skip. */
const SHELL_BUILTINS = new Set([
	"cd", "echo", "exit", "export", "set", "unset", "source",
	"true", "false", "test", "read", "eval", "exec",
	"rm", "mkdir", "cp", "mv", "cat", "ls", "pwd", "touch",
	"chmod", "chown", "grep", "sed", "awk", "find", "xargs",
	"sleep", "wait", "kill",
]);

/**
 * Package manager run commands — these are script indirection, not direct invocations.
 * Note: "npm exec", "pnpm exec", "yarn exec" are NOT indirection — they execute
 * a binary directly. Those are handled by EXEC_WRAPPERS and must NOT appear here.
 */
const SCRIPT_INDIRECTION_PREFIXES = [
	"npm run", "npm test", "npm start",
	"pnpm run", "pnpm test", "pnpm start",
	"yarn run", "yarn test", "yarn start",
];

/** Execution wrappers that should be unwrapped to reveal the real command. */
const EXEC_WRAPPERS = new Set([
	"npx", "pnpx", "yarn dlx",
	"pnpm exec", "yarn exec", "npm exec",
]);

/**
 * Extract CLI consumer facts from package.json scripts.
 *
 * @param scripts - The "scripts" object from package.json.
 * @param filePath - Repo-relative path to the package.json file.
 * @param repoUid - Repository UID.
 */
export function extractPackageScriptConsumers(
	scripts: Record<string, string>,
	filePath: string,
	_repoUid: string,
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];

	for (const [scriptName, scriptBody] of Object.entries(scripts)) {
		if (!scriptBody || typeof scriptBody !== "string") continue;

		// Split on shell operators: &&, ||, ;
		const segments = splitShellChain(scriptBody);

		for (const segment of segments) {
			const trimmed = segment.trim();
			if (!trimmed) continue;

			// Skip script indirection (npm run build, pnpm test, etc.).
			// This runs on the original trimmed segment, not on unwrapped.
			// Exec wrappers (npm exec, npx, pnpm exec) are NOT in the
			// indirection list — they are direct tool invocations, handled
			// by unwrapExecWrapper below.
			if (isScriptIndirection(trimmed)) continue;

			// Unwrap execution wrappers (npx, pnpm exec, etc.).
			const unwrapped = unwrapExecWrapper(trimmed);

			// Parse the command into binary + subcommand path.
			const parsed = parseCommandInvocation(unwrapped);
			if (!parsed) continue;

			// Skip shell built-ins.
			if (SHELL_BUILTINS.has(parsed.binary)) continue;

			facts.push({
				mechanism: "cli_command",
				operation: `CLI ${parsed.commandPath}`,
				address: parsed.commandPath,
				callerStableKey: `${filePath}:scripts.${scriptName}`,
				sourceFile: filePath,
				lineStart: 0, // package.json doesn't have meaningful line numbers per script.
				basis: "literal",
				confidence: parsed.confidence,
				schemaRef: null,
				metadata: {
					scriptName,
					rawCommand: trimmed.slice(0, 100),
					binary: parsed.binary,
					args: parsed.args,
				},
			});
		}
	}

	return facts;
}

// ── Parsing helpers ─────────────────────────────────────────────────

/**
 * Split a shell command string on chain operators: &&, ||, ;
 * Does NOT handle quoted strings containing these operators.
 * Conservative: treats each segment as an independent command.
 */
function splitShellChain(script: string): string[] {
	return script.split(/\s*(?:&&|\|\||;)\s*/);
}

/**
 * Check if a command segment is script indirection.
 */
function isScriptIndirection(segment: string): boolean {
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
function unwrapExecWrapper(segment: string): string {
	for (const wrapper of EXEC_WRAPPERS) {
		if (segment.startsWith(wrapper + " ")) {
			return segment.slice(wrapper.length + 1).trim();
		}
	}
	return segment;
}

interface ParsedCommand {
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
function parseCommandInvocation(segment: string): ParsedCommand | null {
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
