/**
 * FileLocalStringResolver — unit tests.
 *
 * Tests the file-local constant string propagation module.
 * Exercises: string literals, template literals, binary concatenation,
 * binding references, env var stripping, chained bindings, and
 * non-resolvable expressions.
 *
 * Uses the standalone FileLocalStringResolver class (owns its own parser).
 */

import { beforeAll, describe, expect, it } from "vitest";
import {
	FileLocalStringResolver,
	type StringBindingTable,
} from "../../../../src/adapters/extractors/typescript/file-local-string-resolver.js";

let resolver: FileLocalStringResolver;

beforeAll(async () => {
	resolver = new FileLocalStringResolver();
	await resolver.initialize();
});

function resolve(source: string): StringBindingTable {
	return resolver.resolve(source, "src/test.ts");
}

// ── String literals ─────────────────────────────────────────────────

describe("FileLocalStringResolver — string literals", () => {
	it("resolves double-quoted string literal", () => {
		const bindings = resolve('const URL = "/api/v2/products";');
		expect(bindings.has("URL")).toBe(true);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("literal");
		expect(bindings.get("URL")!.confidence).toBe(1.0);
		expect(bindings.get("URL")!.unresolvedSegments).toBe(0);
	});

	it("resolves single-quoted string literal", () => {
		const bindings = resolve("const URL = '/api/v2/orders';");
		expect(bindings.get("URL")!.value).toBe("/api/v2/orders");
		expect(bindings.get("URL")!.basis).toBe("literal");
	});

	it("resolves empty string literal", () => {
		const bindings = resolve('const EMPTY = "";');
		expect(bindings.get("EMPTY")!.value).toBe("");
	});
});

// ── Template literals ───────────────────────────────────────────────

describe("FileLocalStringResolver — template literals", () => {
	it("resolves plain template literal (no substitutions)", () => {
		const bindings = resolve("const URL = `/api/v2/products`;");
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("literal");
	});

	it("resolves template with env var prefix (stripped)", () => {
		const bindings = resolve(
			"const URL = `${import.meta.env.VITE_API_URL}/api/v2/products`;",
		);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("env_prefixed");
		expect(bindings.get("URL")!.confidence).toBe(0.7);
	});

	it("resolves template with process.env prefix (stripped)", () => {
		const bindings = resolve(
			"const URL = `${process.env.API_URL}/api/v2/items`;",
		);
		expect(bindings.get("URL")!.value).toBe("/api/v2/items");
		expect(bindings.get("URL")!.basis).toBe("env_prefixed");
	});

	it("resolves template with binding reference", () => {
		const bindings = resolve(`
const PREFIX = "/api/v2";
const URL = \`\${PREFIX}/products\`;
`);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("template");
	});

	it("resolves template that is only an env var (no suffix)", () => {
		const bindings = resolve(
			"const BACKEND_URL = `${import.meta.env.VITE_API_URL}`;",
		);
		expect(bindings.get("BACKEND_URL")!.value).toBe("");
		expect(bindings.get("BACKEND_URL")!.basis).toBe("env_prefixed");
	});
});

// ── Binary concatenation ────────────────────────────────────────────

describe("FileLocalStringResolver — binary concatenation", () => {
	it("resolves string + string", () => {
		const bindings = resolve(
			'const URL = "/api/v2" + "/products";',
		);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("concat");
		expect(bindings.get("URL")!.confidence).toBe(1.0);
	});

	it("resolves binding + string", () => {
		const bindings = resolve(`
const BASE = "/api/v2";
const URL = BASE + "/products";
`);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("concat");
	});

	it("resolves env var + string (env stripped)", () => {
		const bindings = resolve(
			'const URL = import.meta.env.VITE_API_URL + "/api/v2/products";',
		);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("env_prefixed");
	});
});

// ── Chained bindings (glamCRM pattern) ──────────────────────────────

describe("FileLocalStringResolver — chained bindings", () => {
	it("resolves glamCRM salesTarget pattern: env → BACKEND_URL → BASE_URL", () => {
		const bindings = resolve(`
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/sales-targets\`;
`);
		expect(bindings.get("BASE_URL")!.value).toBe("/api/v2/sales-targets");
		expect(bindings.get("BASE_URL")!.basis).toBe("env_prefixed");
	});

	it("resolves glamCRM user pattern: BACKEND_URL → BASE_URL (two-level chain)", () => {
		const bindings = resolve(`
const BACKEND_URL = \`\${import.meta.env.VITE_API_URL}\`;
const BASE_URL = \`\${BACKEND_URL}/api/v2/users\`;
`);
		expect(bindings.get("BACKEND_URL")!.value).toBe("");
		expect(bindings.get("BASE_URL")!.value).toBe("/api/v2/users");
		expect(bindings.get("BASE_URL")!.basis).toBe("template");
	});

	it("resolves three-level chain", () => {
		const bindings = resolve(`
const A = "/api";
const B = \`\${A}/v2\`;
const C = \`\${B}/products\`;
`);
		expect(bindings.get("C")!.value).toBe("/api/v2/products");
	});
});

// ── Non-resolvable expressions ──────────────────────────────────────

describe("FileLocalStringResolver — non-resolvable", () => {
	it("does not resolve function calls", () => {
		const bindings = resolve("const URL = getBaseUrl();");
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve let bindings", () => {
		const bindings = resolve('let URL = "/api/v2/products";');
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve var bindings", () => {
		const bindings = resolve('var URL = "/api/v2/products";');
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve object property access", () => {
		const bindings = resolve("const URL = config.baseUrl;");
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve destructured bindings", () => {
		const bindings = resolve('const { URL } = { URL: "/api" };');
		// Destructuring — nameNode is not an identifier.
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve unresolved binding references", () => {
		// IMPORTED_BASE is not defined in this file.
		const bindings = resolve(
			"const URL = `${IMPORTED_BASE}/api/v2/products`;",
		);
		// Template has an unresolved segment — confidence 0, not stored.
		expect(bindings.has("URL")).toBe(false);
	});

	it("does not resolve numeric literals", () => {
		const bindings = resolve("const PORT = 3000;");
		expect(bindings.has("PORT")).toBe(false);
	});

	it("skips non-top-level const declarations", () => {
		const bindings = resolve(`
function foo() {
  const URL = "/api/v2/products";
}
`);
		// URL is inside a function — not top-level.
		expect(bindings.has("URL")).toBe(false);
	});
});

// ── Exported const bindings ─────────────────────────────────────────

describe("FileLocalStringResolver — export const", () => {
	it("resolves export const string literal", () => {
		const bindings = resolve('export const URL = "/api/v2/products";');
		expect(bindings.has("URL")).toBe(true);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
		expect(bindings.get("URL")!.basis).toBe("literal");
	});

	it("resolves export const template literal with env prefix", () => {
		const bindings = resolve(
			"export const BASE_URL = `${import.meta.env.VITE_API_URL}/api/v2/users`;",
		);
		expect(bindings.get("BASE_URL")!.value).toBe("/api/v2/users");
		expect(bindings.get("BASE_URL")!.basis).toBe("env_prefixed");
	});

	it("resolves chain: export const referencing earlier non-exported const", () => {
		const bindings = resolve(`
const BACKEND = \`\${import.meta.env.VITE_API_URL}\`;
export const BASE_URL = \`\${BACKEND}/api/v2/items\`;
`);
		expect(bindings.get("BASE_URL")!.value).toBe("/api/v2/items");
	});

	it("resolves chain: non-exported const referencing earlier exported const", () => {
		const bindings = resolve(`
export const PREFIX = "/api/v2";
const FULL_URL = \`\${PREFIX}/products\`;
`);
		expect(bindings.get("FULL_URL")!.value).toBe("/api/v2/products");
	});
});

// ── Binding table shape ─────────────────────────────────────────────

describe("FileLocalStringResolver — output shape", () => {
	it("returns a Map", () => {
		const bindings = resolve('const A = "x";');
		expect(bindings instanceof Map).toBe(true);
	});

	it("returns empty map for file with no const bindings", () => {
		const bindings = resolve("export function foo() {}");
		expect(bindings.size).toBe(0);
	});

	it("resolves multiple bindings in source order", () => {
		const bindings = resolve(`
const A = "/api";
const B = "/v2";
const C = "/products";
`);
		expect(bindings.size).toBe(3);
		expect([...bindings.keys()]).toEqual(["A", "B", "C"]);
	});
});

// ── JSX file support ────────────────────────────────────────────────

describe("FileLocalStringResolver — JSX support", () => {
	it("resolves constants in .jsx files", () => {
		const r = new FileLocalStringResolver();
		// Use the already-initialized resolver by testing via the class.
		// JSX files use tsx grammar.
		const bindings = resolver.resolve(
			'const URL = "/api/v2/products";',
			"src/api/client.jsx",
		);
		expect(bindings.get("URL")!.value).toBe("/api/v2/products");
	});
});
