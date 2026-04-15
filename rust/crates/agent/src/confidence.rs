//! Confidence derivation for the orient envelope.
//!
//! Confidence is a single field (`high` / `medium` / `low`) that
//! tells the agent how much to trust the rest of the response.
//! It is derived from raw trust data, not from the emitted
//! signals: lossy signal truncation must not feed back into
//! confidence.
//!
//! ── Rules (Rust-42, repo focus) ──────────────────────────────
//!
//! Inputs:
//!   - call_resolution_rate: float in [0.0, 1.0]
//!   - stale: bool — `true` iff `get_stale_files` returned a
//!     non-empty list
//!   - enrichment_applied: bool
//!
//! Output tiers:
//!
//!   - high    — resolution rate > 0.50 AND not stale
//!   - medium  — resolution rate in [0.20, 0.50] OR (stale AND
//!               resolution rate > 0.20) OR (high-rate but
//!               enrichment absent)
//!   - low     — resolution rate < 0.20
//!
//! These rules directly encode the table in the agent
//! orientation contract. The order of checks matters: low
//! beats medium beats high, so we short-circuit from the worst
//! case upward.
//!
//! NOTE: focus-scoped "unresolved pressure" mentioned in the
//! contract is a module/symbol-focus concern and does not apply
//! to repo-level orient in Rust-42.

use crate::dto::envelope::Confidence;
use crate::storage_port::AgentTrustSummary;

pub fn derive_repo_confidence(
	trust: &AgentTrustSummary,
	stale: bool,
) -> Confidence {
	let rate = trust.call_resolution_rate;

	if rate < 0.20 {
		return Confidence::Low;
	}

	if rate <= 0.50 {
		return Confidence::Medium;
	}

	// Rate > 0.50 — potentially high, but degrade on stale or
	// absent enrichment.
	if stale {
		return Confidence::Medium;
	}

	if !trust.enrichment_applied && trust.enrichment_eligible > 0 {
		return Confidence::Medium;
	}

	Confidence::High
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ts(rate: f64, enriched: bool, eligible: u64) -> AgentTrustSummary {
		AgentTrustSummary {
			call_resolution_rate: rate,
			resolved_calls: 0,
			unresolved_calls: 0,
			enrichment_applied: enriched,
			enrichment_eligible: eligible,
			enrichment_enriched: if enriched { 1 } else { 0 },
		}
	}

	#[test]
	fn low_below_20_percent() {
		assert_eq!(
			derive_repo_confidence(&ts(0.10, true, 10), false),
			Confidence::Low
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.19, true, 10), false),
			Confidence::Low
		);
	}

	#[test]
	fn medium_in_20_to_50_percent_band() {
		assert_eq!(
			derive_repo_confidence(&ts(0.20, true, 10), false),
			Confidence::Medium
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.50, true, 10), false),
			Confidence::Medium
		);
	}

	#[test]
	fn high_above_50_percent_clean() {
		assert_eq!(
			derive_repo_confidence(&ts(0.55, true, 10), false),
			Confidence::High
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.99, true, 10), false),
			Confidence::High
		);
	}

	#[test]
	fn high_rate_but_stale_degrades_to_medium() {
		assert_eq!(
			derive_repo_confidence(&ts(0.80, true, 10), true),
			Confidence::Medium
		);
	}

	#[test]
	fn high_rate_but_no_enrichment_degrades_to_medium() {
		assert_eq!(
			derive_repo_confidence(&ts(0.80, false, 10), false),
			Confidence::Medium
		);
	}

	#[test]
	fn high_rate_with_zero_enrichment_eligible_stays_high() {
		// No eligible edges means enrichment is not applicable;
		// absence is not a penalty in that case.
		assert_eq!(
			derive_repo_confidence(&ts(0.80, false, 0), false),
			Confidence::High
		);
	}
}
