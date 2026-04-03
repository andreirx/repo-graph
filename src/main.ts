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
import { TypeScriptExtractor } from "./adapters/extractors/typescript/ts-extractor.js";
import { GitAdapter } from "./adapters/git/git-adapter.js";
import { RepoIndexer } from "./adapters/indexer/repo-indexer.js";
import { SqliteStorage } from "./adapters/storage/sqlite/sqlite-storage.js";

const RGR_DIR = join(homedir(), ".rgr");
const DEFAULT_DB_PATH = join(RGR_DIR, "repo-graph.db");

export interface AppContext {
	storage: SqliteStorage;
	extractor: TypeScriptExtractor;
	indexer: RepoIndexer;
	git: GitAdapter;
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

	// Initialize storage
	const storage = new SqliteStorage(resolvedDbPath);
	storage.initialize();

	// Initialize extractor
	const extractor = new TypeScriptExtractor();
	await extractor.initialize();

	// Create indexer and git adapter
	const indexer = new RepoIndexer(storage, extractor);
	const git = new GitAdapter();

	return { storage, extractor, indexer, git };
}

/**
 * Shut down the application gracefully.
 */
export function shutdown(ctx: AppContext): void {
	ctx.storage.close();
}
