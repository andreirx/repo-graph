//! Invalidation planner — determines which files need re-extraction
//! during a refresh/delta index.
//!
//! Mirror of `src/core/delta/invalidation-planner.ts`. Pure policy:
//! no I/O, no storage access. Receives parent hashes and current
//! file states, returns a plan classifying each file.
//!
//! ── Classification rules ─────────────────────────────────────
//!
//!   1. Hash comparison: unchanged / changed / new
//!   2. Deleted: in parent but not in current tree
//!   3. Config-widening: if a recognized config file changed,
//!      unchanged files in its scope are re-extracted

use std::collections::{BTreeMap, BTreeSet};

/// Current state of a file in the working tree.
#[derive(Debug, Clone)]
pub struct CurrentFileState {
	pub file_uid: String,
	pub path: String,
	pub content_hash: String,
}

/// Classification of a single file for delta indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClassification {
	pub path: String,
	pub file_uid: String,
	pub disposition: Disposition,
	pub reason: String,
	pub current_hash: Option<String>,
	pub parent_hash: Option<String>,
}

/// File disposition in the invalidation plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
	Unchanged,
	Changed,
	New,
	Deleted,
	ConfigWidened,
}

/// Counts per disposition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvalidationCounts {
	pub unchanged: u64,
	pub changed: u64,
	pub new: u64,
	pub deleted: u64,
	pub config_widened: u64,
	pub total: u64,
}

/// A config file that changed between parent and current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedConfig {
	pub path: String,
	pub scope: ConfigScope,
}

/// Scope of a changed config's invalidation effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigScope {
	/// Root-level config (no `/` in path) — widens all unchanged files.
	Global,
	/// Nested config — widens unchanged files under this directory.
	Directory(String),
}

/// The invalidation plan output.
#[derive(Debug, Clone)]
pub struct InvalidationPlan {
	pub parent_snapshot_uid: String,
	pub files: Vec<FileClassification>,
	pub changed_configs: Vec<ChangedConfig>,
	/// Paths that need extraction: changed + new + config_widened.
	pub files_to_extract: Vec<String>,
	/// Paths that can be copied forward: unchanged.
	pub files_to_copy: Vec<String>,
	/// Paths deleted since parent.
	pub files_to_delete: Vec<String>,
	pub counts: InvalidationCounts,
}

/// Recognized config filenames that trigger scope-widening when
/// changed. Mirror of the TS config list in invalidation-planner.ts.
const RECOGNIZED_CONFIGS: &[&str] = &[
	"package.json",
	"pnpm-workspace.yaml",
	"tsconfig.json",
	"Cargo.toml",
	"build.gradle",
	"settings.gradle",
	"pyproject.toml",
	"compile_commands.json",
];

/// Check if a filename (last path segment) is a recognized config.
fn is_recognized_config(path: &str) -> bool {
	let filename = path.rsplit('/').next().unwrap_or(path);
	RECOGNIZED_CONFIGS.contains(&filename)
}

/// Build the invalidation plan.
///
/// Pure function — no I/O. Takes the parent snapshot's file hashes
/// and the current working tree's file states, and classifies each
/// file by comparing hashes.
///
/// Mirror of `buildInvalidationPlan` from
/// `src/core/delta/invalidation-planner.ts`.
pub fn build_invalidation_plan(
	parent_snapshot_uid: &str,
	parent_hashes: &BTreeMap<String, String>,
	current_files: &[CurrentFileState],
	repo_uid: &str,
) -> InvalidationPlan {
	let mut files: Vec<FileClassification> = Vec::new();
	let mut changed_configs: Vec<ChangedConfig> = Vec::new();
	let mut current_file_uids: BTreeSet<String> = BTreeSet::new();

	// Phase 1: Classify current files against parent hashes.
	for cf in current_files {
		current_file_uids.insert(cf.file_uid.clone());

		let parent_hash = parent_hashes.get(&cf.file_uid);
		let (disposition, reason) = match parent_hash {
			None => (Disposition::New, "not in parent snapshot".into()),
			Some(ph) if ph == &cf.content_hash => {
				(Disposition::Unchanged, "hash match".into())
			}
			Some(_) => (Disposition::Changed, "hash changed".into()),
		};

		// Track changed configs for scope-widening.
		if disposition == Disposition::Changed && is_recognized_config(&cf.path) {
			let scope = if cf.path.contains('/') {
				let dir = cf.path.rsplit_once('/').unwrap().0;
				ConfigScope::Directory(dir.to_string())
			} else {
				ConfigScope::Global
			};
			changed_configs.push(ChangedConfig {
				path: cf.path.clone(),
				scope,
			});
		}

		files.push(FileClassification {
			path: cf.path.clone(),
			file_uid: cf.file_uid.clone(),
			disposition,
			reason,
			current_hash: Some(cf.content_hash.clone()),
			parent_hash: parent_hash.cloned(),
		});
	}

	// Phase 2: Detect deleted files.
	for (file_uid, hash) in parent_hashes {
		if !current_file_uids.contains(file_uid) {
			let path = file_uid
				.strip_prefix(&format!("{}:", repo_uid))
				.unwrap_or(file_uid)
				.to_string();
			files.push(FileClassification {
				path,
				file_uid: file_uid.clone(),
				disposition: Disposition::Deleted,
				reason: "not in current tree".into(),
				current_hash: None,
				parent_hash: Some(hash.clone()),
			});
		}
	}

	// Phase 3: Config-widening. Unchanged files in scope of a
	// changed config are reclassified as config_widened.
	if !changed_configs.is_empty() {
		for f in &mut files {
			if f.disposition != Disposition::Unchanged {
				continue;
			}
			for cc in &changed_configs {
				let in_scope = match &cc.scope {
					ConfigScope::Global => true,
					ConfigScope::Directory(dir) => f.path.starts_with(&format!("{}/", dir)),
				};
				if in_scope {
					f.disposition = Disposition::ConfigWidened;
					f.reason = format!("config changed: {}", cc.path);
					break;
				}
			}
		}
	}

	// Build output lists and counts.
	let mut files_to_extract = Vec::new();
	let mut files_to_copy = Vec::new();
	let mut files_to_delete = Vec::new();
	let mut counts = InvalidationCounts::default();

	for f in &files {
		match f.disposition {
			Disposition::Unchanged => {
				files_to_copy.push(f.path.clone());
				counts.unchanged += 1;
			}
			Disposition::Changed => {
				files_to_extract.push(f.path.clone());
				counts.changed += 1;
			}
			Disposition::New => {
				files_to_extract.push(f.path.clone());
				counts.new += 1;
			}
			Disposition::Deleted => {
				files_to_delete.push(f.path.clone());
				counts.deleted += 1;
			}
			Disposition::ConfigWidened => {
				files_to_extract.push(f.path.clone());
				counts.config_widened += 1;
			}
		}
	}
	counts.total = files.len() as u64;

	InvalidationPlan {
		parent_snapshot_uid: parent_snapshot_uid.to_string(),
		files,
		changed_configs,
		files_to_extract,
		files_to_copy,
		files_to_delete,
		counts,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_current(path: &str, hash: &str, repo_uid: &str) -> CurrentFileState {
		CurrentFileState {
			file_uid: format!("{}:{}", repo_uid, path),
			path: path.into(),
			content_hash: hash.into(),
		}
	}

	fn make_parent_hashes(entries: &[(&str, &str)], repo_uid: &str) -> BTreeMap<String, String> {
		entries
			.iter()
			.map(|(path, hash)| (format!("{}:{}", repo_uid, path), hash.to_string()))
			.collect()
	}

	#[test]
	fn unchanged_files_are_classified_correctly() {
		let parent = make_parent_hashes(&[("src/a.ts", "hash1")], "r1");
		let current = vec![make_current("src/a.ts", "hash1", "r1")];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.unchanged, 1);
		assert_eq!(plan.files_to_copy, vec!["src/a.ts"]);
		assert!(plan.files_to_extract.is_empty());
	}

	#[test]
	fn changed_files_need_extraction() {
		let parent = make_parent_hashes(&[("src/a.ts", "old_hash")], "r1");
		let current = vec![make_current("src/a.ts", "new_hash", "r1")];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.changed, 1);
		assert_eq!(plan.files_to_extract, vec!["src/a.ts"]);
	}

	#[test]
	fn new_files_need_extraction() {
		let parent = BTreeMap::new();
		let current = vec![make_current("src/new.ts", "h1", "r1")];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.new, 1);
		assert_eq!(plan.files_to_extract, vec!["src/new.ts"]);
	}

	#[test]
	fn deleted_files_are_detected() {
		let parent = make_parent_hashes(&[("src/old.ts", "h1")], "r1");
		let current = vec![];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.deleted, 1);
		assert_eq!(plan.files_to_delete, vec!["src/old.ts"]);
	}

	#[test]
	fn root_config_change_widens_all_unchanged() {
		let parent = make_parent_hashes(
			&[("package.json", "old"), ("src/a.ts", "h1"), ("src/b.ts", "h2")],
			"r1",
		);
		let current = vec![
			make_current("package.json", "new", "r1"),
			make_current("src/a.ts", "h1", "r1"),
			make_current("src/b.ts", "h2", "r1"),
		];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.changed, 1); // package.json
		assert_eq!(plan.counts.config_widened, 2); // a.ts, b.ts
		assert_eq!(plan.counts.unchanged, 0);
		assert_eq!(plan.files_to_extract.len(), 3);
		assert!(plan.files_to_copy.is_empty());
	}

	#[test]
	fn nested_config_widens_only_subdirectory() {
		let parent = make_parent_hashes(
			&[
				("src/sub/tsconfig.json", "old"),
				("src/sub/a.ts", "h1"),
				("src/other/b.ts", "h2"),
			],
			"r1",
		);
		let current = vec![
			make_current("src/sub/tsconfig.json", "new", "r1"),
			make_current("src/sub/a.ts", "h1", "r1"),
			make_current("src/other/b.ts", "h2", "r1"),
		];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.changed, 1); // tsconfig.json
		assert_eq!(plan.counts.config_widened, 1); // src/sub/a.ts
		assert_eq!(plan.counts.unchanged, 1); // src/other/b.ts (out of scope)
	}

	#[test]
	fn mixed_scenario() {
		let parent = make_parent_hashes(
			&[("src/a.ts", "h1"), ("src/b.ts", "h2"), ("src/del.ts", "h3")],
			"r1",
		);
		let current = vec![
			make_current("src/a.ts", "h1", "r1"),  // unchanged
			make_current("src/b.ts", "h2_new", "r1"), // changed
			make_current("src/new.ts", "h4", "r1"),  // new
		];
		let plan = build_invalidation_plan("snap1", &parent, &current, "r1");
		assert_eq!(plan.counts.unchanged, 1);
		assert_eq!(plan.counts.changed, 1);
		assert_eq!(plan.counts.new, 1);
		assert_eq!(plan.counts.deleted, 1);
		assert_eq!(plan.counts.total, 4);
		assert_eq!(plan.files_to_copy, vec!["src/a.ts"]);
		assert!(plan.files_to_extract.contains(&"src/b.ts".to_string()));
		assert!(plan.files_to_extract.contains(&"src/new.ts".to_string()));
		assert_eq!(plan.files_to_delete, vec!["src/del.ts"]);
	}
}
