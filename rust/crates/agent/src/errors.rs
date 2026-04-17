//! Error types for the agent use-case layer.
//!
//! Two concerns live here:
//!
//!   1. `AgentStorageError` — a storage-agnostic error returned by
//!      `AgentStorageRead` implementations. Storage adapters are
//!      expected to map their internal errors (e.g. `StorageError`,
//!      `rusqlite::Error`) into this shape. The agent crate never
//!      sees rusqlite, SQL diagnostics, or table names through the
//!      port.
//!
//!   2. `OrientError` — the error returned by `orient()`. Wraps
//!      storage failures and adds domain-level failure reasons
//!      (missing repo, missing snapshot, unimplemented focus).
//!
//! `FocusNotImplementedYet` is deliberately an `OrientError`
//! variant and NOT a domain-level "focus resolution failed" entry
//! in the output JSON. The contract reserves the
//! `focus.resolved = false` shape for genuine input resolution
//! failures (ambiguous, no match). A slice-scope gap is a caller-
//! visible error, not a silent degraded response.

use std::fmt;

// ── Storage-agnostic port error ──────────────────────────────────

/// An error returned by any `AgentStorageRead` implementation.
///
/// Intentionally storage-agnostic:
///
///   - `operation` is a stable `&'static str` identifier naming
///     the port method that failed (e.g. `"find_cycles"`). Callers
///     and tests can pattern-match on this identifier without
///     depending on any storage-crate internals.
///   - `message` is a free-form human-readable diagnostic provided
///     by the adapter. It is NOT parsed by the agent crate.
///
/// The storage adapter is responsible for converting its own error
/// (e.g. `repo_graph_storage::StorageError`) into this shape. That
/// conversion is NOT implemented here because the agent crate has
/// no dependency on any storage crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStorageError {
	pub operation: &'static str,
	pub message: String,
}

impl AgentStorageError {
	pub fn new(operation: &'static str, message: impl Into<String>) -> Self {
		Self { operation, message: message.into() }
	}
}

impl fmt::Display for AgentStorageError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "agent storage error in {}: {}", self.operation, self.message)
	}
}

impl std::error::Error for AgentStorageError {}

// ── Use-case error ───────────────────────────────────────────────

/// Errors returned by `orient()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrientError {
	/// The storage port failed. The inner error names which
	/// operation failed and why.
	Storage(AgentStorageError),

	/// The given `repo_uid` does not exist in storage.
	NoRepo { repo_uid: String },

	/// The repo exists but has no READY snapshot. Either the repo
	/// has never been indexed successfully or the only snapshots
	/// are still BUILDING / STALE / FAILED.
	NoSnapshot { repo_uid: String },

	/// Rust-42 scope limitation: only repo-level orient is
	/// implemented. Module/path/symbol focus is deferred to
	/// Rust-44+. This is NOT a focus-resolution failure in the
	/// domain sense; it is a slice boundary. The caller must
	/// receive this as an error so they do not mistake a silent
	/// repo-level response for a focused response.
	FocusNotImplementedYet { focus: String },
}

impl fmt::Display for OrientError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Storage(e) => write!(f, "{}", e),
			Self::NoRepo { repo_uid } => {
				write!(f, "repo not found: {}", repo_uid)
			}
			Self::NoSnapshot { repo_uid } => {
				write!(
					f,
					"no READY snapshot for repo: {}. index the repo first.",
					repo_uid
				)
			}
			Self::FocusNotImplementedYet { focus } => write!(
				f,
				"focus '{}' is not supported in Rust-42; only repo-level \
				 orient is available. Module focus ships in Rust-44, \
				 symbol focus in Rust-45.",
				focus
			),
		}
	}
}

impl std::error::Error for OrientError {}

impl From<AgentStorageError> for OrientError {
	fn from(e: AgentStorageError) -> Self {
		Self::Storage(e)
	}
}

// ── Check use-case error ────────────────────────────────────────

/// Errors returned by `run_check()`.
///
/// Distinct from `OrientError` because check has different failure
/// semantics: missing snapshot is NOT an error (it produces
/// `CHECK_INCOMPLETE`), whereas orient requires a snapshot.
/// `NoRepo` IS an error because there is no repo identity to
/// populate the envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
	/// The storage port failed.
	Storage(AgentStorageError),

	/// The given `repo_uid` does not exist in storage.
	NoRepo { repo_uid: String },
}

impl fmt::Display for CheckError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Storage(e) => write!(f, "{}", e),
			Self::NoRepo { repo_uid } => {
				write!(f, "repo not found: {}", repo_uid)
			}
		}
	}
}

impl std::error::Error for CheckError {}

impl From<AgentStorageError> for CheckError {
	fn from(e: AgentStorageError) -> Self {
		Self::Storage(e)
	}
}
