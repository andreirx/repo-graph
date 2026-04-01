import type {
	EdgeType,
	GraphNode,
	Resolution,
	SourceLocation,
} from "../model/index.js";

/**
 * An edge with an unresolved target.
 *
 * Extractors see one file at a time and cannot resolve cross-file
 * references. They emit edges where `targetKey` is a symbolic
 * reference — a stable key, a class name, or a callee expression
 * like "this.repo.findById" — not a resolved node UID.
 *
 * The indexer resolves these against the full file inventory and
 * node graph before persisting them as canonical GraphEdge records.
 *
 * This type exists to make the unresolved nature explicit at the
 * type level so nobody accidentally persists extractor output
 * directly into the edges table.
 */
export interface UnresolvedEdge {
	edgeUid: string;
	snapshotUid: string;
	repoUid: string;
	/** The node UID of the source (always resolved — it's in this file). */
	sourceNodeUid: string;
	/**
	 * Symbolic target reference. NOT a node UID. Examples:
	 * - IMPORTS: "repo:src/types:FILE" (extensionless file stable key)
	 * - CALLS: "this.repo.findById" (callee expression)
	 * - INSTANTIATES: "UserRepository" (class name)
	 * - IMPLEMENTS: "ILLMAdapter" (interface name)
	 */
	targetKey: string;
	type: EdgeType;
	resolution: Resolution;
	extractor: string;
	location: SourceLocation | null;
	metadataJson: string | null;
}

/**
 * Result of extracting a single file.
 * Contains the nodes and unresolved edges found in that file.
 */
export interface ExtractionResult {
	/** Nodes defined in this file (FILE node, symbols). */
	nodes: GraphNode[];
	/**
	 * Edges originating from this file.
	 * Targets are symbolic — the indexer must resolve them to node UIDs
	 * before persisting to the database.
	 */
	edges: UnresolvedEdge[];
}

/**
 * Code extractor port — the contract any language extractor must fulfill.
 *
 * An extractor is a pure function: source text in, nodes and edges out.
 * It does not access the database or the file system (the caller provides
 * the source text). It does not resolve cross-file references — edges
 * carry symbolic target keys that the indexer resolves.
 */
export interface ExtractorPort {
	/** Human-readable name and version. e.g. "ts-core:0.1.0" */
	readonly name: string;

	/** Language IDs this extractor handles. e.g. ["typescript", "tsx"] */
	readonly languages: string[];

	/**
	 * Extract nodes and edges from a single file's source text.
	 *
	 * @param source     - The full source text of the file.
	 * @param filePath   - Repo-relative path (forward slashes). e.g. "src/core/Foo.ts"
	 * @param fileUid    - The file's UID in the database. e.g. "my-repo:src/core/Foo.ts"
	 * @param repoUid    - The repository UID.
	 * @param snapshotUid - The current snapshot UID.
	 */
	extract(
		source: string,
		filePath: string,
		fileUid: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<ExtractionResult>;

	/**
	 * Initialize any resources needed by the extractor (e.g. load WASM grammars).
	 * Must be called once before the first extract() call.
	 */
	initialize(): Promise<void>;
}
