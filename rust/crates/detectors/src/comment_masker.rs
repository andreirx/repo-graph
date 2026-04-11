//! Positional comment masker for detector inputs.
//!
//! Rust mirror of `src/core/seams/comment-masker.ts`. Replaces
//! comment characters with spaces while preserving:
//!
//!   1. Byte length exactly — every detector reports line numbers,
//!      and the Rust regex engine reports BYTE offsets. Any change
//!      in byte length would shift downstream offset-sensitive
//!      operations on the masked content.
//!   2. Newline byte positions exactly — line numbers depend on
//!      newline positions; deleting comment text or shrinking it
//!      would shift every subsequent line.
//!   3. String literal contents — `"http://x"` and
//!      `"process.env.X"` stay intact byte-for-byte. The masker
//!      tracks string state so comment markers inside strings are
//!      not treated as comments.
//!   4. Valid UTF-8 output — the masker only emits ASCII spaces and
//!      newlines for masked positions, and copies original bytes
//!      verbatim for unmasked positions. The output is always
//!      valid UTF-8.
//!
//! Implementation note: byte-level walking, not char-level.
//!
//! - JS strings are indexed in UTF-16 code units. Non-BMP characters
//!   take TWO code units. The TS masker walks one code unit at a
//!   time. A naive Rust port walking `Vec<char>` (Unicode scalar
//!   values) would diverge from the TS implementation on any source
//!   containing non-BMP characters.
//!
//! - More importantly, the Rust `regex` engine reports BYTE offsets.
//!   If the masker shrinks the byte representation (replacing a
//!   multi-byte UTF-8 character in a comment with a single ASCII
//!   space would shrink 2-4 bytes to 1), every subsequent regex
//!   match offset would be wrong relative to the original source.
//!
//! Byte-level walking solves both problems:
//!
//! - Every state-transition trigger (`/`, `*`, `"`, `'`, `` ` ``,
//!   `$`, `{`, `}`, `\`, `\n`, `#`) is an ASCII byte with the top
//!   bit clear.
//! - UTF-8 continuation bytes always have the top bit set (the
//!   `10xxxxxx` pattern). They cannot be confused with any
//!   state-transition trigger.
//! - In comment regions, every non-newline byte is replaced with
//!   a single ASCII space byte. Byte length is preserved exactly.
//!   The result is valid UTF-8 because ASCII spaces are valid
//!   single-byte UTF-8 sequences.
//! - In string and code regions, bytes are copied through verbatim.
//!   Multi-byte UTF-8 sequences are preserved unchanged.
//!
//! This is NOT a formatter and NOT a parser. It is a state machine
//! over individual bytes. Edge cases that require true parsing
//! (regex literals in JS, raw string literals in C++ with custom
//! delimiters, etc.) are deliberately not handled.
//!
//! Pure function. No I/O. No external dependencies.

// ── C-style family (JS, TS, Rust, Java, C, C++) ────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CStyleState {
	Code,
	LineComment,
	BlockComment,
	StringDq,
	StringSq,
	Template,
}

/// Mask comments in C-style source files.
///
/// Handles:
///  - line comments (slash slash to end of line)
///  - block comments (slash star ... star slash, including JSDoc)
///  - double-quoted strings `"..."` with backslash escapes
///  - single-quoted strings `'...'` with backslash escapes
///  - template literals `` `...` `` with `${expr}` interpolation
///
/// Limitations (intentional):
///  - Regex literals (`/pattern/`) are not distinguished from
///    division. A regex literal containing comment markers would be
///    partially masked. Pragmatic: regex literals matching detector
///    seam patterns are vanishingly rare.
///  - Rust block comments are technically nestable, but the common
///    case is non-nested.
///  - C++ raw string literals R"delim(...)delim" are not handled.
pub fn mask_comments_c_style(source: &str) -> String {
	let bytes = source.as_bytes();
	let len = bytes.len();
	let mut out: Vec<u8> = vec![0u8; len];

	let mut state = CStyleState::Code;

	// Stack of template-interpolation frames. Each frame tracks the
	// brace depth inside a `${...}` so that inner `{}` braces don't
	// pop the frame too early.
	let mut template_stack: Vec<u32> = Vec::new();

	let mut i = 0;
	while i < len {
		let b = bytes[i];
		let next = if i + 1 < len { bytes[i + 1] } else { 0 };

		match state {
			CStyleState::Code => {
				// Detect interpolation close: `}` while inside a
				// template-interpolation frame.
				if b == b'}' && !template_stack.is_empty() {
					let top = template_stack.last_mut().unwrap();
					if *top == 0 {
						template_stack.pop();
						state = CStyleState::Template;
						out[i] = b;
						i += 1;
						continue;
					}
					*top -= 1;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'{' && !template_stack.is_empty() {
					*template_stack.last_mut().unwrap() += 1;
					out[i] = b;
					i += 1;
					continue;
				}

				if b == b'/' && next == b'/' {
					// Line comment start. Mask both slashes.
					out[i] = b' ';
					out[i + 1] = b' ';
					i += 2;
					state = CStyleState::LineComment;
					continue;
				}
				if b == b'/' && next == b'*' {
					// Block comment start. Mask both bytes.
					out[i] = b' ';
					out[i + 1] = b' ';
					i += 2;
					state = CStyleState::BlockComment;
					continue;
				}
				if b == b'"' {
					state = CStyleState::StringDq;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'\'' {
					state = CStyleState::StringSq;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'`' {
					state = CStyleState::Template;
					out[i] = b;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			CStyleState::LineComment => {
				if b == b'\n' {
					// Preserve the newline; line comment ends here.
					out[i] = b;
					state = CStyleState::Code;
					i += 1;
					continue;
				}
				// Replace any other byte with an ASCII space.
				// This works for multi-byte UTF-8 sequences too:
				// each byte (including continuation bytes) becomes
				// a space. Byte length is preserved and the output
				// remains valid UTF-8 because ASCII spaces are
				// single-byte sequences.
				out[i] = b' ';
				i += 1;
			}

			CStyleState::BlockComment => {
				if b == b'*' && next == b'/' {
					// Block comment end. Mask both bytes.
					out[i] = b' ';
					out[i + 1] = b' ';
					i += 2;
					state = CStyleState::Code;
					continue;
				}
				// Newlines inside block comments MUST be preserved.
				if b == b'\n' {
					out[i] = b;
					i += 1;
					continue;
				}
				out[i] = b' ';
				i += 1;
			}

			CStyleState::StringDq => {
				if b == b'\\' && i + 1 < len {
					// Consume the escaped byte verbatim. For
					// multi-byte UTF-8 chars after a backslash,
					// only the first byte is consumed by the
					// escape; the remaining continuation bytes
					// are copied as ordinary string content,
					// which is correct because continuation bytes
					// are not state-transition triggers.
					out[i] = b;
					out[i + 1] = bytes[i + 1];
					i += 2;
					continue;
				}
				if b == b'"' {
					state = CStyleState::Code;
					out[i] = b;
					i += 1;
					continue;
				}
				// JS string literals do not allow raw newlines, but
				// real-world code occasionally has them in malformed
				// inputs; treat newline as terminator to recover.
				if b == b'\n' {
					out[i] = b;
					state = CStyleState::Code;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			CStyleState::StringSq => {
				if b == b'\\' && i + 1 < len {
					out[i] = b;
					out[i + 1] = bytes[i + 1];
					i += 2;
					continue;
				}
				if b == b'\'' {
					state = CStyleState::Code;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'\n' {
					out[i] = b;
					state = CStyleState::Code;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			CStyleState::Template => {
				if b == b'\\' && i + 1 < len {
					out[i] = b;
					out[i + 1] = bytes[i + 1];
					i += 2;
					continue;
				}
				if b == b'`' {
					state = CStyleState::Code;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'$' && next == b'{' {
					// Interpolation start. Switch to code state and
					// push a frame.
					out[i] = b;
					out[i + 1] = next;
					i += 2;
					template_stack.push(0);
					state = CStyleState::Code;
					continue;
				}
				out[i] = b;
				i += 1;
			}
		}
	}

	// Safety: every byte we emit is either a verbatim copy from a
	// valid UTF-8 input or an ASCII space (b' ', 0x20) or an ASCII
	// newline (b'\n', 0x0A). Both ASCII bytes are valid single-byte
	// UTF-8 sequences. Verbatim copies preserve UTF-8 validity.
	// Therefore the output is always valid UTF-8.
	String::from_utf8(out).expect(
		"comment masker output should be valid UTF-8 (every emitted byte is either an ASCII space/newline or a verbatim copy from valid UTF-8 input)",
	)
}

// ── Python ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonState {
	Code,
	LineComment,
	StringDq,
	StringSq,
	StringTripleDq,
	StringTripleSq,
}

/// Mask comments in Python source files.
///
/// Handles:
///  - line comments `#` to end of line
///  - single-quoted strings `'...'`
///  - double-quoted strings `"..."`
///  - triple-quoted strings `'''...'''` and `"""..."""` (NOT masked
///    — these are real string literals, not comments, and may
///    contain `#`)
///
/// Limitations (intentional):
///  - f-string `{expr}` interpolation is not specially handled. The
///    interior of an f-string remains in string state, which is
///    correct for the seam-detector use case.
///  - Raw `r"..."` and bytes `b"..."` prefixes work fine because
///    the state machine only switches on the quote byte.
pub fn mask_comments_python(source: &str) -> String {
	let bytes = source.as_bytes();
	let len = bytes.len();
	let mut out: Vec<u8> = vec![0u8; len];

	let mut state = PythonState::Code;

	let mut i = 0;
	while i < len {
		let b = bytes[i];

		match state {
			PythonState::Code => {
				if b == b'#' {
					out[i] = b' ';
					state = PythonState::LineComment;
					i += 1;
					continue;
				}
				// Triple-quoted string detection takes precedence.
				if b == b'"'
					&& i + 2 < len
					&& bytes[i + 1] == b'"'
					&& bytes[i + 2] == b'"'
				{
					out[i] = b;
					out[i + 1] = b'"';
					out[i + 2] = b'"';
					i += 3;
					state = PythonState::StringTripleDq;
					continue;
				}
				if b == b'\''
					&& i + 2 < len
					&& bytes[i + 1] == b'\''
					&& bytes[i + 2] == b'\''
				{
					out[i] = b;
					out[i + 1] = b'\'';
					out[i + 2] = b'\'';
					i += 3;
					state = PythonState::StringTripleSq;
					continue;
				}
				if b == b'"' {
					state = PythonState::StringDq;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'\'' {
					state = PythonState::StringSq;
					out[i] = b;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			PythonState::LineComment => {
				if b == b'\n' {
					out[i] = b;
					state = PythonState::Code;
					i += 1;
					continue;
				}
				out[i] = b' ';
				i += 1;
			}

			PythonState::StringDq => {
				if b == b'\\' && i + 1 < len {
					out[i] = b;
					out[i + 1] = bytes[i + 1];
					i += 2;
					continue;
				}
				if b == b'"' {
					state = PythonState::Code;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'\n' {
					out[i] = b;
					state = PythonState::Code;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			PythonState::StringSq => {
				if b == b'\\' && i + 1 < len {
					out[i] = b;
					out[i + 1] = bytes[i + 1];
					i += 2;
					continue;
				}
				if b == b'\'' {
					state = PythonState::Code;
					out[i] = b;
					i += 1;
					continue;
				}
				if b == b'\n' {
					out[i] = b;
					state = PythonState::Code;
					i += 1;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			PythonState::StringTripleDq => {
				if b == b'"'
					&& i + 2 < len
					&& bytes[i + 1] == b'"'
					&& bytes[i + 2] == b'"'
				{
					out[i] = b;
					out[i + 1] = b'"';
					out[i + 2] = b'"';
					i += 3;
					state = PythonState::Code;
					continue;
				}
				out[i] = b;
				i += 1;
			}

			PythonState::StringTripleSq => {
				if b == b'\''
					&& i + 2 < len
					&& bytes[i + 1] == b'\''
					&& bytes[i + 2] == b'\''
				{
					out[i] = b;
					out[i + 1] = b'\'';
					out[i + 2] = b'\'';
					i += 3;
					state = PythonState::Code;
					continue;
				}
				out[i] = b;
				i += 1;
			}
		}
	}

	String::from_utf8(out).expect(
		"comment masker output should be valid UTF-8 (every emitted byte is either an ASCII space/newline or a verbatim copy from valid UTF-8 input)",
	)
}

// ── Language router ────────────────────────────────────────────────

/// Mask comments in source for the language inferred from
/// `file_path`. Falls back to C-style for unknown extensions.
pub fn mask_comments_for_file(file_path: &str, source: &str) -> String {
	let lower = file_path.to_lowercase();
	if lower.ends_with(".py") {
		mask_comments_python(source)
	} else {
		mask_comments_c_style(source)
	}
}
