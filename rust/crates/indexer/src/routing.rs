//! File routing policy — pure deterministic functions.
//!
//! This module handles language detection, extractor selection,
//! test-file classification, and include/exclude filtering.
//! All functions are pure (no I/O, no filesystem access).
//!
//! Mirror of the inline routing functions in
//! `src/adapters/indexer/repo-indexer.ts`.
//!
//! The caller (orchestrator or external driver) provides file
//! paths and directory names. This module applies policy rules
//! to determine which files should be indexed, which extractor
//! handles each file, and what metadata to attach.

// ── Constants ────────────────────────────────────────────────────

/// Maximum file size (bytes) before a file is skipped as oversized.
/// Mirror of `MAX_FILE_SIZE_BYTES` from `repo-indexer.ts:219`.
pub const MAX_FILE_SIZE_BYTES: usize = 1_000_000;

/// Check whether a file extension is in the supported source set.
/// Mirror of `ALL_SOURCE_EXTENSIONS` from `repo-indexer.ts:174`.
pub fn is_source_extension(ext: &str) -> bool {
	matches!(
		ext,
		".ts" | ".tsx"
			| ".js" | ".jsx"
			| ".rs"
			| ".java"
			| ".py"
			| ".c" | ".h"
			| ".cpp" | ".hpp"
			| ".cc" | ".cxx"
			| ".hxx"
	)
}

/// Check whether a directory name is always excluded from scanning.
/// Mirror of `ALWAYS_EXCLUDED` from `repo-indexer.ts:222`.
pub fn is_always_excluded_dir(name: &str) -> bool {
	matches!(
		name,
		"node_modules"
			| ".git"
			| "dist"
			| "build"
			| "out"
			| ".next"
			| ".nuxt"
			| "coverage"
			| ".turbo"
			| ".cache"
			| "venv"
			| ".venv"
			| "__pycache__"
			| "cdk.out"
	)
}

// ── Language detection ───────────────────────────────────────────

/// Extract the file extension (including the leading dot) from a
/// file path. Returns `""` if no dot is found.
///
/// Mirror of `getExtension` from `repo-indexer.ts:2921`.
pub fn get_extension(file_path: &str) -> &str {
	match file_path.rfind('.') {
		Some(pos) => &file_path[pos..],
		None => "",
	}
}

/// Detect the language identifier for a file path based on its
/// extension. Returns `None` for unsupported extensions.
///
/// Mirror of `detectLanguage` from `repo-indexer.ts:2888`.
///
/// Note: the TS version only returns non-null for JS/TS family
/// files. The Rust version extends this to C/C++/Java/Rust to
/// support the native extractors. The `language` field on
/// `TrackedFile` is `Some("typescript")`, `Some("rust")`, etc.
/// for supported languages, and `None` for languages without
/// extractors.
pub fn detect_language(file_path: &str) -> Option<&'static str> {
	match get_extension(file_path) {
		".ts" => Some("typescript"),
		".tsx" => Some("tsx"),
		".js" => Some("javascript"),
		".jsx" => Some("jsx"),
		".java" => Some("java"),
		".py" => Some("python"),
		".rs" => Some("rust"),
		".c" | ".h" => Some("c"),
		".cpp" | ".cc" | ".cxx" | ".hpp" | ".hxx" => Some("cpp"),
		_ => None,
	}
}

/// Map a language identifier to its file extensions. Used to build
/// the extension → extractor routing table from each extractor's
/// `languages()` list.
///
/// Mirror of `languageToExtensions` from `repo-indexer.ts:186`.
pub fn language_to_extensions(lang: &str) -> &'static [&'static str] {
	match lang {
		"typescript" => &[".ts"],
		"tsx" => &[".tsx", ".jsx"],
		"javascript" => &[".js"],
		"rust" => &[".rs"],
		"java" => &[".java"],
		"python" => &[".py"],
		"c" => &[".c", ".h"],
		"cpp" => &[".cpp", ".hpp", ".cc", ".cxx", ".hxx"],
		_ => &[],
	}
}

// ── Test-file detection ──────────────────────────────────────────

/// Detect whether a repo-relative file path is a test file.
///
/// Mirror of `isTestFile` from `repo-indexer.ts:2904`.
/// Uses path-pattern conventions common across TS, Python, Java,
/// and Rust test ecosystems.
pub fn is_test_file(file_path: &str) -> bool {
	file_path.contains("__tests__")
		|| file_path.contains(".test.")
		|| file_path.contains(".spec.")
		|| file_path.contains("/test/")
		|| file_path.contains("/tests/")
		|| file_path.starts_with("test/")
		|| file_path.starts_with("tests/")
		|| file_path.starts_with("__tests__/")
}

// ── Exclude / include filtering ──────────────────────────────────

/// Check whether a file should be excluded based on user-provided
/// exclude patterns. Matches against both the file name and the
/// full repo-relative path.
///
/// Mirror of `isExcluded` from `repo-indexer.ts:2981`.
pub fn is_excluded(rel_path: &str, name: &str, exclude_patterns: &[String]) -> bool {
	for pattern in exclude_patterns {
		if pattern == name || pattern == rel_path {
			return true;
		}
		if match_simple_glob(rel_path, pattern) {
			return true;
		}
	}
	false
}

/// Check whether a file should be included based on user-provided
/// include patterns. If the include list is empty, all files pass.
/// Otherwise, at least one pattern must match.
pub fn passes_include_filter(rel_path: &str, include_patterns: &[String]) -> bool {
	if include_patterns.is_empty() {
		return true;
	}
	include_patterns
		.iter()
		.any(|p| match_simple_glob(rel_path, p))
}

/// Simple glob matching. Converts a glob pattern to a match
/// against a file path.
///
/// Supported wildcards:
///   - `*` matches any characters except `/`
///   - `**` matches any characters including `/`
///   - `.` is treated as a literal dot
///
/// Mirror of `matchSimpleGlob` from `repo-indexer.ts:2967`.
/// Implemented without regex to avoid adding a dependency for
/// this constrained pattern language.
pub fn match_simple_glob(file_path: &str, pattern: &str) -> bool {
	glob_match(file_path.as_bytes(), pattern.as_bytes())
}

/// Recursive byte-level glob matcher.
fn glob_match(path: &[u8], pat: &[u8]) -> bool {
	match (pat.first(), path.first()) {
		// Pattern exhausted: path must also be exhausted.
		(None, None) => true,
		(None, Some(_)) => false,

		// `**` — match zero or more characters including `/`.
		(Some(b'*'), _) if pat.len() >= 2 && pat[1] == b'*' => {
			let rest_pat = &pat[2..];
			// Try matching `**` against zero chars, then against
			// progressively longer prefixes of path.
			if glob_match(path, rest_pat) {
				return true;
			}
			for i in 0..path.len() {
				if glob_match(&path[i + 1..], rest_pat) {
					return true;
				}
			}
			false
		}

		// `*` — match zero or more non-`/` characters.
		(Some(b'*'), _) => {
			let rest_pat = &pat[1..];
			// Try matching `*` against zero chars, then against
			// progressively longer non-`/` prefixes.
			if glob_match(path, rest_pat) {
				return true;
			}
			for i in 0..path.len() {
				if path[i] == b'/' {
					break;
				}
				if glob_match(&path[i + 1..], rest_pat) {
					return true;
				}
			}
			false
		}

		// Literal character match.
		(Some(&pc), Some(&fc)) => {
			if pc == fc {
				glob_match(&path[1..], &pat[1..])
			} else {
				false
			}
		}

		// Pattern has characters left but path is exhausted.
		(Some(_), None) => {
			// Only matches if remaining pattern is all `*` or `**`.
			pat.iter().all(|&b| b == b'*')
		}
	}
}

// ── Extractor routing table ──────────────────────────────────────

use std::collections::BTreeMap;

use crate::extractor_port::ExtractorPort;

/// Build a routing table from extension to extractor index.
/// Each extractor's `languages()` list is expanded via
/// `language_to_extensions` to produce the mapping.
///
/// Returns a `BTreeMap` for deterministic iteration. The value
/// is the index into the extractors slice, so the caller can
/// look up the extractor by index.
///
/// If two extractors claim the same extension, the later one wins
/// (matching TS behavior where the last-registered extractor takes
/// priority for overlapping extensions).
pub fn build_extension_routing_table(
	extractors: &[&dyn ExtractorPort],
) -> BTreeMap<String, usize> {
	let mut table = BTreeMap::new();
	for (idx, ext) in extractors.iter().enumerate() {
		for lang in ext.languages() {
			for file_ext in language_to_extensions(lang) {
				table.insert((*file_ext).to_string(), idx);
			}
		}
	}
	table
}

/// Look up the extractor index for a file path using the routing
/// table built by `build_extension_routing_table`.
pub fn route_file(
	file_path: &str,
	routing_table: &BTreeMap<String, usize>,
) -> Option<usize> {
	let ext = get_extension(file_path);
	routing_table.get(ext).copied()
}

#[cfg(test)]
mod tests {
	use super::*;

	// ── get_extension ────────────────────────────────────────

	#[test]
	fn get_extension_returns_dot_ts() {
		assert_eq!(get_extension("src/core/Foo.ts"), ".ts");
	}

	#[test]
	fn get_extension_returns_empty_for_no_dot() {
		assert_eq!(get_extension("Makefile"), "");
	}

	#[test]
	fn get_extension_handles_multiple_dots() {
		assert_eq!(get_extension("src/app.test.ts"), ".ts");
	}

	// ── detect_language ──────────────────────────────────────

	#[test]
	fn detect_language_typescript() {
		assert_eq!(detect_language("src/index.ts"), Some("typescript"));
	}

	#[test]
	fn detect_language_tsx() {
		assert_eq!(detect_language("src/App.tsx"), Some("tsx"));
	}

	#[test]
	fn detect_language_java() {
		assert_eq!(detect_language("src/main/java/Foo.java"), Some("java"));
	}

	#[test]
	fn detect_language_rust() {
		assert_eq!(detect_language("src/main.rs"), Some("rust"));
	}

	#[test]
	fn detect_language_python() {
		assert_eq!(detect_language("src/app.py"), Some("python"));
	}

	// ── language_to_extensions ────────────────────────────────

	#[test]
	fn language_to_extensions_typescript() {
		assert_eq!(language_to_extensions("typescript"), &[".ts"]);
	}

	#[test]
	fn language_to_extensions_tsx_includes_jsx() {
		assert_eq!(language_to_extensions("tsx"), &[".tsx", ".jsx"]);
	}

	#[test]
	fn language_to_extensions_cpp() {
		assert_eq!(
			language_to_extensions("cpp"),
			&[".cpp", ".hpp", ".cc", ".cxx", ".hxx"]
		);
	}

	#[test]
	fn language_to_extensions_unknown() {
		assert_eq!(language_to_extensions("go"), &[] as &[&str]);
	}

	// ── is_source_extension ──────────────────────────────────

	#[test]
	fn source_extension_ts() {
		assert!(is_source_extension(".ts"));
	}

	#[test]
	fn source_extension_hpp() {
		assert!(is_source_extension(".hpp"));
	}

	#[test]
	fn source_extension_go_not_supported() {
		assert!(!is_source_extension(".go"));
	}

	#[test]
	fn source_extension_md_not_supported() {
		assert!(!is_source_extension(".md"));
	}

	// ── is_always_excluded_dir ────────────────────────────────

	#[test]
	fn excluded_node_modules() {
		assert!(is_always_excluded_dir("node_modules"));
	}

	#[test]
	fn excluded_pycache() {
		assert!(is_always_excluded_dir("__pycache__"));
	}

	#[test]
	fn excluded_cdk_out() {
		assert!(is_always_excluded_dir("cdk.out"));
	}

	#[test]
	fn not_excluded_src() {
		assert!(!is_always_excluded_dir("src"));
	}

	// ── is_test_file ─────────────────────────────────────────

	#[test]
	fn test_file_double_underscores_dir() {
		assert!(is_test_file("src/__tests__/foo.ts"));
	}

	#[test]
	fn test_file_dot_test() {
		assert!(is_test_file("src/app.test.ts"));
	}

	#[test]
	fn test_file_dot_spec() {
		assert!(is_test_file("src/app.spec.ts"));
	}

	#[test]
	fn test_file_top_level_test_dir() {
		assert!(is_test_file("test/unit/foo.ts"));
	}

	#[test]
	fn test_file_top_level_tests_dir() {
		assert!(is_test_file("tests/integration/bar.ts"));
	}

	#[test]
	fn test_file_nested_test_dir() {
		assert!(is_test_file("src/core/test/helper.ts"));
	}

	#[test]
	fn not_test_file_regular_source() {
		assert!(!is_test_file("src/core/service.ts"));
	}

	#[test]
	fn not_test_file_test_in_name_without_dot() {
		// "testing" in the path but no matching pattern.
		assert!(!is_test_file("src/testing-utils.ts"));
	}

	// ── match_simple_glob ────────────────────────────────────

	#[test]
	fn glob_star_matches_single_segment() {
		assert!(match_simple_glob("src/foo.ts", "src/*.ts"));
	}

	#[test]
	fn glob_star_does_not_cross_slash() {
		assert!(!match_simple_glob("src/core/foo.ts", "src/*.ts"));
	}

	#[test]
	fn glob_double_star_crosses_slash() {
		assert!(match_simple_glob("src/core/foo.ts", "**/*.ts"));
	}

	#[test]
	fn glob_exact_match() {
		assert!(match_simple_glob("src/index.ts", "src/index.ts"));
	}

	#[test]
	fn glob_no_match() {
		assert!(!match_simple_glob("src/index.ts", "src/main.ts"));
	}

	#[test]
	fn glob_double_star_at_end() {
		assert!(match_simple_glob("src/core/deep/file.ts", "src/**"));
	}

	// ── is_excluded ──────────────────────────────────────────

	#[test]
	fn excluded_by_exact_name() {
		let patterns = vec!["Makefile".to_string()];
		assert!(is_excluded("Makefile", "Makefile", &patterns));
	}

	#[test]
	fn excluded_by_glob_pattern() {
		let patterns = vec!["**/*.generated.ts".to_string()];
		assert!(is_excluded(
			"src/api/schema.generated.ts",
			"schema.generated.ts",
			&patterns
		));
	}

	#[test]
	fn not_excluded_when_no_patterns_match() {
		let patterns = vec!["*.md".to_string()];
		assert!(!is_excluded("src/index.ts", "index.ts", &patterns));
	}

	// ── passes_include_filter ────────────────────────────────

	#[test]
	fn include_empty_passes_all() {
		assert!(passes_include_filter("src/anything.ts", &[]));
	}

	#[test]
	fn include_matches() {
		let patterns = vec!["src/**".to_string()];
		assert!(passes_include_filter("src/core/foo.ts", &patterns));
	}

	#[test]
	fn include_rejects_non_matching() {
		let patterns = vec!["src/**".to_string()];
		assert!(!passes_include_filter("lib/bar.ts", &patterns));
	}

	// ── build_extension_routing_table ─────────────────────────

	#[test]
	fn routing_table_maps_extensions_to_extractor_index() {
		use crate::extractor_port::{ExtractorError, ExtractorPort};
		use crate::types::ExtractionResult;
		use repo_graph_classification::types::RuntimeBuiltinsSet;

		struct FakeExtractor {
			langs: Vec<String>,
			builtins: RuntimeBuiltinsSet,
		}
		impl FakeExtractor {
			fn new(langs: Vec<String>) -> Self {
				Self {
					langs,
					builtins: RuntimeBuiltinsSet {
						identifiers: vec![],
						module_specifiers: vec![],
					},
				}
			}
		}
		impl ExtractorPort for FakeExtractor {
			fn name(&self) -> &str {
				"fake:1"
			}
			fn languages(&self) -> &[String] {
				&self.langs
			}
			fn runtime_builtins(&self) -> &RuntimeBuiltinsSet {
				&self.builtins
			}
			fn initialize(&mut self) -> Result<(), ExtractorError> {
				Ok(())
			}
			fn extract(
				&self,
				_: &str,
				_: &str,
				_: &str,
				_: &str,
				_: &str,
			) -> Result<ExtractionResult, ExtractorError> {
				unimplemented!()
			}
		}

		let ts_ext = FakeExtractor::new(vec!["typescript".into(), "tsx".into()]);
		let rs_ext = FakeExtractor::new(vec!["rust".into()]);
		let extractors: Vec<&dyn ExtractorPort> = vec![&ts_ext, &rs_ext];
		let table = build_extension_routing_table(&extractors);

		assert_eq!(route_file("src/index.ts", &table), Some(0));
		assert_eq!(route_file("src/App.tsx", &table), Some(0));
		assert_eq!(route_file("src/App.jsx", &table), Some(0));
		assert_eq!(route_file("src/main.rs", &table), Some(1));
		assert_eq!(route_file("src/README.md", &table), None);
	}
}
