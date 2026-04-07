/**
 * Boundary matcher — unit tests.
 *
 * Tests the mechanism-keyed matching strategy and orchestrator.
 * Pure logic tests — no storage, no I/O.
 *
 * Covers:
 *   - HTTP path normalization (segment wildcarding)
 *   - Matcher key computation
 *   - Exact method+path matching
 *   - Path-only fallback matching (unknown method)
 *   - Multi-provider / multi-consumer cartesian matching
 *   - Confidence scoring (literal vs wildcard, short vs long paths)
 *   - Mechanism dispatch (only same-mechanism facts are matched)
 *   - No false positives (different paths, different methods)
 *   - Empty inputs produce no candidates
 */

import { randomUUID } from "node:crypto";
import { describe, expect, it } from "vitest";
import {
	HttpBoundaryMatchStrategy,
	type MatchableConsumerFact,
	type MatchableProviderFact,
	getMatchStrategy,
	matchBoundaryFacts,
} from "../../../src/core/classification/boundary-matcher.js";

// ── Helpers ─────────────────────────────────────────────────────────

function makeProvider(overrides?: Partial<MatchableProviderFact>): MatchableProviderFact {
	return {
		factUid: randomUUID(),
		mechanism: "http",
		operation: "GET /api/v2/products/{id}",
		address: "/api/v2/products/{id}",
		handlerStableKey: "repo:Controller.java#getById",
		sourceFile: "src/Controller.java",
		lineStart: 10,
		framework: "spring-mvc",
		basis: "annotation",
		schemaRef: null,
		metadata: { httpMethod: "GET" },
		...overrides,
	};
}

function makeConsumer(overrides?: Partial<MatchableConsumerFact>): MatchableConsumerFact {
	return {
		factUid: randomUUID(),
		mechanism: "http",
		operation: "GET /api/v2/products/{param}",
		address: "/api/v2/products/{param}",
		callerStableKey: "repo:client.ts#fetchProducts",
		sourceFile: "src/api/client.ts",
		lineStart: 25,
		basis: "template",
		confidence: 0.8,
		schemaRef: null,
		metadata: { httpMethod: "GET" },
		...overrides,
	};
}

const http = new HttpBoundaryMatchStrategy();

// ── Matcher key computation ─────────────────────────────────────────

describe("HttpBoundaryMatchStrategy — computeMatcherKey", () => {
	it("normalizes Spring {id} to {_}", () => {
		const key = http.computeMatcherKey("/api/v2/products/{id}", {
			httpMethod: "GET",
		});
		expect(key).toBe("GET /api/v2/products/{_}");
	});

	it("normalizes consumer {param} to {_}", () => {
		const key = http.computeMatcherKey("/api/v2/products/{param}", {
			httpMethod: "GET",
		});
		expect(key).toBe("GET /api/v2/products/{_}");
	});

	it("normalizes Express :id to {_}", () => {
		const key = http.computeMatcherKey("/api/v2/products/:id", {
			httpMethod: "GET",
		});
		expect(key).toBe("GET /api/v2/products/{_}");
	});

	it("normalizes multiple path parameters", () => {
		const key = http.computeMatcherKey(
			"/api/v2/projects/{projectId}/tasks/{taskId}",
			{ httpMethod: "GET" },
		);
		expect(key).toBe("GET /api/v2/projects/{_}/tasks/{_}");
	});

	it("preserves literal segments exactly", () => {
		const key = http.computeMatcherKey("/api/v2/products", {
			httpMethod: "POST",
		});
		expect(key).toBe("POST /api/v2/products");
	});

	it("uses * for unknown HTTP method", () => {
		const key = http.computeMatcherKey("/api/v2/products", {});
		expect(key).toBe("* /api/v2/products");
	});

	it("uppercases method", () => {
		const key = http.computeMatcherKey("/health", { httpMethod: "get" });
		expect(key).toBe("GET /health");
	});

	it("handles root path", () => {
		const key = http.computeMatcherKey("/", { httpMethod: "GET" });
		expect(key).toBe("GET /");
	});

	it("Spring {id} and consumer {param} produce the same key", () => {
		const springKey = http.computeMatcherKey("/api/v2/products/{id}", {
			httpMethod: "GET",
		});
		const consumerKey = http.computeMatcherKey("/api/v2/products/{param}", {
			httpMethod: "GET",
		});
		expect(springKey).toBe(consumerKey);
	});

	it("Spring {id} and Express :productId produce the same key", () => {
		const springKey = http.computeMatcherKey("/api/v2/products/{id}", {
			httpMethod: "GET",
		});
		const expressKey = http.computeMatcherKey("/api/v2/products/:productId", {
			httpMethod: "GET",
		});
		expect(springKey).toBe(expressKey);
	});
});

// ── Exact matching ──────────────────────────────────────────────────

describe("HttpBoundaryMatchStrategy — exact matching", () => {
	it("matches provider {id} to consumer {param} for same path", () => {
		const prov = makeProvider();
		const cons = makeConsumer();
		const candidates = http.match([prov], [cons]);
		expect(candidates.length).toBe(1);
		expect(candidates[0].matchBasis).toBe("address_match");
		expect(candidates[0].providerFactUid).toBe(prov.factUid);
		expect(candidates[0].consumerFactUid).toBe(cons.factUid);
	});

	it("matches literal paths exactly", () => {
		const candidates = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
		);
		expect(candidates.length).toBe(1);
		expect(candidates[0].matchBasis).toBe("address_match");
	});

	it("does NOT match different paths", () => {
		const candidates = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/orders", metadata: { httpMethod: "GET" } })],
		);
		expect(candidates.length).toBe(0);
	});

	it("does NOT match different HTTP methods", () => {
		const candidates = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "POST" } })],
		);
		expect(candidates.length).toBe(0);
	});

	it("does NOT match different segment counts", () => {
		const candidates = http.match(
			[makeProvider({ address: "/api/v2/products/{id}", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
		);
		expect(candidates.length).toBe(0);
	});

	it("matches multiple consumers to one provider", () => {
		const provider = makeProvider({
			address: "/api/v2/products",
			metadata: { httpMethod: "GET" },
		});
		const consumers = [
			makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "GET" }, callerStableKey: "a" }),
			makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "GET" }, callerStableKey: "b" }),
		];
		const candidates = http.match([provider], consumers);
		expect(candidates.length).toBe(2);
		expect(candidates[0].providerFactUid).toBe(provider.factUid);
		expect(candidates[1].providerFactUid).toBe(provider.factUid);
	});

	it("matches multiple providers to one consumer", () => {
		const providers = [
			makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" }, handlerStableKey: "a" }),
			makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" }, handlerStableKey: "b" }),
		];
		const consumer = makeConsumer({
			address: "/api/v2/products",
			metadata: { httpMethod: "GET" },
		});
		const candidates = http.match(providers, [consumer]);
		expect(candidates.length).toBe(2);
	});
});

// ── Path-only fallback ──────────────────────────────────────────────

describe("HttpBoundaryMatchStrategy — path-only fallback", () => {
	it("matches consumer with unknown method via path-only (heuristic)", () => {
		const candidates = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", metadata: {} })],
		);
		expect(candidates.length).toBe(1);
		expect(candidates[0].matchBasis).toBe("heuristic");
	});

	it("path-only match has lower confidence than exact match", () => {
		const exact = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.9, metadata: { httpMethod: "GET" } })],
		);
		const pathOnly = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.9, metadata: {} })],
		);
		expect(pathOnly[0].confidence).toBeLessThan(exact[0].confidence);
	});
});

// ── Confidence scoring ──────────────────────────────────────────────

describe("HttpBoundaryMatchStrategy — confidence", () => {
	it("literal consumer has higher confidence than template consumer", () => {
		const literal = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.95, basis: "literal", metadata: { httpMethod: "GET" } })],
		);
		const template = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.8, basis: "template", metadata: { httpMethod: "GET" } })],
		);
		expect(literal[0].confidence).toBeGreaterThan(template[0].confidence);
	});

	it("all-literal path has higher confidence than wildcard path", () => {
		const allLiteral = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.9, metadata: { httpMethod: "GET" } })],
		);
		const hasWildcard = http.match(
			[makeProvider({ address: "/api/v2/products/{id}", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products/{param}", confidence: 0.9, metadata: { httpMethod: "GET" } })],
		);
		expect(allLiteral[0].confidence).toBeGreaterThan(hasWildcard[0].confidence);
	});

	it("short paths (/health) have lower confidence than multi-segment paths", () => {
		const short = http.match(
			[makeProvider({ address: "/health", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/health", confidence: 0.9, metadata: { httpMethod: "GET" } })],
		);
		const long = http.match(
			[makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } })],
			[makeConsumer({ address: "/api/v2/products", confidence: 0.9, metadata: { httpMethod: "GET" } })],
		);
		expect(short[0].confidence).toBeLessThan(long[0].confidence);
	});

	it("confidence is always in [0, 1]", () => {
		const candidates = http.match(
			[makeProvider()],
			[makeConsumer({ confidence: 1.0 })],
		);
		expect(candidates[0].confidence).toBeGreaterThanOrEqual(0);
		expect(candidates[0].confidence).toBeLessThanOrEqual(1);
	});
});

// ── Orchestrator ────────────────────────────────────────────────────

describe("matchBoundaryFacts — orchestrator", () => {
	it("dispatches HTTP facts to HTTP strategy", () => {
		const candidates = matchBoundaryFacts(
			[makeProvider()],
			[makeConsumer()],
		);
		expect(candidates.length).toBe(1);
	});

	it("does NOT match across mechanisms", () => {
		const candidates = matchBoundaryFacts(
			[makeProvider({ mechanism: "http" })],
			[makeConsumer({ mechanism: "grpc" as any })],
		);
		expect(candidates.length).toBe(0);
	});

	it("returns empty for empty inputs", () => {
		expect(matchBoundaryFacts([], [])).toEqual([]);
		expect(matchBoundaryFacts([makeProvider()], [])).toEqual([]);
		expect(matchBoundaryFacts([], [makeConsumer()])).toEqual([]);
	});

	it("skips mechanism with no registered strategy", () => {
		const candidates = matchBoundaryFacts(
			[makeProvider({ mechanism: "ioctl" as any })],
			[makeConsumer({ mechanism: "ioctl" as any })],
		);
		// No IOCTL strategy registered — silently skipped.
		expect(candidates.length).toBe(0);
	});

	it("matches mixed-mechanism facts correctly", () => {
		const httpProv = makeProvider({ address: "/api/v2/products", metadata: { httpMethod: "GET" } });
		const grpcProv = makeProvider({ mechanism: "grpc" as any, address: "OrderService.GetOrder" });
		const httpCons = makeConsumer({ address: "/api/v2/products", metadata: { httpMethod: "GET" } });
		const grpcCons = makeConsumer({ mechanism: "grpc" as any, address: "OrderService.GetOrder" });

		// Only HTTP has a strategy — grpc facts are skipped.
		const candidates = matchBoundaryFacts(
			[httpProv, grpcProv],
			[httpCons, grpcCons],
		);
		expect(candidates.length).toBe(1);
		// The matched candidate should reference the HTTP provider.
		expect(candidates[0].providerFactUid).toBe(httpProv.factUid);
	});
});

// ── getMatchStrategy ────────────────────────────────────────────────

describe("getMatchStrategy", () => {
	it("returns HTTP strategy for http mechanism", () => {
		const strategy = getMatchStrategy("http");
		expect(strategy).not.toBeNull();
		expect(strategy!.mechanism).toBe("http");
	});

	it("returns null for unregistered mechanism", () => {
		const strategy = getMatchStrategy("ioctl");
		expect(strategy).toBeNull();
	});
});
