import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import ignore, { type Ignore } from "ignore";
import { v4 as uuidv4 } from "uuid";
import type {
	GraphEdge,
	GraphNode,
	Repo,
	Snapshot,
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
import { UnresolvedEdgeCategory } from "../../core/diagnostics/unresolved-edge-categories.js";
import { CURRENT_CLASSIFIER_VERSION } from "../../core/diagnostics/unresolved-edge-classification.js";
import {
	getMatchStrategy,
	matchBoundaryFacts,
} from "../../core/classification/boundary-matcher.js";
import { detectFrameworkBoundary } from "../../core/classification/framework-boundary.js";
import { detectLambdaEntrypoints } from "../../core/classification/framework-entrypoints.js";
import { classifyUnresolvedEdge } from "../../core/classification/unresolved-classifier.js";
import {
	emptyFileSignals,
	type FileSignals,
	type PackageDependencySet,
	type SnapshotSignals,
	type TsconfigAliases,
} from "../../core/classification/signals.js";
import type {
	ExtractionResult,
	ExtractorPort,
	ImportBinding,
	UnresolvedEdge,
} from "../../core/ports/extractor.js";
import type {
	IndexerPort,
	IndexOptions,
	IndexProgressEvent,
	IndexResult,
} from "../../core/ports/indexer.js";
import type {
	FileSignalRow,
	InferenceRow,
	Measurement,
	PersistedBoundaryConsumerFact,
	PersistedBoundaryLink,
	PersistedBoundaryProviderFact,
	PersistedUnresolvedEdge,
	ResolverNode,
	StoragePort,
} from "../../core/ports/storage.js";
import { detectLinuxSystemPatterns } from "../extractors/cpp/linux-system-detector.js";
import { detectSpringBeans } from "../extractors/java/spring-bean-detector.js";
import { detectPytestItems } from "../extractors/python/pytest-detector.js";
import { extractSpringRoutes, initSpringRouteParser } from "../extractors/java/spring-route-extractor.js";
import { extractMakefileConsumers } from "../extractors/cli/makefile-cli-extractor.js";
import { extractShellScriptConsumers } from "../extractors/cli/shell-script-cli-extractor.js";
import { extractCommanderCommands } from "../extractors/typescript/commander-command-extractor.js";
import { extractPackageScriptConsumers } from "../extractors/manifest/package-script-cli-extractor.js";
import { extractExpressRoutes } from "../extractors/typescript/express-route-extractor.js";
import { FileLocalStringResolver } from "../extractors/typescript/file-local-string-resolver.js";
import { extractHttpClientRequests } from "../extractors/typescript/http-client-extractor.js";
import { readCargoDependencies } from "../config/cargo-reader.js";
import { readCompileCommands, type CompilationDatabase } from "../config/compile-commands-reader.js";
import { readGradleDependencies } from "../config/gradle-reader.js";
import { readPythonDependencies } from "../config/python-deps-reader.js";
import { readTsconfigAliases } from "../config/tsconfig-reader.js";
import type { AnnotationsPort } from "../../core/ports/annotations.js";
import type { ModuleDiscoveryPort } from "../../core/ports/discovery.js";
import { discoverModules } from "../../core/modules/module-discovery.js";
// Delta indexing imports — used by refreshRepo once the build
// pipeline is decomposed into reusable phase helpers.
// import { buildInvalidationPlan, type CurrentFileState } from "../../core/delta/invalidation-planner.js";
// import type { InvalidationPlan } from "../../core/delta/invalidation-plan.js";
import {
	applyContentTruncation,
	attributePackageDescription,
	attributeReadme,
	isEmptyContent,
	preferReadmeFile,
	resolveCollisions,
} from "../../core/annotations/attribution.js";
import {
	AnnotationContractClass,
	AnnotationKind,
	type Annotation,
} from "../../core/annotations/types.js";
import { extractPackageDescription } from "../annotations/extractors/package-description-extractor.js";
import { extractReadme } from "../annotations/extractors/readme-extractor.js";
import {
	buildToolchainJson,
	INDEXER_VERSION,
	MANIFEST_EXTRACTOR_VERSION,
} from "../../version.js";
import {
	extractPackageDependencies,
	extractPackageManifest,
} from "../extractors/manifest/package-json.js";

/**
 * Pre-built resolver index for edge resolution and module-edge creation.
 * Built once from row-at-a-time DB iteration (no bulk .all()).
 * Shared across all edge resolution batches and module-edge derivation.
 */
interface ResolverIndex {
	nodesByStableKey: Map<string, ResolverNode>;
	nodesByName: Map<string, ResolverNode[]>;
	nodeUidToFileUid: Map<string, string>;
	fileResolution: Map<string, string>;
	perFileIncludeResolution: Map<string, Map<string, string>>;
	/** For module-edge creation: stableKey → nodeUid. */
	stableKeyToUid: Map<string, string>;
	/** For module-edge creation: file nodeUid → module stableKey. */
	fileToModule: Map<string, string>;
}

/**
 * Output of the extraction phase. Passed to downstream phases
 * (resolution, detectors, finalization) as a boundary DTO.
 * Both full-index and refresh orchestrators produce this shape.
 */
interface ExtractionPhaseResult {
	trackedFiles: TrackedFile[];
	nodesTotal: number;
	extractionEdgesTotal: number;
	allMetrics: Map<string, { cc: number; params: number; nesting: number }>;
	fileSignalsCache: Map<string, FileSignals>;
	skippedOversized: number;
	filesReadFailed: number;
}


/** All file extensions that any registered extractor might handle. */
const ALL_SOURCE_EXTENSIONS = new Set([
	".ts", ".tsx", ".js", ".jsx",  // TypeScript/JavaScript
	".rs",                          // Rust
	".java",                        // Java
	".py",                          // Python
	".c", ".h",                     // C
	".cpp", ".hpp", ".cc", ".cxx", ".hxx",  // C++
]);

/**
 * Map a language ID (from ExtractorPort.languages) to file extensions.
 */
function languageToExtensions(lang: string): string[] {
	switch (lang) {
		case "typescript":
			return [".ts"];
		case "tsx":
			return [".tsx", ".jsx"];
		case "javascript":
			return [".js"];
		case "rust":
			return [".rs"];
		case "java":
			return [".java"];
		case "python":
			return [".py"];
		case "c":
			return [".c", ".h"];
		case "cpp":
			return [".cpp", ".hpp", ".cc", ".cxx", ".hxx"];
		default:
			return [];
	}
}

/**
 * Maximum file size in bytes for extraction. Files above this threshold
 * are skipped entirely. This is an operational containment measure for
 * pathological files (generated register headers, concatenated outputs)
 * that would consume excessive memory in the WASM tree-sitter runtime.
 *
 * 1 MB ≈ ~25,000 lines of typical C/C++ code. The largest non-generated
 * source files in the Linux kernel are under 30,000 lines. Generated
 * register mask headers can exceed 200,000 lines (8+ MB).
 */
const MAX_FILE_SIZE_BYTES = 1_000_000; // 1 MB

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
	/**
	 * Optional AnnotationsPort. When present, the indexer extracts
	 * README + package.json-description annotations during the
	 * indexing pass and persists them via the port.
	 *
	 * When absent (e.g. in tests that don't exercise annotations),
	 * annotation extraction is silently skipped. The hard-rule
	 * isolation invariant (annotations-contract.txt §7) holds
	 * either way: the indexer WRITES annotations but NEVER reads them.
	 */
	/** Map of file extension → ExtractorPort. e.g. ".ts" → TypeScriptExtractor */
	private extractorsByExtension: Map<string, ExtractorPort>;
	/** Lazy-initialized string resolver for boundary extraction. */
	private stringResolver: FileLocalStringResolver | null = null;
	/** Whether the Spring route parser has been initialized. */
	private springParserReady = false;

	constructor(
		private storage: StoragePort,
		extractors: ExtractorPort | ExtractorPort[],
		private annotations?: AnnotationsPort,
		private discovery?: ModuleDiscoveryPort,
	) {
		// Build extension → extractor lookup from each extractor's declared languages.
		this.extractorsByExtension = new Map();
		const list = Array.isArray(extractors) ? extractors : [extractors];
		for (const ext of list) {
			for (const lang of ext.languages) {
				// Map language IDs to file extensions. Convention:
				// "typescript" → .ts, "tsx" → .tsx, "rust" → .rs, etc.
				const extensions = languageToExtensions(lang);
				for (const fileExt of extensions) {
					this.extractorsByExtension.set(fileExt, ext);
				}
			}
		}
	}

	/** Get the extractor for a file, or null if unsupported. */
	private getExtractorForFile(filePath: string): ExtractorPort | null {
		const ext = filePath.slice(filePath.lastIndexOf("."));
		return this.extractorsByExtension.get(ext) ?? null;
	}


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

		// 1. Create snapshot with toolchain provenance and basis commit.
		// basisCommit comes from options (resolved by the composition root
		// via GitPort). The indexer itself does not call git.
		const snapshot = this.storage.createSnapshot({
			repoUid,
			kind: snapshotKind,
			parentSnapshotUid: parentSnapshot?.snapshotUid,
			basisCommit: options?.basisCommit,
			toolchainJson: JSON.stringify(buildToolchainJson()),
		});

		try {
			// 1a. Read snapshot-level classifier signals BEFORE extraction.
			// These degrade to empty on any read/parse failure — indexing
			// must not fail because classifier inputs are unavailable.
			const snapshotSignals = this.buildSnapshotSignals();

			// 1b. Read compile_commands.json for C/C++ include path resolution.
			// Optional: if absent, C/C++ #include resolution falls back to
			// direct filename matching only.
			const compileDb = await readCompileCommands(repo.rootPath);

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

			const extraction = await this.extractFilesIntoSnapshot(
				snapshot,
				repo,
				filePaths,
				repoUid,
				emit,
			);
			const {
				trackedFiles,
				nodesTotal,
				extractionEdgesTotal,
				allMetrics,
				fileSignalsCache,
				skippedOversized,
				filesReadFailed,
			} = extraction;

			// ═══════════════════════════════════════════════════════════════
			// PHASE 3: Resolve & Classify + Module edges
			// ═══════════════════════════════════════════════════════════════
			const resolution = this.resolveSnapshotEdges(
				snapshot,
				trackedFiles,
				repoUid,
				snapshotSignals,
				compileDb,
				extractionEdgesTotal,
				options?.edgeBatchSize,
				emit,
			);

			const { resolvedTotal, unresolvedCount, unresolvedBreakdown, moduleEdgesCount } = resolution;

			// ═══════════════════════════════════════════════════════════════
			// PHASE 3d + 4: Module discovery, detectors, boundary extraction
			// ═══════════════════════════════════════════════════════════════
			await this.runPostpasses(
				snapshot,
				repo,
				filePaths,
				trackedFiles,
				repoUid,
				fileSignalsCache,
			);

			// ═══════════════════════════════════════════════════════════════
			// PHASE 5: Finalize
			// ═══════════════════════════════════════════════════════════════
			return this.finalizeSnapshot(
				snapshot,
				repo,
				repoUid,
				trackedFiles,
				nodesTotal,
				resolvedTotal,
				moduleEdgesCount,
				unresolvedCount,
				unresolvedBreakdown,
				allMetrics,
				skippedOversized,
				filesReadFailed,
				startTime,
				emit,
			);

		} catch (err) {
			// Mark snapshot as failed
			this.storage.updateSnapshotStatus({
				snapshotUid: snapshot.snapshotUid,
				status: SnapshotStatus.FAILED,
			});
			throw err;
		}
	}

	async refreshRepo(
		repoUid: string,
		options?: IndexOptions,
	): Promise<IndexResult> {
		const repo = this.storage.getRepo({ uid: repoUid });
		if (!repo) {
			throw new Error(`Repository not found: ${repoUid}`);
		}

		// TODO: delta indexing implementation.
		// Currently falls through to full re-extraction with parent link.
		// The correct implementation will use:
		//   1. buildInvalidationPlan (support module, shipped)
		//   2. copyForwardUnchangedFiles (storage, shipped)
		//   3. executeBuildPipeline (shared internal phases, not yet extracted)
		// Blocked on: runIndex decomposition into reusable phase helpers.
		return this.runIndex(repoUid, SnapshotKind.REFRESH, options);
	}

	// ── Phase helpers (shared between full and refresh orchestrators) ──

	/**
	 * Phase 1 + 2: Extract files and create MODULE nodes.
	 *
	 * Per-file: read → extract → persist nodes → persist extraction edges
	 * → persist file signals → discard source text.
	 * Then create MODULE nodes from directory structure.
	 *
	 * Returns an ExtractionPhaseResult DTO for downstream phases.
	 * Both full-index and refresh orchestrators call this with their
	 * respective file lists.
	 */
	private async extractFilesIntoSnapshot(
		snapshot: Snapshot,
		repo: Repo,
		filePaths: string[],
		repoUid: string,
		emit: (event: IndexProgressEvent) => void,
	): Promise<ExtractionPhaseResult> {
		const trackedFiles: TrackedFile[] = [];
		let nodesTotal = 0;
		let extractionEdgesTotal = 0;

		const allMetrics = new Map<
			string,
			{ cc: number; params: number; nesting: number }
		>();

		// Minimal import-bindings-only cache for Lambda entrypoint
		// detection (Phase 4). Classification loads signals per-batch
		// from the file_signals table.
		const fileSignalsCache = new Map<string, FileSignals>();

		const packageDepsCache = new Map<string, PackageDependencySet>();
		const tsconfigAliasesCache = new Map<string, TsconfigAliases>();
		let skippedOversized = 0;
		let filesReadFailed = 0;
		let extractIdx = 0;

		for (const relPath of filePaths) {
			emit({
				phase: "extracting",
				current: extractIdx,
				total: filePaths.length,
				file: relPath,
			});

			const absPath = join(repo.rootPath, relPath);
			let content: string;
			try {
				content = await readFile(absPath, "utf-8");
			} catch {
				const failedFileUid = `${repoUid}:${relPath}`;
				trackedFiles.push({
					fileUid: failedFileUid,
					repoUid,
					path: relPath,
					language: detectLanguage(relPath),
					isTest: isTestFile(relPath),
					isGenerated: false,
					isExcluded: false,
				});
				this.storage.upsertFiles([trackedFiles[trackedFiles.length - 1]]);
				this.storage.upsertFileVersions([{
					snapshotUid: snapshot.snapshotUid,
					fileUid: failedFileUid,
					contentHash: "",
					astHash: null,
					extractor: (this.getExtractorForFile(relPath)?.name ?? "unknown"),
					parseStatus: ParseStatus.FAILED,
					sizeBytes: 0,
					lineCount: 0,
					indexedAt: new Date().toISOString(),
				}]);
				filesReadFailed++;
				extractIdx++;
				continue;
			}

			if (content.length > MAX_FILE_SIZE_BYTES) {
				const skippedFileUid = `${repoUid}:${relPath}`;
				trackedFiles.push({
					fileUid: skippedFileUid,
					repoUid,
					path: relPath,
					language: detectLanguage(relPath),
					isTest: isTestFile(relPath),
					isGenerated: false,
					isExcluded: true,
				});
				this.storage.upsertFiles([trackedFiles[trackedFiles.length - 1]]);
				this.storage.upsertFileVersions([{
					snapshotUid: snapshot.snapshotUid,
					fileUid: skippedFileUid,
					contentHash: hashContent(content),
					astHash: null,
					extractor: "skipped:oversized",
					parseStatus: ParseStatus.SKIPPED,
					sizeBytes: Buffer.byteLength(content, "utf-8"),
					lineCount: content.split("\n").length,
					indexedAt: new Date().toISOString(),
				}]);
				skippedOversized++;
				extractIdx++;
				continue;
			}

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

			this.storage.upsertFiles([trackedFiles[trackedFiles.length - 1]]);
			this.storage.upsertFileVersions([{
				snapshotUid: snapshot.snapshotUid,
				fileUid,
				contentHash,
				astHash: null,
				extractor: (this.getExtractorForFile(relPath)?.name ?? "unknown"),
				parseStatus: ParseStatus.PARSED,
				sizeBytes: Buffer.byteLength(content, "utf-8"),
				lineCount: content.split("\n").length,
				indexedAt: new Date().toISOString(),
			}]);
			const extractor = this.getExtractorForFile(relPath);
			if (!extractor) {
				extractIdx++;
				continue;
			}
			let result: ExtractionResult;
			try {
				result = await extractor.extract(
					content,
					relPath,
					fileUid,
					repoUid,
					snapshot.snapshotUid,
				);
			} catch {
				this.storage.upsertFileVersions([{
					snapshotUid: snapshot.snapshotUid,
					fileUid,
					contentHash,
					astHash: null,
					extractor: (this.getExtractorForFile(relPath)?.name ?? "unknown"),
					parseStatus: ParseStatus.FAILED,
					sizeBytes: Buffer.byteLength(content, "utf-8"),
					lineCount: content.split("\n").length,
					indexedAt: new Date().toISOString(),
				}]);
				extractIdx++;
				continue;
			}

			this.storage.insertNodes(result.nodes);
			nodesTotal += result.nodes.length;

			if (result.edges.length > 0) {
				this.storage.insertExtractionEdges(result.edges.map((e) => ({
					...e,
					lineStart: e.location?.lineStart ?? null,
					colStart: e.location?.colStart ?? null,
					lineEnd: e.location?.lineEnd ?? null,
					colEnd: e.location?.colEnd ?? null,
					sourceFileUid: fileUid,
				})));
				extractionEdgesTotal += result.edges.length;
			}
			for (const [key, m] of result.metrics) {
				allMetrics.set(key, {
					cc: m.cyclomaticComplexity,
					params: m.parameterCount,
					nesting: m.maxNestingDepth,
				});
			}

			const sameFileValueSymbols = new Set<string>();
			const sameFileClassSymbols = new Set<string>();
			const sameFileInterfaceSymbols = new Set<string>();
			for (const node of result.nodes) {
				if (node.kind === NodeKind.SYMBOL) {
					const sub = node.subtype;
					if (
						sub === NodeSubtype.FUNCTION ||
						sub === NodeSubtype.CLASS ||
						sub === NodeSubtype.METHOD ||
						sub === NodeSubtype.VARIABLE ||
						sub === NodeSubtype.CONSTANT ||
						sub === NodeSubtype.ENUM ||
						sub === NodeSubtype.ENUM_MEMBER ||
						sub === NodeSubtype.CONSTRUCTOR ||
						sub === NodeSubtype.GETTER ||
						sub === NodeSubtype.SETTER ||
						sub === NodeSubtype.PROPERTY
					) {
						sameFileValueSymbols.add(node.name);
					}
					if (sub === NodeSubtype.CLASS) {
						sameFileClassSymbols.add(node.name);
					}
					if (sub === NodeSubtype.INTERFACE) {
						sameFileInterfaceSymbols.add(node.name);
					}
				}
			}
			const packageDependencies = await this.resolveNearestPackageDeps(
				relPath,
				repo.rootPath,
				packageDepsCache,
			);
			const tsconfigAliases = await this.resolveNearestTsconfigAliases(
				relPath,
				repo.rootPath,
				tsconfigAliasesCache,
			);
			const hasBindings = result.importBindings.length > 0;
			const hasDeps = packageDependencies.names.length > 0;
			const hasAliases = tsconfigAliases.entries.length > 0;
			if (hasBindings || hasDeps || hasAliases) {
				this.storage.insertFileSignals([{
					snapshotUid: snapshot.snapshotUid,
					fileUid,
					importBindingsJson: hasBindings
						? JSON.stringify(result.importBindings)
						: null,
					packageDependenciesJson: hasDeps
						? JSON.stringify(packageDependencies)
						: null,
					tsconfigAliasesJson: hasAliases
						? JSON.stringify(tsconfigAliases)
						: null,
				}]);
			}

			if (hasBindings) {
				fileSignalsCache.set(fileUid, {
					importBindings: result.importBindings,
					sameFileValueSymbols: new Set(),
					sameFileClassSymbols: new Set(),
					sameFileInterfaceSymbols: new Set(),
					packageDependencies: { names: [] },
					tsconfigAliases: { entries: [] },
				});
			}

			extractIdx++;
		}

		emit({
			phase: "extracting",
			current: filePaths.length,
			total: filePaths.length,
		});

		// Phase 2: MODULE nodes.
		const moduleNodes = this.createModuleNodes(
			filePaths,
			repoUid,
			snapshot.snapshotUid,
		);
		if (moduleNodes.length > 0) {
			this.storage.insertNodes(moduleNodes);
			nodesTotal += moduleNodes.length;
		}

		return {
			trackedFiles,
			nodesTotal,
			extractionEdgesTotal,
			allMetrics,
			fileSignalsCache,
			skippedOversized,
			filesReadFailed,
		};
	}

	/**
	 * Phase 3: Resolve extraction edges, classify unresolved, create
	 * module-level edges. Operates on whatever nodes and extraction
	 * edges exist in the snapshot (from extraction or copy-forward).
	 */
	private resolveSnapshotEdges(
		snapshot: Snapshot,
		trackedFiles: TrackedFile[],
		repoUid: string,
		snapshotSignals: SnapshotSignals,
		compileDb: CompilationDatabase | null,
		extractionEdgesTotal: number,
		edgeBatchSize?: number,
		emit: (event: IndexProgressEvent) => void = () => {},
	): { resolvedTotal: number; unresolvedCount: number; unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>>; moduleEdgesCount: number } {
		emit({ phase: "persisting", current: 0, total: 0 });

		const resolverIndex = this.buildResolverIndex(
			snapshot.snapshotUid,
			trackedFiles,
			repoUid,
			compileDb,
		);

		const BATCH_SIZE = edgeBatchSize ?? 10_000;
		let edgeCursor: string | null = null;
		let resolvedTotal = 0;
		let unresolvedCount = 0;
		const resolvedImportPairs: Array<[string, string]> = [];
		const unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>> = {};
		const classificationObservedAt = new Date().toISOString();

		emit({ phase: "resolving", current: 0, total: extractionEdgesTotal });

		// eslint-disable-next-line no-constant-condition
		while (true) {
			const batch = this.storage.queryExtractionEdgesBatch(
				snapshot.snapshotUid,
				BATCH_SIZE,
				edgeCursor,
			);
			if (batch.length === 0) break;
			edgeCursor = batch[batch.length - 1].edgeUid;

			const batchUnresolved: UnresolvedEdge[] = batch.map((se) => ({
				edgeUid: se.edgeUid,
				snapshotUid: se.snapshotUid,
				repoUid: se.repoUid,
				sourceNodeUid: se.sourceNodeUid,
				targetKey: se.targetKey,
				type: se.type as any,
				resolution: se.resolution as any,
				extractor: se.extractor,
				location: se.lineStart !== null ? {
					lineStart: se.lineStart,
					colStart: se.colStart ?? 0,
					lineEnd: se.lineEnd ?? se.lineStart,
					colEnd: se.colEnd ?? 0,
				} : null,
				metadataJson: se.metadataJson,
			}));

			const batchSourceFileUids = new Set<string>();
			for (const se of batch) {
				if (se.sourceFileUid) batchSourceFileUids.add(se.sourceFileUid);
				if (se.sourceFileUid) {
					resolverIndex.nodeUidToFileUid.set(se.sourceNodeUid, se.sourceFileUid);
				}
			}
			const batchSignals = new Map<string, FileSignalRow>();
			const batchImportBindings = new Map<string, ImportBinding[]>();
			if (batchSourceFileUids.size > 0) {
				const signalRows = this.storage.queryFileSignalsBatch(
					snapshot.snapshotUid,
					[...batchSourceFileUids],
				);
				for (const row of signalRows) {
					batchSignals.set(row.fileUid, row);
					if (row.importBindingsJson) {
						const bindings = JSON.parse(row.importBindingsJson) as ImportBinding[];
						if (bindings.length > 0) {
							batchImportBindings.set(row.fileUid, bindings);
						}
					}
				}
			}

			const batchResult = this.resolveEdges(
				batchUnresolved,
				resolverIndex,
				repoUid,
				snapshot.snapshotUid,
				batchImportBindings,
			);

			if (batchResult.resolved.length > 0) {
				this.storage.insertEdges(batchResult.resolved);
				for (const e of batchResult.resolved) {
					if (e.type === EdgeType.IMPORTS) {
						resolvedImportPairs.push([e.sourceNodeUid, e.targetNodeUid]);
					}
				}
			}
			resolvedTotal += batchResult.resolved.length;

			if (batchResult.stillUnresolved.length > 0) {
				const classifiedRows: PersistedUnresolvedEdge[] = [];
				for (const { edge, category, sourceFileUid } of batchResult.stillUnresolved) {
					const fileSignals = this.buildFileSignalsForClassification(
						snapshot.snapshotUid,
						sourceFileUid,
						batchSignals,
					);
					const verdict = classifyUnresolvedEdge(edge, category, snapshotSignals, fileSignals);
					let { classification } = verdict;
					let { basisCode } = verdict;
					const fwOverride = detectFrameworkBoundary(edge.targetKey, category, fileSignals.importBindings);
					if (fwOverride) {
						classification = fwOverride.classification;
						basisCode = fwOverride.basisCode;
					}
					classifiedRows.push({
						...edge, category, classification,
						classifierVersion: CURRENT_CLASSIFIER_VERSION,
						basisCode, observedAt: classificationObservedAt,
					});
					unresolvedBreakdown[category] = (unresolvedBreakdown[category] ?? 0) + 1;
				}
				this.storage.insertUnresolvedEdges(classifiedRows);
				unresolvedCount += batchResult.stillUnresolved.length;
			}

			emit({
				phase: "resolving",
				current: resolvedTotal + unresolvedCount,
				total: extractionEdgesTotal,
			});
		}

		emit({ phase: "resolving", current: extractionEdgesTotal, total: extractionEdgesTotal });

		const moduleEdges = this.createModuleEdgesFromIndex(
			resolverIndex, resolvedImportPairs, snapshot.snapshotUid, repoUid,
		);
		if (moduleEdges.length > 0) {
			this.storage.insertEdges(moduleEdges);
		}

		return { resolvedTotal, unresolvedCount, unresolvedBreakdown, moduleEdgesCount: moduleEdges.length };
	}

	/**
	 * Phase 3d + 4: Module discovery, framework detectors, boundary
	 * extraction. Operates on persisted nodes/edges.
	 */
	private async runPostpasses(
		snapshot: Snapshot,
		repo: Repo,
		filePaths: string[],
		trackedFiles: TrackedFile[],
		repoUid: string,
		fileSignalsCache: Map<string, FileSignals>,
	): Promise<void> {
		// Module discovery.
		if (this.discovery) {
			const discoveredRoots = await this.discovery.discoverDeclaredModules(
				repo.rootPath, repoUid,
			);
			if (discoveredRoots.length > 0) {
				const moduleResult = discoverModules({
					repoUid, snapshotUid: snapshot.snapshotUid, discoveredRoots, trackedFiles,
				});
				if (moduleResult.candidates.length > 0) {
					this.storage.insertModuleCandidates(moduleResult.candidates);
					this.storage.insertModuleCandidateEvidence(moduleResult.evidence);
					this.storage.insertModuleFileOwnership(moduleResult.ownership);
				}
			}
		}

		// Lambda entrypoint detection.
		{
			const entrypointInferences: InferenceRow[] = [];
			const detectedAt = new Date().toISOString();
			for (const [fileUid, signals] of fileSignalsCache) {
				const fileSymbols = this.storage.querySymbolsByFile(snapshot.snapshotUid, fileUid);
				const exportedSymbols = fileSymbols
					.filter((n) => n.visibility != null)
					.map((n) => ({ stableKey: n.stableKey, name: n.name, visibility: n.visibility, subtype: n.subtype }));
				const detected = detectLambdaEntrypoints({ importBindings: signals.importBindings, exportedSymbols });
				for (const ep of detected) {
					entrypointInferences.push({
						inferenceUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
						targetStableKey: ep.targetStableKey, kind: "framework_entrypoint",
						valueJson: JSON.stringify({ convention: ep.convention, reason: ep.reason }),
						confidence: ep.confidence,
						basisJson: JSON.stringify({ convention: ep.convention, classifier_version: CURRENT_CLASSIFIER_VERSION }),
						extractor: INDEXER_VERSION, createdAt: detectedAt,
					});
				}
			}
			if (entrypointInferences.length > 0) {
				this.storage.deleteInferencesByKind(snapshot.snapshotUid, "framework_entrypoint");
				this.storage.insertInferences(entrypointInferences);
			}
		}

		// Spring bean detection.
		{
			const springInferences: InferenceRow[] = [];
			const detectedAt = new Date().toISOString();
			for (const relPath of filePaths) {
				if (!relPath.endsWith(".java")) continue;
				let content: string;
				try { content = await readFile(join(repo.rootPath, relPath), "utf-8"); } catch { continue; }
				if (content.length > MAX_FILE_SIZE_BYTES) continue;
				const fileUid = `${repoUid}:${relPath}`;
				const fileSymbols = this.storage.querySymbolsByFile(snapshot.snapshotUid, fileUid);
				const beans = detectSpringBeans(content, relPath, fileSymbols);
				for (const bean of beans) {
					springInferences.push({
						inferenceUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
						targetStableKey: bean.targetStableKey, kind: "spring_container_managed",
						valueJson: JSON.stringify({ annotation: bean.annotation, convention: bean.convention, reason: bean.reason }),
						confidence: bean.confidence,
						basisJson: JSON.stringify({ convention: bean.convention, classifier_version: CURRENT_CLASSIFIER_VERSION }),
						extractor: INDEXER_VERSION, createdAt: detectedAt,
					});
				}
			}
			if (springInferences.length > 0) {
				this.storage.deleteInferencesByKind(snapshot.snapshotUid, "spring_container_managed");
				this.storage.insertInferences(springInferences);
			}
		}

		// Pytest detection.
		{
			const pytestInferences: InferenceRow[] = [];
			const detectedAt = new Date().toISOString();
			for (const relPath of filePaths) {
				if (!relPath.endsWith(".py")) continue;
				let content: string;
				try { content = await readFile(join(repo.rootPath, relPath), "utf-8"); } catch { continue; }
				if (content.length > MAX_FILE_SIZE_BYTES) continue;
				const fileUid = `${repoUid}:${relPath}`;
				const fileSymbols = this.storage.querySymbolsByFile(snapshot.snapshotUid, fileUid);
				const items = detectPytestItems(content, relPath, fileSymbols);
				for (const item of items) {
					const kind = item.convention.startsWith("pytest_fixture") ? "pytest_fixture" : "pytest_test";
					pytestInferences.push({
						inferenceUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
						targetStableKey: item.targetStableKey, kind,
						valueJson: JSON.stringify({ convention: item.convention, reason: item.reason }),
						confidence: item.confidence,
						basisJson: JSON.stringify({ convention: item.convention, classifier_version: CURRENT_CLASSIFIER_VERSION }),
						extractor: INDEXER_VERSION, createdAt: detectedAt,
					});
				}
			}
			if (pytestInferences.length > 0) {
				this.storage.deleteInferencesByKind(snapshot.snapshotUid, "pytest_test");
				this.storage.deleteInferencesByKind(snapshot.snapshotUid, "pytest_fixture");
				this.storage.insertInferences(pytestInferences);
			}
		}

		// Linux/system detection.
		{
			const systemInferences: InferenceRow[] = [];
			const detectedAt = new Date().toISOString();
			for (const relPath of filePaths) {
				const ext = relPath.slice(relPath.lastIndexOf("."));
				if (![".c", ".h", ".cpp", ".hpp", ".cc", ".cxx", ".hxx"].includes(ext)) continue;
				let content: string;
				try { content = await readFile(join(repo.rootPath, relPath), "utf-8"); } catch { continue; }
				if (content.length > MAX_FILE_SIZE_BYTES) continue;
				const fileUid = `${repoUid}:${relPath}`;
				const fileSymbols = this.storage.querySymbolsByFile(snapshot.snapshotUid, fileUid);
				const entries = detectLinuxSystemPatterns(content, relPath, fileSymbols);
				for (const entry of entries) {
					systemInferences.push({
						inferenceUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
						targetStableKey: entry.targetStableKey, kind: "linux_system_managed",
						valueJson: JSON.stringify({ convention: entry.convention, reason: entry.reason }),
						confidence: entry.confidence,
						basisJson: JSON.stringify({ convention: entry.convention, classifier_version: CURRENT_CLASSIFIER_VERSION }),
						extractor: INDEXER_VERSION, createdAt: detectedAt,
					});
				}
			}
			if (systemInferences.length > 0) {
				this.storage.deleteInferencesByKind(snapshot.snapshotUid, "linux_system_managed");
				this.storage.insertInferences(systemInferences);
			}
		}

		// Boundary fact extraction.
		{
			const providerFacts: PersistedBoundaryProviderFact[] = [];
			const consumerFacts: PersistedBoundaryConsumerFact[] = [];
			const boundaryObservedAt = new Date().toISOString();

			for (const relPath of filePaths) {
				let content: string;
				try { content = await readFile(join(repo.rootPath, relPath), "utf-8"); } catch { continue; }
				if (content.length > MAX_FILE_SIZE_BYTES) continue;
				const fileUid = `${repoUid}:${relPath}`;
				const fileSymbols = this.storage.querySymbolsByFile(snapshot.snapshotUid, fileUid);

				if (relPath.endsWith(".java")) {
					if (!this.springParserReady) {
						await initSpringRouteParser();
						this.springParserReady = true;
					}
					const routes = extractSpringRoutes(content, relPath, repoUid, fileSymbols);
					for (const r of routes) {
						const strategy = getMatchStrategy(r.mechanism);
						const matcherKey = strategy ? strategy.computeMatcherKey(r.address, r.metadata) : r.operation;
						providerFacts.push({ ...r, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "spring-route-extractor:0.1", observedAt: boundaryObservedAt });
					}
				}

				if (relPath.endsWith(".ts") || relPath.endsWith(".tsx") || relPath.endsWith(".js") || relPath.endsWith(".jsx")) {
					if (!this.stringResolver) {
						this.stringResolver = new FileLocalStringResolver();
						await this.stringResolver.initialize();
					}
					const bindings = this.stringResolver.resolve(content, relPath);
					const cliCommands = extractCommanderCommands(content, relPath, repoUid, fileSymbols);
					for (const cmd of cliCommands) {
						const strategy = getMatchStrategy(cmd.mechanism);
						const matcherKey = strategy ? strategy.computeMatcherKey(cmd.address, cmd.metadata) : cmd.operation;
						providerFacts.push({ ...cmd, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "commander-command-extractor:0.1", observedAt: boundaryObservedAt });
					}
					const routes = extractExpressRoutes(content, relPath, repoUid, fileSymbols, bindings);
					for (const r of routes) {
						const strategy = getMatchStrategy(r.mechanism);
						const matcherKey = strategy ? strategy.computeMatcherKey(r.address, r.metadata) : r.operation;
						providerFacts.push({ ...r, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "express-route-extractor:0.1", observedAt: boundaryObservedAt });
					}
					const requests = extractHttpClientRequests(content, relPath, repoUid, fileSymbols, bindings);
					for (const c of requests) {
						const strategy = getMatchStrategy(c.mechanism);
						const matcherKey = strategy ? strategy.computeMatcherKey(c.address, c.metadata) : c.operation;
						consumerFacts.push({ ...c, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "http-client-extractor:0.1", observedAt: boundaryObservedAt });
					}
				}
			}

			// package.json script consumers.
			{
				const packageJsonPaths = await this.findPackageJsonFiles(repo.rootPath);
				for (const pkgRelPath of packageJsonPaths) {
					try {
						const pkgContent = await readFile(join(repo.rootPath, pkgRelPath), "utf-8");
						const pkg = JSON.parse(pkgContent);
						if (pkg.scripts && typeof pkg.scripts === "object") {
							const scriptFacts = extractPackageScriptConsumers(pkg.scripts, pkgRelPath, repoUid);
							for (const c of scriptFacts) {
								const strategy = getMatchStrategy(c.mechanism);
								const matcherKey = strategy ? strategy.computeMatcherKey(c.address, c.metadata) : c.operation;
								consumerFacts.push({ ...c, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "package-script-cli-extractor:0.1", observedAt: boundaryObservedAt });
							}
						}
					} catch { /* skip */ }
				}
			}

			// Shell script consumers.
			{
				const shellPaths = await this.findShellScripts(repo.rootPath);
				for (const shellRelPath of shellPaths) {
					try {
						const shellContent = await readFile(join(repo.rootPath, shellRelPath), "utf-8");
						const shellFacts = extractShellScriptConsumers(shellContent, shellRelPath, repoUid);
						for (const c of shellFacts) {
							const strategy = getMatchStrategy(c.mechanism);
							const matcherKey = strategy ? strategy.computeMatcherKey(c.address, c.metadata) : c.operation;
							consumerFacts.push({ ...c, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "shell-script-cli-extractor:0.1", observedAt: boundaryObservedAt });
						}
					} catch { /* skip */ }
				}
			}

			// Makefile consumers.
			{
				const makefilePaths = await this.findMakefiles(repo.rootPath);
				for (const mkRelPath of makefilePaths) {
					try {
						const mkContent = await readFile(join(repo.rootPath, mkRelPath), "utf-8");
						const mkFacts = extractMakefileConsumers(mkContent, mkRelPath, repoUid);
						for (const c of mkFacts) {
							const strategy = getMatchStrategy(c.mechanism);
							const matcherKey = strategy ? strategy.computeMatcherKey(c.address, c.metadata) : c.operation;
							consumerFacts.push({ ...c, factUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid, matcherKey, extractor: "makefile-cli-extractor:0.1", observedAt: boundaryObservedAt });
						}
					} catch { /* skip */ }
				}
			}

			if (providerFacts.length > 0) this.storage.insertBoundaryProviderFacts(providerFacts);
			if (consumerFacts.length > 0) this.storage.insertBoundaryConsumerFacts(consumerFacts);

			// Materialize boundary links.
			if (providerFacts.length > 0 && consumerFacts.length > 0) {
				const candidates = matchBoundaryFacts(providerFacts, consumerFacts);
				if (candidates.length > 0) {
					const materializedAt = new Date().toISOString();
					const links: PersistedBoundaryLink[] = candidates.map((c) => ({
						linkUid: uuidv4(),
						snapshotUid: snapshot.snapshotUid,
						repoUid,
						providerFactUid: c.providerFactUid,
						consumerFactUid: c.consumerFactUid,
						matchBasis: c.matchBasis,
						confidence: c.confidence,
						metadataJson: null,
						materializedAt,
					}));
					this.storage.insertBoundaryLinks(links);
				}
			}
		}
	}

	/**
	 * Phase 5: Finalize snapshot — diagnostics, counts, status,
	 * annotations, orphan check. Returns the IndexResult.
	 */
	private async finalizeSnapshot(
		snapshot: Snapshot,
		repo: Repo,
		repoUid: string,
		trackedFiles: TrackedFile[],
		nodesTotal: number,
		resolvedTotal: number,
		moduleEdgesCount: number,
		unresolvedCount: number,
		unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>>,
		allMetrics: Map<string, { cc: number; params: number; nesting: number }>,
		skippedOversized: number,
		filesReadFailed: number,
		startTime: number,
		emit: (event: IndexProgressEvent) => void,
	): Promise<IndexResult> {
		// Persist metrics.
		if (allMetrics.size > 0) {
			const now = new Date().toISOString();
			const measurements: Measurement[] = [];
			for (const [stableKey, m] of allMetrics) {
				measurements.push({
					measurementUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
					targetStableKey: stableKey, kind: "cyclomatic_complexity",
					valueJson: JSON.stringify({ value: m.cc }), source: INDEXER_VERSION, createdAt: now,
				});
				measurements.push({
					measurementUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
					targetStableKey: stableKey, kind: "parameter_count",
					valueJson: JSON.stringify({ value: m.params }), source: INDEXER_VERSION, createdAt: now,
				});
				measurements.push({
					measurementUid: uuidv4(), snapshotUid: snapshot.snapshotUid, repoUid,
					targetStableKey: stableKey, kind: "max_nesting_depth",
					valueJson: JSON.stringify({ value: m.nesting }), source: INDEXER_VERSION, createdAt: now,
				});
			}
			this.storage.insertMeasurements(measurements);
		}

		// Extract domain versions.
		await this.extractManifestVersions(repo.rootPath, repoUid, snapshot.snapshotUid);

		// Annotations.
		const resolverIndex = this.buildResolverIndex(snapshot.snapshotUid, trackedFiles, repoUid, null);
		const annotationCollisionsDropped = this.annotations
			? await this.extractAndPersistAnnotations(repo.rootPath, repoUid, snapshot.snapshotUid, resolverIndex)
			: 0;

		// Finalize snapshot.
		this.storage.updateSnapshotCounts(snapshot.snapshotUid);
		this.storage.updateSnapshotStatus({ snapshotUid: snapshot.snapshotUid, status: SnapshotStatus.READY });

		emit({ phase: "persisting", current: 1, total: 1 });

		const orphanedDeclarations = this.countOrphanedDeclarations(repoUid, snapshot.snapshotUid);

		const extractionDiagnostics = {
			diagnostics_version: 1,
			edges_total: resolvedTotal + moduleEdgesCount,
			unresolved_total: unresolvedCount,
			unresolved_breakdown: unresolvedBreakdown,
			annotation_collisions_dropped: annotationCollisionsDropped,
			files_skipped_oversized: skippedOversized,
			files_read_failed: filesReadFailed,
		};
		this.storage.updateSnapshotExtractionDiagnostics(snapshot.snapshotUid, JSON.stringify(extractionDiagnostics));

		return {
			snapshotUid: snapshot.snapshotUid,
			filesTotal: trackedFiles.length,
			nodesTotal,
			edgesTotal: resolvedTotal + moduleEdgesCount,
			edgesUnresolved: unresolvedCount,
			unresolvedBreakdown,
			durationMs: Date.now() - startTime,
			orphanedDeclarations,
		};
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

	/**
	 * Find all Makefile and *.mk files in the repo.
	 * Returns repo-relative paths.
	 */
	private async findMakefiles(rootPath: string): Promise<string[]> {
		const results: string[] = [];
		const ig = await loadGitignore(rootPath);

		const walk = async (dir: string) => {
			let entries: import("node:fs").Dirent[];
			try {
				entries = await readdir(dir, { withFileTypes: true });
			} catch {
				return;
			}
			for (const entry of entries) {
				if (entry.isDirectory()) {
					if (ALWAYS_EXCLUDED.has(entry.name)) continue;
					const relDir = relative(rootPath, join(dir, entry.name));
					if (ig.ignores(relDir + "/")) continue;
					await walk(join(dir, entry.name));
				} else if (
					entry.name === "Makefile" ||
					entry.name === "GNUmakefile" ||
					entry.name === "makefile" ||
					entry.name.endsWith(".mk")
				) {
					const relPath = relative(rootPath, join(dir, entry.name));
					results.push(relPath);
				}
			}
		};

		await walk(rootPath);
		return results;
	}

	/**
	 * Find all shell script files (.sh, .bash) in the repo.
	 * Returns repo-relative paths.
	 */
	private async findShellScripts(rootPath: string): Promise<string[]> {
		const results: string[] = [];
		const ig = await loadGitignore(rootPath);

		const walk = async (dir: string) => {
			let entries: import("node:fs").Dirent[];
			try {
				entries = await readdir(dir, { withFileTypes: true });
			} catch {
				return;
			}
			for (const entry of entries) {
				if (entry.isDirectory()) {
					if (ALWAYS_EXCLUDED.has(entry.name)) continue;
					const relDir = relative(rootPath, join(dir, entry.name));
					if (ig.ignores(relDir + "/")) continue;
					await walk(join(dir, entry.name));
				} else if (entry.name.endsWith(".sh") || entry.name.endsWith(".bash")) {
					const relPath = relative(rootPath, join(dir, entry.name));
					results.push(relPath);
				}
			}
		};

		await walk(rootPath);
		return results;
	}

	/**
	 * Find all package.json files in the repo, excluding node_modules.
	 * Returns repo-relative paths.
	 */
	private async findPackageJsonFiles(rootPath: string): Promise<string[]> {
		const results: string[] = [];
		const ig = await loadGitignore(rootPath);

		const walk = async (dir: string) => {
			let entries: import("node:fs").Dirent[];
			try {
				entries = await readdir(dir, { withFileTypes: true });
			} catch {
				return;
			}
			for (const entry of entries) {
				if (entry.isDirectory()) {
					if (ALWAYS_EXCLUDED.has(entry.name)) continue;
					const relDir = relative(rootPath, join(dir, entry.name));
					if (ig.ignores(relDir + "/")) continue;
					await walk(join(dir, entry.name));
				} else if (entry.name === "package.json") {
					const relPath = relative(rootPath, join(dir, entry.name));
					results.push(relPath);
				}
			}
		};

		await walk(rootPath);
		return results;
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
				if (!ALL_SOURCE_EXTENSIONS.has(ext)) continue;
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

	// ── Resolver index ────────────────────────────────────────────────

	/**
	 * Pre-built resolver index for edge resolution. All Maps are built
	 * from row-at-a-time DB iteration — no bulk .all() materialization.
	 */
	private buildResolverIndex(
		snapshotUid: string,
		trackedFiles: TrackedFile[],
		repoUid: string,
		compileDb?: CompilationDatabase | null,
	): ResolverIndex {
		// Build lookup maps from row-at-a-time iterator.
		// Peak memory = Maps only (no intermediate array).
		const nodesByStableKey = new Map<string, ResolverNode>();
		const nodesByName = new Map<string, ResolverNode[]>();
		const nodeUidToFileUid = new Map<string, string>();
		// For module-edge creation: stableKey→nodeUid and file→module Maps.
		const stableKeyToUid = new Map<string, string>();
		const fileToModule = new Map<string, string>();

		for (const node of this.storage.queryResolverNodesIter(snapshotUid)) {
			nodesByStableKey.set(node.stableKey, node);
			stableKeyToUid.set(node.stableKey, node.nodeUid);

			const existing = nodesByName.get(node.name) ?? [];
			existing.push(node);
			nodesByName.set(node.name, existing);
			if (node.qualifiedName && node.qualifiedName !== node.name) {
				const existingQ = nodesByName.get(node.qualifiedName) ?? [];
				existingQ.push(node);
				nodesByName.set(node.qualifiedName, existingQ);
			}

			if (node.fileUid) {
				nodeUidToFileUid.set(node.nodeUid, node.fileUid);
			}

			// FILE nodes: build file→module mapping for module-edge creation.
			if (node.kind === NodeKind.FILE && node.qualifiedName) {
				const modPath = getModulePath(node.qualifiedName);
				if (modPath) {
					const moduleKey = `${repoUid}:${modPath}:MODULE`;
					fileToModule.set(node.nodeUid, moduleKey);
				}
			}
		}

		// Build file resolution map: extensionless path → file stable key
		const fileResolution = new Map<string, string>();
		for (const file of trackedFiles) {
			const stableKey = `${repoUid}:${file.path}:FILE`;
			fileResolution.set(`${repoUid}:${file.path}:FILE`, stableKey);
			const withoutExt = stripExtension(file.path);
			const extlessKey = `${repoUid}:${withoutExt}:FILE`;
			if (!fileResolution.has(extlessKey)) {
				fileResolution.set(extlessKey, stableKey);
			}
			if (file.path.endsWith("/index.ts") || file.path.endsWith("/index.tsx")) {
				const dirPath = file.path.replace(/\/index\.tsx?$/, "");
				const dirKey = `${repoUid}:${dirPath}:FILE`;
				if (!fileResolution.has(dirKey)) {
					fileResolution.set(dirKey, stableKey);
				}
			}
		}

		// C/C++ include path resolution from compile_commands.json.
		const perFileIncludeResolution = new Map<string, Map<string, string>>();
		if (compileDb) {
			const headerStableKeys = new Map<string, string>();
			for (const file of trackedFiles) {
				if (file.path.endsWith(".h") || file.path.endsWith(".hpp") ||
					file.path.endsWith(".hxx")) {
					headerStableKeys.set(file.path, `${repoUid}:${file.path}:FILE`);
				}
			}
			for (const [sourceRelPath, entry] of compileDb.entries) {
				const sourceFileUid = `${repoUid}:${sourceRelPath}`;
				const resolution = new Map<string, string>();
				for (const incPath of entry.includePaths) {
					const prefix = incPath === "" ? "" : incPath + "/";
					for (const [headerPath, headerKey] of headerStableKeys) {
						if (prefix === "" || headerPath.startsWith(prefix)) {
							const bareName = prefix === ""
								? headerPath
								: headerPath.slice(prefix.length);
							if (!resolution.has(bareName)) {
								resolution.set(bareName, headerKey);
							}
						}
					}
				}
				if (resolution.size > 0) {
					perFileIncludeResolution.set(sourceFileUid, resolution);
				}
			}
		}

		return {
			nodesByStableKey,
			nodesByName,
			nodeUidToFileUid,
			fileResolution,
			perFileIncludeResolution,
			stableKeyToUid,
			fileToModule,
		};
	}

	/**
	 * Build full FileSignals for classification from batch-loaded
	 * staged signals (import bindings, package deps, tsconfig aliases)
	 * and rebuilt same-file symbol sets from persisted nodes.
	 *
	 * No dependence on fileSignalsCache. All data comes from DB.
	 */
	private buildFileSignalsForClassification(
		snapshotUid: string,
		sourceFileUid: string | undefined,
		batchSignals: Map<string, FileSignalRow>,
	): FileSignals {
		if (!sourceFileUid) return emptyFileSignals();

		const row = batchSignals.get(sourceFileUid);

		// Parse staged signals.
		const importBindings: ImportBinding[] = row?.importBindingsJson
			? JSON.parse(row.importBindingsJson)
			: [];
		const packageDependencies: PackageDependencySet = row?.packageDependenciesJson
			? JSON.parse(row.packageDependenciesJson)
			: { names: [] };
		const tsconfigAliases: TsconfigAliases = row?.tsconfigAliasesJson
			? JSON.parse(row.tsconfigAliasesJson)
			: { entries: [] };

		// Rebuild same-file symbol sets from persisted nodes.
		const sameFileValueSymbols = new Set<string>();
		const sameFileClassSymbols = new Set<string>();
		const sameFileInterfaceSymbols = new Set<string>();
		const fileSymbols = this.storage.querySymbolsByFile(snapshotUid, sourceFileUid);
		for (const sym of fileSymbols) {
			const sub = sym.subtype;
			if (
				sub === NodeSubtype.FUNCTION ||
				sub === NodeSubtype.CLASS ||
				sub === NodeSubtype.METHOD ||
				sub === NodeSubtype.VARIABLE ||
				sub === NodeSubtype.CONSTANT ||
				sub === NodeSubtype.ENUM ||
				sub === NodeSubtype.ENUM_MEMBER ||
				sub === NodeSubtype.CONSTRUCTOR ||
				sub === NodeSubtype.GETTER ||
				sub === NodeSubtype.SETTER ||
				sub === NodeSubtype.PROPERTY
			) {
				sameFileValueSymbols.add(sym.name);
			}
			if (sub === NodeSubtype.CLASS) {
				sameFileClassSymbols.add(sym.name);
			}
			if (sub === NodeSubtype.INTERFACE) {
				sameFileInterfaceSymbols.add(sym.name);
			}
		}

		return {
			importBindings,
			sameFileValueSymbols,
			sameFileClassSymbols,
			sameFileInterfaceSymbols,
			packageDependencies,
			tsconfigAliases,
		};
	}

	// ── Module-level edge creation ─────────────────────────────────────

	/**
	 * Create module-level edges from the pre-built resolver index and
	 * accumulated resolved IMPORTS pairs. No queryAllNodes needed.
	 *
	 * @param resolvedImportPairs - [sourceNodeUid, targetNodeUid] pairs
	 *   accumulated during batch resolution. Only IMPORTS edges.
	 */
	private createModuleEdgesFromIndex(
		index: ResolverIndex,
		resolvedImportPairs: Array<[string, string]>,
		snapshotUid: string,
		repoUid: string,
	): GraphEdge[] {
		const edges: GraphEdge[] = [];

		// 1. OWNS edges: MODULE -> FILE
		for (const [fileNodeUid, moduleKey] of index.fileToModule.entries()) {
			const moduleUid = index.stableKeyToUid.get(moduleKey);
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

		// 2. MODULE->MODULE IMPORTS: derived from file-level IMPORTS edges.
		const moduleImportPairs = new Set<string>();
		for (const [srcUid, tgtUid] of resolvedImportPairs) {
			const sourceModuleKey = index.fileToModule.get(srcUid);
			const targetModuleKey = index.fileToModule.get(tgtUid);
			if (
				sourceModuleKey &&
				targetModuleKey &&
				sourceModuleKey !== targetModuleKey
			) {
				const pairKey = `${sourceModuleKey}|${targetModuleKey}`;
				if (moduleImportPairs.has(pairKey)) continue;
				moduleImportPairs.add(pairKey);

				const sourceModuleUid = index.stableKeyToUid.get(sourceModuleKey);
				const targetModuleUid = index.stableKeyToUid.get(targetModuleKey);
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

	/**
	 * Resolve a batch of unresolved edges against the pre-built resolver
	 * index. Returns resolved GraphEdges and categorized still-unresolved
	 * edges with their source file UIDs for classification.
	 */
	private resolveEdges(
		unresolved: UnresolvedEdge[],
		index: ResolverIndex,
		repoUid: string,
		snapshotUid: string,
		importBindingsByFile?: Map<string, ImportBinding[]>,
	): {
		resolved: GraphEdge[];
		stillUnresolved: Array<{
			edge: UnresolvedEdge;
			category: UnresolvedEdgeCategory;
			sourceFileUid: string | undefined;
		}>;
		unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>>;
	} {
		const resolved: GraphEdge[] = [];
		const stillUnresolved: Array<{
			edge: UnresolvedEdge;
			category: UnresolvedEdgeCategory;
			sourceFileUid: string | undefined;
		}> = [];
		const unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>> = {};

		for (const edge of unresolved) {
			const targetNodeUid = this.resolveTarget(
				edge,
				index.nodesByStableKey,
				index.nodesByName,
				index.fileResolution,
				importBindingsByFile,
				index.nodeUidToFileUid,
				index.perFileIncludeResolution,
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
				const category = categorizeUnresolvedEdge(edge);
				const sourceFileUid = index.nodeUidToFileUid.get(edge.sourceNodeUid);
				stillUnresolved.push({ edge, category, sourceFileUid });
				unresolvedBreakdown[category] =
					(unresolvedBreakdown[category] ?? 0) + 1;
			}
		}

		return { resolved, stillUnresolved, unresolvedBreakdown };
	}

	private resolveTarget(
		edge: UnresolvedEdge,
		nodesByStableKey: Map<string, ResolverNode>,
		nodesByName: Map<string, ResolverNode[]>,
		fileResolution: Map<string, string>,
		importBindingsByFile: Map<string, ImportBinding[]> | undefined,
		nodeUidToFileUid: Map<string, string>,
		perFileIncludeResolution: Map<string, Map<string, string>>,
	): string | null {
		switch (edge.type) {
			case EdgeType.IMPORTS: {
				// For C/C++ includes, use the source file's per-TU include
				// paths from compile_commands.json.
				const sourceFileUid = nodeUidToFileUid.get(edge.sourceNodeUid);
				const tuIncludes = sourceFileUid
					? perFileIncludeResolution.get(sourceFileUid)
					: undefined;
				return this.resolveImportTarget(
					edge.targetKey,
					nodesByStableKey,
					fileResolution,
					edge.repoUid,
					tuIncludes,
				);
			}
			case EdgeType.CALLS:
				return this.resolveCallTarget(
					edge.targetKey,
					edge.sourceNodeUid,
					nodesByStableKey,
					nodesByName,
					fileResolution,
					importBindingsByFile,
					nodeUidToFileUid,
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
		nodesByStableKey: Map<string, ResolverNode>,
		fileResolution: Map<string, string>,
		repoUid?: string,
		tuIncludeResolution?: Map<string, string>,
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

		// C/C++ #include: targetKey is a bare header name (e.g., "util.h").
		// First try per-TU include resolution from compile_commands.json.
		// This respects the source file's actual -I flags, not a global set.
		if (tuIncludeResolution) {
			const resolvedHeader = tuIncludeResolution.get(targetKey);
			if (resolvedHeader) {
				const node = nodesByStableKey.get(resolvedHeader);
				if (node) return node.nodeUid;
			}
		}

		// Fallback: try constructing a stable key with repoUid prefix.
		// This handles headers at the repo root without compile_commands.json.
		if (repoUid && !targetKey.includes(":")) {
			const constructedKey = `${repoUid}:${targetKey}:FILE`;
			const constructed = fileResolution.get(constructedKey);
			if (constructed) {
				const node = nodesByStableKey.get(constructed);
				if (node) return node.nodeUid;
			}
			const directConstructed = nodesByStableKey.get(constructedKey);
			if (directConstructed) return directConstructed.nodeUid;
		}

		return null;
	}

	private resolveCallTarget(
		targetKey: string,
		sourceNodeUid: string,
		nodesByStableKey: Map<string, ResolverNode>,
		nodesByName: Map<string, ResolverNode[]>,
		fileResolution: Map<string, string>,
		importBindingsByFile: Map<string, ImportBinding[]> | undefined,
		nodeUidToFileUid: Map<string, string>,
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
			}

			// For "obj.method()" where obj is not "this", try method name
			const resolved = this.pickUnambiguous(
				nodesByName.get(methodName),
				EdgeType.CALLS,
			);
			if (resolved) return resolved;
		}

		// Simple function call: "classifyMedia" → look for a function with that name.
		const globalResult = this.pickUnambiguous(
			nodesByName.get(targetKey),
			EdgeType.CALLS,
		);
		if (globalResult) return globalResult;

		// Import-binding-assisted resolution.
		// If the bare identifier was imported in this file, use the import
		// binding to narrow the search to the specific source module.
		// This resolves calls like:
		//   import { classifyMedia } from "./media";
		//   classifyMedia(asset)
		// without requiring a type checker.
		if (importBindingsByFile && nodeUidToFileUid.size > 0) {
			const sourceFileUid = nodeUidToFileUid.get(sourceNodeUid);
			if (sourceFileUid) {
				const bindings = importBindingsByFile.get(sourceFileUid);
				if (bindings) {
					const binding = bindings.find(
						(b) => b.identifier === targetKey,
					);
					if (binding) {
						// Resolve the binding's specifier to a file.
						const resolvedFileKey = this.resolveImportSpecifierToFile(
							binding.specifier,
							sourceFileUid,
							nodesByStableKey,
							fileResolution,
						);
						if (resolvedFileKey) {
							// Find the symbol in that specific file.
							const candidates = nodesByName.get(targetKey);
							if (candidates) {
								const inFile = candidates.filter(
									(n) => n.fileUid === resolvedFileKey,
								);
								const result = this.pickUnambiguous(inFile, EdgeType.CALLS);
								if (result) return result;
							}
						}
					}
				}
			}
		}

		return null;
	}

	/**
	 * Resolve an import specifier to a file UID.
	 * Handles relative paths: "./media" → "repo:src/media.ts" file UID.
	 */
	/**
	 * Resolve an import specifier to a file UID.
	 * Handles relative paths: "./media" → "repo:src/media.ts" file UID.
	 * Returns the fileUid string (format: "repoUid:path"), not a node UID.
	 */
	private resolveImportSpecifierToFile(
		specifier: string,
		sourceFileUid: string,
		_nodesByStableKey: Map<string, ResolverNode>,
		fileResolution: Map<string, string>,
	): string | null {
		if (!specifier.startsWith(".")) return null;

		// Extract the source file's directory from its file UID.
		// fileUid format: "repoUid:path/to/file.ts"
		const colonIdx = sourceFileUid.indexOf(":");
		if (colonIdx < 0) return null;
		const repoUid = sourceFileUid.slice(0, colonIdx);
		const sourcePath = sourceFileUid.slice(colonIdx + 1);
		const sourceDir = sourcePath.includes("/")
			? sourcePath.slice(0, sourcePath.lastIndexOf("/"))
			: "";

		// Resolve the relative specifier against the source directory.
		const resolvedPath = resolveRelativePath(sourceDir, specifier);
		const targetFileKey = `${repoUid}:${resolvedPath}:FILE`;

		// Try the file resolution map (handles extensionless → with extension).
		const resolvedStableKey = fileResolution.get(targetFileKey);
		if (resolvedStableKey) {
			// Extract the fileUid from the stable key: "repoUid:path.ts:FILE" → "repoUid:path.ts"
			const fileUid = resolvedStableKey.replace(/:FILE$/, "");
			return fileUid;
		}

		return null;
	}

	private resolveNamedTarget(
		targetKey: string,
		nodesByName: Map<string, ResolverNode[]>,
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
	/**
	 * Read snapshot-level classifier signals from the repo root:
	 *   - tsconfig.json → path alias entries
	 *   - package.json  → dependency name set
	 *
	 * Both reads are best-effort. If either file is absent, unreadable,
	 * or unparseable, the corresponding signal degrades to an empty
	 * set. Indexing MUST NOT fail because classifier signals are
	 * unavailable — classification is additive, not required for
	 * graph correctness.
	 */
	private buildSnapshotSignals(): SnapshotSignals {
		// Merge runtime builtins from ALL registered extractors so that
		// both TS and Rust builtins are recognized in mixed-language repos.
		const allIdentifiers: string[] = [];
		const allModuleSpecifiers: string[] = [];
		const seen = new Set<ExtractorPort>();
		for (const ext of this.extractorsByExtension.values()) {
			if (seen.has(ext)) continue; // same extractor serves multiple extensions
			seen.add(ext);
			allIdentifiers.push(...ext.runtimeBuiltins.identifiers);
			allModuleSpecifiers.push(...ext.runtimeBuiltins.moduleSpecifiers);
		}
		return {
			runtimeBuiltins: {
				identifiers: Object.freeze(allIdentifiers),
				moduleSpecifiers: Object.freeze(allModuleSpecifiers),
			},
		};
	}

	/**
	 * Resolve the nearest package.json ancestor for a file and return
	 * its declared dependency set. Walks upward from the file's
	 * directory to the repo root. First hit wins.
	 *
	 * Cache semantics: every directory on the walk path is cached so
	 * subsequent files in the same subtree resolve in O(1). A cache
	 * miss triggers at most one directory walk + one file read.
	 *
	 * If no package.json is found between fileDir and repoRoot
	 * (inclusive), returns an empty PackageDependencySet.
	 */
	private async resolveNearestPackageDeps(
		fileRelPath: string,
		repoRootPath: string,
		cache: Map<string, PackageDependencySet>,
	): Promise<PackageDependencySet> {
		const emptyDeps: PackageDependencySet = { names: Object.freeze([]) };
		// Language-aware manifest selection:
		// .rs → Cargo.toml, .java → build.gradle, .py → pyproject.toml/requirements.txt,
		// .ts/.js → package.json.
		const isRustFile = fileRelPath.endsWith(".rs");
		const isJavaFile = fileRelPath.endsWith(".java");
		const isPythonFile = fileRelPath.endsWith(".py");
		// Separate cache keys per language to prevent cross-contamination
		// when multiple manifest types exist at the same directory level.
		const cachePrefix = isRustFile ? "rs:" : isJavaFile ? "java:" : isPythonFile ? "py:" : "js:";
		// Work with the file's directory (repo-relative, forward slashes).
		let dir = fileRelPath.includes("/")
			? fileRelPath.slice(0, fileRelPath.lastIndexOf("/"))
			: "";

		// Collect uncached dirs to backfill after resolution.
		const uncachedDirs: string[] = [];

		while (true) {
			const cacheKey = `${cachePrefix}${dir}`;
			const cached = cache.get(cacheKey);
			if (cached !== undefined) {
				for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, cached);
				return cached;
			}
			uncachedDirs.push(dir);

			// Try reading the language-appropriate package manifest.
			// Rust files use Cargo.toml; TS/JS files use package.json.
			// This prevents mixed-manifest repos from classifying Rust
			// files against Node deps or vice versa.
			const absDir = dir === "" ? repoRootPath : join(repoRootPath, dir);

			if (isRustFile) {
				const cargoDeps = await readCargoDependencies(absDir);
				if (cargoDeps !== null && cargoDeps.names.length > 0) {
					for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, cargoDeps);
					return cargoDeps;
				}
			} else if (isJavaFile) {
				// build.gradle / build.gradle.kts for Java projects.
				const gradleDeps = await readGradleDependencies(absDir);
				if (gradleDeps !== null && gradleDeps.names.length > 0) {
					for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, gradleDeps);
					return gradleDeps;
				}
			} else if (isPythonFile) {
				// pyproject.toml / requirements.txt for Python projects.
				// A present manifest with zero deps is a valid result —
				// stop walking. This prevents inheriting unrelated parent
				// dependencies in monorepos where a leaf package
				// intentionally has no third-party deps.
				const pythonDeps = await readPythonDependencies(absDir);
				if (pythonDeps !== null) {
					const resolved = pythonDeps.names.length > 0 ? pythonDeps : emptyDeps;
					for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, resolved);
					return resolved;
				}
			} else {
				const pkgPath = join(absDir, "package.json");
				try {
					const content = await readFile(pkgPath, "utf-8");
					const deps = extractPackageDependencies(content);
					const resolved = deps ?? emptyDeps;
					for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, resolved);
					return resolved;
				} catch {
					// No package.json here — walk up.
				}
			}

			// Stop if we've reached repo root (dir === "").
			if (dir === "") break;
			// Move to parent directory.
			const slash = dir.lastIndexOf("/");
			dir = slash >= 0 ? dir.slice(0, slash) : "";
		}

		// No manifest found anywhere up to repo root.
		for (const d of uncachedDirs) cache.set(`${cachePrefix}${d}`, emptyDeps);
		return emptyDeps;
	}

	/**
	 * Resolve the nearest tsconfig.json ancestor for a file and return
	 * its effective path aliases (following `extends` chains).
	 *
	 * Same walk + cache pattern as resolveNearestPackageDeps: every
	 * directory on the upward path is cached so sibling files resolve
	 * in O(1).
	 */
	private async resolveNearestTsconfigAliases(
		fileRelPath: string,
		repoRootPath: string,
		cache: Map<string, TsconfigAliases>,
	): Promise<TsconfigAliases> {
		const emptyAliases: TsconfigAliases = { entries: Object.freeze([]) };
		let dir = fileRelPath.includes("/")
			? fileRelPath.slice(0, fileRelPath.lastIndexOf("/"))
			: "";

		const uncachedDirs: string[] = [];

		while (true) {
			const cached = cache.get(dir);
			if (cached !== undefined) {
				for (const d of uncachedDirs) cache.set(d, cached);
				return cached;
			}
			uncachedDirs.push(dir);

			const tsconfigDir = dir === ""
				? repoRootPath
				: join(repoRootPath, dir);
			const aliases = await readTsconfigAliases(tsconfigDir);
			if (aliases !== null) {
				for (const d of uncachedDirs) cache.set(d, aliases);
				return aliases;
			}

			if (dir === "") break;
			const slash = dir.lastIndexOf("/");
			dir = slash >= 0 ? dir.slice(0, slash) : "";
		}

		for (const d of uncachedDirs) cache.set(d, emptyAliases);
		return emptyAliases;
	}

	/**
	 * Extract domain versions from manifest files (package.json).
	 * Stores package_name and package_version as measurements.
	 */
	private async extractManifestVersions(
		rootPath: string,
		repoUid: string,
		snapshotUid: string,
	): Promise<void> {
		const manifestPath = join(rootPath, "package.json");
		if (!existsSync(manifestPath)) return;

		const content = await readFile(manifestPath, "utf-8");
		const manifest = extractPackageManifest(content, "package.json");
		if (!manifest) return;

		const now = new Date().toISOString();
		const repoStableKey = `${repoUid}:.:MODULE`;
		const measurements: Measurement[] = [];

		if (manifest.packageName) {
			measurements.push({
				measurementUid: uuidv4(),
				snapshotUid,
				repoUid,
				targetStableKey: repoStableKey,
				kind: "package_name",
				valueJson: JSON.stringify({
					value: manifest.packageName,
					source_file: manifest.sourcePath,
				}),
				source: MANIFEST_EXTRACTOR_VERSION,
				createdAt: now,
			});
		}

		if (manifest.packageVersion) {
			measurements.push({
				measurementUid: uuidv4(),
				snapshotUid,
				repoUid,
				targetStableKey: repoStableKey,
				kind: "package_version",
				valueJson: JSON.stringify({
					value: manifest.packageVersion,
					source_file: manifest.sourcePath,
				}),
				source: MANIFEST_EXTRACTOR_VERSION,
				createdAt: now,
			});
		}

		if (measurements.length > 0) {
			this.storage.insertMeasurements(measurements);
		}
	}

	/**
	 * Extract provisional annotations (README + package.json
	 * description) and persist them via AnnotationsPort. Returns
	 * the count of annotations dropped due to same-kind same-target
	 * collisions.
	 *
	 * Walks the repository filesystem (gitignore-respecting) looking
	 * for package.json files and README.md / README.txt files. Uses
	 * the attribution helpers from core/annotations/ to map each
	 * candidate to a target stable_key (repo or MODULE). Drops
	 * candidates that cannot be attributed. Applies deterministic
	 * collision resolution before inserting.
	 *
	 * Isolation: writes only. Never reads annotations back. The
	 * AnnotationsPort is injected and the indexer has no read
	 * method exposed on it.
	 */
	private async extractAndPersistAnnotations(
		rootPath: string,
		repoUid: string,
		snapshotUid: string,
		resolverIndex: ResolverIndex,
	): Promise<number> {
		if (!this.annotations) return 0;

		// Build a lookup: directory path → MODULE stable_key.
		// Uses the resolver index's nodesByStableKey to find MODULE nodes
		// without materializing all nodes.
		const moduleByPath = new Map<string, string>();
		for (const [stableKey, node] of resolverIndex.nodesByStableKey) {
			if (node.kind === NodeKind.MODULE && node.qualifiedName) {
				moduleByPath.set(node.qualifiedName, stableKey);
			}
		}
		// Repo-level target: distinct from any MODULE. No graph node is
		// created for it; the stable_key is synthetic. Resolver handles
		// the "." alias that maps queries to this target.
		const repoStableKey = `${repoUid}:REPO`;

		// Scan the filesystem for annotation source files
		const { readmePaths, packageJsonPaths } =
			await this.scanAnnotationSources(rootPath);

		const candidates: Array<Omit<Annotation, "annotation_uid">> = [];
		const now = new Date().toISOString();

		// README extraction
		// Group by directory to apply README preference (.md over .txt)
		const readmesByDir = new Map<string, string[]>();
		for (const rel of readmePaths) {
			const lastSlash = rel.lastIndexOf("/");
			const dir = lastSlash >= 0 ? rel.slice(0, lastSlash) : "";
			const arr = readmesByDir.get(dir) ?? [];
			arr.push(rel);
			readmesByDir.set(dir, arr);
		}
		for (const [dir, paths] of readmesByDir) {
			const filenames = paths.map((p) => p.split("/").pop() ?? p);
			const preferred = preferReadmeFile(filenames);
			if (!preferred) continue;
			const preferredPath = paths.find((p) => p.endsWith(preferred));
			if (!preferredPath) continue;

			let raw: string;
			try {
				raw = await readFile(join(rootPath, preferredPath), "utf-8");
			} catch {
				continue;
			}
			const cand = extractReadme(preferredPath, raw);
			if (!cand) continue;

			// Attribute: repo root or owning module
			const isRepoRoot = dir === "" || dir === ".";
			const owningModuleKey = moduleByPath.get(dir) ?? null;
			const attribution = attributeReadme({
				isRepoRoot,
				repoStableKey,
				owningModuleStableKey: owningModuleKey,
			});
			if (!attribution) continue;

			const normalized = applyContentTruncation(cand.content);
			if (isEmptyContent(normalized)) continue;

			candidates.push({
				snapshot_uid: snapshotUid,
				target_kind: attribution.target_kind,
				target_stable_key: attribution.target_stable_key,
				annotation_kind: AnnotationKind.MODULE_README,
				contract_class: AnnotationContractClass.HINT,
				content: normalized,
				content_hash: sha256Hex(normalized),
				source_file: cand.sourceFile,
				source_line_start: cand.sourceLineStart,
				source_line_end: cand.sourceLineEnd,
				language: cand.language,
				provisional: true,
				extracted_at: now,
			});
		}

		// package.json description extraction
		for (const rel of packageJsonPaths) {
			let raw: string;
			try {
				raw = await readFile(join(rootPath, rel), "utf-8");
			} catch {
				continue;
			}
			const cand = extractPackageDescription(rel, raw);
			if (!cand) continue;

			const lastSlash = rel.lastIndexOf("/");
			const dir = lastSlash >= 0 ? rel.slice(0, lastSlash) : "";
			const isRepoRoot = dir === "" || dir === ".";
			const owningModuleKey = moduleByPath.get(dir) ?? null;
			const attribution = attributePackageDescription({
				isRepoRoot,
				repoStableKey,
				owningModuleStableKey: owningModuleKey,
			});
			if (!attribution) continue;

			const normalized = applyContentTruncation(cand.content);
			if (isEmptyContent(normalized)) continue;

			candidates.push({
				snapshot_uid: snapshotUid,
				target_kind: attribution.target_kind,
				target_stable_key: attribution.target_stable_key,
				annotation_kind: AnnotationKind.PACKAGE_DESCRIPTION,
				contract_class: AnnotationContractClass.HINT,
				content: normalized,
				content_hash: sha256Hex(normalized),
				source_file: cand.sourceFile,
				source_line_start: cand.sourceLineStart,
				source_line_end: cand.sourceLineEnd,
				language: "json",
				provisional: true,
				extracted_at: now,
			});
		}

		// Apply collision resolution
		const { keptIndices, droppedCount } = resolveCollisions(candidates);

		// Assign UUIDs and insert
		const toInsert: Annotation[] = keptIndices.map((i) => ({
			annotation_uid: uuidv4(),
			...candidates[i],
		}));
		if (toInsert.length > 0) {
			this.annotations.insertAnnotations(toInsert);
		}

		return droppedCount;
	}

	/**
	 * Scan the filesystem for annotation source files: README.md,
	 * README.txt, and package.json files. Respects .gitignore and
	 * ALWAYS_EXCLUDED directories.
	 */
	private async scanAnnotationSources(
		rootPath: string,
	): Promise<{
		readmePaths: string[];
		packageJsonPaths: string[];
	}> {
		const readmePaths: string[] = [];
		const packageJsonPaths: string[] = [];
		const ig = await loadGitignore(rootPath);

		const walk = async (currentPath: string): Promise<void> => {
			const entries = await readdir(currentPath, { withFileTypes: true });
			for (const entry of entries) {
				const fullPath = join(currentPath, entry.name);
				const relPath = toPosixPath(relative(rootPath, fullPath));
				if (entry.isDirectory()) {
					if (ALWAYS_EXCLUDED.has(entry.name)) continue;
					if (ig.ignores(`${relPath}/`)) continue;
					if (existsSync(join(fullPath, "pyvenv.cfg"))) continue;
					await walk(fullPath);
				} else if (entry.isFile()) {
					if (ig.ignores(relPath)) continue;
					const lower = entry.name.toLowerCase();
					if (lower === "readme.md" || lower === "readme.txt") {
						readmePaths.push(relPath);
					} else if (lower === "package.json") {
						packageJsonPaths.push(relPath);
					}
				}
			}
		};

		await walk(rootPath);
		return { readmePaths, packageJsonPaths };
	}

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
		candidates: ResolverNode[] | undefined,
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

/**
 * Full-length sha256 hex digest. Used for annotation content_hash
 * per annotations-contract.txt §4 (no truncation).
 */
function sha256Hex(content: string): string {
	return `sha256:${createHash("sha256").update(content).digest("hex")}`;
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
 * Resolve a relative import specifier against a source directory.
 * "./media" from "src/lib" → "src/lib/media"
 * "../utils" from "src/lib" → "src/utils"
 */
function resolveRelativePath(sourceDir: string, specifier: string): string {
	const parts = sourceDir ? sourceDir.split("/") : [];
	const specParts = specifier.split("/");

	for (const seg of specParts) {
		if (seg === ".") continue;
		if (seg === "..") {
			parts.pop();
		} else {
			parts.push(seg);
		}
	}
	return parts.join("/");
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
	candidates: ResolverNode[],
	edgeType: EdgeType,
): ResolverNode[] {
	let filtered: ResolverNode[];

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
 * Categorize an unresolved edge into a machine-stable extraction
 * failure category (UnresolvedEdgeCategory). This maps the edge to
 * "what kind of resolution gap is this?" — distinct from SEMANTIC
 * CLASSIFICATION (classifyUnresolvedEdge in core/classification/).
 *
 * Both functions operate on UnresolvedEdges but produce orthogonal
 * axes:
 *   categorize → UnresolvedEdgeCategory (extraction failure mode)
 *   classify   → UnresolvedEdgeClassification (semantic meaning)
 *
 * Human-readable labels for categories are rendered at display time
 * via humanLabelForCategory(); they are not persisted as JSON keys.
 */
function categorizeUnresolvedEdge(edge: UnresolvedEdge): UnresolvedEdgeCategory {
	const type = edge.type;

	if (type === EdgeType.IMPORTS) {
		return UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND;
	}

	if (type === EdgeType.INSTANTIATES) {
		return UnresolvedEdgeCategory.INSTANTIATES_CLASS_NOT_FOUND;
	}

	if (type === EdgeType.IMPLEMENTS) {
		return UnresolvedEdgeCategory.IMPLEMENTS_INTERFACE_NOT_FOUND;
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
				return UnresolvedEdgeCategory.CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO;
			}
			return UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT;
		}
		if (key.includes(".")) {
			return UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO;
		}
		return UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING;
	}

	return UnresolvedEdgeCategory.OTHER;
}
