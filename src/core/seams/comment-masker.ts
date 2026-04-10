/**
 * Positional comment masker for seam detectors.
 *
 * The seam detector pipeline runs regex against raw source text. That
 * matches patterns like `process.env.X` or `fs.writeFile("a")` even
 * when they appear inside comments or documentation, producing false
 * positives in the operational dependency contract.
 *
 * This masker is a small lexical pre-pass that replaces comment
 * characters with spaces while preserving:
 *
 *   1. Newlines exactly — every detector reports line numbers, and
 *      downstream pinning depends on positional stability. Deleting
 *      comment text would shift every subsequent line.
 *   2. String literal contents — `"http://x"` and `"process.env.X"`
 *      stay intact. The masker tracks string state so comment markers
 *      inside strings are not treated as comments.
 *   3. Total length — each input character maps to exactly one output
 *      character. Detectors can map masked-text positions back to
 *      source positions 1:1.
 *
 * This is NOT a formatter and NOT a parser. It is a state machine
 * over individual characters. Edge cases that require true parsing
 * (regex literals in JS, raw string literals in C++ with custom
 * delimiters, etc.) are deliberately not handled — see notes inline.
 *
 * Pure function. No I/O. No external dependencies.
 */

// ── C-style family (JS, TS, Rust, Java, C, C++) ────────────────────

/**
 * Mask comments in C-style source files.
 *
 * Handles:
 *  - line comments         (slash slash to end of line)
 *  - block comments        (slash star ... star slash, including JSDoc)
 *  - double-quoted strings "..." with backslash escapes
 *  - single-quoted strings '...' with backslash escapes
 *  - template literals     `...` with `${expr}` interpolation
 *
 * Limitations (intentional):
 *  - Regex literals (slash pattern slash) are not distinguished from
 *    division. A regex literal containing comment markers would be
 *    partially masked. Pragmatic: regex literals matching seam
 *    patterns are vanishingly rare and the worst case is a missed
 *    false-positive suppression.
 *  - Rust block comments are technically nestable, but the common
 *    case is non-nested. A nested-block-comment-stripping pass can
 *    be added later if real-world code surfaces a case.
 *  - C++ raw string literals R"delim(...)delim" are not handled.
 *    They are extremely rare in seam-detection contexts.
 *
 * @param source raw source text
 * @returns masked text of identical length and identical newline positions
 */
export function maskCommentsCStyle(source: string): string {
	const len = source.length;
	const out: string[] = new Array(len);

	// State machine. The template-state stack tracks nested template
	// literals: `outer ${inner ${nested}}`.
	type State =
		| "code"
		| "line_comment"
		| "block_comment"
		| "string_dq"
		| "string_sq"
		| "template";
	let state: State = "code";
	// Stack of template states for handling `${...}` interpolation:
	// when we see `${` inside a template, we push and switch to code;
	// when the matching `}` arrives, we pop back to template.
	// Each frame tracks the brace depth inside the interpolation so
	// inner `{}` braces don't pop too early.
	const templateStack: { braceDepth: number }[] = [];

	for (let i = 0; i < len; i++) {
		const ch = source[i];
		const next = i + 1 < len ? source[i + 1] : "";

		switch (state) {
			case "code": {
				// Detect interpolation close: `}` while inside an
				// interpolation frame. We are in "code" state but the
				// outermost frame is a template-interpolation.
				if (ch === "}" && templateStack.length > 0) {
					const top = templateStack[templateStack.length - 1];
					if (top.braceDepth === 0) {
						templateStack.pop();
						state = "template";
						out[i] = ch;
						break;
					}
					top.braceDepth--;
					out[i] = ch;
					break;
				}
				if (ch === "{" && templateStack.length > 0) {
					templateStack[templateStack.length - 1].braceDepth++;
					out[i] = ch;
					break;
				}

				if (ch === "/" && next === "/") {
					// Line comment start. Replace both slashes with spaces.
					out[i] = " ";
					out[i + 1] = " ";
					i++;
					state = "line_comment";
					break;
				}
				if (ch === "/" && next === "*") {
					// Block comment start. Replace `/*` with two spaces.
					out[i] = " ";
					out[i + 1] = " ";
					i++;
					state = "block_comment";
					break;
				}
				if (ch === '"') {
					state = "string_dq";
					out[i] = ch;
					break;
				}
				if (ch === "'") {
					state = "string_sq";
					out[i] = ch;
					break;
				}
				if (ch === "`") {
					state = "template";
					out[i] = ch;
					break;
				}
				out[i] = ch;
				break;
			}

			case "line_comment": {
				if (ch === "\n") {
					// Preserve the newline; line comment ends here.
					out[i] = ch;
					state = "code";
					break;
				}
				// Replace any other char with a space (keeps width).
				out[i] = " ";
				break;
			}

			case "block_comment": {
				if (ch === "*" && next === "/") {
					// Block comment end. Replace `*/` with two spaces.
					out[i] = " ";
					out[i + 1] = " ";
					i++;
					state = "code";
					break;
				}
				// Newlines inside block comments MUST be preserved
				// for line-number stability.
				if (ch === "\n") {
					out[i] = ch;
					break;
				}
				out[i] = " ";
				break;
			}

			case "string_dq": {
				if (ch === "\\" && i + 1 < len) {
					// Consume the escaped character verbatim.
					out[i] = ch;
					out[i + 1] = source[i + 1];
					i++;
					break;
				}
				if (ch === '"') {
					state = "code";
					out[i] = ch;
					break;
				}
				// JS string literals do not allow raw newlines, but
				// real-world code occasionally has them in malformed
				// inputs; treat newline as terminator to recover.
				if (ch === "\n") {
					out[i] = ch;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}

			case "string_sq": {
				if (ch === "\\" && i + 1 < len) {
					out[i] = ch;
					out[i + 1] = source[i + 1];
					i++;
					break;
				}
				if (ch === "'") {
					state = "code";
					out[i] = ch;
					break;
				}
				if (ch === "\n") {
					out[i] = ch;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}

			case "template": {
				if (ch === "\\" && i + 1 < len) {
					out[i] = ch;
					out[i + 1] = source[i + 1];
					i++;
					break;
				}
				if (ch === "`") {
					state = "code";
					out[i] = ch;
					break;
				}
				if (ch === "$" && next === "{") {
					// Interpolation start. Switch to code state and
					// push a frame so the matching `}` pops back here.
					out[i] = ch;
					out[i + 1] = next;
					i++;
					templateStack.push({ braceDepth: 0 });
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}
		}
	}

	return out.join("");
}

// ── Python ─────────────────────────────────────────────────────────

/**
 * Mask comments in Python source files.
 *
 * Handles:
 *  - line comments       `#` to end of line
 *  - single-quoted strings `'...'`
 *  - double-quoted strings `"..."`
 *  - triple-quoted strings `'''...'''` and `"""..."""` (NOT masked —
 *    these are real string literals, not comments, and may contain `#`)
 *
 * Limitations (intentional):
 *  - f-string `{expr}` interpolation is not specially handled. The
 *    interior of an f-string remains in string state, which is
 *    correct for the seam-detector use case (we don't want to detect
 *    `process.env.X` from inside an f-string body).
 *  - Raw `r"..."` and bytes `b"..."` prefixes work fine because the
 *    state machine only switches on the quote character.
 *
 * @param source raw source text
 * @returns masked text of identical length and identical newline positions
 */
export function maskCommentsPython(source: string): string {
	const len = source.length;
	const out: string[] = new Array(len);

	type State =
		| "code"
		| "line_comment"
		| "string_dq"
		| "string_sq"
		| "string_triple_dq"
		| "string_triple_sq";
	let state: State = "code";

	for (let i = 0; i < len; i++) {
		const ch = source[i];

		switch (state) {
			case "code": {
				if (ch === "#") {
					out[i] = " ";
					state = "line_comment";
					break;
				}
				// Triple-quoted string detection takes precedence.
				if (
					ch === '"' &&
					source[i + 1] === '"' &&
					source[i + 2] === '"'
				) {
					out[i] = ch;
					out[i + 1] = '"';
					out[i + 2] = '"';
					i += 2;
					state = "string_triple_dq";
					break;
				}
				if (
					ch === "'" &&
					source[i + 1] === "'" &&
					source[i + 2] === "'"
				) {
					out[i] = ch;
					out[i + 1] = "'";
					out[i + 2] = "'";
					i += 2;
					state = "string_triple_sq";
					break;
				}
				if (ch === '"') {
					state = "string_dq";
					out[i] = ch;
					break;
				}
				if (ch === "'") {
					state = "string_sq";
					out[i] = ch;
					break;
				}
				out[i] = ch;
				break;
			}

			case "line_comment": {
				if (ch === "\n") {
					out[i] = ch;
					state = "code";
					break;
				}
				out[i] = " ";
				break;
			}

			case "string_dq": {
				if (ch === "\\" && i + 1 < len) {
					out[i] = ch;
					out[i + 1] = source[i + 1];
					i++;
					break;
				}
				if (ch === '"') {
					state = "code";
					out[i] = ch;
					break;
				}
				if (ch === "\n") {
					out[i] = ch;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}

			case "string_sq": {
				if (ch === "\\" && i + 1 < len) {
					out[i] = ch;
					out[i + 1] = source[i + 1];
					i++;
					break;
				}
				if (ch === "'") {
					state = "code";
					out[i] = ch;
					break;
				}
				if (ch === "\n") {
					out[i] = ch;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}

			case "string_triple_dq": {
				if (
					ch === '"' &&
					source[i + 1] === '"' &&
					source[i + 2] === '"'
				) {
					out[i] = ch;
					out[i + 1] = '"';
					out[i + 2] = '"';
					i += 2;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}

			case "string_triple_sq": {
				if (
					ch === "'" &&
					source[i + 1] === "'" &&
					source[i + 2] === "'"
				) {
					out[i] = ch;
					out[i + 1] = "'";
					out[i + 2] = "'";
					i += 2;
					state = "code";
					break;
				}
				out[i] = ch;
				break;
			}
		}
	}

	return out.join("");
}

// ── Language router ────────────────────────────────────────────────

/**
 * Mask comments in source for the language inferred from filePath.
 * Falls back to C-style for unknown extensions.
 *
 * The seam detector pipeline reads files by extension already, so the
 * filePath argument is always available. This is the convenience
 * entry point detector layers should use.
 */
export function maskCommentsForFile(filePath: string, source: string): string {
	const lower = filePath.toLowerCase();
	if (lower.endsWith(".py")) return maskCommentsPython(source);
	// Default: C-style family covers JS, TS, Rust, Java, C, C++, and
	// any other extension currently in the seam detector matrix.
	return maskCommentsCStyle(source);
}
