/**
 * SB-2-pre-2 tolerance proof (TS half).
 *
 * Asserts that the TS storage adapter accepts insert + read of
 * nodes whose `subtype` column matches the seven canonical
 * strings added by SB-2-pre-2 for state-boundary resources
 * (CONNECTION, FILE_PATH, DIRECTORY_PATH, LOGICAL, CACHE, BUCKET,
 * CONTAINER) plus reuse of the pre-existing NAMESPACE subtype
 * for BLOB context.
 *
 * Scope: canonical vocabulary alignment. No emitter logic, no
 * binding logic, no state-extractor dependency.
 *
 * Pair test:
 *   `rust/crates/storage/tests/state_boundary_subtypes_tolerance.rs`
 * exercises the same vocabulary through the Rust storage layer.
 * Cross-runtime tolerance is transitive via the shared SQL schema
 * and per-runtime non-validating subtype deserialization
 * (confirmed at SB-2-pre).
 */

import { randomUUID } from "node:crypto";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../src/adapters/storage/sqlite/sqlite-storage.js";
import type { GraphNode } from "../../../src/core/model/index.js";
import {
	NodeKind,
	NodeSubtype,
	SnapshotKind,
} from "../../../src/core/model/index.js";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let dbPath: string;

const REPO_UID = "myservice";

beforeEach(() => {
	dbPath = join(tmpdir(), `sb-2-pre-2-tolerance-${randomUUID()}.db`);
	provider = new SqliteConnectionProvider(dbPath);
	provider.initialize();
	storage = new SqliteStorage(provider.getDatabase());
	storage.addRepo({
		repoUid: REPO_UID,
		name: "myservice",
		rootPath: "/tmp/myservice",
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
		// best-effort
	}
});

function makeResourceNode(
	snapshotUid: string,
	stableKey: string,
	kind: GraphNode["kind"],
	subtype: GraphNode["subtype"],
	name: string,
): GraphNode {
	return {
		nodeUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		stableKey,
		kind,
		subtype,
		name,
		qualifiedName: null,
		fileUid: null,
		parentNodeUid: null,
		location: null,
		signature: null,
		visibility: null,
		docComment: null,
		metadataJson: null,
	};
}

describe("SB-2-pre-2 — TS storage tolerates new resource subtypes", () => {
	it("round-trips all seven new subtype strings plus reused NAMESPACE", () => {
		const snapshot = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc123",
		});

		const nodes: GraphNode[] = [
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:db:postgres:DATABASE_URL:DB_RESOURCE",
				NodeKind.DB_RESOURCE,
				NodeSubtype.CONNECTION,
				"DATABASE_URL",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:fs:/etc/app/settings.yaml:FS_PATH",
				NodeKind.FS_PATH,
				NodeSubtype.FILE_PATH,
				"/etc/app/settings.yaml",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:fs:/var/cache:FS_PATH",
				NodeKind.FS_PATH,
				NodeSubtype.DIRECTORY_PATH,
				"/var/cache",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:fs:CACHE_DIR:FS_PATH",
				NodeKind.FS_PATH,
				NodeSubtype.LOGICAL,
				"CACHE_DIR",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:cache:redis:REDIS_URL:STATE",
				NodeKind.STATE,
				NodeSubtype.CACHE,
				"REDIS_URL",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:blob:s3:artifacts-bucket:BLOB",
				NodeKind.BLOB,
				NodeSubtype.BUCKET,
				"artifacts-bucket",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:blob:azure:logs-container:BLOB",
				NodeKind.BLOB,
				NodeSubtype.CONTAINER,
				"logs-container",
			),
			// Reuse case: pre-existing NAMESPACE for BLOB.
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:blob:gcs:my-ns:BLOB",
				NodeKind.BLOB,
				NodeSubtype.NAMESPACE,
				"my-ns",
			),
		];

		storage.insertNodes(nodes);

		const roundtrip = storage.queryAllNodes(snapshot.snapshotUid);
		expect(roundtrip).toHaveLength(8);

		const subtypes = roundtrip
			.map((n) => n.subtype)
			.filter((s): s is string => s !== null)
			.sort();

		// Eight distinct subtype strings expected. NAMESPACE is
		// shared across MODULE and BLOB contexts; in this test
		// it only appears on the BLOB node, so the sorted set has
		// 8 unique strings.
		expect(subtypes).toEqual([
			"BUCKET",
			"CACHE",
			"CONNECTION",
			"CONTAINER",
			"DIRECTORY_PATH",
			"FILE_PATH",
			"LOGICAL",
			"NAMESPACE",
		]);
	});

	it("NAMESPACE subtype context is carried by parent kind, not by subtype", () => {
		// Two nodes both with subtype=NAMESPACE but different
		// kind (MODULE vs BLOB). Both must round-trip with kind
		// and subtype preserved independently. Pins the "flat
		// enum, context-from-kind" design.
		const snapshot = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc123",
		});

		storage.insertNodes([
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:src/utils:MODULE",
				NodeKind.MODULE,
				NodeSubtype.NAMESPACE,
				"utils",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:blob:gcs:shared-ns:BLOB",
				NodeKind.BLOB,
				NodeSubtype.NAMESPACE,
				"shared-ns",
			),
		]);

		const roundtrip = storage.queryAllNodes(snapshot.snapshotUid);
		const moduleNode = roundtrip.find((n) => n.kind === "MODULE");
		const blobNode = roundtrip.find((n) => n.kind === "BLOB");

		expect(moduleNode).toBeDefined();
		expect(moduleNode?.subtype).toBe("NAMESPACE");
		expect(blobNode).toBeDefined();
		expect(blobNode?.subtype).toBe("NAMESPACE");
	});

	it("exposes the seven new NodeSubtype entries with contract-exact string values", () => {
		expect(NodeSubtype.CONNECTION).toBe("CONNECTION");
		expect(NodeSubtype.FILE_PATH).toBe("FILE_PATH");
		expect(NodeSubtype.DIRECTORY_PATH).toBe("DIRECTORY_PATH");
		expect(NodeSubtype.LOGICAL).toBe("LOGICAL");
		expect(NodeSubtype.CACHE).toBe("CACHE");
		expect(NodeSubtype.BUCKET).toBe("BUCKET");
		expect(NodeSubtype.CONTAINER).toBe("CONTAINER");

		// Pre-existing NAMESPACE unchanged; intentionally reused
		// for BLOB namespace context.
		expect(NodeSubtype.NAMESPACE).toBe("NAMESPACE");
	});
});
