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
	/**
	 * Find active, non-expired waiver declarations matching the filter.
	 *
	 * Version-scoped semantics: when reqId, requirementVersion, and
	 * obligationId are all supplied, returns at most the waiver(s)
	 * bound to that exact (req_id, version, obligation_id) tuple.
	 *
	 * Expiry semantics: a waiver with expires_at <= now is excluded
	 * (expired waivers stop suppressing verdict, per Decision Q3).
	 * A waiver without expires_at never expires.
	 *
	 * @param now - ISO 8601 timestamp for expiry comparison. Omit to
	 *              use the current time.
	 */
	findActiveWaivers(input: FindActiveWaiversInput): Declaration[];

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

	/**
	 * Resolve repo-relative file paths to their owning MODULE nodes
	 * within a snapshot.
	 *
	 * For each input path, returns:
	 *   - whether a FILE node exists for that path in this snapshot
	 *   - the stable_key of the owning MODULE (via OWNS edge), if any
	 *
	 * A file is considered "matched" iff it has a FILE node in the snapshot.
	 * A matched file may still have owningModuleKey === null if no OWNS
	 * edge exists (defensive: should not occur in normally-indexed repos).
	 *
	 * Used by change-impact analysis to map git diff output to the
	 * indexed graph's module-centric view.
	 */
	resolveChangedFilesToModules(
		input: ResolveChangedFilesToModulesInput,
	): ChangedFileResolution[];

	/**
	 * Reverse-IMPORTS BFS traversal at the MODULE level.
	 *
	 * Given a set of seed MODULE stable_keys, returns all MODULE nodes
	 * that reach any seed via IMPORTS edges (i.e. modules that depend
	 * on any seed, directly or transitively). Seeds themselves are NOT
	 * emitted — only reached modules.
	 *
	 * Distance is the minimum hop count from seed set to the reached
	 * module. Cycle-safe (each node visited at most once, at its
	 * minimum distance).
	 *
	 * When maxDepth is omitted, traversal is unbounded. When provided,
	 * traversal stops after `maxDepth` hops.
	 */
	findReverseModuleImports(
		input: FindReverseModuleImportsInput,
	): ReverseModuleImportResult[];

	/** Compute structural metrics for all modules in a snapshot. */
	computeModuleStats(snapshotUid: string): ModuleStats[];

	/** Query function-level metrics from the measurements table. */
	queryFunctionMetrics(input: QueryFunctionMetricsInput): FunctionMetricRow[];

	/** Query per-module complexity aggregates from measurements. */
	queryModuleMetricAggregates(snapshotUid: string): ModuleMetricAggregate[];

	/** Query extracted domain versions from measurements. */
	queryDomainVersions(snapshotUid: string): DomainVersionRow[];

	/** Read measurements by kind for a snapshot. */
	queryMeasurementsByKind(
		snapshotUid: string,
		kind: string,
	): Array<{ targetStableKey: string; valueJson: string }>;

	/** Delete measurements by kind(s) for a snapshot. Used for idempotent re-import. */
	deleteMeasurementsByKind(snapshotUid: string, kinds: string[]): void;

	/**
	 * Query per-file hotspot input data by joining churn and complexity
	 * measurements. Returns files that have BOTH churn and complexity data.
	 */
	queryHotspotInputs(snapshotUid: string): HotspotInput[];

	/** Insert inferences (batch). */
	insertInferences(inferences: InferenceRow[]): void;

	/** Delete inferences by kind for a snapshot. Used for idempotent recompute. */
	deleteInferencesByKind(snapshotUid: string, kind: string): void;

	/** Query inferences by kind for a snapshot. */
	queryInferences(snapshotUid: string, kind: string): InferenceRow[];

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

export interface FindActiveWaiversInput {
	repoUid: string;
	reqId?: string;
	requirementVersion?: number;
	obligationId?: string;
	/** ISO 8601 timestamp. Omit for current time. */
	now?: string;
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

export interface ResolveChangedFilesToModulesInput {
	snapshotUid: string;
	repoUid: string;
	/** Repo-relative file paths (forward slashes). */
	filePaths: string[];
}

export interface ChangedFileResolution {
	filePath: string;
	/** True iff a FILE node exists for this path in the snapshot. */
	matched: boolean;
	/** Stable_key of the owning MODULE, or null if unmatched / no OWNS edge. */
	owningModuleKey: string | null;
}

export interface FindReverseModuleImportsInput {
	snapshotUid: string;
	seedModuleKeys: string[];
	/** Undefined = unbounded traversal. Positive integer = hop cap. */
	maxDepth?: number;
}

export interface ReverseModuleImportResult {
	moduleStableKey: string;
	/** Minimum hop count from the seed set (>= 1). */
	distance: number;
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

export interface DomainVersionRow {
	kind: string;
	value: string;
	sourceFile: string;
}

export interface HotspotInput {
	fileStableKey: string;
	filePath: string;
	churnLines: number;
	changeFrequency: number;
	sumComplexity: number;
}

export interface InferenceRow {
	inferenceUid: string;
	snapshotUid: string;
	repoUid: string;
	targetStableKey: string;
	kind: string;
	valueJson: string;
	confidence: number;
	basisJson: string;
	extractor: string;
	createdAt: string;
}

export interface ModuleMetricAggregate {
	modulePath: string;
	functionCount: number;
	avgCyclomaticComplexity: number;
	maxCyclomaticComplexity: number;
	avgNestingDepth: number;
	maxNestingDepth: number;
}
