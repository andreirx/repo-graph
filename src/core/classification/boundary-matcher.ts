/**
 * Boundary matcher — strategy-dispatched provider/consumer pairing.
 *
 * Pure core business logic. No I/O, no storage, no framework awareness.
 * The orchestrator dispatches by mechanism to the correct match strategy.
 *
 * Architecture:
 *   - BoundaryMatchStrategy: one per mechanism (http, grpc, ioctl, ...)
 *   - Each strategy owns its own address normalization and matching rules.
 *   - The orchestrator groups facts by mechanism and delegates matching.
 *   - computeMatcherKey: used by the indexer at persist time to populate
 *     the matcher_key column. Same normalization as the match logic.
 *
 * HTTP matching:
 *   Structured segment normalization. Path is split into segments.
 *   Literal segments compare exactly. Parameter segments ({id}, :id,
 *   {param}) normalize to wildcard token {_}. Method is prefixed.
 *   Matching is by normalized key equality.
 *
 * Maturity: PROTOTYPE. Sufficient to validate the boundary-fact model.
 * Known limitations:
 *   - No query parameter matching
 *   - No route prefix/wildcard segment matching (e.g. /api/**)
 *   - No cross-mechanism matching (HTTP consumer → gRPC provider)
 *   - Confidence scoring is a simple product, not calibrated
 */

import type {
	BoundaryConsumerFact,
	BoundaryLinkCandidate,
	BoundaryMechanism,
	BoundaryProviderFact,
} from "./api-boundary.js";

// ── Matchable fact types ────────────────────────────────────────────

/**
 * A provider fact with a stable persisted UID.
 * The matcher requires UIDs so that link candidates carry stable
 * identifiers back to the persisted facts — no object-identity
 * assumptions across the strategy boundary.
 */
export type MatchableProviderFact = BoundaryProviderFact & {
	factUid: string;
};

/**
 * A consumer fact with a stable persisted UID.
 */
export type MatchableConsumerFact = BoundaryConsumerFact & {
	factUid: string;
};

// ── Strategy interface ──────────────────────────────────────────────

/**
 * A mechanism-specific match strategy. One per boundary mechanism.
 *
 * Responsibilities:
 *   1. Normalize an address into a matcher key string
 *   2. Match providers to consumers for this mechanism
 *
 * Contract: the match method receives facts with stable persisted
 * UIDs. Link candidates MUST carry those UIDs, not object references.
 * Strategies are free to normalize, clone, or reconstruct working
 * objects internally — the UID is the stable association.
 */
export interface BoundaryMatchStrategy {
	/** The mechanism this strategy handles. */
	readonly mechanism: BoundaryMechanism;

	/**
	 * Compute the normalized matcher key for an address.
	 *
	 * Used at two sites:
	 *   - Persist time (indexer computes key for the DB column)
	 *   - Match time (strategy normalizes before comparing)
	 *
	 * @param address - The raw address from the fact (e.g. "/api/v2/products/{id}")
	 * @param metadata - Mechanism-specific metadata (e.g. { httpMethod: "GET" })
	 */
	computeMatcherKey(address: string, metadata: Record<string, unknown>): string;

	/**
	 * Match providers against consumers for this mechanism.
	 * Both arrays are pre-filtered to this mechanism.
	 * Returns zero or more link candidates carrying persisted fact UIDs.
	 */
	match(
		providers: MatchableProviderFact[],
		consumers: MatchableConsumerFact[],
	): BoundaryLinkCandidate[];
}

// ── HTTP match strategy ─────────────────────────────────────────────

/** Wildcard token used in normalized path segments for any parameter. */
const PARAM_WILDCARD = "{_}";

/**
 * Normalize a single path segment for matching.
 *
 * Patterns recognized as parameters:
 *   - {id}, {productId}, {param} — curly-brace path variable (Spring, consumer extractor)
 *   - :id, :productId — colon path parameter (Express, Koa)
 *
 * Everything else is treated as a literal segment.
 */
function normalizeSegment(segment: string): string {
	if (segment.startsWith("{") && segment.endsWith("}")) return PARAM_WILDCARD;
	if (segment.startsWith(":") && segment.length > 1) return PARAM_WILDCARD;
	return segment;
}

/**
 * Normalize an HTTP path into a canonical segment representation.
 *
 * Input: "/api/v2/products/{id}/reviews"
 * Output: "/api/v2/products/{_}/reviews"
 *
 * The raw path is preserved in the fact; this produces only the
 * matching key. Parameter names are discarded for matching purposes
 * because provider and consumer use different naming conventions
 * (Spring {id} vs consumer {param} vs Express :productId).
 */
function normalizeHttpPath(path: string): string {
	// Handle empty or missing path.
	if (!path || path === "/") return "/";

	const segments = path.split("/");
	return segments.map(normalizeSegment).join("/");
}

/**
 * Count how many segments in a normalized path are wildcards vs literals.
 * Used for confidence adjustment — more literal segments = higher confidence.
 */
function countSegmentTypes(normalizedPath: string): {
	total: number;
	literal: number;
	wildcard: number;
} {
	const segments = normalizedPath.split("/").filter((s) => s.length > 0);
	let wildcard = 0;
	for (const s of segments) {
		if (s === PARAM_WILDCARD) wildcard++;
	}
	return {
		total: segments.length,
		literal: segments.length - wildcard,
		wildcard,
	};
}

export class HttpBoundaryMatchStrategy implements BoundaryMatchStrategy {
	readonly mechanism: BoundaryMechanism = "http";

	computeMatcherKey(
		address: string,
		metadata: Record<string, unknown>,
	): string {
		const method =
			typeof metadata.httpMethod === "string"
				? metadata.httpMethod.toUpperCase()
				: "*";
		const normalizedPath = normalizeHttpPath(address);
		return `${method} ${normalizedPath}`;
	}

	match(
		providers: MatchableProviderFact[],
		consumers: MatchableConsumerFact[],
	): BoundaryLinkCandidate[] {
		if (providers.length === 0 || consumers.length === 0) return [];

		// Build provider index: matcherKey → provider facts.
		const providerIndex = new Map<string, MatchableProviderFact[]>();
		// Also build path-only index for consumers without a known method.
		const providerByPathOnly = new Map<string, MatchableProviderFact[]>();

		for (const p of providers) {
			const key = this.computeMatcherKey(p.address, p.metadata);
			if (!providerIndex.has(key)) providerIndex.set(key, []);
			providerIndex.get(key)!.push(p);

			// Path-only key (strip method prefix).
			const pathOnly = normalizeHttpPath(p.address);
			if (!providerByPathOnly.has(pathOnly)) {
				providerByPathOnly.set(pathOnly, []);
			}
			providerByPathOnly.get(pathOnly)!.push(p);
		}

		const candidates: BoundaryLinkCandidate[] = [];

		for (const c of consumers) {
			const consumerKey = this.computeMatcherKey(c.address, c.metadata);
			const method =
				typeof c.metadata.httpMethod === "string"
					? c.metadata.httpMethod.toUpperCase()
					: null;

			// Strategy 1: exact key match (method + normalized path).
			const exactMatches = providerIndex.get(consumerKey);
			if (exactMatches) {
				for (const p of exactMatches) {
					const segInfo = countSegmentTypes(normalizeHttpPath(c.address));
					candidates.push({
						providerFactUid: p.factUid,
						consumerFactUid: c.factUid,
						matchBasis: "address_match",
						confidence: computeHttpConfidence(c, segInfo),
					});
				}
				continue;
			}

			// Strategy 2: path-only match (consumer has unknown method or
			// method is wildcard "*"). Lower confidence.
			if (!method || method === "*") {
				const pathOnly = normalizeHttpPath(c.address);
				const pathMatches = providerByPathOnly.get(pathOnly);
				if (pathMatches) {
					for (const p of pathMatches) {
						const segInfo = countSegmentTypes(pathOnly);
						candidates.push({
							providerFactUid: p.factUid,
							consumerFactUid: c.factUid,
							matchBasis: "heuristic",
							confidence: computeHttpConfidence(c, segInfo) * 0.7,
						});
					}
				}
			}
		}

		return candidates;
	}
}

/**
 * Compute HTTP match confidence from consumer fact and path structure.
 *
 * Factors:
 *   - Consumer extraction confidence (literal > template, axios > fetch)
 *   - Path specificity: more literal segments = more discriminating match
 *   - All-literal paths get a slight boost (no wildcard ambiguity)
 *
 * This is a PROTOTYPE scoring model. Not calibrated against real-world
 * false-positive rates. Sufficient for model validation.
 */
function computeHttpConfidence(
	consumer: BoundaryConsumerFact,
	segInfo: { total: number; literal: number; wildcard: number },
): number {
	// Base: consumer's extraction confidence (0.7 - 0.95).
	let conf = consumer.confidence;

	// Path specificity factor:
	// Paths with more literal segments are more discriminating.
	// Single-segment paths (/health) are common but ambiguous.
	// Multi-segment API paths (/api/v2/products/{_}) are specific.
	if (segInfo.total <= 1) {
		conf *= 0.8; // Short paths are less discriminating.
	} else if (segInfo.wildcard === 0) {
		conf *= 1.0; // All-literal path — exact structural match.
	} else {
		// Mixed literal + wildcard: slight penalty per wildcard.
		const wildcardRatio = segInfo.wildcard / segInfo.total;
		conf *= 1.0 - wildcardRatio * 0.1;
	}

	// Clamp to [0, 1].
	return Math.max(0, Math.min(1, conf));
}

// ── Orchestrator ────────────────────────────────────────────────────

/** Default strategy registry. Extend as new mechanisms are added. */
const DEFAULT_STRATEGIES: BoundaryMatchStrategy[] = [
	new HttpBoundaryMatchStrategy(),
];

/**
 * Match boundary provider facts against consumer facts.
 *
 * Groups facts by mechanism, dispatches to the appropriate strategy,
 * and collects all link candidates. Facts with no registered strategy
 * are silently skipped (no match possible until a strategy is added).
 *
 * @param providers - All provider facts to match (must carry factUid). May span mechanisms.
 * @param consumers - All consumer facts to match (must carry factUid). May span mechanisms.
 * @param strategies - Override strategy registry (for testing). Defaults to all built-in strategies.
 */
export function matchBoundaryFacts(
	providers: MatchableProviderFact[],
	consumers: MatchableConsumerFact[],
	strategies: BoundaryMatchStrategy[] = DEFAULT_STRATEGIES,
): BoundaryLinkCandidate[] {
	// Build mechanism → strategy lookup.
	const strategyMap = new Map<BoundaryMechanism, BoundaryMatchStrategy>();
	for (const s of strategies) {
		strategyMap.set(s.mechanism, s);
	}

	// Group providers and consumers by mechanism.
	const providersByMechanism = new Map<BoundaryMechanism, MatchableProviderFact[]>();
	const consumersByMechanism = new Map<BoundaryMechanism, MatchableConsumerFact[]>();

	for (const p of providers) {
		const mech = p.mechanism;
		if (!providersByMechanism.has(mech)) providersByMechanism.set(mech, []);
		providersByMechanism.get(mech)!.push(p);
	}
	for (const c of consumers) {
		const mech = c.mechanism;
		if (!consumersByMechanism.has(mech)) consumersByMechanism.set(mech, []);
		consumersByMechanism.get(mech)!.push(c);
	}

	// Match within each mechanism.
	const allCandidates: BoundaryLinkCandidate[] = [];

	for (const [mech, strategy] of strategyMap) {
		const mechProviders = providersByMechanism.get(mech);
		const mechConsumers = consumersByMechanism.get(mech);
		if (!mechProviders || !mechConsumers) continue;

		const candidates = strategy.match(mechProviders, mechConsumers);
		allCandidates.push(...candidates);
	}

	return allCandidates;
}

// ── Exports for indexer use ─────────────────────────────────────────

/**
 * Get the match strategy for a mechanism.
 * Returns null if no strategy is registered.
 * Used by the indexer to compute matcher_key at persist time.
 */
export function getMatchStrategy(
	mechanism: BoundaryMechanism,
	strategies: BoundaryMatchStrategy[] = DEFAULT_STRATEGIES,
): BoundaryMatchStrategy | null {
	return strategies.find((s) => s.mechanism === mechanism) ?? null;
}
