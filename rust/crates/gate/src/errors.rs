//! Error types for the gate policy crate.
//!
//! Two concerns live here:
//!
//!   1. `GateStorageError` — storage-agnostic error returned by
//!      any `GateStorageRead` implementation. Storage adapters
//!      are expected to map their internal errors (rusqlite,
//!      `StorageError`) into this shape. The gate crate never
//!      sees SQL diagnostics or table names.
//!
//!   2. `GateError` — the error returned by `assemble`. Wraps
//!      storage failures and adds domain-level failure reasons
//!      (malformed measurements, missing value fields).
//!
//! The pure `compute` function does NOT return `GateError`. It
//! operates on an already-validated `GateInput` — the `assemble`
//! layer is responsible for pre-parsing every measurement and
//! inference row, and for propagating `GateError::MalformedEvidence`
//! with the exact pre-relocation diagnostic strings when a
//! value_json row cannot be parsed. This preserves the
//! Rust-28 / Rust-29 / Rust-31 fail-loud contract: authored
//! policy evidence with a malformed row aborts the gate run
//! rather than silently degrading to MISSING_EVIDENCE.
//!
//! `compute` itself is total over `GateInput` and cannot error
//! out; any parse failure has already become `GateError` before
//! compute sees the input.

use std::fmt;

// ── Storage-agnostic port error ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateStorageError {
	pub operation: &'static str,
	pub message: String,
}

impl GateStorageError {
	pub fn new(operation: &'static str, message: impl Into<String>) -> Self {
		Self { operation, message: message.into() }
	}
}

impl fmt::Display for GateStorageError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "gate storage error in {}: {}", self.operation, self.message)
	}
}

impl std::error::Error for GateStorageError {}

// ── Use-case error ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
	/// The storage port failed.
	Storage(GateStorageError),
	/// An evidence row (measurement, inference, waiver) was
	/// malformed at the adapter boundary in a way that makes it
	/// impossible to continue the gate run. Adapters should
	/// prefer returning `Storage(...)` with a clear message;
	/// this variant is reserved for cases where the gate crate
	/// itself detects a structural problem after the port has
	/// already returned data.
	MalformedEvidence {
		operation: &'static str,
		reason: String,
	},
}

impl fmt::Display for GateError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Storage(e) => write!(f, "{}", e),
			Self::MalformedEvidence { operation, reason } => {
				write!(f, "malformed evidence in {}: {}", operation, reason)
			}
		}
	}
}

impl std::error::Error for GateError {}

impl From<GateStorageError> for GateError {
	fn from(e: GateStorageError) -> Self {
		Self::Storage(e)
	}
}
