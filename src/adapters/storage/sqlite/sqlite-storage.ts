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
	BoundaryLinkRow,
	CountUnresolvedEdgesInput,
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
	PersistedBoundaryConsumerFact,
	PersistedBoundaryLink,
	PersistedBoundaryProviderFact,
	FileSignalRow,
	FileSymbolRow,
	PersistedUnresolvedEdge,
	QueryBoundaryFactsInput,
	ExtractionEdge,
	ResolverNode,
	QueryBoundaryLinksInput,
	QueryUnresolvedEdgesInput,
	ResolveChangedFilesToModulesInput,
	ReverseModuleImportResult,
	HotspotInput,
	ImportEdgeResult,
	InferenceRow,
	Measurement,
	CopyForwardInput,
	CopyForwardResult,
	ModuleCandidateRollup,
	ModuleMetricAggregate,
	ModuleOwnedFileRow,
	QueryFunctionMetricsInput,
	RepoRef,
	ResolveSymbolInput,
	StoragePort,
	UnresolvedEdgeCountRow,
	UnresolvedEdgeSampleRow,
	UpdateSnapshotStatusInput,
} from "../../../core/ports/storage.js";
import type { UnresolvedEdgeCategory } from "../../../core/diagnostics/unresolved-edge-categories.js";
import type {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../../../core/diagnostics/unresolved-edge-classification.js";
import type {
	AssignmentKind,
	DiagnosticKind,
	DiagnosticSeverity,
	DiagnosticSourceType,
	EvidenceKind,
	EvidenceSourceType,
	ModuleCandidate,
	ModuleCandidateEvidence,
	ModuleDiscoveryDiagnostic,
	ModuleDiscoveryDiagnosticInput,
	ModuleFileOwnership,
	ModuleKind,
} from "../../../core/modules/module-candidate.js";
import { buildDiagnosticUid } from "../../../core/modules/module-candidate.js";
import type {
	BuildSystem,
	ProjectSurface,
	ProjectSurfaceEvidence,
	RuntimeKind,
	SurfaceEvidenceKind,
	SurfaceEvidenceSourceType,
	SurfaceKind,
} from "../../../core/runtime/project-surface.js";
import type {
	ConfigRootKind,
	EntrypointKind,
	SurfaceConfigRoot,
	SurfaceEntrypoint,
} from "../../../core/topology/topology-links.js";
import type {
	EnvAccessKind,
	EnvAccessPattern,
	SurfaceEnvDependency,
	SurfaceEnvEvidence,
} from "../../../core/seams/env-dependency.js";
import type {
	MutationKind,
	MutationPattern,
	SurfaceFsMutation,
	SurfaceFsMutationEvidence,
} from "../../../core/seams/fs-mutation.js";
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

	queryAllNodes(snapshotUid: string): GraphNode[] {
		const rows = this.db.prepare(
			`SELECT node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
			        name, qualified_name, file_uid, parent_node_uid,
			        line_start, col_start, line_end, col_end,
			        signature, visibility, doc_comment, metadata_json
			 FROM nodes WHERE snapshot_uid = ?`,
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			nodeUid: r.node_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			stableKey: r.stable_key as string,
			kind: r.kind as string,
			subtype: (r.subtype as string) ?? null,
			name: r.name as string,
			qualifiedName: (r.qualified_name as string) ?? null,
			fileUid: (r.file_uid as string) ?? null,
			parentNodeUid: (r.parent_node_uid as string) ?? null,
			location: r.line_start !== null ? {
				lineStart: r.line_start as number,
				colStart: (r.col_start as number) ?? 0,
				lineEnd: (r.line_end as number) ?? (r.line_start as number),
				colEnd: (r.col_end as number) ?? 0,
			} : null,
			signature: (r.signature as string) ?? null,
			visibility: (r.visibility as string) ?? null,
			docComment: (r.doc_comment as string) ?? null,
			metadataJson: (r.metadata_json as string) ?? null,
		})) as GraphNode[];
	}

	queryResolverNodes(snapshotUid: string): ResolverNode[] {
		const rows = this.db.prepare(
			`SELECT node_uid, stable_key, name, qualified_name, kind, subtype, file_uid
			 FROM nodes WHERE snapshot_uid = ?`,
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			nodeUid: r.node_uid as string,
			stableKey: r.stable_key as string,
			name: r.name as string,
			qualifiedName: (r.qualified_name as string) ?? null,
			kind: r.kind as string,
			subtype: (r.subtype as string) ?? null,
			fileUid: (r.file_uid as string) ?? null,
		}));
	}

	/**
	 * Row-at-a-time iterator over resolver nodes. Uses better-sqlite3's
	 * .iterate() to avoid materializing the full result set as a JS array.
	 * Yields ResolverNode objects one at a time — the caller builds Maps
	 * directly, keeping peak memory = Maps only.
	 */
	*queryResolverNodesIter(snapshotUid: string): IterableIterator<ResolverNode> {
		const stmt = this.db.prepare(
			`SELECT node_uid, stable_key, name, qualified_name, kind, subtype, file_uid
			 FROM nodes WHERE snapshot_uid = ?`,
		);
		for (const r of stmt.iterate(snapshotUid) as Iterable<Record<string, unknown>>) {
			yield {
				nodeUid: r.node_uid as string,
				stableKey: r.stable_key as string,
				name: r.name as string,
				qualifiedName: (r.qualified_name as string) ?? null,
				kind: r.kind as string,
				subtype: (r.subtype as string) ?? null,
				fileUid: (r.file_uid as string) ?? null,
			};
		}
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

	deleteEdgesByUids(edgeUids: string[]): void {
		if (edgeUids.length === 0) return;
		const placeholders = edgeUids.map(() => "?").join(", ");
		this.db
			.prepare(`DELETE FROM edges WHERE edge_uid IN (${placeholders})`)
			.run(...edgeUids);
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

	// ── Unresolved Edges ────────────────────────────────────────────────

	insertUnresolvedEdges(edges: PersistedUnresolvedEdge[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO unresolved_edges
			 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
			  type, resolution, extractor,
			  line_start, col_start, line_end, col_end, metadata_json,
			  category, classification, classifier_version, basis_code, observed_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of edges) {
				stmt.run(
					e.edgeUid,
					e.snapshotUid,
					e.repoUid,
					e.sourceNodeUid,
					e.targetKey,
					e.type,
					e.resolution,
					e.extractor,
					e.location?.lineStart ?? null,
					e.location?.colStart ?? null,
					e.location?.lineEnd ?? null,
					e.location?.colEnd ?? null,
					e.metadataJson,
					e.category,
					e.classification,
					e.classifierVersion,
					e.basisCode,
					e.observedAt,
				);
			}
		});
		insertAll();
	}

	queryUnresolvedEdges(
		input: QueryUnresolvedEdgesInput,
	): UnresolvedEdgeSampleRow[] {
		// Join through nodes → files so the caller gets source path in
		// one round trip. LEFT JOIN on files: a source node may have
		// file_uid = NULL defensively (should be rare in practice).
		let sql = `
			SELECT
				ue.edge_uid          AS edge_uid,
				ue.classification    AS classification,
				ue.category          AS category,
				ue.basis_code        AS basis_code,
				ue.target_key        AS target_key,
				ue.source_node_uid   AS source_node_uid,
				n.stable_key         AS source_stable_key,
				f.path               AS source_file_path,
				ue.line_start        AS line_start,
				ue.col_start         AS col_start,
				n.visibility         AS source_node_visibility,
				ue.metadata_json     AS metadata_json
			FROM unresolved_edges ue
			LEFT JOIN nodes n ON n.node_uid = ue.source_node_uid
			LEFT JOIN files f ON f.file_uid = n.file_uid
			WHERE ue.snapshot_uid = ?`;
		const params: unknown[] = [input.snapshotUid];

		if (input.classification) {
			sql += " AND ue.classification = ?";
			params.push(input.classification);
		}
		if (input.category) {
			sql += " AND ue.category = ?";
			params.push(input.category);
		}
		if (input.basisCode) {
			sql += " AND ue.basis_code = ?";
			params.push(input.basisCode);
		}

		// Deterministic ordering. SQLite default: NULLS FIRST on ASC.
		// Tiebreak by edge_uid to make ties within the same file/line
		// stable across runs regardless of insertion order.
		sql +=
			" ORDER BY f.path ASC, ue.line_start ASC, ue.target_key ASC, ue.edge_uid ASC";

		if (input.limit !== undefined) {
			sql += " LIMIT ?";
			params.push(input.limit);
		}

		const rows = this.db.prepare(sql).all(...params) as Array<{
			edge_uid: string;
			classification: string;
			category: string;
			basis_code: string;
			target_key: string;
			source_node_uid: string;
			source_stable_key: string | null;
			source_file_path: string | null;
			line_start: number | null;
			col_start: number | null;
			source_node_visibility: string | null;
			metadata_json: string | null;
		}>;

		return rows.map((r) => ({
			edgeUid: r.edge_uid,
			classification: r.classification as UnresolvedEdgeClassification,
			category: r.category as UnresolvedEdgeCategory,
			basisCode: r.basis_code as UnresolvedEdgeBasisCode,
			targetKey: r.target_key,
			sourceNodeUid: r.source_node_uid,
			sourceStableKey: r.source_stable_key ?? "",
			sourceFilePath: r.source_file_path,
			lineStart: r.line_start,
			colStart: r.col_start,
			sourceNodeVisibility: r.source_node_visibility,
			metadataJson: r.metadata_json,
		}));
	}

	mergeUnresolvedEdgesMetadata(
		updates: Array<{ edgeUid: string; metadataJsonPatch: string }>,
	): void {
		const readStmt = this.db.prepare(
			"SELECT metadata_json FROM unresolved_edges WHERE edge_uid = ?",
		);
		const writeStmt = this.db.prepare(
			"UPDATE unresolved_edges SET metadata_json = ? WHERE edge_uid = ?",
		);
		const runAll = this.db.transaction(() => {
			for (const u of updates) {
				// Read existing metadata.
				const row = readStmt.get(u.edgeUid) as
					| { metadata_json: string | null }
					| undefined;
				let existing: Record<string, unknown> = {};
				if (row?.metadata_json) {
					try {
						existing = JSON.parse(row.metadata_json) as Record<
							string,
							unknown
						>;
					} catch {
						// Malformed existing metadata — start fresh.
					}
				}
				// Shallow-merge the patch.
				let patch: Record<string, unknown> = {};
				try {
					patch = JSON.parse(u.metadataJsonPatch) as Record<string, unknown>;
				} catch {
					continue; // Skip malformed patches.
				}
				const merged = { ...existing, ...patch };
				writeStmt.run(JSON.stringify(merged), u.edgeUid);
			}
		});
		runAll();
	}

	countUnresolvedEdges(
		input: CountUnresolvedEdgesInput,
	): UnresolvedEdgeCountRow[] {
		// Map groupBy to a column name. Guard against injection by
		// allowing only the two documented axes.
		const column =
			input.groupBy === "classification" ? "classification" : "category";

		let sql = `
			SELECT ${column} AS key, COUNT(*) AS count
			FROM unresolved_edges
			WHERE snapshot_uid = ?`;
		const params: unknown[] = [input.snapshotUid];

		// Optional category pre-filter (e.g. restrict to CALLS-family
		// categories before grouping by classification).
		if (input.filterCategories && input.filterCategories.length > 0) {
			const placeholders = input.filterCategories.map(() => "?").join(", ");
			sql += ` AND category IN (${placeholders})`;
			params.push(...input.filterCategories);
		}

		sql += ` GROUP BY ${column} ORDER BY key ASC`;

		const rows = this.db.prepare(sql).all(...params) as Array<{
			key: string;
			count: number;
		}>;
		return rows.map((r) => ({ key: r.key, count: r.count }));
	}

	// ── Extraction Edges (durable raw edge facts) ────────────────────

	insertExtractionEdges(edges: ExtractionEdge[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO extraction_edges
			 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
			  type, resolution, extractor,
			  line_start, col_start, line_end, col_end, metadata_json, source_file_uid)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of edges) {
				stmt.run(
					e.edgeUid, e.snapshotUid, e.repoUid, e.sourceNodeUid, e.targetKey,
					e.type, e.resolution, e.extractor,
					e.lineStart, e.colStart, e.lineEnd, e.colEnd,
					e.metadataJson, e.sourceFileUid,
				);
			}
		});
		insertAll();
	}

	queryExtractionEdgesBatch(
		snapshotUid: string,
		limit: number,
		afterEdgeUid: string | null,
	): ExtractionEdge[] {
		const sql = afterEdgeUid
			? "SELECT * FROM extraction_edges WHERE snapshot_uid = ? AND edge_uid > ? ORDER BY edge_uid LIMIT ?"
			: "SELECT * FROM extraction_edges WHERE snapshot_uid = ? ORDER BY edge_uid LIMIT ?";
		const params = afterEdgeUid
			? [snapshotUid, afterEdgeUid, limit]
			: [snapshotUid, limit];
		const rows = this.db.prepare(sql).all(...params) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapExtractionEdge(r));
	}

	queryExtractionEdgesByFiles(
		snapshotUid: string,
		sourceFileUids: string[],
	): ExtractionEdge[] {
		if (sourceFileUids.length === 0) return [];
		const placeholders = sourceFileUids.map(() => "?").join(",");
		const sql = `SELECT * FROM extraction_edges
		             WHERE snapshot_uid = ? AND source_file_uid IN (${placeholders})`;
		const rows = this.db
			.prepare(sql)
			.all(snapshotUid, ...sourceFileUids) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapExtractionEdge(r));
	}

	copyExtractionEdgesForFiles(
		fromSnapshotUid: string,
		toSnapshotUid: string,
		toRepoUid: string,
		sourceFileUids: string[],
	): number {
		if (sourceFileUids.length === 0) return 0;
		// Database-native copy: INSERT...SELECT with SQLite-generated UUIDs.
		// Uses lower(hex(randomblob(16))) to produce new edge_uid values
		// entirely in SQL — no JS materialization of the source row set.
		// This is critical for Linux-scale delta indexing where unchanged
		// files may have hundreds of thousands of raw edges.
		const placeholders = sourceFileUids.map(() => "?").join(",");
		const result = this.db.prepare(`
			INSERT INTO extraction_edges (
				edge_uid, snapshot_uid, repo_uid,
				source_node_uid, target_key, type, resolution, extractor,
				line_start, col_start, line_end, col_end,
				metadata_json, source_file_uid
			)
			SELECT
				lower(hex(randomblob(16))), ?, ?,
				source_node_uid, target_key, type, resolution, extractor,
				line_start, col_start, line_end, col_end,
				metadata_json, source_file_uid
			FROM extraction_edges
			WHERE snapshot_uid = ? AND source_file_uid IN (${placeholders})
		`).run(toSnapshotUid, toRepoUid, fromSnapshotUid, ...sourceFileUids);
		return result.changes;
	}

	private mapExtractionEdge(r: Record<string, unknown>): ExtractionEdge {
		return {
			edgeUid: r.edge_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceNodeUid: r.source_node_uid as string,
			targetKey: r.target_key as string,
			type: r.type as string,
			resolution: r.resolution as string,
			extractor: r.extractor as string,
			lineStart: (r.line_start as number) ?? null,
			colStart: (r.col_start as number) ?? null,
			lineEnd: (r.line_end as number) ?? null,
			colEnd: (r.col_end as number) ?? null,
			metadataJson: (r.metadata_json as string) ?? null,
			sourceFileUid: (r.source_file_uid as string) ?? null,
		};
	}

	insertFileSignals(signals: FileSignalRow[]): void {
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO file_signals
			 (snapshot_uid, file_uid, import_bindings_json,
			  package_dependencies_json, tsconfig_aliases_json)
			 VALUES (?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const s of signals) {
				stmt.run(
					s.snapshotUid,
					s.fileUid,
					s.importBindingsJson,
					s.packageDependenciesJson ?? null,
					s.tsconfigAliasesJson ?? null,
				);
			}
		});
		insertAll();
	}

	queryFileSignals(snapshotUid: string, fileUid: string): FileSignalRow | null {
		const row = this.db.prepare(
			"SELECT * FROM file_signals WHERE snapshot_uid = ? AND file_uid = ?",
		).get(snapshotUid, fileUid) as Record<string, unknown> | undefined;
		if (!row) return null;
		return {
			snapshotUid: row.snapshot_uid as string,
			fileUid: row.file_uid as string,
			importBindingsJson: (row.import_bindings_json as string) ?? null,
			packageDependenciesJson: (row.package_dependencies_json as string) ?? null,
			tsconfigAliasesJson: (row.tsconfig_aliases_json as string) ?? null,
		};
	}

	queryAllFileSignals(snapshotUid: string): FileSignalRow[] {
		const rows = this.db.prepare(
			"SELECT * FROM file_signals WHERE snapshot_uid = ?",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			snapshotUid: r.snapshot_uid as string,
			fileUid: r.file_uid as string,
			importBindingsJson: (r.import_bindings_json as string) ?? null,
			packageDependenciesJson: (r.package_dependencies_json as string) ?? null,
			tsconfigAliasesJson: (r.tsconfig_aliases_json as string) ?? null,
		}));
	}

	deleteFileSignals(snapshotUid: string): void {
		this.db.prepare("DELETE FROM file_signals WHERE snapshot_uid = ?")
			.run(snapshotUid);
	}

	queryFileSignalsBatch(
		snapshotUid: string,
		fileUids: string[],
	): FileSignalRow[] {
		if (fileUids.length === 0) return [];
		const placeholders = fileUids.map(() => "?").join(",");
		const sql = `SELECT * FROM file_signals
		             WHERE snapshot_uid = ? AND file_uid IN (${placeholders})`;
		const rows = this.db
			.prepare(sql)
			.all(snapshotUid, ...fileUids) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			snapshotUid: r.snapshot_uid as string,
			fileUid: r.file_uid as string,
			importBindingsJson: (r.import_bindings_json as string) ?? null,
			packageDependenciesJson: (r.package_dependencies_json as string) ?? null,
			tsconfigAliasesJson: (r.tsconfig_aliases_json as string) ?? null,
		}));
	}

	querySymbolsByFile(
		snapshotUid: string,
		fileUid: string,
	): FileSymbolRow[] {
		const rows = this.db.prepare(
			`SELECT stable_key, node_uid, name, qualified_name, subtype,
			        line_start, visibility
			 FROM nodes
			 WHERE snapshot_uid = ? AND file_uid = ? AND kind = 'SYMBOL'`,
		).all(snapshotUid, fileUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			stableKey: r.stable_key as string,
			nodeUid: r.node_uid as string,
			name: r.name as string,
			qualifiedName: (r.qualified_name as string) ?? r.name as string,
			subtype: (r.subtype as string) ?? null,
			lineStart: (r.line_start as number) ?? null,
			visibility: (r.visibility as string) ?? null,
		}));
	}

	// ── Boundary Facts (source of truth) ──────────────────────────────

	insertBoundaryProviderFacts(facts: PersistedBoundaryProviderFact[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO boundary_provider_facts
			 (fact_uid, snapshot_uid, repo_uid, mechanism, operation, address,
			  matcher_key, handler_stable_key, source_file, line_start,
			  framework, basis, schema_ref, metadata_json, extractor, observed_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const f of facts) {
				stmt.run(
					f.factUid,
					f.snapshotUid,
					f.repoUid,
					f.mechanism,
					f.operation,
					f.address,
					f.matcherKey,
					f.handlerStableKey,
					f.sourceFile,
					f.lineStart,
					f.framework,
					f.basis,
					f.schemaRef,
					f.metadata ? JSON.stringify(f.metadata) : null,
					f.extractor,
					f.observedAt,
				);
			}
		});
		insertAll();
	}

	insertBoundaryConsumerFacts(facts: PersistedBoundaryConsumerFact[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO boundary_consumer_facts
			 (fact_uid, snapshot_uid, repo_uid, mechanism, operation, address,
			  matcher_key, caller_stable_key, source_file, line_start,
			  basis, confidence, schema_ref, metadata_json, extractor, observed_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const f of facts) {
				stmt.run(
					f.factUid,
					f.snapshotUid,
					f.repoUid,
					f.mechanism,
					f.operation,
					f.address,
					f.matcherKey,
					f.callerStableKey,
					f.sourceFile,
					f.lineStart,
					f.basis,
					f.confidence,
					f.schemaRef,
					f.metadata ? JSON.stringify(f.metadata) : null,
					f.extractor,
					f.observedAt,
				);
			}
		});
		insertAll();
	}

	queryBoundaryProviderFacts(
		input: QueryBoundaryFactsInput,
	): PersistedBoundaryProviderFact[] {
		const conditions = ["snapshot_uid = ?"];
		const params: unknown[] = [input.snapshotUid];

		if (input.mechanism) {
			conditions.push("mechanism = ?");
			params.push(input.mechanism);
		}
		if (input.matcherKey) {
			conditions.push("matcher_key = ?");
			params.push(input.matcherKey);
		}
		if (input.sourceFile) {
			conditions.push("source_file = ?");
			params.push(input.sourceFile);
		}

		let sql = `SELECT * FROM boundary_provider_facts WHERE ${conditions.join(" AND ")} ORDER BY source_file ASC, line_start ASC`;
		if (input.limit) {
			sql += ` LIMIT ${input.limit}`;
		}

		const rows = this.db.prepare(sql).all(...params) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			factUid: r.fact_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			mechanism: r.mechanism as string,
			operation: r.operation as string,
			address: r.address as string,
			matcherKey: r.matcher_key as string,
			handlerStableKey: r.handler_stable_key as string,
			sourceFile: r.source_file as string,
			lineStart: r.line_start as number,
			framework: r.framework as string,
			basis: r.basis as string,
			schemaRef: (r.schema_ref as string) ?? null,
			metadata: r.metadata_json ? JSON.parse(r.metadata_json as string) : {},
			extractor: r.extractor as string,
			observedAt: r.observed_at as string,
		} as PersistedBoundaryProviderFact));
	}

	queryBoundaryConsumerFacts(
		input: QueryBoundaryFactsInput,
	): PersistedBoundaryConsumerFact[] {
		const conditions = ["snapshot_uid = ?"];
		const params: unknown[] = [input.snapshotUid];

		if (input.mechanism) {
			conditions.push("mechanism = ?");
			params.push(input.mechanism);
		}
		if (input.matcherKey) {
			conditions.push("matcher_key = ?");
			params.push(input.matcherKey);
		}
		if (input.sourceFile) {
			conditions.push("source_file = ?");
			params.push(input.sourceFile);
		}

		let sql = `SELECT * FROM boundary_consumer_facts WHERE ${conditions.join(" AND ")} ORDER BY source_file ASC, line_start ASC`;
		if (input.limit) {
			sql += ` LIMIT ${input.limit}`;
		}

		const rows = this.db.prepare(sql).all(...params) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			factUid: r.fact_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			mechanism: r.mechanism as string,
			operation: r.operation as string,
			address: r.address as string,
			matcherKey: r.matcher_key as string,
			callerStableKey: r.caller_stable_key as string,
			sourceFile: r.source_file as string,
			lineStart: r.line_start as number,
			basis: r.basis as string,
			confidence: r.confidence as number,
			schemaRef: (r.schema_ref as string) ?? null,
			metadata: r.metadata_json ? JSON.parse(r.metadata_json as string) : {},
			extractor: r.extractor as string,
			observedAt: r.observed_at as string,
		} as PersistedBoundaryConsumerFact));
	}

	// ── Boundary Links (derived artifact) ─────────────────────────────

	insertBoundaryLinks(links: PersistedBoundaryLink[]): void {
		const stmt = this.db.prepare(
			`INSERT INTO boundary_links
			 (link_uid, snapshot_uid, repo_uid, provider_fact_uid, consumer_fact_uid,
			  match_basis, confidence, metadata_json, materialized_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const l of links) {
				stmt.run(
					l.linkUid,
					l.snapshotUid,
					l.repoUid,
					l.providerFactUid,
					l.consumerFactUid,
					l.matchBasis,
					l.confidence,
					l.metadataJson,
					l.materializedAt,
				);
			}
		});
		insertAll();
	}

	queryBoundaryLinks(input: QueryBoundaryLinksInput): BoundaryLinkRow[] {
		const conditions = ["bl.snapshot_uid = ?"];
		const params: unknown[] = [input.snapshotUid];

		if (input.mechanism) {
			conditions.push("bp.mechanism = ?");
			params.push(input.mechanism);
		}
		if (input.matchBasis) {
			conditions.push("bl.match_basis = ?");
			params.push(input.matchBasis);
		}

		let sql = `
			SELECT
				bl.link_uid,
				bl.match_basis,
				bl.confidence,
				bl.metadata_json,
				bp.mechanism   AS provider_mechanism,
				bp.operation   AS provider_operation,
				bp.address     AS provider_address,
				bp.handler_stable_key AS provider_handler_stable_key,
				bp.source_file AS provider_source_file,
				bp.line_start  AS provider_line_start,
				bp.framework   AS provider_framework,
				bc.operation   AS consumer_operation,
				bc.address     AS consumer_address,
				bc.caller_stable_key AS consumer_caller_stable_key,
				bc.source_file AS consumer_source_file,
				bc.line_start  AS consumer_line_start,
				bc.basis       AS consumer_basis,
				bc.confidence  AS consumer_confidence
			FROM boundary_links bl
			JOIN boundary_provider_facts bp ON bp.fact_uid = bl.provider_fact_uid
			JOIN boundary_consumer_facts bc ON bc.fact_uid = bl.consumer_fact_uid
			WHERE ${conditions.join(" AND ")}
			ORDER BY bp.source_file ASC, bp.line_start ASC
		`;
		if (input.limit) {
			sql += ` LIMIT ${input.limit}`;
		}

		const rows = this.db.prepare(sql).all(...params) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			linkUid: r.link_uid as string,
			matchBasis: r.match_basis as string,
			confidence: r.confidence as number,
			providerMechanism: r.provider_mechanism as string,
			providerOperation: r.provider_operation as string,
			providerAddress: r.provider_address as string,
			providerHandlerStableKey: r.provider_handler_stable_key as string,
			providerSourceFile: r.provider_source_file as string,
			providerLineStart: r.provider_line_start as number,
			providerFramework: r.provider_framework as string,
			consumerOperation: r.consumer_operation as string,
			consumerAddress: r.consumer_address as string,
			consumerCallerStableKey: r.consumer_caller_stable_key as string,
			consumerSourceFile: r.consumer_source_file as string,
			consumerLineStart: r.consumer_line_start as number,
			consumerBasis: r.consumer_basis as string,
			consumerConfidence: r.consumer_confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	deleteBoundaryLinksBySnapshot(snapshotUid: string): void {
		this.db
			.prepare("DELETE FROM boundary_links WHERE snapshot_uid = ?")
			.run(snapshotUid);
	}

	deleteBoundaryFactsBySnapshot(snapshotUid: string): void {
		// Delete derived links first (FK constraint on fact tables).
		this.db
			.prepare("DELETE FROM boundary_links WHERE snapshot_uid = ?")
			.run(snapshotUid);
		this.db
			.prepare("DELETE FROM boundary_consumer_facts WHERE snapshot_uid = ?")
			.run(snapshotUid);
		this.db
			.prepare("DELETE FROM boundary_provider_facts WHERE snapshot_uid = ?")
			.run(snapshotUid);
	}

	// ── Module Candidates ──────────────────────────────────────────────

	insertModuleCandidates(candidates: ModuleCandidate[]): void {
		if (candidates.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO module_candidates
			 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
			  module_kind, canonical_root_path, confidence, display_name,
			  metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const c of candidates) {
				stmt.run(
					c.moduleCandidateUid,
					c.snapshotUid,
					c.repoUid,
					c.moduleKey,
					c.moduleKind,
					c.canonicalRootPath,
					c.confidence,
					c.displayName,
					c.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertModuleCandidateEvidence(evidence: ModuleCandidateEvidence[]): void {
		if (evidence.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO module_candidate_evidence
			 (evidence_uid, module_candidate_uid, snapshot_uid, repo_uid,
			  source_type, source_path, evidence_kind, confidence, payload_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of evidence) {
				stmt.run(
					e.evidenceUid,
					e.moduleCandidateUid,
					e.snapshotUid,
					e.repoUid,
					e.sourceType,
					e.sourcePath,
					e.evidenceKind,
					e.confidence,
					e.payloadJson,
				);
			}
		});
		insertAll();
	}

	insertModuleFileOwnership(ownership: ModuleFileOwnership[]): void {
		if (ownership.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO module_file_ownership
			 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
			  assignment_kind, confidence, basis_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const o of ownership) {
				stmt.run(
					o.snapshotUid,
					o.repoUid,
					o.fileUid,
					o.moduleCandidateUid,
					o.assignmentKind,
					o.confidence,
					o.basisJson,
				);
			}
		});
		insertAll();
	}

	queryModuleCandidates(snapshotUid: string): ModuleCandidate[] {
		const rows = this.db.prepare(
			"SELECT * FROM module_candidates WHERE snapshot_uid = ? ORDER BY canonical_root_path",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			moduleCandidateUid: r.module_candidate_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			moduleKey: r.module_key as string,
			moduleKind: r.module_kind as ModuleKind,
			canonicalRootPath: r.canonical_root_path as string,
			confidence: r.confidence as number,
			displayName: (r.display_name as string) ?? null,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	queryModuleCandidateEvidence(moduleCandidateUid: string): ModuleCandidateEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM module_candidate_evidence WHERE module_candidate_uid = ?",
		).all(moduleCandidateUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			evidenceUid: r.evidence_uid as string,
			moduleCandidateUid: r.module_candidate_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceType: r.source_type as EvidenceSourceType,
			sourcePath: r.source_path as string,
			evidenceKind: r.evidence_kind as EvidenceKind,
			confidence: r.confidence as number,
			payloadJson: (r.payload_json as string) ?? null,
		}));
	}

	queryAllModuleCandidateEvidence(snapshotUid: string): ModuleCandidateEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM module_candidate_evidence WHERE snapshot_uid = ?",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			evidenceUid: r.evidence_uid as string,
			moduleCandidateUid: r.module_candidate_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceType: r.source_type as EvidenceSourceType,
			sourcePath: r.source_path as string,
			evidenceKind: r.evidence_kind as EvidenceKind,
			confidence: r.confidence as number,
			payloadJson: (r.payload_json as string) ?? null,
		}));
	}

	queryModuleFileOwnership(snapshotUid: string): ModuleFileOwnership[] {
		const rows = this.db.prepare(
			"SELECT * FROM module_file_ownership WHERE snapshot_uid = ? ORDER BY file_uid",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			fileUid: r.file_uid as string,
			moduleCandidateUid: r.module_candidate_uid as string,
			assignmentKind: r.assignment_kind as AssignmentKind,
			confidence: r.confidence as number,
			basisJson: (r.basis_json as string) ?? null,
		}));
	}

	// ── Module Discovery Diagnostics ──────────────────────────────────

	insertModuleDiscoveryDiagnostics(
		diagnostics: ModuleDiscoveryDiagnosticInput[],
	): void {
		if (diagnostics.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR IGNORE INTO module_discovery_diagnostics
			 (diagnostic_uid, snapshot_uid, source_type, diagnostic_kind,
			  file_path, line, raw_text, message, severity, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const d of diagnostics) {
				const uid = buildDiagnosticUid(d);
				stmt.run(
					uid,
					d.snapshotUid,
					d.sourceType,
					d.diagnosticKind,
					d.filePath,
					d.line,
					d.rawText,
					d.message,
					d.severity,
					d.metadataJson,
				);
			}
		});
		insertAll();
	}

	queryModuleDiscoveryDiagnostics(
		snapshotUid: string,
		filters?: { sourceType?: string; diagnosticKind?: string },
	): ModuleDiscoveryDiagnostic[] {
		let sql = `SELECT * FROM module_discovery_diagnostics WHERE snapshot_uid = ?`;
		const params: unknown[] = [snapshotUid];

		if (filters?.sourceType) {
			sql += ` AND source_type = ?`;
			params.push(filters.sourceType);
		}
		if (filters?.diagnosticKind) {
			sql += ` AND diagnostic_kind = ?`;
			params.push(filters.diagnosticKind);
		}
		sql += ` ORDER BY file_path, line`;

		const rows = this.db.prepare(sql).all(...params) as Array<
			Record<string, unknown>
		>;
		return rows.map((r) => ({
			diagnosticUid: r.diagnostic_uid as string,
			snapshotUid: r.snapshot_uid as string,
			sourceType: r.source_type as DiagnosticSourceType,
			diagnosticKind: r.diagnostic_kind as DiagnosticKind,
			filePath: r.file_path as string,
			line: (r.line as number) ?? null,
			rawText: (r.raw_text as string) ?? null,
			message: r.message as string,
			severity: r.severity as DiagnosticSeverity,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	/**
	 * Query resolved IMPORTS edges with source/target file UIDs.
	 *
	 * Returns only edges where both source and target nodes have a file_uid.
	 * Used for module dependency edge derivation.
	 *
	 * See docs/architecture/module-graph-contract.txt.
	 */
	queryResolvedImportsWithFiles(snapshotUid: string): Array<{
		sourceFileUid: string;
		targetFileUid: string;
	}> {
		const rows = this.db.prepare(`
			SELECT
				src.file_uid AS source_file_uid,
				tgt.file_uid AS target_file_uid
			FROM edges e
			JOIN nodes src ON e.source_node_uid = src.node_uid
			JOIN nodes tgt ON e.target_node_uid = tgt.node_uid
			WHERE e.snapshot_uid = ?
			  AND e.type = 'IMPORTS'
			  AND src.file_uid IS NOT NULL
			  AND tgt.file_uid IS NOT NULL
		`).all(snapshotUid) as Array<Record<string, unknown>>;

		return rows.map((r) => ({
			sourceFileUid: r.source_file_uid as string,
			targetFileUid: r.target_file_uid as string,
		}));
	}

	queryModuleCandidateRollups(snapshotUid: string): ModuleCandidateRollup[] {
		const rows = this.db.prepare(`
			SELECT
				mc.module_candidate_uid,
				mc.module_key,
				mc.canonical_root_path,
				mc.display_name,
				mc.module_kind,
				mc.confidence,
				COALESCE(ownership.file_count, 0) AS file_count,
				COALESCE(ownership.test_file_count, 0) AS test_file_count,
				COALESCE(symbols.symbol_count, 0) AS symbol_count,
				COALESCE(evidence.evidence_count, 0) AS evidence_count,
				COALESCE(ownership.languages, '') AS languages,
				CASE WHEN dir_mod.node_uid IS NOT NULL THEN 1 ELSE 0 END AS has_directory_module,
				COALESCE(evidence.evidence_sources, '') AS evidence_sources,
				evidence.primary_source AS primary_source
			FROM module_candidates mc
			LEFT JOIN (
				SELECT
					mfo.module_candidate_uid,
					COUNT(*) AS file_count,
					SUM(CASE WHEN f.is_test = 1 THEN 1 ELSE 0 END) AS test_file_count,
					GROUP_CONCAT(DISTINCT f.language ORDER BY f.language) AS languages
				FROM module_file_ownership mfo
				JOIN files f ON f.file_uid = mfo.file_uid
				WHERE mfo.snapshot_uid = ?
				GROUP BY mfo.module_candidate_uid
			) ownership ON ownership.module_candidate_uid = mc.module_candidate_uid
			LEFT JOIN (
				SELECT
					mfo.module_candidate_uid,
					COUNT(*) AS symbol_count
				FROM module_file_ownership mfo
				JOIN nodes n ON n.file_uid = mfo.file_uid AND n.snapshot_uid = mfo.snapshot_uid AND n.kind = 'SYMBOL'
				WHERE mfo.snapshot_uid = ?
				GROUP BY mfo.module_candidate_uid
			) symbols ON symbols.module_candidate_uid = mc.module_candidate_uid
			LEFT JOIN (
				SELECT
					module_candidate_uid,
					COUNT(*) AS evidence_count,
					-- Deterministic ordering: alphabetical by source_type
					GROUP_CONCAT(DISTINCT source_type ORDER BY source_type) AS evidence_sources,
					(
						-- Primary source: highest confidence, alphabetical tie-breaker
						SELECT source_type
						FROM module_candidate_evidence e2
						WHERE e2.module_candidate_uid = module_candidate_evidence.module_candidate_uid
						  AND e2.snapshot_uid = module_candidate_evidence.snapshot_uid
						ORDER BY e2.confidence DESC, e2.source_type ASC
						LIMIT 1
					) AS primary_source
				FROM module_candidate_evidence
				WHERE snapshot_uid = ?
				GROUP BY module_candidate_uid
			) evidence ON evidence.module_candidate_uid = mc.module_candidate_uid
			LEFT JOIN nodes dir_mod
				ON dir_mod.snapshot_uid = mc.snapshot_uid
				AND dir_mod.kind = 'MODULE'
				AND dir_mod.qualified_name = mc.canonical_root_path
			WHERE mc.snapshot_uid = ?
			ORDER BY mc.canonical_root_path
		`).all(snapshotUid, snapshotUid, snapshotUid, snapshotUid) as Array<Record<string, unknown>>;

		return rows.map((r) => {
			const moduleKind = r.module_kind as string;
			const evidenceSourcesStr = (r.evidence_sources as string) ?? "";
			return {
				moduleCandidateUid: r.module_candidate_uid as string,
				moduleKey: r.module_key as string,
				canonicalRootPath: r.canonical_root_path as string,
				displayName: (r.display_name as string) ?? null,
				moduleKind,
				confidence: r.confidence as number,
				fileCount: r.file_count as number,
				symbolCount: r.symbol_count as number,
				testFileCount: r.test_file_count as number,
				evidenceCount: r.evidence_count as number,
				languages: (r.languages as string) ?? "",
				hasDirectoryModule: (r.has_directory_module as number) === 1,
				// Layer 3 visibility fields
				evidenceSources: evidenceSourcesStr ? evidenceSourcesStr.split(",") : [],
				primarySource: (r.primary_source as string) ?? null,
				isInferred: moduleKind === "inferred",
			};
		});
	}

	queryModuleOwnedFiles(
		snapshotUid: string,
		moduleCandidateUid: string,
		limit?: number,
	): ModuleOwnedFileRow[] {
		const limitClause = limit ? `LIMIT ${limit}` : "";
		const rows = this.db.prepare(`
			SELECT
				mfo.file_uid,
				f.path AS file_path,
				f.language,
				f.is_test,
				mfo.assignment_kind,
				mfo.confidence
			FROM module_file_ownership mfo
			JOIN files f ON f.file_uid = mfo.file_uid
			WHERE mfo.snapshot_uid = ?
			  AND mfo.module_candidate_uid = ?
			ORDER BY f.path
			${limitClause}
		`).all(snapshotUid, moduleCandidateUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			fileUid: r.file_uid as string,
			filePath: r.file_path as string,
			language: (r.language as string) ?? null,
			isTest: (r.is_test as number) === 1,
			assignmentKind: r.assignment_kind as string,
			confidence: r.confidence as number,
		}));
	}

	// ── Delta indexing copy-forward ────────────────────────────────

	queryFileVersionHashes(snapshotUid: string): Map<string, string> {
		const rows = this.db.prepare(
			"SELECT file_uid, content_hash FROM file_versions WHERE snapshot_uid = ?",
		).all(snapshotUid) as Array<{ file_uid: string; content_hash: string }>;
		const map = new Map<string, string>();
		for (const r of rows) {
			map.set(r.file_uid, r.content_hash);
		}
		return map;
	}

	copyForwardUnchangedFiles(input: CopyForwardInput): CopyForwardResult {
		if (input.fileUids.length === 0) {
			return { nodesCopied: 0, extractionEdgesCopied: 0, fileSignalsCopied: 0, fileVersionsCopied: 0 };
		}

		const { fromSnapshotUid, toSnapshotUid, repoUid, fileUids } = input;
		const placeholders = fileUids.map(() => "?").join(",");

		let nodesCopied = 0;
		let extractionEdgesCopied = 0;
		let fileSignalsCopied = 0;
		let fileVersionsCopied = 0;

		const copyAll = this.db.transaction(() => {
			// 1. Build node_uid mapping (old → new) for files being copied.
			this.db.exec("CREATE TEMP TABLE IF NOT EXISTS _node_uid_map (old_uid TEXT PRIMARY KEY, new_uid TEXT NOT NULL)");
			this.db.exec("DELETE FROM _node_uid_map");

			this.db.prepare(`
				INSERT INTO _node_uid_map (old_uid, new_uid)
				SELECT node_uid, lower(hex(randomblob(16)))
				FROM nodes
				WHERE snapshot_uid = ? AND file_uid IN (${placeholders})
			`).run(fromSnapshotUid, ...fileUids);

			// 2. Copy nodes with new UIDs.
			const nodeResult = this.db.prepare(`
				INSERT INTO nodes (
					node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
					name, qualified_name, file_uid, parent_node_uid,
					line_start, col_start, line_end, col_end,
					signature, visibility, doc_comment, metadata_json
				)
				SELECT
					m.new_uid, ?, n.repo_uid, n.stable_key, n.kind, n.subtype,
					n.name, n.qualified_name, n.file_uid, n.parent_node_uid,
					n.line_start, n.col_start, n.line_end, n.col_end,
					n.signature, n.visibility, n.doc_comment, n.metadata_json
				FROM nodes n
				JOIN _node_uid_map m ON n.node_uid = m.old_uid
			`).run(toSnapshotUid);
			nodesCopied = nodeResult.changes;

			// 3. Copy extraction_edges with new edge_uids and remapped source_node_uids.
			const edgeResult = this.db.prepare(`
				INSERT INTO extraction_edges (
					edge_uid, snapshot_uid, repo_uid,
					source_node_uid, target_key, type, resolution, extractor,
					line_start, col_start, line_end, col_end,
					metadata_json, source_file_uid
				)
				SELECT
					lower(hex(randomblob(16))), ?, ?,
					COALESCE(m.new_uid, e.source_node_uid),
					e.target_key, e.type, e.resolution, e.extractor,
					e.line_start, e.col_start, e.line_end, e.col_end,
					e.metadata_json, e.source_file_uid
				FROM extraction_edges e
				LEFT JOIN _node_uid_map m ON e.source_node_uid = m.old_uid
				WHERE e.snapshot_uid = ? AND e.source_file_uid IN (${placeholders})
			`).run(toSnapshotUid, repoUid, fromSnapshotUid, ...fileUids);
			extractionEdgesCopied = edgeResult.changes;

			// 4. Copy file_signals (no node_uid references).
			const signalResult = this.db.prepare(`
				INSERT INTO file_signals (
					snapshot_uid, file_uid, import_bindings_json,
					package_dependencies_json, tsconfig_aliases_json
				)
				SELECT
					?, file_uid, import_bindings_json,
					package_dependencies_json, tsconfig_aliases_json
				FROM file_signals
				WHERE snapshot_uid = ? AND file_uid IN (${placeholders})
			`).run(toSnapshotUid, fromSnapshotUid, ...fileUids);
			fileSignalsCopied = signalResult.changes;

			// 5. Copy file_versions (no node_uid references).
			const versionResult = this.db.prepare(`
				INSERT INTO file_versions (
					snapshot_uid, file_uid, content_hash, ast_hash,
					extractor, parse_status, size_bytes, line_count, indexed_at
				)
				SELECT
					?, file_uid, content_hash, ast_hash,
					extractor, parse_status, size_bytes, line_count, indexed_at
				FROM file_versions
				WHERE snapshot_uid = ? AND file_uid IN (${placeholders})
			`).run(toSnapshotUid, fromSnapshotUid, ...fileUids);
			fileVersionsCopied = versionResult.changes;

			// Cleanup temp table.
			this.db.exec("DELETE FROM _node_uid_map");
		});

		copyAll();

		return { nodesCopied, extractionEdgesCopied, fileSignalsCopied, fileVersionsCopied };
	}

	deleteModuleCandidatesBySnapshot(snapshotUid: string): void {
		// CASCADE handles evidence and ownership rows.
		this.db.prepare("DELETE FROM module_candidates WHERE snapshot_uid = ?")
			.run(snapshotUid);
	}

	// ── Project Surfaces ───────────────────────────────────────────

	insertProjectSurfaces(surfaces: ProjectSurface[]): void {
		if (surfaces.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO project_surfaces
			 (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
			  surface_kind, display_name, root_path, entrypoint_path,
			  build_system, runtime_kind, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const s of surfaces) {
				stmt.run(
					s.projectSurfaceUid, s.snapshotUid, s.repoUid, s.moduleCandidateUid,
					s.surfaceKind, s.displayName, s.rootPath, s.entrypointPath,
					s.buildSystem, s.runtimeKind, s.confidence, s.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertProjectSurfaceEvidence(evidence: ProjectSurfaceEvidence[]): void {
		if (evidence.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO project_surface_evidence
			 (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid,
			  source_type, source_path, evidence_kind, confidence, payload_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of evidence) {
				stmt.run(
					e.projectSurfaceEvidenceUid, e.projectSurfaceUid,
					e.snapshotUid, e.repoUid,
					e.sourceType, e.sourcePath, e.evidenceKind, e.confidence,
					e.payloadJson,
				);
			}
		});
		insertAll();
	}

	queryProjectSurfaces(snapshotUid: string): ProjectSurface[] {
		const rows = this.db.prepare(
			"SELECT * FROM project_surfaces WHERE snapshot_uid = ? ORDER BY root_path, surface_kind",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			moduleCandidateUid: r.module_candidate_uid as string,
			surfaceKind: r.surface_kind as SurfaceKind,
			displayName: (r.display_name as string) ?? null,
			rootPath: r.root_path as string,
			entrypointPath: (r.entrypoint_path as string) ?? null,
			buildSystem: r.build_system as BuildSystem,
			runtimeKind: r.runtime_kind as RuntimeKind,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	queryProjectSurfaceEvidence(projectSurfaceUid: string): ProjectSurfaceEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM project_surface_evidence WHERE project_surface_uid = ?",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			projectSurfaceEvidenceUid: r.project_surface_evidence_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceType: r.source_type as SurfaceEvidenceSourceType,
			sourcePath: r.source_path as string,
			evidenceKind: r.evidence_kind as SurfaceEvidenceKind,
			confidence: r.confidence as number,
			payloadJson: (r.payload_json as string) ?? null,
		}));
	}

	queryAllProjectSurfaceEvidence(snapshotUid: string): ProjectSurfaceEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM project_surface_evidence WHERE snapshot_uid = ?",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			projectSurfaceEvidenceUid: r.project_surface_evidence_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceType: r.source_type as SurfaceEvidenceSourceType,
			sourcePath: r.source_path as string,
			evidenceKind: r.evidence_kind as SurfaceEvidenceKind,
			confidence: r.confidence as number,
			payloadJson: (r.payload_json as string) ?? null,
		}));
	}

	// ── Topology Links ─────────────────────────────────────────────────

	insertSurfaceConfigRoots(roots: SurfaceConfigRoot[]): void {
		if (roots.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_config_roots
			 (surface_config_root_uid, project_surface_uid, snapshot_uid, repo_uid,
			  config_path, config_kind, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const r of roots) {
				stmt.run(
					r.surfaceConfigRootUid, r.projectSurfaceUid,
					r.snapshotUid, r.repoUid,
					r.configPath, r.configKind, r.confidence, r.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertSurfaceEntrypoints(entrypoints: SurfaceEntrypoint[]): void {
		if (entrypoints.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_entrypoints
			 (surface_entrypoint_uid, project_surface_uid, snapshot_uid, repo_uid,
			  entrypoint_path, entrypoint_target, entrypoint_kind,
			  display_name, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of entrypoints) {
				stmt.run(
					e.surfaceEntrypointUid, e.projectSurfaceUid,
					e.snapshotUid, e.repoUid,
					e.entrypointPath, e.entrypointTarget, e.entrypointKind,
					e.displayName, e.confidence, e.metadataJson,
				);
			}
		});
		insertAll();
	}

	querySurfaceConfigRoots(projectSurfaceUid: string): SurfaceConfigRoot[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_config_roots WHERE project_surface_uid = ? ORDER BY config_path",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceConfigRootUid: r.surface_config_root_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			configPath: r.config_path as string,
			configKind: r.config_kind as ConfigRootKind,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	queryAllSurfaceConfigRoots(snapshotUid: string): SurfaceConfigRoot[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_config_roots WHERE snapshot_uid = ? ORDER BY config_path",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceConfigRootUid: r.surface_config_root_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			configPath: r.config_path as string,
			configKind: r.config_kind as ConfigRootKind,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	querySurfaceEntrypoints(projectSurfaceUid: string): SurfaceEntrypoint[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_entrypoints WHERE project_surface_uid = ?",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceEntrypointUid: r.surface_entrypoint_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			entrypointPath: (r.entrypoint_path as string) ?? null,
			entrypointTarget: (r.entrypoint_target as string) ?? null,
			entrypointKind: r.entrypoint_kind as EntrypointKind,
			displayName: (r.display_name as string) ?? null,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	queryAllSurfaceEntrypoints(snapshotUid: string): SurfaceEntrypoint[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_entrypoints WHERE snapshot_uid = ? ORDER BY entrypoint_path",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceEntrypointUid: r.surface_entrypoint_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			entrypointPath: (r.entrypoint_path as string) ?? null,
			entrypointTarget: (r.entrypoint_target as string) ?? null,
			entrypointKind: r.entrypoint_kind as EntrypointKind,
			displayName: (r.display_name as string) ?? null,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	// ── Env Dependencies ──────────────────────────────────────────────

	insertSurfaceEnvDependencies(deps: SurfaceEnvDependency[]): void {
		if (deps.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_env_dependencies
			 (surface_env_dependency_uid, snapshot_uid, repo_uid, project_surface_uid,
			  env_name, access_kind, default_value, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const d of deps) {
				stmt.run(
					d.surfaceEnvDependencyUid, d.snapshotUid, d.repoUid, d.projectSurfaceUid,
					d.envName, d.accessKind, d.defaultValue, d.confidence, d.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertSurfaceEnvEvidence(evidence: SurfaceEnvEvidence[]): void {
		if (evidence.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_env_evidence
			 (surface_env_evidence_uid, surface_env_dependency_uid, snapshot_uid, repo_uid,
			  source_file_path, line_number, access_pattern, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of evidence) {
				stmt.run(
					e.surfaceEnvEvidenceUid, e.surfaceEnvDependencyUid,
					e.snapshotUid, e.repoUid,
					e.sourceFilePath, e.lineNumber, e.accessPattern, e.confidence,
					e.metadataJson,
				);
			}
		});
		insertAll();
	}

	querySurfaceEnvDependencies(projectSurfaceUid: string): SurfaceEnvDependency[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_env_dependencies WHERE project_surface_uid = ? ORDER BY env_name",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceEnvDependencyUid: r.surface_env_dependency_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			envName: r.env_name as string,
			accessKind: r.access_kind as EnvAccessKind,
			defaultValue: (r.default_value as string) ?? null,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	queryAllSurfaceEnvDependencies(snapshotUid: string): SurfaceEnvDependency[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_env_dependencies WHERE snapshot_uid = ? ORDER BY env_name",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => ({
			surfaceEnvDependencyUid: r.surface_env_dependency_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			envName: r.env_name as string,
			accessKind: r.access_kind as EnvAccessKind,
			defaultValue: (r.default_value as string) ?? null,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		}));
	}

	querySurfaceEnvEvidence(surfaceEnvDependencyUid: string): SurfaceEnvEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_env_evidence WHERE surface_env_dependency_uid = ?",
		).all(surfaceEnvDependencyUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapEnvEvidenceRow(r));
	}

	querySurfaceEnvEvidenceBySurface(
		projectSurfaceUid: string,
	): SurfaceEnvEvidence[] {
		// Env evidence has no denormalized project_surface_uid column
		// because every evidence row is always linked to a dependency
		// identity row (no dynamic-null-FK case as on the fs side).
		// JOIN through surface_env_dependencies to scope by surface.
		const rows = this.db.prepare(
			`SELECT e.*
			   FROM surface_env_evidence e
			   JOIN surface_env_dependencies d
			     ON e.surface_env_dependency_uid = d.surface_env_dependency_uid
			  WHERE d.project_surface_uid = ?
			  ORDER BY e.source_file_path, e.line_number`,
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapEnvEvidenceRow(r));
	}

	private mapEnvEvidenceRow(r: Record<string, unknown>): SurfaceEnvEvidence {
		return {
			surfaceEnvEvidenceUid: r.surface_env_evidence_uid as string,
			surfaceEnvDependencyUid: r.surface_env_dependency_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			sourceFilePath: r.source_file_path as string,
			lineNumber: r.line_number as number,
			accessPattern: r.access_pattern as EnvAccessPattern,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		};
	}

	// ── FS Mutations ────────────────────────────────────────────────

	insertSurfaceFsMutations(mutations: SurfaceFsMutation[]): void {
		if (mutations.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_fs_mutations
			 (surface_fs_mutation_uid, snapshot_uid, repo_uid, project_surface_uid,
			  target_path, mutation_kind, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const m of mutations) {
				stmt.run(
					m.surfaceFsMutationUid, m.snapshotUid, m.repoUid, m.projectSurfaceUid,
					m.targetPath, m.mutationKind, m.confidence, m.metadataJson,
				);
			}
		});
		insertAll();
	}

	insertSurfaceFsMutationEvidence(evidence: SurfaceFsMutationEvidence[]): void {
		if (evidence.length === 0) return;
		const stmt = this.db.prepare(
			`INSERT OR REPLACE INTO surface_fs_mutation_evidence
			 (surface_fs_mutation_evidence_uid, surface_fs_mutation_uid, snapshot_uid,
			  repo_uid, project_surface_uid, source_file_path, line_number,
			  mutation_kind, mutation_pattern, dynamic_path, confidence, metadata_json)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		);
		const insertAll = this.db.transaction(() => {
			for (const e of evidence) {
				stmt.run(
					e.surfaceFsMutationEvidenceUid,
					e.surfaceFsMutationUid,
					e.snapshotUid,
					e.repoUid,
					e.projectSurfaceUid,
					e.sourceFilePath,
					e.lineNumber,
					e.mutationKind,
					e.mutationPattern,
					e.dynamicPath ? 1 : 0,
					e.confidence,
					e.metadataJson,
				);
			}
		});
		insertAll();
	}

	querySurfaceFsMutations(projectSurfaceUid: string): SurfaceFsMutation[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_fs_mutations WHERE project_surface_uid = ? ORDER BY target_path, mutation_kind",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapFsMutationRow(r));
	}

	queryAllSurfaceFsMutations(snapshotUid: string): SurfaceFsMutation[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_fs_mutations WHERE snapshot_uid = ? ORDER BY target_path, mutation_kind",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapFsMutationRow(r));
	}

	querySurfaceFsMutationEvidence(surfaceFsMutationUid: string): SurfaceFsMutationEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_fs_mutation_evidence WHERE surface_fs_mutation_uid = ?",
		).all(surfaceFsMutationUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapFsMutationEvidenceRow(r));
	}

	querySurfaceFsMutationEvidenceBySurface(
		projectSurfaceUid: string,
	): SurfaceFsMutationEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_fs_mutation_evidence WHERE project_surface_uid = ? ORDER BY source_file_path, line_number",
		).all(projectSurfaceUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapFsMutationEvidenceRow(r));
	}

	queryAllSurfaceFsMutationEvidence(snapshotUid: string): SurfaceFsMutationEvidence[] {
		const rows = this.db.prepare(
			"SELECT * FROM surface_fs_mutation_evidence WHERE snapshot_uid = ?",
		).all(snapshotUid) as Array<Record<string, unknown>>;
		return rows.map((r) => this.mapFsMutationEvidenceRow(r));
	}

	private mapFsMutationRow(r: Record<string, unknown>): SurfaceFsMutation {
		return {
			surfaceFsMutationUid: r.surface_fs_mutation_uid as string,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			targetPath: r.target_path as string,
			mutationKind: r.mutation_kind as MutationKind,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		};
	}

	private mapFsMutationEvidenceRow(r: Record<string, unknown>): SurfaceFsMutationEvidence {
		return {
			surfaceFsMutationEvidenceUid: r.surface_fs_mutation_evidence_uid as string,
			surfaceFsMutationUid: (r.surface_fs_mutation_uid as string) ?? null,
			snapshotUid: r.snapshot_uid as string,
			repoUid: r.repo_uid as string,
			projectSurfaceUid: r.project_surface_uid as string,
			sourceFilePath: r.source_file_path as string,
			lineNumber: r.line_number as number,
			mutationKind: r.mutation_kind as MutationKind,
			mutationPattern: r.mutation_pattern as MutationPattern,
			dynamicPath: (r.dynamic_path as number) === 1,
			confidence: r.confidence as number,
			metadataJson: (r.metadata_json as string) ?? null,
		};
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
		//
		// Framework-liveness inferences also suppress dead-code:
		//   - "framework_entrypoint" (Lambda handlers)
		//   - "spring_container_managed" (Spring beans, @Configuration, @Bean)
		params.push(input.snapshotUid);
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
			  AND n.stable_key NOT IN (
				SELECT i.target_stable_key FROM inferences i
				WHERE i.snapshot_uid = ?
				  AND i.kind IN ('framework_entrypoint', 'spring_container_managed', 'pytest_test', 'pytest_fixture', 'linux_system_managed')
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
