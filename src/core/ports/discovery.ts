/**
 * Discovery port — the contract for module discovery adapters.
 *
 * The indexer depends on this port, not on filesystem manifest
 * scanning logic directly. This keeps the dependency direction
 * clean: core/indexer → port ← adapter.
 *
 * The adapter is responsible for:
 *   - Walking the filesystem for manifest files
 *   - Reading manifest content
 *   - Expanding workspace glob patterns against real directories
 *   - Calling the pure manifest detectors
 *   - Returning DiscoveredModuleRoot[] for the orchestrator
 *
 * The orchestrator (src/core/modules/module-discovery.ts) converts
 * discovered roots into candidates, evidence, and ownership.
 */

import type { DiscoveredModuleRoot } from "../modules/manifest-detectors.js";

/**
 * Discovers declared module roots in a repository.
 *
 * Implementations scan the filesystem for manifest/workspace files,
 * parse them, expand glob patterns, and return the detected roots.
 */
export interface ModuleDiscoveryPort {
	/**
	 * Scan the repository for declared module roots.
	 *
	 * @param rootPath - Absolute path to the repository root.
	 * @param repoUid - Repository UID (for file UID construction).
	 * @returns Discovered module roots with evidence metadata.
	 */
	discoverDeclaredModules(
		rootPath: string,
		repoUid: string,
	): Promise<DiscoveredModuleRoot[]>;
}
