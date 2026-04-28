/**
 * Graph context adapters.
 *
 * The graph context adapter provides deterministic facts from repo-graph
 * to enrich LLM prompts beyond code_only synthesis.
 *
 * Adapters:
 * - NullGraphContextAdapter: returns null (code_only mode)
 * - RmapGraphContextAdapter: shells out to `rmap context` (future work)
 */

export { NullGraphContextAdapter } from './NullGraphContextAdapter.js';
export type { IGraphContextAdapter, GraphContext } from '../../core/types.js';
