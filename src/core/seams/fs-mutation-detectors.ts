/**
 * Filesystem mutation detectors — pure functions.
 *
 * Scan source file content for mutation operations.
 * Cross-language: TS/JS, Python, Rust, Java, C/C++.
 *
 * Slice 1: mutation only — no reads.
 * Literal first-argument paths are captured. Dynamic paths produce
 * detections with `targetPath: null` and `dynamicPath: true`.
 *
 * No filesystem, no AST. Line-based regex scanning.
 */

import { maskCommentsForFile } from "./comment-masker.js";
import type {
	DetectedFsMutation,
	MutationKind,
	MutationPattern,
} from "./fs-mutation.js";

/**
 * Detect filesystem mutation occurrences in a source file.
 *
 * Source content is run through a positional comment masker first so
 * that fs-mutation patterns inside line comments, block comments, and
 * JSDoc do not produce false positives. The masker preserves length
 * and newline positions, so detector line numbers remain stable.
 */
export function detectFsMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const ext = filePath.slice(filePath.lastIndexOf("."));
	const masked = maskCommentsForFile(filePath, content);
	switch (ext) {
		case ".ts":
		case ".tsx":
		case ".js":
		case ".jsx":
			return detectJsMutations(masked, filePath);
		case ".py":
			return detectPythonMutations(masked, filePath);
		case ".rs":
			return detectRustMutations(masked, filePath);
		case ".java":
			return detectJavaMutations(masked, filePath);
		case ".c":
		case ".h":
		case ".cpp":
		case ".hpp":
		case ".cc":
		case ".cxx":
			return detectCMutations(masked, filePath);
		default:
			return [];
	}
}

// ── JS/TS ──────────────────────────────────────────────────────────

interface JsPattern {
	regex: RegExp;
	kind: MutationKind;
	pattern: MutationPattern;
	/** True if this is a two-ended op (rename, copy) — capture destination too. */
	twoEnded?: boolean;
}

const JS_PATTERNS: JsPattern[] = [
	{ regex: /\bfs\.writeFile(?:Sync)?\s*\(/g, kind: "write_file", pattern: "fs_write_file" },
	{ regex: /\bfs\.appendFile(?:Sync)?\s*\(/g, kind: "append_file", pattern: "fs_append_file" },
	{ regex: /\bfs\.unlink(?:Sync)?\s*\(/g, kind: "delete_path", pattern: "fs_unlink" },
	{ regex: /\bfs\.rm(?:Sync)?\s*\(/g, kind: "delete_path", pattern: "fs_rm" },
	{ regex: /\bfs\.mkdir(?:Sync)?\s*\(/g, kind: "create_dir", pattern: "fs_mkdir" },
	{ regex: /\bfs\.createWriteStream\s*\(/g, kind: "write_file", pattern: "fs_create_write_stream" },
	{ regex: /\bfs\.rename(?:Sync)?\s*\(/g, kind: "rename_path", pattern: "fs_rename", twoEnded: true },
	{ regex: /\bfs\.copyFile(?:Sync)?\s*\(/g, kind: "copy_path", pattern: "fs_copy_file", twoEnded: true },
	{ regex: /\bfs\.chmod(?:Sync)?\s*\(/g, kind: "chmod_path", pattern: "fs_chmod" },
];

function detectJsMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		for (const p of JS_PATTERNS) {
			p.regex.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = p.regex.exec(line)) !== null) {
				const after = line.slice(m.index + m[0].length);
				const pathLiteral = extractFirstStringArg(after);
				let destinationPath: string | null = null;
				if (p.twoEnded && pathLiteral !== null) {
					// Skip past the first literal + comma to find the second arg.
					destinationPath = extractSecondStringArg(after);
				}
				results.push({
					filePath,
					lineNumber,
					mutationKind: p.kind,
					mutationPattern: p.pattern,
					targetPath: pathLiteral,
					destinationPath,
					dynamicPath: pathLiteral === null,
					confidence: 0.90,
				});
			}
		}
	}

	return results;
}

// ── Python ─────────────────────────────────────────────────────────

function detectPythonMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// open(path, "w") / open(path, "a")
		const openMatches = line.matchAll(/\bopen\s*\(\s*(?:["']([^"']+)["']|[^,)]+)\s*,\s*["']([wa])[^"']*["']/g);
		for (const m of openMatches) {
			const path = m[1] ?? null;
			const mode = m[2];
			results.push({
				filePath,
				lineNumber,
				mutationKind: mode === "a" ? "append_file" : "write_file",
				mutationPattern: mode === "a" ? "py_open_append" : "py_open_write",
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.85,
			});
		}

		// os.remove(path), os.unlink(path), shutil.rmtree(path)
		const removeMatches = line.matchAll(/\b(os\.(?:remove|unlink)|shutil\.rmtree)\s*\(\s*(?:["']([^"']+)["']|[^)]+)\s*\)/g);
		for (const m of removeMatches) {
			const callName = m[1];
			const path = m[2] ?? null;
			let pattern: MutationPattern = "py_os_remove";
			if (callName === "os.unlink") pattern = "py_os_unlink";
			else if (callName === "shutil.rmtree") pattern = "py_shutil_rmtree";
			results.push({
				filePath,
				lineNumber,
				mutationKind: "delete_path",
				mutationPattern: pattern,
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.90,
			});
		}

		// os.mkdir(path), os.makedirs(path)
		const mkdirMatches = line.matchAll(/\bos\.(mkdir|makedirs)\s*\(\s*(?:["']([^"']+)["']|[^)]+)/g);
		for (const m of mkdirMatches) {
			const callName = m[1];
			const path = m[2] ?? null;
			results.push({
				filePath,
				lineNumber,
				mutationKind: "create_dir",
				mutationPattern: callName === "makedirs" ? "py_os_makedirs" : "py_os_mkdir",
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.90,
			});
		}

		// pathlib Path("...").write_text / write_bytes / mkdir
		const pathlibWriteMatches = line.matchAll(/Path\s*\(\s*["']([^"']+)["']\s*\)\.write_(?:text|bytes)/g);
		for (const m of pathlibWriteMatches) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "write_file",
				mutationPattern: "py_pathlib_write",
				targetPath: m[1],
				dynamicPath: false,
				confidence: 0.85,
			});
		}

		const pathlibMkdirMatches = line.matchAll(/Path\s*\(\s*["']([^"']+)["']\s*\)\.mkdir/g);
		for (const m of pathlibMkdirMatches) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "create_dir",
				mutationPattern: "py_pathlib_mkdir",
				targetPath: m[1],
				dynamicPath: false,
				confidence: 0.85,
			});
		}

		// tempfile.NamedTemporaryFile, tempfile.mkstemp
		if (/\btempfile\.(?:NamedTemporaryFile|mkstemp|TemporaryFile)/.test(line)) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "create_temp",
				mutationPattern: "py_tempfile",
				targetPath: null,
				dynamicPath: true,
				confidence: 0.90,
			});
		}
	}

	return results;
}

// ── Rust ───────────────────────────────────────────────────────────

function detectRustMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	const patterns: Array<{ rx: RegExp; kind: MutationKind; pattern: MutationPattern; twoEnded?: boolean }> = [
		{ rx: /(?:std::)?fs::write\s*\(\s*"([^"]+)"/g, kind: "write_file", pattern: "rust_fs_write" },
		{ rx: /(?:std::)?fs::write\s*\(/g, kind: "write_file", pattern: "rust_fs_write" },
		{ rx: /(?:std::)?fs::remove_file\s*\(\s*"([^"]+)"/g, kind: "delete_path", pattern: "rust_fs_remove_file" },
		{ rx: /(?:std::)?fs::remove_file\s*\(/g, kind: "delete_path", pattern: "rust_fs_remove_file" },
		{ rx: /(?:std::)?fs::remove_dir_all\s*\(\s*"([^"]+)"/g, kind: "delete_path", pattern: "rust_fs_remove_dir_all" },
		{ rx: /(?:std::)?fs::remove_dir_all\s*\(/g, kind: "delete_path", pattern: "rust_fs_remove_dir_all" },
		{ rx: /(?:std::)?fs::create_dir(?:_all)?\s*\(\s*"([^"]+)"/g, kind: "create_dir", pattern: "rust_fs_create_dir" },
		{ rx: /(?:std::)?fs::create_dir(?:_all)?\s*\(/g, kind: "create_dir", pattern: "rust_fs_create_dir" },
		{ rx: /(?:std::)?fs::rename\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)"/g, kind: "rename_path", pattern: "rust_fs_rename", twoEnded: true },
		{ rx: /(?:std::)?fs::rename\s*\(\s*"([^"]+)"/g, kind: "rename_path", pattern: "rust_fs_rename" },
		{ rx: /(?:std::)?fs::copy\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)"/g, kind: "copy_path", pattern: "rust_fs_copy", twoEnded: true },
		{ rx: /(?:std::)?fs::copy\s*\(\s*"([^"]+)"/g, kind: "copy_path", pattern: "rust_fs_copy" },
	];

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;
		const seenPositions = new Set<number>();

		for (const p of patterns) {
			p.rx.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = p.rx.exec(line)) !== null) {
				if (seenPositions.has(m.index)) continue;
				seenPositions.add(m.index);
				const path = m[1] ?? null;
				const destination = p.twoEnded && m[2] ? m[2] : null;
				results.push({
					filePath,
					lineNumber,
					mutationKind: p.kind,
					mutationPattern: p.pattern,
					targetPath: path,
					destinationPath: destination,
					dynamicPath: path === null,
					confidence: 0.90,
				});
			}
		}
	}

	return results;
}

// ── Java ───────────────────────────────────────────────────────────

function detectJavaMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// Files.write, Files.writeString
		if (/\bFiles\.write(?:String)?\s*\(/.test(line)) {
			const literal = line.match(/Files\.write(?:String)?\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/);
			results.push({
				filePath,
				lineNumber,
				mutationKind: "write_file",
				mutationPattern: "java_files_write",
				targetPath: literal ? literal[1] : null,
				dynamicPath: literal === null,
				confidence: 0.85,
			});
		}

		// Files.delete
		if (/\bFiles\.delete\s*\(/.test(line)) {
			const literal = line.match(/Files\.delete\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/);
			results.push({
				filePath,
				lineNumber,
				mutationKind: "delete_path",
				mutationPattern: "java_files_delete",
				targetPath: literal ? literal[1] : null,
				dynamicPath: literal === null,
				confidence: 0.85,
			});
		}

		// Files.createDirectory / Files.createDirectories
		if (/\bFiles\.createDirector(?:y|ies)\s*\(/.test(line)) {
			const literal = line.match(/createDirector(?:y|ies)\s*\(\s*Paths\.get\s*\(\s*"([^"]+)"/);
			results.push({
				filePath,
				lineNumber,
				mutationKind: "create_dir",
				mutationPattern: "java_files_create_directory",
				targetPath: literal ? literal[1] : null,
				dynamicPath: literal === null,
				confidence: 0.85,
			});
		}

		// new FileOutputStream("...")
		const fosMatch = line.match(/new\s+FileOutputStream\s*\(\s*"([^"]+)"/);
		if (fosMatch) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "write_file",
				mutationPattern: "java_file_output_stream",
				targetPath: fosMatch[1],
				dynamicPath: false,
				confidence: 0.85,
			});
		} else if (/\bnew\s+FileOutputStream\s*\(/.test(line)) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "write_file",
				mutationPattern: "java_file_output_stream",
				targetPath: null,
				dynamicPath: true,
				confidence: 0.80,
			});
		}
	}

	return results;
}

// ── C/C++ ──────────────────────────────────────────────────────────

function detectCMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const results: DetectedFsMutation[] = [];
	const lines = content.split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const lineNumber = i + 1;

		// fopen(path, "w" | "a" | "wb" | "ab")
		const fopenMatches = line.matchAll(/\bfopen\s*\(\s*(?:"([^"]+)"|[^,]+)\s*,\s*"([wa][b+]?)"/g);
		for (const m of fopenMatches) {
			const path = m[1] ?? null;
			const mode = m[2];
			results.push({
				filePath,
				lineNumber,
				mutationKind: mode.startsWith("a") ? "append_file" : "write_file",
				mutationPattern: mode.startsWith("a") ? "c_fopen_append" : "c_fopen_write",
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.85,
			});
		}

		// unlink(path)
		const unlinkMatches = line.matchAll(/\bunlink\s*\(\s*(?:"([^"]+)"|[^)]+)\s*\)/g);
		for (const m of unlinkMatches) {
			const path = m[1] ?? null;
			results.push({
				filePath,
				lineNumber,
				mutationKind: "delete_path",
				mutationPattern: "c_unlink",
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.90,
			});
		}

		// remove(path)
		const removeMatches = line.matchAll(/\bremove\s*\(\s*"([^"]+)"\s*\)/g);
		for (const m of removeMatches) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "delete_path",
				mutationPattern: "c_remove",
				targetPath: m[1],
				dynamicPath: false,
				confidence: 0.85,
			});
		}

		// rmdir(path)
		const rmdirMatches = line.matchAll(/\brmdir\s*\(\s*(?:"([^"]+)"|[^)]+)\s*\)/g);
		for (const m of rmdirMatches) {
			const path = m[1] ?? null;
			results.push({
				filePath,
				lineNumber,
				mutationKind: "delete_path",
				mutationPattern: "c_rmdir",
				targetPath: path,
				dynamicPath: path === null,
				confidence: 0.90,
			});
		}

		// mkdir(path, ...)
		const mkdirMatches = line.matchAll(/\bmkdir\s*\(\s*"([^"]+)"/g);
		for (const m of mkdirMatches) {
			results.push({
				filePath,
				lineNumber,
				mutationKind: "create_dir",
				mutationPattern: "c_mkdir",
				targetPath: m[1],
				dynamicPath: false,
				confidence: 0.85,
			});
		}
	}

	return results;
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Extract the second string literal argument from a call's argument list.
 * Used for two-ended operations like rename(src, dst) and copyFile(src, dst).
 *
 * Input is the substring AFTER the matched call's opening "(".
 * Returns the second literal contents, or null if the second argument
 * is not a string literal.
 */
function extractSecondStringArg(after: string): string | null {
	// Skip the first arg (must be a string literal for this to apply).
	let i = 0;
	while (i < after.length && /\s/.test(after[i])) i++;
	if (i >= after.length) return null;
	const firstQuote = after[i];
	if (firstQuote !== '"' && firstQuote !== "'" && firstQuote !== "`") return null;
	i++;
	while (i < after.length && after[i] !== firstQuote) {
		if (after[i] === "\\") { i += 2; continue; }
		i++;
	}
	if (i >= after.length) return null;
	i++; // past closing quote of first arg
	// Skip whitespace and comma.
	while (i < after.length && /[\s,]/.test(after[i])) i++;
	if (i >= after.length) return null;
	// Now extract second string literal.
	const ch = after[i];
	if (ch !== '"' && ch !== "'" && ch !== "`") return null;
	const quote = ch;
	let result = "";
	i++;
	while (i < after.length && after[i] !== quote) {
		if (quote === "`" && after[i] === "$" && after[i + 1] === "{") return null;
		if (after[i] === "\\" && i + 1 < after.length) {
			result += after[i + 1];
			i += 2;
			continue;
		}
		result += after[i];
		i++;
	}
	if (i >= after.length) return null;
	return result;
}

/**
 * Extract a string literal from the start of an argument list.
 * Returns the literal contents if the first character (after whitespace
 * and opening paren if missing) is a quote; otherwise null.
 *
 * Input is the substring AFTER the matched call's opening "(".
 */
function extractFirstStringArg(after: string): string | null {
	// Skip leading whitespace.
	let i = 0;
	while (i < after.length && /\s/.test(after[i])) i++;
	if (i >= after.length) return null;
	const ch = after[i];
	if (ch !== '"' && ch !== "'" && ch !== "`") return null;
	const quote = ch;
	let result = "";
	i++;
	while (i < after.length && after[i] !== quote) {
		// Reject template literal interpolations as dynamic.
		if (quote === "`" && after[i] === "$" && after[i + 1] === "{") {
			return null;
		}
		// Reject string concatenation/interpolation continuation.
		if (after[i] === "\\" && i + 1 < after.length) {
			result += after[i + 1];
			i += 2;
			continue;
		}
		result += after[i];
		i++;
	}
	if (i >= after.length) return null;
	return result;
}
