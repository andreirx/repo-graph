/**
 * Composition root — the only file that knows about concrete adapters.
 *
 * Instantiates storage, extractor, and indexer.
 * Injects them into the CLI.
 * This is the "Main Component" from Clean Architecture.
 */

import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { SqliteAnnotationsStorage } from "./adapters/annotations/sqlite-annotations-storage.js";
import { RustExtractor } from "./adapters/extractors/rust/rust-extractor.js";
import { TypeScriptExtractor } from "./adapters/extractors/typescript/ts-extractor.js";
import { GitAdapter } from "./adapters/git/git-adapter.js";
import { IstanbulCoverageImporter } from "./adapters/importers/istanbul-coverage.js";
import { RepoIndexer } from "./adapters/indexer/repo-indexer.js";
import { SqliteConnectionProvider } from "./adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "./adapters/storage/sqlite/sqlite-storage.js";
import type { CoverageImporterPort } from "./core/ports/coverage-importer.js";

const RGR_DIR = join(homedir(), ".rgr");
const DEFAULT_DB_PATH = join(RGR_DIR, "repo-graph.db");

export interface AppContext {
	/** Shared SQLite infrastructure. Owns the connection lifecycle. */
	sqliteProvider: SqliteConnectionProvider;
	storage: SqliteStorage;
	/**
	 * Annotations storage adapter. Deliberately separate from the
	 * main storage port — provisional annotations must not be read
	 * by computed-truth surfaces. See annotations-contract.txt §7.
	 */
	annotations: SqliteAnnotationsStorage;
	extractor: TypeScriptExtractor;
	indexer: RepoIndexer;
	git: GitAdapter;
	coverageImporters: CoverageImporterPort[];
}

/**
 * Bootstrap the application. Creates the ~/.rgr directory and
 * initializes all adapters.
 *
 * @param dbPath - Override the default database path (useful for testing).
 */
export async function bootstrap(dbPath?: string): Promise<AppContext> {
	const resolvedDbPath = dbPath ?? process.env.RGR_DB_PATH ?? DEFAULT_DB_PATH;

	// Ensure the data directory exists
	const dir =
		resolvedDbPath === DEFAULT_DB_PATH
			? RGR_DIR
			: resolvedDbPath.substring(0, resolvedDbPath.lastIndexOf("/"));
	mkdirSync(dir, { recursive: true });

	// Shared SQLite infrastructure: opens connection, runs migrations,
	// owns lifecycle. All SQLite-backed adapters share this connection.
	const sqliteProvider = new SqliteConnectionProvider(resolvedDbPath);
	sqliteProvider.initialize();

	// Storage adapter — receives the shared Database handle.
	const storage = new SqliteStorage(sqliteProvider.getDatabase());
	// Annotations adapter — separate port, separate instance, same connection.
	const annotations = new SqliteAnnotationsStorage(
		sqliteProvider.getDatabase(),
	);

	// Initialize extractors (multi-language support).
	const extractor = new TypeScriptExtractor();
	await extractor.initialize();
	const rustExtractor = new RustExtractor();
	await rustExtractor.initialize();

	// Create indexer with all extractors. Files are routed to the
	// correct extractor by extension.
	const indexer = new RepoIndexer(
		storage,
		[extractor, rustExtractor],
		annotations,
	);
	const git = new GitAdapter();
	const coverageImporters: CoverageImporterPort[] = [
		new IstanbulCoverageImporter(),
		// Future: new JacocoCoverageImporter(),
	];

	return {
		sqliteProvider,
		storage,
		annotations,
		extractor,
		indexer,
		git,
		coverageImporters,
	};
}

/**
 * Shut down the application gracefully.
 */
export function shutdown(ctx: AppContext): void {
	ctx.sqliteProvider.close();
}
