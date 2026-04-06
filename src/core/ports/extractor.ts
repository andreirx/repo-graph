import type { RuntimeBuiltinsSet } from "../classification/signals.js";
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
 * A single import-statement binding observed in the source file.
 *
 * ImportBinding is a SIDE-CHANNEL extractor fact: it records what
 * identifiers were introduced by import statements, for use by the
 * classifier. It is NOT an edge, and it does NOT affect the
 * graph's trust posture.
 *
 * The extractor emits ONE ImportBinding per identifier-bearing
 * specifier, including:
 *   - default imports             `import X from "m"`       → {X, m}
 *   - named imports               `import { a } from "m"`   → {a, m}
 *   - renamed named imports       `import { a as b }...`    → {b, m} (local name)
 *   - namespace imports           `import * as ns from "m"` → {ns, m}
 *   - combinations                `import X, { a } from ...`→ both
 *   - type-only statements        `import type { X }...`    → {X, m, isTypeOnly=true}
 *
 * NOT captured (deferred):
 *   - side-effect imports         `import "m"`              (no identifier)
 *   - dynamic imports             `await import("m")`
 *   - CJS require                 `require("m")`
 *   - specifier-level type        `import { type X }...`    (first slice: treated as value)
 *
 * Side-effect imports are skipped because the first classifier
 * has no use for identifier-less imports. They may be added later
 * for boundary/liveness analysis.
 */
export interface ImportBinding {
	/** Local identifier introduced by this import. */
	identifier: string;
	/** Module specifier as written in source (quotes stripped). */
	specifier: string;
	/** True iff specifier starts with "." (path-relative import). */
	isRelative: boolean;
	/** Source location of the import statement. */
	location: SourceLocation | null;
	/**
	 * True iff the enclosing statement is `import type ...`.
	 * First-slice scope: statement-level only. Specifier-level
	 * `{ type X }` is NOT detected yet and will yield isTypeOnly=false.
	 */
	isTypeOnly: boolean;
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
	/**
	 * Function-level metrics computed from the AST.
	 * Keyed by the node's stable_key. Values are deterministic
	 * measurements persisted into the measurements table.
	 */
	metrics: Map<string, ExtractedMetrics>;
	/**
	 * Every identifier-bearing import binding observed in the file.
	 * See ImportBinding for coverage. This is a side-channel fact
	 * stream for downstream classification; it does NOT correspond
	 * to graph edges.
	 */
	importBindings: ImportBinding[];
}

export interface ExtractedMetrics {
	cyclomaticComplexity: number;
	parameterCount: number;
	maxNestingDepth: number;
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
	 * Runtime builtins known to this extractor's language ecosystem.
	 * Contains globally-available identifiers and stdlib module
	 * specifiers. The indexer feeds this into SnapshotSignals so the
	 * classifier can distinguish runtime globals from unknown callees.
	 *
	 * Extractors for languages without a concept of runtime globals
	 * return an empty set.
	 */
	readonly runtimeBuiltins: RuntimeBuiltinsSet;

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
