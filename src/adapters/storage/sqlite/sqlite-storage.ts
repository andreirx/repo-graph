import Database from "better-sqlite3";
import { v4 as uuidv4 } from "uuid";
import type {
	CycleResult,
	DeadNodeResult,
	Declaration,
	FileVersion,
	GraphEdge,
	GraphNode,
	NodeResult,
	PathResult,
	PathStep,
	Repo,
	Snapshot,
	TrackedFile,
} from "../../../core/model/index.js";
import { EdgeType, SnapshotStatus } from "../../../core/model/index.js";
import type {
	CreateSnapshotInput,
	FindCyclesInput,
	FindDeadNodesInput,
	FindImportsBetweenPathsInput,
	FindImportsInput,
	FindPathInput,
	FindTraversalInput,
	GetDeclarationsInput,
	ImportEdgeResult,
	RepoRef,
	ResolveSymbolInput,
	StoragePort,
	UpdateSnapshotStatusInput,
} from "../../../core/ports/storage.js";
import { INITIAL_MIGRATION } from "./migrations/001-initial.js";
import { runMigration002 } from "./migrations/002-provenance-columns.js";

export class SqliteStorage implements StoragePort {
	private db: Database.Database;

	constructor(dbPath: string) {
		this.db = new Database(dbPath);
	}

	// ── Lifecycle ──────────────────────────────────────────────────────

	initialize(): void {
		this.db.pragma("journal_mode = WAL");
		this.db.pragma("foreign_keys = ON");

		// Migration 001: initial schema (CREATE TABLE IF NOT EXISTS — safe to replay)
		const statements = INITIAL_MIGRATION.split(";")
			.map((s) => s.trim())
			.filter((s) => s.length > 0 && !s.startsWith("PRAGMA"));

		const runInitial = this.db.transaction(() => {
			for (const stmt of statements) {
				this.db.exec(`${stmt};`);
			}
		});
		runInitial();

		// Migration 002+: incremental migrations.
		// Each checks schema_migrations and column existence before acting.
		const applied = this.db
			.prepare("SELECT version FROM schema_migrations WHERE version = 2")
			.get();
		if (!applied) {
			const runIncremental = this.db.transaction(() => {
				runMigration002(this.db);
			});
			runIncremental();
		}
	}

	close(): void {
		this.db.close();
	}

	// ── Repos ──────────────────────────────────────────────────────────

	addRepo(repo: Repo): void {
		this.db
			.prepare(
				`INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?)`,
			)
			.run(
				repo.repoUid,
				repo.name,
				repo.rootPath,
				repo.defaultBranch,
				repo.createdAt,
				repo.metadataJson,
			);
	}

	getRepo(ref: RepoRef): Repo | null {
		let row: Record<string, unknown> | undefined;
		if ("uid" in ref) {
			row = this.db
				.prepare("SELECT * FROM repos WHERE repo_uid = ?")
				.get(ref.uid) as Record<string, unknown> | undefined;
		} else if ("name" in ref) {
			row = this.db
				.prepare("SELECT * FROM repos WHERE name = ?")
				.get(ref.name) as Record<string, unknown> | undefined;
		} else {
			row = this.db
				.prepare("SELECT * FROM repos WHERE root_path = ?")
				.get(ref.rootPath) as Record<string, unknown> | undefined;
		}
		return row ? this.mapRepo(row) : null;
	}

	listRepos(): Repo[] {
		const rows = this.db
			.prepare("SELECT * FROM repos ORDER BY name")
			.all() as Record<string, unknown>[];
		return rows.map((r) => this.mapRepo(r));
	}

	removeRepo(repoUid: string): void {
		this.db.prepare("DELETE FROM repos WHERE repo_uid = ?").run(repoUid);
	}

	// ── Snapshots ──────────────────────────────────────────────────────

	createSnapshot(input: CreateSnapshotInput): Snapshot {
		const uid = `${input.repoUid}/${new Date().toISOString()}/${uuidv4().slice(0, 8)}`;
		const now = new Date().toISOString();
		this.db
			.prepare(
				`INSERT INTO snapshots
			 (snapshot_uid, repo_uid, parent_snapshot_uid, kind, basis_ref, basis_commit, status, created_at, label, toolchain_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			)
			.run(
				uid,
				input.repoUid,
				input.parentSnapshotUid ?? null,
				input.kind,
				input.basisRef ?? null,
				input.basisCommit ?? null,
				SnapshotStatus.BUILDING,
				now,
				input.label ?? null,
				input.toolchainJson ?? null,
			);
		const snapshot = this.getSnapshot(uid);
		if (!snapshot) {
			throw new Error(`Failed to read back snapshot ${uid} after insert`);
		}
		return snapshot;
	}

	getSnapshot(snapshotUid: string): Snapshot | null {
		const row = this.db
			.prepare("SELECT * FROM snapshots WHERE snapshot_uid = ?")
			.get(snapshotUid) as Record<string, unknown> | undefined;
		return row ? this.mapSnapshot(row) : null;
	}

	getLatestSnapshot(repoUid: string): Snapshot | null {
		const row = this.db
			.prepare(
				`SELECT * FROM snapshots
			 WHERE repo_uid = ? AND status = ?
			 ORDER BY created_at DESC LIMIT 1`,
			)
			.get(repoUid, SnapshotStatus.READY) as
			| Record<string, unknown>
			| undefined;
		return row ? this.mapSnapshot(row) : null;
	}

	updateSnapshotStatus(input: UpdateSnapshotStatusInput): void {
		this.db
			.prepare(
				`UPDATE snapshots SET status = ?, completed_at = ?
			 WHERE snapshot_uid = ?`,
			)
			.run(
				input.status,
				input.completedAt ?? new Date().toISOString(),
				input.snapshotUid,
			);
	}

	updateSnapshotCounts(snapshotUid: string): void {
		this.db
			.prepare(
				`UPDATE snapshots SET
			   files_total = (SELECT COUNT(*) FROM file_versions WHERE snapshot_uid = ?),
			   nodes_total = (SELECT COUNT(*) FROM nodes WHERE snapshot_uid = ?),
			   edges_total = (SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?)
			 WHERE snapshot_uid = ?`,
			)
			.run(snapshotUid, snapshotUid, snapshotUid, snapshotUid);
	}

	// ── Files ──────────────────────────────────────────────────────────

	upsertFiles(files: TrackedFile[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO files (file_uid, repo_uid, path, language, is_test, is_generated, is_excluded)
			 VALUES (?, ?, ?, ?, ?, ?, ?)
			 ON CONFLICT(file_uid) DO UPDATE SET
			   language = excluded.language,
			   is_test = excluded.is_test,
			   is_generated = excluded.is_generated,
			   is_excluded = excluded.is_excluded`,
		);
		const upsertAll = this.db.transaction(() => {
			for (const f of files) {
				stmt.run(
					f.fileUid,
					f.repoUid,
					f.path,
					f.language,
					f.isTest ? 1 : 0,
					f.isGenerated ? 1 : 0,
					f.isExcluded ? 1 : 0,
				);
			}
		});
		upsertAll();
	}

	upsertFileVersions(fileVersions: FileVersion[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO file_versions
			 (snapshot_uid, file_uid, content_hash, ast_hash, extractor, parse_status, size_bytes, line_count, indexed_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			 ON CONFLICT(snapshot_uid, file_uid) DO UPDATE SET
			   content_hash = excluded.content_hash,
			   ast_hash = excluded.ast_hash,
			   extractor = excluded.extractor,
			   parse_status = excluded.parse_status,
			   size_bytes = excluded.size_bytes,
			   line_count = excluded.line_count,
			   indexed_at = excluded.indexed_at`,
		);
		const upsertAll = this.db.transaction(() => {
			for (const fv of fileVersions) {
				stmt.run(
					fv.snapshotUid,
					fv.fileUid,
					fv.contentHash,
					fv.astHash,
					fv.extractor,
					fv.parseStatus,
					fv.sizeBytes,
					fv.lineCount,
					fv.indexedAt,
				);
			}
		});
		upsertAll();
	}

	getFilesByRepo(repoUid: string): TrackedFile[] {
		const rows = this.db
			.prepare(
				"SELECT * FROM files WHERE repo_uid = ? AND is_excluded = 0 ORDER BY path",
			)
			.all(repoUid) as Record<string, unknown>[];
		return rows.map((r) => this.mapFile(r));
	}

	getStaleFiles(snapshotUid: string): TrackedFile[] {
		const rows = this.db
			.prepare(
				`SELECT f.* FROM files f
			 JOIN file_versions fv ON f.file_uid = fv.file_uid
			 WHERE fv.snapshot_uid = ? AND fv.parse_status = 'stale'`,
			)
			.all(snapshotUid) as Record<string, unknown>[];
		return rows.map((r) => this.mapFile(r));
	}

	// ── Nodes & Edges ──────────────────────────────────────────────────

	insertNodes(nodes: GraphNode[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO nodes
			 (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name,
			  qualified_name, file_uid, parent_node_uid,
			  line_start, col_start, line_end, col_end,
			  signature, visibility, doc_comment, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const n of nodes) {
				stmt.run(
					n.nodeUid,
					n.snapshotUid,
					n.repoUid,
					n.stableKey,
					n.kind,
					n.subtype,
					n.name,
					n.qualifiedName,
					n.fileUid,
					n.parentNodeUid,
					n.location?.lineStart ?? null,
					n.location?.colStart ?? null,
					n.location?.lineEnd ?? null,
					n.location?.colEnd ?? null,
					n.signature,
					n.visibility,
					n.docComment,
					n.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertEdges(edges: GraphEdge[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO edges
			 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid,
			  type, resolution, extractor,
			  line_start, col_start, line_end, col_end, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of edges) {
				stmt.run(
					e.edgeUid,
					e.snapshotUid,
					e.repoUid,
					e.sourceNodeUid,
					e.targetNodeUid,
					e.type,
					e.resolution,
					e.extractor,
					e.location?.lineStart ?? null,
					e.location?.colStart ?? null,
					e.location?.lineEnd ?? null,
					e.location?.colEnd ?? null,
					e.metadataJson,
				);
			}
		});
		insertAll();
	}

	deleteNodesByFile(snapshotUid: string, fileUid: string): void {
		const deleteOps = this.db.transaction(() => {
			// Delete edges where source or target is in the file's nodes
			this.db
				.prepare(
					`DELETE FROM edges WHERE snapshot_uid = ? AND (
				   source_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND file_uid = ?)
				   OR target_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND file_uid = ?)
				 )`,
				)
				.run(snapshotUid, snapshotUid, fileUid, snapshotUid, fileUid);

			// Delete the nodes themselves
			this.db
				.prepare("DELETE FROM nodes WHERE snapshot_uid = ? AND file_uid = ?")
				.run(snapshotUid, fileUid);
		});
		deleteOps();
	}

	// ── Declarations ───────────────────────────────────────────────────

	insertDeclaration(declaration: Declaration): void {
		this.db
			.prepare(
				`INSERT INTO declarations
			 (declaration_uid, repo_uid, snapshot_uid, target_stable_key, kind,
			  value_json, created_at, created_by, supersedes_uid, is_active, authored_basis_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			)
			.run(
				declaration.declarationUid,
				declaration.repoUid,
				declaration.snapshotUid,
				declaration.targetStableKey,
				declaration.kind,
				declaration.valueJson,
				declaration.createdAt,
				declaration.createdBy,
				declaration.supersedesUid,
				declaration.isActive ? 1 : 0,
				declaration.authoredBasisJson ?? null,
			);
	}

	getActiveDeclarations(input: GetDeclarationsInput): Declaration[] {
		let sql = "SELECT * FROM declarations WHERE repo_uid = ? AND is_active = 1";
		const params: unknown[] = [input.repoUid];

		if (input.kind) {
			sql += " AND kind = ?";
			params.push(input.kind);
		}
		if (input.targetStableKey) {
			sql += " AND target_stable_key = ?";
			params.push(input.targetStableKey);
		}
		sql += " ORDER BY created_at DESC";

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((r) => this.mapDeclaration(r));
	}

	deactivateDeclaration(declarationUid: string): void {
		this.db
			.prepare(
				"UPDATE declarations SET is_active = 0 WHERE declaration_uid = ?",
			)
			.run(declarationUid);
	}

	// ── Graph Queries ──────────────────────────────────────────────────

	getNodeByStableKey(snapshotUid: string, stableKey: string): GraphNode | null {
		const row = this.db
			.prepare("SELECT * FROM nodes WHERE snapshot_uid = ? AND stable_key = ?")
			.get(snapshotUid, stableKey) as Record<string, unknown> | undefined;
		return row ? this.mapNode(row) : null;
	}

	resolveSymbol(input: ResolveSymbolInput): GraphNode[] {
		const limit = input.limit ?? 20;
		const pattern = `%${input.query}%`;
		const rows = this.db
			.prepare(
				`SELECT * FROM nodes
			 WHERE snapshot_uid = ? AND kind = 'SYMBOL'
			   AND (name LIKE ? OR qualified_name LIKE ? OR stable_key LIKE ?)
			 ORDER BY
			   CASE WHEN name = ? THEN 0
			        WHEN qualified_name = ? THEN 1
			        WHEN name LIKE ? THEN 2
			        ELSE 3
			   END
			 LIMIT ?`,
			)
			.all(
				input.snapshotUid,
				pattern,
				pattern,
				pattern,
				input.query,
				input.query,
				`${input.query}%`,
				limit,
			) as Record<string, unknown>[];
		return rows.map((r) => this.mapNode(r));
	}

	findCallers(input: FindTraversalInput): NodeResult[] {
		const maxDepth = input.maxDepth ?? 1;
		const edgeTypes = input.edgeTypes ?? [EdgeType.CALLS];
		const typePlaceholders = edgeTypes.map(() => "?").join(", ");

		// The CTE carries the traversed edge_uid so the final join
		// picks the exact edge that was walked, not any random edge
		// touching the node.
		const sql = `
			WITH RECURSIVE traversal(node_uid, traversed_edge_uid, depth, path) AS (
				SELECT e.source_node_uid, e.edge_uid, 1,
				       e.source_node_uid || ' -> ' || e.target_node_uid
				FROM edges e
				JOIN nodes target_n ON e.target_node_uid = target_n.node_uid
				WHERE target_n.snapshot_uid = ?
				  AND target_n.stable_key = ?
				  AND e.snapshot_uid = ?
				  AND e.type IN (${typePlaceholders})

				UNION ALL

				SELECT e.source_node_uid, e.edge_uid, t.depth + 1,
				       e.source_node_uid || ' -> ' || t.path
				FROM edges e
				JOIN traversal t ON e.target_node_uid = t.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type IN (${typePlaceholders})
				  AND t.depth < ?
				  AND t.path NOT LIKE '%' || e.source_node_uid || '%'
			)
			SELECT DISTINCT
				n.node_uid, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e_info.type AS edge_type, e_info.resolution, e_info.extractor,
				t.depth
			FROM traversal t
			JOIN nodes n ON t.node_uid = n.node_uid
			LEFT JOIN files f ON n.file_uid = f.file_uid
			LEFT JOIN edges e_info ON e_info.edge_uid = t.traversed_edge_uid
			ORDER BY t.depth ASC, n.name ASC
		`;

		const params = [
			input.snapshotUid,
			input.stableKey,
			input.snapshotUid,
			...edgeTypes,
			input.snapshotUid,
			...edgeTypes,
			maxDepth,
		];

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((r) => this.mapNodeResult(r));
	}

	findCallees(input: FindTraversalInput): NodeResult[] {
		const maxDepth = input.maxDepth ?? 1;
		const edgeTypes = input.edgeTypes ?? [EdgeType.CALLS];
		const typePlaceholders = edgeTypes.map(() => "?").join(", ");

		const sql = `
			WITH RECURSIVE traversal(node_uid, traversed_edge_uid, depth, path) AS (
				SELECT e.target_node_uid, e.edge_uid, 1,
				       e.source_node_uid || ' -> ' || e.target_node_uid
				FROM edges e
				JOIN nodes source_n ON e.source_node_uid = source_n.node_uid
				WHERE source_n.snapshot_uid = ?
				  AND source_n.stable_key = ?
				  AND e.snapshot_uid = ?
				  AND e.type IN (${typePlaceholders})

				UNION ALL

				SELECT e.target_node_uid, e.edge_uid, t.depth + 1,
				       t.path || ' -> ' || e.target_node_uid
				FROM edges e
				JOIN traversal t ON e.source_node_uid = t.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type IN (${typePlaceholders})
				  AND t.depth < ?
				  AND t.path NOT LIKE '%' || e.target_node_uid || '%'
			)
			SELECT DISTINCT
				n.node_uid, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e_info.type AS edge_type, e_info.resolution, e_info.extractor,
				t.depth
			FROM traversal t
			JOIN nodes n ON t.node_uid = n.node_uid
			LEFT JOIN files f ON n.file_uid = f.file_uid
			LEFT JOIN edges e_info ON e_info.edge_uid = t.traversed_edge_uid
			ORDER BY t.depth ASC, n.name ASC
		`;

		const params = [
			input.snapshotUid,
			input.stableKey,
			input.snapshotUid,
			...edgeTypes,
			input.snapshotUid,
			...edgeTypes,
			maxDepth,
		];

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((r) => this.mapNodeResult(r));
	}

	findImports(input: FindImportsInput): NodeResult[] {
		return this.findCallees({
			snapshotUid: input.snapshotUid,
			stableKey: input.stableKey,
			maxDepth: input.maxDepth ?? 1,
			edgeTypes: [EdgeType.IMPORTS],
		});
	}

	findPath(input: FindPathInput): PathResult {
		const maxDepth = input.maxDepth ?? 8;
		const edgeTypes = input.edgeTypes ?? [EdgeType.CALLS, EdgeType.IMPORTS];
		const typePlaceholders = edgeTypes.map(() => "?").join(", ");

		// Resolve start and end node UIDs
		const fromNode = this.getNodeByStableKey(
			input.snapshotUid,
			input.fromStableKey,
		);
		const toNode = this.getNodeByStableKey(
			input.snapshotUid,
			input.toStableKey,
		);

		if (!fromNode || !toNode) {
			return { found: false, pathLength: 0, steps: [] };
		}

		const sql = `
			WITH RECURSIVE path_search(node_uid, depth, path_uids, path_edges) AS (
				SELECT ?, 0, ?, ''

				UNION ALL

				SELECT e.target_node_uid, p.depth + 1,
				       p.path_uids || ',' || e.target_node_uid,
				       p.path_edges || CASE WHEN p.path_edges = '' THEN '' ELSE '|' END
				         || e.source_node_uid || ':' || e.type || ':' || e.target_node_uid
				FROM edges e
				JOIN path_search p ON e.source_node_uid = p.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type IN (${typePlaceholders})
				  AND p.depth < ?
				  AND p.path_uids NOT LIKE '%' || e.target_node_uid || '%'
			)
			SELECT path_uids, path_edges, depth
			FROM path_search
			WHERE node_uid = ?
			ORDER BY depth ASC
			LIMIT 1
		`;

		const params = [
			fromNode.nodeUid,
			fromNode.nodeUid,
			input.snapshotUid,
			...edgeTypes,
			maxDepth,
			toNode.nodeUid,
		];

		const row = this.db.prepare(sql).get(...params) as
			| Record<string, unknown>
			| undefined;

		if (!row) {
			return { found: false, pathLength: 0, steps: [] };
		}

		// Parse the path and build steps
		const pathUids = (row.path_uids as string).split(",").filter(Boolean);
		const steps: PathStep[] = [];

		for (const uid of pathUids) {
			const node = this.db
				.prepare(
					`SELECT n.*, f.path AS file_path FROM nodes n
				 LEFT JOIN files f ON n.file_uid = f.file_uid
				 WHERE n.node_uid = ?`,
				)
				.get(uid) as Record<string, unknown> | undefined;
			if (node) {
				steps.push({
					nodeUid: node.node_uid as string,
					symbol: (node.qualified_name as string) ?? (node.name as string),
					file: (node.file_path as string) ?? "",
					line: (node.line_start as number) ?? null,
					edgeType: "", // filled below
				});
			}
		}

		// Fill edge types from the path_edges string
		const edgeSegments = (row.path_edges as string).split("|").filter(Boolean);
		for (let i = 0; i < edgeSegments.length && i + 1 < steps.length; i++) {
			const parts = edgeSegments[i].split(":");
			if (parts.length >= 2) {
				steps[i + 1].edgeType = parts[1];
			}
		}

		return {
			found: true,
			pathLength: row.depth as number,
			steps,
		};
	}

	findDeadNodes(input: FindDeadNodesInput): DeadNodeResult[] {
		let kindFilter = "";
		const params: unknown[] = [input.snapshotUid, input.snapshotUid];

		if (input.kind) {
			kindFilter = " AND n.kind = ?";
			params.push(input.kind);
		}

		// Get repo_uid from snapshot for declaration lookup
		const snapshot = this.getSnapshot(input.snapshotUid);
		if (!snapshot) return [];
		params.push(snapshot.repoUid, input.snapshotUid);

		let minLinesFilter = "";
		if (input.minLines) {
			minLinesFilter = " AND (n.line_end - n.line_start + 1) >= ?";
			params.push(input.minLines);
		}

		// Entrypoint declarations apply if:
		//   - repo-wide (snapshot_uid IS NULL), OR
		//   - scoped to this specific snapshot
		const sql = `
			SELECT
				n.node_uid, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start,
				CASE WHEN n.line_end IS NOT NULL AND n.line_start IS NOT NULL
				     THEN n.line_end - n.line_start + 1
				     ELSE NULL
				END AS line_count
			FROM nodes n
			LEFT JOIN files f ON n.file_uid = f.file_uid
			WHERE n.snapshot_uid = ?
			  AND n.node_uid NOT IN (
				SELECT e.target_node_uid FROM edges e
				WHERE e.snapshot_uid = ?
				  AND e.type IN ('IMPORTS', 'CALLS', 'IMPLEMENTS', 'INSTANTIATES',
				                 'ROUTES_TO', 'REGISTERED_BY', 'TESTED_BY', 'COVERS')
			  )
			  ${kindFilter}
			  AND n.stable_key NOT IN (
				SELECT d.target_stable_key FROM declarations d
				WHERE d.repo_uid = ?
				  AND d.kind = 'entrypoint'
				  AND d.is_active = 1
				  AND (d.snapshot_uid IS NULL OR d.snapshot_uid = ?)
			  )
			  ${minLinesFilter}
			ORDER BY n.name ASC
		`;

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((r) => ({
			nodeUid: r.node_uid as string,
			symbol: (r.qualified_name as string) ?? (r.name as string),
			kind: r.kind as string,
			subtype: (r.subtype as string) ?? null,
			file: (r.file_path as string) ?? "",
			line: (r.line_start as number) ?? null,
			lineCount: (r.line_count as number) ?? null,
		}));
	}

	findCycles(input: FindCyclesInput): CycleResult[] {
		const level = input.level ?? "module";
		const nodeKind = level === "module" ? "MODULE" : "FILE";
		const edgeType = EdgeType.IMPORTS;

		const sql = `
			WITH RECURSIVE cycle_search(start_uid, current_uid, path, is_cycle) AS (
				SELECT n.node_uid, n.node_uid, n.node_uid, 0
				FROM nodes n
				WHERE n.snapshot_uid = ? AND n.kind = ?

				UNION ALL

				SELECT cs.start_uid, e.target_node_uid,
				       cs.path || ' -> ' || e.target_node_uid,
				       CASE WHEN e.target_node_uid = cs.start_uid THEN 1 ELSE 0 END
				FROM edges e
				JOIN cycle_search cs ON e.source_node_uid = cs.current_uid
				WHERE e.snapshot_uid = ?
				  AND e.type = ?
				  AND cs.is_cycle = 0
				  AND (cs.path NOT LIKE '%' || e.target_node_uid || '%'
				       OR e.target_node_uid = cs.start_uid)
			)
			SELECT DISTINCT path FROM cycle_search
			WHERE is_cycle = 1
			ORDER BY path
		`;

		const rows = this.db
			.prepare(sql)
			.all(input.snapshotUid, nodeKind, input.snapshotUid, edgeType) as Record<
			string,
			unknown
		>[];

		// The CTE finds the same logical cycle from every starting node,
		// producing rotated variants (A->B->C->A, B->C->A->B, C->A->B->C).
		// Canonicalize each cycle by rotating so the lexicographically
		// smallest UID comes first, then deduplicate.
		const seen = new Set<string>();
		const results: CycleResult[] = [];

		for (const r of rows) {
			const rawUids = (r.path as string).split(" -> ");
			// The path includes the start node repeated at the end; remove it
			// to get the unique members of the ring.
			const ringUids = rawUids.slice(0, -1);
			if (ringUids.length === 0) continue;

			const canonicalKey = canonicalizeCycle(ringUids);
			if (seen.has(canonicalKey)) continue;
			seen.add(canonicalKey);

			const canonicalUids = canonicalKey.split(",");
			const nodes = canonicalUids.map((uid) => {
				const node = this.db
					.prepare("SELECT name, file_uid FROM nodes WHERE node_uid = ?")
					.get(uid) as Record<string, unknown> | undefined;
				return {
					nodeUid: uid,
					name: (node?.name as string) ?? uid,
					file: null as string | null,
				};
			});

			results.push({
				cycleId: `cycle-${results.length + 1}`,
				length: canonicalUids.length, // number of edges in the ring
				nodes,
			});
		}

		return results;
	}

	// ── Boundary queries ──────────────────────────────────────────────

	findImportsBetweenPaths(
		input: FindImportsBetweenPathsInput,
	): ImportEdgeResult[] {
		// Find all IMPORTS edges where the source FILE node's path starts
		// with sourcePrefix and the target FILE node's path starts with
		// targetPrefix. Uses the files table to match paths.
		// Normalize: strip trailing slashes to prevent double-slash in LIKE pattern.
		const srcPrefix = input.sourcePrefix.replace(/\/+$/, "");
		const tgtPrefix = input.targetPrefix.replace(/\/+$/, "");
		const srcPattern = `${srcPrefix}/%`;
		const tgtPattern = `${tgtPrefix}/%`;

		const rows = this.db
			.prepare(
				`SELECT
				   src_f.path AS source_file,
				   tgt_f.path AS target_file,
				   e.line_start AS line
				 FROM edges e
				 JOIN nodes src_n ON e.source_node_uid = src_n.node_uid
				 JOIN nodes tgt_n ON e.target_node_uid = tgt_n.node_uid
				 JOIN files src_f ON src_n.file_uid = src_f.file_uid
				 JOIN files tgt_f ON tgt_n.file_uid = tgt_f.file_uid
				 WHERE e.snapshot_uid = ?
				   AND e.type = 'IMPORTS'
				   AND src_f.path LIKE ?
				   AND tgt_f.path LIKE ?
				 ORDER BY src_f.path, e.line_start`,
			)
			.all(input.snapshotUid, srcPattern, tgtPattern) as Record<
			string,
			unknown
		>[];

		return rows.map((row) => ({
			sourceFile: row.source_file as string,
			targetFile: row.target_file as string,
			line: (row.line as number) ?? null,
		}));
	}

	// ── Row mappers ────────────────────────────────────────────────────

	private mapRepo(row: Record<string, unknown>): Repo {
		return {
			repoUid: row.repo_uid as string,
			name: row.name as string,
			rootPath: row.root_path as string,
			defaultBranch: (row.default_branch as string) ?? null,
			createdAt: row.created_at as string,
			metadataJson: (row.metadata_json as string) ?? null,
		};
	}

	private mapSnapshot(row: Record<string, unknown>): Snapshot {
		return {
			snapshotUid: row.snapshot_uid as string,
			repoUid: row.repo_uid as string,
			parentSnapshotUid: (row.parent_snapshot_uid as string) ?? null,
			kind: row.kind as Snapshot["kind"],
			basisRef: (row.basis_ref as string) ?? null,
			basisCommit: (row.basis_commit as string) ?? null,
			dirtyHash: (row.dirty_hash as string) ?? null,
			status: row.status as Snapshot["status"],
			filesTotal: (row.files_total as number) ?? 0,
			nodesTotal: (row.nodes_total as number) ?? 0,
			edgesTotal: (row.edges_total as number) ?? 0,
			createdAt: row.created_at as string,
			completedAt: (row.completed_at as string) ?? null,
			label: (row.label as string) ?? null,
			toolchainJson: (row.toolchain_json as string) ?? null,
		};
	}

	private mapFile(row: Record<string, unknown>): TrackedFile {
		return {
			fileUid: row.file_uid as string,
			repoUid: row.repo_uid as string,
			path: row.path as string,
			language: (row.language as string) ?? null,
			isTest: (row.is_test as number) === 1,
			isGenerated: (row.is_generated as number) === 1,
			isExcluded: (row.is_excluded as number) === 1,
		};
	}

	private mapNode(row: Record<string, unknown>): GraphNode {
		return {
			nodeUid: row.node_uid as string,
			snapshotUid: row.snapshot_uid as string,
			repoUid: row.repo_uid as string,
			stableKey: row.stable_key as string,
			kind: row.kind as GraphNode["kind"],
			subtype: (row.subtype as GraphNode["subtype"]) ?? null,
			name: row.name as string,
			qualifiedName: (row.qualified_name as string) ?? null,
			fileUid: (row.file_uid as string) ?? null,
			parentNodeUid: (row.parent_node_uid as string) ?? null,
			location:
				row.line_start != null
					? {
							lineStart: row.line_start as number,
							colStart: (row.col_start as number) ?? 0,
							lineEnd: (row.line_end as number) ?? (row.line_start as number),
							colEnd: (row.col_end as number) ?? 0,
						}
					: null,
			signature: (row.signature as string) ?? null,
			visibility: (row.visibility as GraphNode["visibility"]) ?? null,
			docComment: (row.doc_comment as string) ?? null,
			metadataJson: (row.metadata_json as string) ?? null,
		};
	}

	private mapDeclaration(row: Record<string, unknown>): Declaration {
		return {
			declarationUid: row.declaration_uid as string,
			repoUid: row.repo_uid as string,
			snapshotUid: (row.snapshot_uid as string) ?? null,
			targetStableKey: row.target_stable_key as string,
			kind: row.kind as Declaration["kind"],
			valueJson: row.value_json as string,
			createdAt: row.created_at as string,
			createdBy: (row.created_by as string) ?? null,
			supersedesUid: (row.supersedes_uid as string) ?? null,
			isActive: (row.is_active as number) === 1,
			authoredBasisJson: (row.authored_basis_json as string) ?? null,
		};
	}

	private mapNodeResult(row: Record<string, unknown>): NodeResult {
		return {
			nodeUid: row.node_uid as string,
			symbol: (row.qualified_name as string) ?? (row.name as string),
			kind: row.kind as string,
			subtype: (row.subtype as string) ?? null,
			file: (row.file_path as string) ?? "",
			line: (row.line_start as number) ?? null,
			column: (row.col_start as number) ?? null,
			edgeType: (row.edge_type as string) ?? null,
			resolution: (row.resolution as string) ?? null,
			evidence: row.extractor ? [row.extractor as string] : [],
			depth: (row.depth as number) ?? 0,
		};
	}
}

/**
 * Canonicalize a cycle by rotating the UID array so the lexicographically
 * smallest UID comes first. This makes A->B->C and B->C->A and C->A->B
 * all produce the same canonical key, enabling deduplication.
 */
function canonicalizeCycle(uids: string[]): string {
	let minIdx = 0;
	for (let i = 1; i < uids.length; i++) {
		if (uids[i] < uids[minIdx]) {
			minIdx = i;
		}
	}
	const rotated = [...uids.slice(minIdx), ...uids.slice(0, minIdx)];
	return rotated.join(",");
}
