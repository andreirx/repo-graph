//! State-boundary evidence blob.
//!
//! Contract §8 defines the structured evidence written into
//! `edges.metadata_json` for every state-boundary READS or WRITES
//! edge. This module owns the Rust struct that mirrors that blob
//! and the `LogicalNameSource` enum cited in §8.
//!
//! Design locks:
//!
//! - SB-2.4 (serde struct + serde_json): the evidence shape is a
//!   real Rust type, not a manually-constructed JSON string. Any
//!   drift in the wire contract breaks a test rather than
//!   silently emitting malformed metadata.
//! - SB-2.5 (placement: state-extractor): `LogicalNameSource`
//!   lives here, not in `state-bindings`. It is an evidence-
//!   field value — the outcome of the caller's logical-name-
//!   precedence decision — not binding-table policy.
//!
//! Serialization is via `serde_json`. The `binding_notes` field
//! is skipped when `None` so absent notes do not appear as
//! `"binding_notes": null` in the emitted JSON; contract §8 lists
//! this key as optional.

use repo_graph_state_bindings::{table::Direction, Basis};
use serde::{Deserialize, Serialize};

/// Source of the logical-name segment in a resource stable key.
/// Serialized value appears as the `logical_name_source` field in
/// `edges.metadata_json`.
///
/// Per contract §8, slice-1 values:
///
/// - `env_key` — logical name is the env / config key read at the
///   call site (contract §5.3 precedence rule 1).
/// - `literal_identifier` — logical name is a stable literal
///   identifier passed to the call (precedence rule 2).
/// - `normalized_path` — logical name is a normalized literal
///   filesystem path (precedence rule 3, path form).
/// - `normalized_url` — logical name is a normalized literal URL
///   (precedence rule 3, URL form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogicalNameSource {
	/// Logical name sourced from a config / environment key.
	#[serde(rename = "env_key")]
	EnvKey,
	/// Logical name sourced from a stable literal identifier.
	#[serde(rename = "literal_identifier")]
	LiteralIdentifier,
	/// Logical name sourced from a normalized literal filesystem
	/// path.
	#[serde(rename = "normalized_path")]
	NormalizedPath,
	/// Logical name sourced from a normalized literal URL.
	#[serde(rename = "normalized_url")]
	NormalizedUrl,
}

/// Evidence blob serialized into `edges.metadata_json` per
/// contract §8.
///
/// Field order matches the contract's listed order. Serde
/// serialization respects struct declaration order, so the JSON
/// key order on the wire is stable and predictable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateBoundaryEvidence {
	/// Contract version of the evidence shape. Always `1` for
	/// slice 1; bumps per contract §13 versioning rules.
	pub state_boundary_version: u32,
	/// Basis tag copied from the matched binding entry.
	pub basis: Basis,
	/// Canonical binding-key string.
	/// Format: `<language>:<module>:<symbol_path>:<direction>`
	/// per contract §8. Produced by
	/// `BindingEntry::binding_key()`.
	pub binding_key: String,
	/// Direction of the emitted edge.
	///
	/// For a `Read` or `Write` binding this equals the binding's
	/// direction. For a `ReadWrite` binding the emitter produces
	/// TWO edges at the same call site; each edge's evidence
	/// blob carries `direction = read` or `direction = write`
	/// respectively (never `direction = read_write`). The binding
	/// entry's read-or-write ambiguity is visible in the
	/// `binding_key` suffix (`...:read_write`); the per-edge
	/// direction is resolved. This lets downstream readers
	/// interpret a single edge without branching on binding
	/// semantics.
	pub direction: Direction,
	/// Source of the logical-name segment on the target
	/// resource's stable key.
	pub logical_name_source: LogicalNameSource,
	/// Optional free-text notes copied from the binding entry.
	/// Omitted from JSON when `None` (contract §8 lists notes as
	/// optional).
	#[serde(skip_serializing_if = "Option::is_none", default)]
	pub binding_notes: Option<String>,
}

/// Current contract-defined evidence version. Bumps per contract
/// §13 versioning rules. Emitted as `state_boundary_version` on
/// every evidence blob.
pub const STATE_BOUNDARY_EVIDENCE_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn evidence_serializes_with_contract_field_names() {
		let ev = StateBoundaryEvidence {
			state_boundary_version: STATE_BOUNDARY_EVIDENCE_VERSION,
			basis: Basis::SdkCall,
			binding_key: "typescript:@aws-sdk/client-s3:PutObjectCommand:write".to_string(),
			direction: Direction::Write,
			logical_name_source: LogicalNameSource::LiteralIdentifier,
			binding_notes: None,
		};
		let json = serde_json::to_string(&ev).unwrap();

		// Pin the serialized shape. Field order matches struct
		// declaration order (serde default).
		assert!(
			json.contains("\"state_boundary_version\":1"),
			"got {}",
			json
		);
		assert!(json.contains("\"basis\":\"sdk_call\""), "got {}", json);
		assert!(
			json.contains("\"binding_key\":\"typescript:@aws-sdk/client-s3:PutObjectCommand:write\""),
			"got {}",
			json
		);
		assert!(json.contains("\"direction\":\"write\""), "got {}", json);
		assert!(
			json.contains("\"logical_name_source\":\"literal_identifier\""),
			"got {}",
			json
		);
		assert!(
			!json.contains("binding_notes"),
			"notes should not appear when None, got {}",
			json
		);
	}

	#[test]
	fn evidence_includes_notes_when_present() {
		let ev = StateBoundaryEvidence {
			state_boundary_version: STATE_BOUNDARY_EVIDENCE_VERSION,
			basis: Basis::StdlibApi,
			binding_key: "typescript:fs:readFile:read".to_string(),
			direction: Direction::Read,
			logical_name_source: LogicalNameSource::NormalizedPath,
			binding_notes: Some("probe entry".to_string()),
		};
		let json = serde_json::to_string(&ev).unwrap();
		assert!(
			json.contains("\"binding_notes\":\"probe entry\""),
			"got {}",
			json
		);
	}

	#[test]
	fn logical_name_source_renames_match_contract() {
		let cases = [
			(LogicalNameSource::EnvKey, "env_key"),
			(LogicalNameSource::LiteralIdentifier, "literal_identifier"),
			(LogicalNameSource::NormalizedPath, "normalized_path"),
			(LogicalNameSource::NormalizedUrl, "normalized_url"),
		];
		for (variant, expected) in cases {
			let json = serde_json::to_string(&variant).unwrap();
			assert_eq!(json, format!("\"{}\"", expected));
		}
	}
}
