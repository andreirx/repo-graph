//! Crate-level error type for the storage substrate.
//!
//! Per R2-C decision D-C4: a single canonical `StorageError` enum
//! at the crate level rather than per-module error types. R2-C
//! introduces this type for migration runner errors; R2-D and
//! R2-E will extend it with new variants for connection lifecycle
//! and CRUD failures respectively.
//!
//! ── Variant policy ────────────────────────────────────────────
//!
//! - `Sqlite(rusqlite::Error)` is the catch-all for any rusqlite
//!   operation that fails. The `#[from]` derive enables `?` to
//!   automatically wrap rusqlite errors at call sites.
//!
//! - `MalformedRequirement { declaration_uid, reason }` is the
//!   structured analog of the TypeScript `MalformedRequirementError`
//!   class from `004-obligation-ids.ts`. The `declaration_uid` is
//!   carried so audit logs can identify the offending row; the
//!   `reason` is a human-readable description of which structural
//!   invariant was violated. The TS class extends `Error` with the
//!   same two fields exposed as `declarationUid` and (implicit
//!   message). Rust's struct-variant pattern carries both fields
//!   directly without needing a separate type.
//!
//! - Additional variants will be added by R2-D / R2-E as needed.
//!   The convention is: prefer adding a new variant over reusing
//!   an existing one with a different semantic meaning. Each
//!   variant should answer one specific question about why the
//!   operation failed.
//!
//! ── Display semantics ─────────────────────────────────────────
//!
//! `MalformedRequirement` formats as:
//!
//! ```text
//! malformed requirement declaration <uid>: <reason>
//! ```
//!
//! Matching the TS error message exactly:
//!
//! ```text
//! malformed requirement declaration ${declarationUid}: ${reason}
//! ```
//!
//! This parity at the error-message level is intentional: error
//! messages may be captured in logs, reproducibility reports, or
//! cross-runtime audit trails. Diverging the message text would
//! create a soft drift surface even if the structural behavior
//! were identical.

/// Crate-level error type for storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
	/// Wraps any underlying rusqlite error. The `#[from]` enables
	/// automatic propagation via `?` from rusqlite operations.
	#[error(transparent)]
	Sqlite(#[from] rusqlite::Error),

	/// A requirement declaration's `value_json` has a structural
	/// invariant violation. Used by both migration 004 (obligation-id
	/// backfill) and runtime reads (gate command). The declaration UID
	/// and reason are preserved for diagnostics.
	#[error("malformed requirement declaration {declaration_uid}: {reason}")]
	MalformedRequirement {
		declaration_uid: String,
		reason: String,
	},

	/// A waiver declaration's `value_json` has a structural invariant
	/// violation. Waivers are authored governance inputs; malformed
	/// active waivers must fail loudly (not silently disappear from
	/// gate evaluation). Follows the same principle as
	/// `MalformedRequirement` (Rust-24).
	///
	/// Added in Rust-25 for `find_active_waivers`.
	#[error("malformed waiver declaration {declaration_uid}: {reason}")]
	MalformedWaiver {
		declaration_uid: String,
		reason: String,
	},

	/// A quality_policy declaration's `value_json` has a structural
	/// invariant violation. Quality policies are authored governance
	/// inputs; malformed active policies must fail loudly (not silently
	/// disappear from evaluation). Follows the same principle as
	/// `MalformedRequirement`.
	#[error("malformed quality_policy declaration {declaration_uid}: {reason}")]
	MalformedQualityPolicy {
		declaration_uid: String,
		reason: String,
	},

	/// The target declaration for a supersede operation does not
	/// exist or is already inactive. Supersede is an explicit
	/// lifecycle transition that requires an active source.
	///
	/// Added in Rust-37 for `supersede_declaration`.
	#[error("cannot supersede declaration {declaration_uid}: {reason}")]
	SupersedeError {
		declaration_uid: String,
		reason: String,
	},

	/// A storage method that has been declared in the
	/// `TrustStorageRead` trait but not yet implemented in the
	/// adapter. Returns through the typed error channel instead
	/// of panicking via `todo!()`, so callers can handle it
	/// explicitly.
	///
	/// Added in R4-E for the 4 complex methods deferred to R4-F.
	/// R4-F has landed and all methods are now implemented. This
	/// variant is currently unreachable and can be removed in a
	/// future cleanup pass. Kept for now to avoid a breaking
	/// change on the public `StorageError` enum.
	#[error("storage method not yet implemented: {0}")]
	NotImplemented(String),

	/// A migration failed with a migration-specific error that is
	/// not a raw SQLite error. Used for migration-level invariant
	/// violations like FK check failures after table rebuild.
	///
	/// Added for migration 018 (surface identity table rebuild).
	#[error("migration error: {0}")]
	Migration(String),

	/// JSON serialization failed for a value being written to storage.
	/// This should be rare since most serialized values are well-typed
	/// structs that always serialize successfully.
	#[error("serialization error: {0}")]
	SerializationError(String),

	/// An argument to a storage method violated a documented invariant.
	/// Used for precondition checks that are not SQLite-level constraints.
	#[error("invalid argument: {0}")]
	InvalidArgument(String),

	/// A measurement's `value_json` could not be parsed as a numeric value.
	///
	/// Measurements are expected to have `{ "value": <number> }` in
	/// `value_json`. If the value is missing, not numeric, or the JSON
	/// is malformed, this error is raised during enrichment.
	///
	/// Added for quality policy assessment enrichment (QP-Step-5).
	#[error("measurement parse error for {target_stable_key}: {reason}")]
	MeasurementParseError {
		target_stable_key: String,
		reason: String,
	},

	/// `insert_nodes` detected two or more nodes in the input
	/// batch that share the same `stable_key`. This is an
	/// identity-model defect — either an extractor bug
	/// (duplicate emission), an indexer bug (double-building the
	/// same symbol), or a stable_key format gap (declaration
	/// merging case not disambiguated).
	///
	/// Mirrors the TypeScript `NodeStableKeyCollisionError` class
	/// from `sqlite-storage.ts:138`. The Rust port preserves the
	/// structural data (the `collisions` field) for programmatic
	/// inspection AND formats a human-readable multi-line message
	/// matching the TS class output exactly.
	///
	/// **Why a custom variant instead of letting SQLite's UNIQUE
	/// constraint failure propagate:** the bare SQLite error
	/// reads `UNIQUE constraint failed: nodes.snapshot_uid,
	/// nodes.stable_key` and does not identify which rows
	/// collided. With this preflight check, the error names the
	/// stable_key and surfaces all colliding nodes' identity
	/// fields so the upstream extractor/indexer bug can be found
	/// directly. Pinned by the user as a parity-critical R2-E
	/// behavior.
	#[error("{}", format_node_collisions(.collisions))]
	NodeStableKeyCollision {
		collisions: Vec<NodeCollision>,
	},
}

/// One stable_key collision: a single stable_key value plus the
/// list of nodes in the input batch that share that key.
///
/// Mirrors the inner `{ stableKey, nodes }` shape from the TS
/// `NodeStableKeyCollisionError` constructor argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCollision {
	pub stable_key: String,
	pub nodes: Vec<NodeCollisionInfo>,
}

/// Identity fields of one node involved in a stable_key
/// collision. Mirrors the TS inner array element shape.
///
/// `line_start` is the only position field carried (matches the
/// TS error class which uses `location.lineStart` for display).
/// The full `SourceLocation` is not propagated because the error
/// message only needs the line number for display, and carrying
/// the full location through the error type would couple this
/// error to the parity-boundary DTO unnecessarily.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCollisionInfo {
	pub node_uid: String,
	pub stable_key: String,
	pub kind: String,
	pub subtype: Option<String>,
	pub name: String,
	pub qualified_name: Option<String>,
	pub file_uid: Option<String>,
	pub line_start: Option<i64>,
}

/// Format a list of `NodeCollision` rows into the multi-line
/// human-readable message used by the
/// `NodeStableKeyCollision` variant's `Display` impl.
///
/// Mirrors the TypeScript `NodeStableKeyCollisionError`
/// constructor at `sqlite-storage.ts:138-185` line by line:
///
/// ```text
/// Node stable_key collision(s) detected during insert.
/// <N> colliding key(s):
///
///   stable_key: <key>
///     kind=<kind> subtype=<subtype-or-dash> name=<name> qualified_name=<qn-or-dash>
///       file: <path-or-(unknown)>:<line>  [file_uid=<file_uid>]  node_uid=<uid>
///
///   stable_key: <key2>
///     ...
///
/// This is an identity-model defect. The stable_key format is not disambiguating these emissions.
/// ```
///
/// The format is parity-faithful at the per-line level. Tests
/// pin specific substrings rather than full message comparisons
/// to keep the contract focused on what matters (the stable_key
/// and the colliding node identities).
fn format_node_collisions(collisions: &[NodeCollision]) -> String {
	use std::fmt::Write;

	let mut out = String::new();
	let _ = writeln!(&mut out, "Node stable_key collision(s) detected during insert.");
	let _ = writeln!(&mut out, "{} colliding key(s):", collisions.len());
	let _ = writeln!(&mut out);
	for c in collisions {
		let _ = writeln!(&mut out, "  stable_key: {}", c.stable_key);
		for n in &c.nodes {
			let loc = match n.line_start {
				Some(ls) => format!(":{}", ls),
				None => String::new(),
			};
			// Extract path from file_uid if present (format is
			// "<repo_uid>:<repo-relative-path>"). Mirrors the TS
			// `extractPathFromFileUid` helper.
			let file_path = n.file_uid.as_ref().and_then(|fuid| {
				let first_colon = fuid.find(':')?;
				Some(&fuid[first_colon + 1..])
			});
			let _ = writeln!(
				&mut out,
				"    kind={} subtype={} name={} qualified_name={}",
				n.kind,
				n.subtype.as_deref().unwrap_or("-"),
				n.name,
				n.qualified_name.as_deref().unwrap_or("-"),
			);
			let file_uid_suffix = match &n.file_uid {
				Some(fuid) => format!("  [file_uid={}]", fuid),
				None => String::new(),
			};
			let _ = writeln!(
				&mut out,
				"      file: {}{}{}  node_uid={}",
				file_path.unwrap_or("(unknown)"),
				loc,
				file_uid_suffix,
				n.node_uid,
			);
		}
		let _ = writeln!(&mut out);
	}
	let _ = write!(
		&mut out,
		"This is an identity-model defect. The stable_key format is not disambiguating these emissions."
	);
	out
}

/// Convenience constructor for `MalformedRequirement` errors.
///
/// Used inside migration 004's `backfill_obligation_ids` function
/// at every structural-invariant check. Reduces boilerplate at
/// the call site without obscuring the error type.
pub(crate) fn malformed_requirement(
	declaration_uid: &str,
	reason: impl Into<String>,
) -> StorageError {
	StorageError::MalformedRequirement {
		declaration_uid: declaration_uid.to_string(),
		reason: reason.into(),
	}
}
