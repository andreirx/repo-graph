/**
 * Null graph context adapter.
 * Returns null for all context requests.
 * Used for code_only synthesis mode.
 */

import type { IGraphContextAdapter, GraphContext } from '../../core/types.js';

export class NullGraphContextAdapter implements IGraphContextAdapter {
  readonly name = 'null';

  async getContext(
    _scopeKind: 'file' | 'folder',
    _scopePath: string
  ): Promise<GraphContext | null> {
    return null;
  }

  async isAvailable(): Promise<boolean> {
    return true; // Always available - just returns null
  }
}
