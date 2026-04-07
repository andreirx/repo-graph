/**
 * HTTP client request fact extractor — unit tests (prototype).
 *
 * Tests the regex-based scanner against known axios/fetch patterns.
 * Does not require TypeScript compilation or node_modules.
 */

import { describe, expect, it } from "vitest";
import { extractHttpClientRequests } from "../../../../src/adapters/extractors/typescript/http-client-extractor.js";

/** Shorthand: extract from inline source with no enclosing symbols. */
function extract(source: string) {
	return extractHttpClientRequests(source, "src/api/client.ts", "test-repo", []);
}

/** Extract with enclosing symbols for caller attribution. */
function extractWithSymbols(
	source: string,
	symbols: Array<{ stableKey: string; name: string; lineStart: number | null }>,
) {
	return extractHttpClientRequests(source, "src/api/client.ts", "test-repo", symbols);
}

// ── Axios patterns ─────────────────────────────────────────────────

describe("extractHttpClientRequests — axios patterns", () => {
	it("extracts axios.get with string literal URL", () => {
		const facts = extract(`
const res = axios.get("/api/v2/products");
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[0].basis).toBe("literal");
		expect(facts[0].confidence).toBe(0.95);
	});

	it("extracts axios.post", () => {
		const facts = extract(`
axios.post("/api/v2/products", data);
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("extracts axios.put", () => {
		const facts = extract(`
axios.put("/api/v2/products/123", data);
`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("PUT");
	});

	it("extracts axios.delete", () => {
		const facts = extract(`
axios.delete("/api/v2/products/123");
`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("DELETE");
	});

	it("extracts axios.patch", () => {
		const facts = extract(`
axios.patch("/api/v2/products/123", data);
`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("PATCH");
	});

	it("extracts axios with template literal and base URL variable", () => {
		const facts = extract(
			"const res = axios.get(`${import.meta.env.VITE_API_URL}/api/v2/projects`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/projects");
		expect(facts[0].basis).toBe("template");
		expect(facts[0].confidence).toBe(0.8);
	});

	it("strips VITE_API_URL from template literal", () => {
		const facts = extract(
			"axios.get(`${import.meta.env.VITE_API_URL}/api/v2/orders`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/orders");
	});

	it("strips process.env.API_URL from template literal", () => {
		const facts = extract(
			"axios.get(`${process.env.API_URL}/api/v2/users`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/users");
	});

	it("strips named BASE_URL variable from template literal", () => {
		const facts = extract(
			"axios.get(`${API_BASE}/api/v2/items`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/items");
	});

	it("is case-insensitive on HTTP method name", () => {
		const facts = extract(`
axios.GET("/api/test");
`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("GET");
	});
});

// ── Template literal path param normalization ──────────────────────

describe("extractHttpClientRequests — path param normalization", () => {
	it("normalizes ${id} to {param}", () => {
		const facts = extract(
			"axios.get(`/api/v2/products/${productId}`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products/{param}");
	});

	it("normalizes multiple interpolations to {param}", () => {
		const facts = extract(
			"axios.get(`/api/v2/projects/${projectId}/tasks/${taskId}`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/projects/{param}/tasks/{param}");
	});

	it("strips base URL and normalizes path params in same URL", () => {
		const facts = extract(
			"axios.get(`${import.meta.env.VITE_API_URL}/api/v2/projects/${id}/members`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/projects/{param}/members");
	});
});

// ── Fetch patterns ─────────────────────────────────────────────────

describe("extractHttpClientRequests — fetch patterns", () => {
	it("extracts fetch with string literal (defaults to GET)", () => {
		const facts = extract(`
fetch("/api/v2/products");
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[0].metadata.httpMethod).toBe("GET");
	});

	it("extracts fetch with template literal and base URL", () => {
		const facts = extract(
			"fetch(`${import.meta.env.VITE_API_URL}/api/v2/products`);",
		);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
	});

	it("detects method in options object on same line", () => {
		const facts = extract(
			'fetch("/api/v2/products", { method: "POST", body: data });',
		);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("detects method in options object on next line", () => {
		const facts = extract(`fetch("/api/v2/products", {
  method: "DELETE",
  headers: {},
});`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("DELETE");
	});

	it("fetch confidence is lower than axios (less structured)", () => {
		const literalFetch = extract('fetch("/api/v2/x");');
		const literalAxios = extract('axios.get("/api/v2/x");');
		expect(literalFetch[0].confidence).toBeLessThan(literalAxios[0].confidence);
	});

	it("fetch template confidence is lower than fetch literal", () => {
		const templateFetch = extract(
			"fetch(`${import.meta.env.VITE_API_URL}/api/v2/x`);",
		);
		const literalFetch = extract('fetch("/api/v2/x");');
		expect(templateFetch[0].confidence).toBeLessThan(literalFetch[0].confidence);
	});
});

// ── Boundary fact shape ────────────────────────────────────────────

describe("extractHttpClientRequests — boundary fact shape", () => {
	it("emits mechanism=http", () => {
		const facts = extract('axios.get("/api/v2/x");');
		expect(facts[0].mechanism).toBe("http");
	});

	it("operation combines method + path", () => {
		const facts = extract('axios.post("/api/v2/orders");');
		expect(facts[0].operation).toBe("POST /api/v2/orders");
	});

	it("sourceFile matches the provided filePath", () => {
		const facts = extract('axios.get("/api/v2/x");');
		expect(facts[0].sourceFile).toBe("src/api/client.ts");
	});

	it("lineStart is 1-indexed", () => {
		const facts = extract('axios.get("/api/v2/x");');
		// Source has a leading newline from template literal... but our
		// extract helper passes raw string. Line 1 is "axios.get(...)".
		expect(facts[0].lineStart).toBeGreaterThanOrEqual(1);
	});

	it("schemaRef is null (prototype — no schema resolution)", () => {
		const facts = extract('axios.get("/api/v2/x");');
		expect(facts[0].schemaRef).toBeNull();
	});

	it("metadata.rawUrl is present and truncated to 100 chars", () => {
		const longPath = "/api/v2/" + "a".repeat(200);
		const facts = extract(`axios.get("${longPath}");`);
		expect(facts[0].metadata.rawUrl).toBeDefined();
		expect((facts[0].metadata.rawUrl as string).length).toBeLessThanOrEqual(100);
	});
});

// ── Caller attribution ─────────────────────────────────────────────

describe("extractHttpClientRequests — caller attribution", () => {
	it("attributes call to the enclosing symbol", () => {
		const source = `
function fetchProducts() {
  return axios.get("/api/v2/products");
}`;
		const symbols = [
			{ stableKey: "test-repo:src/api/client.ts#fetchProducts", name: "fetchProducts", lineStart: 2 },
		];
		const facts = extractWithSymbols(source, symbols);
		expect(facts.length).toBe(1);
		expect(facts[0].callerStableKey).toBe("test-repo:src/api/client.ts#fetchProducts");
	});

	it("attributes to nearest preceding symbol", () => {
		const source = `
function foo() {}

function bar() {
  return axios.get("/api/v2/items");
}`;
		const symbols = [
			{ stableKey: "test-repo:src/api/client.ts#foo", name: "foo", lineStart: 2 },
			{ stableKey: "test-repo:src/api/client.ts#bar", name: "bar", lineStart: 4 },
		];
		const facts = extractWithSymbols(source, symbols);
		expect(facts.length).toBe(1);
		expect(facts[0].callerStableKey).toBe("test-repo:src/api/client.ts#bar");
	});

	it("falls back to unknown:file:line when no enclosing symbol", () => {
		const facts = extract('axios.get("/api/v2/x");');
		expect(facts[0].callerStableKey).toMatch(/^unknown:src\/api\/client\.ts:\d+$/);
	});
});

// ── No false positives ─────────────────────────────────────────────

describe("extractHttpClientRequests — no false positives", () => {
	it("returns empty for plain function calls", () => {
		const facts = extract(`
const data = parseJSON(response);
const value = compute(42);
`);
		expect(facts).toEqual([]);
	});

	it("returns empty for non-HTTP code", () => {
		const facts = extract(`
export class UserService {
  getUser(id: string) {
    return this.db.query("SELECT * FROM users WHERE id = ?", [id]);
  }
}
`);
		expect(facts).toEqual([]);
	});

	it("ignores URLs that do not start with /", () => {
		// After stripping base URL, if the remainder doesn't start
		// with / it is not a recognizable API path.
		const facts = extract('axios.get("https://external-api.com/data");');
		expect(facts).toEqual([]);
	});

	it("ignores bare / path", () => {
		const facts = extract('axios.get("/");');
		expect(facts).toEqual([]);
	});

	it("does not match object property .get() that is not axios", () => {
		const facts = extract(`
const value = cache.get("key");
const item = map.get(id);
`);
		expect(facts).toEqual([]);
	});

	it("does not match fetch-like identifiers that are not the fetch API", () => {
		// "fetchData" is a custom function name, not the fetch() API.
		const facts = extract(`
const result = fetchData("/api/something");
`);
		expect(facts).toEqual([]);
	});
});

// ── Multiple calls in one file ─────────────────────────────────────

describe("extractHttpClientRequests — multiple calls", () => {
	it("extracts multiple axios calls from one file", () => {
		const facts = extract(`
axios.get("/api/v2/products");
axios.post("/api/v2/orders", orderData);
axios.delete("/api/v2/users/123");
`);
		expect(facts.length).toBe(3);
		expect(facts.map((f) => f.metadata.httpMethod)).toEqual(["GET", "POST", "DELETE"]);
	});

	it("extracts mixed axios and fetch calls", () => {
		const facts = extract(`
axios.get("/api/v2/products");
fetch("/api/v2/orders");
`);
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[1].address).toBe("/api/v2/orders");
	});
});
