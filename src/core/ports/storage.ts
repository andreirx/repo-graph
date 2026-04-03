import type {
	CycleResult,
	DeadNodeResult,
	Declaration,
	EdgeType,
	FileVersion,
	GraphEdge,
	GraphNode,
	ModuleStats,
	NodeKind,
	NodeResult,
	PathResult,
	Repo,
	Snapshot,
	SnapshotKind,
	SnapshotStatus,
	TrackedFile,
} from "../model/index.js";

/**
 * Storage port — the contract any storage backend must fulfill.
 *
 * Methods are domain query contracts, not CLI-shaped.
 * Recursive graph traversals (callers, callees, path, cycles)
 * are storage responsibilities because they benefit from
 * database-native recursive CTEs.
 *
 * Input objects prevent method explosion as filters evolve.
 */
export interface StoragePort {
	// ── Lifecycle ────────────────────────────────────────────────────────

	/** Initialize the storage backend (create tables, run migrations). */
	initialize(): void;

	/** Close the storage backend and release resources. */
	close(): void;

	// ── Repos ────────────────────────────────────────────────────────────

	addRepo(repo: Repo): void;
	getRepo(ref: RepoRef): Repo | null;
	listRepos(): Repo[];
	removeRepo(repoUid: string): void;

	// ── Snapshots ────────────────────────────────────────────────────────

	createSnapshot(input: CreateSnapshotInput): Snapshot;
	getSnapshot(snapshotUid: string): Snapshot | null;
	getLatestSnapshot(repoUid: string): Snapshot | null;
	updateSnapshotStatus(input: UpdateSnapshotStatusInput): void;
	updateSnapshotCounts(snapshotUid: string): void;

	// ── Files ────────────────────────────────────────────────────────────

	upsertFiles(files: TrackedFile[]): void;
	upsertFileVersions(fileVersions: FileVersion[]): void;
	getFilesByRepo(repoUid: string): TrackedFile[];
	getStaleFiles(snapshotUid: string): TrackedFile[];

	// ── Nodes & Edges (bulk insert for indexing) ─────────────────────────

	insertNodes(nodes: GraphNode[]): void;
	insertEdges(edges: GraphEdge[]): void;

	/** Remove all nodes and edges for a given file in a given snapshot. */
	deleteNodesByFile(snapshotUid: string, fileUid: string): void;

	// ── Declarations ─────────────────────────────────────────────────────

	insertDeclaration(declaration: Declaration): void;
	getActiveDeclarations(input: GetDeclarationsInput): Declaration[];
	deactivateDeclaration(declarationUid: string): void;

	// ── Graph Queries ────────────────────────────────────────────────────

	getNodeByStableKey(snapshotUid: string, stableKey: string): GraphNode | null;

	/** Resolve a partial symbol name to matching stable keys. */
	resolveSymbol(input: ResolveSymbolInput): GraphNode[];

	findCallers(input: FindTraversalInput): NodeResult[];
	findCallees(input: FindTraversalInput): NodeResult[];
	findImports(input: FindImportsInput): NodeResult[];
	findPath(input: FindPathInput): PathResult;
	findDeadNodes(input: FindDeadNodesInput): DeadNodeResult[];
	findCycles(input: FindCyclesInput): CycleResult[];

	/**
	 * Find all IMPORTS edges where the source file is under sourcePrefix
	 * and the target file is under targetPrefix.
	 *
	 * Used for boundary violation detection: given a forbidden dependency
	 * (moduleA must not import moduleB), returns the specific file-level
	 * imports that cross the boundary.
	 */
	findImportsBetweenPaths(
		input: FindImportsBetweenPathsInput,
	): ImportEdgeResult[];

	/** Compute structural metrics for all modules in a snapshot. */
	computeModuleStats(snapshotUid: string): ModuleStats[];

	/** Query function-level metrics from the measurements table. */
	queryFunctionMetrics(input: QueryFunctionMetricsInput): FunctionMetricRow[];

	/** Query per-module complexity aggregates from measurements. */
	queryModuleMetricAggregates(snapshotUid: string): ModuleMetricAggregate[];

	/** Insert measurements (batch). */
	insertMeasurements(measurements: Measurement[]): void;
}

// ── Input types ──────────────────────────────────────────────────────────

/** Identify a repo by UID, name, or root path. */
export type RepoRef = { uid: string } | { name: string } | { rootPath: string };

export interface CreateSnapshotInput {
	repoUid: string;
	kind: SnapshotKind;
	basisRef?: string;
	basisCommit?: string;
	parentSnapshotUid?: string;
	label?: string;
	/** Snapshot-level toolchain provenance JSON. */
	toolchainJson?: string;
}

export interface UpdateSnapshotStatusInput {
	snapshotUid: string;
	status: SnapshotStatus;
	completedAt?: string;
}

export interface GetDeclarationsInput {
	repoUid: string;
	kind?: string;
	targetStableKey?: string;
}

export interface ResolveSymbolInput {
	snapshotUid: string;
	/** Partial or full qualified name to match against. */
	query: string;
	/** Limit results. */
	limit?: number;
}

export interface FindTraversalInput {
	snapshotUid: string;
	/** Stable key of the target symbol. */
	stableKey: string;
	/** Maximum traversal depth. Default: 1 (direct only). */
	maxDepth?: number;
	/** Filter to specific edge types. Default: [CALLS]. */
	edgeTypes?: EdgeType[];
}

export interface FindImportsInput {
	snapshotUid: string;
	/** Stable key of the file or module. */
	stableKey: string;
	/** Maximum traversal depth. Default: 1. */
	maxDepth?: number;
}

export interface FindPathInput {
	snapshotUid: string;
	fromStableKey: string;
	toStableKey: string;
	/** Maximum traversal depth. Default: 8. */
	maxDepth?: number;
	/** Edge types to traverse. Default: [CALLS, IMPORTS]. */
	edgeTypes?: EdgeType[];
}

export interface FindDeadNodesInput {
	snapshotUid: string;
	/** Filter to specific node kinds. */
	kind?: NodeKind;
	/** Only return nodes with at least this many lines. */
	minLines?: number;
}

export interface FindCyclesInput {
	snapshotUid: string;
	/** Detection granularity. Default: MODULE. */
	level?: "file" | "module";
}

export interface FindImportsBetweenPathsInput {
	snapshotUid: string;
	/** Source path prefix (e.g. "src/core"). Files under this prefix. */
	sourcePrefix: string;
	/** Target path prefix (e.g. "src/adapters"). Files under this prefix. */
	targetPrefix: string;
}

export interface ImportEdgeResult {
	sourceFile: string;
	targetFile: string;
	line: number | null;
}

export interface Measurement {
	measurementUid: string;
	snapshotUid: string;
	repoUid: string;
	targetStableKey: string;
	kind: string;
	valueJson: string;
	source: string;
	createdAt: string;
}

export interface QueryFunctionMetricsInput {
	snapshotUid: string;
	/** Sort field. Default: cyclomatic_complexity. */
	sortBy?: "cyclomatic_complexity" | "parameter_count" | "max_nesting_depth";
	/** Max results to return. Default: all. */
	limit?: number;
}

export interface FunctionMetricRow {
	stableKey: string;
	symbol: string;
	file: string;
	line: number | null;
	cyclomaticComplexity: number;
	parameterCount: number;
	maxNestingDepth: number;
}

export interface ModuleMetricAggregate {
	modulePath: string;
	functionCount: number;
	avgCyclomaticComplexity: number;
	maxCyclomaticComplexity: number;
	avgNestingDepth: number;
	maxNestingDepth: number;
}
