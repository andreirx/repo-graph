//! Boundary declaration parser — policy-aware parsing (RS-MG-3).
//!
//! Parses raw boundary declarations from storage and filters to
//! discovered-module boundaries for evaluation.
//!
//! Design decisions:
//! - Malformed JSON / unknown explicit domain → explicit error
//! - Missing/legacy selectorDomain → ignored (not error)
//! - Normalized DTO only (no raw selector structs)
//!
//! This is policy-adjacent interpretation, not raw storage access.

use serde::Deserialize;

// ── Input DTO ──────────────────────────────────────────────────────

/// Raw declaration row from storage.
///
/// Mirrors the storage `DeclarationRow` shape but only includes
/// fields needed for boundary parsing.
#[derive(Debug, Clone)]
pub struct RawBoundaryDeclaration {
	pub declaration_uid: String,
	pub value_json: String,
}

// ── Output DTO ─────────────────────────────────────────────────────

/// Normalized boundary for evaluation.
///
/// Minimal DTO for the violation evaluator. Contains only the
/// fields needed for policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatableBoundary {
	pub declaration_uid: String,
	pub source_canonical_path: String,
	pub target_canonical_path: String,
	pub reason: Option<String>,
}

// ── Errors ─────────────────────────────────────────────────────────

/// Error during boundary declaration parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryParseError {
	/// JSON parsing failed.
	InvalidJson {
		declaration_uid: String,
		message: String,
	},
	/// Explicit but unrecognized selectorDomain.
	UnknownSelectorDomain {
		declaration_uid: String,
		domain: String,
	},
	/// discovered_module payload is malformed.
	InvalidDiscoveredModulePayload {
		declaration_uid: String,
		message: String,
	},
}

impl std::fmt::Display for BoundaryParseError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BoundaryParseError::InvalidJson {
				declaration_uid,
				message,
			} => {
				write!(
					f,
					"boundary {} has invalid JSON: {}",
					declaration_uid, message
				)
			}
			BoundaryParseError::UnknownSelectorDomain {
				declaration_uid,
				domain,
			} => {
				write!(
					f,
					"boundary {} has unknown selectorDomain: {} (expected \"directory_module\" or \"discovered_module\")",
					declaration_uid, domain
				)
			}
			BoundaryParseError::InvalidDiscoveredModulePayload {
				declaration_uid,
				message,
			} => {
				write!(
					f,
					"boundary {} has invalid discovered_module payload: {}",
					declaration_uid, message
				)
			}
		}
	}
}

impl std::error::Error for BoundaryParseError {}

// ── Parser ─────────────────────────────────────────────────────────

/// Parse raw boundary declarations and extract discovered-module boundaries.
///
/// Filtering logic:
/// - `None` (missing selectorDomain) → legacy directory_module → ignored
/// - `"directory_module"` → legacy → ignored
/// - `"discovered_module"` → parsed and returned
/// - any other explicit value → error
///
/// Fails fast on first malformed declaration.
pub fn parse_discovered_module_boundaries(
	declarations: &[RawBoundaryDeclaration],
) -> Result<Vec<EvaluatableBoundary>, BoundaryParseError> {
	let mut results = Vec::new();

	for decl in declarations {
		// Parse JSON
		let value: BoundaryValueJson = serde_json::from_str(&decl.value_json).map_err(|e| {
			BoundaryParseError::InvalidJson {
				declaration_uid: decl.declaration_uid.clone(),
				message: e.to_string(),
			}
		})?;

		// Check selectorDomain
		match value.selector_domain.as_deref() {
			// Missing or explicit directory_module → legacy, skip
			None | Some("directory_module") => continue,

			// discovered_module → parse and include
			Some("discovered_module") => {
				let boundary = parse_discovered_module_payload(decl, &value)?;
				results.push(boundary);
			}

			// Unknown explicit domain → error
			Some(domain) => {
				return Err(BoundaryParseError::UnknownSelectorDomain {
					declaration_uid: decl.declaration_uid.clone(),
					domain: domain.to_string(),
				});
			}
		}
	}

	Ok(results)
}

// ── Internal types ─────────────────────────────────────────────────

/// Intermediate JSON structure for boundary value.
///
/// Serde optional fields handle both legacy and discovered-module shapes.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundaryValueJson {
	#[serde(default)]
	selector_domain: Option<String>,

	// discovered_module fields
	source: Option<ModuleIdentityJson>,
	forbids: Option<ForbidsJson>,

	// Shared field
	reason: Option<String>,
}

/// Module identity for discovered_module boundaries.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleIdentityJson {
	canonical_root_path: Option<String>,
}

/// Forbids field — can be string (legacy) or object (discovered_module).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ForbidsJson {
	/// Legacy directory_module: forbids is a string path.
	/// The string content is not used — we only need to detect this variant
	/// to reject it for discovered_module boundaries.
	#[allow(dead_code)]
	LegacyString(String),
	/// discovered_module: forbids is an object with canonicalRootPath
	DiscoveredModule(ModuleIdentityJson),
}

/// Parse a discovered_module boundary payload.
fn parse_discovered_module_payload(
	decl: &RawBoundaryDeclaration,
	value: &BoundaryValueJson,
) -> Result<EvaluatableBoundary, BoundaryParseError> {
	// Extract source.canonicalRootPath
	let source_path = value
		.source
		.as_ref()
		.and_then(|s| s.canonical_root_path.as_ref())
		.ok_or_else(|| BoundaryParseError::InvalidDiscoveredModulePayload {
			declaration_uid: decl.declaration_uid.clone(),
			message: "missing source.canonicalRootPath".to_string(),
		})?;

	// Extract forbids.canonicalRootPath
	let target_path = match &value.forbids {
		Some(ForbidsJson::DiscoveredModule(m)) => {
			m.canonical_root_path.as_ref().ok_or_else(|| {
				BoundaryParseError::InvalidDiscoveredModulePayload {
					declaration_uid: decl.declaration_uid.clone(),
					message: "missing forbids.canonicalRootPath".to_string(),
				}
			})?
		}
		Some(ForbidsJson::LegacyString(_)) => {
			return Err(BoundaryParseError::InvalidDiscoveredModulePayload {
				declaration_uid: decl.declaration_uid.clone(),
				message: "forbids must be object for discovered_module, got string".to_string(),
			});
		}
		None => {
			return Err(BoundaryParseError::InvalidDiscoveredModulePayload {
				declaration_uid: decl.declaration_uid.clone(),
				message: "missing forbids".to_string(),
			});
		}
	};

	Ok(EvaluatableBoundary {
		declaration_uid: decl.declaration_uid.clone(),
		source_canonical_path: source_path.clone(),
		target_canonical_path: target_path.clone(),
		reason: value.reason.clone(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_decl(uid: &str, json: &str) -> RawBoundaryDeclaration {
		RawBoundaryDeclaration {
			declaration_uid: uid.to_string(),
			value_json: json.to_string(),
		}
	}

	// ── Happy path ─────────────────────────────────────────────────

	#[test]
	fn parses_discovered_module_boundary() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": { "canonicalRootPath": "packages/db" },
			"reason": "UI must not access DB"
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls).expect("parse");

		assert_eq!(result.len(), 1);
		assert_eq!(result[0].declaration_uid, "decl-1");
		assert_eq!(result[0].source_canonical_path, "packages/app");
		assert_eq!(result[0].target_canonical_path, "packages/db");
		assert_eq!(result[0].reason, Some("UI must not access DB".to_string()));
	}

	#[test]
	fn parses_discovered_module_boundary_without_reason() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls).expect("parse");

		assert_eq!(result.len(), 1);
		assert!(result[0].reason.is_none());
	}

	// ── Legacy filtering ───────────────────────────────────────────

	#[test]
	fn ignores_missing_selector_domain_legacy() {
		let json = r#"{ "forbids": "src/db" }"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls).expect("parse");

		assert!(result.is_empty());
	}

	#[test]
	fn ignores_explicit_directory_module() {
		let json = r#"{
			"selectorDomain": "directory_module",
			"forbids": "src/db"
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls).expect("parse");

		assert!(result.is_empty());
	}

	#[test]
	fn filters_mixed_declarations() {
		let discovered = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;
		let legacy1 = r#"{ "forbids": "src/db" }"#;
		let legacy2 = r#"{ "selectorDomain": "directory_module", "forbids": "src/core" }"#;

		let decls = vec![
			make_decl("legacy-1", legacy1),
			make_decl("discovered-1", discovered),
			make_decl("legacy-2", legacy2),
		];
		let result = parse_discovered_module_boundaries(&decls).expect("parse");

		assert_eq!(result.len(), 1);
		assert_eq!(result[0].declaration_uid, "discovered-1");
	}

	// ── Error: unknown selector domain ─────────────────────────────

	#[test]
	fn error_on_unknown_selector_domain() {
		let json = r#"{
			"selectorDomain": "bogus",
			"forbids": "src/db"
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::UnknownSelectorDomain {
				declaration_uid,
				domain,
			} => {
				assert_eq!(declaration_uid, "decl-1");
				assert_eq!(domain, "bogus");
			}
			_ => panic!("expected UnknownSelectorDomain"),
		}
	}

	#[test]
	fn error_on_typo_in_selector_domain() {
		let json = r#"{
			"selectorDomain": "discovered_modules",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::UnknownSelectorDomain { domain, .. } => {
				assert_eq!(domain, "discovered_modules");
			}
			_ => panic!("expected UnknownSelectorDomain"),
		}
	}

	// ── Error: invalid JSON ────────────────────────────────────────

	#[test]
	fn error_on_invalid_json() {
		let json = "not valid json";

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidJson {
				declaration_uid, ..
			} => {
				assert_eq!(declaration_uid, "decl-1");
			}
			_ => panic!("expected InvalidJson"),
		}
	}

	// ── Error: invalid discovered_module payload ───────────────────

	#[test]
	fn error_on_missing_source() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidDiscoveredModulePayload { message, .. } => {
				assert!(message.contains("source"));
			}
			_ => panic!("expected InvalidDiscoveredModulePayload"),
		}
	}

	#[test]
	fn error_on_missing_forbids() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" }
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidDiscoveredModulePayload { message, .. } => {
				assert!(message.contains("forbids"));
			}
			_ => panic!("expected InvalidDiscoveredModulePayload"),
		}
	}

	#[test]
	fn error_on_string_forbids_for_discovered_module() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": "packages/db"
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidDiscoveredModulePayload { message, .. } => {
				assert!(message.contains("forbids must be object"));
			}
			_ => panic!("expected InvalidDiscoveredModulePayload"),
		}
	}

	#[test]
	fn error_on_missing_source_canonical_path() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": {},
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidDiscoveredModulePayload { message, .. } => {
				assert!(message.contains("source.canonicalRootPath"));
			}
			_ => panic!("expected InvalidDiscoveredModulePayload"),
		}
	}

	#[test]
	fn error_on_missing_forbids_canonical_path() {
		let json = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": {}
		}"#;

		let decls = vec![make_decl("decl-1", json)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::InvalidDiscoveredModulePayload { message, .. } => {
				assert!(message.contains("forbids.canonicalRootPath"));
			}
			_ => panic!("expected InvalidDiscoveredModulePayload"),
		}
	}

	// ── Fail-fast behavior ─────────────────────────────────────────

	#[test]
	fn fails_fast_on_first_error() {
		let valid = r#"{
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": "packages/app" },
			"forbids": { "canonicalRootPath": "packages/db" }
		}"#;
		let invalid = r#"{ "selectorDomain": "bogus", "forbids": "x" }"#;

		// Invalid comes first
		let decls = vec![make_decl("invalid", invalid), make_decl("valid", valid)];
		let result = parse_discovered_module_boundaries(&decls);

		assert!(result.is_err());
		match result.unwrap_err() {
			BoundaryParseError::UnknownSelectorDomain {
				declaration_uid, ..
			} => {
				assert_eq!(declaration_uid, "invalid");
			}
			_ => panic!("expected UnknownSelectorDomain"),
		}
	}
}
