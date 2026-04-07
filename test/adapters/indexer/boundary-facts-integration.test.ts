/**
 * Boundary-facts indexer integration test.
 *
 * Verifies end-to-end wiring: extractor → fact persistence → matcher → derived links.
 *
 * Uses an in-memory fixture with:
 *   - A Java file with Spring @GetMapping + @PostMapping routes (provider)
 *   - A TS file with axios.get + axios.post calls to matching paths (consumer)
 *
 * Assertions:
 *   - Provider facts are persisted with correct matcherKey normalization
 *   - Consumer facts are persisted with correct matcherKey normalization
 *   - Derived boundary links are materialized for matching paths
 *   - Non-matching paths produce no links
 *   - Facts survive in isolation (separate from edges table)
 */

import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { JavaExtractor } from "../../../src/adapters/extractors/java/java-extractor.js";
import { TypeScriptExtractor } from "../../../src/adapters/extractors/typescript/ts-extractor.js";
import { RepoIndexer } from "../../../src/adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";

// ── Fixture content ─────────────────────────────────────────────────

const JAVA_CONTROLLER = `
package com.example;

import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/v2/products")
public class ProductController {

    @GetMapping("/{id}")
    public Product getById(Long id) { return null; }

    @PostMapping("")
    public Product create(Product p) { return null; }

    @GetMapping("")
    public List<Product> list() { return null; }
}
`;

const TS_CLIENT = `
import axios from "axios";

export async function fetchProduct(id: string) {
  return axios.get(\`/api/v2/products/\${id}\`);
}

export async function createProduct(data: any) {
  return axios.post("/api/v2/products", data);
}

export async function fetchOrders() {
  return axios.get("/api/v2/orders");
}
`;

const PACKAGE_JSON = JSON.stringify({
	name: "boundary-test",
	version: "1.0.0",
	dependencies: { axios: "^1.0.0" },
});

const BUILD_GRADLE = `
plugins {
    id 'org.springframework.boot' version '3.0.0'
}
dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web:3.0.0'
}
`;

// ── Test setup ──────────────────────────────────────────────────────

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let tsExtractor: TypeScriptExtractor;
let javaExtractor: JavaExtractor;
let indexer: RepoIndexer;
let dbPath: string;
let fixtureRoot: string;

const REPO_UID = "boundary-test";

beforeAll(async () => {
	tsExtractor = new TypeScriptExtractor();
	await tsExtractor.initialize();
	javaExtractor = new JavaExtractor();
	await javaExtractor.initialize();
});

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-boundary-int-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	indexer = new RepoIndexer(storage, [tsExtractor, javaExtractor]);

	// Create temporary fixture directory with files.
	fixtureRoot = join(tmpdir(), `rgr-boundary-fixture-${randomUUID()}`);
	mkdirSync(join(fixtureRoot, "src"), { recursive: true });
	writeFileSync(
		join(fixtureRoot, "src", "ProductController.java"),
		JAVA_CONTROLLER,
	);
	writeFileSync(join(fixtureRoot, "src", "client.ts"), TS_CLIENT);
	writeFileSync(join(fixtureRoot, "package.json"), PACKAGE_JSON);
	writeFileSync(join(fixtureRoot, "build.gradle"), BUILD_GRADLE);

	storage.addRepo({
		repoUid: REPO_UID,
		name: REPO_UID,
		rootPath: fixtureRoot,
		defaultBranch: "main",
		createdAt: new Date().toISOString(),
		metadataJson: null,
	});
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore
	}
	try {
		rmSync(fixtureRoot, { recursive: true, force: true });
	} catch {
		// ignore
	}
});

// ── Integration tests ───────────────────────────────────────────────

describe("boundary-facts indexer integration", () => {
	it("persists provider facts from Spring route extraction", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const providers = storage.queryBoundaryProviderFacts({
			snapshotUid: result.snapshotUid,
		});

		// ProductController has 3 route annotations: GET /{id}, POST "", GET ""
		expect(providers.length).toBe(3);

		// Check one specific route.
		const getById = providers.find((p) =>
			p.address === "/api/v2/products/{id}",
		);
		expect(getById).toBeDefined();
		expect(getById!.mechanism).toBe("http");
		expect(getById!.operation).toBe("GET /api/v2/products/{id}");
		expect(getById!.matcherKey).toBe("GET /api/v2/products/{_}");
		expect(getById!.framework).toBe("spring-mvc");
		expect(getById!.basis).toBe("annotation");
		expect(getById!.sourceFile).toBe("src/ProductController.java");
	});

	it("persists consumer facts from HTTP client extraction", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const consumers = storage.queryBoundaryConsumerFacts({
			snapshotUid: result.snapshotUid,
		});

		// client.ts has 3 axios calls: get products/{id}, post products, get orders
		expect(consumers.length).toBe(3);

		// Check one specific consumer.
		const fetchProduct = consumers.find((c) =>
			c.address === "/api/v2/products/{param}",
		);
		expect(fetchProduct).toBeDefined();
		expect(fetchProduct!.mechanism).toBe("http");
		expect(fetchProduct!.matcherKey).toBe("GET /api/v2/products/{_}");
		expect(fetchProduct!.basis).toBe("template");
		expect(fetchProduct!.sourceFile).toBe("src/client.ts");
	});

	it("materializes derived links for matching provider-consumer pairs", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const links = storage.queryBoundaryLinks({
			snapshotUid: result.snapshotUid,
		});

		// Provider routes:      GET /api/v2/products/{id}, POST /api/v2/products, GET /api/v2/products
		// Consumer calls:       GET /api/v2/products/{param}, POST /api/v2/products, GET /api/v2/orders
		//
		// Expected matches:
		//   GET /api/v2/products/{_} → fetchProduct (address_match)
		//   POST /api/v2/products    → createProduct (address_match)
		//   GET /api/v2/products     → (no consumer with this exact path)
		//   GET /api/v2/orders       → (no provider)
		//
		// So 2 links.
		expect(links.length).toBe(2);

		const getMatch = links.find((l) =>
			l.providerAddress === "/api/v2/products/{id}",
		);
		expect(getMatch).toBeDefined();
		expect(getMatch!.matchBasis).toBe("address_match");
		expect(getMatch!.providerMechanism).toBe("http");
		expect(getMatch!.consumerAddress).toBe("/api/v2/products/{param}");
		expect(getMatch!.confidence).toBeGreaterThan(0);
		expect(getMatch!.confidence).toBeLessThanOrEqual(1);

		const postMatch = links.find((l) =>
			l.providerAddress === "/api/v2/products",
		);
		expect(postMatch).toBeDefined();
		expect(postMatch!.matchBasis).toBe("address_match");
	});

	it("does NOT create links for unmatched paths", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		const links = storage.queryBoundaryLinks({
			snapshotUid: result.snapshotUid,
		});

		// GET /api/v2/orders has no provider — should not appear in links.
		const ordersLink = links.find(
			(l) =>
				l.consumerAddress.includes("orders") ||
				l.providerAddress.includes("orders"),
		);
		expect(ordersLink).toBeUndefined();
	});

	it("boundary facts are separate from the edges table", async () => {
		const result = await indexer.indexRepo(REPO_UID);

		// Boundary facts exist.
		const providers = storage.queryBoundaryProviderFacts({
			snapshotUid: result.snapshotUid,
		});
		expect(providers.length).toBeGreaterThan(0);

		// The edges table does NOT contain BOUNDARY_LINK type.
		const db = provider.getDatabase();
		const boundaryEdges = db
			.prepare(
				"SELECT COUNT(*) as count FROM edges WHERE snapshot_uid = ? AND type = 'BOUNDARY_LINK'",
			)
			.get(result.snapshotUid) as { count: number };
		expect(boundaryEdges.count).toBe(0);
	});

	it("re-indexing replaces boundary facts cleanly", async () => {
		// Index once.
		const result1 = await indexer.indexRepo(REPO_UID);
		const p1 = storage.queryBoundaryProviderFacts({
			snapshotUid: result1.snapshotUid,
		});
		expect(p1.length).toBe(3);

		// Re-index (refresh creates a new snapshot).
		const result2 = await indexer.refreshRepo(REPO_UID);

		// New snapshot has its own facts.
		const p2 = storage.queryBoundaryProviderFacts({
			snapshotUid: result2.snapshotUid,
		});
		expect(p2.length).toBe(3);

		// Old snapshot facts are still there (snapshot-scoped, not replaced).
		const p1Again = storage.queryBoundaryProviderFacts({
			snapshotUid: result1.snapshotUid,
		});
		expect(p1Again.length).toBe(3);
	});
});
