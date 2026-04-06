/**
 * Compiler-assisted receiver type binding for unresolved obj.method() calls.
 *
 * Uses the TypeScript `Program` / `TypeChecker` API as a SIDE-CHANNEL
 * to enrich unresolved observations that the syntax-only tree-sitter
 * extractor could not resolve. Does NOT replace tree-sitter extraction;
 * augments it for a specific failure class.
 *
 * Scope (C1 first slice):
 *   - only `CALLS_OBJ_METHOD_NEEDS_TYPE_INFO` edges
 *   - only `classification = unknown` + `local_like` receiver origin
 *   - enriches with receiver type name; does NOT resolve edges
 *
 * Architecture:
 *   - one `ts.Program` per nearest-owning tsconfig.json, cached
 *   - mapping: unresolved row → source file + line/col → TS AST node
 *     → call expression → receiver expression → TypeChecker type
 *   - output stored back on unresolved_edges.metadata_json
 *
 * Performance:
 *   - Program creation is expensive (seconds per package)
 *   - designed to run as a separate `rgr enrich` pass, not inline
 *     with default indexing
 */

import * as ts from "typescript";
import { join, dirname } from "node:path";

export interface ReceiverTypeResult {
	edgeUid: string;
	receiverType: string | null;
	receiverTypeOrigin: "compiler" | "failed";
	/** If the type resolved to a named type, its display name. */
	typeDisplayName: string | null;
	/** If the type appears to be from an external package. */
	isExternalType: boolean;
	/** Error/skip reason if resolution failed. */
	failureReason: string | null;
}

export interface EnrichmentProgress {
	phase: string;
	current: number;
	total: number;
}

/**
 * Resolve receiver types for a batch of unresolved call sites using
 * the TypeScript compiler.
 *
 * @param repoRootPath Absolute path to the repo root.
 * @param sites Unresolved call sites to enrich.
 * @param onProgress Optional progress callback.
 */
export async function resolveReceiverTypes(
	repoRootPath: string,
	sites: ReadonlyArray<{
		edgeUid: string;
		sourceFilePath: string;
		lineStart: number;
		colStart: number;
		targetKey: string;
	}>,
	onProgress?: (p: EnrichmentProgress) => void,
): Promise<ReceiverTypeResult[]> {
	if (sites.length === 0) return [];

	const emit = onProgress ?? (() => {});

	// Group sites by their nearest owning tsconfig.
	emit({ phase: "grouping", current: 0, total: sites.length });
	const byTsconfig = groupSitesByTsconfig(repoRootPath, sites);

	const results: ReceiverTypeResult[] = [];
	let processed = 0;

	for (const [tsconfigPath, groupSites] of byTsconfig) {
		emit({ phase: "building_program", current: processed, total: sites.length });

		// Build the Program for this tsconfig group.
		const program = buildProgram(tsconfigPath);
		if (!program) {
			// Failed to build program — mark all sites as failed.
			for (const site of groupSites) {
				results.push({
					edgeUid: site.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed",
					typeDisplayName: null,
					isExternalType: false,
					failureReason: `failed to build program for ${tsconfigPath}`,
				});
			}
			processed += groupSites.length;
			continue;
		}

		const checker = program.getTypeChecker();

		emit({ phase: "resolving_types", current: processed, total: sites.length });

		for (const site of groupSites) {
			const absPath = join(repoRootPath, site.sourceFilePath);
			const sourceFile = program.getSourceFile(absPath);

			if (!sourceFile) {
				results.push({
					edgeUid: site.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed",
					typeDisplayName: null,
					isExternalType: false,
					failureReason: "source file not in program",
				});
				processed++;
				continue;
			}

			const result = resolveReceiverAtLocation(
				sourceFile,
				checker,
				site.lineStart,
				site.colStart,
				site.edgeUid,
			);
			results.push(result);
			processed++;
		}
	}

	emit({ phase: "done", current: sites.length, total: sites.length });
	return results;
}

// ── Program builder ─────────────────────────────────────────────────

function buildProgram(tsconfigPath: string): ts.Program | null {
	try {
		const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
		if (configFile.error) return null;

		const parsed = ts.parseJsonConfigFileContent(
			configFile.config,
			ts.sys,
			dirname(tsconfigPath),
		);

		return ts.createProgram(parsed.fileNames, {
			...parsed.options,
			// Don't emit output — we only need the type checker.
			noEmit: true,
		});
	} catch {
		return null;
	}
}

// ── Tsconfig grouping ───────────────────────────────────────────────

function groupSitesByTsconfig(
	repoRootPath: string,
	sites: ReadonlyArray<{
		edgeUid: string;
		sourceFilePath: string;
		lineStart: number;
		colStart: number;
		targetKey: string;
	}>,
): Map<string, typeof sites[number][]> {
	const cache = new Map<string, string | null>();
	const groups = new Map<string, typeof sites[number][]>();

	for (const site of sites) {
		const fileDir = site.sourceFilePath.includes("/")
			? site.sourceFilePath.slice(0, site.sourceFilePath.lastIndexOf("/"))
			: "";

		let tsconfigPath = findNearestTsconfig(fileDir, repoRootPath, cache);
		if (!tsconfigPath) {
			// Fallback: use repo root even without a tsconfig (Program
			// can still resolve with default options).
			tsconfigPath = join(repoRootPath, "tsconfig.json");
		}

		const group = groups.get(tsconfigPath) ?? [];
		group.push(site);
		groups.set(tsconfigPath, group);
	}

	return groups;
}

function findNearestTsconfig(
	relDir: string,
	repoRoot: string,
	cache: Map<string, string | null>,
): string | null {
	let dir = relDir;
	while (true) {
		const cached = cache.get(dir);
		if (cached !== undefined) return cached;

		const candidate = dir === ""
			? join(repoRoot, "tsconfig.json")
			: join(repoRoot, dir, "tsconfig.json");

		if (ts.sys.fileExists(candidate)) {
			cache.set(dir, candidate);
			return candidate;
		}

		if (dir === "") {
			cache.set(dir, null);
			return null;
		}
		const slash = dir.lastIndexOf("/");
		dir = slash >= 0 ? dir.slice(0, slash) : "";
	}
}

// ── Per-site type resolution ────────────────────────────────────────

function resolveReceiverAtLocation(
	sourceFile: ts.SourceFile,
	checker: ts.TypeChecker,
	lineStart: number,
	colStart: number,
	edgeUid: string,
): ReceiverTypeResult {
	try {
		// Convert 1-based line/col to 0-based TS position.
		const position = ts.getPositionOfLineAndCharacter(
			sourceFile,
			lineStart - 1,
			colStart,
		);

		// Find the call expression at this position.
		const callExpr = findCallExpressionAt(sourceFile, position);
		if (!callExpr) {
			return failed(edgeUid, "no call expression found at position");
		}

		// Get the receiver expression (left side of property access).
		if (!ts.isPropertyAccessExpression(callExpr.expression) &&
			!ts.isElementAccessExpression(callExpr.expression)) {
			return failed(edgeUid, "call expression is not a property/element access");
		}

		const receiverExpr = ts.isPropertyAccessExpression(callExpr.expression)
			? callExpr.expression.expression
			: (callExpr.expression as ts.ElementAccessExpression).expression;

		// Ask the TypeChecker for the receiver's type.
		const type = checker.getTypeAtLocation(receiverExpr);
		const typeStr = checker.typeToString(type);

		// Check if the type is useful (not `any`, `unknown`, `error`).
		if (
			type.flags & ts.TypeFlags.Any ||
			type.flags & ts.TypeFlags.Unknown
		) {
			return failed(edgeUid, `receiver type is ${typeStr}`);
		}

		// Determine if the type appears to be from an external package.
		const symbol = type.getSymbol?.() ?? type.aliasSymbol;
		let isExternal = false;
		if (symbol) {
			const declarations = symbol.getDeclarations?.();
			if (declarations && declarations.length > 0) {
				const declFile = declarations[0].getSourceFile().fileName;
				isExternal = declFile.includes("node_modules");
			}
		}

		return {
			edgeUid,
			receiverType: typeStr,
			receiverTypeOrigin: "compiler",
			typeDisplayName: symbol?.getName() ?? typeStr,
			isExternalType: isExternal,
			failureReason: null,
		};
	} catch (err) {
		return failed(edgeUid, `exception: ${err instanceof Error ? err.message : String(err)}`);
	}
}

function failed(edgeUid: string, reason: string): ReceiverTypeResult {
	return {
		edgeUid,
		receiverType: null,
		receiverTypeOrigin: "failed",
		typeDisplayName: null,
		isExternalType: false,
		failureReason: reason,
	};
}

/**
 * Find the CallExpression node that contains the given position.
 * Walks the AST from the root, looking for the most specific
 * CallExpression that spans the position.
 */
function findCallExpressionAt(
	sourceFile: ts.SourceFile,
	position: number,
): ts.CallExpression | null {
	let result: ts.CallExpression | null = null;

	function visit(node: ts.Node): void {
		const start = node.getStart(sourceFile);
		const end = node.getEnd();
		if (position < start || position >= end) return;

		if (ts.isCallExpression(node)) {
			result = node; // Keep the most specific (deepest) call.
		}
		ts.forEachChild(node, visit);
	}

	visit(sourceFile);
	return result;
}
