//! Enumerated limit codes surfaced by the agent orientation
//! surface.
//!
//! A limit record appears in the output when the use case knows
//! it cannot compute something a signal code would normally
//! cover. Every limit code carries a stable identifier and a
//! human-readable summary. The `summary` field is convenience;
//! the `code` field is the contract.
//!
//! Rules for adding a new limit code:
//!
//!   1. Every limit code is an explicit variant of `LimitCode`.
//!      No free-form strings.
//!   2. Every limit code has a stable wire-format string in
//!      `as_str()`. The string is the only stable surface.
//!   3. Limits are NOT a dumping ground for debug output. A
//!      limit code means "a specific capability is unavailable
//!      in this response". It is a product statement, not a log
//!      line.
//!
//! Rust-42 scope: only the limit codes the repo-level orient
//! pipeline can actually emit are listed here. The contract
//! reserves additional codes (e.g. `IMPORTS_ONE_HOP` for the
//! `imports` surface) for future commands.

use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitCode {
	/// The repo has no active requirement declarations, so the
	/// gate pipeline has nothing to evaluate. This is an
	/// absence-of-configured-policy state, NOT a gate result.
	/// It does not become a `GATE_PASS` (which would imply
	/// obligations existed and all passed). It is a limit so
	/// the agent can distinguish "gate not configured" from
	/// "gate configured and passing".
	///
	/// Replaces the `GATE_UNAVAILABLE` code used during Rust-42,
	/// which existed only because gate policy was trapped in the
	/// `rgr` binary crate and could not be called from the agent
	/// layer. After Rust-43A relocated gate into
	/// `repo-graph-gate`, gate evaluation IS available — the
	/// relevant "unavailable" state shifted from tooling to
	/// policy configuration.
	GateNotConfigured,

	/// Module discovery data (Layer-1 discovered modules catalog)
	/// is not yet queryable through the Rust storage path. The
	/// repo-level `MODULE_SUMMARY` signal falls back to raw
	/// snapshot totals instead of discovered module counts.
	ModuleDataUnavailable,

	/// Cyclomatic complexity measurements are not produced by the
	/// Rust indexer. `HIGH_COMPLEXITY` is therefore never emitted
	/// from a Rust-indexed repo.
	ComplexityUnavailable,

	/// The `DEAD_CODE` signal was suppressed because the trust
	/// layer's `reliability.dead_code` axis is not High. This
	/// happens when extraction quality is insufficient for
	/// deletion decisions — typically because the call graph
	/// has too many unresolved edges, or framework-liveness
	/// inferences and entrypoint declarations are missing.
	///
	/// The reasons vector on the emitted `Limit` carries the
	/// trust crate's own reason strings verbatim (e.g.
	/// `"missing_entrypoint_declarations"`,
	/// `"call_graph_reliability_low"`). The agent crate does
	/// NOT synthesize reason vocabulary.
	///
	/// Introduced in the Rust-43 F1/F3 fix slice after the
	/// spike on this repo showed 86% of symbols incorrectly
	/// reported as dead. See
	/// `docs/spikes/2026-04-15-orient-on-repo-graph.md`.
	DeadCodeUnreliable,
}

impl LimitCode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::GateNotConfigured => "GATE_NOT_CONFIGURED",
			Self::ModuleDataUnavailable => "MODULE_DATA_UNAVAILABLE",
			Self::ComplexityUnavailable => "COMPLEXITY_UNAVAILABLE",
			Self::DeadCodeUnreliable => "DEAD_CODE_UNRELIABLE",
		}
	}

	/// Canonical summary string that accompanies this code in
	/// the output envelope. Stable wording — changing this is a
	/// contract change.
	pub fn summary(self) -> &'static str {
		match self {
			Self::GateNotConfigured => {
				"No active requirement declarations. Gate has no \
				 obligations to evaluate."
			}
			Self::ModuleDataUnavailable => {
				"Module discovery data is not queryable through the Rust \
				 storage path. Repo-level counts fall back to raw \
				 snapshot totals."
			}
			Self::ComplexityUnavailable => {
				"Cyclomatic complexity measurements are not produced by \
				 the Rust indexer. HIGH_COMPLEXITY cannot be emitted."
			}
			Self::DeadCodeUnreliable => {
				"Dead-code signal suppressed: trust layer reports \
				 dead_code_reliability is not High. The underlying \
				 graph does not have enough entrypoint declarations, \
				 framework-liveness inferences, or resolved call \
				 edges to make deletion decisions reliable. See the \
				 accompanying reasons list for the specific trust \
				 axis that failed."
			}
		}
	}
}

impl Serialize for LimitCode {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

/// One limit record in the output envelope.
///
/// Shape rules:
///
///   - `code` is a stable enumerated identifier.
///   - `summary` is looked up from the code — callers cannot
///     supply their own text. This keeps the per-code wording
///     as a single-site contract.
///   - `reasons` is a free-form list of human-readable strings
///     describing WHY the limit fired. Most limits have no
///     reasons and serialize without the field. Limits
///     triggered by an upstream policy layer (notably
///     `DEAD_CODE_UNRELIABLE`, which surfaces the trust
///     crate's `reliability.dead_code.reasons` verbatim) carry
///     the reasons through to the output envelope so an agent
///     can display or match on them.
///
/// The `reasons` field is skipped during serialization when
/// empty, preserving the pre-Rust-43-fix output shape for every
/// limit that does not use it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limit {
	pub code: LimitCode,
	pub summary: &'static str,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub reasons: Vec<String>,
}

impl Limit {
	/// Construct a limit record from a code. The summary is
	/// looked up by the code — callers cannot supply their own
	/// summary string, which is how the contract stays stable.
	///
	/// No reasons attached. Use
	/// `from_code_with_reasons` when the limit needs to carry
	/// upstream diagnostic strings.
	pub fn from_code(code: LimitCode) -> Self {
		Self {
			code,
			summary: code.summary(),
			reasons: Vec::new(),
		}
	}

	/// Construct a limit record from a code with an attached
	/// reasons list. Reasons are passed through verbatim — the
	/// caller is responsible for the vocabulary. This is how
	/// `DEAD_CODE_UNRELIABLE` surfaces the trust crate's
	/// `reliability.dead_code.reasons` to the output envelope.
	pub fn from_code_with_reasons(
		code: LimitCode,
		reasons: Vec<String>,
	) -> Self {
		Self {
			code,
			summary: code.summary(),
			reasons,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn limit_code_serializes_as_screaming_snake() {
		let s = serde_json::to_string(&LimitCode::GateNotConfigured).unwrap();
		assert_eq!(s, "\"GATE_NOT_CONFIGURED\"");
	}

	#[test]
	fn limit_carries_canonical_summary() {
		let l = Limit::from_code(LimitCode::GateNotConfigured);
		assert_eq!(l.code, LimitCode::GateNotConfigured);
		assert!(l.summary.contains("requirement declarations"));
	}

	#[test]
	fn limit_serializes_with_code_and_summary() {
		let l = Limit::from_code(LimitCode::ComplexityUnavailable);
		let s = serde_json::to_string(&l).unwrap();
		assert!(s.contains("\"code\":\"COMPLEXITY_UNAVAILABLE\""));
		assert!(s.contains("\"summary\":"));
	}
}
