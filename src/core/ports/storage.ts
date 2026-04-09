import type {
	BoundaryConsumerFact,
	BoundaryMechanism,
	BoundaryProviderFact,
} from "../classification/api-boundary.js";
import type { UnresolvedEdgeCategory } from "../diagnostics/unresolved-edge-categories.js";
import type {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../diagnostics/unresolved-edge-classification.js";
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
import type { UnresolvedEdge } from "./extractor.js";
import type {
	ModuleCandidate,
	ModuleCandidateEvidence,
	ModuleFileOwnership,
} from "../modules/module-candidate.js";

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
	// Lifecycle (initialize/close) is NOT part of this port.
	// Those are infrastructure concerns owned by the composition root
	// and the backend-specific provider (e.g. SqliteConnectionProvider).
	// Core code depends only on query/command methods below.

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
	/**
	 * Persist the snapshot-level extraction diagnostics payload.
	 * Called by the indexer after resolution completes, so the
	 * unresolved-edge breakdown can be retrieved by the trust
	 * reporting surface later without re-running extraction.
	 *
	 * The payload is an opaque JSON string at this layer; the
	 * indexer constructs it and the trust service parses it.
	 */
	updateSnapshotExtractionDiagnostics(
		snapshotUid: string,
		diagnosticsJson: string,
	): void;
	/**
	 * Read the extraction diagnostics payload for a snapshot.
	 * Returns null for snapshots indexed before migration 005 was
	 * applied — the trust report surfaces this as "diagnostics
	 * unavailable; re-index to populate."
	 */
	getSnapshotExtractionDiagnostics(snapshotUid: string): string | null;

	// ── Files ────────────────────────────────────────────────────────────

	upsertFiles(files: TrackedFile[]): void;
	upsertFileVersions(fileVersions: FileVersion[]): void;
	getFilesByRepo(repoUid: string): TrackedFile[];
	getStaleFiles(snapshotUid: string): TrackedFile[];

	// ── Nodes & Edges (bulk insert for indexing) ─────────────────────────

	insertNodes(nodes: GraphNode[]): void;
	insertEdges(edges: GraphEdge[]): void;

	/**
	 * Read all nodes for a snapshot. Used by the staged indexer to
	 * reconstruct the full node set for resolution after per-file
	 * extraction persisted them incrementally.
	 */
	queryAllNodes(snapshotUid: string): GraphNode[];

	/** Remove all nodes and edges for a given file in a given snapshot. */
	deleteNodesByFile(snapshotUid: string, fileUid: string): void;

	/** Delete edges by their UIDs. Used for idempotent promotion reruns. */
	deleteEdgesByUids(edgeUids: string[]): void;

	// ── Unresolved Edges ─────────────────────────────────────────────────

	/**
	 * Persist pre-classified unresolved-edge observations.
	 *
	 * Storage does NOT classify. Callers (the indexer) are responsible
	 * for running the classifier and supplying `category`,
	 * `classification`, `basisCode`, `classifierVersion`, and
	 * `observedAt` on every row.
	 */
	insertUnresolvedEdges(edges: PersistedUnresolvedEdge[]): void;

	/**
	 * Query unresolved-edge samples for a snapshot. Joins through
	 * nodes and files so the result is operationally readable (source
	 * file path + line included).
	 *
	 * Ordering is deterministic: sourceFilePath ASC, lineStart ASC,
	 * targetKey ASC, edge_uid ASC tiebreak. SQLite's default NULL
	 * ordering (NULLS FIRST on ASC) applies where columns are nullable.
	 */
	queryUnresolvedEdges(
		input: QueryUnresolvedEdgesInput,
	): UnresolvedEdgeSampleRow[];

	/**
	 * Shallow-merge a JSON patch into existing metadata_json on
	 * unresolved-edge rows. Preserves all prior metadata keys
	 * (e.g. rawCalleeName from the extractor) and adds/overwrites
	 * keys from the patch.
	 *
	 * Used by the enrichment pass to attach compiler-derived evidence
	 * without destroying extractor-provided context.
	 */
	mergeUnresolvedEdgesMetadata(
		updates: Array<{ edgeUid: string; metadataJsonPatch: string }>,
	): void;

	/**
	 * Count unresolved edges for a snapshot grouped by either
	 * classification or category. Returns ordered rows (key ASC).
	 */
	countUnresolvedEdges(
		input: CountUnresolvedEdgesInput,
	): UnresolvedEdgeCountRow[];

	// ── Staging (transient indexing artifacts) ───────────────────────────
	//
	// Used by the two-phase indexer pipeline. Extraction persists raw
	// edges and file signals immediately. Resolution reads them back
	// in batches. Staging rows are cleaned up after finalization.

	/**
	 * Read all nodes for a snapshot with only the fields needed by the
	 * edge resolver — not full GraphNode objects. Smaller memory footprint.
	 *
	 * CAUTION: materializes the full result set as a JS array. For
	 * Linux-scale snapshots (500K+ nodes), use queryResolverNodesIter
	 * to avoid V8 heap exhaustion from bulk .all() materialization.
	 */
	queryResolverNodes(snapshotUid: string): ResolverNode[];

	/**
	 * Row-at-a-time iterator over slim resolver nodes for a snapshot.
	 * Uses better-sqlite3's .iterate() — does NOT materialize the
	 * full result set. Consumers build Maps directly from the stream,
	 * keeping peak memory = Maps only (no duplicate intermediate array).
	 */
	queryResolverNodesIter(snapshotUid: string): IterableIterator<ResolverNode>;

	/** Persist raw extracted edges to staging table (batch). */
	insertStagedEdges(edges: StagedEdge[]): void;

	/** Read all staged edges for a snapshot. */
	queryStagedEdges(snapshotUid: string): StagedEdge[];

	/**
	 * Read staged edges in batches using cursor-based pagination.
	 * Orders by edge_uid ASC. Returns up to `limit` rows where
	 * edge_uid > afterEdgeUid (or from the start if null).
	 * Stable under concurrent deletion.
	 */
	queryStagedEdgesBatch(
		snapshotUid: string,
		limit: number,
		afterEdgeUid: string | null,
	): StagedEdge[];

	/** Delete all staged edges for a snapshot (cleanup after resolution). */
	deleteStagedEdges(snapshotUid: string): void;

	/** Persist per-file import bindings to staging table (batch). */
	insertFileSignals(signals: FileSignalRow[]): void;

	/** Read import bindings for a specific file in a snapshot. */
	queryFileSignals(snapshotUid: string, fileUid: string): FileSignalRow | null;

	/**
	 * Read file signals for a batch of file UIDs in a snapshot.
	 * Used during edge resolution to load import bindings only for
	 * files referenced by the current edge batch.
	 */
	queryFileSignalsBatch(
		snapshotUid: string,
		fileUids: string[],
	): FileSignalRow[];

	/** Read all file signals for a snapshot. */
	queryAllFileSignals(snapshotUid: string): FileSignalRow[];

	/** Delete all file signals for a snapshot (optional cleanup). */
	deleteFileSignals(snapshotUid: string): void;

	/**
	 * Query SYMBOL nodes for a specific file in a snapshot.
	 * Returns only the fields needed by framework detectors and
	 * boundary extractors. Small per-file result set (5-50 rows).
	 *
	 * Use for targeted per-file passes. For whole-snapshot iteration,
	 * prefer queryResolverNodesIter to avoid per-file query thrash.
	 */
	querySymbolsByFile(
		snapshotUid: string,
		fileUid: string,
	): FileSymbolRow[];

	// ── Boundary Facts (source of truth) ─────────────────────────────────
	//
	// Raw boundary interaction observations. These are the durable
	// substrate. Provider facts = something exposes an entry surface.
	// Consumer facts = something reaches across a boundary.
	//
	// These tables are NOT the edges table. Boundary links are NOT
	// language edges. They live in explicit boundary-fact surfaces.

	/** Persist boundary provider facts (batch). */
	insertBoundaryProviderFacts(facts: PersistedBoundaryProviderFact[]): void;

	/** Persist boundary consumer facts (batch). */
	insertBoundaryConsumerFacts(facts: PersistedBoundaryConsumerFact[]): void;

	/** Query provider facts for a snapshot. */
	queryBoundaryProviderFacts(
		input: QueryBoundaryFactsInput,
	): PersistedBoundaryProviderFact[];

	/** Query consumer facts for a snapshot. */
	queryBoundaryConsumerFacts(
		input: QueryBoundaryFactsInput,
	): PersistedBoundaryConsumerFact[];

	// ── Boundary Links (derived artifact) ────────────────────────────────
	//
	// Materialized provider-consumer matches. EXPLICITLY DERIVED.
	// Can be deleted and recomputed from raw facts without data loss.

	/** Persist materialized boundary links (batch). */
	insertBoundaryLinks(links: PersistedBoundaryLink[]): void;

	/**
	 * Query materialized boundary links for a snapshot.
	 * Returns enriched rows with provider/consumer details joined in
	 * so results are operationally readable without further lookups.
	 */
	queryBoundaryLinks(
		input: QueryBoundaryLinksInput,
	): BoundaryLinkRow[];

	/**
	 * Delete all derived boundary links for a snapshot.
	 * Used for idempotent re-materialization: delete old links,
	 * rerun matcher, insert new links.
	 */
	deleteBoundaryLinksBySnapshot(snapshotUid: string): void;

	/**
	 * Delete all boundary facts (provider + consumer) and derived
	 * links for a snapshot. Used when re-indexing replaces all facts.
	 */
	deleteBoundaryFactsBySnapshot(snapshotUid: string): void;

	// ── Module Candidates (discovered modules) ──────────────────────────
	//
	// Machine-derived module discovery facts. Separate from declarations
	// (human-authored) and inferences (node-targeted). Architectural
	// precedent: boundary-fact tables.

	/** Persist discovered module candidates (batch). */
	insertModuleCandidates(candidates: ModuleCandidate[]): void;

	/** Persist evidence items for module candidates (batch). */
	insertModuleCandidateEvidence(evidence: ModuleCandidateEvidence[]): void;

	/** Persist file-to-module ownership assignments (batch). */
	insertModuleFileOwnership(ownership: ModuleFileOwnership[]): void;

	/** Query all module candidates for a snapshot. */
	queryModuleCandidates(snapshotUid: string): ModuleCandidate[];

	/** Query evidence items for a specific module candidate. */
	queryModuleCandidateEvidence(moduleCandidateUid: string): ModuleCandidateEvidence[];

	/** Query all evidence items for a snapshot. */
	queryAllModuleCandidateEvidence(snapshotUid: string): ModuleCandidateEvidence[];

	/** Query file ownership for a snapshot. */
	queryModuleFileOwnership(snapshotUid: string): ModuleFileOwnership[];

	/** Delete all module candidates (and cascaded evidence/ownership) for a snapshot. */
	deleteModuleCandidatesBySnapshot(snapshotUid: string): void;

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
	 * Count edges of a given type in a snapshot.
	 * Used by trust reporting to compute resolved-edge rates.
	 */
	countEdgesByType(snapshotUid: string, edgeType: string): number;

	/**
	 * Find 2-node MODULE cycles where one module's path is a strict
	 * path-prefix ancestor of the other's. Used by trust reporting
	 * to detect registry/dispatch patterns characteristic of block
	 * or plugin systems (e.g. `packages/foo/src -> packages/foo/src/core -> packages/foo/src`).
	 *
	 * Returns pairs: one ancestor module + one descendant module that
	 * form a 2-node IMPORTS cycle. Single-pair per detection; does
	 * not enumerate longer cycle chains.
	 *
	 * This is a trust-specific query intentionally kept separate
	 * from the general `findCycles` API so it can evolve without
	 * affecting user-facing cycle output.
	 */
	findPathPrefixModuleCycles(
		snapshotUid: string,
	): PathPrefixModuleCycle[];

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

export interface PathPrefixModuleCycle {
	/** Ancestor module (path-prefix parent) stable_key. */
	ancestorStableKey: string;
	/** Descendant module stable_key (path is under ancestor's path). */
	descendantStableKey: string;
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

// ── Unresolved edge types ────────────────────────────────────────────────

/**
 * Persisted unresolved-edge row. Extends the extractor's
 * UnresolvedEdge with the classifier verdict and observation
 * provenance.
 *
 * Clean separation from UnresolvedEdge:
 *   - UnresolvedEdge       = pre-persistence, pre-classification, emitted by extractor
 *   - PersistedUnresolvedEdge = post-classification, persisted to unresolved_edges
 */
export interface PersistedUnresolvedEdge extends UnresolvedEdge {
	category: UnresolvedEdgeCategory;
	classification: UnresolvedEdgeClassification;
	classifierVersion: number;
	basisCode: UnresolvedEdgeBasisCode;
	/** ISO 8601 timestamp stamped by the caller at commit time. */
	observedAt: string;
}

export interface QueryUnresolvedEdgesInput {
	snapshotUid: string;
	classification?: UnresolvedEdgeClassification;
	category?: UnresolvedEdgeCategory;
	basisCode?: UnresolvedEdgeBasisCode;
	/** Max rows to return. Omit for unbounded. */
	limit?: number;
}

export interface CountUnresolvedEdgesInput {
	snapshotUid: string;
	groupBy: "classification" | "category";
	/**
	 * Optional: restrict to rows matching these categories BEFORE
	 * grouping. Used by the trust service to count only CALLS-family
	 * unresolved edges grouped by classification.
	 */
	filterCategories?: string[];
}

/**
 * Count row for unresolved-edge aggregate queries. `key` holds either
 * a classification or category value depending on the groupBy axis
 * used at the query site. Rows are ordered by `key` ASC.
 */
export interface UnresolvedEdgeCountRow {
	key: string;
	count: number;
}

/**
 * Reading-shape row for unresolved-edge samples. Richer than
 * PersistedUnresolvedEdge: exposes source stable_key and source file
 * path via join, so the result is operationally readable without
 * further lookups.
 */
export interface UnresolvedEdgeSampleRow {
	edgeUid: string;
	classification: UnresolvedEdgeClassification;
	category: UnresolvedEdgeCategory;
	basisCode: UnresolvedEdgeBasisCode;
	targetKey: string;
	sourceNodeUid: string;
	sourceStableKey: string;
	/** Repo-relative path of the source file. NULL iff the source node has no file_uid. */
	sourceFilePath: string | null;
	lineStart: number | null;
	colStart: number | null;
	/** Visibility of the enclosing source node (EXPORT, PRIVATE, null). */
	sourceNodeVisibility: string | null;
	/** Raw metadata_json from the unresolved_edges row. May contain enrichment data. */
	metadataJson: string | null;
}

// ── Boundary fact types ─────────────────────────────────────────────────

/**
 * Persisted boundary provider fact. Extends the core domain type
 * with storage identity, scope, matcher key, and provenance.
 *
 * Clean separation from BoundaryProviderFact:
 *   - BoundaryProviderFact       = emitted by extractor, pre-persistence
 *   - PersistedBoundaryProviderFact = post-normalization, persisted to boundary_provider_facts
 */
export interface PersistedBoundaryProviderFact extends BoundaryProviderFact {
	/** Storage identity. */
	factUid: string;
	/** Scope: which snapshot this fact belongs to. */
	snapshotUid: string;
	/** Scope: which repo this fact belongs to. */
	repoUid: string;
	/**
	 * Normalized operation identifier for matching.
	 * Mechanism-specific format, computed by the match strategy.
	 * For HTTP: "GET /api/v2/products/{_}" (params → wildcard tokens).
	 * Opaque to storage; semantics owned by the match strategy.
	 */
	matcherKey: string;
	/** Which adapter + version produced this fact (e.g. "spring-route-extractor:0.1"). */
	extractor: string;
	/** ISO 8601 timestamp when this fact was extracted. Immutable. */
	observedAt: string;
}

/**
 * Persisted boundary consumer fact. Extends the core domain type
 * with storage identity, scope, matcher key, and provenance.
 *
 * Clean separation from BoundaryConsumerFact:
 *   - BoundaryConsumerFact       = emitted by extractor, pre-persistence
 *   - PersistedBoundaryConsumerFact = post-normalization, persisted to boundary_consumer_facts
 */
export interface PersistedBoundaryConsumerFact extends BoundaryConsumerFact {
	/** Storage identity. */
	factUid: string;
	/** Scope: which snapshot this fact belongs to. */
	snapshotUid: string;
	/** Scope: which repo this fact belongs to. */
	repoUid: string;
	/**
	 * Normalized operation identifier for matching.
	 * Same format as the provider matcher_key for the same mechanism.
	 */
	matcherKey: string;
	/** Which adapter + version produced this fact (e.g. "http-client-extractor:0.1"). */
	extractor: string;
	/** ISO 8601 timestamp when this fact was extracted. Immutable. */
	observedAt: string;
}

/**
 * Persisted boundary link — a materialized provider-consumer match.
 * EXPLICITLY DERIVED. Can be deleted and recomputed without data loss.
 */
export interface PersistedBoundaryLink {
	linkUid: string;
	snapshotUid: string;
	repoUid: string;
	providerFactUid: string;
	consumerFactUid: string;
	/** How the match was determined. */
	matchBasis: "exact_contract" | "address_match" | "operation_match" | "heuristic";
	/** Combined confidence (provider × consumer × match quality). */
	confidence: number;
	/** Matcher-specific metadata (e.g. segment comparison details). */
	metadataJson: string | null;
	/** ISO 8601 timestamp when this link was materialized. */
	materializedAt: string;
}

/**
 * Input filter for querying boundary facts (provider or consumer).
 */
export interface QueryBoundaryFactsInput {
	snapshotUid: string;
	/** Filter to a specific mechanism (http, grpc, etc.). */
	mechanism?: BoundaryMechanism;
	/** Filter by matcher_key (exact match). */
	matcherKey?: string;
	/** Filter by source file (repo-relative path). */
	sourceFile?: string;
	/** Max rows to return. Omit for unbounded. */
	limit?: number;
}

/**
 * Input filter for querying materialized boundary links.
 */
export interface QueryBoundaryLinksInput {
	snapshotUid: string;
	/** Filter to a specific mechanism (via provider's mechanism). */
	mechanism?: BoundaryMechanism;
	/** Filter by match basis. */
	matchBasis?: PersistedBoundaryLink["matchBasis"];
	/** Max rows to return. Omit for unbounded. */
	limit?: number;
}

/**
 * Reading-shape row for boundary links. Joins provider and consumer
 * details so results are operationally readable without further lookups.
 */
export interface BoundaryLinkRow {
	linkUid: string;
	matchBasis: string;
	confidence: number;
	// Provider side
	providerMechanism: string;
	providerOperation: string;
	providerAddress: string;
	providerHandlerStableKey: string;
	providerSourceFile: string;
	providerLineStart: number;
	providerFramework: string;
	// Consumer side
	consumerOperation: string;
	consumerAddress: string;
	consumerCallerStableKey: string;
	consumerSourceFile: string;
	consumerLineStart: number;
	consumerBasis: string;
	consumerConfidence: number;
	/** Link-level metadata_json (matcher notes). */
	metadataJson: string | null;
}

// ── Staging types ───────────────────────────────────────────────────

/**
 * A staged unresolved edge — raw extractor output persisted before
 * resolution. Same shape as UnresolvedEdge but with an additional
 * sourceFileUid for per-TU resolution context.
 */
export interface StagedEdge {
	edgeUid: string;
	snapshotUid: string;
	repoUid: string;
	sourceNodeUid: string;
	targetKey: string;
	type: string;
	resolution: string;
	extractor: string;
	lineStart: number | null;
	colStart: number | null;
	lineEnd: number | null;
	colEnd: number | null;
	metadataJson: string | null;
	/** File UID of the source file this edge was extracted from. */
	sourceFileUid: string | null;
}

/**
 * Per-file import binding signals — persisted during extraction so
 * the classifier can read them back during the resolution phase
 * without keeping all bindings in memory.
 */
export interface FileSignalRow {
	snapshotUid: string;
	fileUid: string;
	/** JSON-serialized ImportBinding[]. */
	importBindingsJson: string | null;
	/** JSON-serialized PackageDependencySet. Nearest-owning manifest deps. */
	packageDependenciesJson: string | null;
	/** JSON-serialized TsconfigAliases. Nearest-owning tsconfig paths. */
	tsconfigAliasesJson: string | null;
}

/**
 * Slim node representation for the edge resolver.
 * Only the fields needed for resolution and affinity filtering.
 * Much smaller than full GraphNode (~7 fields vs ~18).
 */
export interface ResolverNode {
	nodeUid: string;
	stableKey: string;
	name: string;
	qualifiedName: string | null;
	kind: string;
	subtype: string | null;
	fileUid: string | null;
}

/**
 * Per-file symbol row for framework detectors and boundary extractors.
 * Only SYMBOL-kind nodes, with the fields those passes actually need.
 */
export interface FileSymbolRow {
	stableKey: string;
	nodeUid: string;
	name: string;
	qualifiedName: string;
	subtype: string | null;
	lineStart: number | null;
	visibility: string | null;
}
