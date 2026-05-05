/**
 * Backend probe functions.
 *
 * Probes check if a provider is available and enumerate its models.
 * Each transport family has its own probe implementation.
 */

import type {
  ProviderCandidate,
  ProbeResult,
  DiscoveredModel,
  TransportFamily,
  BackendFlavor,
} from './types.js';
import { getFlavorProfile, matchPreferredModel } from './flavors.js';

// ── Probe configuration ──────────────────────────────────────────────

const DEFAULT_PROBE_TIMEOUT_MS = 5000;

// ── Main probe dispatcher ────────────────────────────────────────────

/**
 * Probe a provider candidate.
 *
 * Dispatches to the appropriate transport-specific probe.
 */
export async function probeCandidate(
  candidate: ProviderCandidate,
  options: { timeoutMs?: number; apiKey?: string } = {}
): Promise<ProbeResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_PROBE_TIMEOUT_MS;
  const startTime = Date.now();

  try {
    let result: ProbeResult;

    switch (candidate.transport) {
      case 'openai_cloud':
        result = await probeOpenAICloud(candidate, options.apiKey, timeoutMs);
        break;

      case 'openai_compatible':
        result = await probeOpenAICompatible(candidate, timeoutMs);
        break;

      case 'ollama':
        result = await probeOllama(candidate, timeoutMs);
        break;

      default:
        result = {
          candidate,
          success: false,
          error: `Unknown transport family: ${candidate.transport}`,
          models: [],
        };
    }

    result.latencyMs = Date.now() - startTime;
    return result;

  } catch (error) {
    return {
      candidate,
      success: false,
      error: error instanceof Error ? error.message : String(error),
      latencyMs: Date.now() - startTime,
      models: [],
    };
  }
}

// ── OpenAI Cloud probe ───────────────────────────────────────────────

async function probeOpenAICloud(
  candidate: ProviderCandidate,
  apiKey: string | undefined,
  timeoutMs: number
): Promise<ProbeResult> {
  if (!apiKey) {
    return {
      candidate,
      success: false,
      error: 'OPENAI_API_KEY not set',
      models: [],
    };
  }

  try {
    const response = await fetch(`${candidate.endpoint}/models`, {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!response.ok) {
      return {
        candidate,
        success: false,
        error: `HTTP ${response.status}: ${response.statusText}`,
        models: [],
      };
    }

    const data = await response.json() as { data?: Array<{ id: string }> };
    const models = parseOpenAIModelList(data, candidate.transport);

    return {
      candidate,
      success: true,
      models,
    };

  } catch (error) {
    return {
      candidate,
      success: false,
      error: error instanceof Error ? error.message : String(error),
      models: [],
    };
  }
}

// ── OpenAI-compatible local probe ────────────────────────────────────

async function probeOpenAICompatible(
  candidate: ProviderCandidate,
  timeoutMs: number
): Promise<ProbeResult> {
  const flavor = candidate.flavor ?? 'generic';
  const profile = getFlavorProfile(flavor);

  // Some backends (llama.cpp) have a separate health endpoint
  const healthUrl = `${candidate.endpoint}${profile.healthPath}`;
  const modelsUrl = `${candidate.endpoint}${profile.modelsPath}`;

  try {
    // First check health/availability
    if (profile.healthPath !== profile.modelsPath) {
      const healthResponse = await fetch(healthUrl, {
        method: 'GET',
        signal: AbortSignal.timeout(timeoutMs),
      });

      if (!healthResponse.ok) {
        return {
          candidate,
          success: false,
          error: `Health check failed: HTTP ${healthResponse.status}`,
          models: [],
        };
      }
    }

    // Then get models
    const modelsResponse = await fetch(modelsUrl, {
      method: 'GET',
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!modelsResponse.ok) {
      return {
        candidate,
        success: false,
        error: `Models endpoint failed: HTTP ${modelsResponse.status}`,
        models: [],
      };
    }

    const data = await modelsResponse.json() as { data?: Array<{ id: string }> };
    const models = parseOpenAIModelList(data, candidate.transport);

    // Try to detect backend version from response headers
    const serverHeader = modelsResponse.headers.get('server');

    return {
      candidate,
      success: true,
      models,
      backendVersion: serverHeader ?? undefined,
    };

  } catch (error) {
    return {
      candidate,
      success: false,
      error: error instanceof Error ? error.message : String(error),
      models: [],
    };
  }
}

// ── Ollama probe ─────────────────────────────────────────────────────

async function probeOllama(
  candidate: ProviderCandidate,
  timeoutMs: number
): Promise<ProbeResult> {
  try {
    const response = await fetch(`${candidate.endpoint}/api/tags`, {
      method: 'GET',
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!response.ok) {
      return {
        candidate,
        success: false,
        error: `HTTP ${response.status}: ${response.statusText}`,
        models: [],
      };
    }

    const data = await response.json() as {
      models?: Array<{
        name: string;
        model?: string;
        modified_at?: string;
        size?: number;
      }>;
    };

    const models: DiscoveredModel[] = (data.models ?? []).map((m) => {
      const id = m.name;
      const match = matchPreferredModel(id, 'ollama');

      return {
        id,
        name: id,
        isPreferred: match !== null,
        preferenceRank: match?.rank ?? null,
        rawMetadata: m as Record<string, unknown>,
      };
    });

    // Sort by preference rank (preferred first, then alphabetical)
    models.sort((a, b) => {
      if (a.isPreferred && !b.isPreferred) return -1;
      if (!a.isPreferred && b.isPreferred) return 1;
      if (a.preferenceRank !== null && b.preferenceRank !== null) {
        return a.preferenceRank - b.preferenceRank;
      }
      return a.id.localeCompare(b.id);
    });

    return {
      candidate,
      success: true,
      models,
    };

  } catch (error) {
    return {
      candidate,
      success: false,
      error: error instanceof Error ? error.message : String(error),
      models: [],
    };
  }
}

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Parse OpenAI-format model list response.
 */
function parseOpenAIModelList(
  data: { data?: Array<{ id: string; [key: string]: unknown }> },
  transport: TransportFamily
): DiscoveredModel[] {
  const rawModels = data.data ?? [];

  const models: DiscoveredModel[] = rawModels.map((m) => {
    const id = m.id;
    const match = matchPreferredModel(id, transport);

    return {
      id,
      name: normalizeModelName(id),
      isPreferred: match !== null,
      preferenceRank: match?.rank ?? null,
      rawMetadata: m,
    };
  });

  // Sort by preference rank (preferred first, then alphabetical)
  models.sort((a, b) => {
    if (a.isPreferred && !b.isPreferred) return -1;
    if (!a.isPreferred && b.isPreferred) return 1;
    if (a.preferenceRank !== null && b.preferenceRank !== null) {
      return a.preferenceRank - b.preferenceRank;
    }
    return a.id.localeCompare(b.id);
  });

  return models;
}

/**
 * Normalize model name for display.
 *
 * Strips common prefixes like paths in LM Studio model IDs.
 */
function normalizeModelName(modelId: string): string {
  // LM Studio often includes full paths like "lmstudio-community/qwen..."
  // Extract just the model name part
  const parts = modelId.split('/');
  if (parts.length > 1) {
    return parts[parts.length - 1];
  }
  return modelId;
}
