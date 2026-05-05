/**
 * Discovery orchestration.
 *
 * Main entry point for provider discovery. Builds candidate list,
 * probes all candidates, and produces a machine-readable report.
 */

import type {
  ProviderCandidate,
  ProbeResult,
  DiscoveryReport,
  DiscoveryConfig,
  ProviderSelection,
  TransportFamily,
  BackendFlavor,
} from './types.js';
import { DEFAULT_PROBE_ENDPOINTS, getFlavorProfile } from './flavors.js';
import { probeCandidate } from './probes.js';

// ── Version ──────────────────────────────────────────────────────────

// This should be imported from package.json in production
const RGISTR_VERSION = '0.2.0';

// ── Main discovery function ──────────────────────────────────────────

/**
 * Discover available LLM providers.
 *
 * Probes all configured/default endpoints and produces a report.
 */
export async function discoverProviders(
  config: DiscoveryConfig = {}
): Promise<DiscoveryReport> {
  const timestamp = new Date().toISOString();
  const notes: string[] = [];

  // Build candidate list
  const candidates = buildCandidateList(config, notes);

  // Get API key for cloud probes
  const apiKey = process.env.OPENAI_API_KEY;
  if (apiKey && !config.localOnly) {
    notes.push('OPENAI_API_KEY detected');
  }

  // Probe all candidates in parallel
  const results = await Promise.all(
    candidates.map((c) =>
      probeCandidate(c, {
        timeoutMs: config.probeTimeoutMs,
        apiKey: c.transport === 'openai_cloud' ? apiKey : undefined,
      })
    )
  );

  // Filter to available providers
  const availableProviders = results.filter((r) => r.success);

  // Auto-select if not explicit
  let selection: ProviderSelection | null = null;
  if (!config.explicitProvider && availableProviders.length > 0) {
    selection = autoSelectProvider(availableProviders, config);
  }

  return {
    timestamp,
    version: RGISTR_VERSION,
    candidates,
    results,
    availableProviders,
    selection,
    notes,
  };
}

// ── Candidate list building ──────────────────────────────────────────

function buildCandidateList(
  config: DiscoveryConfig,
  notes: string[]
): ProviderCandidate[] {
  const candidates: ProviderCandidate[] = [];
  let idCounter = 0;

  const makeId = (prefix: string) => `${prefix}-${++idCounter}`;

  // Add default endpoints
  for (const def of DEFAULT_PROBE_ENDPOINTS) {
    // Skip cloud if localOnly
    if (config.localOnly && def.transport === 'openai_cloud') {
      continue;
    }

    // Skip local if cloudOnly
    if (config.cloudOnly && def.transport !== 'openai_cloud') {
      continue;
    }

    // Skip cloud if no API key
    if (def.transport === 'openai_cloud' && !process.env.OPENAI_API_KEY) {
      continue;
    }

    const flavor = def.flavor;
    const label = flavor
      ? getFlavorProfile(flavor).label
      : def.transport === 'openai_cloud'
        ? 'OpenAI'
        : 'Ollama';

    candidates.push({
      id: makeId(def.flavor ?? def.transport),
      transport: def.transport,
      flavor: flavor,
      endpoint: def.endpoint,
      label,
      source: 'default',
      priority: def.priority,
    });
  }

  // Add additional endpoints from config
  if (config.additionalEndpoints) {
    for (const endpoint of config.additionalEndpoints) {
      // Detect transport from endpoint shape
      const { transport, flavor } = detectTransportFromEndpoint(endpoint);

      candidates.push({
        id: makeId('config'),
        transport,
        flavor,
        endpoint,
        label: `Custom (${endpoint})`,
        source: 'config',
        priority: 100, // Lower priority than defaults
      });

      notes.push(`Additional endpoint configured: ${endpoint}`);
    }
  }

  // Sort by priority
  candidates.sort((a, b) => a.priority - b.priority);

  return candidates;
}

/**
 * Detect transport family from endpoint URL.
 *
 * Heuristic based on port numbers and path patterns.
 */
function detectTransportFromEndpoint(endpoint: string): {
  transport: TransportFamily;
  flavor: BackendFlavor | null;
} {
  try {
    const url = new URL(endpoint);

    // Ollama default port
    if (url.port === '11434') {
      return { transport: 'ollama', flavor: null };
    }

    // LM Studio default port
    if (url.port === '1234') {
      return { transport: 'openai_compatible', flavor: 'lmstudio' };
    }

    // Common local ports - assume OpenAI-compatible
    if (['8080', '8081', '5000', '3000'].includes(url.port)) {
      return { transport: 'openai_compatible', flavor: 'generic' };
    }

    // Cloud OpenAI
    if (url.hostname.includes('openai.com')) {
      return { transport: 'openai_cloud', flavor: null };
    }

    // Default to OpenAI-compatible for unknown
    return { transport: 'openai_compatible', flavor: 'generic' };

  } catch {
    return { transport: 'openai_compatible', flavor: 'generic' };
  }
}

// ── Auto-selection ───────────────────────────────────────────────────

/**
 * Auto-select the best available provider and model.
 */
function autoSelectProvider(
  available: ProbeResult[],
  config: DiscoveryConfig
): ProviderSelection | null {
  // If explicit model requested, find it
  if (config.explicitModel) {
    for (const provider of available) {
      const model = provider.models.find(
        (m) => m.id === config.explicitModel || m.name === config.explicitModel
      );
      if (model) {
        return {
          provider,
          model,
          reason: `Explicit model selection: ${config.explicitModel}`,
        };
      }
    }
    // Explicit model not found - return null to indicate failure
    return null;
  }

  // Find the best preferred model across all providers
  let bestProvider: ProbeResult | null = null;
  let bestModel: { id: string; name: string; isPreferred: boolean; preferenceRank: number | null } | null = null;
  let bestRank = Infinity;

  for (const provider of available) {
    for (const model of provider.models) {
      if (model.isPreferred && model.preferenceRank !== null) {
        if (model.preferenceRank < bestRank) {
          bestRank = model.preferenceRank;
          bestProvider = provider;
          bestModel = model;
        }
      }
    }
  }

  // If found a preferred model, use it
  if (bestProvider && bestModel) {
    return {
      provider: bestProvider,
      model: bestModel,
      reason: `Preferred model match: ${bestModel.name}`,
    };
  }

  // Fallback: first available provider with any model
  for (const provider of available) {
    if (provider.models.length > 0) {
      return {
        provider,
        model: provider.models[0],
        reason: `First available: ${provider.candidate.label}`,
      };
    }
  }

  return null;
}

// ── Report formatting ────────────────────────────────────────────────

/**
 * Format discovery report for human-readable output.
 */
export function formatDiscoveryReport(report: DiscoveryReport): string {
  const lines: string[] = [];

  lines.push('=== rgistr Provider Discovery ===');
  lines.push(`Version: ${report.version}`);
  lines.push(`Timestamp: ${report.timestamp}`);
  lines.push('');

  // Notes
  if (report.notes.length > 0) {
    lines.push('Notes:');
    for (const note of report.notes) {
      lines.push(`  - ${note}`);
    }
    lines.push('');
  }

  // Probe results
  lines.push('Probed endpoints:');
  for (const result of report.results) {
    const status = result.success ? 'OK' : 'FAIL';
    const latency = result.latencyMs !== undefined ? `${result.latencyMs}ms` : '';
    const modelCount = result.success ? `(${result.models.length} models)` : '';
    const error = result.error ? ` - ${result.error}` : '';

    lines.push(
      `  [${status}] ${result.candidate.label} @ ${result.candidate.endpoint} ${latency} ${modelCount}${error}`
    );

    // Show first few models if successful
    if (result.success && result.models.length > 0) {
      const shown = result.models.slice(0, 3);
      for (const model of shown) {
        const pref = model.isPreferred ? ' *' : '';
        lines.push(`       - ${model.name}${pref}`);
      }
      if (result.models.length > 3) {
        lines.push(`       ... and ${result.models.length - 3} more`);
      }
    }
  }
  lines.push('');

  // Selection
  if (report.selection) {
    lines.push('Selected:');
    lines.push(`  Provider: ${report.selection.provider.candidate.label}`);
    lines.push(`  Model: ${report.selection.model.name}`);
    lines.push(`  Reason: ${report.selection.reason}`);
  } else if (report.availableProviders.length === 0) {
    lines.push('No providers available.');
    lines.push('');
    lines.push('To use rgistr, you need one of:');
    lines.push('  - OPENAI_API_KEY environment variable set');
    lines.push('  - LM Studio running at http://127.0.0.1:1234');
    lines.push('  - MLX server running at http://127.0.0.1:8080');
    lines.push('  - Ollama running at http://127.0.0.1:11434');
  } else {
    lines.push('No model selected (no preferred models found).');
  }

  return lines.join('\n');
}

/**
 * Format discovery report as JSON for machine consumption.
 */
export function formatDiscoveryReportJSON(report: DiscoveryReport): string {
  // Create a clean JSON-serializable version
  const output = {
    version: report.version,
    timestamp: report.timestamp,
    notes: report.notes,
    providers: report.results.map((r) => ({
      id: r.candidate.id,
      label: r.candidate.label,
      transport: r.candidate.transport,
      flavor: r.candidate.flavor,
      endpoint: r.candidate.endpoint,
      available: r.success,
      error: r.error ?? null,
      latencyMs: r.latencyMs ?? null,
      models: r.models.map((m) => ({
        id: m.id,
        name: m.name,
        preferred: m.isPreferred,
        preferenceRank: m.preferenceRank,
      })),
    })),
    selection: report.selection
      ? {
          providerId: report.selection.provider.candidate.id,
          providerLabel: report.selection.provider.candidate.label,
          modelId: report.selection.model.id,
          modelName: report.selection.model.name,
          reason: report.selection.reason,
        }
      : null,
  };

  return JSON.stringify(output, null, 2);
}
