//! TypeScript-language adapter: `ResolvedCallsite` → emitter input.
//!
//! The Rust `ts-extractor` crate produces `ResolvedCallsite` facts
//! (indexer::types) as a side-channel of `ExtractionResult`. Each
//! fact carries a resolved `(module, symbol)` callee pair, an
//! `Arg0Payload` classification, and the enclosing symbol's node
//! UID. This adapter turns those facts into
//! `StateBoundaryCallsite` inputs and drives the language-
//! agnostic `StateBoundaryEmitter`.
//!
//! SB-3 design locks (recorded in
//! `docs/milestones/rmap-state-boundaries-v1.md` SB-3 section):
//!
//! - 3-impl.1 (C): free adapter function here; emitter method set
//!   unchanged.
//! - 3-impl.2 (A): synthetic `imports_in_file` vector satisfies
//!   the state-bindings Form-A matcher without adding a new
//!   matcher entry point. The matcher's imports-presence check
//!   is redundant for pre-resolved callsites but preserved to
//!   keep a single matching code path.
//! - 3-impl.3: `Arg0Payload::StringLiteral` → `CallsiteLogicalName::Fs`
//!   (preserves colon-bearing Windows / URI paths);
//!   `Arg0Payload::EnvKeyRead` → `CallsiteLogicalName::Generic`.
//!   `LogicalNameSource` is discriminated per payload shape:
//!     - string literal, URL-shaped (`<scheme>://...`, per
//!       `is_url_shaped`) → `NormalizedUrl`
//!     - string literal, anything else (POSIX/Windows/relative
//!       paths) → `NormalizedPath`
//!     - env-read → `EnvKey`
//!   Contract §8 defines `normalized_path` and `normalized_url`
//!   as distinct evidence values; the adapter MUST preserve that
//!   distinction rather than collapsing both into `NormalizedPath`.
//!
//! Input validation: if a payload string fails the newtype
//! construction (empty after trim, control chars), the call site
//! is silently skipped — the extractor should not have surfaced
//! such a payload, and propagating the failure upward would
//! abort emission for an entire snapshot over one malformed
//! call. Silent skip is safer; future enrichment may surface
//! these as diagnostic facts.

use repo_graph_indexer::types::{Arg0Payload, ResolvedCallsite};
use repo_graph_state_bindings::{
	CalleePath, FsPathOrLogical, ImportView, LogicalName,
};

use crate::emit::{CallsiteLogicalName, StateBoundaryCallsite, StateBoundaryEmitter};
use crate::emit::EmitError;
use crate::evidence::LogicalNameSource;

/// Classify a string-literal payload as URL-shaped vs path-shaped.
///
/// URL shape is recognized via RFC 3986's scheme grammar applied
/// narrowly: `<scheme>://...` where `<scheme>` starts with an
/// ASCII letter followed by letters, digits, `+`, `-`, or `.`.
/// This catches `file:///etc/config`, `http://...`, `s3://...`,
/// etc.
///
/// Windows drive-letter paths (`C:\Windows\path`) are NOT URLs —
/// they contain `:` but not `://`. POSIX paths and relative paths
/// obviously are not URLs either. Anything that is not URL-shaped
/// is treated as a path.
fn is_url_shaped(s: &str) -> bool {
	// Find the `://` anchor. Must be preceded by a non-empty
	// RFC-3986 scheme.
	let Some(scheme_end) = s.find("://") else {
		return false;
	};
	if scheme_end == 0 {
		return false;
	}
	let scheme = &s[..scheme_end];
	let mut chars = scheme.chars();
	let first = match chars.next() {
		Some(c) => c,
		None => return false,
	};
	if !first.is_ascii_alphabetic() {
		return false;
	}
	chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Drive the emitter over a batch of `ResolvedCallsite` facts
/// produced by the Rust `ts-extractor`.
///
/// Each callsite is adapted into a `StateBoundaryCallsite` and
/// passed to the emitter. Callsites whose payload fails newtype
/// validation are silently skipped (see module docs). The return
/// value is the total number of edges emitted across the batch
/// (0 for unmatched callsites, 1 for read/write bindings, 2 for
/// read_write bindings per call).
///
/// This function does NOT consume the emitter; the caller drains
/// facts via `emitter.drain()` after all batches are processed.
pub fn emit_from_resolved_callsites(
	callsites: &[ResolvedCallsite],
	emitter: &mut StateBoundaryEmitter<'_>,
) -> Result<usize, EmitError> {
	let mut total = 0;
	for rc in callsites {
		if let Some(site) = adapt_resolved_callsite(rc) {
			total += emitter.emit_for_callsite(&site)?;
		}
	}
	Ok(total)
}

/// Adapt a single `ResolvedCallsite` into a
/// `StateBoundaryCallsite`. Returns `None` if the payload fails
/// newtype validation (empty, control chars, etc.).
///
/// Exposed as a free function (not a method on the emitter) so
/// the adapter layer can be unit-tested without a full emitter
/// setup.
pub fn adapt_resolved_callsite(rc: &ResolvedCallsite) -> Option<StateBoundaryCallsite> {
	let (logical_name, logical_name_source) = match &rc.arg0_payload {
		Arg0Payload::StringLiteral { value } => {
			// Fs variant preserves colon-bearing Windows / URI
			// paths. For non-FS bindings, the emitter downgrades
			// to LogicalName and errors on unconvertible payloads;
			// under SB-3's FS-only binding table this is unreachable,
			// but the typed error path stands for future slices.
			let payload = FsPathOrLogical::new(value.clone()).ok()?;
			// Distinguish URL-shaped literals (`file:///...`,
			// `s3://...`, etc.) from path-shaped literals
			// (`/etc/config`, `C:\Windows\path`, `./rel`). The
			// contract evidence enum carries both as separate
			// variants (§8), so the adapter must not collapse
			// them to `NormalizedPath`.
			let source = if is_url_shaped(value) {
				LogicalNameSource::NormalizedUrl
			} else {
				LogicalNameSource::NormalizedPath
			};
			(CallsiteLogicalName::Fs(payload), source)
		}
		Arg0Payload::EnvKeyRead { key_name } => {
			// Env-key names never contain `:`; Generic keeps them
			// in the strictest newtype.
			let ln = LogicalName::new(key_name.clone()).ok()?;
			(CallsiteLogicalName::Generic(ln), LogicalNameSource::EnvKey)
		}
	};

	// Synthetic imports_in_file: the module is guaranteed to be
	// imported (ts-extractor only emits a ResolvedCallsite when
	// resolution via import_bindings succeeded), so the matcher's
	// imports-presence check is a tautology here. The single-
	// entry vector satisfies it without introducing a matcher API
	// variant that skips the check.
	let imports_in_file = vec![ImportView {
		module_path: rc.resolved_module.clone(),
		imported_symbol: rc.resolved_symbol.clone(),
		import_alias: None,
	}];

	Some(StateBoundaryCallsite {
		source_node_uid: rc.enclosing_symbol_node_uid.clone(),
		// file_uid is not carried on ResolvedCallsite (slice-1
		// scope). The caller can pass it separately if needed;
		// for the FS emission path it is unused downstream.
		file_uid: String::new(),
		source_location: rc.source_location,
		imports_in_file,
		callee: CalleePath {
			resolved_module: Some(rc.resolved_module.clone()),
			resolved_symbol: rc.resolved_symbol.clone(),
		},
		logical_name,
		logical_name_source,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use repo_graph_classification::types::SourceLocation;

	fn loc() -> SourceLocation {
		SourceLocation {
			line_start: 10,
			col_start: 0,
			line_end: 10,
			col_end: 20,
		}
	}

	#[test]
	fn string_literal_payload_becomes_fs_variant() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "/etc/config".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).expect("valid payload");
		assert!(matches!(adapted.logical_name, CallsiteLogicalName::Fs(_)));
		assert_eq!(adapted.logical_name_source, LogicalNameSource::NormalizedPath);
		assert_eq!(adapted.callee.resolved_module.as_deref(), Some("fs"));
		assert_eq!(adapted.callee.resolved_symbol, "readFile");
	}

	#[test]
	fn windows_drive_letter_preserved_through_fs_variant() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "C:\\Windows\\path".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).expect("colon-bearing payload valid");
		match &adapted.logical_name {
			CallsiteLogicalName::Fs(path) => assert_eq!(path.as_str(), "C:\\Windows\\path"),
			other => panic!("expected Fs variant, got {:?}", other),
		}
	}

	#[test]
	fn env_key_read_becomes_generic_variant() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::EnvKeyRead {
				key_name: "CACHE_DIR".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).expect("valid env-key payload");
		assert!(matches!(adapted.logical_name, CallsiteLogicalName::Generic(_)));
		assert_eq!(adapted.logical_name_source, LogicalNameSource::EnvKey);
	}

	#[test]
	fn imports_in_file_is_synthetic_single_entry() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "/x".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).unwrap();
		assert_eq!(adapted.imports_in_file.len(), 1);
		assert_eq!(adapted.imports_in_file[0].module_path, "fs");
		assert_eq!(adapted.imports_in_file[0].imported_symbol, "readFile");
		assert!(adapted.imports_in_file[0].import_alias.is_none());
	}

	#[test]
	fn url_shaped_literal_classified_as_normalized_url() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "file:///etc/config".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).expect("valid URL payload");
		assert_eq!(
			adapted.logical_name_source,
			LogicalNameSource::NormalizedUrl,
			"file:/// must map to NormalizedUrl"
		);
		match &adapted.logical_name {
			CallsiteLogicalName::Fs(path) => {
				assert_eq!(path.as_str(), "file:///etc/config")
			}
			other => panic!("expected Fs variant, got {:?}", other),
		}
	}

	#[test]
	fn windows_drive_letter_is_not_classified_as_url() {
		// `C:\Windows\path` contains `:` but NOT `://`. Must
		// remain `NormalizedPath`.
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "C:\\Windows\\path".to_string(),
			},
			source_location: loc(),
		};
		let adapted = adapt_resolved_callsite(&rc).unwrap();
		assert_eq!(
			adapted.logical_name_source,
			LogicalNameSource::NormalizedPath,
			"Windows drive letter must stay NormalizedPath"
		);
	}

	#[test]
	fn is_url_shaped_classifier_cases() {
		// Positive cases.
		assert!(is_url_shaped("file:///etc/config"));
		assert!(is_url_shaped("http://example.com"));
		assert!(is_url_shaped("https://example.com/x"));
		assert!(is_url_shaped("s3://bucket/key"));
		assert!(is_url_shaped("ftp://host/file"));
		assert!(is_url_shaped("git+ssh://host/repo"));
		// Negative cases.
		assert!(!is_url_shaped("/etc/config"));
		assert!(!is_url_shaped("./rel/path"));
		assert!(!is_url_shaped("../up/one"));
		assert!(!is_url_shaped("C:\\Windows\\path"));
		assert!(!is_url_shaped("D:/forward/slashes"), "single colon is not URL");
		assert!(!is_url_shaped("://no-scheme"));
		assert!(!is_url_shaped("1scheme://bad"), "scheme must start with letter");
		assert!(!is_url_shaped("scheme_under://bad"), "underscore not allowed in scheme");
		assert!(!is_url_shaped(""));
		assert!(!is_url_shaped("just-a-name"));
	}

	#[test]
	fn empty_string_literal_payload_is_skipped() {
		let rc = ResolvedCallsite {
			enclosing_symbol_node_uid: "sym-1".to_string(),
			resolved_module: "fs".to_string(),
			resolved_symbol: "readFile".to_string(),
			arg0_payload: Arg0Payload::StringLiteral {
				value: "".to_string(),
			},
			source_location: loc(),
		};
		assert!(adapt_resolved_callsite(&rc).is_none());
	}
}
