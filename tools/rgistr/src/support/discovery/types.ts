/**
 * Discovery support module types.
 *
 * DTOs for provider discovery, backend probing, and normalized reporting.
 * These are transport-neutral policy types, not adapter concerns.
 */

// ── Transport families ───────────────────────────────────────────────

/**
 * Transport family determines the wire protocol.
 *
 * - openai_cloud: OpenAI API with API key auth
 * - openai_compatible: Local OpenAI-compatible servers (LM Studio, MLX, llama.cpp)
 * - ollama: Ollama-native API
 */
export type TransportFamily = 'openai_cloud' | 'openai_compatible' | 'ollama';

/**
 * Backend flavor within the openai_compatible family.
 *
 * Flavor affects:
 * - default probe endpoint
 * - model listing behavior
 * - capability quirks
 * - display labeling
 *
 * Does NOT affect wire protocol (all use OpenAI-compatible transport).
 */
export type BackendFlavor = 'lmstudio' | 'mlx' | 'llamacpp' | 'generic';

// ── Provider candidates ──────────────────────────────────────────────

/**
 * A provider candidate before probing.
 *
 * Represents a potential LLM backend that discovery will attempt to reach.
 */
export interface ProviderCandidate {
  /** Unique identifier for this candidate (e.g., "lmstudio-default"). */
  id: string;

  /** Transport family. */
  transport: TransportFamily;

  /** Backend flavor (only meaningful for openai_compatible). */
  flavor: BackendFlavor | null;

  /** Base endpoint URL to probe. */
  endpoint: string;

  /** Human-readable label for display. */
  label: string;

  /** Source of this candidate (env, default, config). */
  source: 'env' | 'default' | 'config';

  /** Priority for ranking (lower = higher priority). */
  priority: number;
}

// ── Discovered models ────────────────────────────────────────────────

/**
 * A model discovered from a provider.
 */
export interface DiscoveredModel {
  /** Model ID as returned by the backend. */
  id: string;

  /** Normalized model name for display. */
  name: string;

  /** Whether this model matches a preferred model alias. */
  isPreferred: boolean;

  /** Preference rank if preferred (lower = better). */
  preferenceRank: number | null;

  /** Raw model metadata from the backend (for debugging). */
  rawMetadata?: Record<string, unknown>;
}

// ── Probe results ────────────────────────────────────────────────────

/**
 * Result of probing a single provider candidate.
 */
export interface ProbeResult {
  /** The candidate that was probed. */
  candidate: ProviderCandidate;

  /** Whether the probe succeeded. */
  success: boolean;

  /** Error message if probe failed. */
  error?: string;

  /** Response time in milliseconds. */
  latencyMs?: number;

  /** Models discovered from this provider. */
  models: DiscoveredModel[];

  /** Backend version if reported. */
  backendVersion?: string;
}

// ── Discovery report ─────────────────────────────────────────────────

/**
 * Complete discovery report.
 *
 * Machine-readable output for:
 * - CLI display
 * - CI verification
 * - Programmatic selection
 */
export interface DiscoveryReport {
  /** Discovery timestamp (ISO 8601). */
  timestamp: string;

  /** rgistr version that performed discovery. */
  version: string;

  /** All candidates that were probed. */
  candidates: ProviderCandidate[];

  /** Results for each probed candidate. */
  results: ProbeResult[];

  /** Candidates that succeeded (convenience accessor). */
  availableProviders: ProbeResult[];

  /** Selected provider and model (if auto-selection ran). */
  selection: ProviderSelection | null;

  /** Warnings or notes (e.g., "OPENAI_API_KEY set but not tested"). */
  notes: string[];
}

/**
 * Selected provider and model for generation.
 */
export interface ProviderSelection {
  /** Provider that was selected. */
  provider: ProbeResult;

  /** Model that was selected. */
  model: DiscoveredModel;

  /** Reason for selection. */
  reason: string;
}

// ── Preferred model aliases ──────────────────────────────────────────

/**
 * A preferred model alias for ranking.
 *
 * Maps friendly names to actual model IDs that may vary by backend.
 */
export interface PreferredModelAlias {
  /** Friendly alias name (e.g., "qwen3.6"). */
  alias: string;

  /** Model ID patterns that match this alias. */
  patterns: string[];

  /** Preference rank (lower = higher priority). */
  rank: number;

  /** Transport families this alias applies to. */
  transports: TransportFamily[];
}

// ── Flavor profiles ──────────────────────────────────────────────────

/**
 * Backend flavor profile.
 *
 * Defines flavor-specific defaults and quirks without affecting transport.
 */
export interface FlavorProfile {
  /** Backend flavor identifier. */
  flavor: BackendFlavor;

  /** Human-readable label. */
  label: string;

  /** Default base endpoint URL. */
  defaultEndpoint: string;

  /** Path for model listing. */
  modelsPath: string;

  /** Path for health/connection check. */
  healthPath: string;

  /** Known capability quirks. */
  quirks: string[];
}

// ── Discovery configuration ──────────────────────────────────────────

/**
 * Configuration for discovery behavior.
 */
export interface DiscoveryConfig {
  /** Additional endpoints to probe (from config/env). */
  additionalEndpoints?: string[];

  /** Timeout for probes in milliseconds. */
  probeTimeoutMs?: number;

  /** Whether to skip cloud providers. */
  localOnly?: boolean;

  /** Whether to skip local providers. */
  cloudOnly?: boolean;

  /** Explicit provider to use (skip discovery). */
  explicitProvider?: string;

  /** Explicit model to use. */
  explicitModel?: string;
}
