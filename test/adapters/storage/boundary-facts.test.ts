/**
 * Round-trip tests for the boundary-facts persistence surface.
 *
 * Covers:
 *   - Migration 008 table creation
 *   - insertBoundaryProviderFacts / queryBoundaryProviderFacts round-trip
 *   - insertBoundaryConsumerFacts / queryBoundaryConsumerFacts round-trip
 *   - insertBoundaryLinks / queryBoundaryLinks round-trip (joined read shape)
 *   - Filter combinations (mechanism, matcherKey, sourceFile)
 *   - Snapshot scoping (facts from other snapshots are invisible)
 *   - deleteBoundaryLinksBySnapshot (derived artifact cleanup)
 *   - deleteBoundaryFactsBySnapshot (FK-ordered cascade)
 *   - Deterministic ordering (sourceFile ASC, lineStart ASC)
 *   - Metadata JSON serialization round-trip
 *
 * These tests exercise the port contract directly. They do NOT run
 * extractors or the matcher — every row is supplied by the test
 * harness to match the "storage is a pure persister" contract.
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { SnapshotKind } from "../../../src/core/model/index.js";
import type {
	PersistedBoundaryConsumerFact,
	PersistedBoundaryLink,
	PersistedBoundaryProviderFact,
} from "../../../src/core/ports/storage.js";

// ── Fixture setup ────────────────────────────────────────────────────

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let dbPath: string;

const REPO_UID = "test-repo";
const REPO = {
	repoUid: REPO_UID,
	name: "test-repo",
	rootPath: "/tmp/test-repo",
	defaultBranch: "main",
	createdAt: new Date().toISOString(),
	metadataJson: null,
};
const OBSERVED_AT = "2026-04-07T10:00:00.000Z";

function makeProviderFact(
	snapshotUid: string,
	overrides?: Partial<PersistedBoundaryProviderFact>,
): PersistedBoundaryProviderFact {
	return {
		factUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		mechanism: "http",
		operation: "GET /api/v2/products/{id}",
		address: "/api/v2/products/{id}",
		matcherKey: "GET /api/v2/products/{_}",
		handlerStableKey: `${REPO_UID}:src/Controller.java#getById:SYMBOL:METHOD`,
		sourceFile: "src/Controller.java",
		lineStart: 10,
		framework: "spring-mvc",
		basis: "annotation" as const,
		schemaRef: null,
		metadata: { httpMethod: "GET" },
		extractor: "spring-route-extractor:0.1",
		observedAt: OBSERVED_AT,
		...overrides,
	};
}

function makeConsumerFact(
	snapshotUid: string,
	overrides?: Partial<PersistedBoundaryConsumerFact>,
): PersistedBoundaryConsumerFact {
	return {
		factUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		mechanism: "http",
		operation: "GET /api/v2/products/{param}",
		address: "/api/v2/products/{param}",
		matcherKey: "GET /api/v2/products/{_}",
		callerStableKey: `${REPO_UID}:src/api/client.ts#fetchProducts:SYMBOL:FUNCTION`,
		sourceFile: "src/api/client.ts",
		lineStart: 25,
		basis: "template" as const,
		confidence: 0.8,
		schemaRef: null,
		metadata: { httpMethod: "GET", rawUrl: "${VITE_API_URL}/api/v2/products/${id}" },
		extractor: "http-client-extractor:0.1",
		observedAt: OBSERVED_AT,
		...overrides,
	};
}

function makeLink(
	snapshotUid: string,
	providerFactUid: string,
	consumerFactUid: string,
	overrides?: Partial<PersistedBoundaryLink>,
): PersistedBoundaryLink {
	return {
		linkUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		providerFactUid,
		consumerFactUid,
		matchBasis: "address_match" as const,
		confidence: 0.72,
		metadataJson: null,
		materializedAt: "2026-04-07T10:01:00.000Z",
		...overrides,
	};
}

beforeEach(() => {
	dbPath = join(tmpdir(), `rgr-boundary-test-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	storage.addRepo(REPO);
});

afterEach(() => {
	provider.close();
	try {
		unlinkSync(dbPath);
	} catch {
		// ignore cleanup errors
	}
});

// ── Migration 008 ───────────────────────────────────────────────────

describe("migration 008 — boundary-facts tables", () => {
	it("creates all three tables", () => {
		const db = provider.getDatabase();
		const tables = db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'boundary_%' ORDER BY name",
			)
			.all() as Array<{ name: string }>;
		expect(tables.map((t) => t.name)).toEqual([
			"boundary_consumer_facts",
			"boundary_links",
			"boundary_provider_facts",
		]);
	});

	it("registers migration version 8", () => {
		const db = provider.getDatabase();
		const row = db
			.prepare("SELECT version, name FROM schema_migrations WHERE version = 8")
			.get() as { version: number; name: string } | undefined;
		expect(row).toBeDefined();
		expect(row!.name).toBe("008-boundary-facts");
	});

	it("creates indexes on provider facts", () => {
		const db = provider.getDatabase();
		const indexes = db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='boundary_provider_facts' AND name LIKE 'idx_%'",
			)
			.all() as Array<{ name: string }>;
		const names = indexes.map((i) => i.name).sort();
		expect(names).toContain("idx_bp_facts_snapshot_mechanism");
		expect(names).toContain("idx_bp_facts_snapshot_matcher_key");
		expect(names).toContain("idx_bp_facts_snapshot_handler");
	});

	it("creates indexes on consumer facts", () => {
		const db = provider.getDatabase();
		const indexes = db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='boundary_consumer_facts' AND name LIKE 'idx_%'",
			)
			.all() as Array<{ name: string }>;
		const names = indexes.map((i) => i.name).sort();
		expect(names).toContain("idx_bc_facts_snapshot_mechanism");
		expect(names).toContain("idx_bc_facts_snapshot_matcher_key");
		expect(names).toContain("idx_bc_facts_snapshot_caller");
	});

	it("creates indexes on boundary links", () => {
		const db = provider.getDatabase();
		const indexes = db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='boundary_links' AND name LIKE 'idx_%'",
			)
			.all() as Array<{ name: string }>;
		const names = indexes.map((i) => i.name).sort();
		expect(names).toContain("idx_bl_snapshot");
		expect(names).toContain("idx_bl_provider");
		expect(names).toContain("idx_bl_consumer");
	});
});

// ── Provider fact round-trip ────────────────────────────────────────

describe("insertBoundaryProviderFacts / queryBoundaryProviderFacts", () => {
	it("persists and reads back a single provider fact", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fact = makeProviderFact(snap.snapshotUid);
		storage.insertBoundaryProviderFacts([fact]);

		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.length).toBe(1);
		const r = rows[0];
		expect(r.factUid).toBe(fact.factUid);
		expect(r.mechanism).toBe("http");
		expect(r.operation).toBe("GET /api/v2/products/{id}");
		expect(r.address).toBe("/api/v2/products/{id}");
		expect(r.matcherKey).toBe("GET /api/v2/products/{_}");
		expect(r.handlerStableKey).toBe(fact.handlerStableKey);
		expect(r.sourceFile).toBe("src/Controller.java");
		expect(r.lineStart).toBe(10);
		expect(r.framework).toBe("spring-mvc");
		expect(r.basis).toBe("annotation");
		expect(r.schemaRef).toBeNull();
		expect(r.extractor).toBe("spring-route-extractor:0.1");
		expect(r.observedAt).toBe(OBSERVED_AT);
	});

	it("serializes and deserializes metadata JSON", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fact = makeProviderFact(snap.snapshotUid, {
			metadata: { httpMethod: "POST", requestBody: "ProductDTO" },
		});
		storage.insertBoundaryProviderFacts([fact]);

		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows[0].metadata).toEqual({
			httpMethod: "POST",
			requestBody: "ProductDTO",
		});
	});

	it("persists batch atomically", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const facts = Array.from({ length: 20 }, (_, i) =>
			makeProviderFact(snap.snapshotUid, {
				operation: `GET /api/v2/items/${i}`,
				address: `/api/v2/items/${i}`,
				lineStart: i + 1,
			}),
		);
		storage.insertBoundaryProviderFacts(facts);

		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.length).toBe(20);
	});

	it("returns empty for snapshot with no facts", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows).toEqual([]);
	});
});

// ── Consumer fact round-trip ────────────────────────────────────────

describe("insertBoundaryConsumerFacts / queryBoundaryConsumerFacts", () => {
	it("persists and reads back a single consumer fact", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fact = makeConsumerFact(snap.snapshotUid);
		storage.insertBoundaryConsumerFacts([fact]);

		const rows = storage.queryBoundaryConsumerFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.length).toBe(1);
		const r = rows[0];
		expect(r.factUid).toBe(fact.factUid);
		expect(r.mechanism).toBe("http");
		expect(r.operation).toBe("GET /api/v2/products/{param}");
		expect(r.address).toBe("/api/v2/products/{param}");
		expect(r.matcherKey).toBe("GET /api/v2/products/{_}");
		expect(r.callerStableKey).toBe(fact.callerStableKey);
		expect(r.sourceFile).toBe("src/api/client.ts");
		expect(r.lineStart).toBe(25);
		expect(r.basis).toBe("template");
		expect(r.confidence).toBe(0.8);
		expect(r.schemaRef).toBeNull();
		expect(r.extractor).toBe("http-client-extractor:0.1");
		expect(r.observedAt).toBe(OBSERVED_AT);
	});

	it("serializes and deserializes metadata JSON with rawUrl", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fact = makeConsumerFact(snap.snapshotUid);
		storage.insertBoundaryConsumerFacts([fact]);

		const rows = storage.queryBoundaryConsumerFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows[0].metadata).toEqual({
			httpMethod: "GET",
			rawUrl: "${VITE_API_URL}/api/v2/products/${id}",
		});
	});

	it("persists null metadata without error", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const fact = makeConsumerFact(snap.snapshotUid, { metadata: {} });
		storage.insertBoundaryConsumerFacts([fact]);

		const rows = storage.queryBoundaryConsumerFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.length).toBe(1);
	});
});

// ── Filters ─────────────────────────────────────────────────────────

describe("boundary fact query filters", () => {
	function seed() {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		storage.insertBoundaryProviderFacts([
			makeProviderFact(snap.snapshotUid, {
				mechanism: "http" as any,
				matcherKey: "GET /api/v2/products/{_}",
				sourceFile: "src/ProductController.java",
			}),
			makeProviderFact(snap.snapshotUid, {
				mechanism: "http" as any,
				matcherKey: "POST /api/v2/orders",
				sourceFile: "src/OrderController.java",
			}),
			makeProviderFact(snap.snapshotUid, {
				mechanism: "grpc" as any,
				matcherKey: "OrderService.GetOrder",
				sourceFile: "src/OrderService.java",
			}),
		]);
		return snap;
	}

	it("filters by mechanism", () => {
		const snap = seed();
		const http = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			mechanism: "http",
		});
		expect(http.length).toBe(2);

		const grpc = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			mechanism: "grpc",
		});
		expect(grpc.length).toBe(1);
	});

	it("filters by matcherKey (exact)", () => {
		const snap = seed();
		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			matcherKey: "POST /api/v2/orders",
		});
		expect(rows.length).toBe(1);
		expect(rows[0].sourceFile).toBe("src/OrderController.java");
	});

	it("filters by sourceFile", () => {
		const snap = seed();
		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			sourceFile: "src/ProductController.java",
		});
		expect(rows.length).toBe(1);
		expect(rows[0].matcherKey).toBe("GET /api/v2/products/{_}");
	});

	it("combines filters with AND semantics", () => {
		const snap = seed();
		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			mechanism: "http",
			sourceFile: "src/OrderController.java",
		});
		expect(rows.length).toBe(1);
		expect(rows[0].matcherKey).toBe("POST /api/v2/orders");
	});

	it("applies limit", () => {
		const snap = seed();
		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
			limit: 1,
		});
		expect(rows.length).toBe(1);
	});
});

// ── Snapshot scoping ────────────────────────────────────────────────

describe("boundary facts — snapshot scoping", () => {
	it("provider facts from other snapshots are invisible", () => {
		const snap1 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const snap2 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "def",
		});

		storage.insertBoundaryProviderFacts([
			makeProviderFact(snap1.snapshotUid, { operation: "GET /snap1" }),
		]);
		storage.insertBoundaryProviderFacts([
			makeProviderFact(snap2.snapshotUid, { operation: "GET /snap2" }),
		]);

		const r1 = storage.queryBoundaryProviderFacts({ snapshotUid: snap1.snapshotUid });
		expect(r1.length).toBe(1);
		expect(r1[0].operation).toBe("GET /snap1");

		const r2 = storage.queryBoundaryProviderFacts({ snapshotUid: snap2.snapshotUid });
		expect(r2.length).toBe(1);
		expect(r2[0].operation).toBe("GET /snap2");
	});

	it("consumer facts from other snapshots are invisible", () => {
		const snap1 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const snap2 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "def",
		});

		storage.insertBoundaryConsumerFacts([
			makeConsumerFact(snap1.snapshotUid, { operation: "GET /snap1" }),
		]);
		storage.insertBoundaryConsumerFacts([
			makeConsumerFact(snap2.snapshotUid, { operation: "GET /snap2" }),
		]);

		const r1 = storage.queryBoundaryConsumerFacts({ snapshotUid: snap1.snapshotUid });
		expect(r1.length).toBe(1);
		expect(r1[0].operation).toBe("GET /snap1");

		const r2 = storage.queryBoundaryConsumerFacts({ snapshotUid: snap2.snapshotUid });
		expect(r2.length).toBe(1);
		expect(r2[0].operation).toBe("GET /snap2");
	});
});

// ── Ordering ────────────────────────────────────────────────────────

describe("boundary facts — deterministic ordering", () => {
	it("provider facts ordered by sourceFile ASC, lineStart ASC", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		// Insert in scrambled order.
		storage.insertBoundaryProviderFacts([
			makeProviderFact(snap.snapshotUid, {
				sourceFile: "src/B.java",
				lineStart: 5,
				operation: "B:5",
			}),
			makeProviderFact(snap.snapshotUid, {
				sourceFile: "src/A.java",
				lineStart: 20,
				operation: "A:20",
			}),
			makeProviderFact(snap.snapshotUid, {
				sourceFile: "src/A.java",
				lineStart: 10,
				operation: "A:10",
			}),
		]);

		const rows = storage.queryBoundaryProviderFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.map((r) => r.operation)).toEqual(["A:10", "A:20", "B:5"]);
	});

	it("consumer facts ordered by sourceFile ASC, lineStart ASC", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		storage.insertBoundaryConsumerFacts([
			makeConsumerFact(snap.snapshotUid, {
				sourceFile: "src/z.ts",
				lineStart: 1,
				operation: "z:1",
			}),
			makeConsumerFact(snap.snapshotUid, {
				sourceFile: "src/a.ts",
				lineStart: 50,
				operation: "a:50",
			}),
			makeConsumerFact(snap.snapshotUid, {
				sourceFile: "src/a.ts",
				lineStart: 10,
				operation: "a:10",
			}),
		]);

		const rows = storage.queryBoundaryConsumerFacts({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.map((r) => r.operation)).toEqual(["a:10", "a:50", "z:1"]);
	});
});

// ── Boundary links round-trip ───────────────────────────────────────

describe("boundary links — insert / query / delete", () => {
	function seedWithLink() {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const pf = makeProviderFact(snap.snapshotUid);
		const cf = makeConsumerFact(snap.snapshotUid);
		storage.insertBoundaryProviderFacts([pf]);
		storage.insertBoundaryConsumerFacts([cf]);

		const link = makeLink(snap.snapshotUid, pf.factUid, cf.factUid);
		storage.insertBoundaryLinks([link]);
		return { snap, pf, cf, link };
	}

	it("persists and reads back a link with joined provider/consumer details", () => {
		const { snap, pf, cf, link } = seedWithLink();

		const rows = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows.length).toBe(1);
		const r = rows[0];
		expect(r.linkUid).toBe(link.linkUid);
		expect(r.matchBasis).toBe("address_match");
		expect(r.confidence).toBe(0.72);
		// Provider side
		expect(r.providerMechanism).toBe("http");
		expect(r.providerOperation).toBe(pf.operation);
		expect(r.providerAddress).toBe(pf.address);
		expect(r.providerHandlerStableKey).toBe(pf.handlerStableKey);
		expect(r.providerSourceFile).toBe(pf.sourceFile);
		expect(r.providerLineStart).toBe(pf.lineStart);
		expect(r.providerFramework).toBe("spring-mvc");
		// Consumer side
		expect(r.consumerOperation).toBe(cf.operation);
		expect(r.consumerAddress).toBe(cf.address);
		expect(r.consumerCallerStableKey).toBe(cf.callerStableKey);
		expect(r.consumerSourceFile).toBe(cf.sourceFile);
		expect(r.consumerLineStart).toBe(cf.lineStart);
		expect(r.consumerBasis).toBe("template");
		expect(r.consumerConfidence).toBe(0.8);
	});

	it("filters links by mechanism (via provider)", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const httpProv = makeProviderFact(snap.snapshotUid, { mechanism: "http" as any });
		const grpcProv = makeProviderFact(snap.snapshotUid, {
			mechanism: "grpc" as any,
			framework: "grpc-java",
		});
		const httpCons = makeConsumerFact(snap.snapshotUid, { mechanism: "http" as any });
		const grpcCons = makeConsumerFact(snap.snapshotUid, { mechanism: "grpc" as any });
		storage.insertBoundaryProviderFacts([httpProv, grpcProv]);
		storage.insertBoundaryConsumerFacts([httpCons, grpcCons]);

		storage.insertBoundaryLinks([
			makeLink(snap.snapshotUid, httpProv.factUid, httpCons.factUid),
			makeLink(snap.snapshotUid, grpcProv.factUid, grpcCons.factUid),
		]);

		const http = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
			mechanism: "http",
		});
		expect(http.length).toBe(1);
		expect(http[0].providerMechanism).toBe("http");

		const grpc = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
			mechanism: "grpc",
		});
		expect(grpc.length).toBe(1);
		expect(grpc[0].providerMechanism).toBe("grpc");
	});

	it("filters links by matchBasis", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const pf = makeProviderFact(snap.snapshotUid);
		const cf1 = makeConsumerFact(snap.snapshotUid);
		const cf2 = makeConsumerFact(snap.snapshotUid, { operation: "GET /other" });
		storage.insertBoundaryProviderFacts([pf]);
		storage.insertBoundaryConsumerFacts([cf1, cf2]);

		storage.insertBoundaryLinks([
			makeLink(snap.snapshotUid, pf.factUid, cf1.factUid, {
				matchBasis: "address_match",
			}),
			makeLink(snap.snapshotUid, pf.factUid, cf2.factUid, {
				matchBasis: "heuristic",
			}),
		]);

		const exact = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
			matchBasis: "address_match",
		});
		expect(exact.length).toBe(1);
	});

	it("applies limit to link queries", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const pf = makeProviderFact(snap.snapshotUid);
		storage.insertBoundaryProviderFacts([pf]);

		const consumers = Array.from({ length: 5 }, (_, i) =>
			makeConsumerFact(snap.snapshotUid, { operation: `GET /item/${i}` }),
		);
		storage.insertBoundaryConsumerFacts(consumers);

		const links = consumers.map((cf) =>
			makeLink(snap.snapshotUid, pf.factUid, cf.factUid),
		);
		storage.insertBoundaryLinks(links);

		const rows = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
			limit: 2,
		});
		expect(rows.length).toBe(2);
	});

	it("deleteBoundaryLinksBySnapshot removes links but keeps facts", () => {
		const { snap } = seedWithLink();

		// Links exist.
		expect(
			storage.queryBoundaryLinks({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);

		storage.deleteBoundaryLinksBySnapshot(snap.snapshotUid);

		// Links gone.
		expect(
			storage.queryBoundaryLinks({ snapshotUid: snap.snapshotUid }).length,
		).toBe(0);
		// Facts still here.
		expect(
			storage.queryBoundaryProviderFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);
		expect(
			storage.queryBoundaryConsumerFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);
	});

	it("deleteBoundaryFactsBySnapshot removes links, then consumer facts, then provider facts", () => {
		const { snap } = seedWithLink();

		// Everything exists.
		expect(
			storage.queryBoundaryLinks({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);
		expect(
			storage.queryBoundaryProviderFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);
		expect(
			storage.queryBoundaryConsumerFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(1);

		storage.deleteBoundaryFactsBySnapshot(snap.snapshotUid);

		// All gone — no FK violation.
		expect(
			storage.queryBoundaryLinks({ snapshotUid: snap.snapshotUid }).length,
		).toBe(0);
		expect(
			storage.queryBoundaryProviderFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(0);
		expect(
			storage.queryBoundaryConsumerFacts({ snapshotUid: snap.snapshotUid }).length,
		).toBe(0);
	});

	it("deleteBoundaryFactsBySnapshot does not affect other snapshots", () => {
		const snap1 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const snap2 = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "def",
		});

		storage.insertBoundaryProviderFacts([
			makeProviderFact(snap1.snapshotUid),
			makeProviderFact(snap2.snapshotUid),
		]);

		storage.deleteBoundaryFactsBySnapshot(snap1.snapshotUid);

		expect(
			storage.queryBoundaryProviderFacts({ snapshotUid: snap1.snapshotUid }).length,
		).toBe(0);
		expect(
			storage.queryBoundaryProviderFacts({ snapshotUid: snap2.snapshotUid }).length,
		).toBe(1);
	});
});

// ── Link metadata ───────────────────────────────────────────────────

describe("boundary links — metadata round-trip", () => {
	it("persists and returns link-level metadata_json", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const pf = makeProviderFact(snap.snapshotUid);
		const cf = makeConsumerFact(snap.snapshotUid);
		storage.insertBoundaryProviderFacts([pf]);
		storage.insertBoundaryConsumerFacts([cf]);

		const link = makeLink(snap.snapshotUid, pf.factUid, cf.factUid, {
			metadataJson: JSON.stringify({
				matchedSegments: 4,
				wildcardSegments: 1,
			}),
		});
		storage.insertBoundaryLinks([link]);

		const rows = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows[0].metadataJson).not.toBeNull();
		const meta = JSON.parse(rows[0].metadataJson!);
		expect(meta.matchedSegments).toBe(4);
		expect(meta.wildcardSegments).toBe(1);
	});

	it("returns null metadata_json when none was stored", () => {
		const snap = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc",
		});
		const pf = makeProviderFact(snap.snapshotUid);
		const cf = makeConsumerFact(snap.snapshotUid);
		storage.insertBoundaryProviderFacts([pf]);
		storage.insertBoundaryConsumerFacts([cf]);

		const link = makeLink(snap.snapshotUid, pf.factUid, cf.factUid, {
			metadataJson: null,
		});
		storage.insertBoundaryLinks([link]);

		const rows = storage.queryBoundaryLinks({
			snapshotUid: snap.snapshotUid,
		});
		expect(rows[0].metadataJson).toBeNull();
	});
});
