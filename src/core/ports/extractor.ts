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
	/**
	 * Original exported symbol name for named imports.
	 *
	 * Populated for named imports so that alias resolution works
	 * downstream:
	 * - `import { readFile } from "fs"`
	 *   → `identifier = "readFile"`, `importedName = "readFile"`
	 * - `import { readFile as rf } from "fs"`
	 *   → `identifier = "rf"`, `importedName = "readFile"`
	 *
	 * `null` for default imports (`import fs from "fs"`) and
	 * namespace imports (`import * as fs from "fs"`) because
	 * those patterns bring in the whole module surface; the
	 * actual symbol is determined at the call site by the
	 * member expression, not by the import statement.
	 *
	 * TS-side note: under the Fork-1 state-boundary posture,
	 * TS-codebase extractors (`src/adapters/extractors/*`)
	 * populate this as `null` on every binding. Full TS-side
	 * population is a deferred follow-on (tracked in
	 * `docs/TECH-DEBT.md`). The field is present in the contract
	 * for symmetry; Rust-side consumers (`state-extractor`) are
	 * the only ones populating it today. Cross-runtime parity
	 * harness projects `ImportBinding` to a fixed field set that
	 * does NOT include `importedName`, so this asymmetric
	 * population is parity-transparent.
	 */
	importedName: string | null;
}

/**
 * Classification of a call site's positional argument 0 payload.
 *
 * Added by SB-3-pre for state-boundary integration. Discriminated
 * by the `kind` field.
 *
 * Slice-scoped on purpose: covers only the argument-0 forms SB-3
 * ships (string literal; `process.env.NAME` member read). This is
 * NOT the final universal call-argument model; object-property
 * extraction, constructor configs, and argument positions beyond
 * 0 are reserved for later slices with their own types.
 *
 * Fork-1 note: populated by the Rust `ts-extractor`. TS-codebase
 * extractors under `src/adapters/extractors/*` do NOT populate
 * `ResolvedCallsite`s today; full TS-side extraction of this
 * fact is a deferred follow-on (see `docs/TECH-DEBT.md`). The
 * types are present in the port contract for cross-runtime
 * structural parity.
 */
export type Arg0Payload =
	| {
		/** Argument 0 is a string literal. */
		kind: "string_literal";
		/** The literal string value (quotes stripped). */
		value: string;
	}
	| {
		/** Argument 0 is a `process.env.NAME` member read. */
		kind: "env_key_read";
		/** The env-key identifier (e.g. `"DATABASE_URL"`). */
		keyName: string;
	};

/**
 * A call-site fact with the callee resolved to a
 * `(module, symbol)` pair via the file's import bindings, plus a
 * classification of argument 0's payload.
 *
 * Added by SB-3-pre. Consumed by `state-extractor` downstream.
 *
 * Slice-1 limitation: top-level calls at module scope do NOT
 * produce `ResolvedCallsite` facts. The `enclosingSymbolNodeUid`
 * field is typed as "enclosing SYMBOL node UID" by contract; the
 * call-extraction pipeline uses the FILE node UID as the caller
 * for top-level statements, which would violate the contract.
 * Top-level state touches remain visible as generic CALLS edges.
 */
export interface ResolvedCallsite {
	/** `nodeUid` of the symbol containing the call site. */
	enclosingSymbolNodeUid: string;
	/**
	 * Resolved source module of the callee, via import bindings.
	 * Matches the `specifier` of one of the file's
	 * `importBindings`.
	 */
	resolvedModule: string;
	/**
	 * Resolved exported-symbol path within `resolvedModule`. For
	 * named imports with an alias, this is the ORIGINAL exported
	 * name (via `ImportBinding.importedName`), not the local
	 * alias.
	 */
	resolvedSymbol: string;
	/** Classification of argument 0's payload. */
	arg0Payload: Arg0Payload;
	/** Source location of the call expression. */
	sourceLocation: SourceLocation;
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
	/**
	 * Resolved-callsite facts for state-boundary integration
	 * (SB-3-pre). Empty for TS-codebase extractors under the
	 * Fork-1 posture; the Rust `ts-extractor` is the only
	 * producer today. See `ResolvedCallsite` for coverage and
	 * `docs/TECH-DEBT.md` for the deferred TS-side population.
	 */
	resolvedCallsites: ResolvedCallsite[];
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
