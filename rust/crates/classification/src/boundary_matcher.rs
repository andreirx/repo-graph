//! Boundary matcher — strategy-dispatched provider/consumer pairing.
//!
//! Mirror of `src/core/classification/boundary-matcher.ts`.
//!
//! Pure core business logic. No I/O, no storage, no framework
//! awareness. The orchestrator dispatches by mechanism to the
//! correct match logic.
//!
//! Architecture: the TS version uses a class-per-mechanism strategy
//! pattern (BoundaryMatchStrategy interface + HttpBoundaryMatchStrategy
//! + CliBoundaryMatchStrategy classes). The Rust port uses a simpler
//! enum-dispatch via `match` on `BoundaryMechanism` — more idiomatic
//! for a small fixed set of strategies and avoids trait objects.
//!
//! Maturity: PROTOTYPE. Same limitations as the TS version:
//! no query parameter matching, no route prefix/wildcard segment
//! matching, no cross-mechanism matching, confidence scoring is
//! a simple product (not calibrated).

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::types::{
	BoundaryLinkCandidate, BoundaryMatchBasis, BoundaryMechanism,
	MatchableConsumerFact, MatchableProviderFact,
};

/// Compute the normalized matcher key for a boundary fact.
///
/// Used at two sites:
///   - Persist time (indexer computes key for the DB column)
///   - Match time (strategy normalizes before comparing)
///
/// Returns `None` if the mechanism has no registered strategy.
///
/// Mirror of the TS `getMatchStrategy(mechanism)?.computeMatcherKey(address, metadata)`
/// pattern from `boundary-matcher.ts:432`.
///
/// This function is re-exported as `pub` from the crate root.
pub fn compute_matcher_key(
	mechanism: BoundaryMechanism,
	address: &str,
	metadata: &Map<String, Value>,
) -> Option<String> {
	match mechanism {
		BoundaryMechanism::Http => Some(http_compute_matcher_key(address, metadata)),
		BoundaryMechanism::CliCommand => Some(cli_compute_matcher_key(address)),
		_ => None,
	}
}

/// Match boundary provider facts against consumer facts.
///
/// Groups facts by mechanism, dispatches to the appropriate
/// match logic, and collects all link candidates. Facts with no
/// registered strategy are silently skipped.
///
/// Mirror of `matchBoundaryFacts` from `boundary-matcher.ts:384`.
///
/// This function is re-exported as `pub` from the crate root.
pub fn match_boundary_facts(
	providers: &[MatchableProviderFact],
	consumers: &[MatchableConsumerFact],
) -> Vec<BoundaryLinkCandidate> {
	// Group by mechanism.
	let mut providers_by_mech: HashMap<BoundaryMechanism, Vec<&MatchableProviderFact>> =
		HashMap::new();
	let mut consumers_by_mech: HashMap<BoundaryMechanism, Vec<&MatchableConsumerFact>> =
		HashMap::new();
	for p in providers {
		providers_by_mech
			.entry(p.fact.mechanism)
			.or_default()
			.push(p);
	}
	for c in consumers {
		consumers_by_mech
			.entry(c.fact.mechanism)
			.or_default()
			.push(c);
	}

	let mut all_candidates = Vec::new();

	// Dispatch per mechanism.
	for mech in [BoundaryMechanism::Http, BoundaryMechanism::CliCommand] {
		let Some(mech_providers) = providers_by_mech.get(&mech) else {
			continue;
		};
		let Some(mech_consumers) = consumers_by_mech.get(&mech) else {
			continue;
		};
		let candidates = match mech {
			BoundaryMechanism::Http => {
				http_match(mech_providers, mech_consumers)
			}
			BoundaryMechanism::CliCommand => {
				cli_match(mech_providers, mech_consumers)
			}
			_ => Vec::new(),
		};
		all_candidates.extend(candidates);
	}

	all_candidates
}

// ── HTTP match strategy ──────────────────────────────────────────

const PARAM_WILDCARD: &str = "{_}";

/// Normalize a single path segment. `{id}`, `{param}` → `{_}`;
/// `:id`, `:productId` → `{_}`; everything else → literal.
fn normalize_segment(segment: &str) -> &str {
	if segment.starts_with('{') && segment.ends_with('}') {
		return PARAM_WILDCARD;
	}
	if segment.starts_with(':') && segment.len() > 1 {
		return PARAM_WILDCARD;
	}
	segment
}

/// Normalize an HTTP path into a canonical segment representation.
fn normalize_http_path(path: &str) -> String {
	if path.is_empty() || path == "/" {
		return "/".to_string();
	}
	path.split('/')
		.map(normalize_segment)
		.collect::<Vec<_>>()
		.join("/")
}

struct SegmentInfo {
	total: usize,
	#[allow(dead_code)]
	literal: usize,
	wildcard: usize,
}

fn count_segment_types(normalized_path: &str) -> SegmentInfo {
	let segments: Vec<&str> = normalized_path
		.split('/')
		.filter(|s| !s.is_empty())
		.collect();
	let wildcard = segments.iter().filter(|&&s| s == PARAM_WILDCARD).count();
	SegmentInfo {
		total: segments.len(),
		literal: segments.len() - wildcard,
		wildcard,
	}
}

fn http_compute_matcher_key(address: &str, metadata: &Map<String, Value>) -> String {
	let method = metadata
		.get("httpMethod")
		.and_then(|v| v.as_str())
		.map(|m| m.to_uppercase())
		.unwrap_or_else(|| "*".to_string());
	let normalized_path = normalize_http_path(address);
	format!("{} {}", method, normalized_path)
}

fn http_match(
	providers: &[&MatchableProviderFact],
	consumers: &[&MatchableConsumerFact],
) -> Vec<BoundaryLinkCandidate> {
	if providers.is_empty() || consumers.is_empty() {
		return Vec::new();
	}

	// Build provider index: matcher_key → provider facts.
	let mut provider_index: HashMap<String, Vec<&MatchableProviderFact>> = HashMap::new();
	let mut provider_by_path_only: HashMap<String, Vec<&MatchableProviderFact>> =
		HashMap::new();

	for p in providers {
		let key = http_compute_matcher_key(&p.fact.address, &p.fact.metadata);
		provider_index.entry(key).or_default().push(p);

		let path_only = normalize_http_path(&p.fact.address);
		provider_by_path_only.entry(path_only).or_default().push(p);
	}

	let mut candidates = Vec::new();

	for c in consumers {
		let consumer_key = http_compute_matcher_key(&c.fact.address, &c.fact.metadata);
		let method = c
			.fact
			.metadata
			.get("httpMethod")
			.and_then(|v| v.as_str())
			.map(|m| m.to_uppercase());

		// Strategy 1: exact key match.
		if let Some(exact_matches) = provider_index.get(&consumer_key) {
			let seg_info = count_segment_types(&normalize_http_path(&c.fact.address));
			for p in exact_matches {
				candidates.push(BoundaryLinkCandidate {
					provider_fact_uid: p.fact_uid.clone(),
					consumer_fact_uid: c.fact_uid.clone(),
					match_basis: BoundaryMatchBasis::AddressMatch,
					confidence: compute_http_confidence(c.fact.confidence, &seg_info),
				});
			}
			continue;
		}

		// Strategy 2: path-only match (consumer has unknown method).
		match method.as_deref() {
			None | Some("*") => {
				let path_only = normalize_http_path(&c.fact.address);
				if let Some(path_matches) = provider_by_path_only.get(&path_only) {
					let seg_info = count_segment_types(&path_only);
					for p in path_matches {
						candidates.push(BoundaryLinkCandidate {
							provider_fact_uid: p.fact_uid.clone(),
							consumer_fact_uid: c.fact_uid.clone(),
							match_basis: BoundaryMatchBasis::Heuristic,
							confidence: compute_http_confidence(
								c.fact.confidence,
								&seg_info,
							) * 0.7,
						});
					}
				}
			}
			_ => {}
		}
	}

	candidates
}

fn compute_http_confidence(consumer_confidence: f64, seg_info: &SegmentInfo) -> f64 {
	let mut conf = consumer_confidence;
	if seg_info.total <= 1 {
		conf *= 0.8;
	} else if seg_info.wildcard == 0 {
		conf *= 1.0;
	} else {
		let wildcard_ratio = seg_info.wildcard as f64 / seg_info.total as f64;
		conf *= 1.0 - wildcard_ratio * 0.1;
	}
	conf.clamp(0.0, 1.0)
}

// ── CLI command match strategy ───────────────────────────────────

fn cli_compute_matcher_key(address: &str) -> String {
	address.to_lowercase().trim().to_string()
}

fn cli_match(
	providers: &[&MatchableProviderFact],
	consumers: &[&MatchableConsumerFact],
) -> Vec<BoundaryLinkCandidate> {
	if providers.is_empty() || consumers.is_empty() {
		return Vec::new();
	}

	let mut provider_index: HashMap<String, Vec<&MatchableProviderFact>> = HashMap::new();
	for p in providers {
		let key = cli_compute_matcher_key(&p.fact.address);
		provider_index.entry(key).or_default().push(p);
	}

	let mut candidates = Vec::new();

	for c in consumers {
		let consumer_key = cli_compute_matcher_key(&c.fact.address);

		// Strategy 1: exact command path match.
		if let Some(exact_matches) = provider_index.get(&consumer_key) {
			for p in exact_matches {
				candidates.push(BoundaryLinkCandidate {
					provider_fact_uid: p.fact_uid.clone(),
					consumer_fact_uid: c.fact_uid.clone(),
					match_basis: BoundaryMatchBasis::OperationMatch,
					confidence: c.fact.confidence * 0.9,
				});
			}
			continue;
		}

		// Strategy 2: binary-prefix strip.
		// Consumer paths from scripts include the binary name
		// ("rgr repo add"), but providers register only the
		// subcommand path ("repo add"). Guard: stripped remainder
		// must have >= 2 tokens to avoid ambiguous single-word
		// matches.
		let tokens: Vec<&str> = consumer_key.split_whitespace().collect();
		if tokens.len() >= 3 {
			let without_binary = tokens[1..].join(" ");
			if let Some(prefix_matches) = provider_index.get(&without_binary) {
				for p in prefix_matches {
					candidates.push(BoundaryLinkCandidate {
						provider_fact_uid: p.fact_uid.clone(),
						consumer_fact_uid: c.fact_uid.clone(),
						match_basis: BoundaryMatchBasis::Heuristic,
						confidence: c.fact.confidence * 0.75,
					});
				}
			}
		}
	}

	candidates
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::{
		BoundaryConsumerBasis, BoundaryConsumerFact, BoundaryProviderBasis,
		BoundaryProviderFact,
	};
	use serde_json::json;

	fn make_provider(
		fact_uid: &str,
		mechanism: BoundaryMechanism,
		address: &str,
		metadata: Map<String, Value>,
	) -> MatchableProviderFact {
		MatchableProviderFact {
			fact_uid: fact_uid.to_string(),
			fact: BoundaryProviderFact {
				mechanism,
				operation: format!("GET {}", address),
				address: address.to_string(),
				handler_stable_key: "handler".to_string(),
				source_file: "src/app.ts".to_string(),
				line_start: 10,
				framework: "express".to_string(),
				basis: BoundaryProviderBasis::Registration,
				schema_ref: None,
				metadata,
			},
		}
	}

	fn make_consumer(
		fact_uid: &str,
		mechanism: BoundaryMechanism,
		address: &str,
		confidence: f64,
		metadata: Map<String, Value>,
	) -> MatchableConsumerFact {
		MatchableConsumerFact {
			fact_uid: fact_uid.to_string(),
			fact: BoundaryConsumerFact {
				mechanism,
				operation: format!("GET {}", address),
				address: address.to_string(),
				caller_stable_key: "caller".to_string(),
				source_file: "src/client.ts".to_string(),
				line_start: 20,
				basis: BoundaryConsumerBasis::Literal,
				confidence,
				schema_ref: None,
				metadata,
			},
		}
	}

	fn http_meta(method: &str) -> Map<String, Value> {
		let mut m = Map::new();
		m.insert("httpMethod".to_string(), json!(method));
		m
	}

	// ── computeMatcherKey (HTTP) ─────────────────────────────

	#[test]
	fn http_key_normalizes_spring_param() {
		let key = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api/orders/{id}",
			&http_meta("GET"),
		);
		assert_eq!(key.unwrap(), "GET /api/orders/{_}");
	}

	#[test]
	fn http_key_normalizes_express_param() {
		let key = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api/orders/:id",
			&http_meta("POST"),
		);
		assert_eq!(key.unwrap(), "POST /api/orders/{_}");
	}

	#[test]
	fn http_key_preserves_literal_segments() {
		let key = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api/v2/products",
			&http_meta("GET"),
		);
		assert_eq!(key.unwrap(), "GET /api/v2/products");
	}

	#[test]
	fn http_key_uses_star_for_unknown_method() {
		let key = compute_matcher_key(
			BoundaryMechanism::Http,
			"/health",
			&Map::new(),
		);
		assert_eq!(key.unwrap(), "* /health");
	}

	#[test]
	fn http_key_uppercases_method() {
		let key = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api",
			&http_meta("get"),
		);
		assert_eq!(key.unwrap(), "GET /api");
	}

	#[test]
	fn spring_and_express_params_produce_same_key() {
		let spring = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api/v2/products/{id}",
			&http_meta("GET"),
		);
		let express = compute_matcher_key(
			BoundaryMechanism::Http,
			"/api/v2/products/:productId",
			&http_meta("GET"),
		);
		assert_eq!(spring, express);
	}

	// ── matchBoundaryFacts (HTTP) ────────────────────────────

	#[test]
	fn http_matches_provider_param_to_consumer_param() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::Http,
			"/api/orders/{id}",
			http_meta("GET"),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::Http,
			"/api/orders/{param}",
			0.9,
			http_meta("GET"),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].provider_fact_uid, "p1");
		assert_eq!(links[0].consumer_fact_uid, "c1");
		assert_eq!(links[0].match_basis, BoundaryMatchBasis::AddressMatch);
	}

	#[test]
	fn http_does_not_match_different_methods() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::Http,
			"/api/orders",
			http_meta("GET"),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::Http,
			"/api/orders",
			0.9,
			http_meta("POST"),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert!(links.is_empty());
	}

	#[test]
	fn http_does_not_match_across_mechanisms() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::Http,
			"/api/orders",
			http_meta("GET"),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::Grpc,
			"/api/orders",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert!(links.is_empty());
	}

	#[test]
	fn http_path_only_fallback_for_unknown_method() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::Http,
			"/api/orders",
			http_meta("GET"),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::Http,
			"/api/orders",
			0.9,
			Map::new(), // no httpMethod → path-only fallback
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].match_basis, BoundaryMatchBasis::Heuristic);
		assert!(
			links[0].confidence < 0.9 * 0.8, // heuristic penalty
			"path-only confidence must be lower than exact"
		);
	}

	// ── Confidence ───────────────────────────────────────────

	#[test]
	fn literal_consumer_has_higher_confidence_than_wildcard() {
		let seg_literal = count_segment_types("/api/v2/products");
		let seg_wildcard = count_segment_types("/api/v2/products/{_}");
		let conf_literal = compute_http_confidence(0.9, &seg_literal);
		let conf_wildcard = compute_http_confidence(0.9, &seg_wildcard);
		assert!(conf_literal > conf_wildcard);
	}

	#[test]
	fn short_path_has_lower_confidence() {
		let seg_short = count_segment_types("/health");
		let seg_long = count_segment_types("/api/v2/products");
		let conf_short = compute_http_confidence(0.9, &seg_short);
		let conf_long = compute_http_confidence(0.9, &seg_long);
		assert!(conf_short < conf_long);
	}

	// ── computeMatcherKey (CLI) ──────────────────────────────

	#[test]
	fn cli_key_lowercases_and_trims() {
		let key = compute_matcher_key(
			BoundaryMechanism::CliCommand,
			"  Repo Add  ",
			&Map::new(),
		);
		assert_eq!(key.unwrap(), "repo add");
	}

	// ── matchBoundaryFacts (CLI) ─────────────────────────────

	#[test]
	fn cli_exact_match_produces_operation_match_basis() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::CliCommand,
			"repo add",
			Map::new(),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::CliCommand,
			"repo add",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].match_basis, BoundaryMatchBasis::OperationMatch);
	}

	#[test]
	fn cli_binary_prefix_strip_matches_with_heuristic_basis() {
		// Consumer "rgr repo add" → strip binary "rgr" → "repo add"
		// matches provider "repo add". Guard: 3 tokens ≥ 3, so the
		// stripped remainder ("repo add") has 2 tokens which is OK.
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::CliCommand,
			"repo add",
			Map::new(),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::CliCommand,
			"rgr repo add",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].match_basis, BoundaryMatchBasis::Heuristic);
		assert!(
			links[0].confidence < 0.9 * 0.9,
			"binary-prefix heuristic must have lower confidence than exact"
		);
	}

	#[test]
	fn cli_binary_prefix_strip_requires_at_least_3_tokens() {
		// Consumer "cargo test" has only 2 tokens. Stripping the
		// binary ("cargo") leaves a single token ("test"), which
		// is too ambiguous. The guard (tokens.len() >= 3) prevents
		// false positives where "cargo test" would match a provider
		// named just "test".
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::CliCommand,
			"test",
			Map::new(),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::CliCommand,
			"cargo test",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert!(
			links.is_empty(),
			"2-token consumer must NOT binary-prefix-strip to a single-word provider"
		);
	}

	#[test]
	fn cli_exact_match_is_case_insensitive() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::CliCommand,
			"Repo Add",
			Map::new(),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::CliCommand,
			"repo add",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert_eq!(links.len(), 1, "CLI matching should be case-insensitive");
	}

	#[test]
	fn cli_no_match_for_different_commands() {
		let providers = vec![make_provider(
			"p1",
			BoundaryMechanism::CliCommand,
			"repo add",
			Map::new(),
		)];
		let consumers = vec![make_consumer(
			"c1",
			BoundaryMechanism::CliCommand,
			"repo remove",
			0.9,
			Map::new(),
		)];
		let links = match_boundary_facts(&providers, &consumers);
		assert!(links.is_empty());
	}

	// ── Unknown mechanism ────────────────────────────────────

	#[test]
	fn unknown_mechanism_returns_none_for_matcher_key() {
		let key = compute_matcher_key(
			BoundaryMechanism::Grpc,
			"/some.Service/Method",
			&Map::new(),
		);
		assert!(key.is_none());
	}
}
