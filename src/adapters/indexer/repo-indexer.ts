import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import ignore, { type Ignore } from "ignore";
import { v4 as uuidv4 } from "uuid";
import type {
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
	IndexResult,
} from "../../core/ports/indexer.js";
import type {
	InferenceRow,
	Measurement,
	PersistedBoundaryConsumerFact,
	PersistedBoundaryLink,
	PersistedBoundaryProviderFact,
	PersistedUnresolvedEdge,
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

			// 3+4. Register files, extract, and discard source text per-file.
			// Source text is NOT accumulated in memory. Each file is read,
			// extracted, and the source text is released before the next
			// file is processed. This bounds memory growth to the extraction
			// output (nodes + edges + signals), not the source text corpus.
			const trackedFiles: TrackedFile[] = [];
			const allNodes: GraphNode[] = [];
			const allUnresolvedEdges: UnresolvedEdge[] = [];
			const allMetrics = new Map<
				string,
				{ cc: number; params: number; nesting: number }
			>();
			const fileSignalsCache = new Map<string, FileSignals>();
			const nodeUidToFileUid = new Map<string, string>();
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
					// Register the file as FAILED so it appears in the snapshot
					// and diagnostics. Silent drops create partial indexes that
					// look trustworthy while missing source files.
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

				// Large-file guard: register as SKIPPED so the snapshot
				// knows the file exists but was not extracted.
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
					// Mark file as failed — update the already-persisted version.
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

				allNodes.push(...result.nodes);
				allUnresolvedEdges.push(...result.edges);
				for (const [key, m] of result.metrics) {
					allMetrics.set(key, {
						cc: m.cyclomaticComplexity,
						params: m.parameterCount,
						nesting: m.maxNestingDepth,
					});
				}

				// Build per-file classifier signals.
				// SUBTYPE-AWARE: split same-file symbols into value / class /
				// interface sets so the classifier doesn't misclassify
				// a runtime call against a type-only name (and vice versa).
				const sameFileValueSymbols = new Set<string>();
				const sameFileClassSymbols = new Set<string>();
				const sameFileInterfaceSymbols = new Set<string>();
				for (const node of result.nodes) {
					if (node.kind === NodeKind.SYMBOL) {
						const sub = node.subtype;
						// Value-bindable subtypes (runtime identifiers).
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
					// Build reverse lookup for every extracted node.
					nodeUidToFileUid.set(node.nodeUid, fileUid);
				}
				// Resolve per-file package deps + tsconfig aliases from nearest ancestors.
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
				fileSignalsCache.set(fileUid, {
					importBindings: result.importBindings,
					sameFileValueSymbols,
					sameFileClassSymbols,
					sameFileInterfaceSymbols,
					packageDependencies,
					tsconfigAliases,
				});

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

			// Build import-binding index for call resolution.
				// Maps fileUid → ImportBinding[] so the resolver can use
				// import bindings to disambiguate bare-identifier calls.
				const importBindingsByFile = new Map<string, ImportBinding[]>();
				for (const [fileUid, signals] of fileSignalsCache) {
					if (signals.importBindings.length > 0) {
						importBindingsByFile.set(fileUid, [...signals.importBindings]);
					}
				}

				const { resolved, stillUnresolved, unresolvedBreakdown } =
					this.resolveEdges(
						allUnresolvedEdges,
						allNodes,
						trackedFiles,
						repoUid,
						snapshot.snapshotUid,
						importBindingsByFile,
						compileDb,
					);
			const unresolvedCount = stillUnresolved.length;

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

			// 9a. Classify still-unresolved edges and persist observations.
			// Runs BEFORE count aggregation so both artifacts come from
			// the same unresolved inventory. Purely additive: resolved
			// edges and trust diagnostics are unaffected.
			if (stillUnresolved.length > 0) {
				const observedAt = new Date().toISOString();
				const classifiedRows: PersistedUnresolvedEdge[] = [];
				for (const { edge, category } of stillUnresolved) {
					const sourceFileUid = nodeUidToFileUid.get(edge.sourceNodeUid);
					const fileSignals = sourceFileUid
						? (fileSignalsCache.get(sourceFileUid) ?? emptyFileSignals())
						: emptyFileSignals();
					// Phase 1: generic classification.
					const verdict = classifyUnresolvedEdge(
						edge,
						category,
						snapshotSignals,
						fileSignals,
					);
					let { classification } = verdict;
					let { basisCode } = verdict;

					// Phase 2: framework-boundary post-pass.
					// May override the generic classification for edges
					// matching known runtime-wiring / registration patterns.
					const fwOverride = detectFrameworkBoundary(
						edge.targetKey,
						category,
						fileSignals.importBindings,
					);
					if (fwOverride) {
						classification = fwOverride.classification;
						basisCode = fwOverride.basisCode;
					}

					classifiedRows.push({
						...edge,
						category,
						classification,
						classifierVersion: CURRENT_CLASSIFIER_VERSION,
						basisCode,
						observedAt,
					});
				}
				this.storage.insertUnresolvedEdges(classifiedRows);
			}

			// 9b. Detect framework entrypoints (node-level liveness facts).
			// Scans each file's exports + imports for conventions that
			// indicate the symbol is invoked by an external runtime.
			// Emitted as inferences (kind: "framework_entrypoint").
			{
				const entrypointInferences: InferenceRow[] = [];
				const detectedAt = new Date().toISOString();
				for (const [fileUid, signals] of fileSignalsCache) {
					// Gather exported SYMBOL nodes from this file.
					const fileNodes = allNodes.filter(
						(n) => n.fileUid === fileUid && n.kind === NodeKind.SYMBOL,
					);
					const exportedSymbols = fileNodes
						.filter((n) => n.visibility != null)
						.map((n) => ({
							stableKey: n.stableKey,
							name: n.name,
							visibility: n.visibility,
							subtype: n.subtype,
						}));

					const detected = detectLambdaEntrypoints({
						importBindings: signals.importBindings,
						exportedSymbols,
					});

					for (const ep of detected) {
						entrypointInferences.push({
							inferenceUid: uuidv4(),
							snapshotUid: snapshot.snapshotUid,
							repoUid,
							targetStableKey: ep.targetStableKey,
							kind: "framework_entrypoint",
							valueJson: JSON.stringify({
								convention: ep.convention,
								reason: ep.reason,
							}),
							confidence: ep.confidence,
							basisJson: JSON.stringify({
								convention: ep.convention,
								classifier_version: CURRENT_CLASSIFIER_VERSION,
							}),
							extractor: INDEXER_VERSION,
							createdAt: detectedAt,
						});
					}
				}
				if (entrypointInferences.length > 0) {
					this.storage.deleteInferencesByKind(
						snapshot.snapshotUid,
						"framework_entrypoint",
					);
					this.storage.insertInferences(entrypointInferences);
				}
			}

			// 9d. Spring container-managed bean detection.
				// Scans Java files for Spring stereotype annotations and @Bean
				// factory methods. Emits inferences (kind: "spring_container_managed")
				// that suppress false dead-code reports for container-wired classes.
				{
					const springInferences: InferenceRow[] = [];
					const detectedAt = new Date().toISOString();
					for (const relPath of filePaths) {
						if (!relPath.endsWith(".java")) continue;
						let content: string;
						try {
							content = await readFile(join(repo.rootPath, relPath), "utf-8");
						} catch { continue; }
						if (content.length > MAX_FILE_SIZE_BYTES) continue;
						const fileUid = `${repoUid}:${relPath}`;

						const fileSymbols = allNodes
							.filter(
								(n) => n.fileUid === fileUid && n.kind === NodeKind.SYMBOL,
							)
							.map((n) => ({
								stableKey: n.stableKey,
								name: n.name,
								qualifiedName: n.qualifiedName ?? n.name,
								subtype: n.subtype,
								lineStart: n.location?.lineStart ?? null,
							}));

						const beans = detectSpringBeans(content, relPath, fileSymbols);
						for (const bean of beans) {
							springInferences.push({
								inferenceUid: uuidv4(),
								snapshotUid: snapshot.snapshotUid,
								repoUid,
								targetStableKey: bean.targetStableKey,
								kind: "spring_container_managed",
								valueJson: JSON.stringify({
									annotation: bean.annotation,
									convention: bean.convention,
									reason: bean.reason,
								}),
								confidence: bean.confidence,
								basisJson: JSON.stringify({
									convention: bean.convention,
									classifier_version: CURRENT_CLASSIFIER_VERSION,
								}),
								extractor: INDEXER_VERSION,
								createdAt: detectedAt,
							});
						}
					}
					if (springInferences.length > 0) {
						this.storage.deleteInferencesByKind(
							snapshot.snapshotUid,
							"spring_container_managed",
						);
						this.storage.insertInferences(springInferences);
					}
				}

			// 9e. Pytest test/fixture detection.
				// Scans Python files for pytest conventions (test_* functions,
				// Test* classes, @pytest.fixture). Emits inferences that
				// suppress false dead-code reports for test infrastructure.
				{
					const pytestInferences: InferenceRow[] = [];
					const detectedAt = new Date().toISOString();
					for (const relPath of filePaths) {
						if (!relPath.endsWith(".py")) continue;
						let content: string;
						try {
							content = await readFile(join(repo.rootPath, relPath), "utf-8");
						} catch { continue; }
						if (content.length > MAX_FILE_SIZE_BYTES) continue;
						const fileUid = `${repoUid}:${relPath}`;

						const fileSymbols = allNodes
							.filter(
								(n) => n.fileUid === fileUid && n.kind === NodeKind.SYMBOL,
							)
							.map((n) => ({
								stableKey: n.stableKey,
								name: n.name,
								qualifiedName: n.qualifiedName ?? n.name,
								subtype: n.subtype,
								lineStart: n.location?.lineStart ?? null,
							}));

						const items = detectPytestItems(content, relPath, fileSymbols);
						for (const item of items) {
							const kind = item.convention.startsWith("pytest_fixture")
								? "pytest_fixture"
								: "pytest_test";
							pytestInferences.push({
								inferenceUid: uuidv4(),
								snapshotUid: snapshot.snapshotUid,
								repoUid,
								targetStableKey: item.targetStableKey,
								kind,
								valueJson: JSON.stringify({
									convention: item.convention,
									reason: item.reason,
								}),
								confidence: item.confidence,
								basisJson: JSON.stringify({
									convention: item.convention,
									classifier_version: CURRENT_CLASSIFIER_VERSION,
								}),
								extractor: INDEXER_VERSION,
								createdAt: detectedAt,
							});
						}
					}
					if (pytestInferences.length > 0) {
						this.storage.deleteInferencesByKind(snapshot.snapshotUid, "pytest_test");
						this.storage.deleteInferencesByKind(snapshot.snapshotUid, "pytest_fixture");
						this.storage.insertInferences(pytestInferences);
					}
				}

			// 9f. Linux/system framework detection (C/C++).
				// Scans C/C++ files for kernel module, GCC constructor, and
				// handler registration patterns. Emits inferences that suppress
				// false dead-code reports for framework-managed symbols.
				{
					const systemInferences: InferenceRow[] = [];
					const detectedAt = new Date().toISOString();
					for (const relPath of filePaths) {
						const ext = relPath.slice(relPath.lastIndexOf("."));
						if (![".c", ".h", ".cpp", ".hpp", ".cc", ".cxx", ".hxx"].includes(ext)) continue;
						let content: string;
						try {
							content = await readFile(join(repo.rootPath, relPath), "utf-8");
						} catch { continue; }
						if (content.length > MAX_FILE_SIZE_BYTES) continue;
						const fileUid = `${repoUid}:${relPath}`;

						const fileSymbols = allNodes
							.filter((n) => n.fileUid === fileUid && n.kind === NodeKind.SYMBOL)
							.map((n) => ({
								stableKey: n.stableKey,
								name: n.name,
								qualifiedName: n.qualifiedName ?? n.name,
								subtype: n.subtype,
								lineStart: n.location?.lineStart ?? null,
							}));

						const entries = detectLinuxSystemPatterns(content, relPath, fileSymbols);
						for (const entry of entries) {
							systemInferences.push({
								inferenceUid: uuidv4(),
								snapshotUid: snapshot.snapshotUid,
								repoUid,
								targetStableKey: entry.targetStableKey,
								kind: "linux_system_managed",
								valueJson: JSON.stringify({
									convention: entry.convention,
									reason: entry.reason,
								}),
								confidence: entry.confidence,
								basisJson: JSON.stringify({
									convention: entry.convention,
									classifier_version: CURRENT_CLASSIFIER_VERSION,
								}),
								extractor: INDEXER_VERSION,
								createdAt: detectedAt,
							});
						}
					}
					if (systemInferences.length > 0) {
						this.storage.deleteInferencesByKind(
							snapshot.snapshotUid,
							"linux_system_managed",
						);
						this.storage.insertInferences(systemInferences);
					}
				}

				// 9c. Boundary fact extraction (PROTOTYPE).
				// Runs boundary-specific extractors on applicable files,
				// persists raw facts (source of truth), then materializes
				// intra-repo derived links (convenience artifact).
				//
				// This is separate from the main extractor pipeline because
				// boundary facts are NOT ExtractionResult objects — they
				// have a different shape (BoundaryProviderFact/ConsumerFact)
				// and live in separate tables, not in nodes/edges.
				{
					const providerFacts: PersistedBoundaryProviderFact[] = [];
					const consumerFacts: PersistedBoundaryConsumerFact[] = [];
					const boundaryObservedAt = new Date().toISOString();

					for (const relPath of filePaths) {
						// Re-read source for boundary extraction.
						// fileContents is no longer in memory (released per-file
						// during extraction). Boundary extractors are lightweight
						// regex scanners, so the re-read cost is acceptable.
						let content: string;
						try {
							content = await readFile(
								join(repo.rootPath, relPath),
								"utf-8",
							);
						} catch {
							continue;
						}
						if (content.length > MAX_FILE_SIZE_BYTES) continue;
						const fileUid = `${repoUid}:${relPath}`;

						// Gather symbols from this file for caller/handler attribution.
						const fileSymbols = allNodes
							.filter(
								(n) =>
									n.fileUid === fileUid &&
									n.kind === NodeKind.SYMBOL,
							)
							.map((n) => ({
								stableKey: n.stableKey,
								name: n.name,
								qualifiedName: n.qualifiedName ?? n.name,
								lineStart: n.location?.lineStart ?? null,
							}));

						// Java files: extract Spring route provider facts.
						// Note: this reparses the file with tree-sitter-java
						// because the Java extractor does not expose its parse
						// tree. Passing the tree through would avoid this cost
						// but requires extending ExtractionResult. Deferred.
						if (relPath.endsWith(".java")) {
							if (!this.springParserReady) {
								await initSpringRouteParser();
								this.springParserReady = true;
							}
							const routes = extractSpringRoutes(
								content,
								relPath,
								repoUid,
								fileSymbols,
							);
							for (const r of routes) {
								const strategy = getMatchStrategy(r.mechanism);
								const matcherKey = strategy
									? strategy.computeMatcherKey(r.address, r.metadata)
									: r.operation;
								providerFacts.push({
									...r,
									factUid: uuidv4(),
									snapshotUid: snapshot.snapshotUid,
									repoUid,
									matcherKey,
									extractor: "spring-route-extractor:0.1",
									observedAt: boundaryObservedAt,
								});
							}
						}

						// TS/JS files: extract Express route provider facts AND
						// HTTP client consumer facts. Resolve file-local string
						// bindings first so both extractors can recover constants.
						if (
							relPath.endsWith(".ts") ||
							relPath.endsWith(".tsx") ||
							relPath.endsWith(".js") ||
							relPath.endsWith(".jsx")
						) {
							// Lazy-init the string resolver on first use.
							if (!this.stringResolver) {
								this.stringResolver = new FileLocalStringResolver();
								await this.stringResolver.initialize();
							}
							const bindings = this.stringResolver.resolve(content, relPath);

							// Commander CLI command provider facts.
							const cliCommands = extractCommanderCommands(
								content,
								relPath,
								repoUid,
								fileSymbols,
							);
							for (const cmd of cliCommands) {
								const strategy = getMatchStrategy(cmd.mechanism);
								const matcherKey = strategy
									? strategy.computeMatcherKey(cmd.address, cmd.metadata)
									: cmd.operation;
								providerFacts.push({
									...cmd,
									factUid: uuidv4(),
									snapshotUid: snapshot.snapshotUid,
									repoUid,
									matcherKey,
									extractor: "commander-command-extractor:0.1",
									observedAt: boundaryObservedAt,
								});
							}

							// Express route provider facts.
							const routes = extractExpressRoutes(
								content,
								relPath,
								repoUid,
								fileSymbols,
								bindings,
							);
							for (const r of routes) {
								const strategy = getMatchStrategy(r.mechanism);
								const matcherKey = strategy
									? strategy.computeMatcherKey(r.address, r.metadata)
									: r.operation;
								providerFacts.push({
									...r,
									factUid: uuidv4(),
									snapshotUid: snapshot.snapshotUid,
									repoUid,
									matcherKey,
									extractor: "express-route-extractor:0.1",
									observedAt: boundaryObservedAt,
								});
							}

							// HTTP client consumer facts.
							const requests = extractHttpClientRequests(
								content,
								relPath,
								repoUid,
								fileSymbols,
								bindings,
							);
							for (const c of requests) {
								const strategy = getMatchStrategy(c.mechanism);
								const matcherKey = strategy
									? strategy.computeMatcherKey(c.address, c.metadata)
									: c.operation;
								consumerFacts.push({
									...c,
									factUid: uuidv4(),
									snapshotUid: snapshot.snapshotUid,
									repoUid,
									matcherKey,
									extractor: "http-client-extractor:0.1",
									observedAt: boundaryObservedAt,
								});
							}
						}
					}

					// 9c-ii. Extract CLI consumer facts from package.json scripts.
					// Reads package.json files in the repo and parses "scripts"
					// entries as cli_command consumer invocations.
					{
						const packageJsonPaths = await this.findPackageJsonFiles(repo.rootPath);
						for (const pkgRelPath of packageJsonPaths) {
							try {
								const pkgContent = await readFile(
									join(repo.rootPath, pkgRelPath),
									"utf-8",
								);
								const pkg = JSON.parse(pkgContent);
								if (pkg.scripts && typeof pkg.scripts === "object") {
									const scriptFacts = extractPackageScriptConsumers(
										pkg.scripts,
										pkgRelPath,
										repoUid,
									);
									for (const c of scriptFacts) {
										const strategy = getMatchStrategy(c.mechanism);
										const matcherKey = strategy
											? strategy.computeMatcherKey(c.address, c.metadata)
											: c.operation;
										consumerFacts.push({
											...c,
											factUid: uuidv4(),
											snapshotUid: snapshot.snapshotUid,
											repoUid,
											matcherKey,
											extractor: "package-script-cli-extractor:0.1",
											observedAt: boundaryObservedAt,
										});
									}
								}
							} catch {
								// Skip unreadable / unparseable package.json files.
							}
						}
					}

					// 9c-iii. Extract CLI consumer facts from shell scripts.
					{
						const shellPaths = await this.findShellScripts(repo.rootPath);
						for (const shellRelPath of shellPaths) {
							try {
								const shellContent = await readFile(
									join(repo.rootPath, shellRelPath),
									"utf-8",
								);
								const shellFacts = extractShellScriptConsumers(
									shellContent,
									shellRelPath,
									repoUid,
								);
								for (const c of shellFacts) {
									const strategy = getMatchStrategy(c.mechanism);
									const matcherKey = strategy
										? strategy.computeMatcherKey(c.address, c.metadata)
										: c.operation;
									consumerFacts.push({
										...c,
										factUid: uuidv4(),
										snapshotUid: snapshot.snapshotUid,
										repoUid,
										matcherKey,
										extractor: "shell-script-cli-extractor:0.1",
										observedAt: boundaryObservedAt,
									});
								}
							} catch {
								// Skip unreadable shell scripts.
							}
						}
					}

					// 9c-iv. Extract CLI consumer facts from Makefiles.
					{
						const makefilePaths = await this.findMakefiles(repo.rootPath);
						for (const mkRelPath of makefilePaths) {
							try {
								const mkContent = await readFile(
									join(repo.rootPath, mkRelPath),
									"utf-8",
								);
								const mkFacts = extractMakefileConsumers(
									mkContent,
									mkRelPath,
									repoUid,
								);
								for (const c of mkFacts) {
									const strategy = getMatchStrategy(c.mechanism);
									const matcherKey = strategy
										? strategy.computeMatcherKey(c.address, c.metadata)
										: c.operation;
									consumerFacts.push({
										...c,
										factUid: uuidv4(),
										snapshotUid: snapshot.snapshotUid,
										repoUid,
										matcherKey,
										extractor: "makefile-cli-extractor:0.1",
										observedAt: boundaryObservedAt,
									});
								}
							} catch {
								// Skip unreadable Makefiles.
							}
						}
					}

					// Persist raw facts (source of truth).
					if (providerFacts.length > 0) {
						this.storage.insertBoundaryProviderFacts(providerFacts);
					}
					if (consumerFacts.length > 0) {
						this.storage.insertBoundaryConsumerFacts(consumerFacts);
					}

					// Materialize intra-repo derived links (convenience artifact).
					// These are DISCARDABLE — they can be regenerated from raw facts.
					//
					// The matcher accepts persisted facts (which carry factUid) and
					// returns candidates with stable UIDs — no object-identity
					// assumptions across the strategy boundary.
					if (providerFacts.length > 0 && consumerFacts.length > 0) {
						const candidates = matchBoundaryFacts(
							providerFacts,
							consumerFacts,
						);
						if (candidates.length > 0) {
							const materializedAt = new Date().toISOString();
							const links: PersistedBoundaryLink[] = candidates.map(
								(c) => ({
									linkUid: uuidv4(),
									snapshotUid: snapshot.snapshotUid,
									repoUid,
									providerFactUid: c.providerFactUid,
									consumerFactUid: c.consumerFactUid,
									matchBasis: c.matchBasis,
									confidence: c.confidence,
									metadataJson: null,
									materializedAt,
								}),
							);
							this.storage.insertBoundaryLinks(links);
						}
					}
				}

				// 10. Persist function-level measurements
			if (allMetrics.size > 0) {
				const now = new Date().toISOString();
				const measurements: Measurement[] = [];
				for (const [stableKey, m] of allMetrics) {
					measurements.push({
						measurementUid: uuidv4(),
						snapshotUid: snapshot.snapshotUid,
						repoUid,
						targetStableKey: stableKey,
						kind: "cyclomatic_complexity",
						valueJson: JSON.stringify({ value: m.cc }),
						source: INDEXER_VERSION,
						createdAt: now,
					});
					measurements.push({
						measurementUid: uuidv4(),
						snapshotUid: snapshot.snapshotUid,
						repoUid,
						targetStableKey: stableKey,
						kind: "parameter_count",
						valueJson: JSON.stringify({ value: m.params }),
						source: INDEXER_VERSION,
						createdAt: now,
					});
					measurements.push({
						measurementUid: uuidv4(),
						snapshotUid: snapshot.snapshotUid,
						repoUid,
						targetStableKey: stableKey,
						kind: "max_nesting_depth",
						valueJson: JSON.stringify({ value: m.nesting }),
						source: INDEXER_VERSION,
						createdAt: now,
					});
				}
				this.storage.insertMeasurements(measurements);
			}

			// 11. Extract domain versions from manifest files
			await this.extractManifestVersions(
				repo.rootPath,
				repoUid,
				snapshot.snapshotUid,
			);

			// 11a. Extract provisional annotations (README + package.json
			// description) and persist via AnnotationsPort. Writes-only;
			// indexer never reads annotations back. Skipped silently when
			// no port is injected (annotations-contract.txt §7).
			const annotationCollisionsDropped = this.annotations
				? await this.extractAndPersistAnnotations(
						repo.rootPath,
						repoUid,
						snapshot.snapshotUid,
						allNodes,
					)
				: 0;

			// 12. Finalize snapshot
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

			// 12. Persist snapshot-level extraction diagnostics.
			// These would otherwise be lost after this method returns.
			// The trust reporting surface reads them on demand.
			const extractionDiagnostics = {
				diagnostics_version: 1,
				edges_total: allEdges.length,
				unresolved_total: unresolvedCount,
				unresolved_breakdown: unresolvedBreakdown,
				annotation_collisions_dropped: annotationCollisionsDropped,
				files_skipped_oversized: skippedOversized,
				files_read_failed: filesReadFailed,
			};
			this.storage.updateSnapshotExtractionDiagnostics(
				snapshot.snapshotUid,
				JSON.stringify(extractionDiagnostics),
			);

			return {
				snapshotUid: snapshot.snapshotUid,
				filesTotal: trackedFiles.length,
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

	async refreshRepo(
		repoUid: string,
		options?: IndexOptions,
	): Promise<IndexResult> {
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
		return this.runIndex(repoUid, SnapshotKind.REFRESH, options);
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
		importBindingsByFile?: Map<string, ImportBinding[]>,
		compileDb?: CompilationDatabase | null,
	): {
		resolved: GraphEdge[];
		stillUnresolved: Array<{
			edge: UnresolvedEdge;
			category: UnresolvedEdgeCategory;
		}>;
		unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>>;
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

		// C/C++ include path resolution: build a per-source-file include
		// resolution map from compile_commands.json. Each source file has
		// its own -I flags — different translation units may have different
		// include paths. The resolver uses the source file's include paths
		// to resolve #include directives, not a global flattened set.
		//
		// Map: sourceFileUid → Map<bareHeaderName, resolvedStableKey>
		const perFileIncludeResolution = new Map<string, Map<string, string>>();
		if (compileDb) {
			// Build a lookup: headerPath → stableKey for all header files.
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

		// Build nodeUid → fileUid for import-binding-assisted call resolution.
		const nodeUidToFileUid = new Map<string, string>();
		if (importBindingsByFile) {
			for (const node of allNodes) {
				if (node.fileUid) nodeUidToFileUid.set(node.nodeUid, node.fileUid);
			}
		}

		const resolved: GraphEdge[] = [];
		const stillUnresolved: Array<{
			edge: UnresolvedEdge;
			category: UnresolvedEdgeCategory;
		}> = [];
		const unresolvedBreakdown: Partial<Record<UnresolvedEdgeCategory, number>> = {};

		for (const edge of unresolved) {
			const targetNodeUid = this.resolveTarget(
				edge,
				nodesByStableKey,
				nodesByName,
				fileResolution,
				importBindingsByFile,
				nodeUidToFileUid,
				perFileIncludeResolution,
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
				// Categorize for aggregate diagnostics AND carry the
				// category forward to the semantic classifier.
				const category = categorizeUnresolvedEdge(edge);
				stillUnresolved.push({ edge, category });
				unresolvedBreakdown[category] =
					(unresolvedBreakdown[category] ?? 0) + 1;
			}
		}

		return { resolved, stillUnresolved, unresolvedBreakdown };
	}

	private resolveTarget(
		edge: UnresolvedEdge,
		nodesByStableKey: Map<string, GraphNode>,
		nodesByName: Map<string, GraphNode[]>,
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
		nodesByStableKey: Map<string, GraphNode>,
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
		nodesByStableKey: Map<string, GraphNode>,
		nodesByName: Map<string, GraphNode[]>,
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
		_nodesByStableKey: Map<string, GraphNode>,
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
		allNodes: GraphNode[],
	): Promise<number> {
		if (!this.annotations) return 0;

		// Build a lookup: directory path → MODULE stable_key
		const moduleByPath = new Map<string, string>();
		for (const n of allNodes) {
			if (n.kind === NodeKind.MODULE && n.qualifiedName) {
				moduleByPath.set(n.qualifiedName, n.stableKey);
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
