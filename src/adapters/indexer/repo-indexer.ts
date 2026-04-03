import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import ignore, { type Ignore } from "ignore";
import { v4 as uuidv4 } from "uuid";
import type {
	FileVersion,
	GraphEdge,
	GraphNode,
	TrackedFile,
} from "../../core/model/index.js";
import {
	DeclarationKind,
	EdgeType,
	NodeKind,
	NodeSubtype,
	ParseStatus,
	Resolution,
	SnapshotKind,
	SnapshotStatus,
} from "../../core/model/index.js";
import type {
	ExtractionResult,
	ExtractorPort,
	UnresolvedEdge,
} from "../../core/ports/extractor.js";
import type {
	IndexerPort,
	IndexOptions,
	IndexResult,
} from "../../core/ports/indexer.js";
import type { StoragePort } from "../../core/ports/storage.js";
import { buildToolchainJson, INDEXER_VERSION } from "../../version.js";

/** File extensions the TS extractor handles. */
const TS_EXTENSIONS = new Set([".ts", ".tsx", ".js", ".jsx"]);

/** Directories always excluded from scanning. */
const ALWAYS_EXCLUDED = new Set([
	"node_modules",
	".git",
	"dist",
	"build",
	"out",
	".next",
	".nuxt",
	"coverage",
	".turbo",
	".cache",
	// Python virtualenvs (common in mixed JS/Python repos)
	"venv",
	".venv",
	"__pycache__",
	// CDK build output
	"cdk.out",
]);

export class RepoIndexer implements IndexerPort {
	constructor(
		private storage: StoragePort,
		private extractor: ExtractorPort,
	) {}

	async indexRepo(
		repoUid: string,
		options?: IndexOptions,
	): Promise<IndexResult> {
		return this.runIndex(repoUid, SnapshotKind.FULL, options);
	}

	private async runIndex(
		repoUid: string,
		snapshotKind: SnapshotKind,
		options?: IndexOptions,
	): Promise<IndexResult> {
		const startTime = Date.now();
		const repo = this.storage.getRepo({ uid: repoUid });
		if (!repo) {
			throw new Error(`Repository not found: ${repoUid}`);
		}

		const emit = options?.onProgress ?? (() => {});

		// Find parent snapshot for refresh operations
		const parentSnapshot =
			snapshotKind === SnapshotKind.REFRESH
				? this.storage.getLatestSnapshot(repoUid)
				: null;

		// 1. Create snapshot with toolchain provenance
		const snapshot = this.storage.createSnapshot({
			repoUid,
			kind: snapshotKind,
			parentSnapshotUid: parentSnapshot?.snapshotUid,
			toolchainJson: JSON.stringify(buildToolchainJson()),
		});

		try {
			// 2. Scan file tree
			emit({ phase: "scanning", current: 0, total: 0 });
			const filePaths = await this.scanFiles(
				repo.rootPath,
				options?.exclude,
				options?.include,
			);
			emit({
				phase: "scanning",
				current: filePaths.length,
				total: filePaths.length,
			});

			// 3. Register files and compute hashes
			const trackedFiles: TrackedFile[] = [];
			const fileVersions: FileVersion[] = [];
			const fileContents = new Map<string, string>();

			for (const relPath of filePaths) {
				const absPath = join(repo.rootPath, relPath);
				const content = await readFile(absPath, "utf-8");
				const contentHash = hashContent(content);
				const fileUid = `${repoUid}:${relPath}`;

				trackedFiles.push({
					fileUid,
					repoUid,
					path: relPath,
					language: detectLanguage(relPath),
					isTest: isTestFile(relPath),
					isGenerated: false,
					isExcluded: false,
				});

				fileVersions.push({
					snapshotUid: snapshot.snapshotUid,
					fileUid,
					contentHash,
					astHash: null,
					extractor: this.extractor.name,
					parseStatus: ParseStatus.PARSED,
					sizeBytes: Buffer.byteLength(content, "utf-8"),
					lineCount: content.split("\n").length,
					indexedAt: new Date().toISOString(),
				});

				fileContents.set(relPath, content);
			}

			this.storage.upsertFiles(trackedFiles);
			this.storage.upsertFileVersions(fileVersions);

			// 4. Extract each file
			const allNodes: GraphNode[] = [];
			const allUnresolvedEdges: UnresolvedEdge[] = [];
			let extractIdx = 0;

			for (const relPath of filePaths) {
				emit({
					phase: "extracting",
					current: extractIdx,
					total: filePaths.length,
					file: relPath,
				});

				const content = fileContents.get(relPath);
				if (!content) continue;

				const fileUid = `${repoUid}:${relPath}`;
				let result: ExtractionResult;
				try {
					result = await this.extractor.extract(
						content,
						relPath,
						fileUid,
						repoUid,
						snapshot.snapshotUid,
					);
				} catch {
					// Mark file as failed but continue indexing
					this.storage.upsertFileVersions([
						{
							...fileVersions[extractIdx],
							parseStatus: ParseStatus.FAILED,
						},
					]);
					extractIdx++;
					continue;
				}

				allNodes.push(...result.nodes);
				allUnresolvedEdges.push(...result.edges);
				extractIdx++;
			}

			emit({
				phase: "extracting",
				current: filePaths.length,
				total: filePaths.length,
			});

			// 5. Create MODULE nodes for directories
			const moduleNodes = this.createModuleNodes(
				filePaths,
				repoUid,
				snapshot.snapshotUid,
			);
			allNodes.push(...moduleNodes);

			// 6. Persist nodes
			emit({ phase: "persisting", current: 0, total: allNodes.length });
			this.storage.insertNodes(allNodes);

			// 7. Resolve edges
			emit({
				phase: "resolving",
				current: 0,
				total: allUnresolvedEdges.length,
			});

			const { resolved, unresolvedCount, unresolvedBreakdown } =
				this.resolveEdges(
					allUnresolvedEdges,
					allNodes,
					trackedFiles,
					repoUid,
					snapshot.snapshotUid,
				);

			emit({
				phase: "resolving",
				current: allUnresolvedEdges.length,
				total: allUnresolvedEdges.length,
			});

			// 8. Create module-level edges
			const moduleEdges = this.createModuleEdges(
				allNodes,
				resolved,
				repoUid,
				snapshot.snapshotUid,
			);
			const allEdges = [...resolved, ...moduleEdges];

			// 9. Persist edges
			this.storage.insertEdges(allEdges);

			// 10. Finalize snapshot
			this.storage.updateSnapshotCounts(snapshot.snapshotUid);
			this.storage.updateSnapshotStatus({
				snapshotUid: snapshot.snapshotUid,
				status: SnapshotStatus.READY,
			});

			emit({ phase: "persisting", current: 1, total: 1 });

			// 11. Check for orphaned symbol declarations
			const orphanedDeclarations = this.countOrphanedDeclarations(
				repoUid,
				snapshot.snapshotUid,
			);

			return {
				snapshotUid: snapshot.snapshotUid,
				filesTotal: filePaths.length,
				nodesTotal: allNodes.length,
				edgesTotal: allEdges.length,
				edgesUnresolved: unresolvedCount,
				unresolvedBreakdown,
				durationMs: Date.now() - startTime,
				orphanedDeclarations,
			};
		} catch (err) {
			// Mark snapshot as failed
			this.storage.updateSnapshotStatus({
				snapshotUid: snapshot.snapshotUid,
				status: SnapshotStatus.FAILED,
			});
			throw err;
		}
	}

	async refreshRepo(repoUid: string): Promise<IndexResult> {
		const repo = this.storage.getRepo({ uid: repoUid });
		if (!repo) {
			throw new Error(`Repository not found: ${repoUid}`);
		}

		// For v1, refresh performs a full re-extraction but records it as a
		// REFRESH snapshot with a parent link to the previous snapshot.
		// A true incremental refresh would:
		// 1. Compare content hashes to identify changed/added/removed files
		// 2. Copy unchanged file nodes/edges from the previous snapshot
		// 3. Only re-extract changed/added files
		// 4. Delete nodes/edges for removed files
		// 5. Re-resolve edges affected by changes
		// That optimization is deferred to v2.
		return this.runIndex(repoUid, SnapshotKind.REFRESH);
	}

	// ── File scanning ──────────────────────────────────────────────────

	private async scanFiles(
		rootPath: string,
		excludeGlobs?: string[],
		includeGlobs?: string[],
	): Promise<string[]> {
		const files: string[] = [];
		const excludePatterns = excludeGlobs ?? [];

		// Load .gitignore if present
		const ig = await loadGitignore(rootPath);

		await this.walkDir(rootPath, rootPath, files, excludePatterns, ig);

		// Apply include filter if specified
		if (includeGlobs && includeGlobs.length > 0) {
			return files.filter((f) =>
				includeGlobs.some((g) => matchSimpleGlob(f, g)),
			);
		}

		return files;
	}

	private async walkDir(
		currentPath: string,
		rootPath: string,
		files: string[],
		excludePatterns: string[],
		ig: Ignore,
	): Promise<void> {
		const entries = await readdir(currentPath, { withFileTypes: true });

		for (const entry of entries) {
			const fullPath = join(currentPath, entry.name);
			const relPath = toPosixPath(relative(rootPath, fullPath));

			if (entry.isDirectory()) {
				if (ALWAYS_EXCLUDED.has(entry.name)) continue;
				if (isExcluded(relPath, entry.name, excludePatterns)) continue;
				// .gitignore uses trailing slash for directories
				if (ig.ignores(`${relPath}/`)) continue;
				// Skip Python virtualenvs (contain pyvenv.cfg at their root)
				if (existsSync(join(fullPath, "pyvenv.cfg"))) continue;
				await this.walkDir(fullPath, rootPath, files, excludePatterns, ig);
			} else if (entry.isFile()) {
				const ext = getExtension(entry.name);
				if (!TS_EXTENSIONS.has(ext)) continue;
				if (isExcluded(relPath, entry.name, excludePatterns)) continue;
				if (ig.ignores(relPath)) continue;
				files.push(relPath);
			}
		}
	}

	// ── MODULE node creation ───────────────────────────────────────────

	private createModuleNodes(
		filePaths: string[],
		repoUid: string,
		snapshotUid: string,
	): GraphNode[] {
		// Collect unique directories that contain source files
		const dirs = new Set<string>();
		for (const fp of filePaths) {
			const parts = fp.split("/");
			// Add each ancestor directory
			for (let i = 1; i < parts.length; i++) {
				dirs.add(parts.slice(0, i).join("/"));
			}
		}

		const nodes: GraphNode[] = [];
		for (const dir of dirs) {
			const name = dir.split("/").pop() ?? dir;
			const parentDir = dir.includes("/")
				? dir.split("/").slice(0, -1).join("/")
				: null;

			nodes.push({
				nodeUid: uuidv4(),
				snapshotUid,
				repoUid,
				stableKey: `${repoUid}:${dir}:MODULE`,
				kind: NodeKind.MODULE,
				subtype: NodeSubtype.DIRECTORY,
				name,
				qualifiedName: dir,
				fileUid: null,
				parentNodeUid: null, // flat for now — could link parent modules later
				location: null,
				signature: null,
				visibility: null,
				docComment: null,
				metadataJson: parentDir
					? JSON.stringify({ parentModule: `${repoUid}:${parentDir}:MODULE` })
					: null,
			});
		}

		return nodes;
	}

	// ── Module-level edge creation ─────────────────────────────────────

	/**
	 * Create two kinds of module-level edges:
	 * 1. OWNS: MODULE -> FILE (each file belongs to its directory module)
	 * 2. IMPORTS: MODULE -> MODULE (derived from file-level IMPORTS edges)
	 */
	private createModuleEdges(
		allNodes: GraphNode[],
		resolvedEdges: GraphEdge[],
		repoUid: string,
		snapshotUid: string,
	): GraphEdge[] {
		const edges: GraphEdge[] = [];

		// Build lookup: stable key -> node uid
		const stableKeyToUid = new Map<string, string>();
		for (const n of allNodes) {
			stableKeyToUid.set(n.stableKey, n.nodeUid);
		}

		// Build lookup: file node uid -> module stable key
		const fileToModule = new Map<string, string>();
		for (const n of allNodes) {
			if (n.kind === NodeKind.FILE && n.qualifiedName) {
				const modPath = getModulePath(n.qualifiedName);
				if (modPath) {
					const moduleKey = `${repoUid}:${modPath}:MODULE`;
					fileToModule.set(n.nodeUid, moduleKey);
				}
			}
		}

		// 1. OWNS edges: MODULE -> FILE
		for (const [fileNodeUid, moduleKey] of fileToModule.entries()) {
			const moduleUid = stableKeyToUid.get(moduleKey);
			if (moduleUid) {
				edges.push({
					edgeUid: uuidv4(),
					snapshotUid,
					repoUid,
					sourceNodeUid: moduleUid,
					targetNodeUid: fileNodeUid,
					type: EdgeType.OWNS,
					resolution: Resolution.STATIC,
					extractor: INDEXER_VERSION,
					location: null,
					metadataJson: null,
				});
			}
		}

		// 2. MODULE->MODULE IMPORTS: derived from file-level IMPORTS edges
		// If file A (in module X) imports file B (in module Y), then module X imports module Y.
		const moduleImportPairs = new Set<string>();
		for (const edge of resolvedEdges) {
			if (edge.type !== EdgeType.IMPORTS) continue;
			const sourceModuleKey = fileToModule.get(edge.sourceNodeUid);
			const targetModuleKey = fileToModule.get(edge.targetNodeUid);
			if (
				sourceModuleKey &&
				targetModuleKey &&
				sourceModuleKey !== targetModuleKey
			) {
				const pairKey = `${sourceModuleKey}|${targetModuleKey}`;
				if (moduleImportPairs.has(pairKey)) continue;
				moduleImportPairs.add(pairKey);

				const sourceModuleUid = stableKeyToUid.get(sourceModuleKey);
				const targetModuleUid = stableKeyToUid.get(targetModuleKey);
				if (sourceModuleUid && targetModuleUid) {
					edges.push({
						edgeUid: uuidv4(),
						snapshotUid,
						repoUid,
						sourceNodeUid: sourceModuleUid,
						targetNodeUid: targetModuleUid,
						type: EdgeType.IMPORTS,
						resolution: Resolution.STATIC,
						extractor: INDEXER_VERSION,
						location: null,
						metadataJson: null,
					});
				}
			}
		}

		return edges;
	}

	// ── Edge resolution ────────────────────────────────────────────────

	private resolveEdges(
		unresolved: UnresolvedEdge[],
		allNodes: GraphNode[],
		trackedFiles: TrackedFile[],
		repoUid: string,
		snapshotUid: string,
	): {
		resolved: GraphEdge[];
		unresolvedCount: number;
		unresolvedBreakdown: Record<string, number>;
	} {
		// Build lookup maps
		const nodesByStableKey = new Map<string, GraphNode>();
		const nodesByName = new Map<string, GraphNode[]>();

		for (const node of allNodes) {
			nodesByStableKey.set(node.stableKey, node);
			// Index by short name and qualified name for call resolution
			const existing = nodesByName.get(node.name) ?? [];
			existing.push(node);
			nodesByName.set(node.name, existing);
			if (node.qualifiedName && node.qualifiedName !== node.name) {
				const existingQ = nodesByName.get(node.qualifiedName) ?? [];
				existingQ.push(node);
				nodesByName.set(node.qualifiedName, existingQ);
			}
		}

		// Build file resolution map: extensionless path → file stable key
		const fileResolution = new Map<string, string>();
		for (const file of trackedFiles) {
			const stableKey = `${repoUid}:${file.path}:FILE`;
			// Map the full path with extension
			fileResolution.set(`${repoUid}:${file.path}:FILE`, stableKey);
			// Map extensionless variants for import resolution
			const withoutExt = stripExtension(file.path);
			const extlessKey = `${repoUid}:${withoutExt}:FILE`;
			if (!fileResolution.has(extlessKey)) {
				fileResolution.set(extlessKey, stableKey);
			}
			// Also handle index file resolution: "src/foo" → "src/foo/index.ts"
			if (file.path.endsWith("/index.ts") || file.path.endsWith("/index.tsx")) {
				const dirPath = file.path.replace(/\/index\.tsx?$/, "");
				const dirKey = `${repoUid}:${dirPath}:FILE`;
				if (!fileResolution.has(dirKey)) {
					fileResolution.set(dirKey, stableKey);
				}
			}
		}

		const resolved: GraphEdge[] = [];
		let unresolvedCount = 0;
		const unresolvedBreakdown: Record<string, number> = {};

		for (const edge of unresolved) {
			const targetNodeUid = this.resolveTarget(
				edge,
				nodesByStableKey,
				nodesByName,
				fileResolution,
			);

			if (targetNodeUid) {
				resolved.push({
					edgeUid: edge.edgeUid,
					snapshotUid,
					repoUid,
					sourceNodeUid: edge.sourceNodeUid,
					targetNodeUid,
					type: edge.type,
					resolution: edge.resolution,
					extractor: edge.extractor,
					location: edge.location,
					metadataJson: edge.metadataJson,
				});
			} else {
				unresolvedCount++;
				// Classify the unresolved edge for diagnostics
				const category = classifyUnresolvedEdge(edge);
				unresolvedBreakdown[category] =
					(unresolvedBreakdown[category] ?? 0) + 1;
			}
		}

		return { resolved, unresolvedCount, unresolvedBreakdown };
	}

	private resolveTarget(
		edge: UnresolvedEdge,
		nodesByStableKey: Map<string, GraphNode>,
		nodesByName: Map<string, GraphNode[]>,
		fileResolution: Map<string, string>,
	): string | null {
		switch (edge.type) {
			case EdgeType.IMPORTS:
				return this.resolveImportTarget(
					edge.targetKey,
					nodesByStableKey,
					fileResolution,
				);
			case EdgeType.CALLS:
				return this.resolveCallTarget(
					edge.targetKey,
					edge.sourceNodeUid,
					nodesByStableKey,
					nodesByName,
				);
			case EdgeType.INSTANTIATES:
				return this.resolveNamedTarget(edge.targetKey, nodesByName, edge.type);
			case EdgeType.IMPLEMENTS:
				return this.resolveNamedTarget(edge.targetKey, nodesByName, edge.type);
			default:
				return this.resolveNamedTarget(edge.targetKey, nodesByName, edge.type);
		}
	}

	private resolveImportTarget(
		targetKey: string,
		nodesByStableKey: Map<string, GraphNode>,
		fileResolution: Map<string, string>,
	): string | null {
		// Direct match (already has extension in stable key)
		const directNode = nodesByStableKey.get(targetKey);
		if (directNode) return directNode.nodeUid;

		// Try the file resolution map (extensionless → with extension)
		const resolvedKey = fileResolution.get(targetKey);
		if (resolvedKey) {
			const node = nodesByStableKey.get(resolvedKey);
			if (node) return node.nodeUid;
		}

		return null;
	}

	private resolveCallTarget(
		targetKey: string,
		_sourceNodeUid: string,
		_nodesByStableKey: Map<string, GraphNode>,
		nodesByName: Map<string, GraphNode[]>,
	): string | null {
		// For "this.foo.bar()" style calls, try the last segment as a method name
		// e.g. "this.repo.findById" → look for a method named "findById"
		if (targetKey.includes(".")) {
			const parts = targetKey.split(".");
			const methodName = parts[parts.length - 1];

			// If it starts with "this.", try to find the method by qualified name
			// on the class that owns the calling method
			if (parts[0] === "this" && parts.length >= 3) {
				// "this.repo.findById" — try matching "*.findById" across all classes
				const resolved = this.pickUnambiguous(
					nodesByName.get(methodName),
					EdgeType.CALLS,
				);
				if (resolved) return resolved;
				// If ambiguous (multiple methods with the same name across classes),
				// we cannot disambiguate without type information — leave unresolved.
				// Future: use the property name (parts[1], e.g. "repo") to narrow
				// by matching the field type to a class that defines the method.
			}

			// For "obj.method()" where obj is not "this", try method name
			const resolved = this.pickUnambiguous(
				nodesByName.get(methodName),
				EdgeType.CALLS,
			);
			if (resolved) return resolved;
		}

		// Simple function call: "generateId" → look for a function/method with that name
		return this.pickUnambiguous(nodesByName.get(targetKey), EdgeType.CALLS);
	}

	private resolveNamedTarget(
		targetKey: string,
		nodesByName: Map<string, GraphNode[]>,
		edgeType: EdgeType,
	): string | null {
		return this.pickUnambiguous(nodesByName.get(targetKey), edgeType);
	}

	// ── Orphaned declaration detection ─────────────────────────────────

	/**
	 * Count active symbol-targeting declarations whose target_stable_key
	 * does not match any node in the given snapshot.
	 *
	 * Only checks entrypoint and invariant declarations, which reference
	 * symbol stable_keys. Module and boundary declarations use :MODULE
	 * keys and are not affected by symbol identity changes.
	 */
	private countOrphanedDeclarations(
		repoUid: string,
		snapshotUid: string,
	): number {
		const symbolDeclarationKinds = [
			DeclarationKind.ENTRYPOINT,
			DeclarationKind.INVARIANT,
		];

		let orphaned = 0;
		for (const kind of symbolDeclarationKinds) {
			const decls = this.storage.getActiveDeclarations({ repoUid, kind });
			for (const decl of decls) {
				const node = this.storage.getNodeByStableKey(
					snapshotUid,
					decl.targetStableKey,
				);
				if (!node) {
					orphaned++;
				}
			}
		}
		return orphaned;
	}

	// ── Disambiguation ────────────────────────────────────────────────

	/**
	 * Given a list of candidate nodes sharing the same name, return the
	 * node_uid of the unique match, or null if ambiguous/missing.
	 *
	 * When multiple candidates exist (e.g. a CLASS and a companion
	 * TYPE_ALIAS both named "Foo"), the edge type determines which
	 * declaration space to prefer:
	 *
	 *   INSTANTIATES → CLASS  (you can only `new` a class)
	 *   IMPLEMENTS   → INTERFACE  (you implement interfaces)
	 *   CALLS        → value-space symbols (not TYPE_ALIAS or INTERFACE)
	 *
	 * This is not a heuristic — TypeScript's declaration spaces are
	 * well-defined. A `new Foo()` structurally cannot target a type alias.
	 */
	private pickUnambiguous(
		candidates: GraphNode[] | undefined,
		edgeType: EdgeType,
	): string | null {
		if (!candidates || candidates.length === 0) return null;

		// Always apply affinity filtering first, even for singletons.
		// A lone interface node must not satisfy an INSTANTIATES edge —
		// `new Foo()` cannot target an interface. Declaration-space
		// correctness takes priority over the singleton fast path.
		const filtered = filterByEdgeAffinity(candidates, edgeType);
		if (filtered.length === 1) return filtered[0].nodeUid;

		// Zero after filtering: the only candidates were in the wrong
		// declaration space (e.g. interface-only for INSTANTIATES).
		// More than one: genuinely ambiguous even within the correct space.
		return null;
	}
}

// ── Utility functions ──────────────────────────────────────────────────

function hashContent(content: string): string {
	return createHash("sha256").update(content).digest("hex").slice(0, 16);
}

function detectLanguage(filePath: string): string | null {
	const ext = getExtension(filePath);
	switch (ext) {
		case ".ts":
			return "typescript";
		case ".tsx":
			return "tsx";
		case ".js":
			return "javascript";
		case ".jsx":
			return "jsx";
		default:
			return null;
	}
}

function isTestFile(filePath: string): boolean {
	return (
		filePath.includes("__tests__") ||
		filePath.includes(".test.") ||
		filePath.includes(".spec.") ||
		filePath.includes("/test/") ||
		filePath.includes("/tests/")
	);
}

function getExtension(filePath: string): string {
	const dot = filePath.lastIndexOf(".");
	return dot >= 0 ? filePath.slice(dot) : "";
}

function stripExtension(filePath: string): string {
	const dot = filePath.lastIndexOf(".");
	if (dot < 0) return filePath;
	const ext = filePath.slice(dot);
	if ([".ts", ".tsx", ".js", ".jsx"].includes(ext)) {
		return filePath.slice(0, dot);
	}
	return filePath;
}

function toPosixPath(p: string): string {
	return p.split("\\").join("/");
}

/**
 * Simple glob matching for exclude/include patterns.
 * Supports:
 *   "*.test.ts" — matches any file ending in .test.ts
 *   "src/utils/*" — matches any file directly in src/utils/
 *   "**\/__tests__/**" — matches any path containing __tests__
 */
function matchSimpleGlob(filePath: string, pattern: string): boolean {
	// Convert glob to regex
	const regexStr = pattern
		.replace(/\./g, "\\.")
		.replace(/\*\*/g, "{{GLOBSTAR}}")
		.replace(/\*/g, "[^/]*")
		.replace(/\{\{GLOBSTAR\}\}/g, ".*");
	return new RegExp(`^${regexStr}$`).test(filePath);
}

/**
 * Check if a path or directory name matches any exclusion pattern.
 * Supports exact names, exact paths, and simple globs.
 */
function isExcluded(
	relPath: string,
	name: string,
	excludePatterns: string[],
): boolean {
	for (const pattern of excludePatterns) {
		// Exact match on name or path
		if (pattern === name || pattern === relPath) return true;
		// Glob match on path
		if (matchSimpleGlob(relPath, pattern)) return true;
	}
	return false;
}

/**
 * Get the module path (directory) that a file belongs to.
 */
function getModulePath(filePath: string): string | null {
	const lastSlash = filePath.lastIndexOf("/");
	return lastSlash > 0 ? filePath.slice(0, lastSlash) : null;
}

/**
 * Load the root .gitignore from a repository.
 * Returns an Ignore instance that can test paths.
 * If no .gitignore exists, returns an instance that ignores nothing.
 *
 * Limitation: only reads the root-level .gitignore. Nested .gitignore
 * files in subdirectories are not loaded. This covers the common case
 * for TypeScript/JavaScript repos. Full nested .gitignore support
 * (loading per-directory .gitignore files during the walk) is deferred
 * to v2.
 */
async function loadGitignore(rootPath: string): Promise<Ignore> {
	const ig = ignore();
	const gitignorePath = join(rootPath, ".gitignore");
	if (existsSync(gitignorePath)) {
		const content = await readFile(gitignorePath, "utf-8");
		ig.add(content);
	}
	return ig;
}

/**
 * TypeScript type-only subtypes: exist only at compile time.
 * These can never be the runtime target of CALLS or INSTANTIATES.
 */
export const TYPE_ONLY_SUBTYPES: ReadonlySet<string | null> = new Set([
	NodeSubtype.TYPE_ALIAS,
	NodeSubtype.INTERFACE,
]);

/**
 * When the resolver finds multiple nodes sharing the same name,
 * use the edge type to pick the correct declaration space.
 *
 * TypeScript legally allows the same identifier in both value and
 * type namespaces (e.g. `export const Foo = {}; export type Foo = ...`).
 * The edge type tells us which namespace the reference lives in:
 *
 *   INSTANTIATES → must be a CLASS (runtime `new`)
 *   IMPLEMENTS   → must be an INTERFACE
 *   CALLS        → must be a value-space symbol (not type-only)
 *
 * Returns the filtered subset. If filtering empties the list,
 * returns the original candidates so the caller can still apply
 * its length === 1 check.
 */
export function filterByEdgeAffinity(
	candidates: GraphNode[],
	edgeType: EdgeType,
): GraphNode[] {
	let filtered: GraphNode[];

	switch (edgeType) {
		case EdgeType.INSTANTIATES:
			filtered = candidates.filter((n) => n.subtype === NodeSubtype.CLASS);
			break;
		case EdgeType.IMPLEMENTS:
			filtered = candidates.filter((n) => n.subtype === NodeSubtype.INTERFACE);
			break;
		case EdgeType.CALLS:
			filtered = candidates.filter((n) => !TYPE_ONLY_SUBTYPES.has(n.subtype));
			break;
		default:
			return candidates;
	}

	// If filtering removed everything, return the empty list.
	// This is correct: if the only candidate for INSTANTIATES is an
	// interface, the edge genuinely cannot resolve. Falling back to
	// the unfiltered list would create a false positive (a runtime
	// edge pointing at a type-only declaration).
	return filtered;
}

/**
 * Classify an unresolved edge into a diagnostic category
 * for the breakdown report.
 */
function classifyUnresolvedEdge(edge: UnresolvedEdge): string {
	const type = edge.type;

	if (type === EdgeType.IMPORTS) {
		return "IMPORTS (file not found)";
	}

	if (type === EdgeType.INSTANTIATES) {
		return "INSTANTIATES (class not found)";
	}

	if (type === EdgeType.IMPLEMENTS) {
		return "IMPLEMENTS (interface not found)";
	}

	// CALLS breakdown: use the raw call name (before receiver-type rewriting)
	// to classify accurately. The extractor stores the original call text in
	// metadataJson.rawCalleeName when a rewrite occurs. Without this, a
	// rewritten "this.save()" → "ClassName.save" would be misclassified as
	// "obj.method" instead of "this.method".
	if (type === EdgeType.CALLS) {
		let key = edge.targetKey;
		if (edge.metadataJson) {
			try {
				const meta = JSON.parse(edge.metadataJson);
				if (meta.rawCalleeName) {
					key = meta.rawCalleeName;
				}
			} catch {
				// malformed metadata — use targetKey as-is
			}
		}

		if (key.startsWith("this.")) {
			if (key.split(".").length > 2) {
				return "CALLS this.*.method (needs type info)";
			}
			return "CALLS this.method (needs class context)";
		}
		if (key.includes(".")) {
			return "CALLS obj.method (needs type info)";
		}
		return "CALLS function (ambiguous or missing)";
	}

	return `${type} (other)`;
}
