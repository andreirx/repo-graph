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
/// ── Identity fields ─────────────────────────────────────────
///
/// Two identity fields serve different purposes:
///
///   - `resolved_key: Option<String>` — a graph-node stable
///     key when the focus resolved to an exact FILE or MODULE
///     node. `None` when the focus resolved to a path area
///     without an exact MODULE node. Consumers MUST NOT treat
///     this field as always-present: path-area focus is a valid
///     resolution state with no backing graph node.
///
///   - `resolved_path: Option<String>` — the normalized
///     repo-relative path for the resolved focus. Always
///     present for FILE and MODULE/path-area resolution.
///     Absent for repo-level focus (no path) and for
///     unresolved focus. This is the canonical path string
///     that downstream commands (explain, check, daemon
///     transport) should use to re-issue a focus query.
///
/// ── History ─────────────────────────────────────────────────
///
/// Rust-42 shipped with `resolved_key` only, carrying the
/// `repo_uid` for repo-level focus. The Rust-44 design review
/// identified that `resolved_key` cannot carry raw input path
/// strings for path-area focus without a MODULE node — doing
/// so would make the field polymorphic in an unsafe way
/// (sometimes a stable key, sometimes user input). Option 1
/// was selected: add `resolved_path` as a separate field and
/// keep `resolved_key` strictly nullable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Focus {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub input: Option<String>,
	pub resolved: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resolved_kind: Option<ResolvedKind>,
	/// Graph-node stable key, if the focus resolved to an exact
	/// node. `None` for repo-level focus and for path-area
	/// focus that has no backing MODULE node.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resolved_key: Option<String>,
	/// Normalized repo-relative path for the resolved focus.
	/// Present for FILE and MODULE/path-area resolution. Absent
	/// for repo-level and unresolved focus.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resolved_path: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reason: Option<FocusFailureReason>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub candidates: Vec<FocusCandidate>,
}

impl Focus {
	/// Build a repo-level focus record for a successfully
	/// loaded repo.
	///
	/// `resolved_key` is `None` because repo-level focus does
	/// not resolve to a graph node. The repo identity is
	/// carried on the envelope's top-level `repo` field; it
	/// must not be duplicated into `resolved_key`, which is
	/// reserved for graph-node stable keys.
	pub fn repo() -> Self {
		Self {
			input: None,
			resolved: true,
			resolved_kind: Some(ResolvedKind::Repo),
			resolved_key: None,
			resolved_path: None,
			reason: None,
			candidates: Vec::new(),
		}
	}

	/// Build a file-focus record. The file resolved to an
	/// exact FILE node. `stable_key` is optional because the
	/// path-based resolution can confirm a file exists without
	/// producing the node's stable key in all code paths. When
	/// `None`, `resolved_key` is omitted from the JSON output
	/// (same nullable contract as `path_area`).
	pub fn file(
		input: &str,
		stable_key: Option<&str>,
		path: &str,
	) -> Self {
		Self {
			input: Some(input.to_string()),
			resolved: true,
			resolved_kind: Some(ResolvedKind::File),
			resolved_key: stable_key.map(|k| k.to_string()),
			resolved_path: Some(path.to_string()),
			reason: None,
			candidates: Vec::new(),
		}
	}

	/// Build a module/path-area focus record. If the path
	/// corresponds to an exact MODULE node, `module_stable_key`
	/// carries its stable key. Otherwise `resolved_key` is None
	/// and `resolved_path` alone identifies the area.
	pub fn path_area(
		input: &str,
		module_stable_key: Option<&str>,
		path: &str,
	) -> Self {
		Self {
			input: Some(input.to_string()),
			resolved: true,
			resolved_kind: Some(ResolvedKind::Module),
			resolved_key: module_stable_key.map(|k| k.to_string()),
			resolved_path: Some(path.to_string()),
			reason: None,
			candidates: Vec::new(),
		}
	}

	/// Build a symbol-focus record. The focus resolved to an
	/// exact SYMBOL node. `stable_key` is the node's stable key.
	/// `file_path` is the repo-relative path if the symbol has a
	/// file association (via `AgentSymbolContext`).
	pub fn symbol(
		input: &str,
		stable_key: &str,
		file_path: Option<&str>,
	) -> Self {
		Self {
			input: Some(input.to_string()),
			resolved: true,
			resolved_kind: Some(ResolvedKind::Symbol),
			resolved_key: Some(stable_key.to_string()),
			resolved_path: file_path.map(|p| p.to_string()),
			reason: None,
			candidates: Vec::new(),
		}
	}

	/// Build an ambiguous focus record. The focus matched
	/// multiple candidates and cannot be resolved uniquely.
	pub fn ambiguous(
		input: &str,
		candidates: Vec<FocusCandidate>,
	) -> Self {
		Self {
			input: Some(input.to_string()),
			resolved: false,
			resolved_kind: None,
			resolved_key: None,
			resolved_path: None,
			reason: Some(FocusFailureReason::Ambiguous),
			candidates,
		}
	}

	/// Build an unresolved focus record.
	pub fn no_match(input: &str) -> Self {
		Self {
			input: Some(input.to_string()),
			resolved: false,
			resolved_kind: None,
			resolved_key: None,
			resolved_path: None,
			reason: Some(FocusFailureReason::NoMatch),
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
	fn repo_focus_serializes_without_input_key_or_path() {
		let f = Focus::repo();
		let s = serde_json::to_string(&f).unwrap();
		assert!(!s.contains("\"input\""), "input must be omitted at repo-level: {}", s);
		assert!(s.contains("\"resolved\":true"));
		assert!(s.contains("\"resolved_kind\":\"repo\""));
		assert!(!s.contains("\"resolved_key\""), "repo focus must not emit resolved_key: {}", s);
		assert!(!s.contains("\"resolved_path\""), "repo focus must not emit resolved_path: {}", s);
		assert!(!s.contains("\"candidates\""), "empty candidates must be omitted: {}", s);
	}

	#[test]
	fn file_focus_serializes_with_key_and_path() {
		let f = Focus::file("src/core/service.ts", Some("r1:src/core/service.ts:FILE"), "src/core/service.ts");
		let json = serde_json::to_value(&f).unwrap();
		assert_eq!(json["resolved"], true);
		assert_eq!(json["resolved_kind"], "file");
		assert_eq!(json["resolved_key"], "r1:src/core/service.ts:FILE");
		assert_eq!(json["resolved_path"], "src/core/service.ts");
		assert_eq!(json["input"], "src/core/service.ts");
	}

	#[test]
	fn path_area_focus_with_module_node_serializes_key_and_path() {
		let f = Focus::path_area("src/core", Some("r1:src/core:MODULE"), "src/core");
		let json = serde_json::to_value(&f).unwrap();
		assert_eq!(json["resolved"], true);
		assert_eq!(json["resolved_kind"], "module");
		assert_eq!(json["resolved_key"], "r1:src/core:MODULE");
		assert_eq!(json["resolved_path"], "src/core");
	}

	#[test]
	fn path_area_focus_without_module_node_omits_key() {
		let f = Focus::path_area("src/core", None, "src/core");
		let json = serde_json::to_value(&f).unwrap();
		assert_eq!(json["resolved"], true);
		assert_eq!(json["resolved_kind"], "module");
		assert!(json.get("resolved_key").is_none(), "path-area without MODULE node must not emit resolved_key");
		assert_eq!(json["resolved_path"], "src/core");
	}

	#[test]
	fn no_match_focus_omits_key_and_path() {
		let f = Focus::no_match("nonexistent/path");
		let json = serde_json::to_value(&f).unwrap();
		assert_eq!(json["resolved"], false);
		assert!(json.get("resolved_kind").is_none());
		assert!(json.get("resolved_key").is_none());
		assert!(json.get("resolved_path").is_none());
		assert_eq!(json["reason"], "no_match");
		assert_eq!(json["input"], "nonexistent/path");
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
