//! Stable-key builders for state-boundary resource nodes.
//!
//! Contract §5.1 defines four canonical stable-key shapes:
//!
//!   - DB resource:   `<repo_uid>:db:<driver>:<logical_name>:DB_RESOURCE`
//!   - Filesystem:    `<repo_uid>:fs:<normalized_path_or_logical>:FS_PATH`
//!   - Cache:         `<repo_uid>:cache:<driver>:<logical_name>:STATE`
//!   - Blob:          `<repo_uid>:blob:<provider>:<logical_name>:BLOB`
//!
//! Slice-1 design (SB-1.6): validated newtypes enforce the
//! invariants at construction, and the builders are INFALLIBLE.
//! Illegal state is unrepresentable once a newtype is constructed,
//! so the builders cannot fail and callers do not need to thread
//! `Result` through the emission pipeline.
//!
//! Validation rules:
//!
//! Generic segment newtypes (`RepoUid`, `Driver`, `Provider`,
//! `LogicalName`):
//!
//! - non-empty after trim
//! - no `:` character (preserves stable-key delimiter
//!   parsability for these segments)
//! - no ASCII control characters (codepoints < 0x20)
//!
//! `FsPathOrLogical` deliberately relaxes the `:` rejection. A
//! filesystem payload legitimately contains colons (Windows
//! drive-letter absolute paths `C:\\...`, URI-style references,
//! etc.). FS stable-key parsing is prefix/suffix bounded, not
//! naive colon-split; the path segment is delimited by the fixed
//! `<repo_uid>:fs:` prefix and the fixed `:FS_PATH` suffix. See
//! contract §5.1. `FsPathOrLogical` therefore enforces only:
//!
//! - non-empty after trim
//! - no ASCII control characters (codepoints < 0x20)
//!
//! The contract's logical-name precedence rules (§5.3) are the
//! responsibility of the caller (state-extractor in a later slice).
//! This module only validates shape, not provenance.

use std::fmt;

use thiserror::Error;

// ── Node-kind suffix constants ────────────────────────────────────
//
// These strings MUST match the canonical node-kind names used by
// the indexer and storage layers. state-bindings deliberately does
// NOT depend on those crates; instead, the string literals are
// pinned here as the cross-layer contract surface. If the canonical
// `NodeKind` enum in the Rust storage types ever drifts from these
// values, the contract tests in the state-extractor crate catch it.

/// Node-kind suffix for DB logical-endpoint resource keys.
pub const NODE_KIND_DB_RESOURCE: &str = "DB_RESOURCE";
/// Node-kind suffix for filesystem path / logical filesystem keys.
pub const NODE_KIND_FS_PATH: &str = "FS_PATH";
/// Node-kind suffix for cache endpoint keys (existing `STATE` kind).
pub const NODE_KIND_STATE: &str = "STATE";
/// Node-kind suffix for object-storage endpoint keys.
pub const NODE_KIND_BLOB: &str = "BLOB";

// ── Validation errors ─────────────────────────────────────────────

/// Validation failure when constructing a stable-key newtype.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
	/// The input string was empty or contained only whitespace.
	#[error("{field} must not be empty")]
	Empty {
		/// Field name (e.g. "repo_uid", "driver").
		field: &'static str,
	},

	/// The input contained a `:` character. Colons are reserved
	/// as the stable-key segment delimiter.
	#[error("{field} must not contain ':' (reserved as stable-key delimiter)")]
	ContainsColon {
		/// Field name.
		field: &'static str,
	},

	/// The input contained an ASCII control character (< 0x20).
	#[error("{field} must not contain ASCII control characters")]
	ContainsControl {
		/// Field name.
		field: &'static str,
	},
}

/// Generic segment validator used by `RepoUid`, `Driver`,
/// `Provider`, and `LogicalName`. Rejects empty, colon-bearing,
/// and control-char inputs.
fn validate_segment(field: &'static str, s: &str) -> Result<(), ValidationError> {
	let trimmed = s.trim();
	if trimmed.is_empty() {
		return Err(ValidationError::Empty { field });
	}
	if trimmed.contains(':') {
		return Err(ValidationError::ContainsColon { field });
	}
	if trimmed.chars().any(|c| (c as u32) < 0x20) {
		return Err(ValidationError::ContainsControl { field });
	}
	Ok(())
}

/// Path-segment validator used by `FsPathOrLogical`. Permits `:`
/// in the payload (Windows drive letters, URI-style references).
/// Rejects empty and control-char inputs. FS stable-key parsing
/// is prefix/suffix bounded by design; see contract §5.1.
fn validate_path_segment(field: &'static str, s: &str) -> Result<(), ValidationError> {
	let trimmed = s.trim();
	if trimmed.is_empty() {
		return Err(ValidationError::Empty { field });
	}
	if trimmed.chars().any(|c| (c as u32) < 0x20) {
		return Err(ValidationError::ContainsControl { field });
	}
	Ok(())
}

// ── Validated newtypes ────────────────────────────────────────────

/// Repo identifier. First segment of every stable key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoUid(String);

impl RepoUid {
	/// Construct a validated `RepoUid`. Returns `Err` if the input
	/// is empty, contains `:`, or contains control characters.
	pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
		let s: String = value.into();
		validate_segment("repo_uid", &s)?;
		Ok(Self(s.trim().to_string()))
	}

	/// Borrow the validated string.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for RepoUid {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

/// Driver / backend identifier (e.g. `postgres`, `redis`, `mysql2`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Driver(String);

impl Driver {
	/// Construct a validated `Driver`.
	pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
		let s: String = value.into();
		validate_segment("driver", &s)?;
		Ok(Self(s.trim().to_string()))
	}

	/// Borrow the validated string.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for Driver {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

/// Blob provider identifier (e.g. `s3`, `azure`, `gcs`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Provider(String);

impl Provider {
	/// Construct a validated `Provider`.
	pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
		let s: String = value.into();
		validate_segment("provider", &s)?;
		Ok(Self(s.trim().to_string()))
	}

	/// Borrow the validated string.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for Provider {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

/// Logical-name segment for db / cache / blob resources.
///
/// Sourced per contract §5.3: env-key name, stable literal
/// identifier, or normalized literal endpoint. Construction does
/// not evaluate provenance; callers (state-extractor) are
/// responsible for selecting a valid source per §5.3 before
/// constructing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogicalName(String);

impl LogicalName {
	/// Construct a validated `LogicalName`.
	pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
		let s: String = value.into();
		validate_segment("logical_name", &s)?;
		Ok(Self(s.trim().to_string()))
	}

	/// Borrow the validated string.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for LogicalName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

/// Filesystem path or logical filesystem name.
///
/// Literal paths are normalized by the caller (collapse `..`
/// segments when safe, preserve absolute-vs-relative distinction,
/// never resolve symlinks). This type does not re-normalize; it
/// enforces only the shared segment invariants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FsPathOrLogical(String);

impl FsPathOrLogical {
	/// Construct a validated `FsPathOrLogical`.
	///
	/// Unlike the other newtypes, `:` is permitted in the payload
	/// to accommodate Windows drive-letter absolute paths
	/// (`C:\\Windows\\path`) and URI-style references. FS stable-
	/// key parsing is prefix/suffix bounded, not naive colon-
	/// split, so internal colons in the path do not break parser
	/// determinism. See contract §5.1.
	pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
		let s: String = value.into();
		validate_path_segment("fs_path_or_logical", &s)?;
		Ok(Self(s.trim().to_string()))
	}

	/// Borrow the validated string.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for FsPathOrLogical {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

// ── Stable-key output newtype ─────────────────────────────────────

/// A built stable key. Opaque wrapper to keep callers from
/// treating arbitrary strings as stable keys without going
/// through a builder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StableKey(String);

impl StableKey {
	/// Borrow the underlying string.
	pub fn as_str(&self) -> &str {
		&self.0
	}

	/// Consume and return the underlying string.
	pub fn into_string(self) -> String {
		self.0
	}
}

impl fmt::Display for StableKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

// ── Builders (infallible, consume validated inputs) ───────────────

/// Build a DB resource stable key per contract §5.1:
/// `<repo_uid>:db:<driver>:<logical_name>:DB_RESOURCE`.
pub fn build_db_resource(
	repo_uid: &RepoUid,
	driver: &Driver,
	logical_name: &LogicalName,
) -> StableKey {
	StableKey(format!(
		"{}:db:{}:{}:{}",
		repo_uid.0, driver.0, logical_name.0, NODE_KIND_DB_RESOURCE
	))
}

/// Build a filesystem resource stable key per contract §5.1:
/// `<repo_uid>:fs:<normalized_path_or_logical>:FS_PATH`.
pub fn build_fs_path(repo_uid: &RepoUid, path_or_logical: &FsPathOrLogical) -> StableKey {
	StableKey(format!(
		"{}:fs:{}:{}",
		repo_uid.0, path_or_logical.0, NODE_KIND_FS_PATH
	))
}

/// Build a cache resource stable key per contract §5.1:
/// `<repo_uid>:cache:<driver>:<logical_name>:STATE`.
pub fn build_cache_state(
	repo_uid: &RepoUid,
	driver: &Driver,
	logical_name: &LogicalName,
) -> StableKey {
	StableKey(format!(
		"{}:cache:{}:{}:{}",
		repo_uid.0, driver.0, logical_name.0, NODE_KIND_STATE
	))
}

/// Build a blob resource stable key per contract §5.1:
/// `<repo_uid>:blob:<provider>:<logical_name>:BLOB`.
pub fn build_blob(
	repo_uid: &RepoUid,
	provider: &Provider,
	logical_name: &LogicalName,
) -> StableKey {
	StableKey(format!(
		"{}:blob:{}:{}:{}",
		repo_uid.0, provider.0, logical_name.0, NODE_KIND_BLOB
	))
}

// ── Unit tests: inline smoke coverage ─────────────────────────────
//
// Comprehensive shape / rejection / round-trip coverage lives in
// `tests/stable_key.rs`. The tests inlined here exercise only the
// construction path, so a failure in validation logic fires at
// the same compilation unit as the definition.

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn repo_uid_rejects_empty() {
		assert!(matches!(
			RepoUid::new(""),
			Err(ValidationError::Empty { field: "repo_uid" })
		));
		assert!(matches!(
			RepoUid::new("   "),
			Err(ValidationError::Empty { field: "repo_uid" })
		));
	}

	#[test]
	fn repo_uid_rejects_colon() {
		assert!(matches!(
			RepoUid::new("my:service"),
			Err(ValidationError::ContainsColon { field: "repo_uid" })
		));
	}

	#[test]
	fn repo_uid_rejects_control_char() {
		assert!(matches!(
			RepoUid::new("my\tservice"),
			Err(ValidationError::ContainsControl { field: "repo_uid" })
		));
		assert!(matches!(
			RepoUid::new("my\nservice"),
			Err(ValidationError::ContainsControl { field: "repo_uid" })
		));
	}

	#[test]
	fn repo_uid_trims_surrounding_whitespace() {
		let r = RepoUid::new("  my-service  ").unwrap();
		assert_eq!(r.as_str(), "my-service");
	}

	#[test]
	fn generic_segment_newtypes_reject_colons() {
		// Spot-check that every generic-segment newtype rejects a
		// colon. `FsPathOrLogical` is deliberately NOT in this
		// set; it uses the path-segment validator which permits
		// colons (see the `FsPathOrLogical::new` docs and contract
		// §5.1).
		assert!(Driver::new("pg:bad").is_err());
		assert!(Provider::new("s3:bad").is_err());
		assert!(LogicalName::new("name:bad").is_err());
		// Positive assertion for `FsPathOrLogical`: colons are
		// allowed here.
		assert!(FsPathOrLogical::new("C:\\Windows\\path").is_ok());
	}
}
