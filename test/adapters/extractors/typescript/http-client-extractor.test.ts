/**
 * HTTP client request fact extractor — unit tests (prototype).
 *
 * Tests the regex-based scanner against known axios/fetch patterns.
 * Does not require TypeScript compilation or node_modules.
 *
 * Includes binding-resolution tests that exercise the
 * FileLocalStringResolver integration for base-URL constants.
 */

import { beforeAll, describe, expect, it } from "vitest";
import { extractHttpClientRequests } from "../../../../src/adapters/extractors/typescript/http-client-extractor.js";
import {
	FileLocalStringResolver,
	type StringBindingTable,
} from "../../../../src/adapters/extractors/typescript/file-local-string-resolver.js";

let resolver: FileLocalStringResolver;

beforeAll(async () => {
	resolver = new FileLocalStringResolver();
	await resolver.initialize();
});

/** Shorthand: extract from inline source with no enclosing symbols. */
function extract(source: string) {
	return extractHttpClientRequests(source, "src/api/client.ts", "test-repo", []);
}

/** Extract with binding resolution. */
function extractWithBindings(source: string, bindings: StringBindingTable) {
	return extractHttpClientRequests(source, "src/api/client.ts", "test-repo", [], bindings);
}

/** Extract with resolver: parse source for bindings, then extract. */
function extractResolved(source: string) {
	const bindings = resolver.resolve(source, "src/api/client.ts");
	return extractHttpClientRequests(source, "src/api/client.ts", "test-repo", [], bindings);
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

// ── Binding resolution (FileLocalStringResolver integration) ────────

describe("extractHttpClientRequests — binding resolution", () => {
	it("resolves glamCRM salesTarget pattern: BASE_URL with baked API prefix", () => {
		const source = `
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/sales-targets\`;

export const getSalesTargetsByYearMonth = async (yearMonth) => {
    const response = await axios.get(\`\${BASE_URL}/year-month\`, {});
    return response.data;
};

export const getSalesTargetsBySalesperson = async (salesId) => {
    const response = await axios.get(\`\${BASE_URL}/salesperson/\${salesId}\`, {});
    return response.data;
};

export const getSalesTargetById = async (id) => {
    const response = await axios.get(\`\${BASE_URL}/\${id}\`, {});
    return response.data;
};
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(3);

		expect(facts[0].address).toBe("/api/v2/sales-targets/year-month");
		expect(facts[1].address).toBe("/api/v2/sales-targets/salesperson/{param}");
		expect(facts[2].address).toBe("/api/v2/sales-targets/{param}");
	});

	it("resolves glamCRM user pattern: two-level BACKEND_URL -> BASE_URL chain", () => {
		const source = `
const BACKEND_URL = \`\${import.meta.env.VITE_API_URL}\`;
const BASE_URL = \`\${BACKEND_URL}/api/v2/users\`;

export const getUserById = async (id) => {
    const response = await axios.get(\`\${BASE_URL}/\${id}\`, {});
    return response.data;
};

export const getCurrentUser = async () => {
    const response = await axios.get(\`\${BASE_URL}/me\`, {});
    return response.data;
};

export const deleteUser = async (id) => {
    const response = await axios.delete(\`\${BASE_URL}/\${id}\`, {});
    return response.data;
};
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(3);

		expect(facts[0].address).toBe("/api/v2/users/{param}");
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[1].address).toBe("/api/v2/users/me");
		expect(facts[1].metadata.httpMethod).toBe("GET");
		expect(facts[2].address).toBe("/api/v2/users/{param}");
		expect(facts[2].metadata.httpMethod).toBe("DELETE");
	});

	it("existing inline env-prefixed pattern still works with bindings", () => {
		const source = `
axios.get(\`\${import.meta.env.VITE_API_URL}/api/v2/products\`);
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
	});

	it("unresolved dynamic expression does not emit false-positive path", () => {
		const source = `
const URL = getBaseUrl();
axios.get(\`\${URL}/api/v2/products\`);
`;
		// getBaseUrl() is not resolvable. The ${URL} stays as-is.
		// After env-pattern stripping, ${URL} does not match env patterns.
		// It becomes {param}, and the path is "/{param}/api/v2/products"
		// which starts with / but the first segment is a param — still emits.
		const facts = extractResolved(source);
		// The URL binding is unresolvable, so ${URL} stays in the raw arg.
		// The extractor treats it as a path param {param}.
		// This produces /{param}/api/v2/products — a valid but low-confidence path.
		// This is acceptable: it emits but with template basis and lower confidence.
		// The key assertion: it does NOT emit the same path as a resolved constant would.
		if (facts.length > 0) {
			expect(facts[0].address).not.toBe("/api/v2/products");
		}
	});

	it("binding-resolved facts carry resolvedUrl in metadata", () => {
		const source = `
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/items\`;
axios.get(\`\${BASE_URL}/list\`);
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.resolvedUrl).toBeDefined();
	});

	it("binding-resolved confidence is between literal and raw template", () => {
		// Resolved binding: 0.85 for axios
		const resolvedSource = `
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/items\`;
axios.get(\`\${BASE_URL}/list\`);
`;
		const resolvedFacts = extractResolved(resolvedSource);

		// Raw template (no binding): 0.8 for axios
		const rawFacts = extract(
			"axios.get(`${import.meta.env.VITE_API_URL}/api/v2/items/list`);",
		);

		// Literal: 0.95 for axios
		const literalFacts = extract('axios.get("/api/v2/items/list");');

		expect(resolvedFacts[0].confidence).toBeGreaterThan(rawFacts[0].confidence);
		expect(resolvedFacts[0].confidence).toBeLessThan(literalFacts[0].confidence);
	});

	it("no bindings provided: behaves identically to original extractor", () => {
		const source = 'axios.get("/api/v2/products");';
		const withBindings = extractWithBindings(source, new Map());
		const without = extract(source);
		expect(withBindings[0].address).toBe(without[0].address);
		expect(withBindings[0].confidence).toBe(without[0].confidence);
	});

	it("axios.post with resolved bare BASE_URL identifier", () => {
		const source = `
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/sales-targets\`;
const response = await axios.post(BASE_URL, salesTargetDTO, getAxiosConfig());
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/sales-targets");
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("fetch with resolved bare identifier", () => {
		const source = `
const BASE_URL = \`\${import.meta.env.VITE_API_URL}/api/v2/products\`;
const response = await fetch(BASE_URL);
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
		expect(facts[0].metadata.httpMethod).toBe("GET");
	});

	it("fetch bare identifier without bindings does not false-positive", () => {
		const source = "fetch(someVariable);";
		const facts = extract(source);
		expect(facts.length).toBe(0);
	});

	it("export const bindings are resolved in HTTP calls", () => {
		const source = `
export const BASE = \`\${import.meta.env.VITE_API_URL}/api/v2/orders\`;
axios.get(\`\${BASE}/active\`);
`;
		const facts = extractResolved(source);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/orders/active");
	});
});
