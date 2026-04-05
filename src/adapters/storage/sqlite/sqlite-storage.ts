import Database from "better-sqlite3";
import { v4 as uuidv4 } from "uuid";
import type {
	CycleResult,
	DeadNodeResult,
	Declaration,
	FileVersion,
	GraphEdge,
	GraphNode,
	ModuleStats,
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
	DomainVersionRow,
	FindCyclesInput,
	FindDeadNodesInput,
	FindImportsBetweenPathsInput,
	FindImportsInput,
	FindPathInput,
	FindTraversalInput,
	FunctionMetricRow,
	ChangedFileResolution,
	FindActiveWaiversInput,
	FindReverseModuleImportsInput,
	GetDeclarationsInput,
	PathPrefixModuleCycle,
	ResolveChangedFilesToModulesInput,
	ReverseModuleImportResult,
	HotspotInput,
	ImportEdgeResult,
	InferenceRow,
	Measurement,
	ModuleMetricAggregate,
	QueryFunctionMetricsInput,
	RepoRef,
	ResolveSymbolInput,
	StoragePort,
	UpdateSnapshotStatusInput,
} from "../../../core/ports/storage.js";
/**
 * Thrown by insertNodes when the input batch contains two or more
 * nodes with the same stable_key. This is an identity-model defect
 * — either an extractor bug (duplicate emission), an indexer bug
 * (double-building the same symbol), or a stable_key format gap
 * (declaration merging case not disambiguated).
 *
 * The error message names the colliding stable_key and surfaces
 * each candidate node's identity fields so the root cause can be
 * found directly, instead of being masked by SQLite's bare
 * "UNIQUE constraint failed" message.
 */
/**
 * Extract the repo-relative path from a file_uid.
 *
 * file_uid format is `<repo_uid>:<path>`. The repo_uid never contains
 * a colon, so splitting on the first colon is safe and recovers the
 * path verbatim (including any nested slashes or colons within the
 * path itself — though paths normally contain neither).
 *
 * Used by NodeStableKeyCollisionError to surface the file path
 * directly in the error message, so operators can open the file
 * without a second database lookup.
 */
function extractPathFromFileUid(fileUid: string | null): string | null {
	if (fileUid === null) return null;
	const firstColon = fileUid.indexOf(":");
	if (firstColon < 0) return null;
	return fileUid.slice(firstColon + 1);
}

export class NodeStableKeyCollisionError extends Error {
	constructor(
		public readonly collisions: Array<{
			stableKey: string;
			nodes: Array<{
				nodeUid: string;
				stableKey: string;
				kind: string;
				subtype: string | null;
				name: string;
				qualifiedName: string | null;
				fileUid: string | null;
				location: { lineStart: number } | null;
			}>;
		}>,
	) {
		const lines: string[] = [
			`Node stable_key collision(s) detected during insert.`,
			`${collisions.length} colliding key(s):`,
			"",
		];
		for (const c of collisions) {
			lines.push(`  stable_key: ${c.stableKey}`);
			for (const n of c.nodes) {
				const loc = n.location ? `:${n.location.lineStart}` : "";
				// file_uid format is `<repo_uid>:<repo-relative-path>`.
				// Extract the path so the operator can open the file directly
				// without a second lookup against the files table.
				const filePath = extractPathFromFileUid(n.fileUid);
				lines.push(
					`    kind=${n.kind} subtype=${n.subtype ?? "-"} name=${n.name} ` +
						`qualified_name=${n.qualifiedName ?? "-"}`,
				);
				lines.push(
					`      file: ${filePath ?? "(unknown)"}${loc}` +
						(n.fileUid ? `  [file_uid=${n.fileUid}]` : "") +
						`  node_uid=${n.nodeUid}`,
				);
			}
			lines.push("");
		}
		lines.push(
			`This is an identity-model defect. The stable_key format is not disambiguating these emissions.`,
		);
		super(lines.join("\n"));
		this.name = "NodeStableKeyCollisionError";
	}
}

export class SqliteStorage implements StoragePort {
	/**
	 * Constructor accepts a Database handle owned by the caller
	 * (typically SqliteConnectionProvider). This adapter does NOT
	 * own the connection lifecycle — it does not open, close, or
	 * migrate the database.
	 *
	 * See src/adapters/storage/sqlite/connection-provider.ts for
	 * the infrastructure lifecycle contract.
	 */
	constructor(private db: Database.Database) {}

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

	updateSnapshotExtractionDiagnostics(
		snapshotUid: string,
		diagnosticsJson: string,
	): void {
		this.db
			.prepare(
				"UPDATE snapshots SET extraction_diagnostics_json = ? WHERE snapshot_uid = ?",
			)
			.run(diagnosticsJson, snapshotUid);
	}

	getSnapshotExtractionDiagnostics(snapshotUid: string): string | null {
		const row = this.db
			.prepare(
				"SELECT extraction_diagnostics_json FROM snapshots WHERE snapshot_uid = ?",
			)
			.get(snapshotUid) as
			| { extraction_diagnostics_json: string | null }
			| undefined;
		return row?.extraction_diagnostics_json ?? null;
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
		// Pre-flight: detect stable_key collisions within the input batch.
		// All nodes in a single insertNodes call share the same
		// snapshot_uid, so (snapshot_uid, stable_key) collisions are
		// equivalent to stable_key duplicates inside this array.
		//
		// Without this check, SQLite emits:
		//   SqliteError: UNIQUE constraint failed: nodes.snapshot_uid, nodes.stable_key
		// which does not identify WHICH rows collided. That makes
		// identity-defect diagnosis on real codebases nearly impossible.
		//
		// With this check, the error names the stable_key and surfaces
		// the two (or more) candidate emissions so the upstream
		// extractor/indexer bug can be found directly.
		const seen = new Map<string, GraphNode>();
		const collisions: Array<{ stableKey: string; nodes: GraphNode[] }> = [];
		for (const n of nodes) {
			const existing = seen.get(n.stableKey);
			if (existing === undefined) {
				seen.set(n.stableKey, n);
				continue;
			}
			// Collision found. Group all nodes sharing this key.
			let group = collisions.find((g) => g.stableKey === n.stableKey);
			if (group === undefined) {
				group = { stableKey: n.stableKey, nodes: [existing] };
				collisions.push(group);
			}
			group.nodes.push(n);
		}
		if (collisions.length > 0) {
			throw new NodeStableKeyCollisionError(collisions);
		}

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

	findActiveWaivers(input: FindActiveWaiversInput): Declaration[] {
		// Expiry comparison is lexicographic on ISO 8601 strings. This
		// works for all timezone-normalized formats and is consistent
		// with the string-based storage format.
		const now = input.now ?? new Date().toISOString();

		let sql = `SELECT * FROM declarations
		           WHERE repo_uid = ?
		             AND kind = 'waiver'
		             AND is_active = 1
		             AND (json_extract(value_json, '$.expires_at') IS NULL
		                  OR json_extract(value_json, '$.expires_at') > ?)`;
		const params: unknown[] = [input.repoUid, now];

		if (input.reqId !== undefined) {
			sql += " AND json_extract(value_json, '$.req_id') = ?";
			params.push(input.reqId);
		}
		if (input.requirementVersion !== undefined) {
			sql += " AND json_extract(value_json, '$.requirement_version') = ?";
			params.push(input.requirementVersion);
		}
		if (input.obligationId !== undefined) {
			sql += " AND json_extract(value_json, '$.obligation_id') = ?";
			params.push(input.obligationId);
		}
		sql += " ORDER BY created_at DESC";

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((r) => this.mapDeclaration(r));
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

	resolveChangedFilesToModules(
		input: ResolveChangedFilesToModulesInput,
	): ChangedFileResolution[] {
		if (input.filePaths.length === 0) return [];

		// Build expected FILE stable_keys for each input path.
		// Keep a map from stable_key back to original path so results
		// can be matched to input ordering.
		const keyToPath = new Map<string, string>();
		for (const path of input.filePaths) {
			const key = `${input.repoUid}:${path}:FILE`;
			keyToPath.set(key, path);
		}

		// Single query: for each candidate FILE node, LEFT JOIN the
		// OWNS edge and the owning MODULE. Null module_key means the
		// file exists but has no OWNS edge (defensive).
		const placeholders = Array.from(keyToPath.keys())
			.map(() => "?")
			.join(", ");
		const sql = `
			SELECT
				file_n.stable_key AS file_key,
				mod_n.stable_key AS module_key
			FROM nodes file_n
			LEFT JOIN edges own_e
				ON own_e.target_node_uid = file_n.node_uid
				AND own_e.type = 'OWNS'
				AND own_e.snapshot_uid = ?
			LEFT JOIN nodes mod_n
				ON own_e.source_node_uid = mod_n.node_uid
				AND mod_n.kind = 'MODULE'
				AND mod_n.snapshot_uid = ?
			WHERE file_n.snapshot_uid = ?
				AND file_n.kind = 'FILE'
				AND file_n.stable_key IN (${placeholders})
		`;
		const params = [
			input.snapshotUid,
			input.snapshotUid,
			input.snapshotUid,
			...keyToPath.keys(),
		];
		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];

		// Build lookup map
		const matched = new Map<string, string | null>();
		for (const row of rows) {
			matched.set(
				row.file_key as string,
				(row.module_key as string | null) ?? null,
			);
		}

		// Preserve input order
		return input.filePaths.map((path) => {
			const key = `${input.repoUid}:${path}:FILE`;
			if (matched.has(key)) {
				return {
					filePath: path,
					matched: true,
					owningModuleKey: matched.get(key) ?? null,
				};
			}
			return { filePath: path, matched: false, owningModuleKey: null };
		});
	}

	findReverseModuleImports(
		input: FindReverseModuleImportsInput,
	): ReverseModuleImportResult[] {
		if (input.seedModuleKeys.length === 0) return [];

		const seedPlaceholders = input.seedModuleKeys.map(() => "?").join(", ");
		// Unbounded traversal uses a large sentinel. SQLite recursive CTEs
		// need a depth guard to terminate; the visited-set semantics
		// (WHERE node_uid NOT IN visited) break cycles, but depth also
		// serves as belt-and-suspenders. 10000 is effectively unbounded
		// for any realistic repo.
		const maxDepth = input.maxDepth ?? 10000;

		// Recursive CTE: reverse IMPORTS at MODULE level only.
		// Level 1: modules importing any seed.
		// Level n: modules importing a module found at level n-1.
		// Cycle-safe: track visited node_uids in the path column.
		const sql = `
			WITH RECURSIVE traversal(node_uid, stable_key, distance, path) AS (
				SELECT src_n.node_uid, src_n.stable_key, 1,
				       ',' || src_n.node_uid || ','
				FROM edges e
				JOIN nodes src_n ON e.source_node_uid = src_n.node_uid
				JOIN nodes tgt_n ON e.target_node_uid = tgt_n.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type = 'IMPORTS'
				  AND src_n.kind = 'MODULE'
				  AND tgt_n.kind = 'MODULE'
				  AND tgt_n.stable_key IN (${seedPlaceholders})

				UNION ALL

				SELECT src_n.node_uid, src_n.stable_key, t.distance + 1,
				       t.path || src_n.node_uid || ','
				FROM edges e
				JOIN traversal t ON e.target_node_uid = t.node_uid
				JOIN nodes src_n ON e.source_node_uid = src_n.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type = 'IMPORTS'
				  AND src_n.kind = 'MODULE'
				  AND t.distance < ?
				  AND t.path NOT LIKE '%,' || src_n.node_uid || ',%'
			)
			SELECT stable_key, MIN(distance) AS distance
			FROM traversal
			WHERE stable_key NOT IN (${seedPlaceholders})
			GROUP BY stable_key
			ORDER BY distance ASC, stable_key ASC
		`;
		const params = [
			input.snapshotUid,
			...input.seedModuleKeys,
			input.snapshotUid,
			maxDepth,
			...input.seedModuleKeys,
		];

		const rows = this.db.prepare(sql).all(...params) as Record<
			string,
			unknown
		>[];
		return rows.map((row) => ({
			moduleStableKey: row.stable_key as string,
			distance: row.distance as number,
		}));
	}

	countEdgesByType(snapshotUid: string, edgeType: string): number {
		const row = this.db
			.prepare(
				"SELECT COUNT(*) AS c FROM edges WHERE snapshot_uid = ? AND type = ?",
			)
			.get(snapshotUid, edgeType) as { c: number };
		return row.c;
	}

	findPathPrefixModuleCycles(
		snapshotUid: string,
	): PathPrefixModuleCycle[] {
		// Find 2-node MODULE IMPORTS cycles where one module's qualified_name
		// is a strict path-prefix ancestor of the other's.
		//
		// Algorithm (SQL):
		//   1. Find all (A, B) module pairs where A -> B AND B -> A (mutual IMPORTS).
		//   2. Keep only pairs where A.qualified_name is a strict prefix of
		//      B.qualified_name (or vice versa), using '/' as path separator.
		//   3. Normalize each pair so the ancestor is returned first.
		//
		// Strict prefix check: B.qualified_name must start with
		// A.qualified_name + '/'. This prevents accidental matches between
		// modules with shared prefixes but no ancestor relationship
		// (e.g. "src/api" and "src/api-legacy").
		const sql = `
			WITH mutual_pairs AS (
				SELECT
					e1.source_node_uid AS a_uid,
					e1.target_node_uid AS b_uid
				FROM edges e1
				JOIN edges e2
					ON e2.snapshot_uid = e1.snapshot_uid
					AND e2.type = 'IMPORTS'
					AND e2.source_node_uid = e1.target_node_uid
					AND e2.target_node_uid = e1.source_node_uid
				JOIN nodes a ON a.node_uid = e1.source_node_uid
				JOIN nodes b ON b.node_uid = e1.target_node_uid
				WHERE e1.snapshot_uid = ?
					AND e1.type = 'IMPORTS'
					AND a.kind = 'MODULE'
					AND b.kind = 'MODULE'
					AND a.node_uid < b.node_uid  -- dedupe each pair once
			)
			SELECT
				CASE
					WHEN LENGTH(a.qualified_name) < LENGTH(b.qualified_name)
						THEN a.stable_key
					ELSE b.stable_key
				END AS ancestor_key,
				CASE
					WHEN LENGTH(a.qualified_name) < LENGTH(b.qualified_name)
						THEN b.stable_key
					ELSE a.stable_key
				END AS descendant_key
			FROM mutual_pairs mp
			JOIN nodes a ON a.node_uid = mp.a_uid
			JOIN nodes b ON b.node_uid = mp.b_uid
			WHERE
				(b.qualified_name LIKE a.qualified_name || '/%'
					AND a.qualified_name != b.qualified_name)
				OR
				(a.qualified_name LIKE b.qualified_name || '/%'
					AND a.qualified_name != b.qualified_name)
			ORDER BY ancestor_key, descendant_key
		`;
		const rows = this.db.prepare(sql).all(snapshotUid) as Record<
			string,
			unknown
		>[];
		return rows.map((row) => ({
			ancestorStableKey: row.ancestor_key as string,
			descendantStableKey: row.descendant_key as string,
		}));
	}

	// ── Module metrics ────────────────────────────────────────────────

	computeModuleStats(snapshotUid: string): ModuleStats[] {
		// One query to get all module-level structural metrics.
		// fan_in: count of distinct modules that import this module
		// fan_out: count of distinct modules this module imports
		// file_count: OWNS edges from this module to FILE nodes
		// symbol_count: nodes where parent_node_uid is in this module's files
		//               and visibility = 'export'
		// abstractness: interface/type_alias count / total type count
		// Use correlated subqueries for symbol/type counts to correctly
		// sum across all files owned by each module.
		const rows = this.db
			.prepare(
				`SELECT
				   m.node_uid,
				   m.stable_key,
				   m.name,
				   m.qualified_name AS path,
				   COALESCE(fan_in.cnt, 0) AS fan_in,
				   COALESCE(fan_out.cnt, 0) AS fan_out,
				   COALESCE(files.cnt, 0) AS file_count,
				   (SELECT COUNT(*) FROM nodes n
				    WHERE n.snapshot_uid = ?
				      AND n.kind = 'SYMBOL' AND n.visibility = 'export'
				      AND n.file_uid IN (
				        SELECT tgt.file_uid FROM edges oe
				        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
				        WHERE oe.snapshot_uid = ? AND oe.type = 'OWNS'
				          AND oe.source_node_uid = m.node_uid
				      )
				   ) AS symbol_count,
				   (SELECT COUNT(*) FROM nodes n
				    WHERE n.snapshot_uid = ?
				      AND n.kind = 'SYMBOL'
				      AND n.subtype IN ('INTERFACE', 'TYPE_ALIAS')
				      AND n.parent_node_uid IS NULL
				      AND n.file_uid IN (
				        SELECT tgt.file_uid FROM edges oe
				        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
				        WHERE oe.snapshot_uid = ? AND oe.type = 'OWNS'
				          AND oe.source_node_uid = m.node_uid
				      )
				   ) AS abstract_count,
				   (SELECT COUNT(*) FROM nodes n
				    WHERE n.snapshot_uid = ?
				      AND n.kind = 'SYMBOL'
				      AND n.subtype IN ('INTERFACE', 'TYPE_ALIAS', 'CLASS', 'ENUM')
				      AND n.parent_node_uid IS NULL
				      AND n.file_uid IN (
				        SELECT tgt.file_uid FROM edges oe
				        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
				        WHERE oe.snapshot_uid = ? AND oe.type = 'OWNS'
				          AND oe.source_node_uid = m.node_uid
				      )
				   ) AS type_count
				 FROM nodes m
				 LEFT JOIN (
				   SELECT target_node_uid AS nid, COUNT(DISTINCT source_node_uid) AS cnt
				   FROM edges
				   WHERE snapshot_uid = ? AND type = 'IMPORTS'
				     AND source_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND kind = 'MODULE')
				   GROUP BY target_node_uid
				 ) fan_in ON fan_in.nid = m.node_uid
				 LEFT JOIN (
				   SELECT source_node_uid AS nid, COUNT(DISTINCT target_node_uid) AS cnt
				   FROM edges
				   WHERE snapshot_uid = ? AND type = 'IMPORTS'
				     AND target_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND kind = 'MODULE')
				   GROUP BY source_node_uid
				 ) fan_out ON fan_out.nid = m.node_uid
				 LEFT JOIN (
				   SELECT source_node_uid AS nid, COUNT(*) AS cnt
				   FROM edges
				   WHERE snapshot_uid = ? AND type = 'OWNS'
				   GROUP BY source_node_uid
				 ) files ON files.nid = m.node_uid
				 WHERE m.snapshot_uid = ? AND m.kind = 'MODULE'
				   AND COALESCE(files.cnt, 0) > 0
				 ORDER BY m.qualified_name`,
			)
			.all(
				// Correlated subquery params (6) + join params (6) + final WHERE (1)
				snapshotUid,
				snapshotUid, // symbol_count
				snapshotUid,
				snapshotUid, // abstract_count
				snapshotUid,
				snapshotUid, // type_count
				snapshotUid,
				snapshotUid, // fan_in
				snapshotUid,
				snapshotUid, // fan_out
				snapshotUid, // files
				snapshotUid, // WHERE
			) as Record<string, unknown>[];

		return rows.map((row) => {
			const fanIn = row.fan_in as number;
			const fanOut = row.fan_out as number;
			const total = fanIn + fanOut;
			const instability = total > 0 ? fanOut / total : 0;
			const abstractCount = row.abstract_count as number;
			const typeCount = row.type_count as number;
			const abstractness = typeCount > 0 ? abstractCount / typeCount : 0;
			const distance = Math.abs(abstractness + instability - 1);

			return {
				stableKey: row.stable_key as string,
				name: row.name as string,
				path: (row.path as string) ?? (row.name as string),
				fanIn,
				fanOut,
				instability: Math.round(instability * 100) / 100,
				abstractness: Math.round(abstractness * 100) / 100,
				distanceFromMainSequence: Math.round(distance * 100) / 100,
				fileCount: row.file_count as number,
				symbolCount: row.symbol_count as number,
			};
		});
	}

	queryModuleMetricAggregates(snapshotUid: string): ModuleMetricAggregate[] {
		// Step 1: get per-function metrics with file paths (in TS, fast)
		const funcRows = this.db
			.prepare(
				`SELECT
				   f.path AS file_path,
				   MAX(CASE WHEN m.kind = 'cyclomatic_complexity'
				     THEN CAST(json_extract(m.value_json, '$.value') AS INTEGER) END) AS cc,
				   MAX(CASE WHEN m.kind = 'max_nesting_depth'
				     THEN CAST(json_extract(m.value_json, '$.value') AS INTEGER) END) AS nesting
				 FROM measurements m
				 JOIN nodes n ON m.target_stable_key = n.stable_key
				   AND n.snapshot_uid = m.snapshot_uid
				 LEFT JOIN files f ON n.file_uid = f.file_uid
				 WHERE m.snapshot_uid = ?
				   AND m.kind IN ('cyclomatic_complexity', 'max_nesting_depth')
				 GROUP BY m.target_stable_key`,
			)
			.all(snapshotUid) as Array<{
			file_path: string | null;
			cc: number;
			nesting: number;
		}>;

		// Step 2: aggregate per module (directory) in TypeScript
		// since SQLite lacks a clean dirname() function.
		const moduleMap = new Map<string, { ccs: number[]; nestings: number[] }>();
		for (const row of funcRows) {
			if (!row.file_path) continue;
			const lastSlash = row.file_path.lastIndexOf("/");
			const dir = lastSlash > 0 ? row.file_path.slice(0, lastSlash) : ".";
			let entry = moduleMap.get(dir);
			if (!entry) {
				entry = { ccs: [], nestings: [] };
				moduleMap.set(dir, entry);
			}
			entry.ccs.push(row.cc ?? 0);
			entry.nestings.push(row.nesting ?? 0);
		}

		const results: ModuleMetricAggregate[] = [];
		for (const [dir, data] of moduleMap) {
			const n = data.ccs.length;
			results.push({
				modulePath: dir,
				functionCount: n,
				avgCyclomaticComplexity:
					Math.round((data.ccs.reduce((a, b) => a + b, 0) / n) * 10) / 10,
				maxCyclomaticComplexity: Math.max(...data.ccs),
				avgNestingDepth:
					Math.round((data.nestings.reduce((a, b) => a + b, 0) / n) * 10) / 10,
				maxNestingDepth: Math.max(...data.nestings),
			});
		}

		return results.sort(
			(a, b) => b.maxCyclomaticComplexity - a.maxCyclomaticComplexity,
		);
	}

	queryDomainVersions(snapshotUid: string): DomainVersionRow[] {
		const rows = this.db
			.prepare(
				`SELECT kind, value_json
				 FROM measurements
				 WHERE snapshot_uid = ?
				   AND kind IN ('package_name', 'package_version')
				 ORDER BY kind`,
			)
			.all(snapshotUid) as Array<{
			kind: string;
			value_json: string;
		}>;

		return rows.map((row) => {
			const parsed = JSON.parse(row.value_json) as {
				value: string;
				source_file: string;
			};
			return {
				kind: row.kind,
				value: parsed.value,
				sourceFile: parsed.source_file,
			};
		});
	}

	queryMeasurementsByKind(
		snapshotUid: string,
		kind: string,
	): Array<{ targetStableKey: string; valueJson: string }> {
		const rows = this.db
			.prepare(
				"SELECT target_stable_key, value_json FROM measurements WHERE snapshot_uid = ? AND kind = ?",
			)
			.all(snapshotUid, kind) as Array<{
			target_stable_key: string;
			value_json: string;
		}>;
		return rows.map((r) => ({
			targetStableKey: r.target_stable_key,
			valueJson: r.value_json,
		}));
	}

	deleteMeasurementsByKind(snapshotUid: string, kinds: string[]): void {
		if (kinds.length === 0) return;
		const placeholders = kinds.map(() => "?").join(", ");
		this.db
			.prepare(
				`DELETE FROM measurements
				 WHERE snapshot_uid = ? AND kind IN (${placeholders})`,
			)
			.run(snapshotUid, ...kinds);
	}

	insertMeasurements(measurements: Measurement[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO measurements
			 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, kind,
			  value_json, source, created_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const m of measurements) {
				stmt.run(
					m.measurementUid,
					m.snapshotUid,
					m.repoUid,
					m.targetStableKey,
					m.kind,
					m.valueJson,
					m.source,
					m.createdAt,
				);
			}
		});
		insertAll();
	}

	queryHotspotInputs(snapshotUid: string): HotspotInput[] {
		// Join churn measurements (per-file) with complexity measurements
		// (per-function, summed to file level). Only return files that have
		// BOTH churn and complexity data.
		const rows = this.db
			.prepare(
				`SELECT
				   churn.target_stable_key AS file_key,
				   REPLACE(churn.target_stable_key, churn.repo_uid || ':', '') AS raw_path,
				   CAST(json_extract(churn.value_json, '$.value') AS INTEGER) AS churn_lines,
				   COALESCE(freq.change_freq, 0) AS change_frequency,
				   COALESCE(cc.sum_cc, 0) AS sum_complexity
				 FROM measurements churn
				 LEFT JOIN (
				   SELECT target_stable_key,
				     CAST(json_extract(value_json, '$.value') AS INTEGER) AS change_freq
				   FROM measurements
				   WHERE snapshot_uid = ? AND kind = 'change_frequency'
				 ) freq ON freq.target_stable_key = churn.target_stable_key
				 LEFT JOIN (
				   SELECT n.file_uid, SUM(CAST(json_extract(m.value_json, '$.value') AS INTEGER)) AS sum_cc
				   FROM measurements m
				   JOIN nodes n ON m.target_stable_key = n.stable_key AND n.snapshot_uid = m.snapshot_uid
				   WHERE m.snapshot_uid = ? AND m.kind = 'cyclomatic_complexity'
				   GROUP BY n.file_uid
				 ) cc ON cc.file_uid = (
				   SELECT file_uid FROM files WHERE file_uid = churn.repo_uid || ':' ||
				     REPLACE(REPLACE(churn.target_stable_key, churn.repo_uid || ':', ''), ':FILE', '')
				 )
				 WHERE churn.snapshot_uid = ? AND churn.kind = 'churn_lines'
				   AND cc.sum_cc > 0
				 ORDER BY churn_lines * sum_complexity DESC`,
			)
			.all(snapshotUid, snapshotUid, snapshotUid) as Record<string, unknown>[];

		return rows.map((row) => ({
			fileStableKey: row.file_key as string,
			filePath: ((row.raw_path as string) ?? "").replace(/:FILE$/, ""),
			churnLines: (row.churn_lines as number) ?? 0,
			changeFrequency: (row.change_frequency as number) ?? 0,
			sumComplexity: (row.sum_complexity as number) ?? 0,
		}));
	}

	insertInferences(inferences: InferenceRow[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO inferences
			 (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind,
			  value_json, confidence, basis_json, extractor, created_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const inf of inferences) {
				stmt.run(
					inf.inferenceUid,
					inf.snapshotUid,
					inf.repoUid,
					inf.targetStableKey,
					inf.kind,
					inf.valueJson,
					inf.confidence,
					inf.basisJson,
					inf.extractor,
					inf.createdAt,
				);
			}
		});
		insertAll();
	}

	deleteInferencesByKind(snapshotUid: string, kind: string): void {
		this.db
			.prepare("DELETE FROM inferences WHERE snapshot_uid = ? AND kind = ?")
			.run(snapshotUid, kind);
	}

	queryInferences(snapshotUid: string, kind: string): InferenceRow[] {
		const rows = this.db
			.prepare(
				"SELECT * FROM inferences WHERE snapshot_uid = ? AND kind = ? ORDER BY CAST(json_extract(value_json, '$.normalized_score') AS REAL) DESC",
			)
			.all(snapshotUid, kind) as Record<string, unknown>[];
		return rows.map((row) => ({
			inferenceUid: row.inference_uid as string,
			snapshotUid: row.snapshot_uid as string,
			repoUid: row.repo_uid as string,
			targetStableKey: row.target_stable_key as string,
			kind: row.kind as string,
			valueJson: row.value_json as string,
			confidence: row.confidence as number,
			basisJson: row.basis_json as string,
			extractor: row.extractor as string,
			createdAt: row.created_at as string,
		}));
	}

	queryFunctionMetrics(input: QueryFunctionMetricsInput): FunctionMetricRow[] {
		const sortField = input.sortBy ?? "cyclomatic_complexity";
		const limitClause = input.limit ? `LIMIT ${input.limit}` : "";

		// Pivot three measurement rows per function into one row with
		// all three metrics. Uses conditional aggregation over the
		// measurements table joined to nodes for file/line info.
		const rows = this.db
			.prepare(
				`SELECT
				   m.target_stable_key AS stable_key,
				   n.qualified_name AS symbol,
				   f.path AS file,
				   n.line_start AS line,
				   MAX(CASE WHEN m.kind = 'cyclomatic_complexity'
				     THEN CAST(json_extract(m.value_json, '$.value') AS INTEGER) END) AS cc,
				   MAX(CASE WHEN m.kind = 'parameter_count'
				     THEN CAST(json_extract(m.value_json, '$.value') AS INTEGER) END) AS params,
				   MAX(CASE WHEN m.kind = 'max_nesting_depth'
				     THEN CAST(json_extract(m.value_json, '$.value') AS INTEGER) END) AS nesting
				 FROM measurements m
				 JOIN nodes n ON m.target_stable_key = n.stable_key
				   AND n.snapshot_uid = m.snapshot_uid
				 LEFT JOIN files f ON n.file_uid = f.file_uid
				 WHERE m.snapshot_uid = ?
				   AND m.kind IN ('cyclomatic_complexity', 'parameter_count', 'max_nesting_depth')
				 GROUP BY m.target_stable_key
				 ORDER BY ${sortField === "parameter_count" ? "params" : sortField === "max_nesting_depth" ? "nesting" : "cc"} DESC
				 ${limitClause}`,
			)
			.all(input.snapshotUid) as Record<string, unknown>[];

		return rows.map((row) => ({
			stableKey: row.stable_key as string,
			symbol: (row.symbol as string) ?? "",
			file: (row.file as string) ?? "",
			line: (row.line as number) ?? null,
			cyclomaticComplexity: (row.cc as number) ?? 0,
			parameterCount: (row.params as number) ?? 0,
			maxNestingDepth: (row.nesting as number) ?? 0,
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
