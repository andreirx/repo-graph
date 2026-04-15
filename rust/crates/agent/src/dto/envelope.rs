//! Output envelope for `orient`, `check`, and `explain`.
//!
//! Only the `orient` shape is populated in Rust-42. `check` and
//! `explain` re-use the same envelope; their aggregator pipelines
//! ship in later slices.
//!
//! ── Truncation discipline ────────────────────────────────────
//!
//! Every list section (`signals`, `limits`, `next`) has three
//! associated fields:
//!
//!   - the list itself
//!   - `{section}_truncated: Option<bool>`
//!   - `{section}_omitted_count: Option<usize>`
//!
//! The truncation fields are `Option` and emitted via
//! `skip_serializing_if` so a non-truncated response has only
//! the list. A truncated section carries both fields. The
//! top-level `truncated` boolean is true iff any section was
//! truncated.
//!
//! This matches the JSON shape in
//! `docs/architecture/agent-orientation-contract.md`.

use serde::Serialize;

use crate::dto::limit::Limit;
use crate::dto::signal::Signal;

// ── Focus ─────────────────────────────────────────────────────────

/// What kind of entity the focus resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedKind {
	Repo,
	Module,
	File,
	Symbol,
}

/// Reason a focus failed to resolve. Used only for real domain
/// failures (no match, ambiguous). Scope limitations such as
/// "module focus not shipped yet" are `OrientError`, not
/// `FocusFailureReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusFailureReason {
	NoMatch,
	Ambiguous,
}

/// One candidate when focus resolution is ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusCandidate {
	pub stable_key: String,
	pub file: Option<String>,
	pub kind: ResolvedKind,
}

/// Focus resolution outcome.
///
/// The structure matches the contract shape exactly. At
/// repo-level (Rust-42), every successful response carries
/// `input: None`, `resolved: true`, `resolved_kind: Some(Repo)`,
/// `resolved_key: Some(<repo_uid>)`, and empty `candidates`.
///
/// Module / file / symbol focus resolution ships in Rust-44+
/// and will populate these fields with real data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Focus {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub input: Option<String>,
	pub resolved: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resolved_kind: Option<ResolvedKind>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resolved_key: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reason: Option<FocusFailureReason>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub candidates: Vec<FocusCandidate>,
}

impl Focus {
	/// Build a repo-level focus record for a successfully
	/// loaded repo.
	pub fn repo(repo_uid: &str) -> Self {
		Self {
			input: None,
			resolved: true,
			resolved_kind: Some(ResolvedKind::Repo),
			resolved_key: Some(repo_uid.to_string()),
			reason: None,
			candidates: Vec::new(),
		}
	}
}

// ── Confidence ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
	High,
	Medium,
	Low,
}

// ── Next action ───────────────────────────────────────────────────

/// One structured follow-up action. Rust-42 does not emit any
/// next actions at repo-level; this type is declared so the
/// envelope is complete and future slices can extend it without
/// changing the top-level shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NextKind {
	Orient,
	Check,
	Explain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextAction {
	pub kind: NextKind,
	pub repo: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub target: Option<String>,
	pub reason: String,
}

// ── Envelope ──────────────────────────────────────────────────────

/// Top-level `orient` result envelope.
///
/// `schema` and `command` are compile-time constants so there is
/// no risk of a typo drifting out of sync with the contract.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrientResult {
	pub schema: &'static str,
	pub command: &'static str,
	pub repo: String,
	pub snapshot: String,
	pub focus: Focus,
	pub confidence: Confidence,

	pub signals: Vec<Signal>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub signals_truncated: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub signals_omitted_count: Option<usize>,

	pub limits: Vec<Limit>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub limits_truncated: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub limits_omitted_count: Option<usize>,

	pub next: Vec<NextAction>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub next_truncated: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub next_omitted_count: Option<usize>,

	pub truncated: bool,
}

/// Stable schema identifier. Incrementing this is a contract
/// change; every consumer must be updated in lockstep.
pub const ORIENT_SCHEMA: &str = "rgr.agent.v1";

/// Stable command identifier for `orient`.
pub const ORIENT_COMMAND: &str = "orient";

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn repo_focus_serializes_without_input() {
		let f = Focus::repo("repo-uid-1");
		let s = serde_json::to_string(&f).unwrap();
		assert!(!s.contains("\"input\""), "input must be omitted at repo-level: {}", s);
		assert!(s.contains("\"resolved\":true"));
		assert!(s.contains("\"resolved_kind\":\"repo\""));
		assert!(s.contains("\"resolved_key\":\"repo-uid-1\""));
		assert!(!s.contains("\"candidates\""), "empty candidates must be omitted: {}", s);
	}

	#[test]
	fn confidence_serializes_lowercase() {
		assert_eq!(serde_json::to_string(&Confidence::High).unwrap(), "\"high\"");
		assert_eq!(serde_json::to_string(&Confidence::Medium).unwrap(), "\"medium\"");
		assert_eq!(serde_json::to_string(&Confidence::Low).unwrap(), "\"low\"");
	}

	#[test]
	fn schema_constants_are_locked() {
		assert_eq!(ORIENT_SCHEMA, "rgr.agent.v1");
		assert_eq!(ORIENT_COMMAND, "orient");
	}
}
