/**
 * SB-2-pre tolerance proof (TS half).
 *
 * Asserts that the TS storage adapter accepts insert + read of
 * nodes whose `kind` column matches the three canonical strings
 * added by SB-2-pre: "DB_RESOURCE", "FS_PATH", "BLOB".
 *
 * This is part of the canonical-vocabulary-alignment slice for
 * the state-boundary program. It does NOT exercise state-
 * extractor or any emission pipeline. The test's only job is to
 * prove the TS storage layer tolerates the new vocabulary.
 *
 * Pair test:
 *   `rust/crates/storage/tests/state_boundary_kinds_tolerance.rs`
 * exercises the same vocabulary through the Rust storage layer.
 *
 * Cross-runtime tolerance is transitive: both adapters share the
 * same SQL schema (migration 001 is a single embedded file
 * consumed by both), and neither validates `nodes.kind` against
 * the typed enum at the SQL boundary (confirmed by inspection of
 * `mapNode` at `sqlite-storage.ts:3290`, which uses a bare
 * TypeScript `as` assertion with no runtime validation). A DB
 * written by one therefore reads correctly through the other for
 * these kinds.
 *
 * Full end-to-end cross-runtime proof (Rust emits new-kind nodes
 * via state-extractor → TS reads them) is deferred to SB-2/SB-3
 * when the emission pipeline actually exists.
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
	SnapshotKind,
} from "../../../src/core/model/index.js";

let storage: SqliteStorage;
let provider: SqliteConnectionProvider;
let dbPath: string;

const REPO_UID = "myservice";

beforeEach(() => {
	dbPath = join(tmpdir(), `sb-2-pre-tolerance-${randomUUID()}.db`);
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
		// Cleanup best-effort; tmpdir gets cleaned eventually.
	}
});

function makeResourceNode(
	snapshotUid: string,
	stableKey: string,
	kind: GraphNode["kind"],
	subtype: string,
	name: string,
): GraphNode {
	return {
		nodeUid: randomUUID(),
		snapshotUid,
		repoUid: REPO_UID,
		stableKey,
		kind,
		subtype: subtype as GraphNode["subtype"],
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

describe("SB-2-pre — TS storage tolerates DB_RESOURCE / FS_PATH / BLOB node kinds", () => {
	it("round-trips all three new node kinds", () => {
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
				"CONNECTION",
				"DATABASE_URL",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:fs:/etc/app/settings.yaml:FS_PATH",
				NodeKind.FS_PATH,
				"FILE_PATH",
				"/etc/app/settings.yaml",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				"myservice:blob:s3:artifacts-bucket:BLOB",
				NodeKind.BLOB,
				"BUCKET",
				"artifacts-bucket",
			),
		];

		storage.insertNodes(nodes);

		const roundtrip = storage.queryAllNodes(snapshot.snapshotUid);
		expect(roundtrip).toHaveLength(3);

		const kinds = roundtrip.map((n) => n.kind).sort();
		expect(kinds).toEqual(["BLOB", "DB_RESOURCE", "FS_PATH"]);
	});

	it("preserves stable-key payload byte-for-byte, including FS keys with interior colons", () => {
		// Contract §5.1 parsing-semantics note: FS stable-key
		// payloads may contain `:` (Windows drive letters, URI
		// references). Round-trip must preserve them exactly.
		const snapshot = storage.createSnapshot({
			repoUid: REPO_UID,
			kind: SnapshotKind.FULL,
			basisCommit: "abc123",
		});

		const windowsKey = "myservice:fs:C:\\Windows\\path:FS_PATH";
		const uriKey = "myservice:fs:file:///etc/config:FS_PATH";

		storage.insertNodes([
			makeResourceNode(
				snapshot.snapshotUid,
				windowsKey,
				NodeKind.FS_PATH,
				"FILE_PATH",
				"C:\\Windows\\path",
			),
			makeResourceNode(
				snapshot.snapshotUid,
				uriKey,
				NodeKind.FS_PATH,
				"FILE_PATH",
				"file:///etc/config",
			),
		]);

		const roundtrip = storage.queryAllNodes(snapshot.snapshotUid);
		const keys = roundtrip.map((n) => n.stableKey).sort();
		expect(keys).toContain(windowsKey);
		expect(keys).toContain(uriKey);
	});

	it("exposes the three new NodeKind enum entries with contract-exact string values", () => {
		// The TS enum entries are the product-facing vocabulary
		// for these kinds. Pin the exact string values so any
		// rename drift (by a future refactor) breaks here rather
		// than silently at a far-away call site.
		expect(NodeKind.DB_RESOURCE).toBe("DB_RESOURCE");
		expect(NodeKind.FS_PATH).toBe("FS_PATH");
		expect(NodeKind.BLOB).toBe("BLOB");
	});
});
