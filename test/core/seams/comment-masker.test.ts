/**
 * Comment masker unit tests.
 *
 * Pure function — no I/O. Verifies positional fidelity, string
 * literal preservation, and language-aware masking semantics.
 *
 * Critical invariants:
 *   1. Output length === input length (1:1 character mapping)
 *   2. Newline positions are preserved exactly (line numbers stable)
 *   3. String literal contents are NOT masked
 *   4. Comment characters ARE replaced with spaces
 *   5. Code outside comments and strings is identical to input
 */

import { describe, expect, it } from "vitest";
import {
	maskCommentsCStyle,
	maskCommentsForFile,
	maskCommentsPython,
} from "../../../src/core/seams/comment-masker.js";

// ── Invariants helper ──────────────────────────────────────────────

function assertPositionalInvariants(input: string, output: string) {
	expect(output.length).toBe(input.length);
	// Newline positions must match exactly.
	for (let i = 0; i < input.length; i++) {
		if (input[i] === "\n") {
			expect(output[i]).toBe("\n");
		}
	}
}

// ── C-style: line comments ─────────────────────────────────────────

describe("maskCommentsCStyle — line comments", () => {
	it("masks a single-line comment", () => {
		const input = "// hello world\nconst x = 1;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe("              \nconst x = 1;");
	});

	it("preserves code on the same line BEFORE a line comment", () => {
		const input = "const x = 1; // comment\nconst y = 2;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe("const x = 1;           \nconst y = 2;");
	});

	it("preserves multiple newlines between comments", () => {
		const input = "// a\n\n// b\nconst x = 1;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		const lines = out.split("\n");
		expect(lines.length).toBe(4);
		expect(lines[3]).toBe("const x = 1;");
	});
});

// ── C-style: block comments ────────────────────────────────────────

describe("maskCommentsCStyle — block comments", () => {
	it("masks a single-line block comment", () => {
		const input = "/* hello */ const x = 1;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe("            const x = 1;");
	});

	it("masks a multi-line block comment and preserves newlines", () => {
		const input = "/* line1\nline2\nline3 */const x = 1;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		const lines = out.split("\n");
		expect(lines.length).toBe(3);
		expect(lines[0]).toBe("        ");
		expect(lines[1]).toBe("     ");
		expect(lines[2]).toBe("        const x = 1;");
	});

	it("masks JSDoc-style block comments", () => {
		const input = "/**\n * @param x foo\n */\nfunction f(x) {}";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		const lines = out.split("\n");
		expect(lines.length).toBe(4);
		expect(lines[3]).toBe("function f(x) {}");
		// Lines 0,1,2 are blank-equivalent.
		expect(lines[0].trim()).toBe("");
		expect(lines[1].trim()).toBe("");
		expect(lines[2].trim()).toBe("");
	});

	it("masks the env-detector documentation false-positive case", () => {
		// This is the exact pattern that produced the dogfood phantom
		// vars: env access patterns documented inside JSDoc.
		const input = `/**
 * - process.env.X || "fallback"
 * - process.env.Y ?? "default"
 */
const real = process.env.REAL_VAR;`;
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		// The JSDoc text must be masked.
		expect(out).not.toContain("process.env.X");
		expect(out).not.toContain("process.env.Y");
		// The real production access must survive.
		expect(out).toContain("process.env.REAL_VAR");
	});
});

// ── C-style: string literal preservation ──────────────────────────

describe("maskCommentsCStyle — string literals", () => {
	it("does not mask // inside a double-quoted string", () => {
		const input = 'const url = "http://example.com";';
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});

	it("does not mask /* inside a double-quoted string", () => {
		const input = 'const x = "/* not a comment */";';
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});

	it("does not mask // inside a single-quoted string", () => {
		const input = "const url = 'http://example.com';";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});

	it("does not mask // inside a template literal", () => {
		const input = "const url = `http://example.com`;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});

	it("handles escaped quotes inside strings", () => {
		const input = 'const x = "a \\"b\\" c"; // tail';
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out.startsWith('const x = "a \\"b\\" c";')).toBe(true);
		expect(out.includes("tail")).toBe(false);
	});

	it("handles template literal interpolation", () => {
		const input = "const x = `pre ${process.env.NAME} post`;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		// Inside ${...} we are in code state, so process.env.NAME stays.
		expect(out).toContain("process.env.NAME");
	});

	it("does not mask comment markers inside template literal text outside interpolation", () => {
		const input = "const x = `// not a comment`;";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});

	it("does mask a real comment AFTER a template literal closes", () => {
		const input = "const x = `value`; // real comment";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out.startsWith("const x = `value`;")).toBe(true);
		expect(out).not.toContain("real comment");
	});
});

// ── C-style: line number stability ─────────────────────────────────

describe("maskCommentsCStyle — line number stability", () => {
	it("preserves line numbers across mixed comment + code", () => {
		const input = `// line 1
/* line 2
   line 3
   line 4 */
const x = 1; // line 5
// line 6
const y = 2;`;
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		const lines = out.split("\n");
		expect(lines.length).toBe(7);
		expect(lines[4].trimEnd()).toBe("const x = 1;");
		expect(lines[6]).toBe("const y = 2;");
	});

	it("preserves line numbers when block comment spans many lines", () => {
		const input = `line0
/*
a
b
c
d
*/
line7`;
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		const lines = out.split("\n");
		expect(lines.length).toBe(8);
		expect(lines[0]).toBe("line0");
		expect(lines[7]).toBe("line7");
	});
});

// ── C-style: no-op cases ───────────────────────────────────────────

describe("maskCommentsCStyle — no-op cases", () => {
	it("returns identical text when no comments are present", () => {
		const input = "const x = 1;\nconst y = 2;\n";
		const out = maskCommentsCStyle(input);
		expect(out).toBe(input);
	});

	it("handles empty input", () => {
		expect(maskCommentsCStyle("")).toBe("");
	});

	it("handles input that is only a comment", () => {
		const input = "// hello";
		const out = maskCommentsCStyle(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe("        ");
	});
});

// ── Python: line comments ──────────────────────────────────────────

describe("maskCommentsPython — line comments", () => {
	it("masks # line comments", () => {
		const input = "x = 1 # hello\ny = 2";
		const out = maskCommentsPython(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe("x = 1        \ny = 2");
	});

	it("does not treat # inside string as a comment", () => {
		const input = 'url = "http://x.com#fragment"';
		const out = maskCommentsPython(input);
		assertPositionalInvariants(input, out);
		expect(out).toBe(input);
	});
});

// ── Python: triple-quoted strings ──────────────────────────────────

describe("maskCommentsPython — triple-quoted strings", () => {
	it("does NOT mask content inside triple-double-quoted strings", () => {
		const input = `doc = """
This is a docstring with # not a comment
and process.env.X documented here
"""
real_var = os.environ["REAL"]`;
		const out = maskCommentsPython(input);
		assertPositionalInvariants(input, out);
		// Docstring content is preserved.
		expect(out).toContain("# not a comment");
		expect(out).toContain("process.env.X");
		// Real env access is preserved.
		expect(out).toContain('os.environ["REAL"]');
	});

	it("does NOT mask content inside triple-single-quoted strings", () => {
		const input = "x = '''hash # inside ok'''\ny = 1";
		const out = maskCommentsPython(input);
		assertPositionalInvariants(input, out);
		expect(out).toContain("hash # inside ok");
	});

	it("masks # comments AFTER a triple-quoted string closes", () => {
		const input = `doc = """body"""\nx = 1 # tail`;
		const out = maskCommentsPython(input);
		assertPositionalInvariants(input, out);
		expect(out).toContain('"""body"""');
		expect(out).not.toContain("tail");
	});
});

// ── Language router ────────────────────────────────────────────────

describe("maskCommentsForFile — language routing", () => {
	it("routes .py to python masker", () => {
		const input = 'x = 1 # comment\nurl = "http://x"';
		const out = maskCommentsForFile("src/foo.py", input);
		expect(out).not.toContain("comment");
		expect(out).toContain("http://x");
	});

	it("routes .ts to c-style masker", () => {
		const input = 'const x = 1; // comment\nconst u = "http://x";';
		const out = maskCommentsForFile("src/foo.ts", input);
		expect(out).not.toContain("comment");
		expect(out).toContain("http://x");
	});

	it("routes unknown extensions to c-style masker by default", () => {
		const input = "// comment\ncode();";
		const out = maskCommentsForFile("src/foo.unknown", input);
		expect(out).not.toContain("comment");
		expect(out).toContain("code();");
	});
});
