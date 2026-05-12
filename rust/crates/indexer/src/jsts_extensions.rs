//! FD-SUPPORT-EXT-JSTS: Unified JS/TS extension contract.
//!
//! This module defines the canonical set of JavaScript/TypeScript file extensions
//! and provides utilities for extension classification and grammar selection.
//!
//! All components handling JS/TS files should use these utilities instead of
//! hardcoding extension checks, ensuring consistent behavior across:
//! - Routing (`routing.rs`)
//! - TS extractor (`ts-extractor`)
//! - Express detector (`express_detector.rs`)
//! - React detector (`react_detector.rs`)
//!
//! # Extension Families
//!
//! **Core family:** `.ts`, `.tsx`, `.js`, `.jsx`
//!
//! **Extended family:** `.mts`, `.cts`, `.mjs`, `.cjs`
//! - `.mts` / `.mjs` — ES Module (explicit)
//! - `.cts` / `.cjs` — CommonJS (explicit)
//!
//! # Grammar Selection
//!
//! - **TSX grammar:** `.tsx`, `.jsx` — includes JSX syntax support
//! - **TS grammar:** all others — standard TypeScript/JavaScript
//!
//! Note: Plain `.ts`/`.js` files containing JSX (via pragma) are NOT handled
//! by grammar switching. JSX pragma detection is out of scope.

/// Canonical JS/TS extension family — all extensions this ecosystem handles.
///
/// Includes both core extensions (`.ts`, `.tsx`, `.js`, `.jsx`) and extended
/// family (`.mts`, `.cts`, `.mjs`, `.cjs`).
pub const JSTS_EXTENSIONS: &[&str] = &[
	".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs",
];

/// Extensions that require TSX grammar (JSX syntax support).
///
/// Only `.tsx` and `.jsx` include JSX syntax in the grammar.
/// Other extensions use the standard TS grammar.
pub const JSTS_JSX_EXTENSIONS: &[&str] = &[".tsx", ".jsx"];

/// Core extensions — the most common JS/TS file types.
pub const JSTS_CORE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx"];

/// Extended family — ES Module and CommonJS explicit extensions.
pub const JSTS_EXTENDED_EXTENSIONS: &[&str] = &[".mts", ".cts", ".mjs", ".cjs"];

/// Grammar selection for JS/TS files.
///
/// The grammar determines which tree-sitter language to use for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsTsGrammar {
	/// Standard TypeScript grammar — for `.ts`, `.mts`, `.cts`, `.js`, `.mjs`, `.cjs`
	TypeScript,
	/// TSX grammar — for `.tsx`, `.jsx` (includes JSX syntax support)
	Tsx,
}

/// Check whether an extension is in the JS/TS family.
///
/// Returns `true` for any of the 8 canonical extensions.
///
/// # Example
///
/// ```
/// use repo_graph_indexer::jsts_extensions::is_jsts_extension;
///
/// assert!(is_jsts_extension(".ts"));
/// assert!(is_jsts_extension(".tsx"));
/// assert!(is_jsts_extension(".mjs"));
/// assert!(!is_jsts_extension(".rs"));
/// assert!(!is_jsts_extension(".py"));
/// ```
pub fn is_jsts_extension(ext: &str) -> bool {
	JSTS_EXTENSIONS.contains(&ext)
}

/// Check whether an extension requires the TSX grammar.
///
/// Returns `true` only for `.tsx` and `.jsx`.
///
/// # Example
///
/// ```
/// use repo_graph_indexer::jsts_extensions::is_jsts_jsx_extension;
///
/// assert!(is_jsts_jsx_extension(".tsx"));
/// assert!(is_jsts_jsx_extension(".jsx"));
/// assert!(!is_jsts_jsx_extension(".ts"));
/// assert!(!is_jsts_jsx_extension(".mts"));
/// ```
pub fn is_jsts_jsx_extension(ext: &str) -> bool {
	JSTS_JSX_EXTENSIONS.contains(&ext)
}

/// Check whether an extension is in the core JS/TS family.
///
/// Core extensions are the most common: `.ts`, `.tsx`, `.js`, `.jsx`.
pub fn is_jsts_core_extension(ext: &str) -> bool {
	JSTS_CORE_EXTENSIONS.contains(&ext)
}

/// Check whether an extension is in the extended JS/TS family.
///
/// Extended extensions are ES Module and CommonJS explicit types:
/// `.mts`, `.cts`, `.mjs`, `.cjs`.
pub fn is_jsts_extended_extension(ext: &str) -> bool {
	JSTS_EXTENDED_EXTENSIONS.contains(&ext)
}

/// Get the appropriate grammar for a JS/TS file extension.
///
/// Returns `Some(JsTsGrammar::Tsx)` for `.tsx`/`.jsx`,
/// `Some(JsTsGrammar::TypeScript)` for other JS/TS extensions,
/// `None` for non-JS/TS extensions.
///
/// # Example
///
/// ```
/// use repo_graph_indexer::jsts_extensions::{grammar_for_extension, JsTsGrammar};
///
/// assert_eq!(grammar_for_extension(".tsx"), Some(JsTsGrammar::Tsx));
/// assert_eq!(grammar_for_extension(".jsx"), Some(JsTsGrammar::Tsx));
/// assert_eq!(grammar_for_extension(".ts"), Some(JsTsGrammar::TypeScript));
/// assert_eq!(grammar_for_extension(".mjs"), Some(JsTsGrammar::TypeScript));
/// assert_eq!(grammar_for_extension(".rs"), None);
/// ```
pub fn grammar_for_extension(ext: &str) -> Option<JsTsGrammar> {
	match ext {
		".tsx" | ".jsx" => Some(JsTsGrammar::Tsx),
		".ts" | ".mts" | ".cts" | ".js" | ".mjs" | ".cjs" => Some(JsTsGrammar::TypeScript),
		_ => None,
	}
}

/// Extract the file extension (including the leading dot) from a file path.
///
/// Returns an empty string if no extension is found.
///
/// # Example
///
/// ```
/// use repo_graph_indexer::jsts_extensions::get_extension;
///
/// assert_eq!(get_extension("src/App.tsx"), ".tsx");
/// assert_eq!(get_extension("utils.mjs"), ".mjs");
/// assert_eq!(get_extension("Makefile"), "");
/// ```
pub fn get_extension(file_path: &str) -> &str {
	match file_path.rfind('.') {
		Some(pos) => &file_path[pos..],
		None => "",
	}
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	// ── is_jsts_extension ────────────────────────────────────────

	#[test]
	fn is_jsts_extension_core_ts() {
		assert!(is_jsts_extension(".ts"));
	}

	#[test]
	fn is_jsts_extension_core_tsx() {
		assert!(is_jsts_extension(".tsx"));
	}

	#[test]
	fn is_jsts_extension_core_js() {
		assert!(is_jsts_extension(".js"));
	}

	#[test]
	fn is_jsts_extension_core_jsx() {
		assert!(is_jsts_extension(".jsx"));
	}

	#[test]
	fn is_jsts_extension_extended_mts() {
		assert!(is_jsts_extension(".mts"));
	}

	#[test]
	fn is_jsts_extension_extended_cts() {
		assert!(is_jsts_extension(".cts"));
	}

	#[test]
	fn is_jsts_extension_extended_mjs() {
		assert!(is_jsts_extension(".mjs"));
	}

	#[test]
	fn is_jsts_extension_extended_cjs() {
		assert!(is_jsts_extension(".cjs"));
	}

	#[test]
	fn is_jsts_extension_non_jsts_rs() {
		assert!(!is_jsts_extension(".rs"));
	}

	#[test]
	fn is_jsts_extension_non_jsts_py() {
		assert!(!is_jsts_extension(".py"));
	}

	#[test]
	fn is_jsts_extension_non_jsts_java() {
		assert!(!is_jsts_extension(".java"));
	}

	// ── is_jsts_jsx_extension ────────────────────────────────────

	#[test]
	fn is_jsts_jsx_extension_tsx() {
		assert!(is_jsts_jsx_extension(".tsx"));
	}

	#[test]
	fn is_jsts_jsx_extension_jsx() {
		assert!(is_jsts_jsx_extension(".jsx"));
	}

	#[test]
	fn is_jsts_jsx_extension_ts_not_jsx() {
		assert!(!is_jsts_jsx_extension(".ts"));
	}

	#[test]
	fn is_jsts_jsx_extension_js_not_jsx() {
		assert!(!is_jsts_jsx_extension(".js"));
	}

	#[test]
	fn is_jsts_jsx_extension_mts_not_jsx() {
		assert!(!is_jsts_jsx_extension(".mts"));
	}

	// ── grammar_for_extension ────────────────────────────────────

	#[test]
	fn grammar_tsx() {
		assert_eq!(grammar_for_extension(".tsx"), Some(JsTsGrammar::Tsx));
	}

	#[test]
	fn grammar_jsx() {
		assert_eq!(grammar_for_extension(".jsx"), Some(JsTsGrammar::Tsx));
	}

	#[test]
	fn grammar_ts() {
		assert_eq!(grammar_for_extension(".ts"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_js() {
		assert_eq!(grammar_for_extension(".js"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_mts() {
		assert_eq!(grammar_for_extension(".mts"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_cts() {
		assert_eq!(grammar_for_extension(".cts"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_mjs() {
		assert_eq!(grammar_for_extension(".mjs"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_cjs() {
		assert_eq!(grammar_for_extension(".cjs"), Some(JsTsGrammar::TypeScript));
	}

	#[test]
	fn grammar_non_jsts() {
		assert_eq!(grammar_for_extension(".rs"), None);
		assert_eq!(grammar_for_extension(".py"), None);
		assert_eq!(grammar_for_extension(".java"), None);
	}

	// ── get_extension ────────────────────────────────────────────

	#[test]
	fn get_extension_tsx() {
		assert_eq!(get_extension("src/App.tsx"), ".tsx");
	}

	#[test]
	fn get_extension_mjs() {
		assert_eq!(get_extension("utils.mjs"), ".mjs");
	}

	#[test]
	fn get_extension_no_dot() {
		assert_eq!(get_extension("Makefile"), "");
	}

	#[test]
	fn get_extension_multiple_dots() {
		assert_eq!(get_extension("src/app.test.ts"), ".ts");
	}

	// ── is_jsts_core_extension / is_jsts_extended_extension ──────

	#[test]
	fn core_vs_extended_ts() {
		assert!(is_jsts_core_extension(".ts"));
		assert!(!is_jsts_extended_extension(".ts"));
	}

	#[test]
	fn core_vs_extended_mts() {
		assert!(!is_jsts_core_extension(".mts"));
		assert!(is_jsts_extended_extension(".mts"));
	}
}
