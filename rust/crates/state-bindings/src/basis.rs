//! Basis tags for state-boundary edge evidence.
//!
//! Contract §7 defines the slice-1 basis vocabulary as a closed
//! two-value set. Additional basis values
//! (`sql_literal`, `config_binding`, `type_enriched`,
//! `symbol_pattern`, `import_binding`) are reserved for later
//! slices and MUST NOT appear on slice-1 edges.
//!
//! Serialization: TOML field values are `"stdlib_api"` and
//! `"sdk_call"`. JSON evidence field `basis` carries the same
//! string values. The serde rename is explicit to pin the wire
//! contract.

use serde::{Deserialize, Serialize};

/// Basis tag identifying how a state-boundary binding was matched.
///
/// See contract §7 for the semantic definitions. This enum is the
/// canonical representation; string conversions are via serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Basis {
	/// Standard-library API (e.g. `std::fs`, Node `fs`, Python
	/// `open`, Java `java.nio.file`, C++ `std::filesystem`).
	#[serde(rename = "stdlib_api")]
	StdlibApi,

	/// Third-party SDK API (e.g. `pg`, AWS SDK `PutObjectCommand`,
	/// `redis.createClient`).
	#[serde(rename = "sdk_call")]
	SdkCall,
}

impl Basis {
	/// The canonical string representation used in evidence JSON
	/// and the binding-table TOML.
	pub const fn as_str(self) -> &'static str {
		match self {
			Basis::StdlibApi => "stdlib_api",
			Basis::SdkCall => "sdk_call",
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde::Deserialize;

	// Lightweight wrapper so toml can deserialize a bare basis
	// value inside a table (TOML requires a root table).
	#[derive(Deserialize)]
	struct BasisHolder {
		b: Basis,
	}

	#[test]
	fn as_str_matches_serde_rename() {
		// Pin the wire contract: the compile-time `as_str` and the
		// serde-deserialized form MUST agree. If anyone edits one
		// without the other, this test fires.
		let h: BasisHolder = toml::from_str("b = \"stdlib_api\"").unwrap();
		assert_eq!(h.b, Basis::StdlibApi);
		assert_eq!(h.b.as_str(), "stdlib_api");

		let h: BasisHolder = toml::from_str("b = \"sdk_call\"").unwrap();
		assert_eq!(h.b, Basis::SdkCall);
		assert_eq!(h.b.as_str(), "sdk_call");
	}

	#[test]
	fn rejects_unknown_basis() {
		// `sql_literal`, `type_enriched`, `symbol_pattern`,
		// `import_binding`, `config_binding` are reserved for
		// later slices. Slice 1 MUST reject them at deserialization.
		assert!(toml::from_str::<BasisHolder>("b = \"sql_literal\"").is_err());
		assert!(toml::from_str::<BasisHolder>("b = \"type_enriched\"").is_err());
		assert!(toml::from_str::<BasisHolder>("b = \"symbol_pattern\"").is_err());
		assert!(toml::from_str::<BasisHolder>("b = \"import_binding\"").is_err());
		assert!(toml::from_str::<BasisHolder>("b = \"config_binding\"").is_err());
	}

	#[test]
	fn rejects_empty_and_garbage() {
		assert!(toml::from_str::<BasisHolder>("b = \"\"").is_err());
		assert!(toml::from_str::<BasisHolder>("b = \"STDLIB_API\"").is_err(), "case-sensitive");
	}
}
