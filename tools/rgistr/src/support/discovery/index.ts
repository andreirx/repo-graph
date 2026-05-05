/**
 * Discovery support module.
 *
 * Provides provider discovery, backend probing, and normalized reporting.
 */

// Types
export type {
  TransportFamily,
  BackendFlavor,
  ProviderCandidate,
  DiscoveredModel,
  ProbeResult,
  DiscoveryReport,
  ProviderSelection,
  PreferredModelAlias,
  FlavorProfile,
  DiscoveryConfig,
} from './types.js';

// Flavor profiles and preferred models
export {
  FLAVOR_PROFILES,
  PREFERRED_MODELS,
  DEFAULT_PROBE_ENDPOINTS,
  getFlavorProfile,
  matchPreferredModel,
} from './flavors.js';

// Probing
export { probeCandidate } from './probes.js';

// Discovery orchestration
export {
  discoverProviders,
  formatDiscoveryReport,
  formatDiscoveryReportJSON,
} from './discover.js';
