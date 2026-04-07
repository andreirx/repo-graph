/**
 * Express route fact extractor — unit tests (prototype).
 *
 * Tests the regex-based scanner against known Express patterns.
 * Parallel to Spring route extractor tests: same fact shape, different framework.
 */

import { beforeAll, describe, expect, it } from "vitest";
import { extractExpressRoutes } from "../../../../src/adapters/extractors/typescript/express-route-extractor.js";
import {
	FileLocalStringResolver,
	type StringBindingTable,
} from "../../../../src/adapters/extractors/typescript/file-local-string-resolver.js";

let resolver: FileLocalStringResolver;

beforeAll(async () => {
	resolver = new FileLocalStringResolver();
	await resolver.initialize();
});

/** Extract with express import present. */
function extract(source: string) {
	const withImport = `import express from 'express';\n${source}`;
	return extractExpressRoutes(withImport, "src/server.ts", "test-repo", []);
}

/** Extract with resolver bindings. */
function extractResolved(source: string) {
	const withImport = `import express from 'express';\n${source}`;
	const bindings = resolver.resolve(withImport, "src/server.ts");
	return extractExpressRoutes(withImport, "src/server.ts", "test-repo", [], bindings);
}

// ── HTTP methods ────────────────────────────────────────────────────

describe("extractExpressRoutes — HTTP methods", () => {
	it("extracts app.get", () => {
		const facts = extract('app.get("/api/v2/products", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[0].address).toBe("/api/v2/products");
	});

	it("extracts app.post", () => {
		const facts = extract('app.post("/api/v2/products", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("POST");
	});

	it("extracts app.put", () => {
		const facts = extract('app.put("/api/v2/products/:id", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("PUT");
	});

	it("extracts app.delete", () => {
		const facts = extract('app.delete("/api/v2/products/:id", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("DELETE");
	});

	it("extracts app.patch", () => {
		const facts = extract('app.patch("/api/v2/products/:id", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("PATCH");
	});

	it("extracts router.get", () => {
		const facts = extract('router.get("/api/v2/items", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[0].metadata.receiver).toBe("router");
	});

	it("extracts server.post", () => {
		const facts = extract('server.post("/api/v2/data", handler);');
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.receiver).toBe("server");
	});
});

// ── Path parameter normalization ────────────────────────────────────

describe("extractExpressRoutes — path params", () => {
	it("normalizes :id to {id}", () => {
		const facts = extract('app.get("/api/v2/products/:id", handler);');
		expect(facts[0].address).toBe("/api/v2/products/{id}");
	});

	it("normalizes multiple :params", () => {
		const facts = extract('app.get("/api/v2/:treeId/nodes/:nodeId", handler);');
		expect(facts[0].address).toBe("/api/v2/{treeId}/nodes/{nodeId}");
	});

	it("preserves literal segments exactly", () => {
		const facts = extract('app.get("/api/v2/products", handler);');
		expect(facts[0].address).toBe("/api/v2/products");
	});
});

// ── Boundary fact shape ─────────────────────────────────────────────

describe("extractExpressRoutes — fact shape", () => {
	it("emits mechanism=http", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].mechanism).toBe("http");
	});

	it("emits framework=express", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].framework).toBe("express");
	});

	it("emits basis=registration", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].basis).toBe("registration");
	});

	it("operation combines method + path", () => {
		const facts = extract('app.post("/api/v2/orders", handler);');
		expect(facts[0].operation).toBe("POST /api/v2/orders");
	});

	it("sourceFile matches the provided filePath", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].sourceFile).toBe("src/server.ts");
	});

	it("lineStart is 1-indexed", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].lineStart).toBeGreaterThanOrEqual(1);
	});

	it("schemaRef is null", () => {
		const facts = extract('app.get("/x", handler);');
		expect(facts[0].schemaRef).toBeNull();
	});

	it("metadata.rawPath is present", () => {
		const facts = extract('app.get("/api/v2/products", handler);');
		expect(facts[0].metadata.rawPath).toBeDefined();
	});
});

// ── Receiver provenance ─────────────────────────────────────────────

describe("extractExpressRoutes — receiver provenance", () => {
	it("does NOT match cache.get", () => {
		const facts = extract('cache.get("/some/key", callback);');
		expect(facts).toEqual([]);
	});

	it("does NOT match map.get", () => {
		const facts = extract('map.get("/some/key");');
		expect(facts).toEqual([]);
	});

	it("does NOT match db.get", () => {
		const facts = extract('db.get("/records", handler);');
		expect(facts).toEqual([]);
	});
});

// ── Express import gate ─────────────────────────────────────────────

describe("extractExpressRoutes — import gate", () => {
	it("returns empty for file without express import", () => {
		// Call directly without the express import wrapper.
		const facts = extractExpressRoutes(
			'app.get("/api/v2/products", handler);',
			"src/server.ts",
			"test-repo",
			[],
		);
		expect(facts).toEqual([]);
	});

	it("detects require-style express import", () => {
		const source = `const express = require('express');\napp.get("/x", handler);`;
		const facts = extractExpressRoutes(source, "src/server.ts", "test-repo", []);
		expect(facts.length).toBe(1);
	});
});

// ── No false positives ──────────────────────────────────────────────

describe("extractExpressRoutes — no false positives", () => {
	it("returns empty for non-route code", () => {
		const facts = extract(`
const data = process(input);
const result = compute(42);
`);
		expect(facts).toEqual([]);
	});

	it("ignores bare / path", () => {
		const facts = extract('app.get("/", handler);');
		expect(facts).toEqual([]);
	});

	it("does not extract app.use (middleware, not route)", () => {
		const facts = extract('app.use("/api", middleware);');
		expect(facts).toEqual([]);
	});

	it("does not extract app.listen", () => {
		const facts = extract("app.listen(3000, () => {});");
		expect(facts).toEqual([]);
	});
});

// ── Multiple routes ─────────────────────────────────────────────────

describe("extractExpressRoutes — multiple routes", () => {
	it("extracts multiple routes from one file", () => {
		const facts = extract(`
app.get("/api/v2/products", handler);
app.post("/api/v2/products", handler);
app.delete("/api/v2/products/:id", handler);
`);
		expect(facts.length).toBe(3);
		expect(facts.map((f) => f.metadata.httpMethod)).toEqual(["GET", "POST", "DELETE"]);
	});
});

// ── fraktag pattern ─────────────────────────────────────────────────

describe("extractExpressRoutes — fraktag patterns", () => {
	it("extracts fraktag-style inline async handler", () => {
		const facts = extract(`
app.get('/api/knowledge-bases', async (req, res) => {
  const kbs = fraktag.listKnowledgeBases();
  res.json(kbs);
});

app.get('/api/knowledge-bases/:id', async (req, res) => {
  const kb = fraktag.getKnowledgeBase(req.params.id);
  res.json(kb);
});

app.post('/api/knowledge-bases', async (req, res) => {
  const kb = await fraktag.createKnowledgeBase(req.body);
  res.json(kb);
});
`);
		expect(facts.length).toBe(3);
		expect(facts[0].address).toBe("/api/knowledge-bases");
		expect(facts[0].metadata.httpMethod).toBe("GET");
		expect(facts[1].address).toBe("/api/knowledge-bases/{id}");
		expect(facts[1].metadata.httpMethod).toBe("GET");
		expect(facts[2].address).toBe("/api/knowledge-bases");
		expect(facts[2].metadata.httpMethod).toBe("POST");
	});
});

// ── Binding resolution ──────────────────────────────────────────────

describe("extractExpressRoutes — binding resolution", () => {
	it("resolves route path from file-local constant", () => {
		const facts = extractResolved(`
const PREFIX = "/api/v2";
app.get(\`\${PREFIX}/products\`, handler);
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
	});

	it("resolves bare identifier route path from binding table", () => {
		const facts = extractResolved(`
const PRODUCTS = "/api/v2/products";
app.get(PRODUCTS, handler);
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("/api/v2/products");
	});
});
