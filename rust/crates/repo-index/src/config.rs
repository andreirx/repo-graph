//! Config readers — package.json dependencies and tsconfig.json
//! path aliases, with nearest-ancestor directory lookup.
//!
//! Mirrors the TS indexer's `resolveNearestPackageDeps` and
//! `resolveNearestTsconfigAliases` from `repo-indexer.ts`.
//!
//! Lookup rule (locked): walk from file's parent directory upward
//! to repo root. First matching config file wins. Cached by
//! directory so sibling files resolve in O(1).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use repo_graph_classification::types::{PackageDependencySet, TsconfigAliasEntry, TsconfigAliases};

/// Pre-computed config context for a repo. Caches config lookups
/// by directory so each directory is resolved at most once.
pub struct RepoConfigContext {
	/// Directory → PackageDependencySet cache.
	pkg_cache: HashMap<String, PackageDependencySet>,
	/// Directory → TsconfigAliases cache.
	tsconfig_cache: HashMap<String, TsconfigAliases>,
}

impl RepoConfigContext {
	/// Build config context by pre-scanning the repo root.
	/// The actual per-directory resolution is lazy (on first lookup).
	pub fn new() -> Self {
		Self {
			pkg_cache: HashMap::new(),
			tsconfig_cache: HashMap::new(),
		}
	}

	/// Resolve package dependencies for a file.
	/// Walks from file's directory upward to repo root.
	pub fn resolve_package_deps(
		&mut self,
		file_rel_path: &str,
		repo_root: &Path,
	) -> PackageDependencySet {
		let empty = PackageDependencySet { names: vec![] };
		let dir = parent_dir(file_rel_path);

		// Check cache chain upward.
		let mut probe = dir.clone();
		loop {
			if let Some(cached) = self.pkg_cache.get(&probe) {
				// Backfill cache for unchecked dirs.
				let result = cached.clone();
				self.pkg_cache.insert(dir.clone(), result.clone());
				return result;
			}

			// Try reading package.json at this directory.
			// TS behavior: if the file EXISTS, stop here regardless of
			// parse success. Extract deps if parseable, else empty.
			// A broken leaf manifest should NOT inherit parent deps.
			let abs_dir = if probe.is_empty() {
				repo_root.to_path_buf()
			} else {
				repo_root.join(&probe)
			};
			let pkg_path = abs_dir.join("package.json");
			if pkg_path.exists() {
				let deps = std::fs::read_to_string(&pkg_path)
					.ok()
					.and_then(|content| extract_package_dependencies(&content))
					.unwrap_or_else(|| empty.clone());
				self.pkg_cache.insert(probe.clone(), deps.clone());
				self.pkg_cache.insert(dir.clone(), deps.clone());
				return deps;
			}

			if probe.is_empty() {
				break;
			}
			probe = parent_dir(&probe);
		}

		self.pkg_cache.insert(dir, empty.clone());
		empty
	}

	/// Resolve tsconfig aliases for a file.
	/// Walks from file's directory upward to repo root.
	pub fn resolve_tsconfig_aliases(
		&mut self,
		file_rel_path: &str,
		repo_root: &Path,
	) -> TsconfigAliases {
		let empty = TsconfigAliases { entries: vec![] };
		let dir = parent_dir(file_rel_path);

		let mut probe = dir.clone();
		loop {
			if let Some(cached) = self.tsconfig_cache.get(&probe) {
				let result = cached.clone();
				self.tsconfig_cache.insert(dir.clone(), result.clone());
				return result;
			}

			let abs_dir = if probe.is_empty() {
				repo_root.to_path_buf()
			} else {
				repo_root.join(&probe)
			};
			let tsconfig_path = abs_dir.join("tsconfig.json");
			if tsconfig_path.exists() {
				let aliases = read_tsconfig_aliases_from_path(&tsconfig_path)
					.unwrap_or_else(|| empty.clone());
				self.tsconfig_cache.insert(probe.clone(), aliases.clone());
				self.tsconfig_cache.insert(dir.clone(), aliases.clone());
				return aliases;
			}

			if probe.is_empty() {
				break;
			}
			probe = parent_dir(&probe);
		}

		self.tsconfig_cache.insert(dir, empty.clone());
		empty
	}
}

/// Get the parent directory of a repo-relative path.
fn parent_dir(rel_path: &str) -> String {
	match rel_path.rfind('/') {
		Some(pos) => rel_path[..pos].to_string(),
		None => String::new(), // Root directory.
	}
}

// ── Package.json reader ──────────────────────────────────────────

/// Extract dependency names from package.json content.
/// Reads dependencies, devDependencies, peerDependencies,
/// optionalDependencies. Returns sorted unique names.
///
/// Mirrors TS `extractPackageDependencies` from `package-json.ts:67`.
pub fn extract_package_dependencies(content: &str) -> Option<PackageDependencySet> {
	let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
	let obj = parsed.as_object()?;

	let mut names = BTreeSet::new();
	for field in &["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
		if let Some(serde_json::Value::Object(deps)) = obj.get(*field) {
			for key in deps.keys() {
				names.insert(key.clone());
			}
		}
	}

	Some(PackageDependencySet {
		names: names.into_iter().collect(),
	})
}

// ── Tsconfig.json reader ─────────────────────────────────────────

const MAX_EXTENDS_DEPTH: usize = 10;

/// Strip JSONC comments (// line and /* block */) from source.
///
/// **Locked divergence from TS:** The TS reader uses a conservative
/// regex that does not distinguish comment markers inside string
/// values. This Rust scanner correctly handles strings, so it is a
/// strict superset: any input that parses under TS also parses here,
/// but inputs with `//` or `/*` inside string values parse correctly
/// in Rust and may break in TS. This is accepted as a safe
/// improvement — it cannot produce fewer aliases than TS, only more
/// (and only for pathological inputs with comment syntax in strings).
fn strip_json_comments(source: &str) -> String {
	let mut result = String::with_capacity(source.len());
	let mut chars = source.chars().peekable();
	let mut in_string = false;
	let mut escape_next = false;

	while let Some(ch) = chars.next() {
		if escape_next {
			result.push(ch);
			escape_next = false;
			continue;
		}
		if in_string {
			if ch == '\\' {
				escape_next = true;
			} else if ch == '"' {
				in_string = false;
			}
			result.push(ch);
			continue;
		}
		if ch == '"' {
			in_string = true;
			result.push(ch);
			continue;
		}
		if ch == '/' {
			match chars.peek() {
				Some('/') => {
					// Line comment — skip to end of line.
					for c in chars.by_ref() {
						if c == '\n' {
							result.push('\n');
							break;
						}
					}
					continue;
				}
				Some('*') => {
					// Block comment — skip to */.
					chars.next(); // consume *
					let mut prev = ' ';
					for c in chars.by_ref() {
						if prev == '*' && c == '/' {
							break;
						}
						prev = c;
					}
					continue;
				}
				_ => {}
			}
		}
		result.push(ch);
	}
	result
}

/// Read tsconfig.json at `path`, following extends chains.
/// Returns the effective TsconfigAliases, or None if file missing/unparseable.
pub fn read_tsconfig_aliases_from_path(path: &Path) -> Option<TsconfigAliases> {
	let empty = TsconfigAliases { entries: vec![] };
	let mut visited = std::collections::HashSet::new();
	let mut current = path.to_path_buf();

	for depth in 0..MAX_EXTENDS_DEPTH {
		let canonical = current.canonicalize().unwrap_or_else(|_| current.clone());
		if visited.contains(&canonical) {
			break; // Circular.
		}
		visited.insert(canonical.clone());

		let raw = match std::fs::read_to_string(&current) {
			Ok(c) => c,
			Err(_) => {
				return if depth == 0 { None } else { Some(empty) };
			}
		};

		let stripped = strip_json_comments(&raw);
		let parsed: serde_json::Value = match serde_json::from_str(&stripped) {
			Ok(v) => v,
			Err(_) => {
				return if depth == 0 { None } else { Some(empty) };
			}
		};

		// Check for compilerOptions.paths.
		if let Some(paths) = parsed
			.get("compilerOptions")
			.and_then(|co| co.get("paths"))
			.and_then(|p| p.as_object())
		{
			let entries: Vec<TsconfigAliasEntry> = paths
				.iter()
				.map(|(pattern, subs)| {
					let substitutions = subs
						.as_array()
						.map(|arr| {
							arr.iter()
								.filter_map(|s| s.as_str().map(|s| s.to_string()))
								.collect()
						})
						.unwrap_or_default();
					TsconfigAliasEntry {
						pattern: pattern.clone(),
						substitutions,
					}
				})
				.collect();
			return Some(TsconfigAliases { entries });
		}

		// Follow extends.
		let extends = match parsed.get("extends").and_then(|e| e.as_str()) {
			Some(e) => e.to_string(),
			None => return Some(empty),
		};

		// Only follow relative extends paths.
		if !extends.starts_with('.') && !extends.starts_with('/') {
			return Some(empty);
		}

		let parent_dir_path = current.parent().unwrap_or(Path::new(""));
		let mut next = parent_dir_path.join(&extends);
		if !next.extension().map(|e| e == "json").unwrap_or(false) {
			next.set_extension("json");
		}
		current = next;
	}

	Some(empty)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	// ── extract_package_dependencies ─────────────────────────

	#[test]
	fn extracts_all_dep_fields() {
		let content = r#"{
			"dependencies": {"express": "^4.18.0"},
			"devDependencies": {"vitest": "^1.0.0"},
			"peerDependencies": {"react": "^18.0.0"},
			"optionalDependencies": {"fsevents": "^2.3.0"}
		}"#;
		let deps = extract_package_dependencies(content).unwrap();
		assert_eq!(deps.names, vec!["express", "fsevents", "react", "vitest"]);
	}

	#[test]
	fn returns_sorted_unique_names() {
		let content = r#"{
			"dependencies": {"b-pkg": "1", "a-pkg": "2"},
			"devDependencies": {"a-pkg": "3"}
		}"#;
		let deps = extract_package_dependencies(content).unwrap();
		assert_eq!(deps.names, vec!["a-pkg", "b-pkg"]);
	}

	#[test]
	fn returns_none_on_invalid_json() {
		assert!(extract_package_dependencies("{invalid").is_none());
	}

	// ── strip_json_comments ──────────────────────────────────

	#[test]
	fn strips_line_comments() {
		let input = "{\n  // comment\n  \"key\": 1\n}";
		let stripped = strip_json_comments(input);
		let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
		assert_eq!(parsed["key"], 1);
	}

	#[test]
	fn strips_block_comments() {
		let input = "{ /* block */ \"key\": 1 }";
		let stripped = strip_json_comments(input);
		let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
		assert_eq!(parsed["key"], 1);
	}

	// ── read_tsconfig_aliases_from_path ───────────────────────

	#[test]
	fn reads_paths_from_tsconfig() {
		let dir = tempfile::tempdir().unwrap();
		let tsconfig = dir.path().join("tsconfig.json");
		fs::write(&tsconfig, r#"{
			"compilerOptions": {
				"paths": {
					"@/*": ["./src/*"],
					"@lib/*": ["./lib/*"]
				}
			}
		}"#).unwrap();

		let aliases = read_tsconfig_aliases_from_path(&tsconfig).unwrap();
		assert_eq!(aliases.entries.len(), 2);
		let at = aliases.entries.iter().find(|e| e.pattern == "@/*").unwrap();
		assert_eq!(at.substitutions, vec!["./src/*"]);
	}

	#[test]
	fn follows_extends_chain() {
		let dir = tempfile::tempdir().unwrap();
		let base = dir.path().join("base.json");
		fs::write(&base, r#"{
			"compilerOptions": {
				"paths": { "@/*": ["./src/*"] }
			}
		}"#).unwrap();

		let child = dir.path().join("tsconfig.json");
		fs::write(&child, r#"{ "extends": "./base.json" }"#).unwrap();

		let aliases = read_tsconfig_aliases_from_path(&child).unwrap();
		assert_eq!(aliases.entries.len(), 1);
		assert_eq!(aliases.entries[0].pattern, "@/*");
	}

	#[test]
	fn returns_none_for_missing_file() {
		let result = read_tsconfig_aliases_from_path(Path::new("/nonexistent/tsconfig.json"));
		assert!(result.is_none());
	}

	#[test]
	fn handles_jsonc_comments_in_tsconfig() {
		let dir = tempfile::tempdir().unwrap();
		let tsconfig = dir.path().join("tsconfig.json");
		fs::write(&tsconfig, r#"{
			// This is a comment
			"compilerOptions": {
				/* block comment */
				"paths": { "@/*": ["./src/*"] }
			}
		}"#).unwrap();

		let aliases = read_tsconfig_aliases_from_path(&tsconfig).unwrap();
		assert_eq!(aliases.entries.len(), 1);
	}

	// ── RepoConfigContext ────────────────────────────────────

	#[test]
	fn nearest_ancestor_package_json() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		// Root package.json.
		fs::write(root.join("package.json"), r#"{"dependencies":{"express":"1"}}"#).unwrap();
		// Nested package.json.
		fs::create_dir_all(root.join("packages/web")).unwrap();
		fs::write(
			root.join("packages/web/package.json"),
			r#"{"dependencies":{"react":"18"}}"#,
		).unwrap();

		let mut ctx = RepoConfigContext::new();

		// File in root → gets root deps.
		let root_deps = ctx.resolve_package_deps("src/index.ts", root);
		assert_eq!(root_deps.names, vec!["express"]);

		// File in packages/web → gets nested deps.
		let web_deps = ctx.resolve_package_deps("packages/web/src/App.tsx", root);
		assert_eq!(web_deps.names, vec!["react"]);
	}

	#[test]
	fn malformed_package_json_stops_walk_returns_empty() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		// Root has valid deps.
		fs::write(root.join("package.json"), r#"{"dependencies":{"express":"1"}}"#).unwrap();
		// Nested has malformed package.json.
		fs::create_dir_all(root.join("packages/broken")).unwrap();
		fs::write(root.join("packages/broken/package.json"), "{invalid json}").unwrap();

		let mut ctx = RepoConfigContext::new();
		// File under broken → should get empty deps (malformed stops walk),
		// NOT inherit root's "express".
		let deps = ctx.resolve_package_deps("packages/broken/src/index.ts", root);
		assert!(
			deps.names.is_empty(),
			"malformed package.json should stop walk with empty deps, got {:?}",
			deps.names
		);
	}

	#[test]
	fn nearest_ancestor_tsconfig() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::write(root.join("tsconfig.json"), r#"{
			"compilerOptions": { "paths": { "@/*": ["./src/*"] } }
		}"#).unwrap();

		let mut ctx = RepoConfigContext::new();
		let aliases = ctx.resolve_tsconfig_aliases("src/index.ts", root);
		assert_eq!(aliases.entries.len(), 1);
		assert_eq!(aliases.entries[0].pattern, "@/*");
	}
}
