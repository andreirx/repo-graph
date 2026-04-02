/**
 * INTERNAL domain representations of query results.
 *
 * These types use camelCase (TypeScript convention) for internal use.
 * The CLI JSON output uses snake_case as documented in docs/cli/v1-cli.txt.
 * The conversion from internal types to the external wire format happens
 * in src/cli/formatters/json.ts — that is the serialization boundary.
 *
 * When adding fields here, also update the JSON formatter and the
 * v1-cli.txt response contract documentation.
 */

export interface QueryResult<T> {
	command: string;
	repo: string;
	snapshot: string;
	snapshotScope: string;
	basisCommit: string | null;
	results: T[];
	count: number;
	/** True if files have changed since the snapshot */
	stale: boolean;
}

/**
 * A node result enriched with edge context for caller/callee queries.
 */
export interface NodeResult {
	nodeUid: string;
	symbol: string;
	kind: string;
	subtype: string | null;
	file: string;
	line: number | null;
	column: number | null;
	edgeType: string | null;
	resolution: string | null;
	evidence: string[];
	depth: number;
}

/**
 * A path result showing the chain of edges between two nodes.
 */
export interface PathResult {
	found: boolean;
	pathLength: number;
	steps: PathStep[];
}

export interface PathStep {
	nodeUid: string;
	symbol: string;
	file: string;
	line: number | null;
	edgeType: string;
}

/**
 * A cycle result showing one circular dependency chain.
 */
export interface CycleResult {
	cycleId: string;
	length: number;
	nodes: CycleNode[];
}

export interface CycleNode {
	nodeUid: string;
	name: string;
	file: string | null;
}

/**
 * A dead node result (zero incoming edges, not a declared entrypoint).
 */
export interface DeadNodeResult {
	nodeUid: string;
	symbol: string;
	kind: string;
	subtype: string | null;
	file: string;
	line: number | null;
	lineCount: number | null;
}

/**
 * A boundary violation: an IMPORTS edge that crosses a declared
 * forbidden boundary.
 */
export interface BoundaryViolation {
	/** The module path that has the boundary rule. */
	boundaryModule: string;
	/** The module path that is forbidden. */
	forbiddenModule: string;
	/** The reason from the boundary declaration, if any. */
	reason: string | null;
	/** The file that contains the violating import. */
	sourceFile: string;
	/** The file being imported. */
	targetFile: string;
	/** Line number of the import statement. */
	line: number | null;
}
