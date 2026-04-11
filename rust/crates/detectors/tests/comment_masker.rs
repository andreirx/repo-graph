//! Comment masker tests — Rust mirror of TS `comment-masker.test.ts`.
//!
//! Each test proves a positional / lexical invariant the production
//! detector pipeline depends on. Tests are organized by behavior
//! category, not by verbatim translation of TS prose. The categories
//! correspond to the substep R1-D constraint dimensions:
//!
//!   1. Line comments
//!   2. Block comments (incl. JSDoc)
//!   3. String literal preservation (DQ, SQ, template, escapes)
//!   4. Template literal interpolation
//!   5. Line number stability
//!   6. No-op cases
//!   7. Python line comments
//!   8. Python triple-quoted strings
//!   9. Language router
//!  10. Non-ASCII stability (byte-level invariants under multi-byte
//!      UTF-8 sequences)
//!
//! Critical invariants enforced by `assert_positional_invariants`:
//!  - Output BYTE length == input BYTE length (the regex engine
//!    reports byte offsets; any byte-length change would shift
//!    downstream offset-sensitive operations)
//!  - Newline BYTE positions are preserved exactly
//!  - Output is valid UTF-8

use repo_graph_detectors::comment_masker::{
	mask_comments_c_style, mask_comments_for_file, mask_comments_python,
};

// ── Invariants helper ──────────────────────────────────────────────

/// Assert the masker's byte-level positional invariants:
///   - Same byte length input → output
///   - Same newline byte positions
///   - Output is valid UTF-8 (implied by the function returning
///     `&str`, but asserted explicitly via round-trip)
fn assert_positional_invariants(input: &str, output: &str) {
	let in_bytes = input.as_bytes();
	let out_bytes = output.as_bytes();
	assert_eq!(
		in_bytes.len(),
		out_bytes.len(),
		"byte length mismatch: input has {} bytes, output has {}",
		in_bytes.len(),
		out_bytes.len(),
	);
	for (i, (ib, ob)) in in_bytes.iter().zip(out_bytes.iter()).enumerate() {
		if *ib == b'\n' {
			assert_eq!(
				*ob, b'\n',
				"newline at byte {} in input was not preserved (output has {:?})",
				i, *ob as char
			);
		}
	}
	// Round-trip valid-UTF-8 check: if the &str can be re-parsed
	// from its bytes via from_utf8, it's valid. This is implied by
	// `output: &str` but we assert it explicitly to make the
	// invariant visible at the test boundary.
	assert!(
		std::str::from_utf8(out_bytes).is_ok(),
		"output is not valid UTF-8"
	);
}

// ──────────────────────────────────────────────────────────────────
// 1. Line comments
// ──────────────────────────────────────────────────────────────────

#[test]
fn line_comment_is_blanked_with_newline_preserved() {
	let input = "// hello world\nconst x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, "              \nconst x = 1;");
}

#[test]
fn code_before_line_comment_is_preserved() {
	let input = "const x = 1; // comment\nconst y = 2;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, "const x = 1;           \nconst y = 2;");
}

#[test]
fn multiple_blank_lines_between_comments_preserved() {
	let input = "// a\n\n// b\nconst x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	let lines: Vec<&str> = out.split('\n').collect();
	assert_eq!(lines.len(), 4);
	assert_eq!(lines[3], "const x = 1;");
}

// ──────────────────────────────────────────────────────────────────
// 2. Block comments (incl. JSDoc)
// ──────────────────────────────────────────────────────────────────

#[test]
fn single_line_block_comment_is_blanked() {
	let input = "/* hello */ const x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, "            const x = 1;");
}

#[test]
fn multi_line_block_comment_preserves_internal_newlines() {
	let input = "/* line1\nline2\nline3 */const x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	let lines: Vec<&str> = out.split('\n').collect();
	assert_eq!(lines.len(), 3);
	assert_eq!(lines[0], "        ");
	assert_eq!(lines[1], "     ");
	assert_eq!(lines[2], "        const x = 1;");
}

#[test]
fn jsdoc_block_comment_is_blanked() {
	let input = "/**\n * @param x foo\n */\nfunction f(x) {}";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	let lines: Vec<&str> = out.split('\n').collect();
	assert_eq!(lines.len(), 4);
	assert_eq!(lines[3], "function f(x) {}");
	assert_eq!(lines[0].trim(), "");
	assert_eq!(lines[1].trim(), "");
	assert_eq!(lines[2].trim(), "");
}

#[test]
fn env_detector_doc_false_positive_case_is_masked() {
	// This is the exact pattern that produced the dogfood phantom
	// vars in the TS slice: env access patterns documented inside
	// JSDoc.
	let input = "/**\n * - process.env.X || \"fallback\"\n * - process.env.Y ?? \"default\"\n */\nconst real = process.env.REAL_VAR;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	// JSDoc text must be masked.
	assert!(!out.contains("process.env.X"));
	assert!(!out.contains("process.env.Y"));
	// Real production access must survive.
	assert!(out.contains("process.env.REAL_VAR"));
}

// ──────────────────────────────────────────────────────────────────
// 3. String literal preservation
// ──────────────────────────────────────────────────────────────────

#[test]
fn double_quoted_string_with_slash_slash_is_not_masked() {
	let input = "const url = \"http://example.com\";";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn double_quoted_string_with_slash_star_is_not_masked() {
	let input = "const x = \"/* not a comment */\";";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn single_quoted_string_with_slash_slash_is_not_masked() {
	let input = "const url = 'http://example.com';";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn template_literal_with_slash_slash_is_not_masked() {
	let input = "const url = `http://example.com`;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn escaped_quotes_inside_strings_are_handled() {
	let input = "const x = \"a \\\"b\\\" c\"; // tail";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert!(out.starts_with("const x = \"a \\\"b\\\" c\";"));
	assert!(!out.contains("tail"));
}

// ──────────────────────────────────────────────────────────────────
// 4. Template literal interpolation
// ──────────────────────────────────────────────────────────────────

#[test]
fn template_interpolation_returns_to_code_state() {
	// Inside `${...}` we are in code state, so process.env.NAME
	// stays exactly as-is.
	let input = "const x = `pre ${process.env.NAME} post`;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert!(out.contains("process.env.NAME"));
}

#[test]
fn template_text_outside_interpolation_with_comment_marker_is_not_masked() {
	let input = "const x = `// not a comment`;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn real_comment_after_template_close_is_masked() {
	let input = "const x = `value`; // real comment";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert!(out.starts_with("const x = `value`;"));
	assert!(!out.contains("real comment"));
}

// ──────────────────────────────────────────────────────────────────
// 5. Line number stability
// ──────────────────────────────────────────────────────────────────

#[test]
fn line_numbers_preserved_across_mixed_comment_and_code() {
	let input = "// line 1\n/* line 2\n   line 3\n   line 4 */\nconst x = 1; // line 5\n// line 6\nconst y = 2;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	let lines: Vec<&str> = out.split('\n').collect();
	assert_eq!(lines.len(), 7);
	assert_eq!(lines[4].trim_end(), "const x = 1;");
	assert_eq!(lines[6], "const y = 2;");
}

#[test]
fn block_comment_spanning_many_lines_preserves_line_count() {
	let input = "line0\n/*\na\nb\nc\nd\n*/\nline7";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	let lines: Vec<&str> = out.split('\n').collect();
	assert_eq!(lines.len(), 8);
	assert_eq!(lines[0], "line0");
	assert_eq!(lines[7], "line7");
}

// ──────────────────────────────────────────────────────────────────
// 6. No-op cases
// ──────────────────────────────────────────────────────────────────

#[test]
fn no_comments_returns_identical_text() {
	let input = "const x = 1;\nconst y = 2;\n";
	let out = mask_comments_c_style(input);
	assert_eq!(out, input);
}

#[test]
fn empty_input_returns_empty() {
	assert_eq!(mask_comments_c_style(""), "");
}

#[test]
fn input_that_is_only_a_comment_becomes_all_spaces() {
	let input = "// hello";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, "        ");
}

// ──────────────────────────────────────────────────────────────────
// 7. Python line comments
// ──────────────────────────────────────────────────────────────────

#[test]
fn python_hash_line_comments_are_blanked() {
	let input = "x = 1 # hello\ny = 2";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, "x = 1        \ny = 2");
}

#[test]
fn python_hash_inside_string_is_not_a_comment() {
	let input = "url = \"http://x.com#fragment\"";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

// ──────────────────────────────────────────────────────────────────
// 8. Python triple-quoted strings
// ──────────────────────────────────────────────────────────────────

#[test]
fn python_triple_double_quoted_string_content_is_not_masked() {
	let input = "doc = \"\"\"\nThis is a docstring with # not a comment\nand process.env.X documented here\n\"\"\"\nreal_var = os.environ[\"REAL\"]";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	// Docstring content is preserved.
	assert!(out.contains("# not a comment"));
	assert!(out.contains("process.env.X"));
	// Real env access is preserved.
	assert!(out.contains("os.environ[\"REAL\"]"));
}

#[test]
fn python_triple_single_quoted_string_content_is_not_masked() {
	let input = "x = '''hash # inside ok'''\ny = 1";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert!(out.contains("hash # inside ok"));
}

#[test]
fn python_hash_after_triple_quoted_string_close_is_masked() {
	let input = "doc = \"\"\"body\"\"\"\nx = 1 # tail";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert!(out.contains("\"\"\"body\"\"\""));
	assert!(!out.contains("tail"));
}

// ──────────────────────────────────────────────────────────────────
// 9. Language router
// ──────────────────────────────────────────────────────────────────

#[test]
fn router_dispatches_py_extension_to_python_masker() {
	let input = "x = 1 # comment\nurl = \"http://x\"";
	let out = mask_comments_for_file("src/foo.py", input);
	assert!(!out.contains("comment"));
	assert!(out.contains("http://x"));
}

#[test]
fn router_dispatches_ts_extension_to_c_style_masker() {
	let input = "const x = 1; // comment\nconst u = \"http://x\";";
	let out = mask_comments_for_file("src/foo.ts", input);
	assert!(!out.contains("comment"));
	assert!(out.contains("http://x"));
}

#[test]
fn router_unknown_extension_falls_back_to_c_style() {
	let input = "// comment\ncode();";
	let out = mask_comments_for_file("src/foo.unknown", input);
	assert!(!out.contains("comment"));
	assert!(out.contains("code();"));
}

// ──────────────────────────────────────────────────────────────────
// 10. Non-ASCII stability (byte-level invariants)
// ──────────────────────────────────────────────────────────────────
//
// These tests verify the masker's byte-level correctness on
// inputs that contain multi-byte UTF-8 characters. The invariants:
//
//  - Multi-byte UTF-8 in COMMENTS: every byte (including
//    continuation bytes) is replaced with an ASCII space. Byte
//    length is preserved. Output is valid UTF-8 (because ASCII
//    spaces are valid single-byte UTF-8 sequences).
//
//  - Multi-byte UTF-8 in STRING LITERALS: bytes are preserved
//    exactly. The character is intact in the output.
//
//  - State-transition triggers (slash, star, quote, backtick,
//    dollar, brace, backslash, newline, hash) are all ASCII bytes
//    (top bit zero) and cannot be confused with UTF-8 continuation
//    bytes (which always have the top bit set to `10xxxxxx`). This
//    is a UTF-8 design property, not a coincidence — it makes
//    byte-level state machines safe on UTF-8 input.

#[test]
fn non_ascii_inside_line_comment_is_masked_byte_for_byte() {
	// "é" is U+00E9, encoded as 0xC3 0xA9 in UTF-8 (2 bytes).
	let input = "// café\nconst x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	// The "é" (2 bytes) becomes 2 spaces; "caf" (3 bytes) becomes
	// 3 spaces; "//" becomes 2 spaces; total 7 spaces before \n.
	// "// café" is 8 bytes total (// + space + caf + é = 2+1+3+2).
	let expected = "        \nconst x = 1;";
	assert_eq!(out, expected);
	assert_eq!(out.len(), input.len());
}

#[test]
fn non_ascii_inside_block_comment_is_masked_byte_for_byte() {
	// "🌍" is U+1F30D, encoded as 0xF0 0x9F 0x8C 0x8D in UTF-8
	// (4 bytes). It's a non-BMP character, which is exactly the
	// case where a Vec<char> walker would diverge from the TS
	// UTF-16 walker.
	let input = "/* hello 🌍 world */const x = 1;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out.len(), input.len());
	// The whole comment should be spaces, then `const x = 1;`.
	let expected_prefix = " ".repeat(input.len() - "const x = 1;".len());
	assert!(out.starts_with(&expected_prefix));
	assert!(out.ends_with("const x = 1;"));
}

#[test]
fn non_ascii_inside_string_literal_is_preserved_exactly() {
	let input = "const greeting = \"héllo wörld\";";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	// The string content (including non-ASCII) is unchanged.
	assert_eq!(out, input);
}

#[test]
fn non_ascii_inside_template_literal_is_preserved_exactly() {
	let input = "const greeting = `héllo wörld`;";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out, input);
}

#[test]
fn non_ascii_in_string_with_comment_after_preserves_string_and_masks_comment() {
	let input = "const x = \"日本語\"; // comment with 中文";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	// String content with non-ASCII preserved.
	assert!(out.contains("日本語"));
	// Comment content masked: no Chinese, no "comment" English.
	assert!(!out.contains("中文"));
	assert!(!out.contains("comment"));
}

#[test]
fn non_bmp_emoji_in_comment_does_not_corrupt_subsequent_code() {
	// 4-byte UTF-8 sequence (non-BMP) inside a comment, followed
	// by real code. The byte-level walker must mask all 4 bytes
	// of the emoji as spaces and then continue walking from the
	// next code position correctly.
	let input = "// 🚀 launch\nfs.writeFile(\"out.txt\", data);";
	let out = mask_comments_c_style(input);
	assert_positional_invariants(input, &out);
	// fs.writeFile call must remain intact and detectable.
	assert!(out.contains("fs.writeFile(\"out.txt\""));
}

#[test]
fn python_non_ascii_inside_hash_comment_is_masked() {
	// 2-byte UTF-8 in a Python `#` comment.
	let input = "x = 1 # café\ny = 2";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert_eq!(out.len(), input.len());
	let expected = "x = 1        \ny = 2";
	assert_eq!(out, expected);
}

#[test]
fn python_non_ascii_inside_triple_quoted_string_is_preserved() {
	let input = "doc = \"\"\"日本語のドキュメント\"\"\"\nx = 1";
	let out = mask_comments_python(input);
	assert_positional_invariants(input, &out);
	assert!(out.contains("日本語のドキュメント"));
	assert_eq!(out, input);
}

#[test]
fn output_is_valid_utf8_after_masking_multi_byte_comment() {
	// Direct round-trip check: the output bytes can be parsed as
	// UTF-8 without error. This is implied by the function
	// returning &str, but the test asserts it explicitly to pin
	// the invariant at the public API boundary.
	let input = "/* αβγ δεζ */ code();";
	let out = mask_comments_c_style(input);
	let bytes = out.as_bytes();
	assert!(std::str::from_utf8(bytes).is_ok());
	assert_eq!(bytes.len(), input.len());
}
