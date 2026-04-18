//! Binding-table schema, TOML loader, and validator.
//!
//! Contract §9 is authoritative for the binding-table format. This
//! module defines the Rust types that mirror that format, a loader
//! that parses a TOML string into a validated in-memory
//! `BindingTable`, and a `load_embedded()` accessor for the
//! slice-1 probe entry shipped in `bindings.toml`.
//!
//! Validation scope (slice 1):
//!
//!   - every entry field is present and non-empty
//!   - `resource_kind` is one of {db, fs, cache, blob} (the
//!     `config` value is reserved for the future config/env seam
//!     slice and is rejected here per contract §9.2)
//!   - `direction` is one of {read, write, read_write}
//!   - `basis` is one of {stdlib_api, sdk_call}
//!   - `language` is one of {rust, typescript, python, java, cpp}
//!   - optional `notes` is at most 200 characters
//!   - no two entries share the same (language, module,
//!     symbol_path) triple
//!
//! The loader fails hard on any violation. Callers that need
//! lenient parsing for tooling purposes can build on
//! `RawBindingTable` directly; the `BindingTable` wrapper only
//! holds validated entries.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::basis::Basis;

// ── Embedded probe table ──────────────────────────────────────────

/// The SB-1 probe binding-table file, embedded at compile time.
/// Any change to the file triggers a Rust rebuild.
const EMBEDDED_TABLE_TOML: &str = include_str!("../bindings.toml");

// ── Language / resource / direction enums ─────────────────────────

/// Source language a binding applies to.
///
/// Serde values are lower-case (`"rust"`, `"typescript"`, etc.).
/// The `typescript` variant covers the full TS/JS family (`.ts`,
/// `.tsx`, `.js`, `.jsx`) on the matcher side; the TOML value is
/// `"typescript"` for any entry in that family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
	/// Rust source files (`.rs`). Reserved for a future slice;
	/// slice 1 ships no Rust-language bindings in the embedded
	/// table but the enum variant is present so table rows
	/// added forward-compatibly parse cleanly.
	Rust,
	/// TypeScript / TSX / JavaScript / JSX source files.
	/// The only language actually consumed by slice 1.
	Typescript,
	/// Python source files (`.py`). Reserved for a future slice.
	Python,
	/// Java source files (`.java`). Reserved for a future slice.
	Java,
	/// C and C++ source files. Reserved for a future slice.
	Cpp,
}

/// Resource kind segment of the stable-key shape.
///
/// `config` is deliberately NOT a variant; contract §9.2 forbids
/// it in slice 1. The config/env seam ships in a separate slice
/// with its own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
	/// Database endpoint. Stable-key kind `DB_RESOURCE`.
	Db,
	/// Filesystem path or logical filesystem resource.
	/// Stable-key kind `FS_PATH`.
	Fs,
	/// Cache endpoint (Redis, Memcached, etc.).
	/// Stable-key kind `STATE` (existing node kind, `CACHE`
	/// subtype added by this slice).
	Cache,
	/// Object-storage endpoint (S3 bucket, Azure Blob container,
	/// GCP Storage namespace). Stable-key kind `BLOB`.
	Blob,
}

impl ResourceKind {
	/// The TOML / binding-table string form.
	pub const fn as_str(self) -> &'static str {
		match self {
			ResourceKind::Db => "db",
			ResourceKind::Fs => "fs",
			ResourceKind::Cache => "cache",
			ResourceKind::Blob => "blob",
		}
	}
}

/// Read / write direction of a binding.
///
/// `read_write` is the honest representation for generic entry
/// points (e.g. `db.query(sql)`) where slice 1 does not parse the
/// SQL to determine direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
	/// The binding reads from the external resource. Emits a
	/// `READS` edge.
	Read,
	/// The binding writes to / mutates the external resource.
	/// Emits a `WRITES` edge.
	Write,
	/// The binding both reads and writes (e.g. atomic upsert,
	/// get-or-create, or a generic `query()` entry point whose
	/// direction cannot be determined without SQL parsing).
	/// Emits BOTH `READS` and `WRITES` edges at the same call
	/// site (contract §3.2).
	ReadWrite,
}

impl Direction {
	/// The TOML / binding-table string form.
	pub const fn as_str(self) -> &'static str {
		match self {
			Direction::Read => "read",
			Direction::Write => "write",
			Direction::ReadWrite => "read_write",
		}
	}
}

// ── Raw record (serde-deserialized, pre-validation) ───────────────

/// Raw TOML shape — what the parser produces before validation.
/// Crate-private; the `BindingTable` constructor is the only
/// supported public path and it only yields validated entries.
#[derive(Debug, Deserialize)]
struct RawBindingTable {
	#[serde(default)]
	binding: Vec<RawBindingEntry>,
}

#[derive(Debug, Deserialize)]
struct RawBindingEntry {
	language: Language,
	module: String,
	symbol_path: String,
	resource_kind: ResourceKind,
	driver: String,
	direction: Direction,
	basis: Basis,
	notes: Option<String>,
}

// ── Validated entry ───────────────────────────────────────────────

/// One validated binding-table entry. All string fields are
/// non-empty; direction / basis / resource_kind / language are
/// bound to their enum domains by serde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingEntry {
	/// Language the binding applies to.
	pub language: Language,
	/// Module path as it appears in a source-file import
	/// (e.g. `"@aws-sdk/client-s3"`, `"fs"`, `"std::fs"`).
	pub module: String,
	/// Symbol path within the module
	/// (e.g. `"PutObjectCommand"`, `"readFile"`).
	pub symbol_path: String,
	/// Resource kind this binding targets.
	pub resource_kind: ResourceKind,
	/// Driver / backend identifier used in the stable key's
	/// `<driver>` or `<provider>` segment.
	pub driver: String,
	/// Read / write classification. `ReadWrite` emits both edges
	/// at the same call site (contract §3.2).
	pub direction: Direction,
	/// Basis tag carried in edge evidence (contract §7).
	pub basis: Basis,
	/// Optional free-text notes, bounded to 200 characters.
	pub notes: Option<String>,
}

impl BindingEntry {
	/// The canonical binding key used in edge evidence
	/// (`metadata_json.binding_key`). Format per contract §8:
	/// `<language>:<module>:<symbol_path>[:<direction>]`.
	pub fn binding_key(&self) -> String {
		format!(
			"{}:{}:{}:{}",
			match self.language {
				Language::Rust => "rust",
				Language::Typescript => "typescript",
				Language::Python => "python",
				Language::Java => "java",
				Language::Cpp => "cpp",
			},
			self.module,
			self.symbol_path,
			self.direction.as_str(),
		)
	}
}

/// Max length of the optional `notes` field (contract §9.2).
const MAX_NOTES_LEN: usize = 200;

// ── Errors ────────────────────────────────────────────────────────

/// Loader / validator failure.
#[derive(Debug, Error)]
pub enum TableError {
	/// TOML parsing failed (syntax error, unknown variant, etc.).
	#[error("binding table TOML parse failed: {0}")]
	Parse(#[from] toml::de::Error),

	/// A required string field was empty.
	#[error("binding entry {index} has empty field `{field}`")]
	EmptyField {
		/// Zero-based index of the offending entry.
		index: usize,
		/// Field name.
		field: &'static str,
	},

	/// A string field contained leading or trailing whitespace.
	///
	/// Binding tables are authored configuration, not runtime
	/// input. Silent trimming would hide authoring mistakes and
	/// can cause surprising duplicate collisions (two rows that
	/// differ only in trailing whitespace would normalize to the
	/// same key and one would be silently dropped). The loader
	/// rejects the whole file when this is detected so the
	/// author fixes the source.
	#[error("binding entry {index} `{field}` has leading or trailing whitespace")]
	SurroundingWhitespace {
		/// Zero-based index of the offending entry.
		index: usize,
		/// Field name.
		field: &'static str,
	},

	/// The optional `notes` field exceeded `MAX_NOTES_LEN`.
	#[error("binding entry {index} `notes` is {actual} characters (max {max})")]
	NotesTooLong {
		/// Zero-based index of the offending entry.
		index: usize,
		/// Actual length.
		actual: usize,
		/// Maximum permitted length.
		max: usize,
	},

	/// Two entries share the same (language, module, symbol_path)
	/// tuple.
	#[error("duplicate binding entry (language={language:?}, module={module:?}, symbol_path={symbol_path:?}) at indices {first} and {second}")]
	Duplicate {
		/// Language of the duplicate.
		language: Language,
		/// Module of the duplicate.
		module: String,
		/// Symbol path of the duplicate.
		symbol_path: String,
		/// Index of the first (kept) entry.
		first: usize,
		/// Index of the duplicate entry.
		second: usize,
	},
}

// ── Validated table ───────────────────────────────────────────────

/// Validated in-memory binding table. Construction is the only
/// way to obtain one; every field on every entry satisfies the
/// contract §9 invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingTable {
	entries: Vec<BindingEntry>,
}

impl BindingTable {
	/// Parse and validate a binding table from a TOML source
	/// string.
	pub fn load_str(source: &str) -> Result<Self, TableError> {
		let raw: RawBindingTable = toml::from_str(source)?;
		let mut entries: Vec<BindingEntry> = Vec::with_capacity(raw.binding.len());
		let mut seen: HashSet<(Language, String, String)> = HashSet::new();

		for (index, raw_entry) in raw.binding.into_iter().enumerate() {
			// Emptiness check first: a whitespace-only field is
			// reported as empty, not as surrounding-whitespace.
			// The author intended emptiness; that is the clearer
			// diagnostic.
			if raw_entry.module.trim().is_empty() {
				return Err(TableError::EmptyField { index, field: "module" });
			}
			if raw_entry.symbol_path.trim().is_empty() {
				return Err(TableError::EmptyField {
					index,
					field: "symbol_path",
				});
			}
			if raw_entry.driver.trim().is_empty() {
				return Err(TableError::EmptyField { index, field: "driver" });
			}
			// Reject leading/trailing whitespace on authored
			// configuration. See `TableError::SurroundingWhitespace`
			// docs for rationale.
			if raw_entry.module.trim() != raw_entry.module {
				return Err(TableError::SurroundingWhitespace {
					index,
					field: "module",
				});
			}
			if raw_entry.symbol_path.trim() != raw_entry.symbol_path {
				return Err(TableError::SurroundingWhitespace {
					index,
					field: "symbol_path",
				});
			}
			if raw_entry.driver.trim() != raw_entry.driver {
				return Err(TableError::SurroundingWhitespace {
					index,
					field: "driver",
				});
			}
			if let Some(notes) = &raw_entry.notes {
				if notes.chars().count() > MAX_NOTES_LEN {
					return Err(TableError::NotesTooLong {
						index,
						actual: notes.chars().count(),
						max: MAX_NOTES_LEN,
					});
				}
			}

			let dedup_key = (
				raw_entry.language,
				raw_entry.module.clone(),
				raw_entry.symbol_path.clone(),
			);
			// Locate prior index for duplicate reporting by
			// scanning already-accepted entries (slice-1 tables
			// are small; a linear scan is fine). Future slices
			// that bump the table size materially can replace
			// this with a HashMap<_, usize>.
			if !seen.insert(dedup_key) {
				let first = entries
					.iter()
					.position(|e| {
						e.language == raw_entry.language
							&& e.module == raw_entry.module
							&& e.symbol_path == raw_entry.symbol_path
					})
					.expect("seen-set hit implies prior entry present");
				return Err(TableError::Duplicate {
					language: raw_entry.language,
					module: raw_entry.module,
					symbol_path: raw_entry.symbol_path,
					first,
					second: index,
				});
			}

			entries.push(BindingEntry {
				language: raw_entry.language,
				module: raw_entry.module,
				symbol_path: raw_entry.symbol_path,
				resource_kind: raw_entry.resource_kind,
				driver: raw_entry.driver,
				direction: raw_entry.direction,
				basis: raw_entry.basis,
				notes: raw_entry.notes,
			});
		}

		Ok(BindingTable { entries })
	}

	/// Access the validated embedded binding table shipped with
	/// this crate. Validation runs on first access. A malformed
	/// `bindings.toml` is a build-time / test-time bug, so
	/// failures panic rather than return an error.
	pub fn load_embedded() -> &'static BindingTable {
		static TABLE: OnceLock<BindingTable> = OnceLock::new();
		TABLE.get_or_init(|| {
			BindingTable::load_str(EMBEDDED_TABLE_TOML)
				.expect("embedded bindings.toml should parse and validate")
		})
	}

	/// Iterate all validated entries in declaration order.
	pub fn entries(&self) -> &[BindingEntry] {
		&self.entries
	}

	/// Number of validated entries.
	pub fn len(&self) -> usize {
		self.entries.len()
	}

	/// True iff the table contains no entries.
	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Inline smoke test: confirms the embedded probe file parses
	/// and is shaped as expected. Comprehensive coverage
	/// (rejection paths, duplicates, long notes, bad variants)
	/// lives in `tests/table.rs`.
	#[test]
	fn embedded_probe_table_parses() {
		let table = BindingTable::load_embedded();
		assert_eq!(table.len(), 1, "probe table must ship exactly one entry");
		let e = &table.entries()[0];
		assert_eq!(e.language, Language::Typescript);
		assert_eq!(e.module, "@aws-sdk/client-s3");
		assert_eq!(e.symbol_path, "PutObjectCommand");
		assert_eq!(e.resource_kind, ResourceKind::Blob);
		assert_eq!(e.driver, "s3");
		assert_eq!(e.direction, Direction::Write);
		assert_eq!(e.basis, Basis::SdkCall);
		assert!(e.notes.is_some(), "probe entry includes notes");
	}

	#[test]
	fn binding_key_composes_per_contract() {
		let table = BindingTable::load_embedded();
		let e = &table.entries()[0];
		assert_eq!(
			e.binding_key(),
			"typescript:@aws-sdk/client-s3:PutObjectCommand:write"
		);
	}
}
