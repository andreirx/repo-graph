/**
 * Environment variable access detectors — pure functions.
 *
 * Scan source file content for env var access patterns.
 * Return DetectedEnvDependency[] for each file.
 *
 * Cross-language: TS/JS, Python, Rust, Java, C/C++.
 * All detectors require a string literal var name — dynamic
 * access (process.env[variable]) is not detected.
 *
 * No filesystem, no AST. Line-based regex scanning.
 */

import { maskCommentsForFile } from "./comment-masker.js";
import type {
	DetectedEnvDependency,
	EnvAccessKind,
} from "./env-dependency.js";

/**
 * Detect env var accesses in a source file.
 * Dispatches to language-specific detectors based on file extension.
 *
 * Source content is run through a positional comment masker first so
 * that env-access patterns inside line comments, block comments, and
 * JSDoc do not produce false positives. The masker preserves length
 * and newline positions, so detector line numbers remain stable.
 */
export function detectEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const ext = filePath.slice(filePath.lastIndexOf("."));
	const masked = maskCommentsForFile(filePath, content);
	switch (ext) {
		case ".ts":
		case ".tsx":
		case ".js":
		case ".jsx":
			return detectJsEnvAccesses(masked, filePath);
		case ".py":
			return detectPythonEnvAccesses(masked, filePath);
		case ".rs":
			return detectRustEnvAccesses(masked, filePath);
		case ".java":
			return detectJavaEnvAccesses(masked, filePath);
		case ".c":
		case ".h":
		case ".cpp":
		case ".hpp":
		case ".cc":
		case ".cxx":
			return detectCEnvAccesses(masked, filePath);
		default:
			return [];
	}
}

// ── JS/TS ──────────────────────────────────────────────────────────

function detectJsEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const results: DetectedEnvDependency[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// process.env.VAR_NAME
		const dotMatches = line.matchAll(/process\.env\.([A-Z_][A-Z0-9_]*)/g);
		for (const m of dotMatches) {
			const varName = m[1];
			results.push({
				varName,
				accessKind: inferJsAccessKind(line, varName),
				accessPattern: "process_env_dot",
				filePath,
				lineNumber,
				defaultValue: extractJsDefault(line, varName),
				confidence: 0.95,
			});
		}

		// process.env["VAR_NAME"] or process.env['VAR_NAME']
		const bracketMatches = line.matchAll(/process\.env\[["']([A-Z_][A-Z0-9_]*)["']\]/g);
		for (const m of bracketMatches) {
			const varName = m[1];
			results.push({
				varName,
				accessKind: inferJsAccessKind(line, varName),
				accessPattern: "process_env_bracket",
				filePath,
				lineNumber,
				defaultValue: extractJsDefault(line, varName),
				confidence: 0.95,
			});
		}

		// const { VAR1, VAR2 } = process.env
		const destructureMatch = line.match(
			/(?:const|let|var)\s*\{([^}]+)\}\s*=\s*process\.env/,
		);
		if (destructureMatch) {
			const vars = destructureMatch[1].split(",").map((v) => v.trim());
			for (const v of vars) {
				// Handle renaming: VAR_NAME: localName
				const varName = v.includes(":") ? v.split(":")[0].trim() : v;
				if (/^[A-Z_][A-Z0-9_]*$/.test(varName)) {
					results.push({
						varName,
						accessKind: "unknown",
						accessPattern: "process_env_destructure",
						filePath,
						lineNumber,
						defaultValue: null,
						confidence: 0.90,
					});
				}
			}
		}
	}

	return deduplicateByVarAndFile(results);
}

// ── Python ─────────────────────────────────────────────────────────

function detectPythonEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const results: DetectedEnvDependency[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// os.environ["VAR"] or os.environ['VAR']
		const environMatches = line.matchAll(/os\.environ\[["']([A-Z_][A-Z0-9_]*)["']\]/g);
		for (const m of environMatches) {
			results.push({
				varName: m[1],
				accessKind: "required",
				accessPattern: "os_environ",
				filePath,
				lineNumber,
				defaultValue: null,
				confidence: 0.95,
			});
		}

		// os.environ.get("VAR") or os.environ.get("VAR", "default")
		const getMatches = line.matchAll(/os\.environ\.get\(["']([A-Z_][A-Z0-9_]*)["'](?:\s*,\s*["']([^"']*)["'])?\)/g);
		for (const m of getMatches) {
			results.push({
				varName: m[1],
				accessKind: m[2] !== undefined ? "optional" : "unknown",
				accessPattern: "os_environ",
				filePath,
				lineNumber,
				defaultValue: m[2] ?? null,
				confidence: 0.95,
			});
		}

		// os.getenv("VAR") or os.getenv("VAR", "default")
		const getenvMatches = line.matchAll(/os\.getenv\(["']([A-Z_][A-Z0-9_]*)["'](?:\s*,\s*["']([^"']*)["'])?\)/g);
		for (const m of getenvMatches) {
			results.push({
				varName: m[1],
				accessKind: m[2] !== undefined ? "optional" : "unknown",
				accessPattern: "os_getenv",
				filePath,
				lineNumber,
				defaultValue: m[2] ?? null,
				confidence: 0.95,
			});
		}
	}

	return deduplicateByVarAndFile(results);
}

// ── Rust ───────────────────────────────────────────────────────────

function detectRustEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const results: DetectedEnvDependency[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// std::env::var("VAR") or env::var("VAR")
		const varMatches = line.matchAll(/(?:std::)?env::var(?:_os)?\("([A-Z_][A-Z0-9_]*)"\)/g);
		for (const m of varMatches) {
			results.push({
				varName: m[1],
				accessKind: "unknown",
				accessPattern: "std_env_var",
				filePath,
				lineNumber,
				defaultValue: null,
				confidence: 0.95,
			});
		}
	}

	return deduplicateByVarAndFile(results);
}

// ── Java ───────────────────────────────────────────────────────────

function detectJavaEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const results: DetectedEnvDependency[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// System.getenv("VAR")
		const getenvMatches = line.matchAll(/System\.getenv\("([A-Z_][A-Z0-9_]*)"\)/g);
		for (const m of getenvMatches) {
			results.push({
				varName: m[1],
				accessKind: "unknown",
				accessPattern: "system_getenv",
				filePath,
				lineNumber,
				defaultValue: null,
				confidence: 0.95,
			});
		}
	}

	return deduplicateByVarAndFile(results);
}

// ── C/C++ ──────────────────────────────────────────────────────────

function detectCEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const results: DetectedEnvDependency[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// getenv("VAR")
		const getenvMatches = line.matchAll(/getenv\("([A-Z_][A-Z0-9_]*)"\)/g);
		for (const m of getenvMatches) {
			results.push({
				varName: m[1],
				accessKind: "unknown",
				accessPattern: "c_getenv",
				filePath,
				lineNumber,
				defaultValue: null,
				confidence: 0.95,
			});
		}
	}

	return deduplicateByVarAndFile(results);
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Infer required vs optional for JS env access.
 * - `process.env.X || "fallback"` → optional
 * - `process.env.X ?? "fallback"` → optional
 * - bare `process.env.X` → required
 */
function inferJsAccessKind(line: string, varName: string): EnvAccessKind {
	// Check for fallback patterns after the var access.
	const afterVar = line.slice(line.indexOf(varName) + varName.length);
	if (/^\s*(\|\||&&|\?\?)/.test(afterVar)) return "optional";
	if (/^\s*\)/.test(afterVar)) return "unknown"; // inside a function call
	return "required";
}

/**
 * Extract default value from JS fallback patterns.
 * `process.env.X || "default"` → "default"
 * `process.env.X ?? "default"` → "default"
 */
function extractJsDefault(line: string, varName: string): string | null {
	const afterVar = line.slice(line.indexOf(varName) + varName.length);
	const match = afterVar.match(/^\s*(?:\|\||&&|\?\?)\s*["']([^"']+)["']/);
	return match ? match[1] : null;
}

/**
 * Deduplicate: same var name + same file → keep first occurrence.
 * Multiple accesses to the same var in the same file are one dependency.
 */
function deduplicateByVarAndFile(deps: DetectedEnvDependency[]): DetectedEnvDependency[] {
	const seen = new Set<string>();
	return deps.filter((d) => {
		const key = `${d.filePath}:${d.varName}`;
		if (seen.has(key)) return false;
		seen.add(key);
		return true;
	});
}
