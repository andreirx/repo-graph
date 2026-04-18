//! Form-A matcher primitives.
//!
//! Contract §6.1 defines Form A as the only matcher form permitted
//! in slice 1:
//!
//!   1. The file containing the call site imports module `M`.
//!   2. The call-site callee resolves to a symbol path matching a
//!      binding entry keyed on module `M` and symbol path `S`.
//!   3. The resolution basis is one of {stdlib_api, sdk_call}
//!      (enforced at the binding-table layer, not here).
//!
//! Inputs (SB-1.5 independent-view-struct lock):
//!
//!   - `ImportView` — minimal view of a file's import binding that
//!     the matcher needs: module_path, imported_symbol, optional
//!     local alias.
//!   - `CalleePath` — the extractor-resolved callee expressed in
//!     terms the matcher can compare against a binding's
//!     (module, symbol_path) pair.
//!
//! The matcher returns `Option<MatchResult>` — some when all three
//! Form-A conditions hold, none otherwise. No side effects. No
//! graph emission (that is state-extractor's job in a later slice).
//!
//! This module does NOT depend on any extractor crate. The
//! extractor adapts its own import-binding and callee-resolution
//! types into `ImportView` / `CalleePath` and consumes the match
//! result.

use crate::table::{BindingEntry, BindingTable, Language};

// ── Input view types ──────────────────────────────────────────────

/// The matcher's view of one import binding in the calling file.
///
/// The extractor is responsible for surfacing:
///
/// - `module_path`: the module as it appears in the import
///   (e.g. `"@aws-sdk/client-s3"`, `"fs"`, `"std::fs"`).
/// - `imported_symbol`: the original exported symbol being brought
///   into scope (e.g. `"PutObjectCommand"`).
/// - `import_alias`: the local name the symbol is bound to, if the
///   import is aliased (`import { X as Y }` → `Some("Y")`). `None`
///   means the local name equals `imported_symbol`.
///
/// Alias resolution happens in the extractor BEFORE constructing
/// the matcher's `CalleePath` — the matcher itself does not walk
/// aliases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportView {
	/// Module path as it appears in the source-file import.
	pub module_path: String,
	/// The exported symbol brought in by the import.
	pub imported_symbol: String,
	/// Local alias, if any.
	pub import_alias: Option<String>,
}

/// The matcher's view of a resolved callee at a call site.
///
/// The extractor is responsible for walking aliases, namespace
/// imports, and re-exports to produce:
///
/// - `resolved_module`: the module the callee ultimately comes
///   from, if the extractor can resolve it. `None` means the
///   callee's origin could not be traced — the matcher will not
///   match under Form A in that case.
/// - `resolved_symbol`: the symbol path within `resolved_module`
///   that the callee corresponds to. Compared against a binding's
///   `symbol_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleePath {
	/// Resolved source module of the callee, or `None` if the
	/// extractor could not trace it.
	pub resolved_module: Option<String>,
	/// Resolved symbol path within `resolved_module`.
	pub resolved_symbol: String,
}

// ── Match result ──────────────────────────────────────────────────

/// Outcome of a successful Form-A match.
///
/// Borrow into the `BindingTable` is preserved so the caller can
/// read every binding field (direction, basis, resource_kind,
/// driver, notes) without a secondary lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult<'t> {
	/// The matched binding entry.
	pub binding: &'t BindingEntry,
	/// The canonical binding key for edge-evidence use, per
	/// contract §8.
	pub binding_key: String,
}

// ── Matcher entry point ───────────────────────────────────────────

/// Run the Form-A matcher against a single call site.
///
/// Returns `Some(MatchResult)` when all three Form-A conditions
/// hold for the given language, otherwise `None`.
///
/// Arguments:
///
///   - `imports_in_file`: every `ImportView` present in the file
///     containing the call site. Order does not matter; the
///     matcher scans for any matching `module_path`.
///   - `callee`: the resolved callee at the call site.
///   - `table`: the validated binding table.
///   - `language`: the language of the source file. Bindings for
///     other languages are ignored even if their module /
///     symbol_path happens to match.
pub fn match_form_a<'t>(
	imports_in_file: &[ImportView],
	callee: &CalleePath,
	table: &'t BindingTable,
	language: Language,
) -> Option<MatchResult<'t>> {
	// Form-A condition 2: callee must be resolvable to a module.
	let resolved_module = callee.resolved_module.as_ref()?;

	// Form-A condition 1: the file must import that module.
	let has_import = imports_in_file
		.iter()
		.any(|imp| imp.module_path == *resolved_module);
	if !has_import {
		return None;
	}

	// Form-A condition 2 (cont.): symbol_path match against
	// a binding entry for the same language.
	for binding in table.entries() {
		if binding.language != language {
			continue;
		}
		if binding.module != *resolved_module {
			continue;
		}
		if binding.symbol_path != callee.resolved_symbol {
			continue;
		}
		return Some(MatchResult {
			binding_key: binding.binding_key(),
			binding,
		});
	}

	None
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Build a single-entry probe table for inline matcher smoke
	/// coverage. Cross-cutting match / no-match cases live in
	/// `tests/matcher.rs`.
	fn probe_table() -> BindingTable {
		// The embedded bindings.toml ships exactly the probe
		// entry; reuse it rather than duplicating TOML in tests.
		BindingTable::load_embedded().clone()
	}

	#[test]
	fn matches_probe_entry_when_import_and_callee_align() {
		let table = probe_table();
		let imports = vec![ImportView {
			module_path: "@aws-sdk/client-s3".to_string(),
			imported_symbol: "PutObjectCommand".to_string(),
			import_alias: None,
		}];
		let callee = CalleePath {
			resolved_module: Some("@aws-sdk/client-s3".to_string()),
			resolved_symbol: "PutObjectCommand".to_string(),
		};
		let result = match_form_a(&imports, &callee, &table, Language::Typescript);
		let m = result.expect("probe entry must match");
		assert_eq!(m.binding.driver, "s3");
		assert_eq!(
			m.binding_key,
			"typescript:@aws-sdk/client-s3:PutObjectCommand:write"
		);
	}

	#[test]
	fn no_match_without_import() {
		let table = probe_table();
		let imports: Vec<ImportView> = vec![];
		let callee = CalleePath {
			resolved_module: Some("@aws-sdk/client-s3".to_string()),
			resolved_symbol: "PutObjectCommand".to_string(),
		};
		assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
	}

	#[test]
	fn no_match_when_callee_not_resolved() {
		let table = probe_table();
		let imports = vec![ImportView {
			module_path: "@aws-sdk/client-s3".to_string(),
			imported_symbol: "PutObjectCommand".to_string(),
			import_alias: None,
		}];
		let callee = CalleePath {
			resolved_module: None,
			resolved_symbol: "PutObjectCommand".to_string(),
		};
		assert!(match_form_a(&imports, &callee, &table, Language::Typescript).is_none());
	}
}
