/**
 * TypeScript half of the storage parity harness.
 *
 * Walks the shared `storage-parity-fixtures/` corpus at the repo
 * root, runs each fixture's `operations.json` through the TS
 * `SqliteStorage` adapter, and compares the captured results plus
 * post-state dump against the fixture's `expected.json`.
 *
 * This is the TS side of the R2-F cross-runtime parity check.
 * The Rust half (`rust/crates/storage/tests/parity.rs`) runs the
 * same fixtures through the Rust `StorageConnection` API and
 * compares against the same `expected.json`. If one side passes
 * and the other fails, the storage contract has drifted.
 *
 * Contract notes: see `storage-parity-fixtures/README.md` for
 * the fixture format, supported operations, symbolic binding
 * syntax (`@binding.field`), normalization rules, and canonical
 * ordering requirements. This file implements exactly what the
 * README specifies, and MUST stay byte-structurally equivalent
 * to the Rust harness in its output.
 */

import { randomUUID } from "node:crypto";
import { readFileSync, readdirSync, statSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SqliteConnectionProvider } from "../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../src/adapters/storage/sqlite/sqlite-storage.js";
import type {
	CreateSnapshotInput,
	RepoRef,
	UpdateSnapshotStatusInput,
} from "../../src/core/ports/storage.js";

// ── Fixture loading ──────────────────────────────────────────────

const FIXTURES_ROOT = join(__dirname, "..", "..", "storage-parity-fixtures");

interface Fixture {
	name: string;
	operationsDoc: unknown;
	expected: unknown;
}

function discoverFixtures(): Fixture[] {
	const fixtures: Fixture[] = [];
	for (const entry of readdirSync(FIXTURES_ROOT)) {
		const fullPath = join(FIXTURES_ROOT, entry);
		if (!statSync(fullPath).isDirectory()) continue;
		const opsPath = join(fullPath, "operations.json");
		const expPath = join(fullPath, "expected.json");
		try {
			statSync(opsPath);
			statSync(expPath);
		} catch {
			continue;
		}
		const operationsDoc = JSON.parse(readFileSync(opsPath, "utf-8"));
		const expected = JSON.parse(readFileSync(expPath, "utf-8"));
		fixtures.push({ name: entry, operationsDoc, expected });
	}
	fixtures.sort((a, b) => a.name.localeCompare(b.name));
	return fixtures;
}

// ── Harness state ────────────────────────────────────────────────

interface HarnessState {
	/** Binding-name → captured return value. */
	bindings: Record<string, unknown>;
	/** Dynamic-value substitutions: [actual, placeholder][]. */
	substitutions: [string, string][];
}

// ── Reference resolution ─────────────────────────────────────────

/**
 * Walk a JSON value and replace any string of the form
 * `@<binding>.<field>` with the corresponding field from the
 * bindings map.
 */
function resolveRefs(
	value: unknown,
	bindings: Record<string, unknown>,
): unknown {
	if (typeof value === "string") {
		if (value.startsWith("@")) {
			const rest = value.slice(1);
			const dotIdx = rest.indexOf(".");
			if (dotIdx < 0) {
				throw new Error(`bad ref syntax: ${value}`);
			}
			const bindingName = rest.slice(0, dotIdx);
			const field = rest.slice(dotIdx + 1);
			const binding = bindings[bindingName];
			if (binding === undefined) {
				throw new Error(`unknown binding: ${bindingName}`);
			}
			const resolved = (binding as Record<string, unknown>)[field];
			if (resolved === undefined) {
				throw new Error(
					`binding '${bindingName}' has no field '${field}'`,
				);
			}
			return resolved;
		}
		return value;
	}
	if (Array.isArray(value)) {
		return value.map((item) => resolveRefs(item, bindings));
	}
	if (typeof value === "object" && value !== null) {
		const out: Record<string, unknown> = {};
		for (const [k, v] of Object.entries(value)) {
			out[k] = resolveRefs(v, bindings);
		}
		return out;
	}
	return value;
}

// ── Operation dispatch ───────────────────────────────────────────

type Op = Record<string, unknown>;

function dispatchOp(
	storage: SqliteStorage,
	state: HarnessState,
	rawOp: Op,
): void {
	const opKind = rawOp.op as string;
	if (!opKind) {
		throw new Error("operation missing 'op' field");
	}

	// Resolve @binding.field references in the operation's argument fields.
	const resolved = resolveRefs(rawOp, state.bindings) as Op;
	const bindingName =
		typeof resolved.as === "string" ? (resolved.as as string) : undefined;

	const field = <T>(key: string): T => {
		const v = resolved[key];
		if (v === undefined) {
			throw new Error(`${opKind}: missing '${key}' field`);
		}
		return v as T;
	};

	switch (opKind) {
		case "addRepo": {
			const repo = field<Parameters<typeof storage.addRepo>[0]>("repo");
			storage.addRepo(repo);
			break;
		}

		case "getRepo": {
			const refArg = field<Record<string, string>>("ref");
			const ref: RepoRef =
				"uid" in refArg
					? { uid: refArg.uid }
					: "name" in refArg
						? { name: refArg.name }
						: { rootPath: refArg.rootPath };
			const result = storage.getRepo(ref);
			captureResult(state, bindingName, result);
			break;
		}

		case "listRepos": {
			const repos = storage.listRepos();
			captureResult(state, bindingName, repos);
			break;
		}

		case "removeRepo": {
			storage.removeRepo(field<string>("repoUid"));
			break;
		}

		case "createSnapshot": {
			const input = field<CreateSnapshotInput>("input");
			const snap = storage.createSnapshot(input);
			if (!bindingName) {
				throw new Error(
					"createSnapshot must have an 'as' binding so the generated " +
						"snapshot_uid and created_at can be normalized",
				);
			}
			// Register dynamic-value substitutions.
			state.substitutions.push([snap.snapshotUid, `<SNAP:${bindingName}>`]);
			state.substitutions.push([
				snap.createdAt,
				`<TS:${bindingName}:createdAt>`,
			]);
			state.bindings[bindingName] = JSON.parse(JSON.stringify(snap));
			break;
		}

		case "getSnapshot": {
			const uid = field<string>("snapshotUid");
			const result = storage.getSnapshot(uid);
			captureResult(state, bindingName, result);
			break;
		}

		case "getLatestSnapshot": {
			const repoUid = field<string>("repoUid");
			const result = storage.getLatestSnapshot(repoUid);
			captureResult(state, bindingName, result);
			break;
		}

		case "updateSnapshotStatus": {
			const input = field<UpdateSnapshotStatusInput>("input");
			if (input.completedAt === undefined) {
				throw new Error(
					"updateSnapshotStatus fixtures MUST provide explicit 'completedAt' " +
						"(the auto-generation path is unsupported in R2-F v1 because it " +
						"introduces a new dynamic timestamp the binding machinery cannot " +
						"capture)",
				);
			}
			storage.updateSnapshotStatus(input);
			break;
		}

		case "updateSnapshotCounts": {
			storage.updateSnapshotCounts(field<string>("snapshotUid"));
			break;
		}

		case "upsertFiles": {
			storage.upsertFiles(
				field<Parameters<typeof storage.upsertFiles>[0]>("files"),
			);
			break;
		}

		case "upsertFileVersions": {
			storage.upsertFileVersions(
				field<Parameters<typeof storage.upsertFileVersions>[0]>(
					"fileVersions",
				),
			);
			break;
		}

		case "getFilesByRepo": {
			const files = storage.getFilesByRepo(field<string>("repoUid"));
			captureResult(state, bindingName, files);
			break;
		}

		case "getStaleFiles": {
			const files = storage.getStaleFiles(field<string>("snapshotUid"));
			captureResult(state, bindingName, files);
			break;
		}

		case "queryFileVersionHashes": {
			const hashes = storage.queryFileVersionHashes(field<string>("snapshotUid"));
			if (bindingName) {
				// Map → plain object with sorted keys for determinism.
				const sorted = [...hashes.entries()].sort(([a], [b]) => a.localeCompare(b));
				const obj: Record<string, string> = {};
				for (const [k, v] of sorted) {
					obj[k] = v;
				}
				state.bindings[bindingName] = obj;
			}
			break;
		}

		case "insertNodes": {
			storage.insertNodes(
				field<Parameters<typeof storage.insertNodes>[0]>("nodes"),
			);
			break;
		}

		case "queryAllNodes": {
			const nodes = storage.queryAllNodes(field<string>("snapshotUid"));
			// Sort by nodeUid for deterministic result ordering (mirror Rust).
			const sorted = [...nodes].sort((a, b) =>
				a.nodeUid.localeCompare(b.nodeUid),
			);
			captureResult(state, bindingName, sorted);
			break;
		}

		case "deleteNodesByFile": {
			storage.deleteNodesByFile(
				field<string>("snapshotUid"),
				field<string>("fileUid"),
			);
			break;
		}

		case "insertEdges": {
			storage.insertEdges(
				field<Parameters<typeof storage.insertEdges>[0]>("edges"),
			);
			break;
		}

		case "deleteEdgesByUids": {
			storage.deleteEdgesByUids(field<string[]>("edgeUids"));
			break;
		}

		default:
			throw new Error(`unknown operation: ${opKind}`);
	}
}

function captureResult(
	state: HarnessState,
	bindingName: string | undefined,
	value: unknown,
): void {
	if (bindingName !== undefined) {
		// JSON round-trip strips any class semantics and gives plain JSON.
		state.bindings[bindingName] =
			value === null || value === undefined
				? null
				: JSON.parse(JSON.stringify(value));
	}
}

// ── State dump ───────────────────────────────────────────────────

interface Database {
	prepare: (sql: string) => {
		all: (...params: unknown[]) => unknown[];
		get: (...params: unknown[]) => unknown;
	};
}

interface SchemaDump {
	tables: Record<string, unknown[]>;
	indexes: string[];
}

function dumpState(db: Database): { schema: SchemaDump; tables: Record<string, unknown[]> } {
	return {
		schema: dumpSchema(db),
		tables: dumpTables(db),
	};
}

/**
 * Dump the logical schema as canonical column-shape data, NOT
 * the raw `sqlite_master.sql` text. See the Rust harness's
 * `dump_schema` for the full rationale (the short version: Rust
 * embeds an outdated .sql and adds columns via ALTER TABLE,
 * which makes `sqlite_master.sql` text-different from TS even
 * when the logical schema is identical).
 */
function dumpSchema(db: Database): SchemaDump {
	const tableNames = (
		db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
			)
			.all() as { name: string }[]
	).map((r) => r.name);

	const tablesOut: Record<string, unknown[]> = {};
	for (const tableName of tableNames) {
		const rows = db.prepare(`PRAGMA table_info("${tableName}")`).all() as {
			name: string;
			type: string;
			notnull: number;
			dflt_value: string | null;
			pk: number;
		}[];
		const cols = rows.map((r) => ({
			name: r.name,
			type: r.type,
			notnull: r.notnull !== 0,
			dflt_value: r.dflt_value,
			pk: r.pk,
		}));
		// Sort columns by name for canonical ordering.
		cols.sort((a, b) => a.name.localeCompare(b.name));
		tablesOut[tableName] = cols;
	}

	const indexNames = (
		db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%' ORDER BY name",
			)
			.all() as { name: string }[]
	).map((r) => r.name);

	return {
		tables: tablesOut,
		indexes: indexNames,
	};
}

function dumpTables(db: Database): Record<string, unknown[]> {
	const tableNames = (
		db
			.prepare(
				"SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
			)
			.all() as { name: string }[]
	).map((r) => r.name);

	const out: Record<string, unknown[]> = {};
	for (const tableName of tableNames) {
		const sortKey = sortKeyFor(tableName);
		const sql = `SELECT * FROM "${tableName}" ORDER BY ${sortKey}`;
		const rows = db.prepare(sql).all() as Record<string, unknown>[];
		if (rows.length > 0) {
			out[tableName] = rows;
		}
	}
	return out;
}

function sortKeyFor(table: string): string {
	switch (table) {
		case "repos":
			return "repo_uid";
		case "snapshots":
			return "snapshot_uid";
		case "files":
			return "file_uid";
		case "file_versions":
			return "snapshot_uid, file_uid";
		case "nodes":
			return "node_uid";
		case "edges":
			return "edge_uid";
		case "schema_migrations":
			return "version";
		default:
			return "rowid";
	}
}

// ── Normalization ────────────────────────────────────────────────

/**
 * Apply normalization rules to the dump + results in place.
 * See `storage-parity-fixtures/README.md` → "Normalization rules".
 */
function normalize(
	dump: { schema: unknown[]; tables: Record<string, unknown[]> },
	results: Record<string, unknown>,
	substitutions: [string, string][],
): void {
	// 1. Blanket column normalization:
	//    schema_migrations.applied_at → "<TIMESTAMP>"
	const sm = dump.tables.schema_migrations;
	if (Array.isArray(sm)) {
		for (const row of sm) {
			if (row && typeof row === "object") {
				(row as Record<string, unknown>).applied_at = "<TIMESTAMP>";
			}
		}
	}

	// 2. Binding-based substitutions: walk every string in both
	//    the dump and the results, replace matching literal values
	//    with their placeholders.
	applySubstitutions(dump, substitutions);
	applySubstitutions(results, substitutions);
}

function applySubstitutions(value: unknown, subs: [string, string][]): void {
	if (typeof value === "string") {
		// Strings are primitives; can't mutate in place. Caller
		// must handle substitution at the parent level. This
		// function handles recursion into objects and arrays.
		return;
	}
	if (Array.isArray(value)) {
		for (let i = 0; i < value.length; i++) {
			const item = value[i];
			if (typeof item === "string") {
				for (const [actual, placeholder] of subs) {
					if (item === actual) {
						value[i] = placeholder;
						break;
					}
				}
			} else {
				applySubstitutions(item, subs);
			}
		}
		return;
	}
	if (value && typeof value === "object") {
		const obj = value as Record<string, unknown>;
		for (const [k, v] of Object.entries(obj)) {
			if (typeof v === "string") {
				for (const [actual, placeholder] of subs) {
					if (v === actual) {
						obj[k] = placeholder;
						break;
					}
				}
			} else {
				applySubstitutions(v, subs);
			}
		}
	}
}

// ── Harness ──────────────────────────────────────────────────────

const fixtures = discoverFixtures();

describe("storage parity — TS half against shared storage-parity-fixtures corpus", () => {
	let dbPath: string;
	let provider: SqliteConnectionProvider;
	let storage: SqliteStorage;

	beforeEach(() => {
		// Use a unique temp path for each test. SqliteConnectionProvider
		// does not directly support in-memory mode, so we use a
		// temp file and clean up after.
		dbPath = join(tmpdir(), `rgr-storage-parity-${randomUUID()}.db`);
		provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		storage = new SqliteStorage(provider.getDatabase());
	});

	afterEach(() => {
		provider.close();
		try {
			unlinkSync(dbPath);
		} catch {
			// ignore
		}
	});

	it("discovered at least one fixture", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	for (const fixture of fixtures) {
		it(`fixture: ${fixture.name}`, () => {
			const state: HarnessState = {
				bindings: {},
				substitutions: [],
			};

			const opsDoc = fixture.operationsDoc as { operations: Op[] };
			for (let i = 0; i < opsDoc.operations.length; i++) {
				try {
					dispatchOp(storage, state, opsDoc.operations[i]);
				} catch (err) {
					throw new Error(
						`op[${i}]: ${err instanceof Error ? err.message : String(err)}`,
					);
				}
			}

			const dump = dumpState(provider.getDatabase() as unknown as Database);
			const results = state.bindings;
			normalize(dump, results, state.substitutions);

			const actual = {
				results,
				schema: dump.schema,
				tables: dump.tables,
			};

			expect(actual).toEqual(fixture.expected);
		});
	}
});
