//! Filesystem scanner — walks a repo directory, applies filters,
//! reads source files, and computes content hashes.
//!
//! Matches the TS indexer's scanning behavior:
//!   - Root `.gitignore` only (no nested, no `.git` requirement)
//!   - `ALWAYS_EXCLUDED` directory filtering
//!   - `ALL_SOURCE_EXTENSIONS` file filtering
//!   - Unreadable files tracked as `ReadFailed` (not silently dropped)

use std::path::Path;

use repo_graph_indexer::routing;
use sha2::{Digest, Sha256};

/// A source file discovered on disk.
#[derive(Debug, Clone)]
pub enum ScannedFile {
	/// Successfully read file.
	Ok(ScannedFileOk),
	/// File discovered but could not be read (permissions, binary, etc.).
	ReadFailed {
		rel_path: String,
	},
}

/// A successfully scanned source file.
#[derive(Debug, Clone)]
pub struct ScannedFileOk {
	/// Repo-relative path (forward slashes).
	pub rel_path: String,
	/// UTF-8 source text.
	pub content: String,
	/// SHA-256 hex truncated to 16 chars. Matches TS `hashContent`.
	pub content_hash: String,
	/// File size in bytes.
	pub size_bytes: usize,
	/// Line count (TS `source.split("\n").length` convention).
	pub line_count: usize,
	/// Detected language (from routing policy).
	pub language: Option<&'static str>,
	/// Whether this is a test file (from routing policy).
	pub is_test: bool,
}

/// Compute the content hash matching TS `hashContent`:
/// `SHA-256(content_bytes).hex()[0..16]`.
pub fn hash_content(content: &str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(content.as_bytes());
	let digest = hasher.finalize();
	let hex = format!("{:x}", digest);
	hex[..16].to_string()
}

/// Scan a repository directory for source files.
///
/// Walks `repo_path` recursively, applies:
///   1. Root `.gitignore` patterns (no nested, no `.git` required —
///      matches TS `loadGitignore` at repo-indexer.ts:3014)
///   2. `ALWAYS_EXCLUDED` directory filtering (from indexer routing)
///   3. `ALL_SOURCE_EXTENSIONS` file filtering (from indexer routing)
///
/// Unreadable files are returned as `ScannedFile::ReadFailed` so
/// the caller can track them as `ParseStatus::Failed`.
///
/// Returns files sorted by `rel_path` for deterministic ordering.
pub fn scan_repo(repo_path: &Path) -> Result<Vec<ScannedFile>, ScanError> {
	// Load root .gitignore (no nested, no .git requirement).
	let gitignore = load_root_gitignore(repo_path);

	let mut files = Vec::new();

	for entry in walkdir::WalkDir::new(repo_path)
		.follow_links(false)
		.sort_by_file_name()
	{
		let entry = entry.map_err(|e| ScanError {
			message: format!("walk error: {}", e),
		})?;

		let path = entry.path();
		let rel_path = match path.strip_prefix(repo_path) {
			Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
			Err(_) => continue,
		};

		// Skip the root directory itself.
		if rel_path.is_empty() {
			continue;
		}

		if entry.file_type().is_dir() {
			// Check excluded directories. WalkDir doesn't support
			// pruning, but we filter files by path segments below.
			continue;
		}

		// Check excluded directory segments in the path.
		if rel_path.split('/').any(|seg| routing::is_always_excluded_dir(seg)) {
			continue;
		}

		// Check source extension.
		let ext = routing::get_extension(&rel_path);
		if !routing::is_source_extension(ext) {
			continue;
		}

		// Check root gitignore.
		if let Some(ref gi) = gitignore {
			let is_dir = entry.file_type().is_dir();
			if gi.matched_path_or_any_parents(&rel_path, is_dir).is_ignore() {
				continue;
			}
		}

		// Read content.
		match std::fs::read_to_string(path) {
			Ok(content) => {
				let size_bytes = content.len();
				let line_count = content.split('\n').count();
				let content_hash = hash_content(&content);
				let language = routing::detect_language(&rel_path);
				let is_test = routing::is_test_file(&rel_path);

				files.push(ScannedFile::Ok(ScannedFileOk {
					rel_path,
					content,
					content_hash,
					size_bytes,
					line_count,
					language,
					is_test,
				}));
			}
			Err(_) => {
				files.push(ScannedFile::ReadFailed { rel_path });
			}
		}
	}

	Ok(files)
}

/// Load the root `.gitignore` file only. No nested gitignore,
/// no `.git` directory requirement. Matches TS `loadGitignore`.
fn load_root_gitignore(repo_path: &Path) -> Option<ignore::gitignore::Gitignore> {
	let gitignore_path = repo_path.join(".gitignore");
	if !gitignore_path.exists() {
		return None;
	}
	let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_path);
	// add() reads and parses the file.
	if builder.add(&gitignore_path).is_some() {
		return None; // Parse error — treat as no gitignore.
	}
	builder.build().ok()
}

/// Error from the filesystem scanner.
#[derive(Debug)]
pub struct ScanError {
	pub message: String,
}

impl std::fmt::Display for ScanError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "scan error: {}", self.message)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	/// Extract only Ok files for convenience.
	fn ok_files(files: &[ScannedFile]) -> Vec<&ScannedFileOk> {
		files.iter().filter_map(|f| match f {
			ScannedFile::Ok(ok) => Some(ok),
			_ => None,
		}).collect()
	}

	fn failed_paths(files: &[ScannedFile]) -> Vec<&str> {
		files.iter().filter_map(|f| match f {
			ScannedFile::ReadFailed { rel_path } => Some(rel_path.as_str()),
			_ => None,
		}).collect()
	}

	// ── hash_content ─────────────────────────────────────────

	#[test]
	fn hash_matches_ts_hashcontent() {
		// SHA-256("hello world") = b94d27b9934d3e08...
		assert_eq!(hash_content("hello world"), "b94d27b9934d3e08");
	}

	#[test]
	fn hash_empty_string() {
		// SHA-256("") = e3b0c44298fc1c14...
		assert_eq!(hash_content(""), "e3b0c44298fc1c14");
	}

	#[test]
	fn hash_is_16_chars() {
		assert_eq!(hash_content("any content").len(), 16);
	}

	// ── scan_repo ────────────────────────────────────────────

	#[test]
	fn scan_finds_ts_files_and_excludes_node_modules() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(root.join("src/index.ts"), "const x = 1;").unwrap();
		fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
		fs::write(root.join("node_modules/pkg/index.ts"), "const y = 2;").unwrap();
		fs::write(root.join("README.md"), "# Hello").unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		assert_eq!(ok.len(), 1);
		assert_eq!(ok[0].rel_path, "src/index.ts");
		assert_eq!(ok[0].content, "const x = 1;");
		assert_eq!(ok[0].language, Some("typescript"));
		assert!(!ok[0].is_test);
	}

	#[test]
	fn scan_respects_root_gitignore_without_git_dir() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		// No .git directory — gitignore still works (matches TS).
		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(root.join("src/keep.ts"), "const a = 1;").unwrap();
		fs::write(root.join("src/ignored.ts"), "const b = 2;").unwrap();
		fs::write(root.join(".gitignore"), "src/ignored.ts\n").unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		assert_eq!(ok.len(), 1);
		assert_eq!(ok[0].rel_path, "src/keep.ts");
	}

	#[test]
	fn scan_detects_test_files() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src")).unwrap();
		fs::create_dir_all(root.join("test")).unwrap();
		fs::write(root.join("src/app.ts"), "export {}").unwrap();
		fs::write(root.join("test/app.test.ts"), "it('works', () => {})").unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		let app = ok.iter().find(|f| f.rel_path == "src/app.ts").unwrap();
		assert!(!app.is_test);
		let test = ok.iter().find(|f| f.rel_path == "test/app.test.ts").unwrap();
		assert!(test.is_test);
	}

	#[test]
	fn scan_computes_correct_hash_and_line_count() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		let content = "line1\nline2\n";
		fs::write(root.join("file.ts"), content).unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		assert_eq!(ok.len(), 1);
		assert_eq!(ok[0].content_hash, hash_content(content));
		assert_eq!(ok[0].line_count, 3);
	}

	#[test]
	fn scan_excludes_build_and_dist_dirs() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src")).unwrap();
		fs::create_dir_all(root.join("dist")).unwrap();
		fs::create_dir_all(root.join("build")).unwrap();
		fs::write(root.join("src/app.ts"), "export {}").unwrap();
		fs::write(root.join("dist/app.js"), "var x;").unwrap();
		fs::write(root.join("build/app.js"), "var y;").unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		assert_eq!(ok.len(), 1);
		assert_eq!(ok[0].rel_path, "src/app.ts");
	}

	#[test]
	fn scan_returns_sorted_by_path() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src/b")).unwrap();
		fs::create_dir_all(root.join("src/a")).unwrap();
		fs::write(root.join("src/b/z.ts"), "").unwrap();
		fs::write(root.join("src/a/a.ts"), "").unwrap();
		fs::write(root.join("src/a/b.ts"), "").unwrap();

		let files = scan_repo(root).unwrap();
		let ok = ok_files(&files);
		let paths: Vec<&str> = ok.iter().map(|f| f.rel_path.as_str()).collect();
		assert_eq!(paths, vec!["src/a/a.ts", "src/a/b.ts", "src/b/z.ts"]);
	}

	// ── Read failure tracking ────────────────────────────────

	#[test]
	fn unreadable_file_tracked_as_read_failed() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(root.join("src/good.ts"), "ok").unwrap();

		// Create a directory with the same name as a would-be file
		// to simulate an unreadable "file" (read_to_string on a dir fails).
		// Actually, walkdir only yields files for is_file() entries.
		// Instead, create a file and make it unreadable via permissions.
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let bad_path = root.join("src/bad.ts");
			fs::write(&bad_path, "secret").unwrap();
			fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o000)).unwrap();

			let files = scan_repo(root).unwrap();
			let ok = ok_files(&files);
			let failed = failed_paths(&files);

			assert_eq!(ok.len(), 1);
			assert_eq!(ok[0].rel_path, "src/good.ts");
			assert_eq!(failed.len(), 1);
			assert_eq!(failed[0], "src/bad.ts");

			// Restore permissions for cleanup.
			fs::set_permissions(&bad_path, fs::Permissions::from_mode(0o644)).unwrap();
		}
	}
}
